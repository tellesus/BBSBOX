// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_storage::{Command, Database, Error, Migration, open_reader};
use std::{
    path::Path,
    process::{Command as Process, Stdio},
};

fn migrations() -> [Migration; 1] {
    [Migration {
        module: "fixture",
        version: 1,
        sql: "CREATE TABLE fixture_counter(value INTEGER NOT NULL); INSERT INTO fixture_counter VALUES(0);",
    }]
}
fn increment(
    db: &mut Database,
    scope: &str,
    id: &str,
    request: &[u8],
) -> bbsbox_storage::Result<bbsbox_storage::Receipt> {
    db.transaction(
        Command {
            namespace: scope,
            id,
            request,
        },
        |tx| {
            tx.execute("UPDATE fixture_counter SET value=value+1", [])?;
            let value: i64 = tx.query_row("SELECT value FROM fixture_counter", [], |r| r.get(0))?;
            Ok(value.to_le_bytes().to_vec())
        },
    )
}
fn count(path: &Path) -> i64 {
    open_reader(path)
        .unwrap()
        .query_row("SELECT value FROM fixture_counter", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn durable_receipts_deduplicate_and_scope_commands_after_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("node.sqlite");
    let mut db = Database::open(&path, &migrations()).unwrap();
    let first = increment(&mut db, "fixture/alice", "request-1", b"increment:1").unwrap();
    assert!(!first.replayed);
    drop(db);
    let mut db = Database::open(&path, &migrations()).unwrap();
    let again = increment(&mut db, "fixture/alice", "request-1", b"increment:1").unwrap();
    assert!(again.replayed);
    assert_eq!(first.bytes, again.bytes);
    assert_eq!(count(&path), 1);
    assert!(matches!(
        increment(&mut db, "fixture/alice", "request-1", b"changed"),
        Err(Error::ReusedCommandId)
    ));
    increment(&mut db, "fixture/bob", "request-1", b"increment:1").unwrap();
    assert_eq!(count(&path), 2);
}

#[test]
fn failed_work_rolls_back_state_and_receipt_and_readers_keep_progressing() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("node.sqlite");
    let mut db = Database::open(&path, &migrations()).unwrap();
    let reader = open_reader(&path).unwrap();
    assert_eq!(
        reader
            .pragma_query_value(None, "journal_mode", |r| r.get::<_, String>(0))
            .unwrap(),
        "wal"
    );
    assert!(
        reader
            .execute("UPDATE fixture_counter SET value=99", [])
            .is_err()
    );
    let result = db.transaction(
        Command {
            namespace: "fixture/alice",
            id: "retry",
            request: b"increment:1",
        },
        |tx| {
            tx.execute("UPDATE fixture_counter SET value=1", [])?;
            // A separate WAL reader sees committed state while a write is in progress.
            assert_eq!(
                reader.query_row("SELECT value FROM fixture_counter", [], |r| r
                    .get::<_, i64>(0))?,
                0
            );
            Err(Error::ReceiptTooLarge)
        },
    );
    assert!(result.is_err());
    assert_eq!(count(&path), 0);
    increment(&mut db, "fixture/alice", "retry", b"increment:1").unwrap();
    assert_eq!(count(&path), 1);
}

#[test]
fn invalid_migrations_and_unknown_schemas_fail_without_partial_module_changes() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("node.sqlite");
    let bad = [
        Migration {
            module: "fixture",
            version: 1,
            sql: "CREATE TABLE fixture_counter(value INTEGER);",
        },
        Migration {
            module: "fixture",
            version: 2,
            sql: "INSERT INTO nonexistent_table VALUES(1);",
        },
    ];
    assert!(Database::open(&path, &bad).is_err());
    assert_eq!(
        rusqlite::Connection::open(&path)
            .unwrap()
            .query_row(
                "SELECT count(*) FROM sqlite_schema WHERE type='table'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    drop(Database::open(&path, &migrations()).unwrap());
    let changed = [Migration {
        module: "fixture",
        version: 1,
        sql: "CREATE TABLE different_table(value INTEGER);",
    }];
    assert!(matches!(
        Database::open(&path, &changed),
        Err(Error::ChangedOrUnknownMigration)
    ));
    assert!(matches!(
        Database::open(&path, &[]),
        Err(Error::ChangedOrUnknownMigration)
    ));
    let db = rusqlite::Connection::open(&path).unwrap();
    db.pragma_update(None, "user_version", 99).unwrap();
    drop(db);
    assert!(matches!(
        Database::open(&path, &migrations()),
        Err(Error::UnsupportedSchema(99))
    ));
}

#[test]
fn foreign_databases_and_gaps_in_migration_registry_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("foreign.sqlite");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute("CREATE TABLE unrelated(value TEXT)", [])
        .unwrap();
    drop(db);
    assert!(matches!(
        Database::open(&path, &migrations()),
        Err(Error::ForeignDatabase)
    ));
    let gap = [Migration {
        module: "fixture",
        version: 2,
        sql: "SELECT 1;",
    }];
    assert!(matches!(
        Database::open(temp.path().join("gap.sqlite"), &gap),
        Err(Error::InvalidMigration)
    ));
}

#[test]
fn sqlite_full_and_oversized_responses_roll_back_without_filling_the_disk() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("node.sqlite");
    let mut db = Database::open(&path, &migrations()).unwrap();
    let result = db.transaction(
        Command {
            namespace: "fixture/alice",
            id: "large-response",
            request: b"large",
        },
        |tx| {
            tx.execute("UPDATE fixture_counter SET value=1", [])?;
            Ok(vec![0; 1024 * 1024 + 1])
        },
    );
    assert!(matches!(result, Err(Error::ReceiptTooLarge)));
    assert_eq!(count(&path), 0);
    // Use SQLite's per-connection page limit, not real host disk exhaustion.
    let result=db.transaction(Command{namespace:"fixture/alice",id:"disk-full",request:b"grow"},|tx|{
        let pages:i64=tx.pragma_query_value(None,"page_count",|r|r.get(0))?;
        tx.pragma_update(None,"max_page_count",pages)?;
        tx.execute("UPDATE fixture_counter SET value=1",[])?;
        tx.execute("INSERT INTO core_command_receipts VALUES('fixture/test','large',zeroblob(32),zeroblob(4194304))",[])?;
        Ok(vec![])
    });
    assert!(
        matches!(result,Err(Error::Sqlite(rusqlite::Error::SqliteFailure(error,_))) if error.code==rusqlite::ErrorCode::DiskFull)
    );
    assert_eq!(count(&path), 0);
    drop(db);
    let mut db = Database::open(&path, &migrations()).unwrap();
    assert!(increment(&mut db, "fixture/alice", "disk-full", b"grow").is_ok());
    assert_eq!(count(&path), 1);
}

#[test]
fn crash_before_and_after_commit_gives_one_effect_when_retried() {
    if let Ok(path) = std::env::var("BBSBOX_STORAGE_CRASH_DB") {
        let mut db = Database::open(path, &migrations()).unwrap();
        let before = std::env::var("BBSBOX_STORAGE_CRASH_PHASE").unwrap() == "before";
        db.transaction(
            Command {
                namespace: "fixture/alice",
                id: "crash",
                request: b"increment:1",
            },
            |tx| {
                tx.execute("UPDATE fixture_counter SET value=value+1", [])?;
                if before {
                    std::process::exit(23);
                }
                Ok(b"committed".to_vec())
            },
        )
        .unwrap();
        std::process::exit(24);
    }
    for phase in ["before", "after"] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("node.sqlite");
        let status = Process::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "crash_before_and_after_commit_gives_one_effect_when_retried",
                "--nocapture",
            ])
            .env("BBSBOX_STORAGE_CRASH_DB", &path)
            .env("BBSBOX_STORAGE_CRASH_PHASE", phase)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(if phase == "before" { 23 } else { 24 }));
        assert_eq!(count(&path), if phase == "before" { 0 } else { 1 });
        let mut db = Database::open(&path, &migrations()).unwrap();
        let receipt = increment(&mut db, "fixture/alice", "crash", b"increment:1").unwrap();
        assert_eq!(receipt.replayed, phase == "after");
        assert_eq!(count(&path), 1);
    }
}
