// SPDX-License-Identifier: AGPL-3.0-or-later
//! Core-only maintenance hooks. M12 coordinates configuration/secrets/game checkpoints and activation.
use crate::{
    Database, Error, Result, SCHEMA_VERSION,
    blobs::{self, Info},
    open_reader,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_MANIFEST: u64 = 8 * 1024 * 1024;
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format: String,
    pub scope: String,
    pub core_schema: i32,
    pub database: Info,
    pub blobs: Vec<Info>,
}
pub(crate) fn recovery_marker(database: &Path) -> PathBuf {
    let mut name = database.as_os_str().to_os_string();
    name.push(".recovery-required");
    name.into()
}
fn durable_copy(source: &Path, destination: &Path) -> Result<()> {
    let mut input = File::open(source)?;
    let mut output = private_file(destination)?;
    std::io::copy(&mut input, &mut output)?;
    output.sync_all()?;
    Ok(())
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut output = private_file(path)?;
    output.write_all(bytes)?;
    output.sync_all()?;
    Ok(())
}
fn private_file(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    Ok(options.open(path)?)
}
fn parent(path: &Path) -> &Path {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}
fn space_for(directory: &Path, required: u64, reserve: u64) -> Result<()> {
    let needed = required
        .checked_add(reserve)
        .ok_or(Error::InvalidSnapshot)?;
    let available = fs2::available_space(directory)?;
    if available < needed {
        return Err(Error::DiskPressure { available, needed });
    }
    Ok(())
}
impl Database {
    /// Invoke under the writer lock after maintenance admission is paused/drained.
    /// Serialization protects every inventoried blob from GC for this operation's duration.
    pub fn snapshot_core(&mut self, destination: impl AsRef<Path>) -> Result<Manifest> {
        let destination = destination.as_ref();
        let inventory = self.blob_inventory()?;
        if inventory.len() > 50_000 {
            return Err(Error::InvalidSnapshot);
        }
        for info in &inventory {
            blobs::verify(
                &blobs::root(&self.path).join("data").join(&info.digest),
                info,
            )?;
        }
        blobs::create_private_directory(destination, false)?; // Never overwrite an existing complete or partial snapshot.
        blobs::sync_directory(parent(destination))?;
        let pages = self
            .connection
            .pragma_query_value(None, "page_count", |r| blobs::read_size(r, 0))?;
        let page_size = self
            .connection
            .pragma_query_value(None, "page_size", |r| blobs::read_size(r, 0))?;
        let bytes = inventory.iter().try_fold(
            pages.checked_mul(page_size).ok_or(Error::InvalidSnapshot)?,
            |sum, info| sum.checked_add(info.size).ok_or(Error::InvalidSnapshot),
        )?;
        space_for(destination, bytes, self.budget.control_floor)?;
        let database = destination.join("core.sqlite");
        drop(private_file(&database)?);
        self.connection.backup(
            rusqlite::MAIN_DB,
            &database,
            None::<fn(rusqlite::backup::Progress)>,
        )?;
        // A standalone snapshot must also verify from read-only media without writable WAL sidecars.
        let backup = rusqlite::Connection::open(&database)?;
        backup.pragma_update(None, "journal_mode", "DELETE")?;
        drop(backup);
        OpenOptions::new().write(true).open(&database)?.sync_all()?;
        let data = destination.join("blobs");
        blobs::create_private_directory(&data, false)?;
        for info in &inventory {
            durable_copy(
                &blobs::root(&self.path).join("data").join(&info.digest),
                &data.join(&info.digest),
            )?;
            blobs::verify(&data.join(&info.digest), info)?;
        }
        blobs::sync_directory(&data)?;
        let manifest = Manifest {
            format: "bbsbox.core-snapshot/v1".into(),
            scope: "core-only".into(),
            core_schema: SCHEMA_VERSION,
            database: blobs::hash_file(&database)?,
            blobs: inventory,
        };
        let bytes = serde_json::to_vec(&manifest).map_err(|_| Error::InvalidSnapshot)?;
        if bytes.len() as u64 > MAX_MANIFEST {
            return Err(Error::InvalidSnapshot);
        }
        // Last durable file is the completion marker; partial directories never verify.
        write_new(&destination.join("manifest.json"), &bytes)?;
        blobs::sync_directory(destination)?;
        Ok(manifest)
    }
}
pub fn verify_core(source: impl AsRef<Path>) -> Result<Manifest> {
    let source = source.as_ref();
    let mut bytes = Vec::new();
    File::open(source.join("manifest.json"))?
        .take(MAX_MANIFEST + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_MANIFEST {
        return Err(Error::InvalidSnapshot);
    }
    let manifest: Manifest = serde_json::from_slice(&bytes).map_err(|_| Error::InvalidSnapshot)?;
    if manifest.format != "bbsbox.core-snapshot/v1"
        || manifest.scope != "core-only"
        || manifest.core_schema != SCHEMA_VERSION
        || manifest.blobs.len() > 50_000
    {
        return Err(Error::InvalidSnapshot);
    }
    blobs::verify(&source.join("core.sqlite"), &manifest.database)?;
    let connection = open_reader(source.join("core.sqlite"))?;
    let integrity: String = connection.pragma_query_value(None, "quick_check", |r| r.get(0))?;
    if integrity != "ok" || connection.prepare("PRAGMA foreign_key_check")?.exists([])? {
        return Err(Error::InvalidSnapshot);
    }
    let inventory = connection
        .prepare("SELECT digest,size FROM core_blobs ORDER BY digest LIMIT 50001")?
        .query_map([], |r| {
            Ok(Info {
                digest: r.get(0)?,
                size: blobs::read_size(r, 1)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if inventory != manifest.blobs {
        return Err(Error::InvalidSnapshot);
    }
    for info in &manifest.blobs {
        if !blobs::valid_digest(&info.digest) {
            return Err(Error::InvalidSnapshot);
        }
        blobs::verify(&source.join("blobs").join(&info.digest), info)?;
    }
    Ok(manifest)
}
/// Copy to a fresh directory, leaving a mandatory recovery marker beside node.sqlite.
/// Does not activate restored writers or restore a complete node.
pub fn restore_core(source: impl AsRef<Path>, destination: impl AsRef<Path>) -> Result<PathBuf> {
    let source = source.as_ref();
    let destination = destination.as_ref();
    let manifest = verify_core(source)?;
    blobs::create_private_directory(destination, false)?;
    blobs::sync_directory(parent(destination))?;
    let database = destination.join("node.sqlite");
    write_new(&recovery_marker(&database),b"Core-only restore. M03/M12 must reconcile recovery horizon and rotate writer credentials before activation.\n")?;
    blobs::sync_directory(destination)?;
    let total = manifest
        .blobs
        .iter()
        .try_fold(manifest.database.size, |sum, info| {
            sum.checked_add(info.size).ok_or(Error::InvalidSnapshot)
        })?;
    space_for(
        destination,
        total,
        crate::disk::Budget::default().control_floor,
    )?;
    durable_copy(&source.join("core.sqlite"), &database)?;
    // Verify copies too: source media can change between initial verification and copying.
    blobs::verify(&database, &manifest.database)?;
    let root = blobs::root(&database);
    let data = root.join("data");
    blobs::create_private_directory(&data, true)?;
    for info in &manifest.blobs {
        let path = data.join(&info.digest);
        durable_copy(&source.join("blobs").join(&info.digest), &path)?;
        blobs::verify(&path, info)?;
    }
    blobs::sync_directory(&data)?;
    blobs::sync_directory(&root)?;
    blobs::sync_directory(destination)?;
    Ok(database)
}
