// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use crate::{Command, open_reader, snapshot};
use std::{sync::Barrier, time::Duration};

fn migrations() -> Vec<Migration> {
    vec![Migration {
        module: "fixture",
        version: 1,
        sql: "CREATE TABLE fixture_effects(label TEXT NOT NULL);",
    }]
}
fn effect(database: &mut Database, label: &str) -> Result<()> {
    database.transaction(
        Command {
            namespace: "fixture/member",
            id: label,
            request: label.as_bytes(),
        },
        |tx| {
            tx.execute("INSERT INTO fixture_effects VALUES(?1)", [label])?;
            Ok(b"done".to_vec())
        },
    )?;
    Ok(())
}
fn count(path: &Path) -> i64 {
    open_reader(path)
        .unwrap()
        .query_row("SELECT count(*) FROM fixture_effects", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn maintenance_reserves_capacity_drains_both_priorities_and_snapshots_all_prior_effects() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("node.sqlite");
    let writer = Writer::start(
        &path,
        migrations(),
        Config {
            capacity: 2,
            interactive_burst: 1,
            ..Config::default()
        },
    )
    .unwrap();
    let handle = writer.handle();
    let (started, entered) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let first = handle
        .submit(Priority::Interactive, move |database| {
            started.send(()).unwrap();
            resume.recv_timeout(Duration::from_secs(5)).unwrap();
            effect(database, "executing")
        })
        .unwrap();
    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    let background = handle
        .submit(Priority::Background, |database| {
            effect(database, "background")
        })
        .unwrap();
    let interactive = handle
        .submit(Priority::Interactive, |database| {
            effect(database, "interactive")
        })
        .unwrap();
    assert!(matches!(
        handle.submit(Priority::Interactive, |_| Ok(())),
        Err(Error::QueueFull)
    ));
    let snapshot = directory.path().join("checkpoint");
    let destination = snapshot.clone();
    let nested = handle.clone();
    let checkpoint = handle
        .maintenance(move |database| {
            assert!(matches!(
                nested.submit(Priority::Interactive, |_| Ok(())),
                Err(Error::ReentrantWriter)
            ));
            database.snapshot_core(destination)
        })
        .unwrap();
    for priority in [Priority::Interactive, Priority::Background] {
        assert!(matches!(
            handle.submit(priority, |_| Ok(())),
            Err(Error::MaintenancePaused)
        ));
    }
    assert!(matches!(
        handle.maintenance(|_| Ok(())),
        Err(Error::MaintenancePaused)
    ));
    release.send(()).unwrap();
    checkpoint.blocking_wait().unwrap();
    first.blocking_wait().unwrap();
    background.blocking_wait().unwrap();
    interactive.blocking_wait().unwrap();
    snapshot::verify_core(&snapshot).unwrap();
    assert_eq!(count(&snapshot.join("core.sqlite")), 3);
    handle
        .submit(Priority::Interactive, |database| effect(database, "after"))
        .unwrap()
        .blocking_wait()
        .unwrap();
    assert_eq!(count(&path), 4);
    assert_eq!(count(&snapshot.join("core.sqlite")), 3);
    writer.shutdown().unwrap();
}

#[derive(Debug)]
enum CheckpointError {
    Storage(Error),
    WorkerUnavailable,
}
impl From<Error> for CheckpointError {
    fn from(error: Error) -> Self {
        Self::Storage(error)
    }
}

#[test]
fn errors_and_panics_resume_admission_before_delivering_typed_results() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("node.sqlite");
    let writer = Writer::start(&path, migrations(), Config::default()).unwrap();
    let handle = writer.handle();
    let failed = handle
        .maintenance_service(|_| Err::<(), CheckpointError>(CheckpointError::WorkerUnavailable))
        .unwrap()
        .blocking_wait();
    assert!(matches!(failed, Err(CheckpointError::WorkerUnavailable)));
    handle
        .submit(Priority::Interactive, |_| Ok(()))
        .unwrap()
        .blocking_wait()
        .unwrap();
    let panicked = handle
        .maintenance_service(|database| -> std::result::Result<(), CheckpointError> {
            database.transaction(
                Command {
                    namespace: "fixture/member",
                    id: "aborted",
                    request: b"aborted",
                },
                |tx| {
                    tx.execute("INSERT INTO fixture_effects VALUES('aborted')", [])?;
                    panic!("checkpoint failure");
                },
            )?;
            Ok(())
        })
        .unwrap()
        .blocking_wait();
    assert!(matches!(
        panicked,
        Err(CheckpointError::Storage(Error::WorkPanicked))
    ));
    handle
        .submit(Priority::Background, |database| {
            effect(database, "after-error")
        })
        .unwrap()
        .blocking_wait()
        .unwrap();
    assert_eq!(count(&path), 1);
    writer.shutdown().unwrap();
}

#[test]
fn disconnected_maintenance_is_drained_even_when_shutdown_precedes_its_execution() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("node.sqlite");
    let writer = Writer::start(&path, migrations(), Config::default()).unwrap();
    let handle = writer.handle();
    let (started, entered) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let ordinary = handle
        .submit(Priority::Interactive, move |_| {
            started.send(()).unwrap();
            resume.recv_timeout(Duration::from_secs(5)).unwrap();
            Ok(())
        })
        .unwrap();
    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    drop(
        handle
            .maintenance(|database| effect(database, "retained-checkpoint"))
            .unwrap(),
    );
    let closed = thread::spawn(move || writer.shutdown());
    let mut queues = handle.shared.queues.lock().unwrap();
    while !queues.stopping {
        let (next, timed) = handle
            .shared
            .ready
            .wait_timeout(queues, Duration::from_secs(5))
            .unwrap();
        queues = next;
        assert!(!timed.timed_out(), "shutdown did not close admission");
    }
    drop(queues);
    release.send(()).unwrap();
    ordinary.blocking_wait().unwrap();
    closed.join().unwrap().unwrap();
    assert_eq!(count(&path), 1);
    assert!(matches!(
        handle.maintenance(|_| Ok(())),
        Err(Error::WriterClosed)
    ));
    Writer::start(&path, migrations(), Config::default())
        .unwrap()
        .shutdown()
        .unwrap();
}

#[test]
fn concurrent_maintenance_requests_admit_exactly_one_checkpoint() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("node.sqlite");
    let writer = Writer::start(&path, migrations(), Config::default()).unwrap();
    let handle = writer.handle();
    let ready = Arc::new(Barrier::new(9));
    let released = Arc::new((Mutex::new(false), Condvar::new()));
    let threads = (0..8)
        .map(|_| {
            let handle = handle.clone();
            let ready = ready.clone();
            let released = released.clone();
            thread::spawn(move || {
                ready.wait();
                handle.maintenance(move |database| {
                    let (lock, signal) = &*released;
                    let mut done = lock.lock().unwrap();
                    while !*done {
                        let (next, timed) =
                            signal.wait_timeout(done, Duration::from_secs(5)).unwrap();
                        done = next;
                        assert!(!timed.timed_out(), "checkpoint was not released");
                    }
                    effect(database, "one-checkpoint")
                })
            })
        })
        .collect::<Vec<_>>();
    ready.wait();
    let mut accepted = vec![];
    for thread in threads {
        match thread.join().unwrap() {
            Ok(pending) => accepted.push(pending),
            Err(Error::MaintenancePaused) => {}
            Err(other) => panic!("unexpected admission result: {other}"),
        }
    }
    assert_eq!(accepted.len(), 1);
    *released.0.lock().unwrap() = true;
    released.1.notify_all();
    accepted.pop().unwrap().blocking_wait().unwrap();
    assert_eq!(count(&path), 1);
    writer.shutdown().unwrap();
}
