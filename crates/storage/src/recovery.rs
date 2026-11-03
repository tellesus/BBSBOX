// SPDX-License-Identifier: AGPL-3.0-or-later
//! Cold restored-storage exclusion. M03/M12 own identity, horizon and activation authorization.
use crate::{Database, Error, Migration, Result, disk::Class, snapshot, writer};
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::fs::File;
use std::{
    fs::{self, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
};

pub struct Recovery {
    // Close database handles before releasing the shared process lock on every path.
    database: Option<Database>,
    _lock: writer::WriterLock,
    path: PathBuf,
}
fn regular(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(Error::InvalidRecoveryDecision);
    }
    Ok(())
}
fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
impl Recovery {
    /// Require existing exact-schema restored data; never create a database or apply migrations.
    /// Caller controls the private restoration directory/ancestors throughout this operation.
    pub fn open(path: impl AsRef<Path>, migrations: &[Migration]) -> Result<Self> {
        let original = std::path::absolute(path.as_ref())?;
        regular(&original)?;
        for parent in original
            .parent()
            .ok_or(Error::InvalidConfiguration)?
            .ancestors()
        {
            let metadata = fs::symlink_metadata(parent)?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err(Error::InvalidRecoveryDecision);
            }
        }
        let (lock, path) = writer::lock(&original)?;
        let marker = snapshot::recovery_marker(&path);
        if !marker.try_exists()? {
            return Err(Error::RecoveryNotPending);
        }
        regular(&marker)?;
        crate::verify_exact_registry(&path, migrations)?;
        let database = Database::open(&path, migrations)?;
        Ok(Self {
            database: Some(database),
            _lock: lock,
            path,
        })
    }
    /// Trusted cold reconciliation only. No writer handle or HTTP admission is exposed.
    pub fn database(&mut self) -> &mut Database {
        self.database
            .as_mut()
            .expect("live recovery owns its database")
    }

    /// M12 must first validate/sign the full identity/horizon/module decision and write it in
    /// protected configuration/control storage included by its own continuity/handover policy.
    /// This layer binds/synchronizes the exact record before releasing the persistent marker.
    pub fn finish_after_decision(
        mut self,
        decision: impl AsRef<Path>,
        expected_sha256: [u8; 32],
    ) -> Result<()> {
        let decision = decision.as_ref();
        if expected_sha256 == [0; 32] {
            return Err(Error::InvalidRecoveryDecision);
        }
        regular(decision)?;
        let canonical = decision.canonicalize()?;
        let marker = snapshot::recovery_marker(&self.path);
        if canonical == self.path || canonical == marker {
            return Err(Error::InvalidRecoveryDecision);
        }
        let mut file = OpenOptions::new().read(true).write(true).open(decision)?;
        let mut raw = vec![];
        (&mut file).take(65537).read_to_end(&mut raw)?;
        if raw.is_empty()
            || raw.len() > 65536
            || <[u8; 32]>::from(Sha256::digest(&raw)) != expected_sha256
        {
            return Err(Error::InvalidRecoveryDecision);
        }
        self.database().admit(Class::Control, 65536)?;
        file.sync_all()?;
        sync_directory(canonical.parent().ok_or(Error::InvalidRecoveryDecision)?)?;
        regular(&marker)?;
        drop(self.database.take());
        fs::remove_file(marker)?;
        sync_directory(self.path.parent().ok_or(Error::InvalidConfiguration)?)?;
        Ok(())
    }
}
