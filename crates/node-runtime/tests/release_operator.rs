// SPDX-License-Identifier: AGPL-3.0-or-later
//! Producer/public-inspection fixtures, never production trust or an installed node.
use bbsbox_maintenance::{
    release::{self, Manifest},
    release_payload,
};
use bbsbox_node_runtime::release_operator;
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

struct Fixture {
    _temporary: tempfile::TempDir,
    root: PathBuf,
    bundle: PathBuf,
    key: PathBuf,
    input: PathBuf,
    public: String,
}
impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().canonicalize().unwrap();
        let bundle = root.join("bundle");
        fs::create_dir(&bundle).unwrap();
        for name in release::REQUIRED_FILES {
            let path = bundle.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let raw = if name == "bbsbox-node" {
                let mut bytes = vec![0; 64];
                bytes[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
                bytes[16] = 3;
                bytes[18] = 0x3e;
                bytes
            } else {
                b"fixture only, not source-complete or executable".to_vec()
            };
            fs::write(path, raw).unwrap();
        }
        let key = root.join("producer.key");
        let generated = release_operator::keygen(&key).unwrap();
        assert_eq!(generated.as_object().unwrap().len(), 3);
        assert!(generated.get("secret").is_none());
        let vector: serde_json::Value = serde_json::from_str(include_str!(
            "../../../protocol/fixtures/release-manifest-v1.json"
        ))
        .unwrap();
        let encoded = hex::decode(vector["payload_hex"].as_str().unwrap()).unwrap();
        let mut manifest: Manifest = serde_json::from_slice(&encoded).unwrap();
        manifest.sequence = 1;
        manifest.issuer = generated["key_id"].as_str().unwrap().into();
        manifest.next_key = None;
        manifest.files = release_payload::inventory_unsigned(&bundle).unwrap();
        let input = root.join("input.json");
        fs::write(&input, serde_json::to_vec(&manifest).unwrap()).unwrap();
        Self {
            _temporary: temporary,
            root,
            bundle,
            key,
            input,
            public: generated["public_key"].as_str().unwrap().into(),
        }
    }
    fn sign(&self) -> bbsbox_node_runtime::Result<serde_json::Value> {
        release_operator::sign(&self.bundle, &self.input, &self.key)
    }
    fn command(
        &self,
        name: &str,
        output: Option<&Path>,
    ) -> bbsbox_node_runtime::Result<serde_json::Value> {
        let mut args = vec![
            OsString::from(name),
            "--bundle".into(),
            self.bundle.as_os_str().into(),
            "--public-key".into(),
            self.public.clone().into(),
        ];
        if let Some(output) = output {
            args.extend(["--output".into(), output.as_os_str().into()]);
        }
        release_operator::command(&args)
    }
}
#[test]
fn separate_producer_key_signs_exact_payload_then_public_commands_inspect_and_stage() {
    let f = Fixture::new();
    let original = fs::read(&f.key).unwrap();
    assert!(release_operator::keygen(&f.key).is_err());
    assert_eq!(fs::read(&f.key).unwrap(), original);
    let signed = f.sign().unwrap();
    assert_eq!(signed["installation_authority"], false);
    assert_eq!(signed["files"], 9);
    assert!(!f.bundle.join("producer.key").exists());
    assert!(f.sign().is_err());
    assert_eq!(f.command("release-inspect", None).unwrap(), signed);
    let stage = f.root.join("stage");
    assert_eq!(f.command("release-stage", Some(&stage)).unwrap(), signed);
    assert!(f.command("release-stage", Some(&stage)).is_err());
    fs::write(f.bundle.join("materials/notices.json"), b"tampered").unwrap();
    assert!(f.command("release-inspect", None).is_err());
    let invalid = f.root.join("invalid-stage");
    assert!(f.command("release-stage", Some(&invalid)).is_err());
    assert!(!invalid.exists());
}
#[test]
fn role_wrong_inventory_and_strict_options_fail_before_manifest_publication() {
    let f = Fixture::new();
    fs::write(&f.key, b"AGE-SECRET-KEY-fixture-is-not-a-release-key\n").unwrap();
    assert!(f.sign().is_err());
    assert!(!f.bundle.join("manifest.cose").exists());
    for args in [
        vec!["release-keygen"],
        vec!["release-keygen", "--output", "a", "--output", "b"],
        vec!["release-stage", "--bundle", "a"],
        vec![
            "release-inspect",
            "--bundle",
            "a",
            "--public-key",
            "NODE:WRONG",
        ],
        vec!["release-unknown"],
    ] {
        assert!(
            release_operator::command(&args.into_iter().map(OsString::from).collect::<Vec<_>>())
                .is_err()
        );
    }
    let f = Fixture::new();
    fs::write(f.bundle.join("extra"), b"unlisted").unwrap();
    assert!(f.sign().is_err());
    assert!(!f.bundle.join("manifest.cose").exists());
}

#[test]
fn operator_control_commands_fence_boot_until_exact_completion_and_recheck_the_binary() {
    let f = Fixture::new();
    let signed = f.sign().unwrap();
    let control = f.root.join("control");
    let path = |p: &Path| p.as_os_str().to_os_string();
    let init = vec![
        "release-control-init".into(),
        "--directory".into(),
        path(&control),
        "--public-key".into(),
        f.public.clone().into(),
    ];
    let initial = release_operator::command(&init).unwrap();
    assert_eq!(initial["state"]["revision"], 0);
    assert!(release_operator::command(&init).is_err());
    let check = vec![
        "release-control-check".into(),
        "--directory".into(),
        path(&control),
        "--release".into(),
        path(&f.bundle),
    ];
    assert!(release_operator::command(&check).is_err());
    let request = "01".repeat(16);
    let begun = release_operator::command(&[
        "release-control-begin".into(),
        "--directory".into(),
        path(&control),
        "--revision".into(),
        "0".into(),
        "--bundle".into(),
        path(&f.bundle),
        "--request".into(),
        request.clone().into(),
    ])
    .unwrap();
    assert_eq!(begun["state"]["highest_sequence"], 1);
    assert!(release_operator::command(&check).is_err());
    release_operator::command(&[
        "release-control-complete".into(),
        "--directory".into(),
        path(&control),
        "--revision".into(),
        "1".into(),
        "--request".into(),
        request.into(),
        "--release-id".into(),
        signed["release_id"].as_str().unwrap().into(),
    ])
    .unwrap();
    let accepted = release_operator::command(&check).unwrap();
    assert_eq!(accepted["format"], "bbsbox.release-boot-check/v1");
    let binary = f.bundle.join("bbsbox-node");
    let original = fs::read(&binary).unwrap();
    fs::write(&binary, b"changed").unwrap();
    assert!(release_operator::command(&check).is_err());
    fs::write(binary, original).unwrap();
    assert!(release_operator::command(&check).is_ok());
    assert!(!f.root.join("node.sqlite").exists());
    assert!(!control.join("node.sqlite").exists());
}
#[cfg(unix)]
#[test]
fn producer_key_must_be_private_and_not_a_link() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let f = Fixture::new();
    assert_eq!(
        fs::metadata(&f.key).unwrap().permissions().mode() & 0o777,
        0o600
    );
    fs::set_permissions(&f.key, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(f.sign().is_err());
    assert!(!f.bundle.join("manifest.cose").exists());
    fs::set_permissions(&f.key, fs::Permissions::from_mode(0o600)).unwrap();
    let moved = f.root.join("moved.key");
    fs::rename(&f.key, &moved).unwrap();
    symlink(&moved, &f.key).unwrap();
    assert!(f.sign().is_err());
}
