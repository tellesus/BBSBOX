// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::{
    Account, Error, LocalAccountId, Result, Role, account_row, normalize_username,
    password::{Engine, PreparedPassword},
};
use bbsbox_storage::clock::Clock;
use rand_core::{OsRng, RngCore};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

#[derive(Clone, Copy)]
pub struct Policy {
    pub absolute_lifetime: Duration,
    pub idle_lifetime: Duration,
    pub attempts_per_window: u32,
    pub attempt_window: Duration,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            absolute_lifetime: Duration::from_secs(7 * 86400),
            idle_lifetime: Duration::from_secs(12 * 3600),
            attempts_per_window: 10,
            attempt_window: Duration::from_secs(60),
        }
    }
}
fn millis(duration: Duration) -> Result<i64> {
    i64::try_from(duration.as_millis())
        .ok()
        .filter(|n| *n > 0)
        .ok_or(Error::InvalidConfiguration)
}
fn origin(input: &str) -> Result<String> {
    let uri: http::Uri = input.parse().map_err(|_| Error::InvalidConfiguration)?;
    let authority = uri.authority().ok_or(Error::InvalidConfiguration)?;
    if uri.scheme_str() != Some("https")
        || authority.as_str().contains('@')
        || uri
            .path_and_query()
            .is_some_and(|pq| pq.as_str() != "/" && !pq.as_str().is_empty())
    {
        return Err(Error::InvalidConfiguration);
    }
    let authority = authority.as_str().to_ascii_lowercase();
    let authority = authority.strip_suffix(":443").unwrap_or(&authority);
    Ok(format!("https://{authority}"))
}
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn secret_hash(value: &str) -> Result<[u8; 32]> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Error::InvalidSession);
    }
    let mut bytes = Zeroizing::new([0u8; 32]);
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| Error::InvalidSession)?;
    }
    Ok(Sha256::digest(bytes.as_slice()).into())
}
/// Transport owns secrets only long enough to set cookies. No Debug/Clone/serialization implementation.
pub struct Login {
    pub account: Account,
    session: Zeroizing<String>,
    csrf: Zeroizing<String>,
    max_age: u64,
}
impl Login {
    pub fn session_token(&self) -> &str {
        &self.session
    }
    pub fn csrf_token(&self) -> &str {
        &self.csrf
    }
    pub fn cookie_headers(&self) -> [String; 2] {
        [
            format!(
                "__Host-bbsbox_session={}; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age={}",
                self.session.as_str(),
                self.max_age
            ),
            format!(
                "__Host-bbsbox_csrf={}; Path=/; Secure; SameSite=Strict; Max-Age={}",
                self.csrf.as_str(),
                self.max_age
            ),
        ]
    }
    pub fn clear_cookie_headers() -> [&'static str; 2] {
        [
            "__Host-bbsbox_session=; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age=0",
            "__Host-bbsbox_csrf=; Path=/; Secure; SameSite=Strict; Max-Age=0",
        ]
    }
}
pub struct VerifiedLogin {
    account: LocalAccountId,
    version: i64,
    boot: [u8; 16],
}
pub struct Mutation {
    session_hash: [u8; 32],
    boot: [u8; 16],
}
struct Attempt {
    start: i64,
    count: u32,
}
pub struct Auth {
    boot: [u8; 16],
    origin: String,
    clock: Arc<dyn Clock>,
    policy: Policy,
    engine: Engine,
    dummy: PreparedPassword,
    attempts: Mutex<HashMap<String, Attempt>>,
}
struct SessionRow {
    account: Account,
    session_version: i64,
    current_version: i64,
    boot: Vec<u8>,
    created: i64,
    seen: i64,
    csrf: Vec<u8>,
}
impl Auth {
    /// One shared instance per node process. A fresh boot intentionally invalidates old browser sessions.
    pub fn new(expected_origin: &str, clock: Arc<dyn Clock>, policy: Policy) -> Result<Self> {
        let origin = origin(expected_origin)?;
        if millis(policy.idle_lifetime)? > millis(policy.absolute_lifetime)?
            || policy.absolute_lifetime.as_secs() == 0
            || policy.attempts_per_window == 0
            || policy.attempts_per_window > 1000
        {
            return Err(Error::InvalidConfiguration);
        }
        millis(policy.attempt_window)?;
        let mut boot = [0; 16];
        OsRng.fill_bytes(&mut boot);
        let engine = Engine::default();
        let dummy = engine.prepare(Zeroizing::new(
            "bbsbox nonexistent account dummy password".into(),
        ))?;
        Ok(Self {
            boot,
            origin,
            clock,
            policy,
            engine,
            dummy,
            attempts: Mutex::new(HashMap::new()),
        })
    }
    pub fn passwords(&self) -> &Engine {
        &self.engine
    }
    fn now(&self) -> Result<i64> {
        let now = self.clock.monotonic_millis();
        if now < 0 {
            return Err(Error::InvalidConfiguration);
        }
        Ok(now)
    }
    fn check_origin(&self, request_origin: &str) -> Result<()> {
        if origin(request_origin).ok().as_ref() != Some(&self.origin) {
            return Err(Error::CsrfDenied);
        }
        Ok(())
    }
    fn attempt(&self, username: &str) -> Result<()> {
        let now = self.now()?;
        let window = millis(self.policy.attempt_window)?;
        let mut attempts = self.attempts.lock().map_err(|_| Error::Busy)?;
        attempts.retain(|_, v| now >= v.start && now - v.start < window);
        if attempts.len() >= 1024 && !attempts.contains_key(username) {
            return Err(Error::RateLimited);
        }
        let entry = attempts.entry(username.into()).or_insert(Attempt {
            start: now,
            count: 0,
        });
        if entry.count >= self.policy.attempts_per_window {
            return Err(Error::RateLimited);
        }
        entry.count += 1;
        Ok(())
    }
    /// Run with an independent read connection on a blocking authentication worker, before writer submission.
    pub fn verify_password(
        &self,
        reader: &Connection,
        username: &str,
        password: Zeroizing<String>,
        request_origin: &str,
    ) -> Result<VerifiedLogin> {
        self.check_origin(request_origin)?;
        let username = normalize_username(username).map_err(|_| Error::InvalidCredentials)?;
        self.attempt(&username)?;
        let row:Option<(Vec<u8>,String,i64,bool)>=reader.query_row("SELECT id,password_hash,credential_version,active FROM identity_accounts WHERE username=?1",[&username],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
        let encoded = row
            .as_ref()
            .map(|r| r.1.as_str())
            .unwrap_or(self.dummy.encoded());
        let valid = self.engine.verify(&password, encoded)?;
        let Some((id, _, version, active)) = row else {
            return Err(Error::InvalidCredentials);
        };
        if !valid || !active {
            return Err(Error::InvalidCredentials);
        }
        let id: [u8; 16] = id.try_into().map_err(|_| Error::InvalidCredentials)?;
        self.attempts
            .lock()
            .map_err(|_| Error::Busy)?
            .remove(&username);
        Ok(VerifiedLogin {
            account: LocalAccountId(id),
            version,
            boot: self.boot,
        })
    }
    pub(crate) fn proof_account(
        &self,
        tx: &Transaction<'_>,
        proof: &VerifiedLogin,
    ) -> Result<Account> {
        if proof.boot != self.boot {
            return Err(Error::InvalidCredentials);
        }
        let account = crate::account(tx, proof.account)?;
        let version: i64 = tx.query_row(
            "SELECT credential_version FROM identity_accounts WHERE id=?1",
            [proof.account.bytes().as_slice()],
            |r| r.get(0),
        )?;
        if !account.active || version != proof.version {
            return Err(Error::CredentialsChanged);
        }
        Ok(account)
    }
    /// Return cookies only after the enclosing storage write has committed. Never persist them in receipts.
    pub fn complete_login(&self, tx: &Transaction<'_>, proof: VerifiedLogin) -> Result<Login> {
        let account = self.proof_account(tx, &proof)?;
        let version = proof.version;
        let now = self.now()?;
        let absolute = millis(self.policy.absolute_lifetime)?;
        let idle = millis(self.policy.idle_lifetime)?;
        tx.execute(
            "DELETE FROM identity_sessions WHERE boot<>?1 OR created_tick<=?2 OR seen_tick<=?3",
            params![
                self.boot.as_slice(),
                now.saturating_sub(absolute),
                now.saturating_sub(idle)
            ],
        )?;
        // Bound persistent session growth per account while allowing several modern devices.
        tx.execute("DELETE FROM identity_sessions WHERE token_hash IN (SELECT token_hash FROM identity_sessions WHERE account_id=?1 ORDER BY created_tick DESC,rowid DESC LIMIT -1 OFFSET 31)",[account.id.bytes().as_slice()])?;
        let mut session = Zeroizing::new([0; 32]);
        let mut csrf = Zeroizing::new([0; 32]);
        OsRng.fill_bytes(session.as_mut());
        OsRng.fill_bytes(csrf.as_mut());
        tx.execute(
            "INSERT INTO identity_sessions VALUES(?1,?2,?3,?4,?5,?6,?6)",
            params![
                Sha256::digest(session.as_slice()).as_slice(),
                Sha256::digest(csrf.as_slice()).as_slice(),
                account.id.bytes().as_slice(),
                version,
                self.boot.as_slice(),
                now
            ],
        )?;
        Ok(Login {
            account,
            session: Zeroizing::new(hex(session.as_slice())),
            csrf: Zeroizing::new(hex(csrf.as_slice())),
            max_age: self.policy.absolute_lifetime.as_secs(),
        })
    }
    fn session(&self, connection: &Connection, hash: &[u8; 32]) -> Result<SessionRow> {
        let row:Option<SessionRow>=connection.query_row("SELECT a.id,a.username,a.role,a.active,s.credential_version,a.credential_version,s.boot,s.created_tick,s.seen_tick,s.csrf_hash FROM identity_sessions s JOIN identity_accounts a ON s.account_id=a.id WHERE s.token_hash=?1",[hash.as_slice()],|r|Ok(SessionRow{account:account_row(r)?,session_version:r.get(4)?,current_version:r.get(5)?,boot:r.get(6)?,created:r.get(7)?,seen:r.get(8)?,csrf:r.get(9)?})).optional()?;
        let row = row.ok_or(Error::InvalidSession)?;
        let now = self.now()?;
        if !row.account.active
            || row.session_version != row.current_version
            || row.boot != self.boot
            || now < row.created
            || now < row.seen
            || now - row.created >= millis(self.policy.absolute_lifetime)?
            || now - row.seen >= millis(self.policy.idle_lifetime)?
        {
            return Err(Error::InvalidSession);
        }
        Ok(row)
    }
    pub fn authenticate(&self, connection: &Connection, session_token: &str) -> Result<Account> {
        Ok(self
            .session(connection, &secret_hash(session_token)?)?
            .account)
    }
    pub fn authorize_mutation(
        &self,
        tx: &Transaction<'_>,
        session_token: &str,
        csrf_token: &str,
        request_origin: &str,
    ) -> Result<Mutation> {
        self.check_origin(request_origin)?;
        let hash = secret_hash(session_token)?;
        let row = self.session(tx, &hash)?;
        let csrf = secret_hash(csrf_token).map_err(|_| Error::CsrfDenied)?;
        if row.csrf.len() != 32 || !bool::from(row.csrf.as_slice().ct_eq(&csrf)) {
            return Err(Error::CsrfDenied);
        }
        Ok(Mutation {
            session_hash: hash,
            boot: self.boot,
        })
    }
    pub(crate) fn mutation_account(
        &self,
        tx: &Transaction<'_>,
        actor: &Mutation,
    ) -> Result<Account> {
        if actor.boot != self.boot {
            return Err(Error::InvalidSession);
        }
        Ok(self.session(tx, &actor.session_hash)?.account)
    }
    pub(crate) fn require_sysop(&self, tx: &Transaction<'_>, actor: &Mutation) -> Result<Account> {
        let account = self.mutation_account(tx, actor)?;
        if account.role != Role::Sysop {
            return Err(Error::Forbidden);
        }
        Ok(account)
    }
    pub fn refresh_session(&self, tx: &Transaction<'_>, actor: &Mutation) -> Result<()> {
        if actor.boot != self.boot {
            return Err(Error::InvalidSession);
        }
        self.session(tx, &actor.session_hash)?;
        tx.execute(
            "UPDATE identity_sessions SET seen_tick=?1 WHERE token_hash=?2",
            params![self.now()?, actor.session_hash.as_slice()],
        )?;
        Ok(())
    }
    pub fn logout(&self, tx: &Transaction<'_>, actor: &Mutation) -> Result<()> {
        if actor.boot != self.boot {
            return Err(Error::InvalidSession);
        }
        self.session(tx, &actor.session_hash)?;
        tx.execute(
            "DELETE FROM identity_sessions WHERE token_hash=?1",
            [actor.session_hash.as_slice()],
        )?;
        Ok(())
    }
}
pub fn session_from_cookie_header(header: &str) -> Result<&str> {
    let mut token = None;
    for part in header.split(';') {
        let Some((name, value)) = part.trim().split_once('=') else {
            continue;
        };
        if name == "__Host-bbsbox_session" {
            if token.is_some() {
                return Err(Error::InvalidSession);
            }
            secret_hash(value)?;
            token = Some(value);
        }
    }
    token.ok_or(Error::InvalidSession)
}
