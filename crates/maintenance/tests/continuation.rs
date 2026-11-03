// SPDX-License-Identifier: AGPL-3.0-or-later
//! Isolated foundation continuation only. Fake TLS bytes are not a certificate acceptance claim.
use bbsbox_identity::node::NodeSigner;
use bbsbox_maintenance::{
    BackupIdentity, Limits, archive,
    continuation::{self, Authorization, SourceDisposition},
    snapshot,
};
use bbsbox_storage::{
    Command, Database, Migration,
    writer::{Config, Writer},
};
use coset::{CborSerializable, CoseSign1};
use std::{fs, path::PathBuf};

fn migrations() -> Vec<Migration> {
    bbsbox_identity::migrations()
}
struct Fixture {
    _temporary: tempfile::TempDir,
    root: PathBuf,
    source: PathBuf,
    signer: NodeSigner,
    opened: archive::OpenedArchive,
    restored: snapshot::RestoredSnapshot,
    key: BackupIdentity,
    stage: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().canonicalize().unwrap();
        let source = root.join("node.sqlite");
        let mut database = Database::open(&source, &migrations()).unwrap();
        let signer = NodeSigner::start(&mut database).unwrap();
        database
            .transaction(
                Command {
                    namespace: "fixture",
                    id: "before",
                    request: b"before",
                },
                |_| Ok(b"retained".to_vec()),
            )
            .unwrap();
        let configuration = root.join("configuration");
        fs::create_dir(&configuration).unwrap();
        fs::create_dir(configuration.join("tls")).unwrap();
        let required = [
            "installation.json",
            "node.json",
            "tls/ca-key.pem",
            "tls/ca.pem",
            "tls/server-key.pem",
            "tls/server.pem",
        ]
        .map(String::from)
        .to_vec();
        for path in &required {
            fs::write(
                configuration.join(path),
                b"fixture, not actual TLS material",
            )
            .unwrap();
        }
        let stage = root.join("snapshot");
        snapshot::capture(
            &mut database,
            signer.id(),
            &configuration,
            required,
            &stage,
            42,
            &migrations(),
        )
        .unwrap();
        let key = BackupIdentity::generate();
        let cipher = root.join("backup.age");
        archive::seal(
            &stage,
            &cipher,
            &signer,
            &key.to_public(),
            42,
            Limits::default(),
        )
        .unwrap();
        database
            .transaction(
                Command {
                    namespace: "fixture",
                    id: "after-snapshot",
                    request: b"lost",
                },
                |_| Ok(b"not present in snapshot".to_vec()),
            )
            .unwrap();
        drop(database);
        let opened = archive::open(
            &cipher,
            &root.join("opened"),
            signer.id(),
            &key,
            Limits::default(),
        )
        .unwrap();
        let restored = snapshot::restore(&opened, &root.join("restored"), &migrations()).unwrap();
        Self {
            _temporary: temporary,
            root,
            source,
            signer,
            opened,
            restored,
            key,
            stage,
        }
    }
    fn authorization(source: SourceDisposition) -> Authorization {
        Authorization {
            source,
            reason: "fixture operator confirms source disposition and snapshot horizon".into(),
            recorded_at: i64::MIN,
        }
    }
    fn marker(&self) -> PathBuf {
        PathBuf::from(format!(
            "{}.recovery-required",
            self.restored.database().display()
        ))
    }
    fn prepare(
        &self,
        source: SourceDisposition,
    ) -> bbsbox_maintenance::Result<continuation::Prepared> {
        continuation::prepare(
            &self.restored,
            &self.opened,
            &migrations(),
            Self::authorization(source),
        )
    }
}

#[test]
fn retired_source_continuation_binds_the_snapshot_and_preserves_unreconciled_horizon() {
    let f = Fixture::new();
    let prepared = f.prepare(SourceDisposition::Retired).unwrap();
    assert!(f.marker().exists());
    assert!(matches!(
        Writer::start(f.restored.database(), migrations(), Config::default()),
        Err(bbsbox_storage::Error::WriterAlreadyRunning)
    ));
    let checked = continuation::verify(prepared.decision().encoded(), f.signer.id()).unwrap();
    assert_eq!(
        checked.decision().source_inventory_sha256,
        hex::encode(f.opened.inventory().id())
    );
    assert_eq!(checked.decision().snapshot_time, 42);
    assert_eq!(checked.decision().post_snapshot_writes, "not-reconciled");
    assert_eq!(checked.decision().client_recovery, "separate");
    let signed = prepared.finish_isolated().unwrap();
    assert!(!f.marker().exists());
    let saved = fs::read(f.restored.configuration().join("maintenance/recovery.cose")).unwrap();
    assert_eq!(saved, signed.encoded());
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(f.restored.directory().join("recovery.json")).unwrap())
            .unwrap();
    assert_eq!(report["state"], "continuation-authorized");
    let restored = bbsbox_storage::open_reader(f.restored.database()).unwrap();
    let source = bbsbox_storage::open_reader(&f.source).unwrap();
    let count = |db: &rusqlite::Connection| {
        db.query_row(
            "SELECT count(*) FROM core_command_receipts WHERE command_id='after-snapshot'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
    };
    assert_eq!(count(&restored), 0);
    assert_eq!(
        count(&source),
        1,
        "continuation cannot invent absent writes"
    );
    drop(restored);
    drop(source);
    let writer = Writer::start(f.restored.database(), migrations(), Config::default()).unwrap();
    let current = writer
        .handle()
        .submit_service(
            bbsbox_storage::writer::Priority::Interactive,
            NodeSigner::start,
        )
        .unwrap()
        .blocking_wait()
        .unwrap();
    assert_eq!(current.id(), f.signer.id());
    assert_ne!(current.incarnation(), f.signer.incarnation());
    assert_ne!(
        current.incarnation(),
        &signed.decision().prepared_incarnation
    );
    writer.shutdown().unwrap();
    assert!(f.prepare(SourceDisposition::Retired).is_err());
}

#[test]
fn lost_source_decision_rejects_other_node_domain_corruption_and_noncanonical_signed_bytes() {
    let f = Fixture::new();
    let prepared = f.prepare(SourceDisposition::Lost).unwrap();
    let bytes = prepared.decision().encoded();
    assert_eq!(
        prepared.decision().decision().source,
        SourceDisposition::Lost
    );
    let wrong = bbsbox_credentials::NodeId::from_digest([9; 32]);
    assert!(continuation::verify(bytes, wrong).is_err());
    let mut changed = bytes.to_vec();
    *changed.last_mut().unwrap() ^= 1;
    assert!(continuation::verify(&changed, f.signer.id()).is_err());
    let mut cose = CoseSign1::from_slice(bytes).unwrap();
    cose.signature = f
        .signer
        .sign_current(
            f.signer.incarnation(),
            &cose.tbs_data(b"bbsbox.archive-inventory/v1"),
        )
        .unwrap()
        .to_vec();
    assert!(continuation::verify(&cose.to_vec().unwrap(), f.signer.id()).is_err());
    let mut cose = CoseSign1::from_slice(bytes).unwrap();
    cose.payload = Some(serde_json::to_vec_pretty(prepared.decision().decision()).unwrap());
    cose.signature = f
        .signer
        .sign_current(
            f.signer.incarnation(),
            &cose.tbs_data(continuation::DOMAIN.as_bytes()),
        )
        .unwrap()
        .to_vec();
    assert!(continuation::verify(&cose.to_vec().unwrap(), f.signer.id()).is_err());
    drop(prepared);
    assert!(f.marker().exists());
    assert!(matches!(
        Writer::start(f.restored.database(), migrations(), Config::default()),
        Err(bbsbox_storage::Error::RecoveryRequired)
    ));
}

#[test]
fn unknown_owners_wrong_restore_basis_and_invalid_reason_leave_the_fence_in_place() {
    let f = Fixture::new();
    let configuration = f.restored.configuration().join("node.json");
    let original = fs::read(&configuration).unwrap();
    fs::write(&configuration, b"changed config").unwrap();
    assert!(f.prepare(SourceDisposition::Lost).is_err());
    fs::write(configuration, original).unwrap();
    let mut registry = migrations();
    registry.push(Migration {
        module: "unknown",
        version: 1,
        sql: "CREATE TABLE unknown(value INTEGER);",
    });
    assert!(
        continuation::prepare(
            &f.restored,
            &f.opened,
            &registry,
            Fixture::authorization(SourceDisposition::Lost)
        )
        .is_err()
    );
    let mut invalid = Fixture::authorization(SourceDisposition::Lost);
    invalid.reason = "bad\nreason".into();
    assert!(continuation::prepare(&f.restored, &f.opened, &migrations(), invalid).is_err());
    let cipher = f.root.join("other.age");
    archive::seal(
        &f.stage,
        &cipher,
        &f.signer,
        &f.key.to_public(),
        42,
        Limits::default(),
    )
    .unwrap();
    let other = archive::open(
        &cipher,
        &f.root.join("other"),
        f.signer.id(),
        &f.key,
        Limits::default(),
    )
    .unwrap();
    assert!(
        continuation::prepare(
            &f.restored,
            &other,
            &migrations(),
            Fixture::authorization(SourceDisposition::Lost)
        )
        .is_err()
    );
    assert!(f.marker().exists());
}

#[test]
fn unknown_existing_decision_is_preserved_and_a_failed_finish_never_unblocks_writers() {
    let f = Fixture::new();
    let directory = f.restored.configuration().join("maintenance");
    fs::create_dir(&directory).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let path = directory.join("recovery.cose");
    fs::write(&path, b"unrecognized operator data").unwrap();
    let prepared = f.prepare(SourceDisposition::Retired).unwrap();
    assert!(prepared.finish_isolated().is_err());
    assert_eq!(fs::read(path).unwrap(), b"unrecognized operator data");
    assert!(f.marker().exists());
    assert!(matches!(
        Writer::start(f.restored.database(), migrations(), Config::default()),
        Err(bbsbox_storage::Error::RecoveryRequired)
    ));
}

#[cfg(any(windows, unix))]
#[test]
fn interruption_after_saved_authorization_keeps_the_marker_and_can_be_reprepared() {
    #[cfg(unix)]
    if std::process::Command::new("id")
        .arg("-u")
        .output()
        .unwrap()
        .stdout
        == b"0\n"
    {
        return;
    } // Root bypasses this permission fault.
    let f = Fixture::new();
    let prepared = f.prepare(SourceDisposition::Lost).unwrap();
    let original = *prepared.decision().id();
    #[cfg(windows)]
    let blocker = {
        use std::os::windows::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(f.marker())
            .unwrap()
    };
    #[cfg(unix)]
    let blocker = {
        use std::os::unix::fs::PermissionsExt;
        struct RestoreMode(PathBuf);
        impl Drop for RestoreMode {
            fn drop(&mut self) {
                fs::set_permissions(&self.0, fs::Permissions::from_mode(0o700)).unwrap();
            }
        }
        let path = f.restored.database().parent().unwrap().to_path_buf();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o500)).unwrap();
        RestoreMode(path)
    };
    assert!(prepared.finish_isolated().is_err());
    assert!(f.marker().exists());
    drop(blocker);
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(f.restored.directory().join("recovery.json")).unwrap())
            .unwrap();
    assert_eq!(report["state"], "continuation-authorized");
    let resumed = f.prepare(SourceDisposition::Lost).unwrap();
    assert_ne!(resumed.decision().id(), &original);
    resumed.finish_isolated().unwrap();
    assert!(!f.marker().exists());
}
