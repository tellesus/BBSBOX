// SPDX-License-Identifier: AGPL-3.0-or-later
//! Complete checkpoint-set profile. Actual game package/worker transport is owned by M11.
use crate::{
    Error, Limits, Result, archive,
    checkpoints::{self, CheckpointRecord, Registration},
    inventory::{self, SignedInventory},
    snapshot::{self, Descriptor, Profile, RestoredSnapshot},
};
use bbsbox_credentials::NodeId;
use bbsbox_identity::node::NodeSigner;
use bbsbox_storage::{Database, Migration};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{Read, Write},
    path::Path,
};

const INDEX: &str = "game-checkpoints.json";
const RECOVERY_REPORT: &str = "game-recovery.json";
const MAX_INDEX: usize = 8 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Requirement {
    pub game_id: String,
    pub worker_version: String,
    pub state_schema: u32,
}
fn requirements(games: &[Requirement]) -> Result<BTreeMap<&str, &Requirement>> {
    if games.is_empty() || games.len() > 64 {
        return Err(Error::CheckpointUnavailable);
    }
    let mut result = BTreeMap::new();
    for game in games {
        if !checkpoints::id(&game.game_id)
            || !checkpoints::label(&game.worker_version)
            || game.state_schema == 0
            || result.insert(game.game_id.as_str(), game).is_some()
        {
            return Err(Error::CheckpointUnavailable);
        }
    }
    Ok(result)
}
fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = archive::private_file(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
fn read_index(path: &Path) -> Result<Vec<CheckpointRecord>> {
    let mut bytes = vec![];
    File::open(path)?
        .take(MAX_INDEX as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_INDEX {
        return Err(Error::LimitExceeded);
    }
    let records: Vec<CheckpointRecord> =
        serde_json::from_slice(&bytes).map_err(|_| Error::IncompleteSnapshot)?;
    if serde_json::to_vec(&records).map_err(|_| Error::IncompleteSnapshot)? != bytes {
        return Err(Error::IncompleteSnapshot);
    }
    Ok(records)
}

/// Requirements come from trusted installed packages/registry, independently of archive metadata.
pub fn verify(
    root: &Path,
    signed: &SignedInventory,
    migrations: &[Migration],
    games: &[Requirement],
) -> Result<(Descriptor, Vec<CheckpointRecord>)> {
    let required = requirements(games)?;
    let (descriptor, mut paths) = snapshot::verify_parts(root, signed, migrations, Profile::Games)?;
    let expected_ids: Vec<_> = required.keys().copied().collect();
    if descriptor
        .game_checkpoints
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        != expected_ids
    {
        return Err(Error::CheckpointUnavailable);
    }
    let records = read_index(&root.join(INDEX))?;
    if records
        .iter()
        .map(|r| r.game_id.as_str())
        .collect::<Vec<_>>()
        != expected_ids
    {
        return Err(Error::IncompleteSnapshot);
    }
    let actual: BTreeMap<_, _> = signed
        .inventory()
        .files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect();
    paths.insert(INDEX.into());
    let mut request = None;
    let mut total = 0u64;
    let mut count = 0usize;
    for record in &records {
        let package = required[record.game_id.as_str()];
        if record.worker_version != package.worker_version
            || record.state_schema != package.state_schema
        {
            return Err(Error::UnsupportedCheckpoint {
                game_id: record.game_id.clone(),
            });
        }
        if record.request == [0; 16]
            || request.is_some_and(|known| known != record.request)
            || !checkpoints::label(&record.checkpoint_id)
            || record.files.is_empty()
        {
            return Err(Error::IncompleteSnapshot);
        }
        request = Some(record.request);
        let mut previous: Option<&str> = None;
        for file in &record.files {
            if !inventory::valid_path(&file.path)
                || previous.is_some_and(|p| p >= file.path.as_str())
            {
                return Err(Error::UnsafePath);
            }
            previous = Some(&file.path);
            let path = format!("games/{}/{}", record.game_id, file.path);
            if !inventory::valid_path(&path) {
                return Err(Error::UnsafePath);
            }
            let member = actual.get(path.as_str()).ok_or(Error::IncompleteSnapshot)?;
            if member.size != file.size || member.sha256 != file.sha256 {
                return Err(Error::IncompleteSnapshot);
            }
            total = total.checked_add(file.size).ok_or(Error::LimitExceeded)?;
            count += 1;
            if total > Limits::default().max_total_bytes || count > Limits::default().max_files {
                return Err(Error::LimitExceeded);
            }
            paths.insert(path);
        }
    }
    let actual: BTreeSet<_> = actual.keys().map(|path| (*path).to_string()).collect();
    if actual != paths {
        return Err(Error::IncompleteSnapshot);
    }
    Ok((descriptor, records))
}

pub struct Capture<'a> {
    pub expected_node: NodeId,
    pub configuration: &'a Path,
    pub required_configuration: Vec<String>,
    pub created_at: i64,
    pub migrations: &'a [Migration],
    pub games: &'a [Requirement],
    pub limits: Limits,
}
/// Caller holds drained core maintenance/configuration exclusion. Output is published while all
/// workers are still paused. A resume failure retains the completed archive and reports its fence.
pub fn capture_and_seal(
    database: &mut Database,
    request: Capture<'_>,
    workers: &mut [Registration<'_>],
    staging: &Path,
    destination: &Path,
    recipient: &crate::BackupRecipient,
) -> Result<SignedInventory> {
    let required = requirements(request.games)?;
    let installed = required
        .keys()
        .map(|s| (*s).to_string())
        .collect::<Vec<_>>();
    request.limits.validate()?;
    if fs::symlink_metadata(request.configuration)?
        .file_type()
        .is_symlink()
    {
        return Err(Error::UnsafePath);
    }
    let config_root = request.configuration.canonicalize()?;
    let parent = staging
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()?;
    if parent.starts_with(&config_root) {
        return Err(Error::UnsafePath);
    }
    archive::private_directory(staging)?;
    checkpoints::with_checkpoints(
        &installed,
        workers,
        &staging.join("games"),
        request.limits,
        |records| {
            // Use the already verified foundation provider under the same worker/core barrier.
            let temporary = tempfile::Builder::new()
                .prefix("foundation-")
                .tempdir_in(staging)?;
            let foundation = temporary.path().join("snapshot");
            let captured = snapshot::capture(
                database,
                request.expected_node,
                request.configuration,
                request.required_configuration,
                &foundation,
                request.created_at,
                request.migrations,
            )?;
            let mut descriptor = snapshot::verify(&foundation, &captured, request.migrations)?;
            descriptor.format = Profile::Games.format().into();
            descriptor.game_checkpoints = installed.clone();
            fs::rename(foundation.join("core"), staging.join("core"))?;
            fs::rename(
                foundation.join("configuration"),
                staging.join("configuration"),
            )?;
            let descriptor_bytes =
                serde_json::to_vec(&descriptor).map_err(|_| Error::IncompleteSnapshot)?;
            if descriptor_bytes.len() > 65536 {
                return Err(Error::LimitExceeded);
            }
            let index = serde_json::to_vec(records).map_err(|_| Error::IncompleteSnapshot)?;
            if index.len() > MAX_INDEX {
                return Err(Error::LimitExceeded);
            }
            archive::space(staging, (descriptor_bytes.len() + index.len()) as u64)?;
            write(&staging.join("snapshot.json"), &descriptor_bytes)?;
            write(&staging.join(INDEX), &index)?;
            temporary.close()?;
            archive::sync_directory(staging)?;
            let signer = NodeSigner::start_existing(database, request.expected_node)?;
            let preview = inventory::sign(
                archive::inventory(staging, request.limits)?,
                request.created_at,
                &signer,
                request.limits,
            )?;
            verify(staging, &preview, request.migrations, request.games)?;
            archive::seal(
                staging,
                destination,
                &signer,
                recipient,
                request.created_at,
                request.limits,
            )
        },
    )
}

pub struct RestoredNode {
    node: RestoredSnapshot,
    games: Vec<CheckpointRecord>,
}
impl RestoredNode {
    pub fn node(&self) -> &RestoredSnapshot {
        &self.node
    }
    pub fn games(&self) -> &[CheckpointRecord] {
        &self.games
    }
}

fn recovery_report(records: &[CheckpointRecord]) -> Result<Vec<u8>> {
    serde_json::to_vec(&serde_json::json!({
        "format":"bbsbox.isolated-game-restore/v1","state":"recovery-required","checkpoints":records
    }))
    .map_err(|_| Error::IncompleteSnapshot)
}

pub(crate) fn verify_restored(restored: &RestoredNode, records: &[CheckpointRecord]) -> Result<()> {
    if restored.games() != records {
        return Err(Error::IncompleteSnapshot);
    }
    let root = restored.node().directory();
    let games = root.join("games");
    if !fs::symlink_metadata(&games)?.file_type().is_dir() {
        return Err(Error::UnsafePath);
    }
    let mut names = fs::read_dir(&games)?
        .take(records.len() + 1)
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<std::io::Result<Vec<_>>>()?;
    names.sort();
    if !names.iter().map(|name| name.as_os_str()).eq(records
        .iter()
        .map(|record| std::ffi::OsStr::new(&record.game_id)))
    {
        return Err(Error::IncompleteSnapshot);
    }
    let mut expected = records
        .iter()
        .flat_map(|record| {
            record.files.iter().map(|file| {
                let mut member = file.clone();
                member.path = format!("{}/{}", record.game_id, file.path);
                member
            })
        })
        .collect::<Vec<_>>();
    expected.sort_by(|a, b| a.path.cmp(&b.path));
    if archive::inventory(&games, Limits::default())? != expected {
        return Err(Error::IncompleteSnapshot);
    }
    let report_path = root.join(RECOVERY_REPORT);
    let report = recovery_report(records)?;
    let metadata = fs::symlink_metadata(&report_path)?;
    if !metadata.file_type().is_file() || metadata.len() != report.len() as u64 {
        return Err(Error::IncompleteSnapshot);
    }
    let mut observed = Vec::with_capacity(report.len());
    File::open(report_path)?
        .take(report.len() as u64 + 1)
        .read_to_end(&mut observed)?;
    if observed != report {
        return Err(Error::IncompleteSnapshot);
    }
    Ok(())
}
/// Stage all core/configuration/game data privately. No worker, signer or service is activated.
pub fn restore(
    opened: &archive::OpenedArchive,
    destination: &Path,
    migrations: &[Migration],
    games: &[Requirement],
) -> Result<RestoredNode> {
    let source = opened.content();
    let (descriptor, records) = verify(&source, opened.inventory(), migrations, games)?;
    let node = snapshot::restore_base(
        &source,
        &descriptor,
        opened.inventory(),
        destination,
        migrations,
    )?;
    let target = destination.join("games");
    archive::private_directory(&target)?;
    for record in &records {
        snapshot::copy_records(
            &source.join("games").join(&record.game_id),
            &target.join(&record.game_id),
            &record.files,
        )?;
    }
    write(
        &destination.join(RECOVERY_REPORT),
        &recovery_report(&records)?,
    )?;
    archive::sync_directory(&target)?;
    snapshot::finish_restore(destination)?;
    Ok(RestoredNode {
        node,
        games: records,
    })
}
