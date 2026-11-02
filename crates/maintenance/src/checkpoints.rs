// SPDX-License-Identifier: AGPL-3.0-or-later
//! C01 worker checkpoint provider. Actual game transport/activation belongs to M11.
use crate::{
    Error, Limits, Result, archive,
    inventory::{self, FileRecord},
    snapshot,
};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
};

pub struct WorkerArtifact {
    pub request: [u8; 16],
    pub checkpoint_id: String,
    pub state_schema: u32,
    pub worker_version: String,
    pub directory: PathBuf,
}
/// The adapter owns bounded local IPC. Pause drains accepted commands and rejects new mutations.
/// Both pause and idempotent resume are fenced by this request, including failed/partial pauses.
/// Resume releases the request's frozen checkpoint lease after the coordinator has copied it.
pub trait WorkerCheckpoint: Send {
    fn pause(&mut self, request: [u8; 16]) -> Result<()>;
    fn checkpoint(&mut self, request: [u8; 16]) -> Result<WorkerArtifact>;
    fn resume(&mut self, request: [u8; 16]) -> Result<()>;
}
/// Registration is supplied by the trusted installed-game registry, never a worker response.
pub struct Registration<'a> {
    pub game_id: &'a str,
    pub state_root: &'a Path,
    pub worker: &'a mut dyn WorkerCheckpoint,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointRecord {
    pub game_id: String,
    pub request: [u8; 16],
    pub checkpoint_id: String,
    pub state_schema: u32,
    pub worker_version: String,
    pub files: Vec<FileRecord>,
}
fn id(value: &str) -> bool {
    value.len() <= 32 && !value.contains('/') && inventory::valid_path(value)
}
fn label(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && value.bytes().all(|b| b.is_ascii_graphic())
}
struct Paused<'a, 'b> {
    registrations: &'a mut [Registration<'b>],
    attempted: usize,
    request: [u8; 16],
}
impl Paused<'_, '_> {
    fn resume_all(&mut self) -> Result<()> {
        let mut failed = vec![];
        for index in (0..self.attempted).rev() {
            if !matches!(
                catch_unwind(AssertUnwindSafe(|| self.registrations[index]
                    .worker
                    .resume(self.request))),
                Ok(Ok(()))
            ) {
                failed.push(self.registrations[index].game_id.to_string());
            }
        }
        self.attempted = 0;
        if !failed.is_empty() {
            Err(Error::CheckpointResumeFailed {
                request: self.request,
                games: failed,
            })
        } else {
            Ok(())
        }
    }
}
impl Drop for Paused<'_, '_> {
    fn drop(&mut self) {
        let _ = self.resume_all();
    }
}

/// Call after core admission/drain, retaining configuration exclusion. `publish` must capture core
/// and durably publish the combined signed backup manifest before it returns success.
/// Every installed game, including disabled games retaining state, must have one registration.
pub fn with_checkpoints<T>(
    installed: &[String],
    registrations: &mut [Registration<'_>],
    destination: &Path,
    limits: Limits,
    publish: impl FnOnce(&[CheckpointRecord]) -> Result<T>,
) -> Result<T> {
    limits.validate()?;
    if installed.len() > 64 || registrations.len() != installed.len() {
        return Err(Error::CheckpointUnavailable);
    }
    let expected: BTreeSet<_> = installed.iter().map(String::as_str).collect();
    let supplied: BTreeSet<_> = registrations.iter().map(|r| r.game_id).collect();
    if expected.len() != installed.len()
        || supplied.len() != registrations.len()
        || expected != supplied
        || expected.iter().any(|s| !id(s))
    {
        return Err(Error::CheckpointUnavailable);
    }
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()?;
    let mut roots = vec![];
    for registration in registrations.iter() {
        if std::fs::symlink_metadata(registration.state_root)?
            .file_type()
            .is_symlink()
        {
            return Err(Error::UnsafePath);
        }
        let root = registration.state_root.canonicalize()?;
        if !root.is_dir()
            || parent.starts_with(&root)
            || roots
                .iter()
                .any(|other: &PathBuf| other.starts_with(&root) || root.starts_with(other))
        {
            return Err(Error::UnsafePath);
        }
        roots.push(root);
    }
    let mut request = [0; 16];
    while request == [0; 16] {
        OsRng.fill_bytes(&mut request);
    }
    let mut paused = Paused {
        registrations,
        attempted: 0,
        request,
    };
    let operation = catch_unwind(AssertUnwindSafe(|| {
        // Register the attempt before calling out so a partial pause/panic still receives resume.
        while paused.attempted < paused.registrations.len() {
            let index = paused.attempted;
            paused.attempted += 1;
            paused.registrations[index].worker.pause(request)?;
        }
        archive::private_directory(destination)?;
        let mut records = vec![];
        let mut remaining = limits;
        for (index, registration) in paused.registrations.iter_mut().enumerate() {
            if remaining.max_files == 0 || remaining.max_total_bytes == 0 {
                return Err(Error::LimitExceeded);
            }
            let artifact = registration.worker.checkpoint(request)?;
            if artifact.request != request
                || !label(&artifact.checkpoint_id)
                || artifact.state_schema == 0
                || !label(&artifact.worker_version)
            {
                return Err(Error::IncompleteSnapshot);
            }
            if std::fs::symlink_metadata(&artifact.directory)?
                .file_type()
                .is_symlink()
            {
                return Err(Error::UnsafePath);
            }
            let source = artifact.directory.canonicalize()?;
            if source == roots[index] || !source.starts_with(&roots[index]) {
                return Err(Error::UnsafePath);
            }
            remaining.max_member_bytes = remaining.max_member_bytes.min(remaining.max_total_bytes);
            let files = archive::inventory(&source, remaining)?;
            if files.is_empty()
                || files
                    .iter()
                    .any(|f| format!("games/{}/{}", registration.game_id, f.path).len() > 94)
            {
                return Err(Error::IncompleteSnapshot);
            }
            let bytes = files.iter().map(|f| f.size).sum::<u64>();
            archive::space(destination, bytes)?;
            snapshot::copy_records(&source, &destination.join(registration.game_id), &files)?;
            if archive::inventory(&source, remaining)? != files {
                return Err(Error::ChangedSource);
            }
            remaining.max_files -= files.len();
            remaining.max_total_bytes -= bytes;
            records.push(CheckpointRecord {
                game_id: registration.game_id.into(),
                request,
                checkpoint_id: artifact.checkpoint_id,
                state_schema: artifact.state_schema,
                worker_version: artifact.worker_version,
                files,
            });
        }
        records.sort_by(|a, b| a.game_id.cmp(&b.game_id));
        archive::sync_directory(destination)?;
        publish(&records)
    }));
    let resumed = paused.resume_all();
    // A resume failure is actionable even when export also failed; every worker was attempted.
    resumed?;
    operation.map_err(|_| Error::Storage(bbsbox_storage::Error::WorkPanicked))?
}
