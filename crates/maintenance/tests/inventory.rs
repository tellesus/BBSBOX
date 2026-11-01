// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_identity::node::NodeSigner;
use bbsbox_maintenance::{
    Error, Limits,
    inventory::{self, FileRecord},
};
use bbsbox_storage::Database;
use sha2::{Digest, Sha256};

fn signer(directory: &std::path::Path) -> NodeSigner {
    let mut database = Database::open(
        directory.join("node.sqlite"),
        &bbsbox_identity::migrations(),
    )
    .unwrap();
    NodeSigner::start(&mut database).unwrap()
}
fn file(path: &str) -> FileRecord {
    FileRecord {
        path: path.into(),
        size: 0,
        sha256: hex::encode(Sha256::digest([])),
    }
}

#[test]
fn inventory_is_bound_to_expected_node_domain_and_exact_canonical_bytes() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let first = signer(first.path());
    let second = signer(second.path());
    let signed = inventory::sign(
        vec![file("core/manifest.json")],
        0,
        &first,
        Limits::default(),
    )
    .unwrap();
    let decoded = inventory::verify(signed.encoded(), first.id(), Limits::default()).unwrap();
    assert_eq!(decoded.inventory(), signed.inventory());
    assert_eq!(decoded.node(), first.id());
    assert_eq!(
        *decoded.id(),
        <[u8; 32]>::from(Sha256::digest(signed.encoded()))
    );
    assert!(matches!(
        inventory::verify(signed.encoded(), second.id(), Limits::default()),
        Err(Error::WrongNode)
    ));
    let mut tampered = signed.encoded().to_vec();
    *tampered.last_mut().unwrap() ^= 1;
    assert!(matches!(
        inventory::verify(&tampered, first.id(), Limits::default()),
        Err(Error::InvalidSignature)
    ));
    let mut legacy = signed.encoded().to_vec();
    assert_eq!(&legacy[3..9], &[0xa2, 1, 0x32, 4, 0x58, 0x20]);
    legacy[5] = 0x27;
    assert!(inventory::verify(&legacy, first.id(), Limits::default()).is_err());
    let mut trailing = signed.encoded().to_vec();
    trailing.push(0);
    assert!(inventory::verify(&trailing, first.id(), Limits::default()).is_err());
    let again = inventory::sign(
        vec![file("core/manifest.json")],
        0,
        &first,
        Limits::default(),
    )
    .unwrap();
    assert_ne!(again.id(), signed.id());
}

#[test]
fn large_inventory_uses_canonical_32_bit_cbor_lengths_and_enforces_bounds() {
    let directory = tempfile::tempdir().unwrap();
    let signer = signer(directory.path());
    let files = (0..1000)
        .map(|index| file(&format!("blob-{index:04}")))
        .collect::<Vec<_>>();
    let signed = inventory::sign(files.clone(), i64::MIN, &signer, Limits::default()).unwrap();
    assert!(signed.encoded().len() > 65536);
    assert_eq!(
        inventory::verify(signed.encoded(), signer.id(), Limits::default())
            .unwrap()
            .inventory()
            .created_at,
        i64::MIN
    );
    assert!(matches!(
        inventory::sign(
            files,
            0,
            &signer,
            Limits {
                max_files: 1,
                ..Limits::default()
            }
        ),
        Err(Error::LimitExceeded)
    ));
    assert!(
        inventory::verify(
            signed.encoded(),
            signer.id(),
            Limits {
                max_manifest: 65535,
                ..Limits::default()
            }
        )
        .is_err()
    );
}

#[test]
fn unsafe_duplicate_unsorted_or_unbounded_member_records_never_sign() {
    let directory = tempfile::tempdir().unwrap();
    let signer = signer(directory.path());
    for path in [
        "../outside",
        "/absolute",
        "C:/outside",
        "a\\outside",
        "a/../outside",
        "a//b",
        "a/CON.txt",
        "a/LPT1",
        "a/trailing.",
        "a/\0key",
    ] {
        assert!(
            inventory::sign(vec![file(path)], 0, &signer, Limits::default()).is_err(),
            "{path}"
        );
    }
    for files in [
        vec![file("same"), file("same")],
        vec![file("z"), file("a")],
        vec![],
    ] {
        assert!(inventory::sign(files, 0, &signer, Limits::default()).is_err());
    }
    let mut oversized = file("data");
    oversized.size = 2;
    assert!(matches!(
        inventory::sign(
            vec![oversized],
            0,
            &signer,
            Limits {
                max_member_bytes: 1,
                max_total_bytes: 1,
                ..Limits::default()
            }
        ),
        Err(Error::LimitExceeded)
    ));
}

#[test]
fn malformed_deep_or_false_length_envelopes_fail_before_generic_cbor_parsing() {
    let directory = tempfile::tempdir().unwrap();
    let signer = signer(directory.path());
    for bytes in [
        vec![],
        vec![0x84, 0x5f],
        vec![0x84, 0x5a, 0xff, 0xff, 0xff, 0xff],
        vec![0x84, 0x58, 38, 0x9f, 0x9f, 0x9f],
    ] {
        assert!(inventory::verify(&bytes, signer.id(), Limits::default()).is_err());
    }
}

#[test]
fn public_inventory_vector_verifies_with_exact_integer_and_domain_profile() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../protocol/fixtures/archive-inventory-v1.json"
    ))
    .unwrap();
    let public: [u8; 32] = hex::decode(fixture["public_hex"].as_str().unwrap())
        .unwrap()
        .try_into()
        .unwrap();
    let node = bbsbox_credentials::NodeId::from_public_key(&public).unwrap();
    let encoded = hex::decode(fixture["cose_hex"].as_str().unwrap()).unwrap();
    let signed = inventory::verify(&encoded, node, Limits::default()).unwrap();
    assert_eq!(signed.inventory().created_at, i64::MIN);
    assert_eq!(hex::encode(signed.id()), fixture["inventory_sha256"]);
    assert_eq!(node.to_string(), fixture["node"]);
}
