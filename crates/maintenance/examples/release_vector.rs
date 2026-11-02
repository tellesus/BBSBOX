// SPDX-License-Identifier: AGPL-3.0-or-later
//! Public release profile/signature fixture. It is not an installable software bundle.
use bbsbox_maintenance::{
    inventory::FileRecord,
    release::{self, Manifest, ModuleMigration, ReleaseSigner},
};
use coset::{CborSerializable, CoseSign1};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

fn main() {
    let seed = [31; 32];
    let signer = ReleaseSigner::from_secret(Zeroizing::new(seed)).unwrap();
    let next = ReleaseSigner::from_secret(Zeroizing::new([32; 32])).unwrap();
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
    let manifest = Manifest {
        format: release::DOMAIN.into(),
        issuer: signer.id().to_string(),
        sequence: u64::MAX,
        version: "0.1.0-test".into(),
        target: "debian13-amd64".into(),
        created_at: i64::MIN,
        source_commit: "01".repeat(20),
        core_schema: 3,
        readable_core_schemas: vec![3],
        module_migrations: modules,
        files,
        next_key: Some(next.public()),
    };
    let signed = release::sign(manifest, &signer).unwrap();
    let cose = CoseSign1::from_slice(signed.encoded()).unwrap();
    println!("{}",serde_json::to_string_pretty(&serde_json::json!({
        "warning":"Public fixture seed; not a production release key or installable bundle",
        "domain":release::DOMAIN,"fixture_seed_hex":hex::encode(seed),"public_hex":hex::encode(signer.public()),
        "key_id":signer.id().to_string(),"payload_hex":hex::encode(cose.payload.unwrap()),
        "cose_hex":hex::encode(signed.encoded()),"manifest_sha256":hex::encode(signed.id()),
        "sequence":"18446744073709551615","created_at":"-9223372036854775808"
    })).unwrap());
}
