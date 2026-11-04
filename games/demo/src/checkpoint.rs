// SPDX-License-Identifier: AGPL-3.0-or-later
//! The demo worker owns the snapshot lease and its exact files.
use super::refused;
use bbsbox_game_host::{
    Manifest,
    protocol::{Reason, ResultBody},
};
use rusqlite::Connection;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub struct Manager<'a> {
    root: &'a Path,
    fence: Option<[u8; 16]>,
    completed: bool,
}

impl<'a> Manager<'a> {
    pub fn new(root: &'a Path) -> Self {
        Self {
            root,
            fence: None,
            completed: false,
        }
    }

    pub fn paused(&self) -> bool {
        self.fence.is_some()
    }

    pub fn pause(&mut self, fence: [u8; 16]) -> ResultBody {
        if fence == [0; 16] || self.fence.is_some_and(|current| current != fence) {
            return refused(Reason::Paused);
        }
        self.fence = Some(fence);
        ResultBody::Paused { fence }
    }

    pub fn capture(&mut self, db: &Connection, manifest: &Manifest, fence: [u8; 16]) -> ResultBody {
        if self.fence != Some(fence) {
            return refused(Reason::Paused);
        }
        let (checkpoint_id, directory) = self.location(fence);
        if !self.completed {
            if copy(db, &directory).is_err() {
                return refused(Reason::Unavailable);
            }
            self.completed = true;
        }
        ResultBody::Checkpoint {
            fence,
            checkpoint_id,
            state_schema: manifest.state_schema(),
            worker_version: manifest.worker_version().into(),
            directory,
        }
    }

    pub fn resume(&mut self, fence: [u8; 16]) -> ResultBody {
        if fence == [0; 16] || self.fence.is_some_and(|current| current != fence) {
            return refused(Reason::Paused);
        }
        if self.completed {
            let (_, directory) = self.location(fence);
            if discard(&directory).is_err() {
                return refused(Reason::Unavailable);
            }
        }
        self.fence = None;
        self.completed = false;
        ResultBody::Resumed { fence }
    }

    fn location(&self, fence: [u8; 16]) -> (String, PathBuf) {
        let id = hex::encode(fence);
        let path = self.root.join(format!("checkpoint-{id}"));
        (id, path)
    }
}

fn copy(db: &Connection, destination: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if destination.exists() {
        return Err("checkpoint path already exists".into());
    }
    let parent = destination.parent().ok_or("missing checkpoint parent")?;
    let temporary = tempfile::Builder::new()
        .prefix(".checkpoint-")
        .tempdir_in(parent)?;
    let path = temporary.path().join("state.sqlite");
    db.backup(
        rusqlite::MAIN_DB,
        &path,
        None::<fn(rusqlite::backup::Progress)>,
    )?;
    let snapshot = Connection::open(&path)?;
    snapshot.pragma_update(None, "journal_mode", "DELETE")?;
    drop(snapshot);
    fs::OpenOptions::new().write(true).open(&path)?.sync_all()?;
    sync_directory(temporary.path())?;
    fs::rename(temporary.path(), destination)?;
    if let Err(error) = sync_directory(parent) {
        let _ = discard(destination);
        return Err(error.into());
    }
    Ok(())
}

fn discard(directory: &Path) -> io::Result<()> {
    match fs::remove_file(directory.join("state.sqlite")) {
        Ok(()) => (),
        Err(error) if error.kind() == io::ErrorKind::NotFound => (),
        Err(error) => return Err(error),
    }
    match fs::remove_dir(directory) {
        Ok(()) => (),
        Err(error) if error.kind() == io::ErrorKind::NotFound => (),
        Err(error) => return Err(error),
    }
    sync_directory(
        directory
            .parent()
            .ok_or_else(|| io::Error::other("missing checkpoint parent"))?,
    )
}

fn sync_directory(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    fs::File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
