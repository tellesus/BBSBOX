// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_client_identity::{DeviceKey, RootKey};
use bbsbox_credentials::{Purpose, Purposes};
use bbsbox_identity::{
    Error, migrations,
    public::{resolve_credential, store_credential},
};
use bbsbox_storage::{Database, disk::Class, open_reader};
fn credential() -> bbsbox_credentials::Credential {
    let root = RootKey::generate();
    let device = DeviceKey::generate();
    root.issue_device(
        device.public(),
        Purposes::new(&[Purpose::Authorship]).unwrap(),
    )
    .unwrap()
}
#[test]
fn resolver_persists_verified_public_delegations_without_creating_or_linking_accounts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node.sqlite");
    let mut db = Database::open(&path, &migrations()).unwrap();
    let credential = credential();
    let reader = open_reader(&path).unwrap();
    assert!(
        resolve_credential(&reader, credential.id())
            .unwrap()
            .is_none()
    );
    assert!(
        db.write::<_, Error>(Class::Content, 65536, |tx| store_credential(
            tx,
            &credential
        ))
        .unwrap()
    );
    assert!(
        !db.write::<_, Error>(Class::Content, 65536, |tx| store_credential(
            tx,
            &credential
        ))
        .unwrap()
    );
    drop(db);
    assert_eq!(
        resolve_credential(&reader, credential.id())
            .unwrap()
            .unwrap()
            .encoded(),
        credential.encoded()
    );
    assert_eq!(
        reader
            .query_row("SELECT count(*) FROM identity_accounts", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
#[test]
fn corrupted_persisted_credential_cannot_be_resolved_by_its_old_id() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node.sqlite");
    let mut db = Database::open(&path, &migrations()).unwrap();
    let first = credential();
    let second = credential();
    db.write::<_, Error>(Class::Content, 65536, |tx| store_credential(tx, &first))
        .unwrap();
    let raw = rusqlite::Connection::open(&path).unwrap();
    raw.execute(
        "UPDATE identity_device_credentials SET encoded=?1",
        [second.encoded()],
    )
    .unwrap();
    assert!(matches!(
        resolve_credential(&open_reader(&path).unwrap(), first.id()),
        Err(Error::InvalidStoredCredential)
    ));
}
#[test]
fn migration_two_keeps_existing_accounts_and_receipts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node.sqlite");
    let all = migrations();
    let mut old = Database::open(&path, &all[..1]).unwrap();
    let password = bbsbox_identity::password::Engine::default()
        .prepare(zeroize::Zeroizing::new(
            "local migration test passphrase".into(),
        ))
        .unwrap();
    let account = old
        .write::<_, Error>(Class::Control, 65536, |tx| {
            bbsbox_identity::bootstrap_sysop(tx, "sysop", password)
        })
        .unwrap();
    let command = || bbsbox_storage::Command {
        namespace: "identity/fixture",
        id: "before-upgrade",
        request: b"nonsecret migration fixture",
    };
    old.transaction(command(), |_| Ok(b"durable".to_vec()))
        .unwrap();
    drop(old);
    let mut db = Database::open(&path, &all).unwrap();
    assert_eq!(
        bbsbox_identity::account(&open_reader(&path).unwrap(), account.id).unwrap(),
        account
    );
    assert!(
        db.transaction(command(), |_| panic!("must replay"))
            .unwrap()
            .replayed
    );
    let credential = credential();
    db.write::<_, Error>(Class::Content, 65536, |tx| {
        store_credential(tx, &credential)
    })
    .unwrap();
    assert!(
        resolve_credential(&open_reader(&path).unwrap(), credential.id())
            .unwrap()
            .is_some()
    );
}
