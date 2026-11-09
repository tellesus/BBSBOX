// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_maintenance::{
    client_install::Store,
    client_payload,
    client_release::{self, Manifest, SignedClient},
    inventory::FileRecord,
    release::ReleaseSigner,
};
use coset::{CborSerializable, CoseSign1};
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};
use zeroize::Zeroizing;

fn signer(n: u8) -> ReleaseSigner {
    ReleaseSigner::from_secret(Zeroizing::new([n; 32])).unwrap()
}
fn pe() -> Vec<u8> {
    let mut bytes = vec![0; 128];
    bytes[..2].copy_from_slice(b"MZ");
    bytes[60..64].copy_from_slice(&64u32.to_le_bytes());
    bytes[64..68].copy_from_slice(b"PE\0\0");
    bytes[68..70].copy_from_slice(&0x8664u16.to_le_bytes());
    bytes[86..88].copy_from_slice(&2u16.to_le_bytes());
    bytes[88..90].copy_from_slice(&0x20bu16.to_le_bytes());
    bytes
}
fn package(
    root: &Path,
    key: &ReleaseSigner,
    sequence: u64,
    next_key: Option<[u8; 32]>,
) -> SignedClient {
    fs::create_dir(root).unwrap();
    let mut files = Vec::new();
    for name in client_release::REQUIRED {
        let bytes = if name == client_release::ENTRY {
            pe()
        } else {
            format!("synthetic profile fixture: {name}").into_bytes()
        };
        let target = root.join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, &bytes).unwrap();
        files.push(FileRecord {
            path: name.into(),
            size: bytes.len() as u64,
            sha256: hex::encode(Sha256::digest(&bytes)),
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let manifest = Manifest {
        format: client_release::DOMAIN.into(),
        issuer: key.id().to_string(),
        sequence,
        version: format!("0.1.{sequence}-fixture"),
        target: client_release::TARGET.into(),
        source_commit: "02".repeat(20),
        vault_profiles: vec![1],
        files,
        next_key,
    };
    let signed = client_release::sign(manifest, key).unwrap();
    fs::write(root.join(client_payload::MANIFEST), signed.encoded()).unwrap();
    signed
}

#[test]
fn manifest_checks_authority_domain_profile_canonical_bytes_and_windows_aliases() {
    let temp = tempfile::tempdir().unwrap();
    let key = signer(41);
    let signed = package(&temp.path().join("package"), &key, 1, None);
    assert_eq!(
        client_release::verify(signed.encoded(), &key.public())
            .unwrap()
            .id(),
        signed.id()
    );
    assert!(client_release::verify(signed.encoded(), &signer(42).public()).is_err());
    let mut damaged = signed.encoded().to_vec();
    let last = damaged.len() - 1;
    damaged[last] ^= 1;
    assert!(client_release::verify(&damaged, &key.public()).is_err());
    let mut other_domain = CoseSign1::from_slice(signed.encoded()).unwrap();
    other_domain.signature = SigningKey::from_bytes(&[41; 32])
        .sign(&other_domain.tbs_data(b"bbsbox.release-manifest/v1"))
        .to_bytes()
        .to_vec();
    assert!(client_release::verify(&other_domain.to_vec().unwrap(), &key.public()).is_err());
    let mut trailing = signed.encoded().to_vec();
    trailing.push(0);
    assert!(client_release::verify(&trailing, &key.public()).is_err());
    let mut invalid = Vec::new();
    let base = signed.manifest().clone();
    let mut m = base.clone();
    m.target = "debian13-amd64".into();
    invalid.push(m);
    let mut m = base.clone();
    m.vault_profiles = vec![2];
    invalid.push(m);
    let mut m = base.clone();
    m.sequence = 0;
    invalid.push(m);
    let mut m = base.clone();
    m.sequence = u64::MAX;
    invalid.push(m);
    let mut m = base.clone();
    m.files[0].path = "../outside".into();
    invalid.push(m);
    let mut m = base.clone();
    m.next_key = Some(key.public());
    invalid.push(m);
    let mut m = base.clone();
    m.files.push(FileRecord {
        path: "license.md".into(),
        size: 1,
        sha256: "00".repeat(32),
    });
    m.files.sort_by(|a, b| a.path.cmp(&b.path));
    invalid.push(m);
    let mut m = base.clone();
    m.files.push(FileRecord {
        path: "MATERIALS".into(),
        size: 1,
        sha256: "00".repeat(32),
    });
    m.files.sort_by(|a, b| a.path.cmp(&b.path));
    invalid.push(m);
    for manifest in invalid {
        assert!(client_release::sign(manifest, &key).is_err());
    }
}

#[test]
fn staged_copy_is_independent_and_all_unlisted_changed_missing_or_incompatible_files_fail() {
    let temp = tempfile::tempdir().unwrap();
    let key = signer(43);
    let source = temp.path().join("source");
    let signed = package(&source, &key, 1, None);
    let stage = temp.path().join("stage");
    assert_eq!(
        client_payload::stage(&source, &stage, &key.public())
            .unwrap()
            .id(),
        signed.id()
    );
    assert!(client_payload::stage(&source, &stage, &key.public()).is_err());
    fs::write(source.join("LICENSE.md"), b"changed").unwrap();
    assert!(client_payload::verify(&source, &key.public()).is_err());
    assert!(client_payload::verify(&stage, &key.public()).is_ok());
    fs::write(stage.join("extra.dll"), b"unlisted").unwrap();
    assert!(client_payload::verify(&stage, &key.public()).is_err());
    fs::remove_file(stage.join("extra.dll")).unwrap();
    fs::remove_file(stage.join("materials/source.tar")).unwrap();
    assert!(client_payload::verify(&stage, &key.public()).is_err());
    let incompatible = temp.path().join("wrong-pe");
    let signed = package(&incompatible, &key, 2, None);
    let mut executable = pe();
    executable[68..70].copy_from_slice(&0x14cu16.to_le_bytes());
    fs::write(incompatible.join(client_release::ENTRY), &executable).unwrap();
    let mut manifest = signed.manifest().clone();
    let entry = manifest
        .files
        .iter_mut()
        .find(|f| f.path == client_release::ENTRY)
        .unwrap();
    entry.sha256 = hex::encode(Sha256::digest(&executable));
    let signed = client_release::sign(manifest, &key).unwrap();
    fs::write(
        incompatible.join(client_payload::MANIFEST),
        signed.encoded(),
    )
    .unwrap();
    assert!(client_payload::verify(&incompatible, &key.public()).is_err());
}

#[test]
fn interrupted_install_resumes_after_restart_and_rotation_preserves_the_global_sequence_floor() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("client");
    let key = signer(44);
    let next = signer(45);
    let first = temp.path().join("first");
    let signed = package(&first, &key, 1, Some(next.public()));
    assert!(Store::open(&root).is_err());
    assert!(!root.exists());
    let store = Store::initialize(&root, key.public()).unwrap();
    assert!(Store::open(&root).is_err());
    assert!(store.executable().is_err());
    let pending = store.prepare(0, [1; 16], &first).unwrap();
    assert_eq!(pending.revision, 1);
    assert_eq!(pending.trusted_key, key.public());
    assert_eq!(pending.highest_sequence, 1);
    assert!(store.executable().is_err());
    assert!(store.prepare(0, [1; 16], &first).is_err());
    assert_eq!(store.prepare(1, [1; 16], &first).unwrap(), pending);
    drop(store);
    let store = Store::open(&root).unwrap();
    assert_eq!(store.state().unwrap(), pending);
    assert!(store.activate(1, [2; 16]).is_err());
    let active = store.activate(1, [1; 16]).unwrap();
    assert_eq!(active.trusted_key, next.public());
    assert_eq!(active.active.as_ref().unwrap().id, *signed.id());
    assert!(store.executable().unwrap().is_file());
    assert_eq!(store.prepare(2, [1; 16], &first).unwrap(), active);
    let old_authority = temp.path().join("old-authority");
    package(&old_authority, &key, 3, None);
    assert!(store.prepare(2, [2; 16], &old_authority).is_err());
    let downgrade = temp.path().join("downgrade");
    package(&downgrade, &next, 1, None);
    assert!(store.prepare(2, [2; 16], &downgrade).is_err());
    let second = temp.path().join("second");
    package(&second, &next, 2, None);
    store.prepare(2, [2; 16], &second).unwrap();
    let newer = store.activate(3, [2; 16]).unwrap();
    assert_eq!(newer.highest_sequence, 2);
    assert_eq!(store.prepare(4, [1; 16], &first).unwrap(), newer); // historical retry never reselects an old client
    assert!(store.prepare(4, [1; 16], &second).is_err());
    drop(store);
    assert!(Store::initialize(&root, key.public()).is_err());
    let store = Store::open(&root).unwrap();
    assert_eq!(store.state().unwrap(), newer);
    let executable = store.executable().unwrap();
    fs::write(executable, b"changed after installation").unwrap();
    assert!(store.executable().is_err());
}

#[test]
fn failed_state_commit_leaves_an_inactive_copy_and_retry_uses_the_same_verified_release() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("client");
    let key = signer(46);
    let source = temp.path().join("source");
    let signed = package(&source, &key, 1, None);
    let store = Store::initialize(&root, key.public()).unwrap();
    let sql = rusqlite::Connection::open(root.join("trust.sqlite")).unwrap();
    sql.execute_batch("CREATE TRIGGER fail_client BEFORE UPDATE ON client_control BEGIN SELECT RAISE(ABORT,'fixture failure'); END;").unwrap();
    assert!(store.prepare(0, [1; 16], &source).is_err());
    assert_eq!(store.state().unwrap().revision, 0);
    assert!(store.executable().is_err());
    assert_eq!(
        sql.query_row("SELECT count(*) FROM client_install_requests", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(
        root.join("releases")
            .join(hex::encode(signed.id()))
            .is_dir()
    );
    sql.execute_batch("DROP TRIGGER fail_client").unwrap();
    store.prepare(0, [1; 16], &source).unwrap();
    let installed = root
        .join("releases")
        .join(hex::encode(signed.id()))
        .join(client_release::ENTRY);
    fs::write(&installed, b"tampered stage").unwrap();
    assert!(store.activate(1, [1; 16]).is_err());
    assert_eq!(store.state().unwrap().revision, 1);
    fs::write(&installed, pe()).unwrap();
    store.activate(1, [1; 16]).unwrap();
    assert!(store.executable().is_ok());
    sql.execute("UPDATE client_control SET revision=3", [])
        .unwrap();
    assert!(store.state().is_err());
}

#[cfg(unix)]
#[test]
fn symlinked_packages_and_linked_payload_entries_are_rejected_before_staging() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let key = signer(47);
    let source = temp.path().join("source");
    package(&source, &key, 1, None);
    let alias = temp.path().join("alias");
    symlink(&source, &alias).unwrap();
    assert!(client_payload::verify(&alias, &key.public()).is_err());
    let license = source.join("LICENSE.md");
    let external = temp.path().join("external");
    fs::rename(&license, &external).unwrap();
    symlink(external, &license).unwrap();
    assert!(client_payload::stage(&source, &temp.path().join("stage"), &key.public()).is_err());
    assert!(!temp.path().join("stage").exists());
}

#[cfg(windows)]
#[test]
fn windows_directory_junctions_cannot_redirect_the_client_package() {
    use std::os::windows::process::CommandExt;
    let temp = tempfile::tempdir().unwrap();
    let key = signer(48);
    let source = temp.path().join("source");
    package(&source, &key, 1, None);
    let alias = temp.path().join("junction");
    assert!(source.starts_with(temp.path()) && alias.starts_with(temp.path()));
    let status = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", "$ErrorActionPreference='Stop'; New-Item -ItemType Junction -Path $env:BBSBOX_FIXTURE_LINK -Target $env:BBSBOX_FIXTURE_TARGET | Out-Null"])
        .env("BBSBOX_FIXTURE_LINK", &alias).env("BBSBOX_FIXTURE_TARGET", &source)
        .creation_flags(0x08000000).status().unwrap();
    assert!(status.success());
    assert!(client_payload::verify(&alias, &key.public()).is_err());
    assert!(client_payload::stage(&alias, &temp.path().join("stage"), &key.public()).is_err());
    assert!(!temp.path().join("stage").exists());
}
