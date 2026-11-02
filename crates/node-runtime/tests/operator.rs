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
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let database = root.join("node.sqlite");
        let mut db = Database::open(&database, &migrations()).unwrap();
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
}
