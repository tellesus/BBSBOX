// SPDX-License-Identifier: AGPL-3.0-or-later
//! Local cold-maintenance commands. Restored continuation stays isolated; no HTTP listener.
use crate::{Config, Result, migrations};
use bbsbox_credentials::NodeId;
use bbsbox_identity::node::NodeSigner;
use bbsbox_maintenance::{
    BackupIdentity, BackupRecipient, Limits, archive, checkpoints, game_snapshot, snapshot,
};
use bbsbox_storage::writer::{Config as WriterConfig, Priority, Writer};
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
fn foundation_registry(
    registry: Vec<bbsbox_storage::Migration>,
) -> Result<Vec<bbsbox_storage::Migration>> {
    if registry
        .iter()
        .any(|m| !bbsbox_maintenance::foundation_owner(m.module))
    {
        return Err(
            "This backup profile must be extended for the installed modules/checkpoints".into(),
        );
    }
    Ok(registry)
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
    backup_with_registry(
        config_path,
        expected,
        recipient,
        destination,
        staging_parent,
        migrations(),
    )
}
pub fn backup_with_registry(
    config_path: &Path,
    expected: NodeId,
    recipient: &BackupRecipient,
    destination: &Path,
    staging_parent: &Path,
    registry: Vec<bbsbox_storage::Migration>,
) -> Result<serde_json::Value> {
    backup_with_scope(
        config_path,
        expected,
        recipient,
        destination,
        staging_parent,
        registry,
        false,
    )
}

fn backup_with_scope(
    config_path: &Path,
    expected: NodeId,
    recipient: &BackupRecipient,
    destination: &Path,
    staging_parent: &Path,
    registry: Vec<bbsbox_storage::Migration>,
    with_games: bool,
) -> Result<serde_json::Value> {
    let config = Config::read(config_path)?;
    let registry = foundation_registry(registry)?;
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
    let database_path = config.database.canonicalize()?;
    let installed: serde_json::Value =
        serde_json::from_reader(File::open(root.join("installation.json"))?.take(65537))?;
    if !matches!(
        installed["format"].as_str(),
        Some("bbsbox.development-install/v1" | "bbsbox.install/v1")
    ) || installed["node_id"] != expected.to_string()
    {
        return Err("Installation record does not match this node".into());
    }
    let appliance = installed["format"] == "bbsbox.install/v1"
        && crate::games::is_appliance_layout(&root, &database_path);
    if with_games {
        let data = database_path
            .parent()
            .ok_or("missing node data directory")?;
        if database_path != config.database || data.starts_with(&root) || root.starts_with(data) {
            return Err("use separate direct paths for game-node data and configuration".into());
        }
        let output_parent = destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        for path in [staging_parent, output_parent] {
            let path = path.canonicalize()?;
            if path.starts_with(data)
                || path.starts_with(&root)
                || (appliance && path.starts_with(crate::games::APPLIANCE_STATE))
            {
                return Err(
                    "keep game-node staging and backup output outside live data and configuration"
                        .into(),
                );
            }
        }
    }
    if !with_games
        && (config.game_origin.is_some()
            || crate::games::has_retained_state(&database_path)?
            || (appliance && crate::games::has_appliance_state()?))
    {
        return Err("this node has game state; use backup-game-node".into());
    }
    bbsbox_storage::verify_exact_registry(&config.database, &registry)?;
    if NodeSigner::inspect_existing(&config.database)? != expected {
        return Err("Source node does not match the expected identity".into());
    }
    let stage = private_stage(staging_parent, "bbsbox-snapshot-")?;
    let tree = stage.path().join("snapshot");
    let output = destination.to_path_buf();
    let recipient = recipient.clone();
    let created_at = i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())?;
    let writer = Writer::start(&config.database, registry.clone(), WriterConfig::default())?;
    let game = if with_games {
        let manifest = crate::games::bundled_manifest()?;
        let (state_root, worker) = if appliance {
            if crate::games::has_retained_state(&database_path)? {
                return Err("additional game state prevents a complete appliance backup".into());
            }
            crate::games::appliance_checkpoint(&manifest)?
        } else {
            crate::games::bundled_worker(&database_path, &manifest, false)?
        };
        Some((game_requirement(&manifest), state_root, worker))
    } else {
        None
    };
    let result = writer
        .handle()
        .maintenance_service(move |database| {
            let limits = Limits::default();
            if let Some((game, state_root, mut worker)) = game {
                let games = [game];
                let mut workers = [checkpoints::Registration {
                    game_id: &games[0].game_id,
                    state_root: &state_root,
                    worker: &mut worker,
                }];
                game_snapshot::capture_and_seal(
                    database,
                    game_snapshot::Capture {
                        expected_node: expected,
                        configuration: &root,
                        required_configuration: required,
                        created_at,
                        migrations: &registry,
                        games: &games,
                        limits,
                    },
                    &mut workers,
                    &tree,
                    &output,
                    &recipient,
                )
            } else {
                snapshot::capture(
                    database, expected, &root, required, &tree, created_at, &registry,
                )?;
                let signer = NodeSigner::start_existing(database, expected)?;
                archive::seal(&tree, &output, &signer, &recipient, created_at, limits)
            }
        })?
        .blocking_wait();
    let shutdown = writer.shutdown();
    let signed = result?;
    shutdown?;
    drop(stage);
    drop(lock);
    Ok(
        serde_json::json!({"format":"bbsbox.node-backup/v1","node":expected.to_string(),"inventory_sha256":hex_id(signed.id()),"snapshot_time":created_at,"scope":if with_games { "game-node" } else { "foundation-node" },"game_checkpoints":usize::from(with_games)}),
    )
}
fn game_requirement(manifest: &bbsbox_game_host::Manifest) -> game_snapshot::Requirement {
    game_snapshot::Requirement {
        game_id: manifest.game_id().into(),
        worker_version: manifest.worker_version().into(),
        state_schema: manifest.state_schema(),
    }
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
    restore_with_registry(
        cipher,
        expected,
        identity_file,
        destination,
        staging_parent,
        migrations(),
        None,
    )
}
pub fn restore_with_registry(
    cipher: &Path,
    expected: NodeId,
    identity_file: &Path,
    destination: &Path,
    staging_parent: &Path,
    registry: Vec<bbsbox_storage::Migration>,
    authorization: Option<bbsbox_maintenance::continuation::Authorization>,
) -> Result<serde_json::Value> {
    restore_with_settings(
        cipher,
        expected,
        identity_file,
        destination,
        staging_parent,
        RestoreSettings {
            registry,
            authorization,
            known_prefix: false,
            with_games: false,
        },
    )
}
struct RestoreSettings {
    registry: Vec<bbsbox_storage::Migration>,
    authorization: Option<bbsbox_maintenance::continuation::Authorization>,
    known_prefix: bool,
    with_games: bool,
}
fn restore_with_settings(
    cipher: &Path,
    expected: NodeId,
    identity_file: &Path,
    destination: &Path,
    staging_parent: &Path,
    settings: RestoreSettings,
) -> Result<serde_json::Value> {
    let registry = foundation_registry(settings.registry)?;
    let identity = read_key(identity_file)?;
    let stage = private_stage(staging_parent, "bbsbox-restore-")?;
    let opened = archive::open(
        cipher,
        &stage.path().join("opened"),
        expected,
        &identity,
        Limits::default(),
    )?;
    if settings.with_games {
        let games = [game_requirement(&crate::games::bundled_manifest()?)];
        let restored = game_snapshot::restore(&opened, destination, &registry, &games)?;
        if let Some(authorization) = settings.authorization {
            let decision = bbsbox_maintenance::continuation::prepare_game_node(
                &restored,
                &opened,
                &registry,
                &games,
                authorization,
            )?
            .record_game_node()?;
            return Ok(
                serde_json::json!({"format":"bbsbox.game-node-recovery-record/v1","node":expected.to_string(),"inventory_sha256":hex_id(opened.inventory().id()),"decision_sha256":hex_id(decision.id()),"state":"recovery-required","game_checkpoints":restored.games().len()}),
            );
        }
        return Ok(
            serde_json::json!({"format":"bbsbox.node-restore/v1","node":expected.to_string(),"inventory_sha256":hex_id(opened.inventory().id()),"state":"recovery-required","database":restored.node().database(),"scope":"game-node","game_checkpoints":restored.games().len()}),
        );
    }
    let current_count = registry.len();
    let registry = if settings.known_prefix {
        let applied =
            bbsbox_storage::applied_registry(opened.content().join("core/core.sqlite"), &registry)?;
        if applied.iter().filter(|m| m.module == "identity").count()
            != registry.iter().filter(|m| m.module == "identity").count()
        {
            return Err("This foundation reader requires its current identity migrations".into());
        }
        applied
    } else {
        registry
    };
    let restored = snapshot::restore(&opened, destination, &registry)?;
    if let Some(authorization) = settings.authorization {
        let decision = bbsbox_maintenance::continuation::prepare(
            &restored,
            &opened,
            &registry,
            authorization,
        )?
        .finish_isolated()?;
        if settings.known_prefix {
            let applied = registry
                .iter()
                .map(bbsbox_maintenance::release::ModuleMigration::from)
                .collect::<Vec<_>>();
            return Ok(
                serde_json::json!({"format":"bbsbox.compatible-continuation/v1","node":expected.to_string(),"inventory_sha256":hex_id(opened.inventory().id()),"decision_sha256":hex_id(decision.id()),"isolated_continuation_authorized":true,"source_registry":applied,"migration_required":registry.len()!=current_count,"installed":false,"post_snapshot_writes":"not-reconciled","client_recovery":"separate"}),
            );
        }
        return Ok(
            serde_json::json!({"format":"bbsbox.isolated-continuation/v1","node":expected.to_string(),"inventory_sha256":hex_id(opened.inventory().id()),"decision_sha256":hex_id(decision.id()),"isolated_writer_ready":true,"installed":false,"post_snapshot_writes":"not-reconciled","client_recovery":"separate"}),
        );
    }
    Ok(
        serde_json::json!({"format":"bbsbox.node-restore/v1","node":expected.to_string(),"inventory_sha256":hex_id(opened.inventory().id()),"state":"recovery-required","database":restored.database()}),
    )
}
pub fn migrate_existing(
    database: &Path,
    expected: NodeId,
    registry: Vec<bbsbox_storage::Migration>,
) -> Result<serde_json::Value> {
    regular(database)?;
    if NodeSigner::inspect_existing(database)? != expected {
        return Err("Migration source does not match the expected node".into());
    }
    let writer = Writer::start(database, registry, WriterConfig::default())?;
    let result = writer
        .handle()
        .submit_service(Priority::Interactive, move |database| {
            NodeSigner::start_existing(database, expected)
        })?
        .blocking_wait();
    let shutdown = writer.shutdown();
    result?;
    shutdown?;
    Ok(
        serde_json::json!({"format":"bbsbox.migration-existing/v1","node":expected.to_string(),"core_schema":bbsbox_storage::schema_version(),"listener_opened":false}),
    )
}
pub fn command(arguments: &[OsString]) -> Result<serde_json::Value> {
    command_with_registry(arguments, migrations())
}
/// First-party compiled registries only; no migration SQL is accepted from command input.
pub fn command_with_registry(
    arguments: &[OsString],
    registry: Vec<bbsbox_storage::Migration>,
) -> Result<serde_json::Value> {
    let command = arguments
        .first()
        .and_then(|s| s.to_str())
        .ok_or("Maintenance command is missing")?;
    let names: &[&str] = match command {
        "backup-keygen" => &["--output"],
        "backup-public" => &["--identity-file"],
        "backup-check-recipient" => &["--recipient"],
        "migrate-existing" => &["--database", "--node"],
        "backup" | "backup-game-node" => {
            &["--config", "--node", "--recipient", "--output", "--staging"]
        }
        "restore" | "restore-game-node" => &[
            "--archive",
            "--node",
            "--identity-file",
            "--output",
            "--staging",
        ],
        "restore-and-authorize" | "restore-compatible" | "restore-game-node-and-record" => &[
            "--archive",
            "--node",
            "--identity-file",
            "--output",
            "--staging",
            "--source",
            "--reason",
            "--at",
        ],
        "finish-game-placement" => &[
            "--archive",
            "--node",
            "--identity-file",
            "--staging",
            "--staged",
            "--placed",
            "--decision-sha256",
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
    if command == "backup-public" {
        return Ok(
            serde_json::json!({"format":"bbsbox.backup-recipient/v1","recipient":read_key(path("--identity-file"))?.to_public().to_string()}),
        );
    }
    let recipient = || -> Result<BackupRecipient> {
        options["--recipient"]
            .to_str()
            .ok_or("Invalid backup recipient")?
            .parse()
            .map_err(|_| "Invalid backup recipient".into())
    };
    if command == "backup-check-recipient" {
        return Ok(serde_json::json!({
            "format": "bbsbox.backup-recipient/v1",
            "recipient": recipient()?.to_string(),
        }));
    }
    let expected = options["--node"]
        .to_str()
        .ok_or("Invalid node identity")?
        .parse()?;
    if command == "migrate-existing" {
        return migrate_existing(path("--database"), expected, registry);
    }
    if command == "finish-game-placement" {
        let registry = foundation_registry(registry)?;
        let identity = read_key(path("--identity-file"))?;
        let mut decision = [0; 32];
        let digest = options["--decision-sha256"]
            .to_str()
            .ok_or("Invalid recovery decision digest")?;
        hex::decode_to_slice(digest, &mut decision)?;
        if hex_id(&decision) != digest {
            return Err("Invalid recovery decision digest".into());
        }
        let stage = private_stage(path("--staging"), "bbsbox-placement-")?;
        let opened = archive::open(
            path("--archive"),
            &stage.path().join("opened"),
            expected,
            &identity,
            Limits::default(),
        )?;
        let games = [game_requirement(&crate::games::bundled_manifest()?)];
        let signed = bbsbox_maintenance::continuation::finish_game_placement(
            path("--staged"),
            path("--placed"),
            &opened,
            &registry,
            &games,
            decision,
        )?;
        return Ok(
            serde_json::json!({"format":"bbsbox.game-node-placement/v1","node":expected.to_string(),"inventory_sha256":hex_id(opened.inventory().id()),"decision_sha256":hex_id(signed.id()),"isolated_writer_ready":true,"installed":false}),
        );
    }
    if matches!(command, "backup" | "backup-game-node") {
        backup_with_scope(
            path("--config"),
            expected,
            &recipient()?,
            path("--output"),
            path("--staging"),
            registry,
            command == "backup-game-node",
        )
    } else {
        let authorization = if matches!(
            command,
            "restore-and-authorize" | "restore-compatible" | "restore-game-node-and-record"
        ) {
            use bbsbox_maintenance::continuation::{Authorization, SourceDisposition};
            let text = |name| {
                options[name]
                    .to_str()
                    .ok_or("Continuation option must be text")
            };
            let source = match text("--source")? {
                "retired" => SourceDisposition::Retired,
                "lost" => SourceDisposition::Lost,
                _ => return Err("Source must be explicitly retired or lost".into()),
            };
            Some(Authorization {
                source,
                reason: text("--reason")?.into(),
                recorded_at: text("--at")?.parse()?,
            })
        } else {
            None
        };
        restore_with_settings(
            path("--archive"),
            expected,
            path("--identity-file"),
            path("--output"),
            path("--staging"),
            RestoreSettings {
                registry,
                authorization,
                known_prefix: command == "restore-compatible",
                with_games: matches!(
                    command,
                    "restore-game-node" | "restore-game-node-and-record"
                ),
            },
        )
    }
}
