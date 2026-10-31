// SPDX-License-Identifier: AGPL-3.0-or-later
//! Emit public conformance data from fixed, publicly known test seeds. Never used by runtime code.
use bbsbox_credentials::{DeviceBody, Purpose, Purposes, issue};
use ed25519_dalek::{Signer, SigningKey};
fn main() {
    let root = SigningKey::from_bytes(&[7; 32]);
    let device = SigningKey::from_bytes(&[9; 32]);
    let body = DeviceBody::new(
        root.verifying_key().to_bytes(),
        device.verifying_key().to_bytes(),
        [11; 16],
        Purposes::new(&[Purpose::Authorship, Purpose::IdentityProof]).unwrap(),
    )
    .unwrap();
    let credential = issue(body, |message| root.sign(message).to_bytes()).unwrap();
    println!("{}",serde_json::to_string_pretty(&serde_json::json!({"format":"bbsbox.device-credential/v1","fixture_root_seed_hex":hex::encode([7;32]),"root_public_hex":hex::encode(root.verifying_key().to_bytes()),"device_public_hex":hex::encode(device.verifying_key().to_bytes()),"incarnation_hex":hex::encode([11;16]),"purposes":9,"person_id":credential.body().person_id().to_string(),"device_id":credential.body().device_id().to_string(),"credential_id":credential.id().to_string(),"cose_hex":hex::encode(credential.encoded())})).unwrap());
}
