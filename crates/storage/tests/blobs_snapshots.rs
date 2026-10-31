// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_storage::{
    Command, Database, Error, Migration,
    blobs::{remove_reference, set_reference},
    disk::{Budget, Class, SpaceProbe},
    jobs::{JobSpec, Retry},
    open_reader, snapshot,
    writer::{Config, Writer},
};
use std::{
    io::{self, Read},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

fn migrations() -> Vec<Migration> {
    vec![Migration {
        module: "fixture",
        version: 1,
        sql: "CREATE TABLE fixture_values(value INTEGER);",
    }]
}
fn command(id: &str) -> Command<'_> {
    Command {
        namespace: "fixture/user",
        id,
        request: b"test",
    }
}
fn job(key: &str) -> JobSpec<'_> {
    JobSpec {
        queue: "fixture",
        key,
        kind: "work.v1",
        payload: b"payload",
        ready_at: None,
    }
}
fn count(path: &Path, table: &str) -> i64 {
    open_reader(path)
        .unwrap()
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
fn read(mut file: std::fs::File) -> Vec<u8> {
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    bytes
}
#[test]
fn blobs_deduplicate_and_only_explicitly_released_references_can_be_collected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node.sqlite");
    let mut db = Database::open(&path, &migrations()).unwrap();
    let first = db
        .put_blob("uploads/user", "draft", &b"local file"[..])
        .unwrap();
    let same = db
        .put_blob("pending_export", "lump", &b"local file"[..])
        .unwrap();
    assert_eq!(first, same);
    assert_eq!(count(&path, "core_blobs"), 1);
    assert!(
        db.transaction(command("fail"), |tx| {
            remove_reference(tx, "uploads/user", "draft")?;
            Err(Error::InvalidBlob)
        })
        .is_err()
    );
    assert_eq!(db.collect_blobs(100).unwrap(), 0);
    db.transaction(command("consume"), |tx| {
        set_reference(tx, "files", "entry", &first.digest)?;
        remove_reference(tx, "uploads/user", "draft")?;
        Ok(b"ok".to_vec())
    })
    .unwrap();
    db.transaction(command("delete"), |tx| {
        remove_reference(tx, "files", "entry")?;
        Ok(vec![])
    })
    .unwrap();
    assert_eq!(db.collect_blobs(100).unwrap(), 0); // Unacknowledged export still owns a reference.
    assert_eq!(read(db.get_blob(&first.digest).unwrap()), b"local file");
    db.transaction(command("ack"), |tx| {
        remove_reference(tx, "pending_export", "lump")?;
        Ok(vec![])
    })
    .unwrap();
    assert_eq!(db.collect_blobs(100).unwrap(), 1);
    assert!(matches!(
        db.get_blob(&first.digest),
        Err(Error::InvalidBlob)
    ));
    assert!(matches!(
        db.get_blob("../node.sqlite"),
        Err(Error::InvalidBlob)
    ));
}
struct FailingReader(bool);
impl Read for FailingReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.0 {
            return Err(io::Error::other("disconnected upload"));
        }
        self.0 = true;
        buffer[0] = 1;
        Ok(1)
    }
}
#[test]
fn failed_oversized_or_corrupt_uploads_never_publish_a_bad_reference() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node.sqlite");
    let mut db = Database::open(&path, &migrations()).unwrap();
    db.configure_disk(
        Budget {
            max_blob_bytes: 4,
            ..Budget::default()
        },
        Arc::new(bbsbox_storage::disk::FilesystemSpace),
    )
    .unwrap();
    assert!(matches!(
        db.put_blob("uploads", "large", &b"12345"[..]),
        Err(Error::BlobTooLarge)
    ));
    assert!(matches!(
        db.put_blob("uploads", "failed", FailingReader(false)),
        Err(Error::Io(_))
    ));
    assert_eq!(count(&path, "core_blobs"), 0);
    let info = db.put_blob("uploads", "valid", &b"1234"[..]).unwrap();
    let data = dir.path().join("node.sqlite.blobs/data").join(&info.digest);
    std::fs::write(&data, b"bad").unwrap();
    assert!(matches!(db.get_blob(&info.digest), Err(Error::CorruptBlob)));
    assert!(matches!(
        db.put_blob("uploads", "another", &b"1234"[..]),
        Err(Error::CorruptBlob)
    ));
    assert_eq!(count(&path, "core_blob_refs"), 1);
}
#[test]
fn finalized_bytes_with_failed_metadata_are_recoverable_orphans() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node.sqlite");
    let mut db = Database::open(&path, &migrations()).unwrap();
    let raw = rusqlite::Connection::open(&path).unwrap();
    raw.execute_batch("CREATE TRIGGER fixture_fail_blob BEFORE INSERT ON core_blobs BEGIN SELECT RAISE(ABORT,'fixture failure'); END;").unwrap();
    assert!(matches!(
        db.put_blob("uploads", "failed", &b"finalized"[..]),
        Err(Error::Sqlite(_))
    ));
    assert_eq!(count(&path, "core_blobs"), 0);
    assert_eq!(
        std::fs::read_dir(dir.path().join("node.sqlite.blobs/data"))
            .unwrap()
            .count(),
        1
    );
    assert_eq!(db.recover_blob_orphans(1).unwrap(), 1);
    raw.execute_batch("DROP TRIGGER fixture_fail_blob").unwrap();
    let info = db.put_blob("uploads", "ok", &b"finalized"[..]).unwrap();
    assert_eq!(db.recover_blob_orphans(100).unwrap(), 0);
    assert_eq!(read(db.get_blob(&info.digest).unwrap()), b"finalized");
}
#[test]
fn process_death_during_stream_leaves_only_a_cleanable_staging_file() {
    const ENV: &str = "BBSBOX_BLOB_CRASH_PATH";
    if let Some(path) = std::env::var_os(ENV) {
        struct Crash(bool);
        impl Read for Crash {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                if self.0 {
                    std::process::exit(61);
                }
                self.0 = true;
                buf[..4].copy_from_slice(b"data");
                Ok(4)
            }
        }
        let mut db = Database::open(path, &migrations()).unwrap();
        db.put_blob("uploads", "crash", Crash(false)).unwrap();
        unreachable!();
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node.sqlite");
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "process_death_during_stream_leaves_only_a_cleanable_staging_file",
        ])
        .env(ENV, &path)
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(61));
    let mut db = Database::open(&path, &migrations()).unwrap();
    assert_eq!(count(&path, "core_blob_refs"), 0);
    assert_eq!(db.recover_blob_orphans(100).unwrap(), 1);
    db.put_blob("uploads", "retry", &b"data"[..]).unwrap();
    assert_eq!(count(&path, "core_blobs"), 1);
}
struct Probe(AtomicU64);
impl SpaceProbe for Probe {
    fn available(&self, _: &Path) -> io::Result<u64> {
        Ok(self.0.load(Ordering::SeqCst))
    }
}
#[test]
fn disk_floors_preserve_control_room_and_return_receipts_under_pressure() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node.sqlite");
    let mut db = Database::open(&path, &migrations()).unwrap();
    let probe = Arc::new(Probe(AtomicU64::new(512 * 1024)));
    db.configure_disk(
        Budget {
            content_floor: 1024 * 1024,
            control_floor: 64 * 1024,
            max_blob_bytes: 1024,
        },
        probe.clone(),
    )
    .unwrap();
    assert!(matches!(
        db.transaction(command("content"), |_| Ok(vec![])),
        Err(Error::DiskPressure { .. })
    ));
    assert!(matches!(
        db.put_blob("uploads", "denied", &b"x"[..]),
        Err(Error::DiskPressure { .. })
    ));
    db.transaction_with_budget(command("control"), Class::Control, 65536, |tx| {
        tx.execute("INSERT INTO fixture_values VALUES(1)", [])?;
        Ok(b"committed".to_vec())
    })
    .unwrap();
    let id = db.enqueue(job("retry")).unwrap();
    let claim = db
        .claim_job("fixture", &["work.v1"], Duration::from_secs(60))
        .unwrap()
        .unwrap();
    db.retry_job(&claim, "dependency_missing", Retry::Manual)
        .unwrap();
    assert_eq!(db.job_status(id).unwrap().unwrap().state, "paused");
    probe.0.store(0, Ordering::SeqCst);
    assert!(
        db.transaction(command("control"), |_| panic!("must replay"))
            .unwrap()
            .replayed
    );
    assert!(matches!(
        db.resume_job(id, None),
        Err(Error::DiskPressure { .. })
    ));
    probe.0.store(2 * 1024 * 1024, Ordering::SeqCst);
    assert!(matches!(
        db.transaction(command("runs_out"), |tx| {
            tx.execute("INSERT INTO fixture_values VALUES(2)", [])?;
            probe.0.store(0, Ordering::SeqCst);
            Ok(vec![])
        }),
        Err(Error::DiskPressure { .. })
    ));
    assert_eq!(count(&path, "fixture_values"), 1);
    assert_eq!(count(&path, "core_command_receipts"), 1);
}
#[test]
fn live_wal_snapshot_restores_blobs_receipts_jobs_and_keeps_recovery_required() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node.sqlite");
    let mut db = Database::open(&path, &migrations()).unwrap();
    let info = db.put_blob("files", "kept", &b"original file"[..]).unwrap();
    db.transaction(command("before"), |tx| {
        tx.execute("INSERT INTO fixture_values VALUES(1)", [])?;
        Ok(b"receipt".to_vec())
    })
    .unwrap();
    let id = db.enqueue(job("paused")).unwrap();
    let claim = db
        .claim_job("fixture", &["work.v1"], Duration::from_secs(60))
        .unwrap()
        .unwrap();
    db.retry_job(&claim, "missing_dependency", Retry::Manual)
        .unwrap();
    let snapshot_path = dir.path().join("snapshot");
    let manifest = db.snapshot_core(&snapshot_path).unwrap();
    assert_eq!(manifest.scope, "core-only");
    assert_eq!(manifest.blobs, vec![info.clone()]);
    snapshot::verify_core(&snapshot_path).unwrap();
    db.transaction(command("later"), |tx| {
        tx.execute("INSERT INTO fixture_values VALUES(2)", [])?;
        remove_reference(tx, "files", "kept")?;
        Ok(vec![])
    })
    .unwrap();
    assert_eq!(db.collect_blobs(100).unwrap(), 1);
    let restored = snapshot::restore_core(&snapshot_path, dir.path().join("restored")).unwrap();
    let mut recovered = Database::open(&restored, &migrations()).unwrap();
    assert_eq!(count(&restored, "fixture_values"), 1);
    assert!(
        recovered
            .transaction(command("before"), |_| panic!("must replay"))
            .unwrap()
            .replayed
    );
    assert_eq!(
        read(recovered.get_blob(&info.digest).unwrap()),
        b"original file"
    );
    assert_eq!(recovered.job_status(id).unwrap().unwrap().state, "paused");
    assert!(matches!(
        Writer::start(&restored, migrations(), Config::default()),
        Err(Error::RecoveryRequired)
    ));
    assert!(db.snapshot_core(&snapshot_path).is_err());
    assert!(snapshot::restore_core(&snapshot_path, dir.path().join("restored")).is_err());
}
#[test]
fn incomplete_corrupt_and_path_escape_manifests_cannot_restore() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path().join("node.sqlite"), &migrations()).unwrap();
    let info = db.put_blob("files", "kept", &b"original"[..]).unwrap();
    let path = dir.path().join("snapshot");
    db.snapshot_core(&path).unwrap();
    let manifest = std::fs::read(path.join("manifest.json")).unwrap();
    std::fs::remove_file(path.join("manifest.json")).unwrap();
    assert!(snapshot::verify_core(&path).is_err());
    std::fs::write(path.join("manifest.json"), &manifest).unwrap();
    let mut bad: serde_json::Value = serde_json::from_slice(&manifest).unwrap();
    bad["blobs"][0]["digest"] = "../outside".into();
    std::fs::write(
        path.join("manifest.json"),
        serde_json::to_vec(&bad).unwrap(),
    )
    .unwrap();
    assert!(snapshot::restore_core(&path, dir.path().join("escape")).is_err());
    assert!(!dir.path().join("escape").exists());
    std::fs::write(path.join("manifest.json"), &manifest).unwrap();
    std::fs::write(path.join("blobs").join(&info.digest), b"bad").unwrap();
    assert!(snapshot::restore_core(&path, dir.path().join("corrupt")).is_err());
    assert!(!dir.path().join("corrupt").exists());
    std::fs::write(path.join("blobs").join(&info.digest), b"original").unwrap();
    std::fs::write(path.join("core.sqlite"), b"bad").unwrap();
    assert!(snapshot::verify_core(&path).is_err());
}

#[test]
fn schema_two_upgrade_preserves_jobs_and_rolls_back_when_module_registry_is_wrong() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node.sqlite");
    let mut db = Database::open(&path, &migrations()).unwrap();
    let id = db.enqueue(job("old")).unwrap();
    drop(db);
    let raw = rusqlite::Connection::open(&path).unwrap();
    raw.execute_batch("DROP TABLE core_blob_refs; DROP TABLE core_blobs; PRAGMA user_version=2;")
        .unwrap();
    assert!(matches!(
        Database::open(&path, &[]),
        Err(Error::ChangedOrUnknownMigration)
    ));
    assert_eq!(
        raw.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        raw.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name='core_blobs'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    drop(raw);
    let mut db = Database::open(&path, &migrations()).unwrap();
    assert_eq!(db.job_status(id).unwrap().unwrap().state, "ready");
    db.put_blob("uploads", "new", &b"data"[..]).unwrap();
}

#[cfg(unix)]
#[test]
fn database_alias_uses_the_same_blob_root_and_snapshot_permissions_are_private() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node.sqlite");
    let mut db = Database::open(&path, &migrations()).unwrap();
    let info = db.put_blob("files", "entry", &b"data"[..]).unwrap();
    drop(db);
    let alias = dir.path().join("alias.sqlite");
    symlink(&path, &alias).unwrap();
    let mut db = Database::open(&alias, &migrations()).unwrap();
    assert_eq!(read(db.get_blob(&info.digest).unwrap()), b"data");
    let snapshot = dir.path().join("snapshot");
    db.snapshot_core(&snapshot).unwrap();
    assert_eq!(
        std::fs::metadata(&snapshot).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(snapshot.join("core.sqlite"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::metadata(snapshot.join("blobs").join(info.digest))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}
