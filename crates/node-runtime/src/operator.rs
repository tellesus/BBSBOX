// SPDX-License-Identifier: AGPL-3.0-or-later
//! Local cold-maintenance commands. No HTTP listener or restored writer activation.
use crate::{Config, Result, migrations};
use bbsbox_credentials::NodeId;
use bbsbox_identity::node::NodeSigner;
use bbsbox_maintenance::{BackupIdentity, BackupRecipient, Limits, archive, snapshot};
use bbsbox_storage::writer::{Config as WriterConfig, Writer};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

struct ConfigurationLock(File);
impl Drop for ConfigurationLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}
fn private_stage(parent: &Path, prefix: &str) -> Result<tempfile::TempDir> {
    let mut builder = tempfile::Builder::new();
    builder.prefix(prefix);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(0o700));
    }
    let stage = builder.tempdir_in(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if stage.path().metadata()?.permissions().mode() & 0o077 != 0 {
            return Err("Use local storage supporting private staging directories".into());
        }
    }
    Ok(stage)
}

fn read_key(path: &Path) -> Result<BackupIdentity> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 1024 {
        return Err("Backup identity must be a bounded regular file".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err("Backup identity must be private to its owner".into());
        }
    }
    let mut raw = Zeroizing::new(String::new());
    File::open(path)?.take(1025).read_to_string(&mut raw)?;
    if raw.len() > 1024 {
        return Err("Backup identity exceeds its limit".into());
    }
    raw.trim()
        .parse()
        .map_err(|_| "Invalid backup identity".into())
}
pub fn keygen(path: &Path) -> Result<serde_json::Value> {
    let identity = BackupIdentity::generate();
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if file.metadata()?.permissions().mode() & 0o077 != 0 {
            return Err("Use storage supporting a private backup identity file".into());
        }
    }
    // age's secret string is zeroized when dropped; never include it in command output.
    use bbsbox_maintenance::age_secret::ExposeSecret;
    let secret = identity.to_string();
    file.write_all(secret.expose_secret().as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    sync_parent(path)?;
    Ok(
        serde_json::json!({"format":"bbsbox.backup-recipient/v1","recipient":identity.to_public().to_string()}),
    )
}
fn sync_parent(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(
        path.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?
    .sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
fn regular(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("Expected a regular nonsymlinked source file".into());
    }
    Ok(())
}
fn configuration(config_path: &Path, config: &Config) -> Result<(PathBuf, Vec<String>)> {
    if config_path.file_name() != Some(std::ffi::OsStr::new("node.json")) {
        return Err("Use the installed node.json configuration".into());
    }
    regular(config_path)?;
    let root = config_path
        .parent()
        .ok_or("Configuration directory is missing")?
        .canonicalize()?;
    if config_path.canonicalize()? != root.join("node.json") {
        return Err("Configuration must not traverse a symlink".into());
    }
    let mut required = vec![
        "installation.json".into(),
        "node.json".into(),
        "tls/ca-key.pem".into(),
        "tls/ca.pem".into(),
    ];
    for file in [&config.certificate, &config.certificate_key] {
        regular(file)?;
        if file.canonicalize()? != *file {
            return Err("Certificate paths must not traverse symlinks".into());
        }
        let relative = file
            .strip_prefix(&root)
            .map_err(|_| "Keep node certificates inside its configuration directory")?;
        required.push(
            relative
                .components()
                .map(|part| part.as_os_str().to_str().ok_or("Invalid certificate path"))
                .collect::<std::result::Result<Vec<_>, _>>()?
                .join("/"),
        );
    }
    required.sort();
    required.dedup();
    Ok((root, required))
}
/// Caller stops/drains the service and uses a private local staging parent, never a public media root.
pub fn backup(
    config_path: &Path,
    expected: NodeId,
    recipient: &BackupRecipient,
    destination: &Path,
    staging_parent: &Path,
) -> Result<serde_json::Value> {
    let config = Config::read(config_path)?;
    let (root, required) = configuration(config_path, &config)?;
    // The configuration owner serializes this with local certificate renewal.
    let lock_path = root.join(".tls-renewal.lock");
    if lock_path
        .symlink_metadata()
        .is_ok_and(|m| !m.is_file() || m.file_type().is_symlink())
    {
        return Err("Invalid configuration maintenance lock".into());
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let lock = options.open(lock_path)?;
    lock.try_lock()?;
    let lock = ConfigurationLock(lock);
    // Read again after the lock: renewal may have completed between the first read and acquisition.
    let frozen_config = Config::read(config_path)?;
    if serde_json::to_vec(&config)? != serde_json::to_vec(&frozen_config)? {
        return Err("Configuration changed; retry backup".into());
    }
    regular(&config.database)?;
    bbsbox_storage::verify_exact_registry(&config.database, &migrations())?;
    if NodeSigner::inspect_existing(&config.database)? != expected {
        return Err("Source node does not match the expected identity".into());
    }
    let installed: serde_json::Value =
        serde_json::from_reader(File::open(root.join("installation.json"))?.take(65537))?;
    if installed["format"] != "bbsbox.development-install/v1"
        || installed["node_id"] != expected.to_string()
    {
        return Err("Installation record does not match this node".into());
    }
    let stage = private_stage(staging_parent, "bbsbox-snapshot-")?;
    let tree = stage.path().join("snapshot");
    let output = destination.to_path_buf();
    let recipient = recipient.clone();
    let created_at = i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())?;
    let writer = Writer::start(&config.database, migrations(), WriterConfig::default())?;
    let result = writer
        .handle()
        .maintenance_service(move |database| {
            snapshot::capture(
                database,
                expected,
                &root,
                required,
                &tree,
                created_at,
                &migrations(),
            )?;
            let signer = NodeSigner::start_existing(database, expected)?;
            let signed = archive::seal(
                &tree,
                &output,
                &signer,
                &recipient,
                created_at,
                Limits::default(),
            )?;
            Ok::<_, bbsbox_maintenance::Error>(signed)
        })?
        .blocking_wait();
    let shutdown = writer.shutdown();
    let signed = result?;
    shutdown?;
    drop(stage);
    drop(lock);
    Ok(
        serde_json::json!({"format":"bbsbox.node-backup/v1","node":expected.to_string(),"inventory_sha256":hex_id(signed.id()),"snapshot_time":created_at,"scope":"foundation-node","game_checkpoints":0}),
    )
}
fn hex_id(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub fn restore(
    cipher: &Path,
    expected: NodeId,
    identity_file: &Path,
    destination: &Path,
    staging_parent: &Path,
) -> Result<serde_json::Value> {
    let identity = read_key(identity_file)?;
    let stage = private_stage(staging_parent, "bbsbox-restore-")?;
    let opened = archive::open(
        cipher,
        &stage.path().join("opened"),
        expected,
        &identity,
        Limits::default(),
    )?;
    let restored = snapshot::restore(&opened, destination, &migrations())?;
    Ok(
        serde_json::json!({"format":"bbsbox.node-restore/v1","node":expected.to_string(),"inventory_sha256":hex_id(opened.inventory().id()),"state":"recovery-required","database":restored.database()}),
    )
}
pub fn command(arguments: &[OsString]) -> Result<serde_json::Value> {
    let command = arguments
        .first()
        .and_then(|s| s.to_str())
        .ok_or("Maintenance command is missing")?;
    let names: &[&str] = match command {
        "backup-keygen" => &["--output"],
        "backup" => &["--config", "--node", "--recipient", "--output", "--staging"],
        "restore" => &[
            "--archive",
            "--node",
            "--identity-file",
            "--output",
            "--staging",
        ],
        _ => return Err("Unknown maintenance command".into()),
    };
    let mut options = BTreeMap::new();
    let (chunks, remainder) = arguments[1..].as_chunks::<2>();
    if !remainder.is_empty() {
        return Err("Each maintenance option requires a value".into());
    }
    for pair in chunks {
        let name = pair[0].to_str().ok_or("Invalid option")?;
        if !names.contains(&name) || options.insert(name, pair[1].as_os_str()).is_some() {
            return Err("Unknown or duplicate maintenance option".into());
        }
    }
    if options.len() != names.len() {
        return Err("Required maintenance options are missing".into());
    }
    let path = |name| Path::new(options[name]);
    if command == "backup-keygen" {
        return keygen(path("--output"));
    }
    let expected = options["--node"]
        .to_str()
        .ok_or("Invalid node identity")?
        .parse()?;
    if command == "backup" {
        let recipient = options["--recipient"]
            .to_str()
            .ok_or("Invalid recipient")?
            .parse()
            .map_err(|_| "Invalid backup recipient")?;
        backup(
            path("--config"),
            expected,
            &recipient,
            path("--output"),
            path("--staging"),
        )
    } else {
        restore(
            path("--archive"),
            expected,
            path("--identity-file"),
            path("--output"),
            path("--staging"),
        )
    }
}
