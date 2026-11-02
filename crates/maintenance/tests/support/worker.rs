// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_maintenance::{
    Error, Result,
    checkpoints::{WorkerArtifact, WorkerCheckpoint},
};
use rusqlite::{Connection, params};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[derive(Default)]
pub(crate) struct State {
    pub(crate) paused: Option<[u8; 16]>,
    pub(crate) resumes: usize,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Failure {
    None,
    Pause,
    Checkpoint,
    Stale,
    Outside,
    Resume,
    PanicResume,
}
pub(crate) struct Worker {
    pub(crate) root: PathBuf,
    pub(crate) db: Connection,
    pub(crate) state: Arc<Mutex<State>>,
    pub(crate) failure: Failure,
}
impl Worker {
    pub(crate) fn new(parent: &Path, id: &str) -> Self {
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
    pub(crate) fn reward(&mut self, id: &str, amount: i64) -> bool {
        assert!(self.state.lock().unwrap().paused.is_none());
        reward(&mut self.db, id, amount)
    }
}
pub(crate) fn reward(db: &mut Connection, id: &str, amount: i64) -> bool {
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
