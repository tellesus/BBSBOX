// SPDX-License-Identifier: AGPL-3.0-or-later
//! Public signature fixture only; these synthetic file commitments are not installable software.
use bbsbox_maintenance::{
    client_release::{self, Manifest},
    inventory::FileRecord,
    release::ReleaseSigner,
};
use coset::{CborSerializable, CoseSign1};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;
fn main() {
    let seed = [51; 32];
    let signer = ReleaseSigner::from_secret(Zeroizing::new(seed)).unwrap();
    let next = ReleaseSigner::from_secret(Zeroizing::new([52; 32])).unwrap();
    let mut files = client_release::REQUIRED
        .into_iter()
        .map(|path| {
            let bytes = format!("public client vector: {path}");
            FileRecord {
                path: path.into(),
                size: bytes.len() as u64,
                sha256: hex::encode(Sha256::digest(bytes.as_bytes())),
            }
        })
        .collect::<Vec<_>>();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let manifest = Manifest {
        format: client_release::DOMAIN.into(),
        issuer: signer.id().to_string(),
        sequence: i64::MAX as u64,
        version: "0.1.0-vector".into(),
        target: client_release::TARGET.into(),
        source_commit: "03".repeat(20),
        vault_profiles: vec![1],
        files,
        next_key: Some(next.public()),
    };
    let signed = client_release::sign(manifest, &signer).unwrap();
    let cose = CoseSign1::from_slice(signed.encoded()).unwrap();
    println!("{}",serde_json::to_string_pretty(&serde_json::json!({
        "warning":"Public fixture seed and synthetic commitments; never a production key or installable package",
        "domain":client_release::DOMAIN,"fixture_seed_hex":hex::encode(seed),"public_hex":hex::encode(signer.public()),
        "key_id":signer.id().to_string(),"payload_hex":hex::encode(cose.payload.unwrap()),"cose_hex":hex::encode(signed.encoded()),
        "manifest_sha256":hex::encode(signed.id()),"sequence":"9223372036854775807"
    })).unwrap());
}
