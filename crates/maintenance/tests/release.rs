// SPDX-License-Identifier: AGPL-3.0-or-later
//! Signature/profile fixtures only; payload verification and install permission are separate.
use bbsbox_maintenance::{
    inventory::FileRecord,
    release::{self, Error, Manifest, ModuleMigration, ReleaseSigner},
};
use coset::{CborSerializable, CoseSign1};
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

fn signer() -> ReleaseSigner {
    ReleaseSigner::from_secret(Zeroizing::new([31; 32])).unwrap()
}

#[test]
fn public_cross_implementation_vector_matches_rust_without_integer_rounding() {
    let vector: serde_json::Value = serde_json::from_str(include_str!(
        "../../../protocol/fixtures/release-manifest-v1.json"
    ))
    .unwrap();
    let public: [u8; 32] = hex::decode(vector["public_hex"].as_str().unwrap())
        .unwrap()
        .try_into()
        .unwrap();
    let encoded = hex::decode(vector["cose_hex"].as_str().unwrap()).unwrap();
    let verified = release::verify_manifest(&encoded, &public).unwrap();
    assert_eq!(verified.manifest().sequence, u64::MAX);
    assert_eq!(verified.manifest().created_at, i64::MIN);
    assert_eq!(verified.manifest().issuer, vector["key_id"]);
    assert_eq!(hex::encode(verified.id()), vector["manifest_sha256"]);
    let signer = signer();
    assert_eq!(signer.public(), public);
    assert_eq!(
        release::sign(verified.manifest().clone(), &signer)
            .unwrap()
            .encoded(),
        encoded
    );
}
fn manifest(signer: &ReleaseSigner) -> Manifest {
    let mut files = release::REQUIRED_FILES
        .into_iter()
        .map(|path| {
            let bytes = format!("public fixture: {path}").into_bytes();
            FileRecord {
                path: path.into(),
                size: bytes.len() as u64,
                sha256: hex::encode(Sha256::digest(&bytes)),
            }
        })
        .collect::<Vec<_>>();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let mut modules = bbsbox_identity::migrations()
        .iter()
        .map(ModuleMigration::from)
        .collect::<Vec<_>>();
    modules.sort_by(|a, b| (&a.module, a.version).cmp(&(&b.module, b.version)));
    Manifest {
        format: release::DOMAIN.into(),
        issuer: signer.id().to_string(),
        sequence: 1,
        version: "0.1.0-test".into(),
        target: "debian13-amd64".into(),
        created_at: 0,
        source_commit: "01".repeat(20),
        core_schema: 3,
        readable_core_schemas: vec![3],
        module_migrations: modules,
        files,
        next_key: None,
    }
}

#[test]
fn manifest_is_bound_to_expected_release_key_domain_and_exact_immutable_bytes() {
    let signer = signer();
    let manifest = manifest(&signer);
    let signed = release::sign(manifest.clone(), &signer).unwrap();
    let verified = release::verify_manifest(signed.encoded(), &signer.public()).unwrap();
    assert_eq!(verified.manifest(), &manifest);
    assert_eq!(verified.signer(), &signer.public());
    assert_eq!(
        *verified.id(),
        <[u8; 32]>::from(Sha256::digest(signed.encoded()))
    );
    let other = ReleaseSigner::generate().unwrap();
    assert!(matches!(
        release::verify_manifest(signed.encoded(), &other.public()),
        Err(Error::WrongKey)
    ));
    let node = bbsbox_credentials::NodeId::from_public_key(&signer.public()).unwrap();
    assert_ne!(signer.id().to_string(), node.to_string());
    let mut changed = signed.encoded().to_vec();
    *changed.last_mut().unwrap() ^= 1;
    assert!(matches!(
        release::verify_manifest(&changed, &signer.public()),
        Err(Error::InvalidSignature)
    ));
    let mut cose = CoseSign1::from_slice(signed.encoded()).unwrap();
    cose.signature = SigningKey::from_bytes(&[31; 32])
        .sign(&cose.tbs_data(b"bbsbox.archive-inventory/v1"))
        .to_bytes()
        .to_vec();
    assert!(matches!(
        release::verify_manifest(&cose.to_vec().unwrap(), &signer.public()),
        Err(Error::InvalidSignature)
    ));
    let mut changed = manifest;
    changed.sequence += 1;
    cose = CoseSign1::from_slice(signed.encoded()).unwrap();
    cose.payload = Some(serde_json::to_vec(&changed).unwrap());
    assert!(matches!(
        release::verify_manifest(&cose.to_vec().unwrap(), &signer.public()),
        Err(Error::InvalidSignature)
    ));
}

#[test]
fn sequence_and_time_integer_boundaries_and_signed_key_rotation_declaration_round_trip() {
    let signer = signer();
    let next = ReleaseSigner::generate().unwrap();
    let mut manifest = manifest(&signer);
    manifest.sequence = u64::MAX;
    manifest.created_at = i64::MIN;
    manifest.next_key = Some(next.public());
    let signed = release::sign(manifest.clone(), &signer).unwrap();
    let verified = release::verify_manifest(signed.encoded(), &signer.public()).unwrap();
    assert_eq!(verified.manifest(), &manifest);
    assert_eq!(verified.manifest().next_key, Some(next.public()));
    assert!(
        matches!(
            release::verify_manifest(signed.encoded(), &next.public()),
            Err(Error::WrongKey)
        ),
        "rotation declaration is authorized by the prior key, never self-approved"
    );
    manifest.next_key = Some(signer.public());
    assert!(release::sign(manifest.clone(), &signer).is_err());
    manifest.next_key = Some([0; 32]);
    assert!(matches!(
        release::sign(manifest, &signer),
        Err(Error::InvalidKey)
    ));
}

#[test]
fn missing_materials_unsafe_paths_module_gaps_and_unsupported_targets_cannot_be_signed() {
    let signer = signer();
    let original = manifest(&signer);
    for index in 0..9 {
        let mut candidate = original.clone();
        candidate.files.remove(index);
        assert!(release::sign(candidate, &signer).is_err());
    }
    let mut cases = vec![];
    let mut m = original.clone();
    m.sequence = 0;
    cases.push(m);
    let mut m = original.clone();
    m.target = "windows-amd64".into();
    cases.push(m);
    let mut m = original.clone();
    m.files[0].path = "../outside".into();
    cases.push(m);
    let mut m = original.clone();
    m.files.swap(0, 1);
    cases.push(m);
    let mut m = original.clone();
    m.files[0].size = 0;
    cases.push(m);
    let mut m = original.clone();
    m.files[0].size = 2 * 1024 * 1024 * 1024 + 1;
    cases.push(m);
    let mut m = original.clone();
    for file in &mut m.files {
        file.size = 1024 * 1024 * 1024;
    }
    cases.push(m);
    let mut m = original.clone();
    m.module_migrations.remove(0);
    cases.push(m);
    let mut m = original.clone();
    m.module_migrations[0].sha256 = "not-a-hash".into();
    cases.push(m);
    let mut m = original.clone();
    m.module_migrations.swap(0, 1);
    cases.push(m);
    let mut m = original.clone();
    m.readable_core_schemas = vec![3, 2];
    cases.push(m);
    let mut m = original.clone();
    m.readable_core_schemas = vec![2];
    cases.push(m);
    let mut m = original.clone();
    m.version = "bad\nversion".into();
    cases.push(m);
    let mut m = original.clone();
    m.source_commit = "A".repeat(40);
    cases.push(m);
    let mut m = original.clone();
    m.issuer = "node:wrong-role".into();
    cases.push(m);
    for (index, candidate) in cases.into_iter().enumerate() {
        assert!(release::sign(candidate, &signer).is_err(), "case {index}");
    }
}

#[test]
fn malformed_noncanonical_legacy_algorithm_and_oversized_envelopes_fail_closed() {
    let signer = signer();
    let signed = release::sign(manifest(&signer), &signer).unwrap();
    let mut legacy = signed.encoded().to_vec();
    assert_eq!(legacy[5], 0x32);
    legacy[5] = 0x27;
    let mut trailing = signed.encoded().to_vec();
    trailing.push(0);
    for bytes in [
        vec![],
        vec![0x84, 0x5f],
        vec![0x84, 0x5a, 0xff, 0xff, 0xff, 0xff],
        legacy,
        trailing,
        vec![0; release::MAX_MANIFEST + 129],
    ] {
        assert!(release::verify_manifest(&bytes, &signer.public()).is_err());
    }
    let mut cose = CoseSign1::from_slice(signed.encoded()).unwrap();
    cose.payload = Some(serde_json::to_vec_pretty(signed.manifest()).unwrap());
    cose.signature = SigningKey::from_bytes(&[31; 32])
        .sign(&cose.tbs_data(release::DOMAIN.as_bytes()))
        .to_bytes()
        .to_vec();
    assert!(matches!(
        release::verify_manifest(&cose.to_vec().unwrap(), &signer.public()),
        Err(Error::InvalidManifest)
    ));
    let mut oversized = manifest(&signer);
    oversized.module_migrations = (0..512)
        .map(|i| ModuleMigration {
            module: format!("m{i:03}{}", "x".repeat(120)),
            version: 1,
            sha256: "00".repeat(32),
        })
        .collect();
    assert!(matches!(
        release::sign(oversized, &signer),
        Err(Error::LimitExceeded)
    ));
}
