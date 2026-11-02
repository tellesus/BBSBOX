// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_identity::node::NodeSigner;
use bbsbox_maintenance::{BackupIdentity, Error, Limits, archive, snapshot};
use bbsbox_storage::{
    Command, Database, Migration,
    writer::{Config, Writer},
};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn migrations() -> Vec<Migration> {
    let mut migrations = bbsbox_identity::migrations();
    migrations.push(Migration {
        module: "fixture",
        version: 1,
        sql: "CREATE TABLE fixture_data(value TEXT NOT NULL);",
    });
    migrations
}
struct Fixture {
    _directory: tempfile::TempDir,
    root: PathBuf,
    database: PathBuf,
    configuration: PathBuf,
    expected: bbsbox_credentials::NodeId,
    key: BackupIdentity,
}
impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().to_path_buf();
        let database = root.join("node.sqlite");
        let mut db = Database::open(&database, &migrations()).unwrap();
        let expected = NodeSigner::start(&mut db).unwrap().id();
        drop(db);
        let configuration = root.join("source-config");
        fs::create_dir(&configuration).unwrap();
        fs::create_dir(configuration.join("tls")).unwrap();
        for name in Self::required() {
            fs::write(configuration.join(name), b"private fixture configuration").unwrap();
        }
        fs::write(configuration.join(".tls-renewal.lock"), []).unwrap();
        Self {
            _directory: directory,
            root,
            database,
            configuration,
            expected,
            key: BackupIdentity::generate(),
        }
    }
    fn required() -> Vec<String> {
        [
            "installation.json",
            "node.json",
            "tls/ca-key.pem",
            "tls/ca.pem",
            "tls/server-key.pem",
            "tls/server.pem",
        ]
        .map(String::from)
        .into()
    }
    fn capture(&self, path: &Path) {
        let writer = Writer::start(&self.database, migrations(), Config::default()).unwrap();
        let configuration = self.configuration.clone();
        let stage = path.to_owned();
        let expected = self.expected;
        writer
            .handle()
            .maintenance_service(move |db| {
                snapshot::capture(
                    db,
                    expected,
                    &configuration,
                    Self::required(),
                    &stage,
                    42,
                    &migrations(),
                )
            })
            .unwrap()
            .blocking_wait()
            .unwrap();
        writer.shutdown().unwrap();
    }
    fn seal(&self, stage: &Path, path: &Path) {
        let mut db = Database::open(&self.database, &migrations()).unwrap();
        let signer = NodeSigner::start_existing(&mut db, self.expected).unwrap();
        archive::seal(
            stage,
            path,
            &signer,
            &self.key.to_public(),
            42,
            Limits::default(),
        )
        .unwrap();
    }
}

#[test]
fn coordinated_snapshot_keeps_core_receipts_jobs_blobs_configuration_and_recovery_block() {
    let f = Fixture::new();
    let writer = Writer::start(&f.database, migrations(), Config::default()).unwrap();
    writer
        .handle()
        .submit(bbsbox_storage::writer::Priority::Interactive, |db| {
            db.transaction(
                Command {
                    namespace: "fixture/write",
                    id: "before",
                    request: b"retained",
                },
                |tx| {
                    tx.execute("INSERT INTO fixture_data VALUES('retained')", [])?;
                    bbsbox_storage::jobs::enqueue(
                        tx,
                        bbsbox_storage::jobs::JobSpec {
                            queue: "fixture/job",
                            key: "before",
                            kind: "fixture.work",
                            payload: b"job",
                            ready_at: Some(0),
                        },
                    )?;
                    Ok(b"receipt".to_vec())
                },
            )?;
            db.put_blob("fixture", "kept", &b"node blob"[..])?;
            Ok(())
        })
        .unwrap()
        .blocking_wait()
        .unwrap();
    let stage = f.root.join("stage");
    let target = stage.clone();
    let configuration = f.configuration.clone();
    let expected = f.expected;
    let signed = writer
        .handle()
        .maintenance_service(move |db| {
            snapshot::capture(
                db,
                expected,
                &configuration,
                Fixture::required(),
                &target,
                42,
                &migrations(),
            )
        })
        .unwrap()
        .blocking_wait()
        .unwrap();
    assert!(
        signed
            .inventory()
            .files
            .iter()
            .all(|file| !file.path.ends_with(".tls-renewal.lock"))
    );
    let descriptor = snapshot::verify(&stage, &signed, &migrations()).unwrap();
    assert_eq!(descriptor.configuration.len(), 6);
    writer.shutdown().unwrap();
    let cipher = f.root.join("backup.age");
    f.seal(&stage, &cipher);
    let opened = archive::open(
        &cipher,
        &f.root.join("opened"),
        f.expected,
        &f.key,
        Limits::default(),
    )
    .unwrap();
    let restored = snapshot::restore(&opened, &f.root.join("isolated"), &migrations()).unwrap();
    assert!(!restored.directory().join(".restore-incomplete").exists());
    assert_eq!(
        fs::read(restored.configuration().join("tls/ca-key.pem")).unwrap(),
        fs::read(f.configuration.join("tls/ca-key.pem")).unwrap()
    );
    assert_eq!(
        NodeSigner::inspect_existing(restored.database()).unwrap(),
        f.expected
    );
    let reader = bbsbox_storage::open_reader(restored.database()).unwrap();
    assert_eq!(
        reader
            .query_row("SELECT value FROM fixture_data", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        "retained"
    );
    assert_eq!(
        reader
            .query_row(
                "SELECT response FROM core_command_receipts WHERE namespace='fixture/write'",
                [],
                |r| r.get::<_, Vec<u8>>(0)
            )
            .unwrap(),
        b"receipt"
    );
    assert_eq!(
        reader
            .query_row("SELECT count(*) FROM core_jobs", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        reader
            .query_row("SELECT count(*) FROM core_blobs", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    drop(reader);
    assert!(matches!(
        Writer::start(restored.database(), migrations(), Config::default()),
        Err(bbsbox_storage::Error::RecoveryRequired)
    ));
    let mut database = Database::open(restored.database(), &migrations()).unwrap();
    assert!(matches!(
        NodeSigner::start_existing(&mut database, f.expected),
        Err(bbsbox_identity::Error::Storage(
            bbsbox_storage::Error::RecoveryRequired
        ))
    ));
    let recovery: serde_json::Value =
        serde_json::from_slice(&fs::read(restored.directory().join("recovery.json")).unwrap())
            .unwrap();
    assert_eq!(recovery["state"], "recovery-required");
    assert_eq!(recovery["post_snapshot_writes"], "not-reconciled");
}

#[test]
fn wrong_missing_or_in_progress_configuration_cannot_publish_a_snapshot() {
    let f = Fixture::new();
    let mut database = Database::open(&f.database, &migrations()).unwrap();
    let other = Fixture::new();
    let target = f.root.join("wrong");
    assert!(
        snapshot::capture(
            &mut database,
            other.expected,
            &f.configuration,
            Fixture::required(),
            &target,
            42,
            &migrations()
        )
        .is_err()
    );
    assert!(!target.exists());
    assert_eq!(
        NodeSigner::inspect_existing(&f.database).unwrap(),
        f.expected
    );
    fs::remove_file(f.configuration.join("tls/ca-key.pem")).unwrap();
    assert!(
        snapshot::capture(
            &mut database,
            f.expected,
            &f.configuration,
            Fixture::required(),
            &target,
            42,
            &migrations()
        )
        .is_err()
    );
    assert!(!target.exists());
    fs::write(f.configuration.join("tls/ca-key.pem"), b"fixture").unwrap();
    fs::write(
        f.configuration.join(".network-prepare-incomplete"),
        b"incomplete",
    )
    .unwrap();
    assert!(
        snapshot::capture(
            &mut database,
            f.expected,
            &f.configuration,
            Fixture::required(),
            &target,
            42,
            &migrations()
        )
        .is_err()
    );
    assert!(!target.exists());
    fs::remove_file(f.configuration.join(".network-prepare-incomplete")).unwrap();
    let inside = f.configuration.join("snapshot");
    assert!(
        snapshot::capture(
            &mut database,
            f.expected,
            &f.configuration,
            Fixture::required(),
            &inside,
            42,
            &migrations()
        )
        .is_err()
    );
    assert!(!inside.exists());
}

#[test]
fn isolated_restore_refuses_registry_mismatch_tampering_and_existing_output() {
    let f = Fixture::new();
    let stage = f.root.join("stage");
    f.capture(&stage);
    let cipher = f.root.join("backup.age");
    f.seal(&stage, &cipher);
    let opened = archive::open(
        &cipher,
        &f.root.join("opened"),
        f.expected,
        &f.key,
        Limits::default(),
    )
    .unwrap();
    let target = f.root.join("target");
    assert!(snapshot::restore(&opened, &target, &bbsbox_identity::migrations()).is_err());
    assert!(!target.exists());
    fs::create_dir(&target).unwrap();
    fs::write(target.join("kept"), b"original").unwrap();
    assert!(snapshot::restore(&opened, &target, &migrations()).is_err());
    assert_eq!(fs::read(target.join("kept")).unwrap(), b"original");
    fs::write(
        opened.content().join("configuration/node.json"),
        b"changed after archive verification",
    )
    .unwrap();
    let target = f.root.join("tampered");
    assert!(matches!(
        snapshot::restore(&opened, &target, &migrations()),
        Err(Error::ChangedSource)
    ));
    assert!(!target.exists());
}

#[test]
fn signed_but_incomplete_or_foreign_core_is_not_a_valid_node_snapshot() {
    let f = Fixture::new();
    let other = Fixture::new();
    let stage = other.root.join("stage");
    other.capture(&stage);
    let descriptor_path = stage.join("snapshot.json");
    let mut descriptor: snapshot::Descriptor =
        serde_json::from_slice(&fs::read(&descriptor_path).unwrap()).unwrap();
    descriptor.node = f.expected.to_string();
    fs::write(&descriptor_path, serde_json::to_vec(&descriptor).unwrap()).unwrap();
    let cipher = f.root.join("foreign.age");
    f.seal(&stage, &cipher);
    let opened = archive::open(
        &cipher,
        &f.root.join("foreign"),
        f.expected,
        &f.key,
        Limits::default(),
    )
    .unwrap();
    assert!(matches!(
        snapshot::restore(&opened, &f.root.join("foreign-restore"), &migrations()),
        Err(Error::WrongNode)
    ));
    let stage = f.root.join("incomplete-stage");
    f.capture(&stage);
    let path = stage.join("snapshot.json");
    let mut descriptor: snapshot::Descriptor =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    descriptor
        .game_checkpoints
        .push("uncoordinated-game".into());
    fs::write(path, serde_json::to_vec(&descriptor).unwrap()).unwrap();
    let cipher = f.root.join("incomplete.age");
    f.seal(&stage, &cipher);
    let opened = archive::open(
        &cipher,
        &f.root.join("incomplete"),
        f.expected,
        &f.key,
        Limits::default(),
    )
    .unwrap();
    assert!(matches!(
        snapshot::restore(&opened, &f.root.join("incomplete-restore"), &migrations()),
        Err(Error::IncompleteSnapshot)
    ));
}
