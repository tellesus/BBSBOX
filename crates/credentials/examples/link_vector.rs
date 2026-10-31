// SPDX-License-Identifier: AGPL-3.0-or-later
//! Public conformance fixture from publicly known test seeds.
use bbsbox_credentials::{
    ContactCard, NodeId,
    linking::{Challenge, prove},
};
use ed25519_dalek::{Signer, SigningKey};
fn main() {
    let root = SigningKey::from_bytes(&[7; 32]);
    let node = SigningKey::from_bytes(&[9; 32]);
    let card = ContactCard::new(root.verifying_key().to_bytes()).unwrap();
    let node_id = NodeId::from_public_key(&node.verifying_key().to_bytes()).unwrap();
    let challenge = Challenge::new(node_id, [12; 16], card.person_id(), [13; 32]).unwrap();
    let proof = prove(&challenge, &card, |message| root.sign(message).to_bytes()).unwrap();
    println!("{}",serde_json::to_string_pretty(&serde_json::json!({"format":"bbsbox.account-link/v1","fixture_root_seed_hex":hex::encode([7;32]),"root_public_hex":hex::encode(card.root()),"node_public_hex":hex::encode(node.verifying_key().to_bytes()),"node_id":node_id.to_string(),"account_hex":hex::encode([12;16]),"nonce_hex":hex::encode([13;32]),"person_id":card.person_id().to_string(),"cose_hex":hex::encode(proof)})).unwrap());
}
