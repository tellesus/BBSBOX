// SPDX-License-Identifier: AGPL-3.0-or-later
//! One-time launch tickets and short-lived, game-scoped browser sessions.
use super::Manifest;
use bbsbox_credentials::NodeId;
use bbsbox_identity::LocalAccountId;
use bbsbox_storage::clock::Clock;
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Arc};
use zeroize::Zeroizing;

const TICKET_AGE: i64 = 2 * 60 * 1000;
const SESSION_AGE: i64 = 15 * 60 * 1000;
const MAX_GRANTS: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerId([u8; 16]);

impl PlayerId {
    pub fn for_account(node: NodeId, game: &Manifest, account: LocalAccountId) -> Self {
        let mut hash = Sha256::new();
        hash.update(b"BBSBOX local game player v1\0");
        hash.update(node.bytes());
        hash.update([game.game_id().len() as u8]);
        hash.update(game.game_id().as_bytes());
        hash.update(account.bytes());
        let digest = hash.finalize();
        Self(digest[..16].try_into().expect("fixed digest"))
    }

    pub fn bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

pub struct Secret(Zeroizing<String>);

impl Secret {
    pub fn expose(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Denied,
    Full,
    InvalidManifest,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Ticket,
    Session,
}

struct Grant {
    game: String,
    player: PlayerId,
    expires: i64,
    kind: Kind,
}

pub struct Registry {
    clock: Arc<dyn Clock>,
    grants: BTreeMap<[u8; 32], Grant>,
}

impl Registry {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            clock,
            grants: BTreeMap::new(),
        }
    }

    /// The core supplies a currently authenticated local account. The game only receives PlayerId.
    pub fn launch(
        &mut self,
        node: NodeId,
        game: &Manifest,
        account: LocalAccountId,
    ) -> Result<Secret, Error> {
        if !game.valid() {
            return Err(Error::InvalidManifest);
        }
        let now = self.clock.monotonic_millis();
        self.grants.retain(|_, grant| grant.expires > now);
        if self.grants.len() >= MAX_GRANTS {
            return Err(Error::Full);
        }
        Ok(self.issue(Grant {
            game: game.game_id().into(),
            player: PlayerId::for_account(node, game, account),
            expires: now.saturating_add(TICKET_AGE),
            kind: Kind::Ticket,
        }))
    }

    pub fn exchange(&mut self, game: &Manifest, ticket: &str) -> Result<Secret, Error> {
        let key = token_key(ticket)?;
        self.lookup(game, key, Kind::Ticket)?;
        let mut grant = self.grants.remove(&key).ok_or(Error::Denied)?;
        grant.kind = Kind::Session;
        grant.expires = self.clock.monotonic_millis().saturating_add(SESSION_AGE);
        Ok(self.issue(grant))
    }

    pub fn authorize(&self, game: &Manifest, bearer: &str) -> Result<PlayerId, Error> {
        let key = token_key(bearer)?;
        Ok(self.lookup(game, key, Kind::Session)?.player)
    }

    pub fn revoke(&mut self, game: &Manifest, bearer: &str) -> Result<(), Error> {
        let key = token_key(bearer)?;
        self.lookup(game, key, Kind::Session)?;
        self.grants.remove(&key);
        Ok(())
    }

    fn lookup(&self, game: &Manifest, key: [u8; 32], kind: Kind) -> Result<&Grant, Error> {
        self.grants
            .get(&key)
            .filter(|grant| {
                grant.game == game.game_id()
                    && grant.kind == kind
                    && grant.expires > self.clock.monotonic_millis()
            })
            .ok_or(Error::Denied)
    }

    fn issue(&mut self, grant: Grant) -> Secret {
        let mut bytes = Zeroizing::new([0; 32]);
        loop {
            OsRng.fill_bytes(bytes.as_mut());
            let key: [u8; 32] = Sha256::digest(bytes.as_slice()).into();
            if let std::collections::btree_map::Entry::Vacant(entry) = self.grants.entry(key) {
                entry.insert(grant);
                return Secret(Zeroizing::new(hex::encode(bytes.as_slice())));
            }
        }
    }
}

fn token_key(value: &str) -> Result<[u8; 32], Error> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Error::Denied);
    }
    let mut bytes = Zeroizing::new([0; 32]);
    hex::decode_to_slice(value, bytes.as_mut()).map_err(|_| Error::Denied)?;
    Ok(Sha256::digest(bytes.as_slice()).into())
}
