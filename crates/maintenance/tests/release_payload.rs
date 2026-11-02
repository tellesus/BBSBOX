// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_maintenance::{
    inventory::FileRecord,
    release::{self, Manifest, ModuleMigration, ReleaseSigner},
    release_payload::{self, RuntimeProfile},
};
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};

struct Fixture {
    _temporary: tempfile::TempDir,
    root: PathBuf,
    bundle: PathBuf,
    signer: ReleaseSigner,
    manifest: Manifest,
}
impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().canonicalize().unwrap();
        let bundle = root.join("bundle");
        fs::create_dir(&bundle).unwrap();
        let signer = ReleaseSigner::generate().unwrap();
        let mut files = vec![];
        for name in release::REQUIRED_FILES {
            // Synthetic ELF header validates format checks only; never executed or called a build.
            let bytes = if name == "bbsbox-node" {
                let mut header = vec![0; 64];
                header[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
                header[16] = 3;
                header[18] = 0x3e;
                header
            } else {
                format!("fixture only: {name}").into_bytes()
            };
            let path = bundle.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, &bytes).unwrap();
            files.push(FileRecord {
                path: name.into(),
                size: bytes.len() as u64,
                sha256: hex::encode(Sha256::digest(&bytes)),
            });
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let manifest = Manifest {
            format: release::DOMAIN.into(),
            issuer: signer.id().to_string(),
            sequence: 1,
            version: "0.1.0".into(),
            target: "debian13-amd64".into(),
            created_at: 0,
            source_commit: "01".repeat(20),
            core_schema: 3,
            readable_core_schemas: vec![3],
            module_migrations: bbsbox_identity::migrations()
                .iter()
                .map(ModuleMigration::from)
                .collect(),
            files,
            next_key: None,
        };
        let fixture = Self {
            _temporary: temporary,
            root,
            bundle,
            signer,
            manifest,
        };
        fixture.sign();
        fixture
    }
    fn sign(&self) {
        let signed = release::sign(self.manifest.clone(), &self.signer).unwrap();
        fs::write(
            self.bundle.join(release_payload::MANIFEST),
            signed.encoded(),
        )
        .unwrap();
    }
    fn replace(&mut self, name: &str, bytes: &[u8]) {
        fs::write(self.bundle.join(name), bytes).unwrap();
        let record = self
            .manifest
            .files
            .iter_mut()
            .find(|r| r.path == name)
            .unwrap();
        record.size = bytes.len() as u64;
        record.sha256 = hex::encode(Sha256::digest(bytes));
        self.sign();
    }
    fn verify(&self) -> bool {
        release_payload::verify(&self.bundle, &self.signer.public()).is_ok()
    }
    fn profile(&self) -> RuntimeProfile {
        RuntimeProfile {
            format: release_payload::PROFILE.into(),
            version: self.manifest.version.clone(),
            target: self.manifest.target.clone(),
            core_schema: 3,
            readable_core_schemas: vec![3],
            module_migrations: self.manifest.module_migrations.clone(),
        }
    }
}

#[test]
fn exact_payload_private_staging_and_independent_compiled_profile_match() {
    let f = Fixture::new();
    assert!(f.verify());
    let output = f.root.join("stage");
    let copied = release_payload::stage(&f.bundle, &f.signer.public(), &output).unwrap();
    assert_eq!(copied.manifest(), &f.manifest);
    assert!(f.profile().matches(copied.manifest()));
    for record in &f.manifest.files {
        assert_eq!(
            fs::read(f.bundle.join(&record.path)).unwrap(),
            fs::read(output.join(&record.path)).unwrap()
        );
    }
    assert!(release_payload::stage(&f.bundle, &f.signer.public(), &output).is_err());
    assert!(
        release_payload::stage(&f.bundle, &f.signer.public(), &f.bundle.join("nested")).is_err()
    );
    assert!(!f.bundle.join("nested").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&output).unwrap().permissions().mode() & 0o777,
            0o700
        );
        for record in &f.manifest.files {
            assert_eq!(
                fs::metadata(output.join(&record.path))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
}

#[test]
fn altered_missing_extra_oversized_and_self_inventoried_members_fail() {
    let mut f = Fixture::new();
    let name = "materials/notices.json";
    let original = fs::read(f.bundle.join(name)).unwrap();
    fs::write(f.bundle.join(name), b"changed").unwrap();
    assert!(!f.verify());
    fs::write(f.bundle.join(name), &original).unwrap();
    assert!(f.verify());
    fs::remove_file(f.bundle.join(name)).unwrap();
    assert!(!f.verify());
    fs::write(f.bundle.join(name), &original).unwrap();
    fs::write(f.bundle.join("extra"), b"unlisted").unwrap();
    assert!(!f.verify());
    fs::remove_file(f.bundle.join("extra")).unwrap();
    fs::create_dir(f.bundle.join("empty-extra")).unwrap();
    assert!(!f.verify());
    fs::remove_dir(f.bundle.join("empty-extra")).unwrap();
    let record = f
        .manifest
        .files
        .iter_mut()
        .find(|r| r.path == name)
        .unwrap();
    record.size -= 1;
    f.sign();
    assert!(!f.verify());
    f.manifest.files.push(FileRecord {
        path: release_payload::MANIFEST.into(),
        size: 1,
        sha256: "00".repeat(32),
    });
    f.manifest.files.sort_by(|a, b| a.path.cmp(&b.path));
    f.sign();
    assert!(!f.verify());
    assert!(
        release_payload::stage(&f.bundle, &f.signer.public(), &f.root.join("invalid-stage"))
            .is_err()
    );
    assert!(!f.root.join("invalid-stage").exists());
}

#[test]
fn valid_signatures_do_not_make_windows_or_wrong_architecture_payloads_installable() {
    let mut f = Fixture::new();
    f.replace(
        "bbsbox-node",
        b"MZ windows fixture with a valid release signature",
    );
    assert!(!f.verify());
    let mut elf = vec![0; 64];
    elf[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    elf[16] = 3;
    elf[18] = 0xb7;
    f.replace("bbsbox-node", &elf);
    assert!(!f.verify());
    elf[18] = 0x3e;
    f.replace("bbsbox-node", &elf);
    assert!(f.verify());
    elf[5] = 2;
    f.replace("bbsbox-node", &elf);
    assert!(!f.verify());
}

#[test]
fn compiled_claim_mismatches_and_wrong_authority_are_rejected() {
    let f = Fixture::new();
    let mut cases = vec![];
    let mut p = f.profile();
    p.format = "wrong".into();
    cases.push(p);
    let mut p = f.profile();
    p.version = "0.2.0".into();
    cases.push(p);
    let mut p = f.profile();
    p.target = "windows-amd64".into();
    cases.push(p);
    let mut p = f.profile();
    p.core_schema = 4;
    cases.push(p);
    let mut p = f.profile();
    p.readable_core_schemas = vec![2, 3];
    cases.push(p);
    let mut p = f.profile();
    p.module_migrations[0].sha256 = "00".repeat(32);
    cases.push(p);
    let mut p = f.profile();
    p.module_migrations.pop();
    cases.push(p);
    for p in cases {
        assert!(!p.matches(&f.manifest));
    }
    let wrong = ReleaseSigner::generate().unwrap();
    assert!(release_payload::verify(&f.bundle, &wrong.public()).is_err());
    assert!(release_payload::stage(&f.bundle, &wrong.public(), &f.root.join("wrong")).is_err());
    assert!(!f.root.join("wrong").exists());
}

#[cfg(unix)]
#[test]
fn symlink_members_directories_bundle_roots_and_ancestors_are_rejected() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let linked = f.root.join("linked");
    symlink(&f.bundle, &linked).unwrap();
    assert!(release_payload::verify(&linked, &f.signer.public()).is_err());
    assert!(release_payload::verify(&linked.join(".."), &f.signer.public()).is_err());
    let materials = f.bundle.join("materials");
    let moved = f.root.join("materials");
    fs::rename(&materials, &moved).unwrap();
    symlink(&moved, &materials).unwrap();
    assert!(!f.verify());
    fs::remove_file(&materials).unwrap();
    fs::rename(&moved, &materials).unwrap();
    let member = f.bundle.join("bbsbox-node");
    let moved = f.root.join("executable");
    fs::rename(&member, &moved).unwrap();
    symlink(&moved, &member).unwrap();
    assert!(!f.verify());
}
