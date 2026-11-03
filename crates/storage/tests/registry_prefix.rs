// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_storage::{
    Database, Error, Migration, applied_registry, disk::Class, open_reader, verify_exact_registry,
};
use std::fs;

fn old() -> Vec<Migration> {
    vec![
        Migration {
            module: "alpha",
            version: 1,
            sql: "CREATE TABLE alpha(value INTEGER NOT NULL); INSERT INTO alpha VALUES(17);",
        },
        Migration {
            module: "beta",
            version: 1,
            sql: "CREATE TABLE beta(value INTEGER NOT NULL);",
        },
    ]
}
fn current() -> Vec<Migration> {
    let mut registry = old();
    registry.push(Migration {
        module: "alpha",
        version: 2,
        sql: "ALTER TABLE alpha ADD COLUMN name TEXT; UPDATE alpha SET name='migrated';",
    });
    registry.push(Migration {
        module: "beta",
        version: 2,
        sql: "INSERT INTO beta VALUES(29);",
    });
    registry
}

#[test]
fn selects_only_applied_compiled_prefixes_without_executing_future_sql_or_changing_snapshot_bytes()
{
    let root = tempfile::tempdir().unwrap();
    let database = root.path().join("node.sqlite");
    let mut source = Database::open(&database, &old()).unwrap();
    source.snapshot_core(root.path().join("snapshot")).unwrap();
    drop(source);
    let snapshot = root.path().join("snapshot/core.sqlite");
    let bytes = fs::read(&snapshot).unwrap();
    let applied = applied_registry(&snapshot, &current()).unwrap();
    assert_eq!(
        applied
            .iter()
            .map(|m| (m.module, m.version, m.sql))
            .collect::<Vec<_>>(),
        old()
            .iter()
            .map(|m| (m.module, m.version, m.sql))
            .collect::<Vec<_>>()
    );
    verify_exact_registry(&snapshot, &applied).unwrap();
    assert!(matches!(
        verify_exact_registry(&snapshot, &current()),
        Err(Error::ChangedOrUnknownMigration)
    ));
    let reader = open_reader(&snapshot).unwrap();
    assert!(reader.prepare("SELECT name FROM alpha").is_err());
    assert_eq!(
        reader
            .query_row("SELECT value FROM alpha", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        17
    );
    drop(reader);
    assert_eq!(fs::read(&snapshot).unwrap(), bytes);
    let migrated = Database::open(&database, &current()).unwrap();
    drop(migrated);
    assert_eq!(applied_registry(&database, &current()).unwrap().len(), 4);
    verify_exact_registry(&database, &current()).unwrap();
    assert!(matches!(
        applied_registry(&database, &old()),
        Err(Error::ChangedOrUnknownMigration)
    ));
    let missing = root.path().join("missing.sqlite");
    assert!(applied_registry(&missing, &current()).is_err());
    assert!(!missing.exists());
}

#[test]
fn unknown_changed_holey_and_oversized_metadata_never_become_a_known_prefix() {
    for sql in [
        "DELETE FROM core_migrations WHERE module='alpha' AND version=1;",
        "UPDATE core_migrations SET digest=zeroblob(32) WHERE module='alpha' AND version=1;",
        "UPDATE core_migrations SET digest=zeroblob(1048576) WHERE module='alpha' AND version=1;",
        "UPDATE core_migrations SET module=printf('%1048576s','x') WHERE module='alpha' AND version=1;",
        "UPDATE core_migrations SET module=module || char(0) || 'unknown' WHERE module='alpha';",
        "UPDATE core_migrations SET module=CAST(x'ff' AS TEXT) WHERE module='alpha' AND version=1;",
        "INSERT INTO core_migrations VALUES('unknown',1,zeroblob(32));",
    ] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("node.sqlite");
        let mut database = Database::open(&path, &current()).unwrap();
        database
            .write(Class::Control, 3 * 1024 * 1024, |tx| {
                tx.execute_batch(sql)?;
                Ok::<_, Error>(())
            })
            .unwrap();
        drop(database);
        let before = fs::read(&path).unwrap();
        assert!(applied_registry(&path, &current()).is_err(), "{sql}");
        assert_eq!(fs::read(&path).unwrap(), before);
    }
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("node.sqlite");
    drop(Database::open(&path, &old()).unwrap());
    let mut changed = current();
    changed[0].sql = "CREATE TABLE alpha(value INTEGER NOT NULL); INSERT INTO alpha VALUES(17); ";
    assert!(matches!(
        applied_registry(&path, &changed),
        Err(Error::ChangedOrUnknownMigration)
    ));
    let duplicate = vec![old()[0].clone(), old()[0].clone()];
    assert!(matches!(
        applied_registry(&path, &duplicate),
        Err(Error::InvalidMigration)
    ));
}
