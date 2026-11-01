// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_identity::node::NodeSigner;
use bbsbox_maintenance::{
    BackupIdentity, Error, Limits, archive,
    inventory::{self, FileRecord, SignedInventory},
};
use bbsbox_storage::{Command, Database, Migration, snapshot};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

struct Fixture {
    _root: tempfile::TempDir,
    root: PathBuf,
    staged: PathBuf,
    signer: NodeSigner,
    key: BackupIdentity,
}
impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().to_owned();
        let mut migrations = bbsbox_identity::migrations();
        migrations.push(Migration {
            module: "fixture",
            version: 1,
            sql: "CREATE TABLE fixture_values(value TEXT);",
        });
        let mut database = Database::open(root.join("node.sqlite"), &migrations).unwrap();
        let signer = NodeSigner::start(&mut database).unwrap();
        database
            .transaction(
                Command {
                    namespace: "fixture/member",
                    id: "kept",
                    request: b"kept",
                },
                |tx| {
                    tx.execute("INSERT INTO fixture_values VALUES('retained')", [])?;
                    Ok(b"receipt".to_vec())
                },
            )
            .unwrap();
        database
            .put_blob("fixture", "large", &vec![0x5a; 200_000][..])
            .unwrap();
        let staged = root.join("staged");
        fs::create_dir(&staged).unwrap();
        database.snapshot_core(staged.join("core")).unwrap();
        fs::create_dir(staged.join("config")).unwrap();
        fs::write(
            staged.join("config/node.json"),
            br#"{"name":"Local fixture"}"#,
        )
        .unwrap();
        fs::write(staged.join("empty"), []).unwrap();
        Self {
            _root: directory,
            root,
            staged,
            signer,
            key: BackupIdentity::generate(),
        }
    }
    fn seal(&self, name: &str) -> (PathBuf, SignedInventory) {
        let path = self.root.join(name);
        let signed = archive::seal(
            &self.staged,
            &path,
            &self.signer,
            &self.key.to_public(),
            123,
            Limits::default(),
        )
        .unwrap();
        (path, signed)
    }
}
fn header(path: &str, size: u64, kind: tar::EntryType) -> tar::Header {
    let mut header = tar::Header::new_ustar();
    header.set_path(path).unwrap();
    header.set_size(size);
    header.set_mode(0o777);
    header.set_uid(123);
    header.set_gid(456);
    header.set_mtime(0);
    header.set_entry_type(kind);
    header.set_cksum();
    header
}
fn malicious(
    fixture: &Fixture,
    path: &Path,
    signed: &SignedInventory,
    name: &str,
    body: &[u8],
    kind: tar::EntryType,
    extra: bool,
) {
    let recipient = fixture.key.to_public();
    let encryptor =
        age::Encryptor::with_recipients(std::iter::once(&recipient as &dyn age::Recipient))
            .unwrap();
    let encrypted = encryptor.wrap_output(File::create(path).unwrap()).unwrap();
    let mut tar = tar::Builder::new(encrypted);
    tar.append(
        &header(
            "inventory.cose",
            signed.encoded().len() as u64,
            tar::EntryType::Regular,
        ),
        signed.encoded(),
    )
    .unwrap();
    let mut member = header("files/safe.txt", body.len() as u64, kind);
    if name != "files/safe.txt" {
        member.as_mut_bytes()[..100].fill(0);
        member.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
        member.set_cksum();
    }
    tar.append(&member, body).unwrap();
    if extra {
        tar.append(
            &header("files/safe.txt", body.len() as u64, tar::EntryType::Regular),
            body,
        )
        .unwrap();
    }
    tar.into_inner().unwrap().finish().unwrap();
}

#[test]
fn protected_stream_round_trips_real_core_snapshot_and_only_finishes_after_verification() {
    let fixture = Fixture::new();
    let (path, signed) = fixture.seal("first.age");
    let encoded = fs::read(&path).unwrap();
    assert!(encoded.starts_with(b"age-encryption.org/v1\n"));
    let target = fixture.root.join("opened");
    let opened = archive::open(
        &path,
        &target,
        fixture.signer.id(),
        &fixture.key,
        Limits::default(),
    )
    .unwrap();
    assert_eq!(opened.inventory().id(), signed.id());
    assert_eq!(opened.inventory().node(), fixture.signer.id());
    snapshot::verify_core(opened.content().join("core")).unwrap();
    assert_eq!(
        fs::read(opened.content().join("config/node.json")).unwrap(),
        fs::read(fixture.staged.join("config/node.json")).unwrap()
    );
    assert!(!target.join(".archive-incomplete").exists());
    assert_eq!(
        fs::read(target.join("inventory.cose")).unwrap(),
        signed.encoded()
    );
    let (other, again) = fixture.seal("again.age");
    assert_ne!(again.id(), signed.id());
    assert_ne!(fs::read(other).unwrap(), encoded);
}

#[test]
fn wrong_key_wrong_node_and_existing_outputs_cannot_replace_state() {
    let fixture = Fixture::new();
    let (path, _) = fixture.seal("backup.age");
    let target = fixture.root.join("wrong-key");
    assert!(
        archive::open(
            &path,
            &target,
            fixture.signer.id(),
            &BackupIdentity::generate(),
            Limits::default()
        )
        .is_err()
    );
    assert!(!target.exists());
    let other = Fixture::new();
    let target = fixture.root.join("wrong-node");
    assert!(matches!(
        archive::open(
            &path,
            &target,
            other.signer.id(),
            &fixture.key,
            Limits::default()
        ),
        Err(Error::WrongNode)
    ));
    assert!(!target.exists());
    let bytes = fs::read(&path).unwrap();
    assert!(
        archive::seal(
            &fixture.staged,
            &path,
            &fixture.signer,
            &fixture.key.to_public(),
            123,
            Limits::default()
        )
        .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);
    let existing = fixture.root.join("existing");
    fs::create_dir(&existing).unwrap();
    fs::write(existing.join("kept"), b"original").unwrap();
    assert!(
        archive::open(
            &path,
            &existing,
            fixture.signer.id(),
            &fixture.key,
            Limits::default()
        )
        .is_err()
    );
    assert_eq!(fs::read(existing.join("kept")).unwrap(), b"original");
}

#[test]
fn tampering_truncation_and_appended_ciphertext_never_publish_a_complete_archive() {
    let fixture = Fixture::new();
    let (path, _) = fixture.seal("good.age");
    let bytes = fs::read(path).unwrap();
    let mut middle = bytes.clone();
    let index = middle.len() / 2;
    middle[index] ^= 1;
    let mut tag = bytes.clone();
    *tag.last_mut().unwrap() ^= 1;
    let mut trailing = bytes.clone();
    trailing.extend(b"unauthenticated tail");
    let variants = [
        middle,
        tag,
        trailing,
        bytes[..bytes.len() - 1].to_vec(),
        bytes[..bytes.len() - 16].to_vec(),
        bytes[..bytes.len() - 1024].to_vec(),
    ];
    for (index, content) in variants.iter().enumerate() {
        let path = fixture.root.join(format!("bad-{index}.age"));
        fs::write(&path, content).unwrap();
        let target = fixture.root.join(format!("partial-{index}"));
        assert!(
            archive::open(
                &path,
                &target,
                fixture.signer.id(),
                &fixture.key,
                Limits::default()
            )
            .is_err()
        );
        assert!(!target.join("inventory.cose").exists());
        if target.exists() {
            assert!(target.join(".archive-incomplete").exists());
        }
    }
}

#[test]
fn authenticated_inventory_still_rejects_escaping_links_wrong_bytes_and_duplicate_members() {
    let fixture = Fixture::new();
    let signed = inventory::sign(
        vec![FileRecord {
            path: "safe.txt".into(),
            size: 4,
            sha256: hex::encode(Sha256::digest(b"safe")),
        }],
        0,
        &fixture.signer,
        Limits::default(),
    )
    .unwrap();
    let outside = fixture.root.join("outside");
    fs::write(&outside, b"original").unwrap();
    let cases = [
        (
            "files/../../outside",
            b"safe".as_slice(),
            tar::EntryType::Regular,
            false,
        ),
        (
            "files/safe.txt",
            b"evil".as_slice(),
            tar::EntryType::Regular,
            false,
        ),
        (
            "files/safe.txt",
            b"safe".as_slice(),
            tar::EntryType::Symlink,
            false,
        ),
        (
            "files/safe.txt",
            b"safe".as_slice(),
            tar::EntryType::Link,
            false,
        ),
        (
            "files/safe.txt",
            b"safe".as_slice(),
            tar::EntryType::Regular,
            true,
        ),
    ];
    for (index, (name, body, kind, extra)) in cases.into_iter().enumerate() {
        let path = fixture.root.join(format!("crafted-{index}.age"));
        malicious(&fixture, &path, &signed, name, body, kind, extra);
        let target = fixture.root.join(format!("crafted-{index}"));
        assert!(
            archive::open(
                &path,
                &target,
                fixture.signer.id(),
                &fixture.key,
                Limits::default()
            )
            .is_err()
        );
        assert!(!target.join("inventory.cose").exists());
        assert_eq!(fs::read(&outside).unwrap(), b"original");
    }
}

#[test]
fn bounded_header_member_and_file_inventory_checks_fail_before_final_output() {
    let fixture = Fixture::new();
    let destination = fixture.root.join("limited.age");
    assert!(
        archive::seal(
            &fixture.staged,
            &destination,
            &fixture.signer,
            &fixture.key.to_public(),
            0,
            Limits {
                max_member_bytes: 1024,
                max_total_bytes: 1024,
                ..Limits::default()
            }
        )
        .is_err()
    );
    assert!(!destination.exists());
    assert!(
        archive::seal(
            &fixture.staged,
            &fixture.staged.join("recursive.age"),
            &fixture.signer,
            &fixture.key.to_public(),
            0,
            Limits::default()
        )
        .is_err()
    );
    let header = fixture.root.join("header.age");
    let mut file = File::create(&header).unwrap();
    file.write_all(b"age-encryption.org/v1\n-> Unknown ")
        .unwrap();
    file.write_all(&vec![b'A'; 100_000]).unwrap();
    drop(file);
    let target = fixture.root.join("oversized-header");
    assert!(
        archive::open(
            &header,
            &target,
            fixture.signer.id(),
            &fixture.key,
            Limits::default()
        )
        .is_err()
    );
    assert!(!target.exists());
}

#[cfg(unix)]
#[test]
fn source_links_are_refused_and_extracted_secret_files_are_private() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let fixture = Fixture::new();
    let outside = fixture.root.join("private-source");
    fs::write(&outside, b"secret").unwrap();
    symlink(&outside, fixture.staged.join("linked")).unwrap();
    let output = fixture.root.join("refused.age");
    assert!(
        archive::seal(
            &fixture.staged,
            &output,
            &fixture.signer,
            &fixture.key.to_public(),
            0,
            Limits::default()
        )
        .is_err()
    );
    assert!(!output.exists());
    fs::remove_file(fixture.staged.join("linked")).unwrap();
    let (path, _) = fixture.seal("private.age");
    let target = fixture.root.join("private-opened");
    let opened = archive::open(
        &path,
        &target,
        fixture.signer.id(),
        &fixture.key,
        Limits::default(),
    )
    .unwrap();
    assert_eq!(
        fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(opened.content().join("core/core.sqlite"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}
