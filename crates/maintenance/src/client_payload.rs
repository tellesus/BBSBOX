// SPDX-License-Identifier: AGPL-3.0-or-later
//! Inspect and copy complete signed Windows client payloads; no trust initialization or execution.
use crate::{
    archive,
    client_release::{self, ENTRY, SignedClient},
};
use std::{
    collections::BTreeSet,
    fs::{self, File, Metadata, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub const MANIFEST: &str = "manifest.cose";

pub(crate) fn linked(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}
pub(crate) fn directory(root: &Path) -> Result<PathBuf> {
    let root = std::path::absolute(root)?;
    for parent in root.ancestors() {
        let metadata = fs::symlink_metadata(parent)?;
        if !metadata.is_dir() || linked(&metadata) {
            return Err("client directory has a linked or non-directory ancestor".into());
        }
    }
    Ok(root.canonicalize()?)
}
pub fn read_manifest(root: &Path) -> Result<Vec<u8>> {
    let path = root.join(MANIFEST);
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.is_file() || linked(&metadata) {
        return Err("client manifest must be a regular file".into());
    }
    let mut bytes = Vec::new();
    File::open(path)?
        .take((client_release::MAX_MANIFEST + 129) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > client_release::MAX_MANIFEST + 128 {
        return Err("client manifest exceeds its limit".into());
    }
    Ok(bytes)
}
fn inventory(root: &Path, signed: &SignedClient, runtime: bool) -> Result<()> {
    let mut files = signed
        .manifest()
        .files
        .iter()
        .map(|f| PathBuf::from(&f.path))
        .collect::<BTreeSet<_>>();
    files.insert(PathBuf::from(MANIFEST));
    let mut directories = BTreeSet::new();
    for name in &files {
        for parent in name
            .ancestors()
            .skip(1)
            .filter(|p| !p.as_os_str().is_empty())
        {
            directories.insert(parent.to_path_buf());
        }
    }
    let mut pending = vec![root.to_path_buf()];
    while let Some(parent) = pending.pop() {
        for entry in fs::read_dir(parent)? {
            let entry = entry?;
            let path = entry.path();
            let relative = path.strip_prefix(root)?;
            let metadata = fs::symlink_metadata(&path)?;
            if linked(&metadata) {
                return Err("client package contains a linked entry".into());
            }
            if metadata.is_dir() && directories.remove(relative) {
                pending.push(path);
            } else if metadata.is_file() && files.remove(relative) {
            } else {
                return Err("client package has an unlisted or aliased entry".into());
            }
        }
    }
    if !files.is_empty() || !directories.is_empty() {
        return Err("client package is incomplete".into());
    }
    for record in &signed.manifest().files {
        let metadata = fs::symlink_metadata(root.join(&record.path))?;
        if !metadata.is_file() || linked(&metadata) || metadata.len() != record.size {
            return Err("client file differs from its signed commitment".into());
        }
        if runtime && client_release::MATERIALS.contains(&record.path.as_str()) {
            continue;
        }
        if archive::hash_file(&root.join(&record.path), record.size)?
            != (record.size, record.sha256.clone())
        {
            return Err("client file differs from its signed commitment".into());
        }
    }
    if read_manifest(root)? != signed.encoded() {
        return Err("client manifest changed during inspection".into());
    }
    check_executable(&root.join(ENTRY))?;
    Ok(())
}
pub fn verify(root: &Path, expected: &[u8; 32]) -> Result<SignedClient> {
    let root = directory(root)?;
    let signed = client_release::verify(&read_manifest(&root)?, expected)?;
    inventory(&root, &signed, false)?;
    Ok(signed)
}
/// Runtime-only authentication after full installation verification. The three reserved material
/// files are rebuild inputs, never runtime code; they retain name/type/size checks here. Rebuilding
/// or auditing them requires full verification through `verify` and the material source checker.
pub fn verify_runtime(root: &Path, expected: &[u8; 32]) -> Result<SignedClient> {
    let root = directory(root)?;
    let signed = client_release::verify(&read_manifest(&root)?, expected)?;
    inventory(&root, &signed, true)?;
    Ok(signed)
}
fn check_executable(path: &Path) -> Result<()> {
    let mut file = File::open(path)?;
    let mut dos = [0; 64];
    file.read_exact(&mut dos)?;
    let offset = u32::from_le_bytes(dos[60..64].try_into()?);
    if &dos[..2] != b"MZ" || !(64..=1024 * 1024).contains(&offset) {
        return Err("client is not a supported Windows executable".into());
    }
    file.seek(SeekFrom::Start(u64::from(offset)))?;
    let mut pe = [0; 26];
    file.read_exact(&mut pe)?;
    let flags = u16::from_le_bytes(pe[22..24].try_into()?);
    if &pe[..4] != b"PE\0\0"
        || pe[4..6] != [0x64, 0x86]
        || flags & 2 == 0
        || flags & 0x2000 != 0
        || pe[24..26] != [0x0b, 0x02]
    {
        return Err("client executable has an incompatible PE target".into());
    }
    Ok(())
}
/// Destination must not exist. A failed copy is retained inactive for inspection, never launched.
/// The source can change while copying: the complete fresh copy is verified independently.
pub fn stage(source: &Path, destination: &Path, expected: &[u8; 32]) -> Result<SignedClient> {
    let source = directory(source)?;
    let signed = verify(&source, expected)?;
    let parent = directory(destination.parent().ok_or("client stage needs a parent")?)?;
    let name = destination.file_name().ok_or("client stage needs a name")?;
    if name == "." || name == ".." {
        return Err("invalid client stage name".into());
    }
    let destination = parent.join(name);
    fs::create_dir(&destination)?;
    for record in &signed.manifest().files {
        let target = destination.join(&record.path);
        fs::create_dir_all(target.parent().ok_or("invalid client file path")?)?;
        let mut input = File::open(source.join(&record.path))?.take(record.size + 1);
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)?;
        if std::io::copy(&mut input, &mut output)? != record.size {
            return Err("client source changed while copying".into());
        }
        output.sync_all()?;
    }
    let mut manifest = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination.join(MANIFEST))?;
    manifest.write_all(signed.encoded())?;
    manifest.sync_all()?;
    let copied = verify(&destination, expected)?;
    if copied.id() != signed.id() {
        return Err("client stage changed".into());
    }
    Ok(copied)
}
