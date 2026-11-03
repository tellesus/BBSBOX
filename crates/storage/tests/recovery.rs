// SPDX-License-Identifier: AGPL-3.0-or-later
//! Storage exclusion/durability only; M12 supplies the actual authorized recovery decision.
use bbsbox_storage::{
    Database, Error, Migration,
    disk::Class,
    recovery::Recovery,
    snapshot,
    writer::{Config, Writer},
};
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};

fn migrations() -> Vec<Migration> {
    vec![Migration {
        module: "recovery-fixture",
        version: 1,
        sql: "CREATE TABLE fixture_values(value INTEGER NOT NULL);",
    }]
}
struct Fixture {
    _temporary: tempfile::TempDir,
    root: PathBuf,
    source: PathBuf,
    restored: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().canonicalize().unwrap();
        let source = root.join("source.sqlite");
        let mut database = Database::open(&source, &migrations()).unwrap();
        database
            .write(Class::Control, 1024, |tx| {
                tx.execute("INSERT INTO fixture_values VALUES(1)", [])?;
                Ok::<_, Error>(())
            })
            .unwrap();
        let snapshot = root.join("snapshot");
        database.snapshot_core(&snapshot).unwrap();
        drop(database);
        let restored = snapshot::restore_core(snapshot, root.join("restored")).unwrap();
        Self {
            _temporary: temporary,
            root,
            source,
            restored,
        }
    }
    fn marker(&self) -> PathBuf {
        PathBuf::from(format!("{}.recovery-required", self.restored.display()))
    }
    fn decision(&self) -> (PathBuf, [u8; 32]) {
        let path = self.root.join("decision.json");
        let bytes = b"{\"fixture\":\"caller authorization is outside this storage test\"}\n";
        fs::write(&path, bytes).unwrap();
        (path, Sha256::digest(bytes).into())
    }
}

#[test]
fn cold_reconciliation_holds_writer_exclusion_until_the_exact_record_is_durable() {
    let f = Fixture::new();
    let mut recovery = Recovery::open(&f.restored, &migrations()).unwrap();
    assert!(matches!(
        Writer::start(&f.restored, migrations(), Config::default()),
        Err(Error::WriterAlreadyRunning)
    ));
    assert!(matches!(
        Recovery::open(&f.restored, &migrations()),
        Err(Error::WriterAlreadyRunning)
    ));
    recovery
        .database()
        .write(Class::Control, 1024, |tx| {
            tx.execute("INSERT INTO fixture_values VALUES(2)", [])?;
            Ok::<_, Error>(())
        })
        .unwrap();
    let (decision, hash) = f.decision();
    recovery.finish_after_decision(&decision, hash).unwrap();
    assert!(!f.marker().exists());
    let writer = Writer::start(&f.restored, migrations(), Config::default()).unwrap();
    writer.shutdown().unwrap();
    let reader = bbsbox_storage::open_reader(&f.restored).unwrap();
    assert_eq!(
        reader
            .query_row("SELECT sum(value) FROM fixture_values", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        3
    );
    assert!(matches!(
        Recovery::open(&f.restored, &migrations()),
        Err(Error::RecoveryNotPending)
    ));
}

#[test]
fn abandoned_or_changed_decisions_leave_recovered_writers_blocked() {
    let f = Fixture::new();
    drop(Recovery::open(&f.restored, &migrations()).unwrap());
    assert!(f.marker().exists());
    let (decision, hash) = f.decision();
    fs::write(&decision, b"changed").unwrap();
    assert!(matches!(
        Recovery::open(&f.restored, &migrations())
            .unwrap()
            .finish_after_decision(&decision, hash),
        Err(Error::InvalidRecoveryDecision)
    ));
    assert!(f.marker().exists());
    assert!(matches!(
        Writer::start(&f.restored, migrations(), Config::default()),
        Err(Error::RecoveryRequired)
    ));
    assert!(
        Recovery::open(&f.restored, &migrations())
            .unwrap()
            .finish_after_decision(&decision, [0; 32])
            .is_err()
    );
    assert!(f.marker().exists());
}

#[test]
fn no_initialization_or_implicit_schema_upgrade_and_no_decision_aliasing() {
    let f = Fixture::new();
    let missing = f.root.join("missing.sqlite");
    assert!(Recovery::open(&missing, &migrations()).is_err());
    assert!(!missing.exists());
    assert!(matches!(
        Recovery::open(&f.source, &migrations()),
        Err(Error::RecoveryNotPending)
    ));
    let mut changed = migrations();
    changed.push(Migration {
        module: "recovery-fixture",
        version: 2,
        sql: "CREATE TABLE must_not_apply(value INTEGER);",
    });
    assert!(Recovery::open(&f.restored, &changed).is_err());
    assert_eq!(
        bbsbox_storage::open_reader(&f.restored)
            .unwrap()
            .query_row(
                "SELECT count(*) FROM sqlite_schema WHERE name='must_not_apply'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    let hash: [u8; 32] = Sha256::digest(fs::read(f.marker()).unwrap()).into();
    assert!(
        Recovery::open(&f.restored, &migrations())
            .unwrap()
            .finish_after_decision(f.marker(), hash)
            .is_err()
    );
    assert!(f.marker().exists());
}

#[cfg(unix)]
#[test]
fn linked_database_or_decision_is_rejected_without_releasing_the_marker() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let alias = f.root.join("alias.sqlite");
    symlink(&f.restored, &alias).unwrap();
    assert!(Recovery::open(alias, &migrations()).is_err());
    let (decision, hash) = f.decision();
    let linked = f.root.join("linked.json");
    symlink(decision, &linked).unwrap();
    assert!(
        Recovery::open(&f.restored, &migrations())
            .unwrap()
            .finish_after_decision(linked, hash)
            .is_err()
    );
    assert!(f.marker().exists());
}
