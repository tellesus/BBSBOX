// SPDX-License-Identifier: AGPL-3.0-or-later
#[path = "support/worker.rs"]
mod worker;
use bbsbox_identity::node::NodeSigner;
use bbsbox_maintenance::{
    BackupIdentity, Error, Limits, Result, archive,
    checkpoints::{CheckpointRecord, Registration},
    game_snapshot::{self, Capture, Requirement},
    snapshot,
};
use bbsbox_storage::{
    Command, Database,
    writer::{Config, Writer},
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use worker::{State, Worker, reward};

struct Fixture {
    _directory: tempfile::TempDir,
    root: PathBuf,
    configuration: PathBuf,
    database: PathBuf,
    node: bbsbox_credentials::NodeId,
    key: BackupIdentity,
    games: Vec<Requirement>,
    workers: Vec<Worker>,
    states: Vec<Arc<Mutex<State>>>,
    blob: String,
}
impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().to_path_buf();
        let database = root.join("node.sqlite");
        let mut db = Database::open(&database, &bbsbox_identity::migrations()).unwrap();
        let node = NodeSigner::start(&mut db).unwrap().id();
        db.transaction(
            Command {
                namespace: "fixture/reward",
                id: "pending",
                request: b"10",
            },
            |tx| {
                bbsbox_storage::jobs::enqueue(
                    tx,
                    bbsbox_storage::jobs::JobSpec {
                        queue: "fixture/reward",
                        key: "pending",
                        kind: "game.reward",
                        payload: b"10",
                        ready_at: Some(0),
                    },
                )?;
                Ok(b"receipt".to_vec())
            },
        )
        .unwrap();
        let blob = db
            .put_blob("fixture", "asset", &b"retained node asset"[..])
            .unwrap()
            .digest;
        drop(db);
        let configuration = root.join("configuration");
        fs::create_dir(&configuration).unwrap();
        fs::create_dir(configuration.join("tls")).unwrap();
        for name in Self::required() {
            fs::write(
                configuration.join(name),
                b"configuration fixture; no TLS acceptance claim",
            )
            .unwrap();
        }
        let mut workers = vec![Worker::new(&root, "demo-a"), Worker::new(&root, "demo-b")];
        assert!(workers[0].reward("pending", 10));
        assert!(workers[1].reward("other", 20));
        let states = workers.iter().map(|worker| worker.state.clone()).collect();
        let games = ["demo-a", "demo-b"]
            .map(|id| Requirement {
                game_id: id.into(),
                worker_version: "fixture-1".into(),
                state_schema: 1,
            })
            .into();
        Self {
            _directory: directory,
            root,
            configuration,
            database,
            node,
            key: BackupIdentity::generate(),
            games,
            workers,
            states,
            blob,
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
    fn capture(&mut self) -> Result<PathBuf> {
        let writer = Writer::start(
            &self.database,
            bbsbox_identity::migrations(),
            Config::default(),
        )?;
        let mut workers = std::mem::take(&mut self.workers);
        let roots = workers
            .iter()
            .map(|worker| worker.root.clone())
            .collect::<Vec<_>>();
        let ids = roots
            .iter()
            .map(|root| root.file_name().unwrap().to_str().unwrap().to_string())
            .collect::<Vec<_>>();
        let games = self.games.clone();
        let configuration = self.configuration.clone();
        let node = self.node;
        let stage = self.root.join("stage");
        let output = self.root.join("backup.age");
        let destination = output.clone();
        let recipient = self.key.to_public();
        let pending = writer.handle().maintenance_service(move |database| {
            let mut registrations = workers
                .iter_mut()
                .zip(roots.iter())
                .zip(ids.iter())
                .map(|((worker, root), id)| Registration {
                    game_id: id,
                    state_root: root,
                    worker,
                })
                .collect::<Vec<_>>();
            let migrations = bbsbox_identity::migrations();
            game_snapshot::capture_and_seal(
                database,
                Capture {
                    expected_node: node,
                    configuration: &configuration,
                    required_configuration: Self::required(),
                    created_at: 42,
                    migrations: &migrations,
                    games: &games,
                    limits: Limits::default(),
                },
                &mut registrations,
                &stage,
                &destination,
                &recipient,
            )
        })?;
        let result = pending.blocking_wait();
        let shutdown = writer.shutdown();
        result?;
        shutdown?;
        Ok(output)
    }
    fn open(&self, cipher: &Path, name: &str) -> archive::OpenedArchive {
        archive::open(
            cipher,
            &self.root.join(name),
            self.node,
            &self.key,
            Limits::default(),
        )
        .unwrap()
    }
    fn reseal(&self, source: &Path, name: &str) -> PathBuf {
        let mut database = Database::open(&self.database, &bbsbox_identity::migrations()).unwrap();
        let signer = NodeSigner::start_existing(&mut database, self.node).unwrap();
        let output = self.root.join(name);
        archive::seal(
            source,
            &output,
            &signer,
            &self.key.to_public(),
            42,
            Limits::default(),
        )
        .unwrap();
        output
    }
}

#[test]
fn game_enabled_snapshot_restores_every_checkpoint_core_configuration_and_recovery_block() {
    let mut f = Fixture::new();
    let cipher = f.capture().unwrap();
    for state in &f.states {
        assert!(state.lock().unwrap().paused.is_none());
        assert_eq!(state.lock().unwrap().resumes, 1);
    }
    let opened = f.open(&cipher, "opened");
    let (descriptor, records) = game_snapshot::verify(
        &opened.content(),
        opened.inventory(),
        &bbsbox_identity::migrations(),
        &f.games,
    )
    .unwrap();
    assert_eq!(descriptor.format, "bbsbox.node-snapshot/v1");
    assert_eq!(descriptor.game_checkpoints, ["demo-a", "demo-b"]);
    assert_eq!(records.len(), 2);
    let target = f.root.join("restore");
    let restored =
        game_snapshot::restore(&opened, &target, &bbsbox_identity::migrations(), &f.games).unwrap();
    assert_eq!(restored.games(), records);
    assert!(!target.join(".restore-incomplete").exists());
    assert_eq!(
        fs::read(target.join("configuration/tls/ca-key.pem")).unwrap(),
        fs::read(f.configuration.join("tls/ca-key.pem")).unwrap()
    );
    assert!(matches!(
        Writer::start(
            restored.node().database(),
            bbsbox_identity::migrations(),
            Config::default()
        ),
        Err(bbsbox_storage::Error::RecoveryRequired)
    ));
    assert_eq!(
        NodeSigner::inspect_existing(restored.node().database()).unwrap(),
        f.node
    );
    let database =
        Database::open(restored.node().database(), &bbsbox_identity::migrations()).unwrap();
    let mut bytes = vec![];
    use std::io::Read;
    database
        .get_blob(&f.blob)
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(bytes, b"retained node asset");
    assert!(database.recovery_required().unwrap());
    drop(database);
    assert_eq!(
        bbsbox_storage::open_reader(restored.node().database())
            .unwrap()
            .query_row("SELECT state FROM core_jobs", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ready"
    );
    let mut worker = rusqlite::Connection::open(target.join("games/demo-a/state.sqlite")).unwrap();
    assert!(!reward(&mut worker, "pending", 10));
    assert_eq!(
        worker
            .query_row("SELECT total FROM stats", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        10
    );
    let worker = rusqlite::Connection::open(target.join("games/demo-b/state.sqlite")).unwrap();
    assert_eq!(
        worker
            .query_row("SELECT total FROM stats", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        20
    );
    let recovery: serde_json::Value =
        serde_json::from_slice(&fs::read(target.join("game-recovery.json")).unwrap()).unwrap();
    assert_eq!(recovery["state"], "recovery-required");
    assert!(
        snapshot::restore(
            &opened,
            &f.root.join("foundation-cannot-ignore-games"),
            &bbsbox_identity::migrations()
        )
        .is_err()
    );
}

#[test]
fn installed_package_set_version_and_schema_must_match_before_restore_creates_output() {
    let mut f = Fixture::new();
    let cipher = f.capture().unwrap();
    let opened = f.open(&cipher, "opened");
    let mut wrong_version = f.games.clone();
    wrong_version[0].worker_version = "fixture-2".into();
    let mut wrong_schema = f.games.clone();
    wrong_schema[0].state_schema = 2;
    let mut extra = f.games.clone();
    extra.push(Requirement {
        game_id: "missing".into(),
        worker_version: "fixture-1".into(),
        state_schema: 1,
    });
    for (index, games) in [
        vec![],
        f.games[..1].to_vec(),
        wrong_version,
        wrong_schema,
        extra,
    ]
    .iter()
    .enumerate()
    {
        let target = f.root.join(format!("wrong-{index}"));
        let error = game_snapshot::restore(&opened, &target, &bbsbox_identity::migrations(), games)
            .err()
            .unwrap();
        assert!(matches!(
            error,
            Error::CheckpointUnavailable | Error::UnsupportedCheckpoint { .. }
        ));
        assert!(!target.exists());
    }
}

#[test]
fn missing_worker_and_mismatched_live_worker_cannot_publish_a_complete_game_backup() {
    let mut f = Fixture::new();
    f.workers.pop();
    assert!(matches!(f.capture(), Err(Error::CheckpointUnavailable)));
    assert!(!f.root.join("backup.age").exists());
    for state in &f.states {
        assert!(state.lock().unwrap().paused.is_none());
        assert_eq!(state.lock().unwrap().resumes, 0);
    }
    let mut f = Fixture::new();
    f.games[0].worker_version = "wrong-package".into();
    assert!(matches!(
        f.capture(),
        Err(Error::UnsupportedCheckpoint { .. })
    ));
    assert!(!f.root.join("backup.age").exists());
    for state in &f.states {
        assert!(state.lock().unwrap().paused.is_none());
        assert_eq!(state.lock().unwrap().resumes, 1);
    }
}

#[test]
fn valid_node_signature_cannot_hide_invalid_checkpoint_catalogue_or_unregistered_members() {
    let mut f = Fixture::new();
    let cipher = f.capture().unwrap();
    for variant in 0..6 {
        let opened = f.open(&cipher, &format!("source-{variant}"));
        let content = opened.content();
        let path = content.join("game-checkpoints.json");
        let mut records: Vec<CheckpointRecord> =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        match variant {
            0 => records[1].request[0] ^= 1,
            1 => records[0].files[0].sha256 = "0".repeat(64),
            2 => records[1] = records[0].clone(),
            3 => records[0].files[0].path = "../outside".into(),
            4 => {
                fs::create_dir(content.join("games/unregistered")).unwrap();
                fs::write(
                    content.join("games/unregistered/extra"),
                    b"must not be silently ignored",
                )
                .unwrap();
            }
            5 => {}
            _ => unreachable!(),
        }
        fs::write(
            &path,
            if variant == 5 {
                serde_json::to_vec_pretty(&records).unwrap()
            } else {
                serde_json::to_vec(&records).unwrap()
            },
        )
        .unwrap();
        let malformed = f.reseal(&content, &format!("bad-{variant}.age"));
        let reopened = f.open(&malformed, &format!("bad-open-{variant}"));
        let target = f.root.join(format!("rejected-{variant}"));
        assert!(
            game_snapshot::restore(&reopened, &target, &bbsbox_identity::migrations(), &f.games)
                .is_err()
        );
        assert!(!target.exists());
    }
}

#[test]
fn authenticated_oversized_catalogue_is_bounded_before_restore_output() {
    let mut f = Fixture::new();
    let cipher = f.capture().unwrap();
    let opened = f.open(&cipher, "source");
    let content = opened.content();
    let mut bytes = vec![b' '; 8 * 1024 * 1024 + 1];
    bytes[..2].copy_from_slice(b"[]");
    fs::write(content.join("game-checkpoints.json"), bytes).unwrap();
    let cipher = f.reseal(&content, "oversized.age");
    let reopened = f.open(&cipher, "oversized-open");
    let target = f.root.join("rejected-large");
    assert!(matches!(
        game_snapshot::restore(&reopened, &target, &bbsbox_identity::migrations(), &f.games),
        Err(Error::LimitExceeded)
    ));
    assert!(!target.exists());
}
