// SPDX-License-Identifier: AGPL-3.0-or-later
//! Local client release selection. The pinned bootstrap/store are independent of a hosting node.
use crate::{
    client_payload::{self, Result},
    client_release::{self, SignedClient},
    release::KeyId,
};
use bbsbox_storage::{
    self as storage, Migration,
    disk::Class,
    writer::{Priority, Writer},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

const SCHEMA:&str="
CREATE TABLE client_control(id INTEGER PRIMARY KEY CHECK(id=1),revision INTEGER NOT NULL CHECK(revision>=0),body BLOB NOT NULL CHECK(length(body)<=65536)) STRICT;
CREATE TABLE client_audit(revision INTEGER PRIMARY KEY,sha256 BLOB NOT NULL CHECK(length(sha256)=32)) STRICT;
CREATE TABLE client_manifests(id BLOB PRIMARY KEY CHECK(length(id)=32),authority BLOB NOT NULL CHECK(length(authority)=32),manifest BLOB NOT NULL CHECK(length(manifest)<=65664)) STRICT;
CREATE TABLE client_install_requests(request BLOB PRIMARY KEY CHECK(length(request)=16),release BLOB NOT NULL CHECK(length(release)=32),completed INTEGER NOT NULL CHECK(completed IN(0,1))) STRICT;
";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub id: [u8; 32],
    pub authority: [u8; 32],
    pub sequence: u64,
}
impl Selection {
    fn of(signed: &SignedClient) -> Self {
        Self {
            id: *signed.id(),
            authority: *signed.signer(),
            sequence: signed.manifest().sequence,
        }
    }
    fn valid(&self) -> bool {
        self.id != [0; 32]
            && self.sequence > 0
            && self.sequence <= i64::MAX as u64
            && KeyId::from_public(&self.authority).is_ok()
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pending {
    pub request: [u8; 16],
    pub candidate: Selection,
    pub next_key: Option<[u8; 32]>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub format: String,
    pub revision: u64,
    pub trusted_key: [u8; 32],
    pub highest_sequence: u64,
    pub active: Option<Selection>,
    pub pending: Option<Pending>,
}
impl State {
    fn valid(&self) -> bool {
        self.format == "bbsbox.client-install/v1"
            && self.revision <= i64::MAX as u64
            && self.highest_sequence <= i64::MAX as u64
            && KeyId::from_public(&self.trusted_key).is_ok()
            && self
                .active
                .as_ref()
                .is_none_or(|s| s.valid() && s.sequence <= self.highest_sequence)
            && self.pending.as_ref().is_none_or(|p| {
                p.request != [0; 16]
                    && p.candidate.valid()
                    && p.candidate.sequence == self.highest_sequence
                    && p.candidate.authority == self.trusted_key
                    && p.next_key.is_none_or(|key| {
                        key != self.trusted_key && KeyId::from_public(&key).is_ok()
                    })
            })
    }
}
fn read(connection: &Connection) -> Result<State> {
    let (revision, body): (i64, Vec<u8>) = connection.query_row(
        "SELECT revision,body FROM client_control WHERE id=1 AND length(body)<=65536",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let state: State = serde_json::from_slice(&body)?;
    let (head, hash): (i64, Vec<u8>) = connection.query_row(
        "SELECT revision,sha256 FROM client_audit ORDER BY revision DESC LIMIT 1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if revision < 0
        || state.revision != revision as u64
        || head != revision
        || !state.valid()
        || serde_json::to_vec(&state)? != body
        || hash.as_slice() != Sha256::digest(&body).as_slice()
    {
        return Err("client release control is corrupt or unsupported".into());
    }
    Ok(state)
}
pub struct Store {
    root: PathBuf,
    writer: Writer,
}
fn writer(root: &Path) -> Result<Writer> {
    let path = root.join("trust.sqlite");
    if let Ok(metadata) = fs::symlink_metadata(&path)
        && (!metadata.is_file() || client_payload::linked(&metadata))
    {
        return Err("client trust needs a regular storage file".into());
    }
    Ok(Writer::start(
        &path,
        vec![Migration {
            module: "client-release-control",
            version: 1,
            sql: SCHEMA,
        }],
        storage::writer::Config {
            capacity: 4,
            ..Default::default()
        },
    )?)
}
impl Store {
    /// Explicit trust enrollment. The expected release key must arrive independently of the node.
    pub fn initialize(root: &Path, key: [u8; 32]) -> Result<Self> {
        KeyId::from_public(&key)?;
        if !root.try_exists()? {
            client_payload::directory(root.parent().ok_or("client store needs a parent")?)?;
            fs::create_dir(root)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(root, fs::Permissions::from_mode(0o700))?;
            }
        }
        let root = client_payload::directory(root)?;
        let writer = writer(&root)?;
        let releases = root.join("releases");
        if !releases.try_exists()? {
            fs::create_dir(&releases)?;
        }
        client_payload::directory(&releases)?;
        let state = State {
            format: "bbsbox.client-install/v1".into(),
            revision: 0,
            trusted_key: key,
            highest_sequence: 0,
            active: None,
            pending: None,
        };
        let body = serde_json::to_vec(&state)?;
        writer
            .handle()
            .submit_service(Priority::Interactive, move |db| -> Result<()> {
                db.write(Class::Control, 65536, |tx| {
                    if tx.execute(
                        "INSERT OR IGNORE INTO client_control VALUES(1,0,?1)",
                        [&body],
                    )? != 1
                    {
                        return Err("client release trust already exists".into());
                    }
                    tx.execute(
                        "INSERT INTO client_audit VALUES(0,?1)",
                        [Sha256::digest(&body).as_slice()],
                    )?;
                    Ok(())
                })
            })?
            .blocking_wait()?;
        Ok(Self { root, writer })
    }
    pub fn open(root: &Path) -> Result<Self> {
        let root = client_payload::directory(root)?;
        if !root.join("trust.sqlite").try_exists()? {
            return Err("client release trust has not been initialized".into());
        }
        let store = Self {
            writer: writer(&root)?,
            root,
        };
        store.state()?;
        Ok(store)
    }
    pub fn state(&self) -> Result<State> {
        self.writer
            .handle()
            .submit_service(Priority::Interactive, |db| -> Result<State> {
                db.write(Class::Control, 0, |tx| read(tx))
            })?
            .blocking_wait()
    }
    fn releases(&self) -> Result<PathBuf> {
        client_payload::directory(&self.root.join("releases"))
    }
    fn selected(&self, selection: &Selection) -> Result<SignedClient> {
        let root = self.releases()?.join(hex::encode(selection.id));
        let signed = client_payload::verify(&root, &selection.authority)?;
        if Selection::of(&signed) != *selection {
            return Err("installed client differs from its selection".into());
        }
        Ok(signed)
    }
    fn change<F>(&self, expected: u64, update: F) -> Result<State>
    where
        F: FnOnce(&rusqlite::Transaction<'_>, &mut State) -> Result<()> + Send + 'static,
    {
        self.writer
            .handle()
            .submit_service(Priority::Interactive, move |db| -> Result<State> {
                db.write(Class::Control, 131072, |tx| {
                    let mut state = read(tx)?;
                    if state.revision != expected {
                        return Err("client release revision changed".into());
                    }
                    update(tx, &mut state)?;
                    state.revision = state
                        .revision
                        .checked_add(1)
                        .filter(|r| *r <= i64::MAX as u64)
                        .ok_or("client release revision exhausted")?;
                    if !state.valid() {
                        return Err("invalid client release transition".into());
                    }
                    let body = serde_json::to_vec(&state)?;
                    tx.execute(
                        "UPDATE client_control SET revision=?1,body=?2 WHERE id=1",
                        params![state.revision as i64, &body],
                    )?;
                    tx.execute(
                        "INSERT INTO client_audit VALUES(?1,?2)",
                        params![state.revision as i64, Sha256::digest(&body).as_slice()],
                    )?;
                    Ok(state)
                })
            })?
            .blocking_wait()
    }
    pub fn prepare(&self, expected: u64, request: [u8; 16], package: &Path) -> Result<State> {
        if request == [0; 16] {
            return Err("client installation needs a request ID".into());
        }
        let state = self.state()?;
        if state.revision != expected {
            return Err("client release revision changed".into());
        }
        let known=self.writer.handle().submit_service(Priority::Interactive,move|db|->Result<Option<(Vec<u8>,Vec<u8>)>>{
            db.write(Class::Control,0,|tx|Ok(tx.query_row("SELECT r.release,m.authority FROM client_install_requests r JOIN client_manifests m ON m.id=r.release WHERE r.request=?1",[request.as_slice()],|r|Ok((r.get(0)?,r.get(1)?))).optional()?))
        })?.blocking_wait()?;
        if let Some((id, authority)) = known {
            let authority: [u8; 32] = authority
                .try_into()
                .map_err(|_| "invalid client authority record")?;
            let signed = client_payload::verify(package, &authority)?;
            if signed.id().as_slice() != id {
                return Err("client installation request reused for another release".into());
            }
            return Ok(state);
        }
        if state.pending.is_some() {
            return Err("a client installation is pending; resume it first".into());
        }
        let signed = client_payload::verify(package, &state.trusted_key)?;
        if signed.manifest().sequence <= state.highest_sequence {
            return Err("client release is not newer than the retained sequence".into());
        }
        let releases = self.releases()?;
        let destination = releases.join(hex::encode(signed.id()));
        if destination.try_exists()? {
            if client_payload::verify(&destination, signed.signer())?.id() != signed.id() {
                return Err("existing client stage differs".into());
            }
        } else {
            let temporary = tempfile::Builder::new()
                .prefix(".client-stage-")
                .tempdir_in(&releases)?;
            let payload = temporary.path().join("payload");
            if let Err(error) = client_payload::stage(package, &payload, signed.signer()) {
                let _retained = temporary.keep();
                return Err(error);
            }
            // Both move endpoints are fixed descendants of this validated, locked store.
            if !payload.starts_with(&releases)
                || !destination.starts_with(&releases)
                || destination.try_exists()?
            {
                return Err("client staging destination changed".into());
            }
            fs::rename(&payload, &destination)?;
        }
        let selection = Selection::of(&signed);
        let bytes = signed.encoded().to_vec();
        let next_key = signed.manifest().next_key;
        self.change(expected, move |tx, state| {
            if state.pending.is_some()
                || selection.authority != state.trusted_key
                || selection.sequence <= state.highest_sequence
            {
                return Err("client installation admission changed".into());
            }
            tx.execute(
                "INSERT OR IGNORE INTO client_manifests VALUES(?1,?2,?3)",
                params![
                    selection.id.as_slice(),
                    selection.authority.as_slice(),
                    bytes
                ],
            )?;
            tx.execute(
                "INSERT INTO client_install_requests VALUES(?1,?2,0)",
                params![request.as_slice(), selection.id.as_slice()],
            )?;
            state.highest_sequence = selection.sequence;
            state.pending = Some(Pending {
                request,
                candidate: selection,
                next_key,
            });
            Ok(())
        })
    }
    pub fn activate(&self, expected: u64, request: [u8; 16]) -> Result<State> {
        let state = self.state()?;
        if state.revision != expected {
            return Err("client release revision changed".into());
        }
        let pending = state
            .pending
            .as_ref()
            .filter(|p| p.request == request)
            .ok_or("no matching client installation is pending")?;
        let signed = self.selected(&pending.candidate)?;
        if signed.manifest().next_key != pending.next_key {
            return Err("client key rotation differs from its manifest".into());
        }
        self.change(expected,move|tx,state|{
            let pending=state.pending.take().filter(|p|p.request==request).ok_or("client installation changed")?;
            if tx.execute("UPDATE client_install_requests SET completed=1 WHERE request=?1 AND release=?2 AND completed=0",params![request.as_slice(),pending.candidate.id.as_slice()])?!=1{
                return Err("client installation receipt is missing".into());
            }
            state.active=Some(pending.candidate);
            if let Some(next)=pending.next_key{state.trusted_key=next;}
            Ok(())
        })
    }
    /// Recheck complete installed bytes immediately before a trusted caller starts the executable.
    /// This does not defend against an attacker with local write access to the pinned store itself.
    pub fn executable(&self) -> Result<PathBuf> {
        let state = self.state()?;
        if state.pending.is_some() {
            return Err("finish the pending client installation before launch".into());
        }
        let active = state
            .active
            .as_ref()
            .ok_or("no client release is installed")?;
        self.selected(active)?;
        Ok(self
            .releases()?
            .join(hex::encode(active.id))
            .join(client_release::ENTRY))
    }
}
