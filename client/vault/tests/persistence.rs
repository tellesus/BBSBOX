// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_client_identity::recovery::{Engine, Error as CryptoError};
use bbsbox_client_vault::{Error, Vault};
use bbsbox_credentials::{Purpose, Purposes};
use bbsbox_storage::Error as StorageError;
use rusqlite::Connection;
use zeroize::Zeroizing;

fn pass() -> Zeroizing<String> {
    Zeroizing::new("encrypted local vault fixture passphrase".into())
}
fn purposes() -> Purposes {
    Purposes::new(&[Purpose::Authorship, Purpose::IdentityProof]).unwrap()
}

#[test]
fn restart_keeps_identity_device_and_state_while_single_writer_and_revisions_fence_retries() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("client.sqlite");
    let engine = Engine::default();
    assert!(matches!(
        Vault::unlock(&path, &engine, pass()),
        Err(Error::Uninitialized)
    ));
    assert!(!path.exists());
    let mut vault = Vault::create(&path, &engine, pass(), purposes()).unwrap();
    let person = vault.person_id();
    let credential = vault.credential().id();
    let incarnation = *vault.control_incarnation();
    assert!(matches!(
        Vault::unlock(&path, &engine, pass()),
        Err(Error::Storage(StorageError::WriterAlreadyRunning))
    ));
    assert_eq!(
        vault
            .checkpoint(
                1,
                Zeroizing::new(b"encrypted ratchet state and queued ciphertext".to_vec())
            )
            .unwrap(),
        2
    );
    assert!(matches!(
        vault.checkpoint(1, Zeroizing::new(b"stale update".to_vec())),
        Err(Error::RevisionChanged)
    ));
    drop(vault);
    for file in std::fs::read_dir(root.path()).unwrap() {
        let bytes = std::fs::read(file.unwrap().path()).unwrap();
        assert!(
            !bytes
                .windows(21)
                .any(|window| window == b"encrypted ratchet sta")
        );
    }
    let reopened = Vault::unlock(&path, &engine, pass()).unwrap();
    assert_eq!(reopened.person_id(), person);
    assert_eq!(reopened.credential().id(), credential);
    assert_ne!(reopened.control_incarnation(), &incarnation);
    assert_eq!(reopened.revision(), 2);
    assert_eq!(
        reopened.state(),
        b"encrypted ratchet state and queued ciphertext"
    );
    assert!(!reopened.requires_rejoin());
    drop(reopened);
    assert!(matches!(
        Vault::create(&path, &engine, pass(), purposes()),
        Err(Error::AlreadyInitialized)
    ));
    assert!(matches!(
        Vault::unlock(
            &path,
            &engine,
            Zeroizing::new("another vault fixture passphrase".into())
        ),
        Err(Error::Crypto(CryptoError::DecryptFailed))
    ));
    assert_eq!(
        Vault::unlock(&path, &engine, pass()).unwrap().person_id(),
        person
    );
}

#[test]
fn failed_updates_leave_memory_and_disk_coherent_and_sql_metadata_cannot_relabel_a_record() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("client.sqlite");
    let engine = Engine::default();
    let mut vault = Vault::create(&path, &engine, pass(), purposes()).unwrap();
    let connection = Connection::open(&path).unwrap();
    connection.execute_batch("CREATE TRIGGER fail_save BEFORE UPDATE ON client_vault BEGIN SELECT RAISE(ABORT,'fixture failure'); END;").unwrap();
    assert!(matches!(
        vault.checkpoint(1, Zeroizing::new(b"new state".to_vec())),
        Err(Error::Sqlite(_))
    ));
    assert_eq!(vault.revision(), 1);
    assert!(vault.state().is_empty());
    connection.execute_batch("DROP TRIGGER fail_save").unwrap();
    vault
        .checkpoint(1, Zeroizing::new(b"committed state".to_vec()))
        .unwrap();
    drop(vault);
    let reopened = Vault::unlock(&path, &engine, pass()).unwrap();
    assert_eq!(reopened.revision(), 2);
    assert_eq!(reopened.state(), b"committed state");
    drop(reopened);
    connection
        .execute("UPDATE client_vault SET revision=3", [])
        .unwrap();
    assert!(matches!(
        Vault::unlock(&path, &engine, pass()),
        Err(Error::InvalidRecord)
    ));
}

#[test]
fn incomplete_setup_can_retry_and_explicit_recovery_preserves_the_receive_only_guard() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("original.sqlite");
    let recovered = root.path().join("recovered.sqlite");
    let engine = Engine::default();
    assert!(matches!(
        Vault::create(&path, &engine, Zeroizing::new("short".into()), purposes()),
        Err(Error::Crypto(CryptoError::WeakPassphrase))
    ));
    let original = Vault::create(&path, &engine, pass(), purposes()).unwrap();
    let person = original.person_id();
    let credential = original.credential().id();
    let kit = original
        .export_recovery(&engine, pass(), b"receive snapshot")
        .unwrap();
    drop(original);
    assert!(matches!(
        Vault::recover(&path, &engine, &kit, pass(), pass(), purposes()),
        Err(Error::AlreadyInitialized)
    ));
    let restored = Vault::recover(&recovered, &engine, &kit, pass(), pass(), purposes()).unwrap();
    assert_eq!(restored.person_id(), person);
    assert_ne!(restored.credential().id(), credential);
    assert!(restored.requires_rejoin());
    assert_eq!(restored.state(), b"receive snapshot");
    let fresh_credential = restored.credential().id();
    drop(restored);
    let reopened = Vault::unlock(&recovered, &engine, pass()).unwrap();
    assert_eq!(reopened.credential().id(), fresh_credential);
    assert!(reopened.requires_rejoin());
}

#[test]
fn an_uncertain_caller_cannot_overwrite_a_newer_durable_checkpoint() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("client.sqlite");
    let engine = Engine::default();
    let mut vault = Vault::create(&path, &engine, pass(), purposes()).unwrap();
    let connection = Connection::open(&path).unwrap();
    let record: Vec<u8> = connection
        .query_row("SELECT record FROM client_vault", [], |row| row.get(0))
        .unwrap();
    let opened = bbsbox_client_identity::local::Session::open(&engine, &record, pass()).unwrap();
    let newer = opened
        .session
        .checkpoint(2, b"accepted before reply was lost")
        .unwrap();
    connection
        .execute("UPDATE client_vault SET revision=2,record=?1", [newer])
        .unwrap();
    assert!(matches!(
        vault.checkpoint(1, Zeroizing::new(b"retry from old state".to_vec())),
        Err(Error::RevisionChanged)
    ));
    assert_eq!(vault.revision(), 1);
    assert!(vault.state().is_empty());
    drop(vault);
    let reconciled = Vault::unlock(&path, &engine, pass()).unwrap();
    assert_eq!(reconciled.revision(), 2);
    assert_eq!(reconciled.state(), b"accepted before reply was lost");
}
