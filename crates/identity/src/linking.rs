// SPDX-License-Identifier: AGPL-3.0-or-later
//! Destination-local account binding requires both its current session and root key possession.
use crate::{Auth, Error, LocalAccountId, Result, session::Mutation};
use bbsbox_credentials::{
    ContactCard, NodeId, PersonId,
    linking::{Challenge, verify},
};
use bbsbox_storage::clock::Clock;
use rand_core::{OsRng, RngCore};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use std::sync::Arc;
pub(crate) const SCHEMA:&str="CREATE TABLE identity_account_links(account_id BLOB NOT NULL PRIMARY KEY REFERENCES identity_accounts(id),person_root BLOB NOT NULL CHECK(length(person_root)=32));
CREATE TABLE identity_link_challenges(nonce BLOB NOT NULL PRIMARY KEY CHECK(length(nonce)=32),account_id BLOB NOT NULL REFERENCES identity_accounts(id),node_id BLOB NOT NULL CHECK(length(node_id)=32),person_root BLOB NOT NULL CHECK(length(person_root)=32),boot BLOB NOT NULL CHECK(length(boot)=16),created_tick INTEGER NOT NULL);
CREATE INDEX identity_link_pending ON identity_link_challenges(account_id);
CREATE TABLE identity_link_receipts(nonce BLOB NOT NULL PRIMARY KEY CHECK(length(nonce)=32),account_id BLOB NOT NULL REFERENCES identity_accounts(id),node_id BLOB NOT NULL CHECK(length(node_id)=32),person_root BLOB NOT NULL CHECK(length(person_root)=32),proof BLOB NOT NULL CHECK(length(proof)<=512));";
const LIFETIME: i64 = 5 * 60 * 1000;
pub struct Linker {
    node: NodeId,
    boot: [u8; 16],
    clock: Arc<dyn Clock>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Linked {
    pub account: LocalAccountId,
    pub person: PersonId,
    pub replayed: bool,
}
type PendingRow = (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, i64);
type ReceiptRow = (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>);
fn card(root: Vec<u8>) -> Result<ContactCard> {
    Ok(ContactCard::new(
        root.try_into().map_err(|_| Error::InvalidChallenge)?,
    )?)
}
pub fn linked_person(reader: &Connection, account: LocalAccountId) -> Result<Option<PersonId>> {
    let root: Option<Vec<u8>> = reader
        .query_row(
            "SELECT person_root FROM identity_account_links WHERE account_id=?1",
            [account.bytes().as_slice()],
            |r| r.get(0),
        )
        .optional()?;
    root.map(|root| card(root).map(|card| card.person_id()))
        .transpose()
}
impl Linker {
    /// One instance per destination node process; use the node's configured public identity.
    pub fn new(node: NodeId, clock: Arc<dyn Clock>) -> Self {
        let mut boot = [0; 16];
        OsRng.fill_bytes(&mut boot);
        Self { node, boot, clock }
    }
    pub fn begin(
        &self,
        tx: &Transaction<'_>,
        auth: &Auth,
        actor: &Mutation,
        card: &ContactCard,
    ) -> Result<Challenge> {
        let account = auth.mutation_account(tx, actor)?;
        let now = self.clock.monotonic_millis();
        if now < 0 {
            return Err(Error::InvalidConfiguration);
        }
        if linked_person(tx, account.id)?.is_some_and(|id| id != card.person_id()) {
            return Err(Error::AlreadyLinked);
        }
        tx.execute("DELETE FROM identity_link_challenges WHERE boot<>?1 OR created_tick<=?2 OR created_tick>?3",params![self.boot.as_slice(),now.saturating_sub(LIFETIME),now])?;
        let count: i64 = tx.query_row(
            "SELECT count(*) FROM identity_link_challenges WHERE account_id=?1",
            [account.id.bytes().as_slice()],
            |r| r.get(0),
        )?;
        if count >= 8 {
            return Err(Error::RateLimited);
        }
        let mut nonce = [0; 32];
        while nonce == [0; 32] {
            OsRng.fill_bytes(&mut nonce);
        }
        let challenge = Challenge::new(self.node, *account.id.bytes(), card.person_id(), nonce)?;
        tx.execute(
            "INSERT INTO identity_link_challenges VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                nonce.as_slice(),
                account.id.bytes().as_slice(),
                self.node.bytes().as_slice(),
                card.root().as_slice(),
                self.boot.as_slice(),
                now
            ],
        )?;
        Ok(challenge)
    }
    pub fn finish(
        &self,
        tx: &Transaction<'_>,
        auth: &Auth,
        actor: &Mutation,
        nonce: &[u8; 32],
        proof: &[u8],
    ) -> Result<Linked> {
        let account = auth.mutation_account(tx, actor)?;
        let done:Option<ReceiptRow>=tx.query_row("SELECT account_id,node_id,person_root,proof FROM identity_link_receipts WHERE nonce=?1",[nonce.as_slice()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
        if let Some((owner, node, root, original)) = done {
            let person = card(root)?.person_id();
            if owner != account.id.bytes()
                || node != self.node.bytes()
                || original != proof
                || linked_person(tx, account.id)? != Some(person)
            {
                return Err(Error::InvalidChallenge);
            }
            return Ok(Linked {
                account: account.id,
                person,
                replayed: true,
            });
        }
        let pending:Option<PendingRow>=tx.query_row("SELECT account_id,node_id,person_root,boot,created_tick FROM identity_link_challenges WHERE nonce=?1",[nonce.as_slice()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
        let (owner, node, root, boot, created) = pending.ok_or(Error::InvalidChallenge)?;
        let now = self.clock.monotonic_millis();
        if owner != account.id.bytes()
            || node != self.node.bytes()
            || boot != self.boot
            || now < created
            || now - created >= LIFETIME
        {
            return Err(Error::InvalidChallenge);
        }
        let card = card(root)?;
        let challenge = Challenge::new(self.node, *account.id.bytes(), card.person_id(), *nonce)?;
        verify(&challenge, &card, proof)?;
        if linked_person(tx, account.id)?.is_some_and(|id| id != card.person_id()) {
            return Err(Error::AlreadyLinked);
        }
        tx.execute(
            "INSERT INTO identity_account_links VALUES(?1,?2) ON CONFLICT(account_id) DO NOTHING",
            params![account.id.bytes().as_slice(), card.root().as_slice()],
        )?;
        tx.execute(
            "INSERT INTO identity_link_receipts VALUES(?1,?2,?3,?4,?5)",
            params![
                nonce.as_slice(),
                account.id.bytes().as_slice(),
                self.node.bytes().as_slice(),
                card.root().as_slice(),
                proof
            ],
        )?;
        tx.execute(
            "DELETE FROM identity_link_challenges WHERE nonce=?1",
            [nonce.as_slice()],
        )?;
        Ok(Linked {
            account: account.id,
            person: card.person_id(),
            replayed: false,
        })
    }
    /// Explicit local unlink preserves accepted signed proofs/history and cancels pending requests.
    pub fn unlink(&self, tx: &Transaction<'_>, auth: &Auth, actor: &Mutation) -> Result<bool> {
        let account = auth.mutation_account(tx, actor)?;
        tx.execute(
            "DELETE FROM identity_link_challenges WHERE account_id=?1",
            [account.id.bytes().as_slice()],
        )?;
        Ok(tx.execute(
            "DELETE FROM identity_account_links WHERE account_id=?1",
            [account.id.bytes().as_slice()],
        )? == 1)
    }
}
