// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_identity::{Error, migrations, node::NodeSigner};
use bbsbox_storage::{Database, snapshot};
use ed25519_dalek::{Signature, VerifyingKey};
#[test]
fn node_identity_survives_restart_but_old_writer_context_cannot_sign() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node.sqlite");
    let mut db = Database::open(&path, &migrations()).unwrap();
    let original = NodeSigner::start(&mut db).unwrap();
    let id = original.id();
    let old = *original.incarnation();
    drop(original);
    drop(db);
    let mut db = Database::open(&path, &migrations()).unwrap();
    let restarted = NodeSigner::start(&mut db).unwrap();
    assert_eq!(restarted.id(), id);
    assert_ne!(restarted.incarnation(), &old);
    assert!(matches!(
        restarted.sign_current(&old, b"old context"),
        Err(Error::StaleSigner)
    ));
    let mut message = b"bbsbox.node-sign-fixture/v1\0".to_vec();
    message.extend_from_slice(restarted.id().bytes());
    message.extend_from_slice(restarted.incarnation());
    message.extend_from_slice(b"fixture/1");
    let signature = Signature::try_from(
        restarted
            .sign_current(restarted.incarnation(), &message)
            .unwrap()
            .as_slice(),
    )
    .unwrap();
    VerifyingKey::from_bytes(&restarted.public())
        .unwrap()
        .verify_strict(&message, &signature)
        .unwrap();
}
#[test]
fn corrupt_node_state_fails_without_replacing_registered_identity() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node.sqlite");
    let mut db = Database::open(&path, &migrations()).unwrap();
    let original = NodeSigner::start(&mut db).unwrap();
    let raw = rusqlite::Connection::open(&path).unwrap();
    raw.execute(
        "UPDATE identity_node_identity SET public_key=zeroblob(32)",
        [],
    )
    .unwrap();
    assert!(matches!(
        NodeSigner::start(&mut db),
        Err(Error::InvalidNodeState)
    ));
    assert_eq!(
        raw.query_row("SELECT public_key FROM identity_node_identity", [], |r| r
            .get::<_, Vec<
            u8,
        >>(
            0
        ))
        .unwrap(),
        vec![0; 32]
    );
    assert_ne!(original.public(), [0; 32]);
}
#[test]
fn restored_core_includes_node_key_but_cannot_activate_a_signer_before_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node.sqlite");
    let mut db = Database::open(&path, &migrations()).unwrap();
    let original = NodeSigner::start(&mut db).unwrap();
    let backup = dir.path().join("snapshot");
    db.snapshot_core(&backup).unwrap();
    let restored_path = snapshot::restore_core(&backup, dir.path().join("restore")).unwrap();
    let mut restored = Database::open(&restored_path, &migrations()).unwrap();
    let public = rusqlite::Connection::open(&restored_path)
        .unwrap()
        .query_row("SELECT public_key FROM identity_node_identity", [], |r| {
            r.get::<_, Vec<u8>>(0)
        })
        .unwrap();
    assert_eq!(public, original.public());
    assert!(matches!(
        NodeSigner::start(&mut restored),
        Err(Error::Storage(bbsbox_storage::Error::RecoveryRequired))
    ));
}
#[cfg(unix)]
#[test]
fn live_node_key_database_and_sidecars_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node.sqlite");
    let mut db = Database::open(&path, &migrations()).unwrap();
    NodeSigner::start(&mut db).unwrap();
    for suffix in ["", "-wal", "-shm"] {
        let metadata = std::fs::metadata(dir.path().join(format!("node.sqlite{suffix}"))).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    }
}
