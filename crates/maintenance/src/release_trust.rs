// SPDX-License-Identifier: AGPL-3.0-or-later
//! Operator release control, separate from node snapshots. No application/data activation here.
use crate::{
    archive,
    release::{self, KeyId, SignedRelease},
};
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
const FORMAT: &str = "bbsbox.release-trust/v1";
const APPLICATION: u32 = 0x42525431;
const MAX_STATE: usize = 16384;
const SCHEMA: &str = "
CREATE TABLE control(id INTEGER PRIMARY KEY CHECK(id=1), revision INTEGER NOT NULL CHECK(revision>=0), body BLOB NOT NULL) STRICT;
CREATE TABLE history(revision INTEGER PRIMARY KEY, action TEXT NOT NULL, detail BLOB NOT NULL, state_sha256 BLOB NOT NULL CHECK(length(state_sha256)=32)) STRICT;
CREATE TABLE releases(id BLOB PRIMARY KEY CHECK(length(id)=32), authority BLOB NOT NULL CHECK(length(authority)=32), manifest BLOB NOT NULL, completed INTEGER NOT NULL CHECK(completed IN(0,1))) STRICT;
";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub release_id: [u8; 32],
    pub sequence: u64,
    pub authority: [u8; 32],
    pub version: String,
}
impl Selection {
    fn from_signed(signed: &SignedRelease) -> Self {
        Self {
            release_id: *signed.id(),
            sequence: signed.manifest().sequence,
            authority: *signed.signer(),
            version: signed.manifest().version.clone(),
        }
    }
    fn valid(&self) -> bool {
        self.release_id != [0; 32]
            && self.sequence > 0
            && KeyId::from_public(&self.authority).is_ok()
            && !self.version.is_empty()
            && self.version.len() <= 64
            && self
                .version
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-+".contains(&b))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recovery {
    pub snapshot_inventory: [u8; 32],
    pub reason: String,
    pub recorded_at: i64,
}
impl Recovery {
    fn valid(&self) -> bool {
        self.snapshot_inventory != [0; 32]
            && !self.reason.trim().is_empty()
            && self.reason.len() <= 512
            && !self.reason.chars().any(char::is_control)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Purpose {
    Install,
    Recovery { evidence: Recovery },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pending {
    pub request: [u8; 16],
    pub candidate: Selection,
    pub next_key: Option<[u8; 32]>,
    pub purpose: Purpose,
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
        if self.format != FORMAT
            || self.revision > i64::MAX as u64
            || KeyId::from_public(&self.trusted_key).is_err()
            || self
                .active
                .as_ref()
                .is_some_and(|s| !s.valid() || s.sequence > self.highest_sequence)
        {
            return false;
        }
        if let Some(pending) = &self.pending {
            if pending.request == [0; 16]
                || !pending.candidate.valid()
                || pending.candidate.sequence > self.highest_sequence
                || pending
                    .next_key
                    .as_ref()
                    .is_some_and(|key| KeyId::from_public(key).is_err() || *key == self.trusted_key)
            {
                return false;
            }
            match &pending.purpose {
                Purpose::Install => {
                    if pending.candidate.sequence != self.highest_sequence
                        || pending.candidate.authority != self.trusted_key
                    {
                        return false;
                    }
                }
                Purpose::Recovery { evidence } => {
                    if !evidence.valid() || pending.next_key.is_some() {
                        return false;
                    }
                }
            }
        }
        true
    }
}
#[derive(Debug)]
pub struct Audit {
    pub revision: u64,
    pub action: String,
    pub detail: serde_json::Value,
    pub state_sha256: [u8; 32],
}
pub struct Store {
    root: PathBuf,
}

fn directory(path: &Path) -> Result<PathBuf> {
    let path = std::path::absolute(path)?;
    for parent in path.ancestors() {
        let metadata = fs::symlink_metadata(parent)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err("release control requires unlinked directory ancestors".into());
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(&path)?.permissions().mode() & 0o077 != 0 {
            return Err("release control directory must be private".into());
        }
    }
    Ok(path.canonicalize()?)
}
fn read_state(connection: &Connection) -> Result<State> {
    let (revision, bytes): (i64, Vec<u8>) = connection.query_row(
        "SELECT revision,body FROM control WHERE id=1 AND length(body)<=?1",
        [MAX_STATE as i64],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if bytes.len() > MAX_STATE {
        return Err("release control state exceeds its bound".into());
    }
    let state: State = serde_json::from_slice(&bytes)?;
    if !state.valid()
        || revision < 0
        || state.revision != revision as u64
        || serde_json::to_vec(&state)? != bytes
    {
        return Err("release control state is corrupt or unsupported".into());
    }
    let (latest, sha): (i64, Vec<u8>) = connection.query_row(
        "SELECT revision,state_sha256 FROM history ORDER BY revision DESC LIMIT 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if latest != revision || sha.as_slice() != Sha256::digest(&bytes).as_slice() {
        return Err("release control state and audit head disagree".into());
    }
    Ok(state)
}
impl Store {
    /// Bootstrap only into a fresh directory. Never infer/reset trust from a node backup.
    pub fn initialize(root: &Path, key: [u8; 32]) -> Result<Self> {
        KeyId::from_public(&key)?;
        let parent = root
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        // Validate ancestors without requiring the operator's parent itself to be 0700.
        for ancestor in std::path::absolute(parent)?.ancestors() {
            let metadata = fs::symlink_metadata(ancestor)?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err("release control parent is linked or invalid".into());
            }
        }
        archive::private_directory(root)?;
        let store = Self {
            root: directory(root)?,
        };
        let path = store.root.join("release-trust.sqlite");
        archive::private_file(&path)?.sync_all()?;
        let mut connection = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        connection.pragma_update(None, "journal_mode", "DELETE")?;
        connection.pragma_update(None, "synchronous", "EXTRA")?;
        let state = State {
            format: FORMAT.into(),
            revision: 0,
            trusted_key: key,
            highest_sequence: 0,
            active: None,
            pending: None,
        };
        let body = serde_json::to_vec(&state)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.pragma_update(None, "application_id", APPLICATION)?;
        transaction.pragma_update(None, "user_version", 1)?;
        transaction.execute_batch(SCHEMA)?;
        transaction.execute("INSERT INTO control VALUES(1,0,?1)", [&body])?;
        transaction.execute(
            "INSERT INTO history VALUES(0,'bootstrap',?1,?2)",
            params![
                serde_json::to_vec(&serde_json::json!({"trusted_key":key}))?,
                Sha256::digest(&body).as_slice()
            ],
        )?;
        transaction.commit()?;
        drop(connection);
        archive::sync_directory(&store.root)?;
        archive::sync_directory(parent)?;
        store.state()?;
        Ok(store)
    }
    pub fn open(root: &Path) -> Result<Self> {
        let store = Self {
            root: directory(root)?,
        };
        store.state()?;
        Ok(store)
    }
    fn connection(&self) -> Result<Connection> {
        directory(&self.root)?;
        let path = self.root.join("release-trust.sqlite");
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err("release control database is missing or linked".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err("release control database must be private".into());
            }
        }
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        connection.busy_timeout(Duration::from_millis(250))?;
        let application: u32 =
            connection.pragma_query_value(None, "application_id", |r| r.get(0))?;
        let version: u32 = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
        let mode: String = connection.pragma_query_value(None, "journal_mode", |r| r.get(0))?;
        if application != APPLICATION || version != 1 || !mode.eq_ignore_ascii_case("delete") {
            return Err("foreign or unsupported release control database".into());
        }
        connection.pragma_update(None, "synchronous", "EXTRA")?;
        let synchronization: u32 =
            connection.pragma_query_value(None, "synchronous", |r| r.get(0))?;
        if synchronization != 3 {
            return Err("release control requires SQLite EXTRA synchronization".into());
        }
        Ok(connection)
    }
    pub fn state(&self) -> Result<State> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let state = read_state(&transaction)?;
        transaction.commit()?;
        Ok(state)
    }
    fn change(
        &self,
        expected: u64,
        action: &str,
        apply: impl FnOnce(&Connection, &mut State) -> Result<serde_json::Value>,
    ) -> Result<State> {
        archive::space(&self.root, 1024 * 1024)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut state = read_state(&transaction)?;
        if state.revision != expected {
            return Err("release control revision changed; inspect current pending state".into());
        }
        let detail = apply(&transaction, &mut state)?;
        state.revision = state
            .revision
            .checked_add(1)
            .filter(|r| *r <= i64::MAX as u64)
            .ok_or("release control revision exhausted")?;
        if !state.valid() {
            return Err("invalid release control transition".into());
        }
        let body = serde_json::to_vec(&state)?;
        let detail = serde_json::to_vec(&detail)?;
        if body.len() > MAX_STATE || detail.len() > MAX_STATE {
            return Err("release control transition exceeds its bound".into());
        }
        transaction.execute(
            "UPDATE control SET revision=?1,body=?2 WHERE id=1",
            params![state.revision as i64, body],
        )?;
        transaction.execute(
            "INSERT INTO history VALUES(?1,?2,?3,?4)",
            params![
                state.revision as i64,
                action,
                detail,
                Sha256::digest(&body).as_slice()
            ],
        )?;
        transaction.commit()?;
        Ok(state)
    }
    /// Reserve sequence durably before installer side effects. Pending operations block new ones.
    pub fn begin(&self, expected: u64, encoded: &[u8], request: [u8; 16]) -> Result<State> {
        self.change(expected, "begin-install", |transaction, state| {
            if state.pending.is_some() || request == [0; 16] {
                return Err("release control already pending or request is invalid".into());
            }
            let signed = release::verify_manifest(encoded, &state.trusted_key)?;
            if signed.manifest().sequence <= state.highest_sequence {
                return Err("release sequence is not newer than the durable maximum".into());
            }
            let pending = Pending {
                request,
                candidate: Selection::from_signed(&signed),
                next_key: signed.manifest().next_key,
                purpose: Purpose::Install,
            };
            transaction.execute(
                "INSERT OR IGNORE INTO releases VALUES(?1,?2,?3,0)",
                params![
                    signed.id().as_slice(),
                    signed.signer().as_slice(),
                    signed.encoded()
                ],
            )?;
            state.highest_sequence = signed.manifest().sequence;
            state.pending = Some(pending.clone());
            Ok(serde_json::to_value(pending)?)
        })
    }
    /// Only known previously completed releases qualify for a recorded counter override.
    /// Evidence is a reference: the appliance coordinator still verifies data compatibility and
    /// recovery/signing policy before performing or completing any activation.
    pub fn begin_recovery(
        &self,
        expected: u64,
        release_id: [u8; 32],
        request: [u8; 16],
        evidence: Recovery,
    ) -> Result<State> {
        self.change(expected,"begin-recovery",|transaction,state|{
            if state.pending.is_some() || request==[0;16] || !evidence.valid() || state.active.as_ref().is_some_and(|s|s.release_id==release_id) {return Err("invalid or conflicting release recovery request".into());}
            let record:Option<(Vec<u8>,Vec<u8>)>=transaction.query_row("SELECT authority,manifest FROM releases WHERE id=?1 AND completed=1 AND length(manifest)<=?2",params![release_id.as_slice(),(release::MAX_MANIFEST+128) as i64],|row|Ok((row.get(0)?,row.get(1)?))).optional()?;
            let (authority,encoded)=record.ok_or("recovery target has never completed a verified installation")?;
            let authority:[u8;32]=authority.try_into().map_err(|_|"corrupt historical release authority")?;
            let signed=release::verify_manifest(&encoded,&authority)?;
            if *signed.id()!=release_id || signed.manifest().sequence>state.highest_sequence {return Err("corrupt historical release receipt".into());}
            let pending=Pending{request,candidate:Selection::from_signed(&signed),next_key:None,purpose:Purpose::Recovery{evidence}};
            state.pending=Some(pending.clone());
            Ok(serde_json::to_value(pending)?)
        })
    }
    /// Record completion only after the coordinator's compatible app/data selection is durable.
    pub fn complete(
        &self,
        expected: u64,
        request: [u8; 16],
        release_id: [u8; 32],
    ) -> Result<State> {
        self.change(expected, "complete", |transaction, state| {
            let pending = state
                .pending
                .take()
                .ok_or("no release operation is pending")?;
            if pending.request != request || pending.candidate.release_id != release_id {
                return Err("stale release completion ticket".into());
            }
            if let Purpose::Install = pending.purpose
                && let Some(next) = pending.next_key
            {
                state.trusted_key = next;
            }
            if transaction.execute(
                "UPDATE releases SET completed=1 WHERE id=?1",
                [release_id.as_slice()],
            )? != 1
            {
                return Err("release completion receipt is missing".into());
            }
            let previous = state.active.replace(pending.candidate.clone());
            Ok(serde_json::json!({"completed":pending,"previous":previous}))
        })
    }
    /// Record failed-operation recovery only after restoring the previously selected app/data.
    /// Retains the highest sequence and current authority, including after a failed first install.
    pub fn failed(
        &self,
        expected: u64,
        request: [u8; 16],
        reason: &str,
        recorded_at: i64,
    ) -> Result<State> {
        if reason.trim().is_empty() || reason.len() > 512 || reason.chars().any(char::is_control) {
            return Err("release recovery needs a bounded reason".into());
        }
        self.change(expected,"failed-operation-recovered",|_,state|{
            let pending=state.pending.take().ok_or("no release operation is pending")?;
            if pending.request!=request {return Err("stale release recovery ticket".into());}
            Ok(serde_json::json!({"failed":pending,"retained":state.active,"reason":reason,"recorded_at":recorded_at}))
        })
    }
    /// Bounded audit pagination; records are never silently pruned by this provider.
    pub fn history(&self, after: Option<u64>, limit: usize) -> Result<Vec<Audit>> {
        if !(1..=100).contains(&limit) || after.is_some_and(|r| r > i64::MAX as u64) {
            return Err("invalid release history page".into());
        }
        let connection = self.connection()?;
        let mut query=connection.prepare("SELECT revision,action,length(detail),detail,state_sha256 FROM history WHERE revision>?1 ORDER BY revision LIMIT ?2")?;
        let mut rows = query.query(params![after.map(|r| r as i64).unwrap_or(-1), limit as i64])?;
        let mut result = vec![];
        while let Some(row) = rows.next()? {
            let size: i64 = row.get(2)?;
            if size < 0 || size > MAX_STATE as i64 {
                return Err("oversized release history record".into());
            }
            let bytes: Vec<u8> = row.get(3)?;
            let sha: Vec<u8> = row.get(4)?;
            let revision: i64 = row.get(0)?;
            if bytes.len() > MAX_STATE || revision < 0 {
                return Err("corrupt release history record".into());
            }
            result.push(Audit {
                revision: revision as u64,
                action: row.get(1)?,
                detail: serde_json::from_slice(&bytes)?,
                state_sha256: sha
                    .try_into()
                    .map_err(|_| "corrupt release history digest")?,
            });
        }
        Ok(result)
    }
}
