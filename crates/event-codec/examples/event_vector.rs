// SPDX-License-Identifier: AGPL-3.0-or-later
//! Public conformance vector using only publicly known fixture seeds.
use bbsbox_credentials::Credential;
use bbsbox_event_codec::*;
use ed25519_dalek::{Signer, SigningKey};
fn main() {
    let vector: serde_json::Value = serde_json::from_str(include_str!(
        "../../credentials/tests/fixtures/device-v2.json"
    ))
    .unwrap();
    let credential =
        Credential::verify(&hex::decode(vector["cose_hex"].as_str().unwrap()).unwrap()).unwrap();
    let key = SigningKey::from_bytes(&[9; 32]);
    let body = b"Hello\nneighbors.";
    let draft = Draft {
        kind: "fixture.post".into(),
        schema: 1,
        scope: Scope {
            resource: "forum:fixture".into(),
            audience: "public".into(),
            distribution: Distribution::Shared,
        },
        sequence: u64::MAX,
        previous: Some(EventId::from_bytes([12; 32])),
        dependencies: vec![
            Dependency::event(EventId::from_bytes([13; 32])),
            Dependency::event(EventId::from_bytes([14; 32])),
        ],
        claimed_time: i64::MIN,
        extensions: vec![Extension {
            id: 7,
            required: false,
            data: b"annotation".to_vec(),
        }],
    };
    let event = sign(
        draft,
        Actor::device(&credential),
        body,
        Some(&credential),
        |message| Ok(key.sign(message).to_bytes()),
    )
    .unwrap();
    println!("{}",serde_json::to_string_pretty(&serde_json::json!({"format":DOMAIN,"fixture_device_seed_hex":hex::encode([9;32]),"credential_id":credential.id().to_string(),"actor_subject_hex":hex::encode(event.metadata().actor.subject),"signer_public_hex":hex::encode(key.verifying_key().to_bytes()),"incarnation_hex":hex::encode(event.metadata().actor.incarnation),"stream_hex":hex::encode(event.metadata().stream),"sequence":u64::MAX.to_string(),"previous_hex":hex::encode([12;32]),"dependencies_hex":[hex::encode([13;32]),hex::encode([14;32])],"claimed_time":i64::MIN.to_string(),"body_hex":hex::encode(body),"event_id":event.id().to_string(),"cose_hex":hex::encode(event.commitment().encoded())})).unwrap());
}
