// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_maintenance::{client_release, release::ReleaseSigner};
use zeroize::Zeroizing;
#[test]
fn client_profile_matches_the_independent_public_vector_exactly() {
    let vector: serde_json::Value = serde_json::from_str(include_str!(
        "../../../protocol/fixtures/client-release-v1.json"
    ))
    .unwrap();
    let public: [u8; 32] = hex::decode(vector["public_hex"].as_str().unwrap())
        .unwrap()
        .try_into()
        .unwrap();
    let encoded = hex::decode(vector["cose_hex"].as_str().unwrap()).unwrap();
    let signed = client_release::verify(&encoded, &public).unwrap();
    assert_eq!(signed.manifest().sequence, i64::MAX as u64);
    assert_eq!(hex::encode(signed.id()), vector["manifest_sha256"]);
    let signer = ReleaseSigner::from_secret(Zeroizing::new([51; 32])).unwrap();
    assert_eq!(signer.public(), public);
    assert_eq!(
        client_release::sign(signed.manifest().clone(), &signer)
            .unwrap()
            .encoded(),
        encoded
    );
}
