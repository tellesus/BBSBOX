// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_game_host::{
    Manifest,
    protocol::{Move, Operation, Reason, ResultBody, State},
    sessions::PlayerId,
    worker::Worker,
};
use bbsbox_maintenance::{
    Limits,
    checkpoints::{Registration, WorkerCheckpoint, with_checkpoints},
};
use std::{path::Path, time::Duration};

fn player(id: u8) -> PlayerId {
    serde_json::from_value(serde_json::to_value([id; 16]).unwrap()).unwrap()
}

fn read(body: ResultBody) -> State {
    match body {
        ResultBody::State { state } => state,
        other => panic!("expected game state, received {other:?}"),
    }
}

#[test]
fn actual_worker_process_keeps_two_seats_and_deduplicates_after_restart() {
    let directory = tempfile::tempdir().unwrap();
    let state = directory.path().join("game.sqlite");
    let manifest = Manifest::parse(include_bytes!("../manifest.json")).unwrap();
    let executable = Path::new(env!("CARGO_BIN_EXE_bbsbox-demo-worker"));
    let start = || Worker::launch(executable, &state, &manifest, Duration::from_secs(4)).unwrap();
    let mut worker = start();
    let first = player(1);
    let second = player(2);
    let outside = player(3);
    assert_eq!(
        read(worker.call(Operation::Join { player: first }).unwrap()).seat,
        0
    );
    let step = || Operation::Act {
        player: first,
        command: [9; 16],
        action: Move::Explore,
    };
    assert!(matches!(
        worker.call(step()).unwrap(),
        ResultBody::Refused {
            reason: Reason::Waiting
        }
    ));
    assert_eq!(
        read(worker.call(Operation::Join { player: second }).unwrap()).seat,
        1
    );
    assert!(matches!(
        worker.call(Operation::Join { player: outside }).unwrap(),
        ResultBody::Refused {
            reason: Reason::Full
        }
    ));
    let result = read(worker.call(step()).unwrap());
    assert_eq!((result.chamber, result.turn), (1, 1));
    assert_eq!(read(worker.call(step()).unwrap()), result);
    assert!(matches!(
        worker
            .call(Operation::Act {
                player: first,
                command: [9; 16],
                action: Move::Rest
            })
            .unwrap(),
        ResultBody::Refused {
            reason: Reason::ChangedRetry
        }
    ));
    drop(worker);
    let mut worker = start();
    assert_eq!(read(worker.call(step()).unwrap()), result);
    assert_eq!(
        read(worker.call(Operation::Read { player: second }).unwrap()).chamber,
        1
    );
    let second_move = read(
        worker
            .call(Operation::Act {
                player: second,
                command: [10; 16],
                action: Move::Explore,
            })
            .unwrap(),
    );
    assert_eq!((second_move.chamber, second_move.turn), (2, 0));
    assert!(matches!(
        worker.call(Operation::Read { player: outside }).unwrap(),
        ResultBody::Refused {
            reason: Reason::NotJoined
        }
    ));
}

#[cfg(unix)]
#[test]
fn socket_activated_worker_uses_the_same_protocol_and_exits_when_the_core_disconnects() {
    use std::{
        os::{fd::OwnedFd, unix::net::UnixListener},
        process::{Command, Stdio},
    };
    let directory = tempfile::tempdir().unwrap();
    let state = directory.path().join("game.sqlite");
    let socket = directory.path().join("worker.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let manifest = Manifest::parse(include_bytes!("../manifest.json")).unwrap();
    let executable = env!("CARGO_BIN_EXE_bbsbox-demo-worker");

    let server = std::thread::spawn(move || {
        let (connection, _) = listener.accept().unwrap();
        let stdin = OwnedFd::from(connection.try_clone().unwrap());
        let stdout = OwnedFd::from(connection);
        Command::new(executable)
            .arg("--state")
            .arg(state)
            .stdin(Stdio::from(stdin))
            .stdout(Stdio::from(stdout))
            .spawn()
            .unwrap()
    });
    let mut worker = Worker::connect(&socket, &manifest, Duration::from_secs(3)).unwrap();
    let mut child = server.join().unwrap();
    let first = player(1);
    assert_eq!(
        read(worker.call(Operation::Join { player: first }).unwrap()).seat,
        0
    );
    drop(worker);
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("game worker stayed up after the core disconnected");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn coordinator_copies_a_real_worker_checkpoint_and_resumes_only_the_matching_fence() {
    let directory = tempfile::tempdir().unwrap();
    let live = directory.path().join("live");
    std::fs::create_dir(&live).unwrap();
    let state = live.join("game.sqlite");
    let staged = directory.path().join("staged");
    let manifest = Manifest::parse(include_bytes!("../manifest.json")).unwrap();
    let executable = Path::new(env!("CARGO_BIN_EXE_bbsbox-demo-worker"));
    let start = |path: &Path| Worker::launch(executable, path, &manifest, Duration::from_secs(4));
    let mut worker = start(&state).unwrap();
    assert!(start(&state).is_err());
    let first = player(1);
    let second = player(2);
    worker.call(Operation::Join { player: first }).unwrap();
    worker.call(Operation::Join { player: second }).unwrap();
    worker.pause([1; 16]).unwrap();
    assert!(worker.resume([2; 16]).is_err());
    let move_one = || Operation::Act {
        player: first,
        command: [9; 16],
        action: Move::Explore,
    };
    assert!(matches!(
        worker.call(move_one()).unwrap(),
        ResultBody::Refused {
            reason: Reason::Paused
        }
    ));
    worker.resume([1; 16]).unwrap();
    let expected = read(worker.call(move_one()).unwrap());
    let games = vec![manifest.game_id().to_owned()];
    with_checkpoints(
        &games,
        &mut [Registration {
            game_id: manifest.game_id(),
            state_root: &live,
            worker: &mut worker,
        }],
        &staged,
        Limits::default(),
        |records| {
            assert_eq!(records[0].worker_version, manifest.worker_version());
            assert_eq!(records[0].state_schema, manifest.state_schema());
            assert_eq!(records[0].files[0].path, "state.sqlite");
            Ok(())
        },
    )
    .unwrap();
    assert!(std::fs::read_dir(&live).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("checkpoint-")
    }));
    let later = read(
        worker
            .call(Operation::Act {
                player: second,
                command: [8; 16],
                action: Move::Explore,
            })
            .unwrap(),
    );
    assert_eq!(later.chamber, 2);
    let mut restored = start(&staged.join(manifest.game_id()).join("state.sqlite")).unwrap();
    assert_eq!(read(restored.call(move_one()).unwrap()), expected);
}
