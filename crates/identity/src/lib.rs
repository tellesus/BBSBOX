// SPDX-License-Identifier: AGPL-3.0-or-later
//! Local authentication only. A local session does not establish portable person/forum credentials.
use bbsbox_storage::Migration;
use rand_core::{OsRng, RngCore};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use std::fmt;

pub mod password;
pub mod session;
pub use session::{Auth, Policy};
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug)]
pub enum Error {
    Storage(bbsbox_storage::Error),
    Sqlite(rusqlite::Error),
    InvalidUsername,
    WeakPassword,
    InvalidCredentials,
    InvalidSession,
    CsrfDenied,
    Forbidden,
    AlreadyBootstrapped,
    UsernameTaken,
    LastSysop,
    AccountMissing,
    CredentialsChanged,
    Busy,
    RateLimited,
    InvalidConfiguration,
    InvalidStoredHash,
}
impl From<bbsbox_storage::Error> for Error {
    fn from(e: bbsbox_storage::Error) -> Self {
        Self::Storage(e)
    }
}
impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sqlite(e)
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Storage(_) => "storage operation failed",
            Self::Sqlite(_) => "account database operation failed",
            Self::InvalidUsername => {
                "username must contain 1–32 ASCII letters, digits, underscores or hyphens"
            }
            Self::WeakPassword => {
                "password must contain at least 15 characters and at most 1024 bytes"
            }
            Self::InvalidCredentials => "invalid username or password",
            Self::InvalidSession => "please log in again",
            Self::CsrfDenied => "request origin or CSRF proof is invalid",
            Self::Forbidden => "this action requires a SysOp",
            Self::AlreadyBootstrapped => "a SysOp has already been bootstrapped",
            Self::UsernameTaken => "username is already in use",
            Self::LastSysop => "the last active SysOp must remain active",
            Self::AccountMissing => "account was not found",
            Self::CredentialsChanged => "credentials changed; log in again",
            Self::Busy => "password worker is busy; retry later",
            Self::RateLimited => "too many login attempts; retry after the local cooldown",
            Self::InvalidConfiguration => "invalid authentication configuration",
            Self::InvalidStoredHash => "stored password hash is unsupported or corrupt",
        })
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Storage(e) => Some(e),
            Self::Sqlite(e) => Some(e),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalAccountId([u8; 16]);
impl LocalAccountId {
    pub fn bytes(&self) -> &[u8; 16] {
        &self.0
    }
}
impl fmt::Display for LocalAccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&session::hex(&self.0))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Member,
    Sysop,
}
impl Role {
    fn stored(self) -> &'static str {
        match self {
            Self::Member => "member",
            Self::Sysop => "sysop",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    pub id: LocalAccountId,
    pub username: String,
    pub role: Role,
    pub active: bool,
}
pub fn normalize_username(value: &str) -> Result<String> {
    if value.is_empty()
        || value.len() > 32
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(Error::InvalidUsername);
    }
    Ok(value.to_ascii_lowercase())
}
pub fn migrations() -> Vec<Migration> {
    vec![Migration{module:"identity",version:1,sql:"
CREATE TABLE identity_settings(id INTEGER PRIMARY KEY CHECK(id=1),bootstrapped INTEGER NOT NULL CHECK(bootstrapped=1));
CREATE TABLE identity_accounts(id BLOB PRIMARY KEY CHECK(length(id)=16),username TEXT NOT NULL UNIQUE,password_hash TEXT NOT NULL,credential_version INTEGER NOT NULL DEFAULT 1,role TEXT NOT NULL CHECK(role IN('member','sysop')),active INTEGER NOT NULL DEFAULT 1 CHECK(active IN(0,1)));
CREATE TABLE identity_sessions(token_hash BLOB PRIMARY KEY CHECK(length(token_hash)=32),csrf_hash BLOB NOT NULL CHECK(length(csrf_hash)=32),account_id BLOB NOT NULL REFERENCES identity_accounts(id),credential_version INTEGER NOT NULL,boot BLOB NOT NULL CHECK(length(boot)=16),created_tick INTEGER NOT NULL,seen_tick INTEGER NOT NULL);
CREATE INDEX identity_sessions_account ON identity_sessions(account_id);"}]
}

pub(crate) fn account_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Account> {
    let raw: Vec<u8> = row.get(0)?;
    let id: [u8; 16] = raw.try_into().map_err(|_| rusqlite::Error::InvalidQuery)?;
    let role: String = row.get(2)?;
    let role = match role.as_str() {
        "member" => Role::Member,
        "sysop" => Role::Sysop,
        _ => return Err(rusqlite::Error::InvalidQuery),
    };
    Ok(Account {
        id: LocalAccountId(id),
        username: row.get(1)?,
        role,
        active: row.get(3)?,
    })
}
pub fn account(connection: &Connection, id: LocalAccountId) -> Result<Account> {
    connection
        .query_row(
            "SELECT id,username,role,active FROM identity_accounts WHERE id=?1",
            [id.bytes().as_slice()],
            account_row,
        )
        .optional()?
        .ok_or(Error::AccountMissing)
}
fn insert_account(
    tx: &Transaction<'_>,
    username: &str,
    password: password::PreparedPassword,
    role: Role,
) -> Result<Account> {
    let username = normalize_username(username)?;
    if tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM identity_accounts WHERE username=?1)",
        [&username],
        |r| r.get::<_, bool>(0),
    )? {
        return Err(Error::UsernameTaken);
    }
    let mut id = [0; 16];
    OsRng.fill_bytes(&mut id);
    tx.execute(
        "INSERT INTO identity_accounts(id,username,password_hash,role) VALUES(?1,?2,?3,?4)",
        params![id.as_slice(), username, password.encoded(), role.stored()],
    )?;
    account(tx, LocalAccountId(id))
}
/// Local setup/CLI only; never expose this operation as a public registration endpoint.
pub fn bootstrap_sysop(
    tx: &Transaction<'_>,
    username: &str,
    password: password::PreparedPassword,
) -> Result<Account> {
    if tx.query_row("SELECT EXISTS(SELECT 1 FROM identity_settings)", [], |r| {
        r.get::<_, bool>(0)
    })? || tx.query_row("SELECT EXISTS(SELECT 1 FROM identity_accounts)", [], |r| {
        r.get::<_, bool>(0)
    })? {
        return Err(Error::AlreadyBootstrapped);
    }
    let account = insert_account(tx, username, password, Role::Sysop)?;
    tx.execute("INSERT INTO identity_settings VALUES(1,1)", [])?;
    Ok(account)
}
impl Auth {
    pub fn create_account(
        &self,
        tx: &Transaction<'_>,
        actor: &session::Mutation,
        username: &str,
        password: password::PreparedPassword,
        role: Role,
    ) -> Result<Account> {
        self.require_sysop(tx, actor)?;
        insert_account(tx, username, password, role)
    }
    pub fn set_access(
        &self,
        tx: &Transaction<'_>,
        actor: &session::Mutation,
        id: LocalAccountId,
        role: Role,
        active: bool,
    ) -> Result<Account> {
        self.require_sysop(tx, actor)?;
        let old = account(tx, id)?;
        if old.role == role && old.active == active {
            return Ok(old);
        }
        if old.active && old.role == Role::Sysop && (!active || role != Role::Sysop) {
            let count: i64 = tx.query_row(
                "SELECT count(*) FROM identity_accounts WHERE active=1 AND role='sysop'",
                [],
                |r| r.get(0),
            )?;
            if count <= 1 {
                return Err(Error::LastSysop);
            }
        }
        tx.execute(
            "UPDATE identity_accounts SET role=?1,active=?2 WHERE id=?3",
            params![role.stored(), active, id.bytes().as_slice()],
        )?;
        tx.execute(
            "DELETE FROM identity_sessions WHERE account_id=?1",
            [id.bytes().as_slice()],
        )?;
        account(tx, id)
    }
    /// Local password reset revokes sessions; it cannot recover or replace portable/E2E credentials.
    pub fn reset_password(
        &self,
        tx: &Transaction<'_>,
        actor: &session::Mutation,
        id: LocalAccountId,
        password: password::PreparedPassword,
    ) -> Result<()> {
        self.require_sysop(tx, actor)?;
        account(tx, id)?;
        update_password(tx, id, password)
    }
    pub fn change_password(
        &self,
        tx: &Transaction<'_>,
        actor: &session::Mutation,
        proof: session::VerifiedLogin,
        password: password::PreparedPassword,
    ) -> Result<()> {
        let current = self.mutation_account(tx, actor)?;
        let verified = self.proof_account(tx, &proof)?;
        if current.id != verified.id {
            return Err(Error::Forbidden);
        }
        update_password(tx, current.id, password)
    }
}
fn update_password(
    tx: &Transaction<'_>,
    id: LocalAccountId,
    password: password::PreparedPassword,
) -> Result<()> {
    tx.execute("UPDATE identity_accounts SET password_hash=?1,credential_version=credential_version+1 WHERE id=?2",params![password.encoded(),id.bytes().as_slice()])?;
    tx.execute(
        "DELETE FROM identity_sessions WHERE account_id=?1",
        [id.bytes().as_slice()],
    )?;
    Ok(())
}
