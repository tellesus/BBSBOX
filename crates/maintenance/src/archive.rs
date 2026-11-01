// SPDX-License-Identifier: AGPL-3.0-or-later
//! Protected staged-file archives. Complete-node policy and signing activation belong to the coordinator.
use crate::{
    BackupIdentity, BackupRecipient, Error, Limits, Result,
    inventory::{self, FileRecord, SignedInventory},
};
use bbsbox_credentials::NodeId;
use bbsbox_identity::node::NodeSigner;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Cursor, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

const HEADER_LIMIT: usize = 64 * 1024;
const CONTROL_FLOOR: u64 = 32 * 1024 * 1024;

fn output(path: &Path) -> Result<PathBuf> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()?;
    let path = parent.join(path.file_name().ok_or(Error::UnsafePath)?);
    match fs::symlink_metadata(&path) {
        Ok(_) => {
            Err(io::Error::new(io::ErrorKind::AlreadyExists, "backup output already exists").into())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(path),
        Err(error) => Err(error.into()),
    }
}
fn private_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = fs::DirBuilder::new();
        builder.mode(0o700);
        builder.create(path)?;
    }
    #[cfg(not(unix))]
    fs::create_dir(path)?;
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
fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
fn hash_file(path: &Path, maximum: u64) -> Result<(u64, String)> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(Error::UnsafePath);
    }
    if metadata.len() > maximum {
        return Err(Error::LimitExceeded);
    }
    let mut source = File::open(path)?;
    let mut hash = Sha256::new();
    let mut count = 0u64;
    let mut buffer = [0; 65536];
    loop {
        let size = source.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        count = count.checked_add(size as u64).ok_or(Error::LimitExceeded)?;
        if count > maximum {
            return Err(Error::LimitExceeded);
        }
        hash.update(&buffer[..size]);
    }
    if count != metadata.len() {
        return Err(Error::ChangedSource);
    }
    Ok((count, hex::encode(hash.finalize())))
}
pub fn inventory(root: &Path, limits: Limits) -> Result<Vec<FileRecord>> {
    limits.validate()?;
    if fs::symlink_metadata(root)?.file_type().is_symlink() {
        return Err(Error::UnsafePath);
    }
    let root = root.canonicalize()?;
    let mut directories = vec![root.clone()];
    let mut visited = 0usize;
    let mut files = vec![];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            visited += 1;
            if visited > limits.max_files * 2 {
                return Err(Error::LimitExceeded);
            }
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            let name = path
                .strip_prefix(&root)
                .map_err(|_| Error::UnsafePath)?
                .components()
                .map(|part| part.as_os_str().to_str().ok_or(Error::UnsafePath))
                .collect::<Result<Vec<_>>>()?
                .join("/");
            if !inventory::valid_path(&name) || metadata.file_type().is_symlink() {
                return Err(Error::UnsafePath);
            }
            if metadata.is_dir() {
                directories.push(path);
            } else if metadata.is_file() {
                if files.len() >= limits.max_files {
                    return Err(Error::LimitExceeded);
                }
                let (size, sha256) = hash_file(&path, limits.max_member_bytes)?;
                files.push(FileRecord {
                    path: name,
                    size,
                    sha256,
                });
            } else {
                return Err(Error::UnsafePath);
            }
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}
fn encoded_limit(total: u64, count: usize, manifest: usize) -> Result<u64> {
    let plain = total
        .checked_add(
            (count as u64)
                .checked_mul(1024)
                .ok_or(Error::LimitExceeded)?,
        )
        .and_then(|n| n.checked_add(manifest as u64))
        .and_then(|n| n.checked_add(2048))
        .ok_or(Error::LimitExceeded)?;
    plain
        .checked_add((plain / 65536 + 1) * 16)
        .and_then(|n| n.checked_add(HEADER_LIMIT as u64))
        .ok_or(Error::LimitExceeded)
}
fn space(path: &Path, bytes: u64) -> Result<()> {
    let available = fs2::available_space(path)?;
    let needed = bytes
        .checked_add(CONTROL_FLOOR)
        .ok_or(Error::LimitExceeded)?;
    if available < needed {
        return Err(bbsbox_storage::Error::DiskPressure { available, needed }.into());
    }
    Ok(())
}
fn header(path: &str, size: u64) -> Result<tar::Header> {
    let mut header = tar::Header::new_ustar();
    header.set_path(path)?;
    header.set_size(size);
    header.set_mode(0o600);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(0);
    header.set_entry_type(tar::EntryType::Regular);
    header.set_cksum();
    Ok(header)
}
struct HashReader<R> {
    input: R,
    hash: Sha256,
    count: u64,
}
impl<R: Read> Read for HashReader<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let size = self.input.read(output)?;
        self.hash.update(&output[..size]);
        self.count += size as u64;
        Ok(size)
    }
}

/// Encrypt only an immutable, privately staged tree. Source and destination must be separate.
pub fn seal(
    root: &Path,
    destination: &Path,
    signer: &NodeSigner,
    recipient: &BackupRecipient,
    created_at: i64,
    limits: Limits,
) -> Result<SignedInventory> {
    let destination = output(destination)?;
    let parent = destination.parent().ok_or(Error::UnsafePath)?;
    let source = root.canonicalize()?;
    if destination.starts_with(&source) {
        return Err(Error::UnsafePath);
    }
    let signed = inventory::sign(inventory(root, limits)?, created_at, signer, limits)?;
    space(
        parent,
        encoded_limit(
            signed.inventory().total_bytes()?,
            signed.inventory().files.len(),
            signed.encoded().len(),
        )?,
    )?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    let encryptor =
        age::Encryptor::with_recipients(std::iter::once(recipient as &dyn age::Recipient))
            .map_err(|_| Error::Encryption)?;
    {
        let encrypted = encryptor.wrap_output(temporary.as_file_mut())?;
        let mut tar = tar::Builder::new(encrypted);
        tar.append(
            &header("inventory.cose", signed.encoded().len() as u64)?,
            signed.encoded(),
        )?;
        for record in &signed.inventory().files {
            let path = source.join(&record.path);
            let metadata = fs::symlink_metadata(&path)?;
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(Error::UnsafePath);
            }
            let mut file = File::open(path)?;
            let mut hashed = HashReader {
                input: (&mut file).take(record.size),
                hash: Sha256::new(),
                count: 0,
            };
            tar.append(
                &header(&format!("files/{}", record.path), record.size)?,
                &mut hashed,
            )?;
            let count = hashed.count;
            let hash = hex::encode(hashed.hash.finalize());
            let mut extra = [0; 1];
            if count != record.size || hash != record.sha256 || file.read(&mut extra)? != 0 {
                return Err(Error::ChangedSource);
            }
        }
        let encrypted = tar.into_inner()?;
        encrypted.finish()?;
    }
    temporary.as_file().sync_all()?;
    temporary
        .persist_noclobber(&destination)
        .map_err(|error| Error::Io(error.error))?;
    sync_directory(parent)?;
    Ok(signed)
}

struct HeaderBounded<R> {
    input: R,
    remaining: Arc<AtomicUsize>,
}
fn bounded_header(input: &mut impl BufRead) -> Result<Vec<u8>> {
    let mut prefix = vec![];
    let mut line_start = 0;
    loop {
        let available = input.fill_buf()?;
        if available.is_empty() {
            return Err(Error::Decryption);
        }
        let take = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |at| at + 1);
        if prefix.len() + take > HEADER_LIMIT || prefix.len() - line_start + take > 1024 {
            return Err(Error::LimitExceeded);
        }
        prefix.extend_from_slice(&available[..take]);
        input.consume(take);
        if prefix.last() != Some(&b'\n') {
            continue;
        }
        let line = &prefix[line_start..];
        if line_start == 0 && line != b"age-encryption.org/v1\n" {
            return Err(Error::Decryption);
        }
        if line.starts_with(b"--- ") {
            if line.len() != 48
                || !line[4..47]
                    .iter()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"+/".contains(byte))
            {
                return Err(Error::Decryption);
            }
            return Ok(prefix);
        }
        line_start = prefix.len();
    }
}
impl<R: Read> Read for HeaderBounded<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        let remaining = self.remaining.load(Ordering::Relaxed);
        if remaining == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "age header exceeds its budget",
            ));
        }
        let allowed = output.len().min(remaining);
        let size = self.input.read(&mut output[..allowed])?;
        if remaining != usize::MAX {
            self.remaining.fetch_sub(size, Ordering::Relaxed);
        }
        Ok(size)
    }
}
pub struct OpenedArchive {
    directory: PathBuf,
    inventory: SignedInventory,
}
impl OpenedArchive {
    pub fn content(&self) -> PathBuf {
        self.directory.join("content")
    }
    pub fn inventory(&self) -> &SignedInventory {
        &self.inventory
    }
}
fn regular(entry: &tar::Entry<'_, impl Read>, name: &[u8], size: u64) -> Result<()> {
    if entry.header().entry_type() != tar::EntryType::Regular
        || entry.header().as_ustar().is_none()
        || entry.path_bytes().as_ref() != name
        || entry.size() != size
    {
        return Err(Error::InvalidArchive);
    }
    Ok(())
}

/// Authenticate/decrypt to a fresh private staging directory. No restored writer is activated.
pub fn open(
    source: &Path,
    destination: &Path,
    expected: NodeId,
    identity: &BackupIdentity,
    limits: Limits,
) -> Result<OpenedArchive> {
    limits.validate()?;
    let destination = output(destination)?;
    let file = File::open(source)?;
    if !file.metadata()?.is_file()
        || file.metadata()?.len()
            > encoded_limit(
                limits.max_total_bytes,
                limits.max_files,
                limits.max_manifest + 128,
            )?
    {
        return Err(Error::LimitExceeded);
    }
    let mut file = BufReader::new(file);
    let prefix = bounded_header(&mut file)?;
    let remaining = Arc::new(AtomicUsize::new(HEADER_LIMIT));
    let decryptor = age::Decryptor::new(HeaderBounded {
        input: Cursor::new(prefix).chain(file),
        remaining: remaining.clone(),
    })
    .map_err(|_| Error::Decryption)?;
    remaining.store(usize::MAX, Ordering::Relaxed);
    let plaintext = decryptor
        .decrypt(std::iter::once(identity as &dyn age::Identity))
        .map_err(|_| Error::Decryption)?;
    let mut tar = tar::Archive::new(plaintext);
    let signed;
    let mut directories = vec![];
    {
        let mut entries = tar.entries()?.raw(true);
        let mut first = entries.next().ok_or(Error::InvalidArchive)??;
        if first.size() > limits.max_manifest as u64 + 128 {
            return Err(Error::LimitExceeded);
        }
        regular(&first, b"inventory.cose", first.size())?;
        let mut encoded = vec![];
        first.read_to_end(&mut encoded)?;
        drop(first);
        signed = inventory::verify(&encoded, expected, limits)?;
        space(
            destination.parent().ok_or(Error::UnsafePath)?,
            signed.inventory().total_bytes()?,
        )?;
        private_directory(&destination)?;
        let mut incomplete = private_file(&destination.join(".archive-incomplete"))?;
        incomplete.write_all(b"Archive verification is incomplete.\n")?;
        incomplete.sync_all()?;
        let content = destination.join("content");
        private_directory(&content)?;
        directories.push(content.clone());
        sync_directory(&destination)?;
        for record in &signed.inventory().files {
            let mut entry = entries.next().ok_or(Error::InvalidArchive)??;
            regular(
                &entry,
                format!("files/{}", record.path).as_bytes(),
                record.size,
            )?;
            let path = content.join(&record.path);
            let mut parent = content.clone();
            for component in Path::new(&record.path)
                .parent()
                .ok_or(Error::UnsafePath)?
                .components()
            {
                parent.push(component);
                if !parent.exists() {
                    private_directory(&parent)?;
                    directories.push(parent.clone());
                }
                if fs::symlink_metadata(&parent)?.file_type().is_symlink() || !parent.is_dir() {
                    return Err(Error::UnsafePath);
                }
            }
            let mut output = private_file(&path)?;
            let mut hashed = HashReader {
                input: &mut entry,
                hash: Sha256::new(),
                count: 0,
            };
            io::copy(&mut hashed, &mut output)?;
            if hashed.count != record.size || hex::encode(hashed.hash.finalize()) != record.sha256 {
                return Err(Error::InvalidArchive);
            }
            output.sync_all()?;
        }
        if entries.next().is_some() {
            return Err(Error::InvalidArchive);
        }
    }
    // Tar can stop before the encrypted stream's final authentication tag. Drain it
    // through EOF and reject appended data before publishing any completion marker.
    let mut plaintext = tar.into_inner();
    let mut tail = vec![];
    plaintext.by_ref().take(1025).read_to_end(&mut tail)?;
    if tail.len() > 1024 || tail.iter().any(|byte| *byte != 0) {
        return Err(Error::InvalidArchive);
    }
    for directory in directories.iter().rev() {
        sync_directory(directory)?;
    }
    let mut complete = private_file(&destination.join("inventory.cose"))?;
    complete.write_all(signed.encoded())?;
    complete.sync_all()?;
    sync_directory(&destination)?;
    fs::remove_file(destination.join(".archive-incomplete"))?;
    sync_directory(&destination)?;
    sync_directory(destination.parent().ok_or(Error::UnsafePath)?)?;
    Ok(OpenedArchive {
        directory: destination,
        inventory: signed,
    })
}
