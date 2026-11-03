// SPDX-License-Identifier: AGPL-3.0-or-later
//! Release bytes and compiled-profile checks; no installation or persistent trust changes.
use crate::{
    archive,
    release::{self, Manifest, ModuleMigration, SignedRelease},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub const PROFILE: &str = "bbsbox.release-runtime/v1";
pub const MANIFEST: &str = "manifest.cose";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeProfile {
    pub format: String,
    pub version: String,
    pub target: String,
    pub core_schema: u32,
    pub readable_core_schemas: Vec<u32>,
    pub module_migrations: Vec<ModuleMigration>,
}
impl RuntimeProfile {
    /// Compare declarations with an independently obtained compiled candidate profile.
    /// This does not prove compatibility with the currently installed data or authorize execution.
    pub fn matches(&self, manifest: &Manifest) -> bool {
        self.format == PROFILE
            && self.version == manifest.version
            && self.target == manifest.target
            && self.core_schema == manifest.core_schema
            && self.readable_core_schemas == manifest.readable_core_schemas
            && self.module_migrations == manifest.module_migrations
    }
}

fn directory(root: &Path) -> Result<PathBuf> {
    let root = std::path::absolute(root)?;
    // Reject links in every supplied component, including ancestors of the bundle itself.
    for component in root.ancestors() {
        let metadata = fs::symlink_metadata(component)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err("release directory has a linked or non-directory component".into());
        }
    }
    Ok(root.canonicalize()?)
}
fn read_manifest(root: &Path) -> Result<Vec<u8>> {
    let path = root.join(MANIFEST);
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("release manifest must be a regular file".into());
    }
    let mut bytes = vec![];
    File::open(path)?
        .take((release::MAX_MANIFEST + 129) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > release::MAX_MANIFEST + 128 {
        return Err("release envelope is oversized".into());
    }
    Ok(bytes)
}
fn check_files(root: &Path, signed: &SignedRelease, manifest_present: bool) -> Result<()> {
    let manifest = signed.manifest();
    let mut expected_files = manifest
        .files
        .iter()
        .map(|f| f.path.as_str())
        .collect::<BTreeSet<_>>();
    if expected_files.contains(MANIFEST) {
        return Err("release cannot inventory its own manifest".into());
    }
    if manifest_present {
        expected_files.insert(MANIFEST);
    }
    let mut expected_directories = BTreeSet::new();
    for name in &expected_files {
        let mut path = Path::new(name).parent();
        while let Some(parent) = path.filter(|p| !p.as_os_str().is_empty()) {
            expected_directories.insert(parent.to_path_buf());
            path = parent.parent();
        }
    }
    let mut remaining_files = expected_files;
    let mut remaining_directories = expected_directories;
    let mut pending = vec![root.to_path_buf()];
    // Traverse names/types completely before opening any payload member.
    while let Some(parent) = pending.pop() {
        for entry in fs::read_dir(parent)? {
            let entry = entry?;
            let path = entry.path();
            let relative = path.strip_prefix(root)?;
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                return Err("release contains a symlink".into());
            }
            if metadata.is_dir() && remaining_directories.remove(relative) {
                pending.push(path);
            } else if metadata.is_file() {
                let name = relative
                    .components()
                    .map(|part| part.as_os_str().to_str().ok_or("non-UTF8 release path"))
                    .collect::<std::result::Result<Vec<_>, _>>()?
                    .join("/");
                if !remaining_files.remove(name.as_str()) {
                    return Err("release contains an unlisted file".into());
                }
            } else {
                return Err("release contains an unlisted directory or special entry".into());
            }
        }
    }
    if !remaining_files.is_empty() || !remaining_directories.is_empty() {
        return Err("release payload is incomplete".into());
    }
    for record in &manifest.files {
        if archive::hash_file(&root.join(&record.path), record.size)?
            != (record.size, record.sha256.clone())
        {
            return Err("release file does not match its signed commitment".into());
        }
    }
    check_elf(root)?;
    if manifest_present && read_manifest(root)? != signed.encoded() {
        return Err("release manifest changed during verification".into());
    }
    Ok(())
}
fn check_elf(root: &Path) -> Result<()> {
    let mut header = [0; 64];
    File::open(root.join("bbsbox-node"))?.read_exact(&mut header)?;
    if &header[..7] != b"\x7fELF\x02\x01\x01"
        || !matches!(header[7], 0 | 3)
        || !matches!(&header[16..18], [2, 0] | [3, 0])
        || header[18..20] != [0x3e, 0]
    {
        return Err("release binary must be ELF64 little-endian amd64".into());
    }
    Ok(())
}

/// Boot preflight for a previously completed release receipt, not full candidate acceptance.
/// Large source/dependency materials are checked at staging; boot checks exact manifest/binary.
pub(crate) fn verify_selected_binary(
    root: &Path,
    expected_key: &[u8; 32],
) -> Result<SignedRelease> {
    let root = directory(root)?;
    let signed = release::verify_manifest(&read_manifest(&root)?, expected_key)?;
    let binary = signed
        .manifest()
        .files
        .iter()
        .find(|r| r.path == "bbsbox-node")
        .ok_or("release binary commitment missing")?;
    if archive::hash_file(&root.join("bbsbox-node"), binary.size)?
        != (binary.size, binary.sha256.clone())
    {
        return Err("selected release binary changed".into());
    }
    check_elf(&root)?;
    if read_manifest(&root)? != signed.encoded() {
        return Err("selected release manifest changed".into());
    }
    Ok(signed)
}

/// Source and ancestors must remain under the caller's control throughout inspection/copy.
/// A successful result authenticates this read, not later reads from mutable media.
pub fn verify(root: &Path, expected_key: &[u8; 32]) -> Result<SignedRelease> {
    let root = directory(root)?;
    let signed = release::verify_manifest(&read_manifest(&root)?, expected_key)?;
    check_files(&root, &signed, true)?;
    Ok(signed)
}

/// Producer preflight before publishing the signature file; does not inspect material semantics.
pub fn verify_unsigned(root: &Path, signed: &SignedRelease) -> Result<()> {
    check_files(&directory(root)?, signed, false)
}

/// Inventory a producer-controlled unsigned directory before adding its manifest.
pub fn inventory_unsigned(root: &Path) -> Result<Vec<crate::inventory::FileRecord>> {
    let root = directory(root)?;
    match fs::symlink_metadata(root.join(MANIFEST)) {
        Ok(_) => return Err("release manifest already exists".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.into()),
    }
    Ok(archive::inventory(
        &root,
        crate::Limits {
            max_files: 128,
            max_manifest: release::MAX_MANIFEST,
            max_member_bytes: 2 * 1024 * 1024 * 1024,
            max_total_bytes: 4 * 1024 * 1024 * 1024,
        },
    )?)
}

/// Copy into a fresh private directory, rechecking every copied byte and exact inventory.
/// The caller controls the destination parent and must retain that protection until installation.
/// Failed output is disposable and has no installation authority.
pub fn stage(source: &Path, expected_key: &[u8; 32], destination: &Path) -> Result<SignedRelease> {
    let source = directory(source)?;
    let signed = verify(&source, expected_key)?;
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = directory(parent)?;
    let destination = parent.join(
        destination
            .file_name()
            .ok_or("missing release destination name")?,
    );
    if destination.starts_with(&source) || source.starts_with(&destination) {
        return Err("release staging must be outside its source".into());
    }
    archive::space(
        &parent,
        signed.manifest().files.iter().map(|f| f.size).sum::<u64>() + signed.encoded().len() as u64,
    )?;
    archive::private_directory(&destination)?;
    for record in &signed.manifest().files {
        let target = destination.join(&record.path);
        let mut directories = vec![];
        let mut parent = target.parent().unwrap();
        while parent != destination && !parent.exists() {
            directories.push(parent.to_path_buf());
            parent = parent.parent().unwrap();
        }
        for parent in directories.iter().rev() {
            archive::private_directory(parent)?;
        }
        let mut input = File::open(source.join(&record.path))?;
        let mut output = archive::private_file(&target)?;
        let copied = std::io::copy(&mut (&mut input).take(record.size + 1), &mut output)?;
        if copied != record.size {
            return Err("release source changed while copying".into());
        }
        output.sync_all()?;
    }
    let mut manifest = archive::private_file(&destination.join(MANIFEST))?;
    manifest.write_all(signed.encoded())?;
    manifest.sync_all()?;
    drop(manifest);
    let verified = verify(&destination, expected_key)?;
    // Persist nested directory entries before their parent entries.
    let mut directories = BTreeSet::new();
    for record in &verified.manifest().files {
        let mut parent = destination
            .join(&record.path)
            .parent()
            .unwrap()
            .to_path_buf();
        while parent.starts_with(&destination) {
            directories.insert(parent.clone());
            if parent == destination {
                break;
            }
            parent = parent.parent().unwrap().to_path_buf();
        }
    }
    for directory in directories.iter().rev() {
        archive::sync_directory(directory)?;
    }
    archive::sync_directory(&parent)?;
    Ok(verified)
}
