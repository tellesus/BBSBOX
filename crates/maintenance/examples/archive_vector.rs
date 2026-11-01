// SPDX-License-Identifier: AGPL-3.0-or-later
//! Public, deterministic test key and inventory. Never use this key for a node.
use bbsbox_credentials::NodeId;
use bbsbox_maintenance::inventory::{DOMAIN, FileRecord, Inventory};
use coset::{CborSerializable, CoseSign1Builder, HeaderBuilder, iana};
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};

fn main() {
    let seed = [7; 32];
    let key = SigningKey::from_bytes(&seed);
    let public = key.verifying_key().to_bytes();
    let inventory = Inventory {
        format: DOMAIN.into(),
        node: NodeId::from_public_key(&public).unwrap().to_string(),
        archive: hex::encode([1; 16]),
        created_at: i64::MIN,
        incarnation: [2; 16],
        files: vec![FileRecord {
            path: "core/manifest.json".into(),
            size: 0,
            sha256: hex::encode(Sha256::digest([])),
        }],
    };
    let payload = serde_json::to_vec(&inventory).unwrap();
    let mut cose = CoseSign1Builder::new()
        .protected(
            HeaderBuilder::new()
                .algorithm(iana::Algorithm::Ed25519)
                .key_id(public.to_vec())
                .build(),
        )
        .payload(payload.clone())
        .build();
    cose.signature = key
        .sign(&cose.tbs_data(DOMAIN.as_bytes()))
        .to_bytes()
        .to_vec();
    let encoded = cose.to_vec().unwrap();
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "warning": "Public fixture key; never use for real node identity",
            "domain": DOMAIN,
            "fixture_seed_hex": hex::encode(seed),
            "public_hex": hex::encode(public),
            "node": inventory.node,
            "payload_hex": hex::encode(payload),
            "cose_hex": hex::encode(&encoded),
            "inventory_sha256": hex::encode(Sha256::digest(&encoded)),
        }))
        .unwrap()
    );
}
