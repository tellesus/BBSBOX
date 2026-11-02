// SPDX-License-Identifier: AGPL-3.0-or-later
//! Release-policy fixtures only; no executable, installed data or signing incarnation is activated.
use bbsbox_maintenance::{
    release::{self, ReleaseSigner, SignedRelease},
    release_trust::{Purpose, Recovery, Store},
};
use rusqlite::Connection;
#[cfg(unix)]
use std::fs;
use std::{
    path::PathBuf,
    sync::{Arc, Barrier},
    thread,
};
use zeroize::Zeroizing;

fn signer(seed: u8) -> ReleaseSigner {
    ReleaseSigner::from_secret(Zeroizing::new([seed; 32])).unwrap()
}
fn signed(sequence: u64, signer: &ReleaseSigner, next: Option<[u8; 32]>) -> SignedRelease {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../protocol/fixtures/release-manifest-v1.json"
    ))
    .unwrap();
    let encoded = hex::decode(fixture["cose_hex"].as_str().unwrap()).unwrap();
    let public: [u8; 32] = hex::decode(fixture["public_hex"].as_str().unwrap())
        .unwrap()
        .try_into()
        .unwrap();
    let mut manifest = release::verify_manifest(&encoded, &public)
        .unwrap()
        .manifest()
        .clone();
    manifest.issuer = signer.id().to_string();
    manifest.sequence = sequence;
    manifest.version = format!("0.1.{sequence}");
    manifest.next_key = next;
    release::sign(manifest, signer).unwrap()
}
struct Fixture {
    _temporary: tempfile::TempDir,
    root: PathBuf,
    path: PathBuf,
    store: Store,
    key: ReleaseSigner,
}
impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().canonicalize().unwrap();
        let path = root.join("release-control");
        let key = signer(41);
        let store = Store::initialize(&path, key.public()).unwrap();
        Self {
            _temporary: temporary,
            root,
            path,
            store,
            key,
        }
    }
    fn connection(&self) -> Connection {
        Connection::open(self.path.join("release-trust.sqlite")).unwrap()
    }
    fn install(&self, signed: &SignedRelease, request: u8) {
        let before = self.store.state().unwrap();
        let begun = self
            .store
            .begin(before.revision, signed.encoded(), [request; 16])
            .unwrap();
        self.store
            .complete(begun.revision, [request; 16], *signed.id())
            .unwrap();
    }
}

#[test]
fn pending_restart_fencing_key_rotation_and_failed_install_never_rewind_sequence() {
    let f = Fixture::new();
    let next = signer(42);
    let first = signed(1, &f.key, Some(next.public()));
    let pending = f.store.begin(0, first.encoded(), [1; 16]).unwrap();
    assert_eq!(pending.trusted_key, f.key.public());
    let store = Store::open(&f.path).unwrap();
    assert_eq!(store.state().unwrap(), pending);
    assert!(store.complete(1, [2; 16], *first.id()).is_err());
    assert!(store.complete(1, [1; 16], [3; 32]).is_err());
    assert!(
        store
            .begin(1, signed(2, &next, None).encoded(), [2; 16])
            .is_err()
    );
    assert_eq!(store.state().unwrap(), pending);
    let active = store.complete(1, [1; 16], *first.id()).unwrap();
    assert_eq!(active.trusted_key, next.public());
    assert_eq!(active.highest_sequence, 1);
    assert!(
        store
            .begin(2, signed(2, &f.key, None).encoded(), [2; 16])
            .is_err()
    );
    assert!(
        store
            .begin(2, signed(1, &next, None).encoded(), [2; 16])
            .is_err()
    );
    let second = signed(2, &next, None);
    let begun = store.begin(2, second.encoded(), [2; 16]).unwrap();
    let failed = store
        .failed(
            begun.revision,
            [2; 16],
            "fixture compatible prior selection restored",
            -1,
        )
        .unwrap();
    assert_eq!(failed.highest_sequence, 2);
    assert_eq!(failed.active.unwrap().release_id, *first.id());
    assert_eq!(failed.trusted_key, next.public());
    assert!(
        store
            .begin(failed.revision, second.encoded(), [3; 16])
            .is_err()
    );
    let later = store
        .begin(failed.revision, signed(3, &next, None).encoded(), [3; 16])
        .unwrap();
    assert_eq!(later.highest_sequence, 3);
    let audit = store.history(None, 100).unwrap();
    assert_eq!(audit.len(), 6);
    assert_eq!(audit[4].action, "failed-operation-recovered");
    assert_eq!(store.history(Some(3), 2).unwrap().len(), 2);
}

#[test]
fn deliberate_old_release_recovery_requires_prior_completion_and_keeps_current_authority() {
    let f = Fixture::new();
    let next = signer(42);
    let first = signed(1, &f.key, Some(next.public()));
    f.install(&first, 1);
    let second = signed(2, &next, None);
    f.install(&second, 2);
    let before = f.store.state().unwrap();
    let evidence = Recovery {
        snapshot_inventory: [7; 32],
        reason: "fixture matching app/data snapshot selected".into(),
        recorded_at: i64::MIN,
    };
    assert!(
        f.store
            .begin_recovery(before.revision, [8; 32], [3; 16], evidence.clone())
            .is_err()
    );
    let pending = f
        .store
        .begin_recovery(before.revision, *first.id(), [3; 16], evidence.clone())
        .unwrap();
    assert!(matches!(
        pending.pending.as_ref().unwrap().purpose,
        Purpose::Recovery { .. }
    ));
    assert_eq!(pending.highest_sequence, 2);
    let recovered = f
        .store
        .complete(pending.revision, [3; 16], *first.id())
        .unwrap();
    assert_eq!(recovered.active.unwrap().release_id, *first.id());
    assert_eq!(recovered.highest_sequence, 2);
    assert_eq!(recovered.trusted_key, next.public());
    assert!(
        f.store
            .begin(
                recovered.revision,
                signed(3, &f.key, None).encoded(),
                [4; 16]
            )
            .is_err()
    );
    assert!(
        f.store
            .begin(
                recovered.revision,
                signed(3, &next, None).encoded(),
                [4; 16]
            )
            .is_ok()
    );
    let audit = f.store.history(None, 100).unwrap();
    assert_eq!(audit[5].action, "begin-recovery");
    assert_eq!(
        audit[5].detail["purpose"]["evidence"]["reason"],
        evidence.reason
    );
    assert!(
        f.store
            .begin_recovery(7, *first.id(), [5; 16], evidence)
            .is_err(),
        "pending normal install fences recovery"
    );
}

#[test]
fn state_receipt_and_audit_commit_atomically_when_history_write_fails() {
    let f = Fixture::new();
    let candidate = signed(9, &f.key, None);
    let original = f.store.state().unwrap();
    let connection = f.connection();
    connection.execute_batch("CREATE TRIGGER fixture_fail_history BEFORE INSERT ON history BEGIN SELECT RAISE(ABORT,'fixture audit I/O failure'); END;").unwrap();
    assert!(f.store.begin(0, candidate.encoded(), [1; 16]).is_err());
    assert_eq!(f.store.state().unwrap(), original);
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM releases", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    connection
        .execute_batch("DROP TRIGGER fixture_fail_history")
        .unwrap();
    let pending = f.store.begin(0, candidate.encoded(), [1; 16]).unwrap();
    connection.execute_batch("CREATE TRIGGER fixture_fail_history BEFORE INSERT ON history BEGIN SELECT RAISE(ABORT,'fixture audit I/O failure'); END;").unwrap();
    assert!(f.store.complete(1, [1; 16], *candidate.id()).is_err());
    assert_eq!(f.store.state().unwrap(), pending);
    assert_eq!(
        connection
            .query_row("SELECT completed FROM releases", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    connection
        .execute_batch("DROP TRIGGER fixture_fail_history")
        .unwrap();
    f.store.complete(1, [1; 16], *candidate.id()).unwrap();
}

#[test]
fn concurrent_operators_admit_one_revision_and_all_others_leave_no_effects() {
    let f = Fixture::new();
    let candidate = Arc::new(signed(1, &f.key, None).encoded().to_vec());
    let barrier = Arc::new(Barrier::new(8));
    let threads = (1..=8)
        .map(|request| {
            let path = f.path.clone();
            let barrier = barrier.clone();
            let candidate = candidate.clone();
            thread::spawn(move || {
                let store = Store::open(&path).unwrap();
                barrier.wait();
                store.begin(0, &candidate, [request; 16]).is_ok()
            })
        })
        .collect::<Vec<_>>();
    let accepted = threads
        .into_iter()
        .map(|t| t.join().unwrap())
        .filter(|accepted| *accepted)
        .count();
    assert_eq!(accepted, 1);
    assert_eq!(f.store.state().unwrap().revision, 1);
    assert_eq!(f.store.history(None, 100).unwrap().len(), 2);
}

#[test]
fn missing_foreign_corrupt_and_existing_control_is_never_reinitialized() {
    let f = Fixture::new();
    assert!(Store::initialize(&f.path, signer(42).public()).is_err());
    assert!(Store::open(&f.root.join("missing")).is_err());
    assert!(!f.root.join("missing").exists());
    assert!(Store::initialize(&f.root.join("weak"), [0; 32]).is_err());
    assert!(!f.root.join("weak").exists());
    let original = f.store.state().unwrap();
    let connection = f.connection();
    let mut changed = original.clone();
    changed.highest_sequence = 5;
    connection
        .execute(
            "UPDATE control SET body=?1",
            [serde_json::to_vec(&changed).unwrap()],
        )
        .unwrap();
    assert!(Store::open(&f.path).is_err());
    connection
        .execute(
            "UPDATE control SET body=?1",
            [serde_json::to_vec(&original).unwrap()],
        )
        .unwrap();
    assert!(Store::open(&f.path).is_ok());
    connection.pragma_update(None, "application_id", 0).unwrap();
    assert!(Store::open(&f.path).is_err());
    assert!(Store::initialize(&f.path, f.key.public()).is_err());
    assert_eq!(
        connection
            .query_row("SELECT body FROM control", [], |r| r.get::<_, Vec<u8>>(0))
            .unwrap(),
        serde_json::to_vec(&original).unwrap()
    );
}

#[test]
fn unsigned_counter_maximum_stale_tickets_unknown_recovery_and_history_bounds_fail_closed() {
    let f = Fixture::new();
    let candidate = signed(u64::MAX, &f.key, None);
    assert!(f.store.begin(0, candidate.encoded(), [0; 16]).is_err());
    let pending = f.store.begin(0, candidate.encoded(), [1; 16]).unwrap();
    assert_eq!(pending.highest_sequence, u64::MAX);
    assert!(f.store.failed(1, [1; 16], "bad\nreason", 0).is_err());
    assert!(f.store.failed(0, [1; 16], "stale", 0).is_err());
    assert!(f.store.failed(1, [2; 16], "wrong ticket", 0).is_err());
    let failed = f
        .store
        .failed(
            1,
            [1; 16],
            "first install failed; no active selection",
            i64::MAX,
        )
        .unwrap();
    assert!(failed.active.is_none());
    assert_eq!(failed.highest_sequence, u64::MAX);
    assert!(
        f.store
            .begin(failed.revision, candidate.encoded(), [2; 16])
            .is_err()
    );
    assert!(
        f.store
            .begin_recovery(
                failed.revision,
                *candidate.id(),
                [2; 16],
                Recovery {
                    snapshot_inventory: [7; 32],
                    reason: "not installed".into(),
                    recorded_at: 0
                }
            )
            .is_err()
    );
    for limit in [0, 101] {
        assert!(f.store.history(None, limit).is_err());
    }
    assert!(f.store.history(Some(u64::MAX), 1).is_err());
    let audit = f.store.history(None, 100).unwrap();
    let body = serde_json::to_vec(&failed).unwrap();
    use sha2::{Digest, Sha256};
    assert_eq!(
        audit.last().unwrap().state_sha256,
        <[u8; 32]>::from(Sha256::digest(body))
    );
}

#[test]
fn process_exit_before_commit_recovers_the_previous_state_and_audit() {
    let f = Fixture::new();
    let candidate = signed(9, &f.key, None);
    let pending = f.store.begin(0, candidate.encoded(), [1; 16]).unwrap();
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("release_trust_crash_child")
        .arg("--nocapture")
        .env("BBSBOX_TRUST_CRASH_FIXTURE", &f.path)
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(71));
    let reopened = Store::open(&f.path).unwrap();
    assert_eq!(reopened.state().unwrap(), pending);
    assert_eq!(reopened.history(None, 100).unwrap().len(), 2);
}

#[test]
fn restoring_an_older_core_snapshot_leaves_external_release_maximum_and_pending_state_intact() {
    let f = Fixture::new();
    let node = f.root.join("node.sqlite");
    let mut database = bbsbox_storage::Database::open(&node, &[]).unwrap();
    let snapshot = f.root.join("old-core-snapshot");
    database.snapshot_core(&snapshot).unwrap();
    drop(database);
    let candidate = signed(9, &f.key, None);
    let pending = f.store.begin(0, candidate.encoded(), [1; 16]).unwrap();
    let restored =
        bbsbox_storage::snapshot::restore_core(&snapshot, f.root.join("restored")).unwrap();
    assert!(matches!(
        bbsbox_storage::writer::Writer::start(
            restored,
            vec![],
            bbsbox_storage::writer::Config::default()
        ),
        Err(bbsbox_storage::Error::RecoveryRequired)
    ));
    assert_eq!(Store::open(&f.path).unwrap().state().unwrap(), pending);
    assert_eq!(pending.highest_sequence, 9);
}

#[test]
fn release_trust_crash_child() {
    let Some(path) = std::env::var_os("BBSBOX_TRUST_CRASH_FIXTURE") else {
        return;
    };
    let connection = Connection::open(PathBuf::from(path).join("release-trust.sqlite")).unwrap();
    connection.execute_batch("PRAGMA cache_size=5; PRAGMA synchronous=EXTRA; BEGIN IMMEDIATE; UPDATE control SET body=zeroblob(65536); DELETE FROM history;").unwrap();
    // Exit without connection/transaction destructors, leaving a real rollback journal.
    std::process::exit(71);
}

#[cfg(unix)]
#[test]
fn private_modes_and_unlinked_control_paths_are_required() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let f = Fixture::new();
    let database = f.path.join("release-trust.sqlite");
    assert_eq!(
        fs::metadata(&f.path).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(&database).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let alias = f.root.join("alias");
    symlink(&f.path, &alias).unwrap();
    assert!(Store::open(&alias).is_err());
    assert!(Store::initialize(&alias.join("nested"), f.key.public()).is_err());
    fs::set_permissions(&database, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(Store::open(&f.path).is_err());
    fs::set_permissions(&database, fs::Permissions::from_mode(0o600)).unwrap();
    fs::set_permissions(&f.path, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(Store::open(&f.path).is_err());
    fs::set_permissions(&f.path, fs::Permissions::from_mode(0o700)).unwrap();
    let moved = f.root.join("database");
    fs::rename(&database, &moved).unwrap();
    symlink(&moved, &database).unwrap();
    assert!(Store::open(&f.path).is_err());
}
