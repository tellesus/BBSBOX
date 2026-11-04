// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_identity::node::NodeSigner;
use bbsbox_node_runtime::{Config, migrations, operator};
use bbsbox_storage::{
    Database,
    writer::{Config as WriterConfig, Writer},
};
use std::{fs, path::PathBuf};

struct Fixture {
    _root: tempfile::TempDir,
    root: PathBuf,
    config: PathBuf,
    database: PathBuf,
    node: bbsbox_credentials::NodeId,
}
impl Fixture {
    fn new() -> Self {
        Self::with_registry(&migrations())
    }

    fn with_registry(registry: &[bbsbox_storage::Migration]) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let database = root.join("node.sqlite");
        let mut db = Database::open(&database, registry).unwrap();
        let node = NodeSigner::start(&mut db).unwrap().id();
        drop(db);
        let configuration = root.join("configuration");
        fs::create_dir(&configuration).unwrap();
        fs::create_dir(configuration.join("tls")).unwrap();
        for name in ["ca.pem", "ca-key.pem", "server.pem", "server-key.pem"] {
            fs::write(
                configuration.join("tls").join(name),
                b"fixture only, not a certificate acceptance test",
            )
            .unwrap();
        }
        let config = configuration.join("node.json");
        fs::write(
            &config,
            serde_json::to_vec(&Config {
                format: "bbsbox.node-config/v1".into(),
                name: "Fixture".into(),
                bind: "127.0.0.1:8443".parse().unwrap(),
                additional_binds: vec![],
                origin: "https://localhost:8443".into(),
                additional_origins: vec![],
                database: database.clone(),
                certificate: configuration.join("tls/server.pem"),
                certificate_key: configuration.join("tls/server-key.pem"),
            })
            .unwrap(),
        )
        .unwrap();
        fs::write(configuration.join("installation.json"),serde_json::to_vec(&serde_json::json!({"format":"bbsbox.development-install/v1","node_id":node.to_string()})).unwrap()).unwrap();
        Self {
            _root: directory,
            root,
            config,
            database,
            node,
        }
    }
}

#[test]
fn known_foundation_backup_can_recover_before_the_theme_migration() {
    let mut previous = migrations();
    previous.retain(|migration| migration.module != "settings");
    let fixture = Fixture::with_registry(&previous);
    let identity = fixture.root.join("key.age");
    let public = operator::keygen(&identity).unwrap();
    let recipient = public["recipient"].as_str().unwrap().parse().unwrap();
    let archive = fixture.root.join("before-settings.age");
    operator::backup_with_registry(
        &fixture.config,
        fixture.node,
        &recipient,
        &archive,
        &fixture.root,
        previous,
    )
    .unwrap();
    let output = fixture.root.join("recovered");
    let arguments = [
        "restore-compatible".into(),
        "--archive".into(),
        archive.into_os_string(),
        "--node".into(),
        fixture.node.to_string().into(),
        "--identity-file".into(),
        identity.into_os_string(),
        "--output".into(),
        output.clone().into_os_string(),
        "--staging".into(),
        fixture.root.clone().into_os_string(),
        "--source".into(),
        "retired".into(),
        "--reason".into(),
        "Preparing replacement hardware".into(),
        "--at".into(),
        "0".into(),
    ];
    let restored = operator::command(&arguments).unwrap();
    assert_eq!(restored["migration_required"], true);
    assert!(
        restored["source_registry"]
            .as_array()
            .unwrap()
            .iter()
            .all(|entry| entry["module"] != "settings")
    );
    let database = output.join("data/node.sqlite");
    operator::migrate_existing(&database, fixture.node, migrations()).unwrap();
    let reader = bbsbox_storage::open_reader(database).unwrap();
    let theme = bbsbox_settings::theme(&reader).unwrap();
    assert_eq!(theme.revision, 0);
    assert_eq!(theme.palette, bbsbox_settings::Palette::default());
}

#[test]
fn existing_migration_uses_only_the_supplied_compiled_registry_and_never_initializes_missing_data()
{
    let f = Fixture::new();
    let mut registry = migrations();
    registry.push(bbsbox_storage::Migration{module:"events",version:4,sql:"CREATE TABLE events_upgrade_fixture(value INTEGER NOT NULL); INSERT INTO events_upgrade_fixture VALUES(7);"});
    let wrong = bbsbox_credentials::NodeId::from_digest([9; 32]);
    assert!(operator::migrate_existing(&f.database, wrong, registry.clone()).is_err());
    let missing = f.root.join("missing.sqlite");
    assert!(operator::migrate_existing(&missing, f.node, registry.clone()).is_err());
    assert!(!missing.exists());
    let result = operator::migrate_existing(&f.database, f.node, registry.clone()).unwrap();
    assert_eq!(result["listener_opened"], false);
    assert_eq!(NodeSigner::inspect_existing(&f.database).unwrap(), f.node);
    let reader = bbsbox_storage::open_reader(&f.database).unwrap();
    assert_eq!(
        reader
            .query_row("SELECT value FROM events_upgrade_fixture", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        7
    );
    drop(reader);
    assert!(
        operator::migrate_existing(&f.database, f.node, migrations()).is_err(),
        "older code cannot silently remove an applied migration"
    );
    assert!(operator::migrate_existing(&f.database, f.node, registry).is_ok());
}

#[test]
fn compatible_restore_refuses_changed_sql_new_owners_or_an_older_identity_layout_before_output() {
    use std::ffi::OsString;
    let f = Fixture::new();
    let key = f.root.join("operator.key");
    let public = operator::keygen(&key).unwrap();
    let recipient = public["recipient"].as_str().unwrap().parse().unwrap();
    let cipher = f.root.join("before.age");
    operator::backup(&f.config, f.node, &recipient, &cipher, &f.root).unwrap();
    let before = fs::read(&cipher).unwrap();
    let mut identity = migrations();
    identity.push(bbsbox_storage::Migration {
        module: "identity",
        version: 5,
        sql: "CREATE TABLE identity_future_reader(value INTEGER);",
    });
    let mut changed = migrations();
    changed[0].sql = "SELECT 1;";
    let mut owner = migrations();
    owner.push(bbsbox_storage::Migration {
        module: "other",
        version: 1,
        sql: "CREATE TABLE other_state(value INTEGER);",
    });
    for (index, registry) in [identity, changed, owner].into_iter().enumerate() {
        let destination = f.root.join(format!("rejected-{index}"));
        let mut args = vec![OsString::from("restore-compatible")];
        for (name, value) in [
            ("--archive", cipher.as_os_str().to_owned()),
            ("--node", f.node.to_string().into()),
            ("--identity-file", key.as_os_str().to_owned()),
            ("--output", destination.as_os_str().to_owned()),
            ("--staging", f.root.as_os_str().to_owned()),
            ("--source", "lost".into()),
            (
                "--reason",
                "Fixture lost-source declaration; no installed activation".into(),
            ),
            ("--at", "0".into()),
        ] {
            args.push(name.into());
            args.push(value);
        }
        assert!(operator::command_with_registry(&args, registry).is_err());
        assert!(!destination.exists());
    }
    assert_eq!(fs::read(&cipher).unwrap(), before);
    bbsbox_storage::verify_exact_registry(&f.database, &migrations()).unwrap();
    assert_eq!(NodeSigner::inspect_existing(&f.database).unwrap(), f.node);
}

#[test]
fn explicit_restore_authorization_keeps_client_recovery_and_installed_activation_separate() {
    use std::ffi::OsString;
    let f = Fixture::new();
    let key = f.root.join("operator.key");
    let public = operator::keygen(&key).unwrap();
    let derived = operator::command(&[
        "backup-public".into(),
        "--identity-file".into(),
        key.as_os_str().into(),
    ])
    .unwrap();
    assert_eq!(public, derived);
    let recipient = public["recipient"].as_str().unwrap().parse().unwrap();
    let cipher = f.root.join("backup.age");
    operator::backup(&f.config, f.node, &recipient, &cipher, &f.root).unwrap();
    let destination = f.root.join("authorized");
    let args = vec![
        OsString::from("restore-and-authorize"),
        "--archive".into(),
        cipher.as_os_str().into(),
        "--node".into(),
        f.node.to_string().into(),
        "--identity-file".into(),
        key.as_os_str().into(),
        "--output".into(),
        destination.as_os_str().into(),
        "--staging".into(),
        f.root.as_os_str().into(),
        "--source".into(),
        "retired".into(),
        "--reason".into(),
        "fixture stopped source; isolated continuation only".into(),
        "--at".into(),
        "0".into(),
    ];
    let result = operator::command(&args).unwrap();
    assert_eq!(result["isolated_writer_ready"], true);
    assert_eq!(result["installed"], false);
    assert_eq!(result["client_recovery"], "separate");
    assert_eq!(result["post_snapshot_writes"], "not-reconciled");
    let decision =
        std::fs::read(destination.join("configuration/maintenance/recovery.cose")).unwrap();
    bbsbox_maintenance::continuation::verify(&decision, f.node).unwrap();
    assert!(
        !destination
            .join("data/node.sqlite.recovery-required")
            .exists()
    );
    let writer = Writer::start(
        destination.join("data/node.sqlite"),
        migrations(),
        WriterConfig::default(),
    )
    .unwrap();
    writer.shutdown().unwrap();
    assert_eq!(NodeSigner::inspect_existing(&f.database).unwrap(), f.node);
}
#[test]
fn local_operator_round_trip_keeps_identity_configuration_and_restore_block() {
    let f = Fixture::new();
    let secret = f.root.join("operator.key");
    let generated = operator::keygen(&secret).unwrap();
    assert!(generated.get("secret").is_none());
    let recipient = generated["recipient"].as_str().unwrap().parse().unwrap();
    let cipher = f.root.join("backup.age");
    let result = operator::backup(&f.config, f.node, &recipient, &cipher, &f.root).unwrap();
    assert_eq!(result["scope"], "foundation-node");
    assert_eq!(NodeSigner::inspect_existing(&f.database).unwrap(), f.node);
    let target = f.root.join("restored");
    let restored = operator::restore(&cipher, f.node, &secret, &target, &f.root).unwrap();
    assert_eq!(restored["state"], "recovery-required");
    assert_eq!(
        fs::read(target.join("configuration/tls/ca-key.pem")).unwrap(),
        fs::read(f.config.parent().unwrap().join("tls/ca-key.pem")).unwrap()
    );
    assert!(matches!(
        Writer::start(
            target.join("data/node.sqlite"),
            migrations(),
            WriterConfig::default()
        ),
        Err(bbsbox_storage::Error::RecoveryRequired)
    ));
    let first = fs::read(&cipher).unwrap();
    assert!(operator::backup(&f.config, f.node, &recipient, &cipher, &f.root).is_err());
    assert_eq!(fs::read(&cipher).unwrap(), first);
    assert!(operator::keygen(&secret).is_err());
    let directories: Vec<_> = fs::read_dir(&f.root)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("bbsbox-snapshot-") || name.starts_with("bbsbox-restore-"))
        .collect();
    assert!(
        directories.is_empty(),
        "private temporary plaintext is cleaned after success/failure"
    );
}
#[test]
fn cold_backup_refuses_active_writer_wrong_node_missing_storage_and_configuration_lock() {
    let f = Fixture::new();
    let generated = operator::keygen(&f.root.join("operator.key")).unwrap();
    let recipient = generated["recipient"].as_str().unwrap().parse().unwrap();
    let output = f.root.join("backup.age");
    let writer = Writer::start(&f.database, migrations(), WriterConfig::default()).unwrap();
    assert!(operator::backup(&f.config, f.node, &recipient, &output, &f.root).is_err());
    assert!(!output.exists());
    writer.shutdown().unwrap();
    let wrong = bbsbox_credentials::NodeId::from_digest([0; 32]);
    assert!(operator::backup(&f.config, wrong, &recipient, &output, &f.root).is_err());
    assert!(!output.exists());
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(f.config.parent().unwrap().join(".tls-renewal.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    assert!(operator::backup(&f.config, f.node, &recipient, &output, &f.root).is_err());
    lock.unlock().unwrap();
    drop(lock);
    fs::remove_file(&f.database).unwrap();
    assert!(operator::backup(&f.config, f.node, &recipient, &output, &f.root).is_err());
    assert!(!f.database.exists());
    assert!(!output.exists());
}
#[test]
fn maintenance_options_are_exact_and_keygen_does_not_print_the_secret() {
    for arguments in [
        vec!["backup"],
        vec!["restore", "--node", "bad"],
        vec!["backup-keygen", "--output", "x", "--output", "y"],
        vec!["backup-keygen", "--unknown", "x"],
        vec!["backup-keygen", "--output"],
    ] {
        assert!(
            operator::command(&arguments.into_iter().map(Into::into).collect::<Vec<_>>()).is_err()
        );
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("key");
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_bbsbox-node"))
        .arg("backup-keygen")
        .arg("--output")
        .arg(&path)
        .output()
        .unwrap();
    assert!(result.status.success());
    let stdout = String::from_utf8(result.stdout).unwrap();
    assert!(stdout.contains("age1"));
    assert!(!stdout.contains("AGE-SECRET-KEY"));
    assert!(result.stderr.is_empty());
    let bytes = fs::read(path).unwrap();
    assert!(bytes.starts_with(b"AGE-SECRET-KEY-"));

    let generated: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let recipient = generated["recipient"].as_str().unwrap();
    let check = |recipient: &str| {
        std::process::Command::new(env!("CARGO_BIN_EXE_bbsbox-node"))
            .args(["backup-check-recipient", "--recipient", recipient])
            .output()
            .unwrap()
    };
    let accepted = check(recipient);
    assert!(accepted.status.success());
    let checked: serde_json::Value = serde_json::from_slice(&accepted.stdout).unwrap();
    assert_eq!(checked["recipient"], recipient);
    let (prefix, last) = recipient.split_at(recipient.len() - 1);
    let wrong = format!("{prefix}{}", if last == "q" { "p" } else { "q" });
    assert!(!check(&wrong).status.success());
}
