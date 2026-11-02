// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_maintenance::release_payload::RuntimeProfile;
#[test]
fn real_binary_reports_exact_compiled_profile_without_node_initialization() {
    let directory = tempfile::tempdir().unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_bbsbox-node"))
        .arg("--release-profile")
        .current_dir(directory.path())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let profile: RuntimeProfile = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(profile, bbsbox_node_runtime::release_profile());
    assert_eq!(profile.core_schema, bbsbox_storage::schema_version());
    assert_eq!(profile.readable_core_schemas, vec![profile.core_schema]);
    assert_eq!(
        profile.module_migrations.len(),
        bbsbox_node_runtime::migrations().len()
    );
    assert!(
        std::fs::read_dir(directory.path())
            .unwrap()
            .next()
            .is_none()
    );
}

#[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
#[test]
fn real_linux_candidate_is_signed_staged_and_compared_to_its_compiled_registry() {
    use bbsbox_maintenance::{
        Limits, archive,
        release::{self, Manifest, ReleaseSigner},
        release_payload,
    };
    use std::{fs, os::unix::fs::PermissionsExt, process::Command};
    let temporary = tempfile::tempdir().unwrap();
    let bundle = temporary.path().join("bundle");
    fs::create_dir(&bundle).unwrap();
    let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for name in release::REQUIRED_FILES {
        let target = bundle.join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        match name {
            "bbsbox-node" => {
                fs::copy(env!("CARGO_BIN_EXE_bbsbox-node"), target).unwrap();
            }
            "bbsbox.service" => {
                fs::copy(repository.join("deploy/systemd/bbsbox.service"), target).unwrap();
            }
            "node.example.json" => {
                fs::copy(repository.join("deploy/node.example.json"), target).unwrap();
            }
            name if name.starts_with("materials/") => {
                fs::write(
                    target,
                    b"test placeholder, not corresponding source or an installable release",
                )
                .unwrap();
            }
            _ => {
                fs::copy(repository.join(name), target).unwrap();
            }
        }
    }
    let profile = bbsbox_node_runtime::release_profile();
    let signer = ReleaseSigner::generate().unwrap();
    let manifest = Manifest {
        format: release::DOMAIN.into(),
        issuer: signer.id().to_string(),
        sequence: 1,
        version: profile.version.clone(),
        target: profile.target.clone(),
        created_at: 0,
        source_commit: "01".repeat(20),
        core_schema: profile.core_schema,
        readable_core_schemas: profile.readable_core_schemas.clone(),
        module_migrations: profile.module_migrations.clone(),
        files: archive::inventory(&bundle, Limits::default()).unwrap(),
        next_key: None,
    };
    let signed = release::sign(manifest, &signer).unwrap();
    fs::write(bundle.join(release_payload::MANIFEST), signed.encoded()).unwrap();
    let stage = temporary.path().join("stage");
    let staged = release_payload::stage(&bundle, &signer.public(), &stage).unwrap();
    let binary = stage.join("bbsbox-node");
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    let output = Command::new(binary)
        .arg("--release-profile")
        .current_dir(temporary.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: RuntimeProfile = serde_json::from_slice(&output.stdout).unwrap();
    assert!(actual.matches(staged.manifest()));
    let mut false_claim = staged.manifest().clone();
    false_claim.module_migrations[0].sha256 = "00".repeat(32);
    let authenticated_false_claim = release::sign(false_claim, &signer).unwrap();
    assert!(!actual.matches(authenticated_false_claim.manifest()));
    assert_eq!(
        fs::read_dir(temporary.path()).unwrap().count(),
        2,
        "profile must not initialize node state"
    );
}
