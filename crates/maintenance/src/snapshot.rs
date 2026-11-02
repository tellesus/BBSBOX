// SPDX-License-Identifier: AGPL-3.0-or-later
//! Foundation-node snapshots. Call capture only under drained writer/configuration maintenance.
use crate::{
    Error, Limits, Result, archive,
    inventory::{self, FileRecord, SignedInventory},
};
use bbsbox_credentials::NodeId;
use bbsbox_identity::node::NodeSigner;
use bbsbox_storage::{Database, Migration};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const FORMAT: &str = "bbsbox.foundation-node-snapshot/v1";
const REQUIRED: &[&str] = &[
    "installation.json",
    "node.json",
    "tls/ca-key.pem",
    "tls/ca.pem",
];
const DESCRIPTOR_LIMIT: usize = 65536;
fn configuration_limits() -> Limits {
    Limits {
        max_files: 120,
        max_member_bytes: 1024 * 1024,
        max_total_bytes: 8 * 1024 * 1024,
        ..Limits::default()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Descriptor {
    pub format: String,
    pub node: String,
    pub created_at: i64,
    pub configuration: Vec<FileRecord>,
    pub required_configuration: Vec<String>,
    // This profile is for the current foundation runtime, with no game workers installed.
    // M11 must integrate real worker checkpoints before this profile can include games.
    pub game_checkpoints: Vec<String>,
}
impl Descriptor {
    fn validate(&self, expected: NodeId) -> Result<()> {
        if self.format != FORMAT
            || self.node != expected.to_string()
            || !self.game_checkpoints.is_empty()
            || self.configuration.is_empty()
            || self.configuration.len() > 120
            || self.required_configuration.len() > 120
        {
            return Err(Error::IncompleteSnapshot);
        }
        let mut previous: Option<&str> = None;
        let mut total = 0u64;
        for file in &self.configuration {
            if !inventory::valid_path(&file.path)
                || file.path.starts_with('.')
                || previous.is_some_and(|p| p >= file.path.as_str())
                || file.size > configuration_limits().max_member_bytes
                || file.sha256.len() != 64
                || !file
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            {
                return Err(Error::IncompleteSnapshot);
            }
            previous = Some(&file.path);
            total = total.checked_add(file.size).ok_or(Error::LimitExceeded)?;
        }
        if total > configuration_limits().max_total_bytes {
            return Err(Error::LimitExceeded);
        }
        let names: BTreeSet<_> = self.configuration.iter().map(|f| f.path.as_str()).collect();
        let mut previous: Option<&str> = None;
        for name in &self.required_configuration {
            if !names.contains(name.as_str()) || previous.is_some_and(|p| p >= name.as_str()) {
                return Err(Error::IncompleteSnapshot);
            }
            previous = Some(name);
        }
        if REQUIRED
            .iter()
            .any(|name| !self.required_configuration.iter().any(|p| p == name))
        {
            return Err(Error::IncompleteSnapshot);
        }
        Ok(())
    }
}

fn write_new(path: &Path, data: &[u8]) -> Result<()> {
    let mut file = archive::private_file(path)?;
    file.write_all(data)?;
    file.sync_all()?;
    Ok(())
}
fn copy_records(source: &Path, destination: &Path, records: &[FileRecord]) -> Result<()> {
    archive::private_directory(destination)?;
    let mut directories = vec![destination.to_path_buf()];
    for record in records {
        if !inventory::valid_path(&record.path) {
            return Err(Error::UnsafePath);
        }
        let mut parent = destination.to_path_buf();
        for component in Path::new(&record.path)
            .parent()
            .ok_or(Error::UnsafePath)?
            .components()
        {
            parent.push(component);
            if !parent.exists() {
                archive::private_directory(&parent)?;
                directories.push(parent.clone());
            }
        }
        let input_path = source.join(&record.path);
        let metadata = fs::symlink_metadata(&input_path)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(Error::UnsafePath);
        }
        let mut input = File::open(&input_path)?;
        let path = destination.join(&record.path);
        let mut output = archive::private_file(&path)?;
        let size = std::io::copy(&mut (&mut input).take(record.size + 1), &mut output)?;
        output.sync_all()?;
        if size != record.size
            || archive::hash_file(&path, record.size)? != (record.size, record.sha256.clone())
        {
            return Err(Error::ChangedSource);
        }
    }
    for directory in directories.iter().rev() {
        archive::sync_directory(directory)?;
    }
    Ok(())
}

/// The caller owns configuration exclusion and the writer maintenance callback for this whole call.
/// Every registered migration must belong to the current foundation runtime; no worker state is inferred.
pub fn capture(
    database: &mut Database,
    expected: NodeId,
    configuration: &Path,
    required_configuration: Vec<String>,
    destination: &Path,
    created_at: i64,
    migrations: &[Migration],
) -> Result<SignedInventory> {
    if fs::symlink_metadata(configuration)?
        .file_type()
        .is_symlink()
    {
        return Err(Error::UnsafePath);
    }
    let configuration = configuration.canonicalize()?;
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()?;
    if parent.starts_with(&configuration) {
        return Err(Error::UnsafePath);
    }
    let records = archive::inventory_except(
        &configuration,
        configuration_limits(),
        &[".tls-renewal.lock"],
    )?;
    let descriptor = Descriptor {
        format: FORMAT.into(),
        node: expected.to_string(),
        created_at,
        configuration: records,
        required_configuration,
        game_checkpoints: vec![],
    };
    descriptor.validate(expected)?;
    let bytes = serde_json::to_vec(&descriptor).map_err(|_| Error::IncompleteSnapshot)?;
    if bytes.len() > DESCRIPTOR_LIMIT {
        return Err(Error::LimitExceeded);
    }
    let signer = NodeSigner::start_existing(database, expected)?;
    archive::private_directory(destination)?;
    database.snapshot_core(destination.join("core"))?;
    bbsbox_storage::verify_exact_registry(destination.join("core/core.sqlite"), migrations)?;
    copy_records(
        &configuration,
        &destination.join("configuration"),
        &descriptor.configuration,
    )?;
    // Detect additions/deletions as well as modified captured bytes while configuration is frozen.
    let after = archive::inventory_except(
        &configuration,
        configuration_limits(),
        &[".tls-renewal.lock"],
    )?;
    if after != descriptor.configuration {
        return Err(Error::ChangedSource);
    }
    write_new(&destination.join("snapshot.json"), &bytes)?;
    archive::sync_directory(destination)?;
    let signed = inventory::sign(
        archive::inventory(destination, Limits::default())?,
        created_at,
        &signer,
        Limits::default(),
    )?;
    verify(destination, &signed, migrations)?;
    // The returned inventory authenticates this stage; encrypted publication is a separate operation.
    Ok(signed)
}

/// Recheck the private stage against the authenticated archive inventory, identity and exact registry.
pub fn verify(
    root: &Path,
    signed: &SignedInventory,
    migrations: &[Migration],
) -> Result<Descriptor> {
    if archive::inventory(root, Limits::default())? != signed.inventory().files {
        return Err(Error::ChangedSource);
    }
    let mut bytes = vec![];
    File::open(root.join("snapshot.json"))?
        .take(DESCRIPTOR_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > DESCRIPTOR_LIMIT {
        return Err(Error::LimitExceeded);
    }
    let descriptor: Descriptor =
        serde_json::from_slice(&bytes).map_err(|_| Error::IncompleteSnapshot)?;
    descriptor.validate(signed.node())?;
    if serde_json::to_vec(&descriptor).map_err(|_| Error::IncompleteSnapshot)? != bytes
        || descriptor.created_at != signed.inventory().created_at
    {
        return Err(Error::IncompleteSnapshot);
    }
    let core = root.join("core");
    let manifest = bbsbox_storage::snapshot::verify_core(&core)?;
    bbsbox_storage::verify_exact_registry(core.join("core.sqlite"), migrations)?;
    if NodeSigner::inspect_existing(core.join("core.sqlite"))? != signed.node() {
        return Err(Error::WrongNode);
    }
    let mut expected: BTreeSet<String> =
        ["snapshot.json", "core/manifest.json", "core/core.sqlite"]
            .map(String::from)
            .into();
    expected.extend(
        manifest
            .blobs
            .iter()
            .map(|b| format!("core/blobs/{}", b.digest)),
    );
    for record in &descriptor.configuration {
        let path = format!("configuration/{}", record.path);
        let Some(actual) = signed.inventory().files.iter().find(|f| f.path == path) else {
            return Err(Error::IncompleteSnapshot);
        };
        if actual.size != record.size || actual.sha256 != record.sha256 {
            return Err(Error::IncompleteSnapshot);
        }
        expected.insert(path);
    }
    let actual: BTreeSet<_> = signed
        .inventory()
        .files
        .iter()
        .map(|f| f.path.clone())
        .collect();
    if expected != actual {
        return Err(Error::IncompleteSnapshot);
    }
    Ok(descriptor)
}

pub struct RestoredSnapshot {
    directory: PathBuf,
    database: PathBuf,
}
impl RestoredSnapshot {
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub fn database(&self) -> &Path {
        &self.database
    }
    pub fn configuration(&self) -> PathBuf {
        self.directory.join("configuration")
    }
}
/// Restore only into a fresh private directory. Configuration is staged, never installed or activated.
pub fn restore(
    opened: &archive::OpenedArchive,
    destination: &Path,
    migrations: &[Migration],
) -> Result<RestoredSnapshot> {
    let source = opened.content();
    let descriptor = verify(&source, opened.inventory(), migrations)?;
    archive::private_directory(destination)?;
    write_new(
        &destination.join(".restore-incomplete"),
        b"Isolated restore has not completed.\n",
    )?;
    archive::sync_directory(destination)?;
    let database =
        bbsbox_storage::snapshot::restore_core(source.join("core"), destination.join("data"))?;
    copy_records(
        &source.join("configuration"),
        &destination.join("configuration"),
        &descriptor.configuration,
    )?;
    write_new(&destination.join("recovery.json"),&serde_json::to_vec(&serde_json::json!({
        "format":"bbsbox.isolated-node-restore/v1","node":descriptor.node,
        "inventory_sha256":hex::encode(opened.inventory().id()),"snapshot_time":descriptor.created_at,
        "state":"recovery-required","post_snapshot_writes":"not-reconciled"
    })).map_err(|_|Error::IncompleteSnapshot)?)?;
    // Verify copied node identity/registry without opening a writer or applying any migrations.
    bbsbox_storage::verify_exact_registry(&database, migrations)?;
    if NodeSigner::inspect_existing(&database)? != opened.inventory().node() {
        return Err(Error::WrongNode);
    }
    archive::sync_directory(destination)?;
    fs::remove_file(destination.join(".restore-incomplete"))?;
    archive::sync_directory(destination)?;
    archive::sync_directory(
        destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?;
    Ok(RestoredSnapshot {
        directory: destination.to_path_buf(),
        database,
    })
}
