// SPDX-License-Identifier: AGPL-3.0-or-later
//! Real independent worker SQLite state; no game host/HTTP/activation capability is implied.
use bbsbox_identity::node::NodeSigner;
use bbsbox_maintenance::{
    BackupIdentity, Error, Limits, Result, archive,
    checkpoints::{self, Registration, WorkerArtifact, WorkerCheckpoint},
};
use bbsbox_storage::{Command, Database};
use rusqlite::{Connection, params};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[derive(Default)]
struct State {
    paused: Option<[u8; 16]>,
    resumes: usize,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Failure {
    None,
    Pause,
    Checkpoint,
    Stale,
    Outside,
    Resume,
    PanicResume,
}
struct Worker {
    root: PathBuf,
    db: Connection,
    state: Arc<Mutex<State>>,
    failure: Failure,
}
impl Worker {
    fn new(parent: &Path, id: &str) -> Self {
        let root = parent.join(id);
        fs::create_dir(&root).unwrap();
        let db = Connection::open(root.join("live.sqlite")).unwrap();
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; CREATE TABLE commands(id TEXT PRIMARY KEY,amount INTEGER NOT NULL); CREATE TABLE stats(total INTEGER NOT NULL); INSERT INTO stats VALUES(0);").unwrap();
        Self {
            root,
            db,
            state: Arc::default(),
            failure: Failure::None,
        }
    }
    fn reward(&mut self, id: &str, amount: i64) -> bool {
        assert!(self.state.lock().unwrap().paused.is_none());
        reward(&mut self.db, id, amount)
    }
}
fn reward(db: &mut Connection, id: &str, amount: i64) -> bool {
    let tx = db.transaction().unwrap();
    let changed = tx
        .execute(
            "INSERT OR IGNORE INTO commands VALUES(?1,?2)",
            params![id, amount],
        )
        .unwrap()
        == 1;
    if changed {
        tx.execute("UPDATE stats SET total=total+?1", [amount])
            .unwrap();
    }
    tx.commit().unwrap();
    changed
}
impl WorkerCheckpoint for Worker {
    fn pause(&mut self, request: [u8; 16]) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        if state.paused.is_some() {
            return Err(Error::CheckpointUnavailable);
        }
        state.paused = Some(request);
        if self.failure == Failure::Pause {
            return Err(Error::CheckpointUnavailable);
        }
        Ok(())
    }
    fn checkpoint(&mut self, request: [u8; 16]) -> Result<WorkerArtifact> {
        assert_eq!(self.state.lock().unwrap().paused, Some(request));
        if self.failure == Failure::Checkpoint {
            return Err(Error::CheckpointUnavailable);
        }
        let id = hex::encode(request);
        let directory = self.root.join(format!("checkpoint-{id}"));
        fs::create_dir(&directory)?;
        self.db
            .backup(
                rusqlite::MAIN_DB,
                directory.join("state.sqlite"),
                None::<fn(rusqlite::backup::Progress)>,
            )
            .unwrap();
        let copy = Connection::open(directory.join("state.sqlite")).unwrap();
        copy.pragma_update(None, "journal_mode", "DELETE").unwrap();
        drop(copy);
        fs::OpenOptions::new()
            .write(true)
            .open(directory.join("state.sqlite"))?
            .sync_all()?;
        Ok(WorkerArtifact {
            request: if self.failure == Failure::Stale {
                [0; 16]
            } else {
                request
            },
            checkpoint_id: id,
            state_schema: 1,
            worker_version: "fixture-1".into(),
            directory: if self.failure == Failure::Outside {
                self.root.parent().unwrap().to_path_buf()
            } else {
                directory
            },
        })
    }
    fn resume(&mut self, request: [u8; 16]) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        state.resumes += 1;
        if self.failure == Failure::PanicResume {
            drop(state);
            panic!("fixture resume panic");
        }
        if self.failure == Failure::Resume {
            return Err(Error::CheckpointUnavailable);
        }
        if state.paused == Some(request) {
            state.paused = None;
        }
        Ok(())
    }
}

#[test]
fn independent_worker_checkpoints_keep_pending_core_outbox_and_worker_dedup_consistent() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let mut a = Worker::new(root, "demo-a");
    let mut b = Worker::new(root, "demo-b");
    assert!(a.reward("reward-one", 10));
    assert!(b.reward("reward-two", 20));
    let mut core =
        Database::open(root.join("node.sqlite"), &bbsbox_identity::migrations()).unwrap();
    let signer = NodeSigner::start(&mut core).unwrap();
    let expected = signer.id();
    core.transaction(
        Command {
            namespace: "fixture/reward",
            id: "reward-one",
            request: b"10",
        },
        |tx| {
            bbsbox_storage::jobs::enqueue(
                tx,
                bbsbox_storage::jobs::JobSpec {
                    queue: "fixture/reward",
                    key: "reward-one",
                    kind: "game.reward",
                    payload: b"10",
                    ready_at: Some(0),
                },
            )?;
            Ok(b"pending delivery".to_vec())
        },
    )
    .unwrap();
    drop(signer);
    drop(core);
    let writer = bbsbox_storage::writer::Writer::start(
        root.join("node.sqlite"),
        bbsbox_identity::migrations(),
        bbsbox_storage::writer::Config::default(),
    )
    .unwrap();
    let state_a = a.state.clone();
    let state_b = b.state.clone();
    let gate_a = state_a.clone();
    let gate_b = state_b.clone();
    let root_a = a.root.clone();
    let root_b = b.root.clone();
    let stage = root.join("stage");
    fs::create_dir(&stage).unwrap();
    let cipher = root.join("combined.age");
    let output = cipher.clone();
    let identity = BackupIdentity::generate();
    let recipient = identity.to_public();
    let records = writer
        .handle()
        .maintenance_service(move |core| {
            let signer = NodeSigner::start_existing(core, expected)?;
            checkpoints::with_checkpoints(
                &["demo-a".into(), "demo-b".into()],
                &mut [
                    Registration {
                        game_id: "demo-b",
                        state_root: &root_b,
                        worker: &mut b,
                    },
                    Registration {
                        game_id: "demo-a",
                        state_root: &root_a,
                        worker: &mut a,
                    },
                ],
                &stage.join("games"),
                Limits::default(),
                |records| {
                    assert_eq!(gate_a.lock().unwrap().paused, Some(records[0].request));
                    assert_eq!(gate_b.lock().unwrap().paused, Some(records[0].request));
                    assert_eq!(
                        records
                            .iter()
                            .map(|r| r.game_id.as_str())
                            .collect::<Vec<_>>(),
                        ["demo-a", "demo-b"]
                    );
                    core.snapshot_core(stage.join("core"))?;
                    let mut manifest = fs::File::create(stage.join("worker-checkpoints.json"))?;
                    use std::io::Write;
                    manifest.write_all(&serde_json::to_vec(records).unwrap())?;
                    manifest.sync_all()?;
                    archive::seal(&stage, &output, &signer, &recipient, 0, Limits::default())?;
                    assert!(output.is_file());
                    Ok(records.to_vec())
                },
            )
        })
        .unwrap()
        .blocking_wait()
        .unwrap();
    writer.shutdown().unwrap();
    assert!(state_a.lock().unwrap().paused.is_none());
    assert!(state_b.lock().unwrap().paused.is_none());
    assert_eq!(state_a.lock().unwrap().resumes, 1);
    assert_eq!(state_b.lock().unwrap().resumes, 1);
    let opened = archive::open(
        &cipher,
        &root.join("opened"),
        expected,
        &identity,
        Limits::default(),
    )
    .unwrap();
    let restored = bbsbox_storage::snapshot::restore_core(
        opened.content().join("core"),
        root.join("restore-core"),
    )
    .unwrap();
    assert_eq!(
        bbsbox_storage::open_reader(&restored)
            .unwrap()
            .query_row("SELECT state FROM core_jobs", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ready"
    );
    let restored_game = root.join("restored-game.sqlite");
    fs::copy(
        opened.content().join("games/demo-a/state.sqlite"),
        &restored_game,
    )
    .unwrap();
    let mut db = Connection::open(restored_game).unwrap();
    assert!(!reward(&mut db, "reward-one", 10));
    assert_eq!(
        db.query_row("SELECT total FROM stats", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        10
    );
    assert_eq!(
        serde_json::from_slice::<Vec<checkpoints::CheckpointRecord>>(
            &fs::read(opened.content().join("worker-checkpoints.json")).unwrap()
        )
        .unwrap(),
        records
    );
    assert!(matches!(
        bbsbox_storage::writer::Writer::start(
            restored,
            bbsbox_identity::migrations(),
            bbsbox_storage::writer::Config::default()
        ),
        Err(bbsbox_storage::Error::RecoveryRequired)
    ));
}

#[test]
fn missing_duplicate_or_unsafe_registry_cannot_pause_or_publish_partial_game_sets() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let mut worker = Worker::new(root, "demo");
    let state = worker.state.clone();
    let state_root = worker.root.clone();
    let target = root.join("games");
    let mut called = false;
    let result = checkpoints::with_checkpoints(
        &["demo".into(), "missing".into()],
        &mut [Registration {
            game_id: "demo",
            state_root: &state_root,
            worker: &mut worker,
        }],
        &target,
        Limits::default(),
        |_| {
            called = true;
            Ok(())
        },
    );
    assert!(matches!(result, Err(Error::CheckpointUnavailable)));
    assert!(!called);
    assert!(!target.exists());
    let result = checkpoints::with_checkpoints(
        &["demo".into(), "demo".into()],
        &mut [],
        &target,
        Limits::default(),
        |_| Ok(()),
    );
    assert!(result.is_err());
    let result = checkpoints::with_checkpoints(
        &["../escape".into()],
        &mut [Registration {
            game_id: "../escape",
            state_root: &state_root,
            worker: &mut worker,
        }],
        &target,
        Limits::default(),
        |_| Ok(()),
    );
    assert!(result.is_err());
    assert!(state.lock().unwrap().paused.is_none());
    assert_eq!(state.lock().unwrap().resumes, 0);
}

#[test]
fn duplicate_worker_claims_and_aggregate_budget_never_publish_an_incomplete_set() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let mut a = Worker::new(root, "a");
    let mut b = Worker::new(root, "b");
    let (root_a, root_b) = (a.root.clone(), b.root.clone());
    let (state_a, state_b) = (a.state.clone(), b.state.clone());
    let result = checkpoints::with_checkpoints(
        &["a".into(), "b".into()],
        &mut [
            Registration {
                game_id: "a",
                state_root: &root_a,
                worker: &mut a,
            },
            Registration {
                game_id: "a",
                state_root: &root_b,
                worker: &mut b,
            },
        ],
        &root.join("duplicates"),
        Limits::default(),
        |_| -> Result<()> { panic!("duplicate registration cannot publish") },
    );
    assert!(matches!(result, Err(Error::CheckpointUnavailable)));
    assert_eq!(state_a.lock().unwrap().resumes, 0);
    assert_eq!(state_b.lock().unwrap().resumes, 0);
    let result = checkpoints::with_checkpoints(
        &["a".into(), "b".into()],
        &mut [
            Registration {
                game_id: "a",
                state_root: &root_a,
                worker: &mut a,
            },
            Registration {
                game_id: "b",
                state_root: &root_b,
                worker: &mut b,
            },
        ],
        &root.join("limited"),
        Limits {
            max_files: 1,
            ..Limits::default()
        },
        |_| -> Result<()> { panic!("an over-budget set cannot publish") },
    );
    assert!(matches!(result, Err(Error::LimitExceeded)));
    assert!(state_a.lock().unwrap().paused.is_none());
    assert!(state_b.lock().unwrap().paused.is_none());
    assert_eq!(state_a.lock().unwrap().resumes, 1);
    assert_eq!(state_b.lock().unwrap().resumes, 1);
}

#[test]
fn partial_pause_bad_checkpoint_and_publication_error_resume_every_attempted_worker() {
    for failure in [
        Failure::Pause,
        Failure::Checkpoint,
        Failure::Stale,
        Failure::Outside,
        Failure::None,
    ] {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let mut a = Worker::new(root, "a");
        let mut b = Worker::new(root, "b");
        b.failure = failure;
        let (state_a, state_b) = (a.state.clone(), b.state.clone());
        let (root_a, root_b) = (a.root.clone(), b.root.clone());
        let mut called = false;
        let result = checkpoints::with_checkpoints(
            &["a".into(), "b".into()],
            &mut [
                Registration {
                    game_id: "a",
                    state_root: &root_a,
                    worker: &mut a,
                },
                Registration {
                    game_id: "b",
                    state_root: &root_b,
                    worker: &mut b,
                },
            ],
            &root.join("games"),
            Limits::default(),
            |_| {
                called = true;
                Err::<(), _>(Error::Encryption)
            },
        );
        assert!(result.is_err());
        assert_eq!(called, failure == Failure::None);
        assert!(state_a.lock().unwrap().paused.is_none());
        assert!(state_b.lock().unwrap().paused.is_none());
        assert_eq!(state_a.lock().unwrap().resumes, 1);
        assert_eq!(state_b.lock().unwrap().resumes, 1);
    }
}

#[test]
fn publication_panic_and_one_failed_resume_do_not_skip_other_workers() {
    for failure in [Failure::None, Failure::Resume, Failure::PanicResume] {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let mut a = Worker::new(root, "a");
        let mut b = Worker::new(root, "b");
        b.failure = failure;
        let (state_a, state_b) = (a.state.clone(), b.state.clone());
        let (root_a, root_b) = (a.root.clone(), b.root.clone());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            checkpoints::with_checkpoints(
                &["a".into(), "b".into()],
                &mut [
                    Registration {
                        game_id: "a",
                        state_root: &root_a,
                        worker: &mut a,
                    },
                    Registration {
                        game_id: "b",
                        state_root: &root_b,
                        worker: &mut b,
                    },
                ],
                &root.join("games"),
                Limits::default(),
                |_| {
                    if failure == Failure::None {
                        panic!("fixture publisher panic");
                    }
                    Ok(())
                },
            )
        }));
        if failure == Failure::None {
            assert!(matches!(
                result.unwrap(),
                Err(Error::Storage(bbsbox_storage::Error::WorkPanicked))
            ));
        } else {
            match result.unwrap() {
                Err(Error::CheckpointResumeFailed { request, games }) => {
                    assert_ne!(request, [0; 16]);
                    assert_eq!(games, ["b"]);
                }
                _ => panic!("resume failure must return its request and worker IDs"),
            }
        }
        assert!(state_a.lock().unwrap().paused.is_none());
        assert_eq!(state_a.lock().unwrap().resumes, 1);
        assert_eq!(state_b.lock().unwrap().resumes, 1);
    }
}
