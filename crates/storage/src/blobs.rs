// SPDX-License-Identifier: AGPL-3.0-or-later
//! Content-addressed local bytes. Only the storage writer mutates this directory.
use crate::{Database, Error, Result, disk::Class, valid_key};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

pub(crate) const SCHEMA: &str = "CREATE TABLE core_blobs(digest TEXT PRIMARY KEY, size INTEGER NOT NULL CHECK(size>=0));
CREATE TABLE core_blob_refs(namespace TEXT NOT NULL,ref_key TEXT NOT NULL,digest TEXT NOT NULL REFERENCES core_blobs(digest) ON DELETE RESTRICT,PRIMARY KEY(namespace,ref_key));
CREATE INDEX core_blob_ref_digest ON core_blob_refs(digest);";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Info {
    pub digest: String,
    pub size: u64,
}
pub(crate) fn read_size(row: &rusqlite::Row<'_>, column: usize) -> rusqlite::Result<u64> {
    row.get::<_, i64>(column)?.try_into().map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Integer,
            Box::new(error),
        )
    })
}
pub(crate) fn valid_digest(digest: &str) -> bool {
    digest.len() == 64
        && digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn root(database: &Path) -> PathBuf {
    let mut name = database.as_os_str().to_os_string();
    name.push(".blobs");
    name.into()
}
pub(crate) fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    // Windows development tests verify process crashes, not appliance power-loss durability.
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
pub(crate) fn create_private_directory(path: &Path, recursive: bool) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(recursive);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)?;
    Ok(())
}
pub(crate) fn hash_file(path: &Path) -> Result<Info> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut size = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
        size = size.checked_add(count as u64).ok_or(Error::InvalidBlob)?;
    }
    Ok(Info {
        digest: format!("{:x}", hash.finalize()),
        size,
    })
}
pub(crate) fn verify(path: &Path, expected: &Info) -> Result<()> {
    if !valid_digest(&expected.digest) || hash_file(path)? != *expected {
        return Err(Error::CorruptBlob);
    }
    Ok(())
}
/// Use in a module command/outbox transaction. An upload reference protects bytes until that command consumes it.
pub fn set_reference(tx: &Transaction<'_>, namespace: &str, key: &str, digest: &str) -> Result<()> {
    if !valid_key(namespace) || !valid_key(key) || !valid_digest(digest) {
        return Err(Error::InvalidBlob);
    }
    if !tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM core_blobs WHERE digest=?1)",
        [digest],
        |r| r.get::<_, bool>(0),
    )? {
        return Err(Error::InvalidBlob);
    }
    tx.execute("INSERT INTO core_blob_refs VALUES(?1,?2,?3) ON CONFLICT(namespace,ref_key) DO UPDATE SET digest=excluded.digest",params![namespace,key,digest])?;
    Ok(())
}
/// Explicit module retention action; no reference is implicitly evicted under pressure.
pub fn remove_reference(tx: &Transaction<'_>, namespace: &str, key: &str) -> Result<bool> {
    if !valid_key(namespace) || !valid_key(key) {
        return Err(Error::InvalidBlob);
    }
    Ok(tx.execute(
        "DELETE FROM core_blob_refs WHERE namespace=?1 AND ref_key=?2",
        [namespace, key],
    )? == 1)
}
impl Database {
    /// Stream a bounded upload, finalize bytes, then commit metadata and its protecting reference.
    pub fn put_blob(&mut self, namespace: &str, key: &str, mut source: impl Read) -> Result<Info> {
        if !valid_key(namespace) || !valid_key(key) {
            return Err(Error::InvalidBlob);
        }
        self.admit(Class::Content, 65536)?;
        let root = root(&self.path);
        let data = root.join("data");
        let staging = root.join("staging");
        create_private_directory(&data, true)?;
        create_private_directory(&staging, true)?;
        sync_directory(self.directory())?;
        sync_directory(&root)?;
        let mut temporary = tempfile::NamedTempFile::new_in(&staging)?;
        let mut hash = Sha256::new();
        let mut size = 0u64;
        let mut buffer = [0u8; 65536];
        loop {
            let count = source.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            size = size.checked_add(count as u64).ok_or(Error::BlobTooLarge)?;
            if size > self.budget.max_blob_bytes {
                return Err(Error::BlobTooLarge);
            }
            self.admit(Class::Content, count as u64 + 65536)?;
            temporary.write_all(&buffer[..count])?;
            hash.update(&buffer[..count]);
        }
        temporary.as_file().sync_all()?;
        let info = Info {
            digest: format!("{:x}", hash.finalize()),
            size,
        };
        let destination = data.join(&info.digest);
        if destination.exists() {
            verify(&destination, &info)?;
        } else {
            temporary
                .persist_noclobber(&destination)
                .map_err(|error| Error::Io(error.error))?;
            sync_directory(&data)?;
        }
        self.admit(Class::Content, 65536)?;
        let directory = self.directory().to_owned();
        let tx = self.connection.transaction()?;
        let old: Option<u64> = tx
            .query_row(
                "SELECT size FROM core_blobs WHERE digest=?1",
                [&info.digest],
                |r| read_size(r, 0),
            )
            .optional()?;
        if old.is_some_and(|old| old != info.size) {
            return Err(Error::CorruptBlob);
        }
        tx.execute(
            "INSERT OR IGNORE INTO core_blobs VALUES(?1,?2)",
            params![info.digest, info.size as i64],
        )?;
        set_reference(&tx, namespace, key, &info.digest)?;
        self.budget
            .admit(self.space.as_ref(), &directory, Class::Content, 0)?;
        tx.commit()?;
        Ok(info)
    }
    pub fn get_blob(&self, digest: &str) -> Result<File> {
        if !valid_digest(digest) {
            return Err(Error::InvalidBlob);
        }
        let size: Option<u64> = self
            .connection
            .query_row(
                "SELECT size FROM core_blobs WHERE digest=?1",
                [digest],
                |r| read_size(r, 0),
            )
            .optional()?;
        let info = Info {
            digest: digest.into(),
            size: size.ok_or(Error::InvalidBlob)?,
        };
        let path = root(&self.path).join("data").join(digest);
        verify(&path, &info)?;
        let mut file = File::open(path)?;
        file.seek(SeekFrom::Start(0))?;
        Ok(file)
    }
    pub(crate) fn blob_inventory(&self) -> Result<Vec<Info>> {
        Ok(self
            .connection
            .prepare("SELECT digest,size FROM core_blobs ORDER BY digest LIMIT 50001")?
            .query_map([], |r| {
                Ok(Info {
                    digest: r.get(0)?,
                    size: read_size(r, 1)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }
    /// Delete only unreferenced rows, then bytes. A crash leaves an orphan, never a dangling reference.
    pub fn collect_blobs(&mut self, limit: usize) -> Result<usize> {
        if limit == 0 || limit > 1024 {
            return Err(Error::InvalidConfiguration);
        }
        let tx = self.connection.transaction()?;
        let digests=tx.prepare("SELECT digest FROM core_blobs WHERE NOT EXISTS(SELECT 1 FROM core_blob_refs WHERE core_blob_refs.digest=core_blobs.digest) ORDER BY digest LIMIT ?1")?.query_map([limit as i64],|r|r.get::<_,String>(0))?.collect::<std::result::Result<Vec<_>,_>>()?;
        for digest in &digests {
            if !valid_digest(digest) {
                return Err(Error::InvalidBlob);
            }
            tx.execute("DELETE FROM core_blobs WHERE digest=?1", [digest])?;
        }
        tx.commit()?;
        let data = root(&self.path).join("data");
        for digest in &digests {
            match fs::remove_file(data.join(digest)) {
                Ok(()) => (),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(error.into()),
            }
        }
        if !digests.is_empty() {
            sync_directory(&data)?;
        }
        Ok(digests.len())
    }
    /// Maintenance hook under the writer lock. Unknown files/directories are never recursively removed.
    /// Limits deletions per invocation; the directory scan itself is not a constant-time operation.
    pub fn recover_blob_orphans(&mut self, limit: usize) -> Result<usize> {
        if limit == 0 || limit > 1024 {
            return Err(Error::InvalidConfiguration);
        }
        let root = root(&self.path);
        let mut removed = 0;
        for (name, staging) in [("staging", true), ("data", false)] {
            let directory = root.join(name);
            if !directory.exists() {
                continue;
            }
            for entry in fs::read_dir(&directory)? {
                let entry = entry?;
                if !entry.file_type()?.is_file() {
                    continue;
                }
                let name = entry.file_name();
                let Some(name) = name.to_str() else {
                    continue;
                };
                let known = if staging {
                    false
                } else {
                    if !valid_digest(name) {
                        continue;
                    }
                    self.connection.query_row(
                        "SELECT EXISTS(SELECT 1 FROM core_blobs WHERE digest=?1)",
                        [name],
                        |r| r.get::<_, bool>(0),
                    )?
                };
                if !known {
                    fs::remove_file(entry.path())?;
                    removed += 1;
                    if removed == limit {
                        sync_directory(&directory)?;
                        return Ok(removed);
                    }
                }
            }
            sync_directory(&directory)?;
        }
        Ok(removed)
    }
}
