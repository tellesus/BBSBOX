// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_storage::{
    Command, Database, Error, Migration,
    clock::Clock,
    jobs::{JobSpec, Retry, enqueue},
    open_reader,
    writer::{Config, Priority, Writer},
};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicI64, Ordering},
        mpsc,
    },
    time::Duration,
};

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
fn migrations() -> Vec<Migration> {
    vec![Migration {
        module: "fixture",
        version: 1,
        sql: "CREATE TABLE fixture_effects(value INTEGER NOT NULL); INSERT INTO fixture_effects VALUES(0);",
    }]
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
fn effects(path: &std::path::Path) -> i64 {
    open_reader(path)
        .unwrap()
        .query_row("SELECT value FROM fixture_effects", [], |r| r.get(0))
        .unwrap()
}
fn finish(tx: &rusqlite::Transaction<'_>) -> bbsbox_storage::Result<Vec<u8>> {
    tx.execute("UPDATE fixture_effects SET value=value+1", [])?;
    Ok(b"finished".to_vec())
}

#[test]
fn bounded_queue_prioritizes_interaction_and_reserves_background_progress() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node.sqlite");
    let writer = Writer::start(
        &path,
        migrations(),
        Config {
            capacity: 6,
            interactive_burst: 2,
        },
    )
    .unwrap();
    let handle = writer.handle();
    let (started, entered) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let first = handle
        .submit(Priority::Interactive, move |_| {
            started.send(()).unwrap();
            resume.recv_timeout(Duration::from_secs(5)).unwrap();
            Ok(())
        })
        .unwrap();
    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    let order = Arc::new(Mutex::new(Vec::new()));
    let mut pending = Vec::new();
    for (priority, label) in [
        (Priority::Background, "b0"),
        (Priority::Background, "b1"),
        (Priority::Interactive, "i0"),
        (Priority::Interactive, "i1"),
        (Priority::Interactive, "i2"),
        (Priority::Interactive, "i3"),
    ] {
        let order = order.clone();
        pending.push(
            handle
                .submit(priority, move |_| {
                    order.lock().unwrap().push(label);
                    Ok(())
                })
                .unwrap(),
        );
    }
    assert!(matches!(
        handle.submit(Priority::Interactive, |_| Ok(())),
        Err(Error::QueueFull)
    ));
    // Dropping a caller's reply must not cancel accepted work.
    drop(pending.pop());
    release.send(()).unwrap();
    first.blocking_wait().unwrap();
    for request in pending {
        request.blocking_wait().unwrap();
    }
    writer.shutdown().unwrap();
    assert_eq!(
        *order.lock().unwrap(),
        vec!["i0", "b0", "i1", "i2", "b1", "i3"]
    );
    assert!(matches!(
        handle.submit(Priority::Background, |_| Ok(())),
        Err(Error::WriterClosed)
    ));
}

#[test]
fn callback_panic_rolls_back_and_reentrant_submission_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node.sqlite");
    let writer = Writer::start(&path, migrations(), Config::default()).unwrap();
    let handle = writer.handle();
    let failed = handle
        .submit(Priority::Interactive, |db| {
            db.transaction(
                Command {
                    namespace: "fixture/user",
                    id: "panic",
                    request: b"increment",
                },
                |tx| {
                    tx.execute("UPDATE fixture_effects SET value=99", [])?;
                    panic!("intentional fixture panic");
                },
            )
        })
        .unwrap();
    assert!(matches!(failed.blocking_wait(), Err(Error::WorkPanicked)));
    assert_eq!(effects(&path), 0);
    let nested = handle.clone();
    handle
        .submit(Priority::Interactive, move |_| {
            assert!(matches!(
                nested.submit(Priority::Interactive, |_| Ok(())),
                Err(Error::ReentrantWriter)
            ));
            Ok(())
        })
        .unwrap()
        .blocking_wait()
        .unwrap();
    handle
        .submit(Priority::Interactive, |db| {
            db.transaction(
                Command {
                    namespace: "fixture/user",
                    id: "panic",
                    request: b"increment",
                },
                finish,
            )
        })
        .unwrap()
        .blocking_wait()
        .unwrap();
    assert_eq!(effects(&path), 1);
    writer.shutdown().unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn async_request_thread_keeps_running_while_writer_is_busy() {
    let dir = tempfile::tempdir().unwrap();
    let writer = Writer::start(
        dir.path().join("node.sqlite"),
        migrations(),
        Config::default(),
    )
    .unwrap();
    let (started, entered) = tokio::sync::oneshot::channel();
    let (release, resume) = mpsc::channel();
    let pending = writer
        .handle()
        .submit(Priority::Background, move |_| {
            let _ = started.send(());
            resume.recv_timeout(Duration::from_secs(5)).unwrap();
            Ok(17)
        })
        .unwrap();
    entered.await.unwrap();
    tokio::time::timeout(
        Duration::from_secs(2),
        tokio::time::sleep(Duration::from_millis(1)),
    )
    .await
    .unwrap();
    release.send(()).unwrap();
    assert_eq!(pending.wait().await.unwrap(), 17);
    writer.shutdown().unwrap();
}

#[test]
fn transactional_outbox_deduplicates_and_rolls_back_with_the_command() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node.sqlite");
    let mut db = Database::open(&path, &migrations()).unwrap();
    let failed = db.transaction(
        Command {
            namespace: "fixture/user",
            id: "outbox",
            request: b"enqueue",
        },
        |tx| {
            enqueue(tx, job("child"))?;
            Err(Error::InvalidJob)
        },
    );
    assert!(failed.is_err());
    assert!(
        db.claim_job("fixture", &["work.v1"], Duration::from_secs(30))
            .unwrap()
            .is_none()
    );
    db.transaction(
        Command {
            namespace: "fixture/user",
            id: "outbox",
            request: b"enqueue",
        },
        |tx| {
            let id = enqueue(tx, job("child"))?;
            Ok(id.to_le_bytes().to_vec())
        },
    )
    .unwrap();
    let id = db.enqueue(job("child")).unwrap();
    assert!(matches!(
        db.enqueue(JobSpec {
            payload: b"different",
            ..job("child")
        }),
        Err(Error::ReusedJobId)
    ));
    let claim = db
        .claim_job("fixture", &["work.v1"], Duration::from_secs(30))
        .unwrap()
        .unwrap();
    assert_eq!(claim.id, id);
    assert_eq!(claim.attempt, 1);
    assert!(
        db.claim_job("fixture", &["work.v1"], Duration::from_secs(30))
            .unwrap()
            .is_none()
    );
    assert!(!db.finish_job(&claim, finish).unwrap().replayed);
    assert!(db.finish_job(&claim, finish).unwrap().replayed);
    assert_eq!(effects(&path), 1);
}

#[test]
fn monotonic_leases_fence_expired_workers_even_when_wall_time_moves_back() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node.sqlite");
    let clock = Arc::new(TestClock::default());
    let mut db = Database::open_with_clock(&path, &migrations(), clock.clone()).unwrap();
    db.enqueue(job("lease")).unwrap();
    let first = db
        .claim_job("fixture", &["work.v1"], Duration::from_millis(10))
        .unwrap()
        .unwrap();
    clock.wall.store(-1_000_000, Ordering::SeqCst);
    clock.tick.store(11, Ordering::SeqCst);
    assert!(matches!(
        db.finish_job(&first, finish),
        Err(Error::StaleClaim)
    ));
    assert_eq!(effects(&path), 0);
    let second = db
        .claim_job("fixture", &["work.v1"], Duration::from_millis(10))
        .unwrap()
        .unwrap();
    assert_eq!(second.attempt, 2);
    assert!(matches!(
        db.renew_claim(&first, Duration::from_secs(1)),
        Err(Error::StaleClaim)
    ));
    db.renew_claim(&second, Duration::from_millis(20)).unwrap();
    clock.tick.store(25, Ordering::SeqCst);
    db.finish_job(&second, finish).unwrap();
    assert_eq!(effects(&path), 1);
}

#[test]
fn work_that_outlives_its_lease_rolls_back_before_commit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node.sqlite");
    let clock = Arc::new(TestClock::default());
    let mut db = Database::open_with_clock(&path, &migrations(), clock.clone()).unwrap();
    db.enqueue(job("timeout")).unwrap();
    let claim = db
        .claim_job("fixture", &["work.v1"], Duration::from_millis(10))
        .unwrap()
        .unwrap();
    let result = db.finish_job(&claim, |tx| {
        let result = finish(tx)?;
        clock.tick.store(11, Ordering::SeqCst);
        Ok(result)
    });
    assert!(matches!(result, Err(Error::StaleClaim)));
    assert_eq!(effects(&path), 0);
    let new = db
        .claim_job("fixture", &["work.v1"], Duration::from_secs(1))
        .unwrap()
        .unwrap();
    db.finish_job(&new, finish).unwrap();
    assert_eq!(effects(&path), 1);
}

#[test]
fn retry_pause_unknown_kinds_and_restart_preserve_job_intent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node.sqlite");
    let clock = Arc::new(TestClock::default());
    let writer =
        Writer::start_with_clock(&path, migrations(), Config::default(), clock.clone()).unwrap();
    let handle = writer.handle();
    let id = handle
        .submit(Priority::Interactive, |db| db.enqueue(job("persist")))
        .unwrap()
        .blocking_wait()
        .unwrap();
    let claim = handle
        .submit(Priority::Background, |db| {
            assert!(
                db.claim_job("fixture", &["unsupported.v2"], Duration::from_secs(1))?
                    .is_none()
            );
            Ok(db
                .claim_job("fixture", &["work.v1"], Duration::from_secs(1))?
                .unwrap())
        })
        .unwrap()
        .blocking_wait()
        .unwrap();
    let old = claim.clone();
    handle
        .submit(Priority::Background, move |db| {
            db.retry_job(&claim, "peer_unavailable", Retry::At(100))
        })
        .unwrap()
        .blocking_wait()
        .unwrap();
    handle
        .submit(Priority::Background, |db| {
            assert!(
                db.claim_job("fixture", &["work.v1"], Duration::from_secs(1))?
                    .is_none()
            );
            Ok(())
        })
        .unwrap()
        .blocking_wait()
        .unwrap();
    clock.wall.store(100, Ordering::SeqCst);
    let claim = handle
        .submit(Priority::Background, |db| {
            Ok(db
                .claim_job("fixture", &["work.v1"], Duration::from_secs(1))?
                .unwrap())
        })
        .unwrap()
        .blocking_wait()
        .unwrap();
    handle
        .submit(Priority::Background, move |db| {
            db.retry_job(&claim, "dependency_missing", Retry::Manual)
        })
        .unwrap()
        .blocking_wait()
        .unwrap();
    writer.shutdown().unwrap();
    let writer = Writer::start_with_clock(&path, migrations(), Config::default(), clock).unwrap();
    let handle = writer.handle();
    let status = handle
        .submit(Priority::Interactive, move |db| db.job_status(id))
        .unwrap()
        .blocking_wait()
        .unwrap()
        .unwrap();
    assert_eq!(status.state, "paused");
    assert_eq!(status.error_code.as_deref(), Some("dependency_missing"));
    handle
        .submit(Priority::Interactive, move |db| {
            assert!(db.resume_job(id, None)?);
            Ok(())
        })
        .unwrap()
        .blocking_wait()
        .unwrap();
    let current = handle
        .submit(Priority::Background, |db| {
            Ok(db
                .claim_job("fixture", &["work.v1"], Duration::from_secs(1))?
                .unwrap())
        })
        .unwrap()
        .blocking_wait()
        .unwrap();
    handle
        .submit(Priority::Background, move |db| {
            assert!(matches!(
                db.finish_job(&old, finish),
                Err(Error::StaleClaim)
            ));
            db.finish_job(&current, finish)
        })
        .unwrap()
        .blocking_wait()
        .unwrap();
    assert_eq!(effects(&path), 1);
    writer.shutdown().unwrap();
}

#[test]
fn single_writer_lock_prevents_live_claim_reset_and_restart_fences_old_claim() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node.sqlite");
    let writer = Writer::start(&path, migrations(), Config::default()).unwrap();
    let handle = writer.handle();
    handle
        .submit(Priority::Interactive, |db| db.enqueue(job("restart")))
        .unwrap()
        .blocking_wait()
        .unwrap();
    let first = handle
        .submit(Priority::Background, |db| {
            Ok(db
                .claim_job("fixture", &["work.v1"], Duration::from_secs(60))?
                .unwrap())
        })
        .unwrap()
        .blocking_wait()
        .unwrap();
    assert!(matches!(
        Writer::start(&path, migrations(), Config::default()),
        Err(Error::WriterAlreadyRunning)
    ));
    writer.shutdown().unwrap();
    let writer = Writer::start(&path, migrations(), Config::default()).unwrap();
    let handle = writer.handle();
    handle
        .submit(Priority::Background, move |db| {
            assert!(matches!(
                db.finish_job(&first, finish),
                Err(Error::StaleClaim)
            ));
            let current = db
                .claim_job("fixture", &["work.v1"], Duration::from_secs(60))?
                .unwrap();
            assert_eq!(current.attempt, 2);
            db.finish_job(&current, finish)
        })
        .unwrap()
        .blocking_wait()
        .unwrap();
    assert_eq!(effects(&path), 1);
    writer.shutdown().unwrap();
}

#[test]
fn schema_one_upgrades_atomically_and_keeps_existing_command_receipts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node.sqlite");
    let mut db = Database::open(&path, &migrations()).unwrap();
    db.transaction(
        Command {
            namespace: "fixture/user",
            id: "old",
            request: b"increment",
        },
        finish,
    )
    .unwrap();
    drop(db);
    let raw = rusqlite::Connection::open(&path).unwrap();
    raw.execute_batch("DROP TABLE core_jobs; PRAGMA user_version=1;")
        .unwrap();
    drop(raw);
    let mut db = Database::open(&path, &migrations()).unwrap();
    assert!(
        db.transaction(
            Command {
                namespace: "fixture/user",
                id: "old",
                request: b"increment"
            },
            finish
        )
        .unwrap()
        .replayed
    );
    db.enqueue(job("upgraded")).unwrap();
    assert_eq!(effects(&path), 1);
}

#[test]
fn process_death_during_job_completion_recovers_without_duplicate_effects() {
    const CHILD: &str = "BBSBOX_JOB_CRASH_PATH";
    if let Some(path) = std::env::var_os(CHILD) {
        let writer = Writer::start(path, migrations(), Config::default()).unwrap();
        writer
            .handle()
            .submit::<()>(Priority::Background, |db| {
                db.enqueue(job("crash"))?;
                let claim = db
                    .claim_job("fixture", &["work.v1"], Duration::from_secs(60))?
                    .unwrap();
                db.finish_job(&claim, |tx| {
                    let response = finish(tx)?;
                    if std::env::var("BBSBOX_JOB_CRASH_POINT").unwrap() == "before" {
                        std::process::exit(53);
                    }
                    Ok(response)
                })?;
                std::process::exit(54);
            })
            .unwrap()
            .blocking_wait()
            .unwrap();
        unreachable!();
    }
    for (point, code) in [("before", 53), ("after", 54)] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("node.sqlite");
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "process_death_during_job_completion_recovers_without_duplicate_effects",
            ])
            .env(CHILD, &path)
            .env("BBSBOX_JOB_CRASH_POINT", point)
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(code));
        let writer = Writer::start(&path, migrations(), Config::default()).unwrap();
        writer
            .handle()
            .submit(Priority::Background, move |db| {
                let claim = db.claim_job("fixture", &["work.v1"], Duration::from_secs(60))?;
                if point == "before" {
                    let claim = claim.unwrap();
                    assert_eq!(claim.attempt, 2);
                    db.finish_job(&claim, finish)?;
                } else {
                    assert!(claim.is_none());
                }
                Ok(())
            })
            .unwrap()
            .blocking_wait()
            .unwrap();
        assert_eq!(effects(&path), 1);
        writer.shutdown().unwrap();
    }
}
