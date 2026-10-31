// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_identity::{
    self as identity, Account, Auth, Error, Policy, Role, bootstrap_sysop,
    password::Engine,
    session::{Login, session_from_cookie_header},
};
use bbsbox_storage::{
    Database,
    clock::Clock,
    disk::Class,
    open_reader,
    writer::{Config, Priority, Writer},
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
    time::Duration,
};
use zeroize::Zeroizing;
const ORIGIN: &str = "https://bbsbox.local:8443";
const PASSWORD: &str = "this is a local test passphrase";
const REPLACEMENT: &str = "a different local test passphrase";
#[derive(Default)]
struct TestClock {
    wall: AtomicI64,
    tick: AtomicI64,
}
impl Clock for TestClock {
    fn wall_millis(&self) -> i64 {
        self.wall.load(Ordering::SeqCst)
    }
    fn monotonic_millis(&self) -> i64 {
        self.tick.load(Ordering::SeqCst)
    }
}
struct Fixture {
    _dir: tempfile::TempDir,
    path: PathBuf,
    db: Database,
    auth: Arc<Auth>,
    clock: Arc<TestClock>,
    sysop: Account,
    login: Login,
}
fn secret(value: &str) -> Zeroizing<String> {
    Zeroizing::new(value.into())
}
impl Fixture {
    fn new() -> Self {
        Self::with_policy(Policy::default())
    }
    fn with_policy(policy: Policy) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("node.sqlite");
        let mut db = Database::open(&path, &identity::migrations()).unwrap();
        let clock = Arc::new(TestClock::default());
        let auth = Arc::new(Auth::new(ORIGIN, clock.clone(), policy).unwrap());
        let prepared = auth.passwords().prepare(secret(PASSWORD)).unwrap();
        let sysop = db
            .write(Class::Control, 65536, |tx| {
                bootstrap_sysop(tx, "SysOp", prepared)
            })
            .unwrap();
        let proof = auth
            .verify_password(
                &open_reader(&path).unwrap(),
                "sysop",
                secret(PASSWORD),
                ORIGIN,
            )
            .unwrap();
        let login = db
            .write(Class::Control, 65536, |tx| auth.complete_login(tx, proof))
            .unwrap();
        Self {
            _dir: dir,
            path,
            db,
            auth,
            clock,
            sysop,
            login,
        }
    }
    fn create(&mut self, name: &str, role: Role) -> Account {
        let prepared = self.auth.passwords().prepare(secret(PASSWORD)).unwrap();
        self.db
            .write(Class::Control, 65536, |tx| {
                let actor = self.auth.authorize_mutation(
                    tx,
                    self.login.session_token(),
                    self.login.csrf_token(),
                    ORIGIN,
                )?;
                self.auth.create_account(tx, &actor, name, prepared, role)
            })
            .unwrap()
    }
    fn login_as(&mut self, name: &str, password: &str) -> Login {
        let proof = self
            .auth
            .verify_password(
                &open_reader(&self.path).unwrap(),
                name,
                secret(password),
                ORIGIN,
            )
            .unwrap();
        self.db
            .write(Class::Control, 65536, |tx| {
                self.auth.complete_login(tx, proof)
            })
            .unwrap()
    }
}
#[test]
fn bootstrap_and_normalized_usernames_have_one_atomic_local_owner() {
    let mut f = Fixture::new();
    assert_eq!(f.sysop.username, "sysop");
    assert_eq!(f.sysop.role, Role::Sysop);
    let prepared = f.auth.passwords().prepare(secret(PASSWORD)).unwrap();
    assert!(matches!(
        f.db.write(Class::Control, 65536, |tx| bootstrap_sysop(
            tx, "other", prepared
        )),
        Err(Error::AlreadyBootstrapped)
    ));
    let member = f.create("Neighbor_1", Role::Member);
    assert_eq!(member.username, "neighbor_1");
    let prepared = f.auth.passwords().prepare(secret(PASSWORD)).unwrap();
    assert!(matches!(
        f.db.write(Class::Control, 65536, |tx| {
            let actor = f.auth.authorize_mutation(
                tx,
                f.login.session_token(),
                f.login.csrf_token(),
                ORIGIN,
            )?;
            f.auth
                .create_account(tx, &actor, "NEIGHBOR_1", prepared, Role::Member)
        }),
        Err(Error::UsernameTaken)
    ));
    assert!(matches!(
        identity::normalize_username("bad name"),
        Err(Error::InvalidUsername)
    ));
    assert!(matches!(
        identity::normalize_username(&"x".repeat(33)),
        Err(Error::InvalidUsername)
    ));
}
#[test]
fn credentials_are_salted_and_sessions_store_hashes_instead_of_bearer_secrets() {
    let mut f = Fixture::new();
    let member = f.create("member", Role::Member);
    let reader = open_reader(&f.path).unwrap();
    let hashes = reader
        .prepare("SELECT password_hash FROM identity_accounts")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap();
    assert_ne!(hashes[0], hashes[1]);
    for hash in hashes {
        assert!(hash.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"));
        assert!(!hash.contains(PASSWORD));
    }
    for user in ["sysop", "unknown"] {
        assert!(matches!(
            f.auth
                .verify_password(&reader, user, secret("wrong password value"), ORIGIN),
            Err(Error::InvalidCredentials)
        ));
    }
    let login = f.login_as("MEMBER", PASSWORD);
    assert_eq!(login.account.id, member.id);
    assert_ne!(login.session_token(), f.login.session_token());
    let (session_hash, csrf_hash): (Vec<u8>, Vec<u8>) = reader
        .query_row(
            "SELECT token_hash,csrf_hash FROM identity_sessions WHERE account_id=?1",
            [member.id.bytes().as_slice()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(session_hash.len(), 32);
    assert_ne!(session_hash.as_slice(), login.session_token().as_bytes());
    assert_eq!(csrf_hash.len(), 32);
    assert!(matches!(
        f.auth.passwords().prepare(secret("too short")),
        Err(Error::WeakPassword)
    ));
}
#[test]
fn csrf_origin_cookie_and_logout_rules_apply_before_mutation() {
    let mut f = Fixture::new();
    let cookies = f.login.cookie_headers();
    assert!(cookies[0].contains("HttpOnly"));
    assert!(cookies[0].contains("Secure;"));
    assert!(cookies[0].contains("SameSite=Strict"));
    assert!(!cookies[0].contains("Domain="));
    assert!(!cookies[1].contains("HttpOnly"));
    let header = format!(
        "other=value; __Host-bbsbox_session={}; theme=dark",
        f.login.session_token()
    );
    assert_eq!(
        session_from_cookie_header(&header).unwrap(),
        f.login.session_token()
    );
    assert!(matches!(
        session_from_cookie_header(&format!(
            "{header}; __Host-bbsbox_session={}",
            f.login.session_token()
        )),
        Err(Error::InvalidSession)
    ));
    for origin in [
        "http://bbsbox.local:8443",
        "https://evil.local",
        "null",
        "https://bbsbox.local:8443.evil.local",
    ] {
        assert!(matches!(
            f.db.write(Class::Control, 65536, |tx| f.auth.authorize_mutation(
                tx,
                f.login.session_token(),
                f.login.csrf_token(),
                origin
            )),
            Err(Error::CsrfDenied)
        ));
    }
    assert!(matches!(
        f.db.write(Class::Control, 65536, |tx| f.auth.authorize_mutation(
            tx,
            f.login.session_token(),
            &"0".repeat(64),
            ORIGIN
        )),
        Err(Error::CsrfDenied)
    ));
    f.db.write(Class::Control, 65536, |tx| {
        let actor =
            f.auth
                .authorize_mutation(tx, f.login.session_token(), f.login.csrf_token(), ORIGIN)?;
        f.auth.logout(tx, &actor)
    })
    .unwrap();
    assert!(matches!(
        f.auth
            .authenticate(&open_reader(&f.path).unwrap(), f.login.session_token()),
        Err(Error::InvalidSession)
    ));
}
#[test]
fn members_cannot_administer_and_a_disabled_account_loses_existing_sessions() {
    let mut f = Fixture::new();
    let member = f.create("member", Role::Member);
    let login = f.login_as("member", PASSWORD);
    let prepared = f.auth.passwords().prepare(secret(PASSWORD)).unwrap();
    assert!(matches!(
        f.db.write(Class::Control, 65536, |tx| {
            let actor =
                f.auth
                    .authorize_mutation(tx, login.session_token(), login.csrf_token(), ORIGIN)?;
            f.auth
                .create_account(tx, &actor, "intruder", prepared, Role::Sysop)
        }),
        Err(Error::Forbidden)
    ));
    f.db.write(Class::Control, 65536, |tx| {
        let actor =
            f.auth
                .authorize_mutation(tx, f.login.session_token(), f.login.csrf_token(), ORIGIN)?;
        f.auth
            .set_access(tx, &actor, member.id, Role::Member, false)
    })
    .unwrap();
    assert!(matches!(
        f.auth
            .authenticate(&open_reader(&f.path).unwrap(), login.session_token()),
        Err(Error::InvalidSession)
    ));
    assert!(matches!(
        f.auth.verify_password(
            &open_reader(&f.path).unwrap(),
            "member",
            secret(PASSWORD),
            ORIGIN
        ),
        Err(Error::InvalidCredentials)
    ));
}
#[test]
fn last_sysop_is_preserved_and_old_authorization_cannot_survive_demotion() {
    let mut f = Fixture::new();
    for (role, active) in [(Role::Member, true), (Role::Sysop, false)] {
        assert!(matches!(
            f.db.write(Class::Control, 65536, |tx| {
                let actor = f.auth.authorize_mutation(
                    tx,
                    f.login.session_token(),
                    f.login.csrf_token(),
                    ORIGIN,
                )?;
                f.auth.set_access(tx, &actor, f.sysop.id, role, active)
            }),
            Err(Error::LastSysop)
        ));
    }
    let other = f.create("other_sysop", Role::Sysop);
    let login = f.login_as("other_sysop", PASSWORD);
    let saved =
        f.db.write(Class::Control, 65536, |tx| {
            f.auth
                .authorize_mutation(tx, login.session_token(), login.csrf_token(), ORIGIN)
        })
        .unwrap();
    f.db.write(Class::Control, 65536, |tx| {
        let actor =
            f.auth
                .authorize_mutation(tx, f.login.session_token(), f.login.csrf_token(), ORIGIN)?;
        f.auth.set_access(tx, &actor, other.id, Role::Member, true)
    })
    .unwrap();
    let prepared = f.auth.passwords().prepare(secret(PASSWORD)).unwrap();
    assert!(matches!(
        f.db.write(Class::Control, 65536, |tx| f.auth.create_account(
            tx,
            &saved,
            "intruder",
            prepared,
            Role::Sysop
        )),
        Err(Error::InvalidSession)
    ));
}
#[test]
fn password_reset_fences_pending_verification_and_revokes_old_sessions() {
    let mut f = Fixture::new();
    let member = f.create("member", Role::Member);
    let login = f.login_as("member", PASSWORD);
    let pending = f
        .auth
        .verify_password(
            &open_reader(&f.path).unwrap(),
            "member",
            secret(PASSWORD),
            ORIGIN,
        )
        .unwrap();
    let prepared = f.auth.passwords().prepare(secret(REPLACEMENT)).unwrap();
    f.db.write(Class::Control, 65536, |tx| {
        let actor =
            f.auth
                .authorize_mutation(tx, f.login.session_token(), f.login.csrf_token(), ORIGIN)?;
        f.auth.reset_password(tx, &actor, member.id, prepared)
    })
    .unwrap();
    assert!(matches!(
        f.db.write(Class::Control, 65536, |tx| f
            .auth
            .complete_login(tx, pending)),
        Err(Error::CredentialsChanged)
    ));
    assert!(matches!(
        f.auth
            .authenticate(&open_reader(&f.path).unwrap(), login.session_token()),
        Err(Error::InvalidSession)
    ));
    assert!(matches!(
        f.auth.verify_password(
            &open_reader(&f.path).unwrap(),
            "member",
            secret(PASSWORD),
            ORIGIN
        ),
        Err(Error::InvalidCredentials)
    ));
    assert_eq!(f.login_as("member", REPLACEMENT).account.id, member.id);
}
#[test]
fn monotonic_expiry_refresh_and_process_boot_are_separate_from_wall_time() {
    let mut f = Fixture::new();
    f.clock.wall.store(-9000000, Ordering::SeqCst);
    f.clock.tick.store(11 * 3600 * 1000, Ordering::SeqCst);
    assert!(
        f.auth
            .authenticate(&open_reader(&f.path).unwrap(), f.login.session_token())
            .is_ok()
    );
    f.db.write(Class::Control, 65536, |tx| {
        let actor =
            f.auth
                .authorize_mutation(tx, f.login.session_token(), f.login.csrf_token(), ORIGIN)?;
        f.auth.refresh_session(tx, &actor)
    })
    .unwrap();
    f.clock.tick.store(22 * 3600 * 1000, Ordering::SeqCst);
    assert!(
        f.auth
            .authenticate(&open_reader(&f.path).unwrap(), f.login.session_token())
            .is_ok()
    );
    f.clock.tick.store(24 * 3600 * 1000, Ordering::SeqCst);
    assert!(matches!(
        f.auth
            .authenticate(&open_reader(&f.path).unwrap(), f.login.session_token()),
        Err(Error::InvalidSession)
    ));
    let login = f.login_as("sysop", PASSWORD);
    let restarted = Auth::new(ORIGIN, f.clock.clone(), Policy::default()).unwrap();
    assert!(matches!(
        restarted.authenticate(&open_reader(&f.path).unwrap(), login.session_token()),
        Err(Error::InvalidSession)
    ));
    let proof = restarted
        .verify_password(
            &open_reader(&f.path).unwrap(),
            "sysop",
            secret(PASSWORD),
            ORIGIN,
        )
        .unwrap();
    f.db.write(Class::Control, 65536, |tx| {
        restarted.complete_login(tx, proof)
    })
    .unwrap();
    assert_eq!(
        open_reader(&f.path)
            .unwrap()
            .query_row("SELECT count(*) FROM identity_sessions", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
}
#[test]
fn attempts_are_bounded_for_unknown_and_known_accounts_and_cooldown_recovers() {
    let f = Fixture::new();
    let reader = open_reader(&f.path).unwrap();
    let policy = Policy {
        attempts_per_window: 2,
        attempt_window: Duration::from_secs(30),
        ..Policy::default()
    };
    let auth = Auth::new(ORIGIN, f.clock.clone(), policy).unwrap();
    for username in ["sysop", "unknown"] {
        for _ in 0..2 {
            assert!(matches!(
                auth.verify_password(&reader, username, secret("incorrect passphrase"), ORIGIN),
                Err(Error::InvalidCredentials)
            ));
        }
        assert!(matches!(
            auth.verify_password(&reader, username, secret(PASSWORD), ORIGIN),
            Err(Error::RateLimited)
        ));
    }
    f.clock.tick.store(30000, Ordering::SeqCst);
    assert!(
        auth.verify_password(&reader, "sysop", secret(PASSWORD), ORIGIN)
            .is_ok()
    );
}
#[test]
fn typed_service_errors_roll_back_and_writer_still_accepts_later_work() {
    let f = Fixture::new();
    drop(f.db);
    let writer = Writer::start(&f.path, identity::migrations(), Config::default()).unwrap();
    let pending = writer
        .handle()
        .submit_service(Priority::Interactive, |db| {
            db.write::<(), Error>(Class::Control, 65536, |tx| {
                tx.execute("UPDATE identity_accounts SET username='changed'", [])?;
                Err(Error::Forbidden)
            })
        })
        .unwrap();
    assert!(matches!(pending.blocking_wait(), Err(Error::Forbidden)));
    assert_eq!(
        identity::account(&open_reader(&f.path).unwrap(), f.sysop.id)
            .unwrap()
            .username,
        "sysop"
    );
    let pending = writer
        .handle()
        .submit_service::<(), Error>(Priority::Interactive, |_| panic!("fixture"))
        .unwrap();
    assert!(matches!(
        pending.blocking_wait(),
        Err(Error::Storage(bbsbox_storage::Error::WorkPanicked))
    ));
    writer
        .handle()
        .submit(Priority::Interactive, |_| Ok(()))
        .unwrap()
        .blocking_wait()
        .unwrap();
    writer.shutdown().unwrap();
}
#[test]
fn invalid_auth_configuration_is_rejected() {
    let clock = Arc::new(TestClock::default());
    for origin in [
        "http://bbsbox.local",
        "https://bbsbox.local/path",
        "https://user@bbsbox.local",
        "https://bbsbox.local/?query=1",
    ] {
        assert!(matches!(
            Auth::new(origin, clock.clone(), Policy::default()),
            Err(Error::InvalidConfiguration)
        ));
    }
    assert!(Engine::new(0).is_err());
    assert!(matches!(
        Auth::new(
            ORIGIN,
            clock,
            Policy {
                idle_lifetime: Duration::from_secs(10),
                absolute_lifetime: Duration::from_secs(5),
                ..Policy::default()
            }
        ),
        Err(Error::InvalidConfiguration)
    ));
}

#[test]
fn idle_refresh_cannot_extend_absolute_session_lifetime() {
    let mut f = Fixture::with_policy(Policy {
        absolute_lifetime: Duration::from_secs(10),
        idle_lifetime: Duration::from_secs(6),
        ..Policy::default()
    });
    for tick in [5000, 9000] {
        f.clock.tick.store(tick, Ordering::SeqCst);
        f.db.write(Class::Control, 65536, |tx| {
            let actor = f.auth.authorize_mutation(
                tx,
                f.login.session_token(),
                f.login.csrf_token(),
                ORIGIN,
            )?;
            f.auth.refresh_session(tx, &actor)
        })
        .unwrap();
    }
    f.clock.tick.store(10000, Ordering::SeqCst);
    assert!(matches!(
        f.auth
            .authenticate(&open_reader(&f.path).unwrap(), f.login.session_token()),
        Err(Error::InvalidSession)
    ));
}
#[test]
fn a_member_can_change_only_their_own_password_with_current_password_proof() {
    let mut f = Fixture::new();
    f.create("member", Role::Member);
    let login = f.login_as("member", PASSWORD);
    let other_proof = f
        .auth
        .verify_password(
            &open_reader(&f.path).unwrap(),
            "sysop",
            secret(PASSWORD),
            ORIGIN,
        )
        .unwrap();
    let replacement = f.auth.passwords().prepare(secret(REPLACEMENT)).unwrap();
    assert!(matches!(
        f.db.write(Class::Control, 65536, |tx| {
            let actor =
                f.auth
                    .authorize_mutation(tx, login.session_token(), login.csrf_token(), ORIGIN)?;
            f.auth.change_password(tx, &actor, other_proof, replacement)
        }),
        Err(Error::Forbidden)
    ));
    let proof = f
        .auth
        .verify_password(
            &open_reader(&f.path).unwrap(),
            "member",
            secret(PASSWORD),
            ORIGIN,
        )
        .unwrap();
    let replacement = f.auth.passwords().prepare(secret(REPLACEMENT)).unwrap();
    f.db.write(Class::Control, 65536, |tx| {
        let actor =
            f.auth
                .authorize_mutation(tx, login.session_token(), login.csrf_token(), ORIGIN)?;
        f.auth.change_password(tx, &actor, proof, replacement)
    })
    .unwrap();
    assert!(matches!(
        f.auth
            .authenticate(&open_reader(&f.path).unwrap(), login.session_token()),
        Err(Error::InvalidSession)
    ));
    f.login_as("member", REPLACEMENT);
}
#[test]
fn typed_receipts_keep_module_errors_and_atomic_retry_behavior() {
    let mut f = Fixture::new();
    let command = || bbsbox_storage::Command {
        namespace: "identity/fixture",
        id: "rename",
        request: b"newname",
    };
    let failed =
        f.db.transaction_service::<Error>(command(), Class::Control, 65536, |tx| {
            tx.execute("UPDATE identity_accounts SET username='newname'", [])?;
            Err(Error::Forbidden)
        });
    assert!(matches!(failed, Err(Error::Forbidden)));
    assert_eq!(
        identity::account(&open_reader(&f.path).unwrap(), f.sysop.id)
            .unwrap()
            .username,
        "sysop"
    );
    f.db.transaction_service::<Error>(command(), Class::Control, 65536, |tx| {
        tx.execute("UPDATE identity_accounts SET username='newname'", [])?;
        Ok(b"done".to_vec())
    })
    .unwrap();
    let receipt = f
        .db
        .transaction_service::<Error>(command(), Class::Control, 65536, |_| panic!("must replay"))
        .unwrap();
    assert!(receipt.replayed);
    assert_eq!(receipt.bytes, b"done");
}

#[test]
fn unsupported_or_excessive_stored_hash_parameters_are_rejected_before_hashing() {
    let f = Fixture::new();
    let raw = rusqlite::Connection::open(&f.path).unwrap();
    let original: String = raw
        .query_row("SELECT password_hash FROM identity_accounts", [], |r| {
            r.get(0)
        })
        .unwrap();
    for value in [
        "bad record".to_string(),
        original.replace("m=19456", "m=2147483647"),
        original.replace("argon2id", "argon2i"),
    ] {
        raw.execute("UPDATE identity_accounts SET password_hash=?1", [value])
            .unwrap();
        assert!(matches!(
            f.auth.verify_password(
                &open_reader(&f.path).unwrap(),
                "sysop",
                secret(PASSWORD),
                ORIGIN
            ),
            Err(Error::InvalidStoredHash)
        ));
    }
}
