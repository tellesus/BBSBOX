// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_credentials::*;
use coset::{CborSerializable, CoseSign1, HeaderBuilder, iana};
use ed25519_dalek::{Signer, SigningKey};
fn vector() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/device-v1.json")).unwrap()
}
fn envelope() -> Vec<u8> {
    hex::decode(vector()["cose_hex"].as_str().unwrap()).unwrap()
}
fn body() -> DeviceBody {
    DeviceBody::new(
        SigningKey::from_bytes(&[7; 32]).verifying_key().to_bytes(),
        SigningKey::from_bytes(&[9; 32]).verifying_key().to_bytes(),
        [11; 16],
        Purposes::new(&[Purpose::Authorship, Purpose::IdentityProof]).unwrap(),
    )
    .unwrap()
}
#[test]
fn public_vector_reproduces_exact_ids_bytes_and_signature() {
    let root = SigningKey::from_bytes(&[7; 32]);
    let expected = vector();
    let credential = issue(body(), |message| root.sign(message).to_bytes()).unwrap();
    assert_eq!(
        hex::encode(credential.encoded()),
        expected["cose_hex"].as_str().unwrap()
    );
    assert_eq!(credential.id().to_string(), expected["credential_id"]);
    assert_eq!(
        credential.body().person_id().to_string(),
        expected["person_id"]
    );
    assert_eq!(
        credential.body().device_id().to_string(),
        expected["device_id"]
    );
    assert_eq!(
        Credential::verify(credential.encoded()).unwrap().body(),
        credential.body()
    );
}
#[test]
fn role_ids_and_public_cards_never_substitute_for_each_other() {
    let public = body().root().to_owned();
    let person = PersonId::from_public_key(&public).unwrap();
    for id in [
        NodeId::from_public_key(&public).unwrap().bytes(),
        ForumId::from_public_key(&public).unwrap().bytes(),
        DeviceId::from_public_key(&public).unwrap().bytes(),
        CourierId::from_public_key(&public).unwrap().bytes(),
    ] {
        assert_ne!(person.bytes(), id);
    }
    assert_eq!(person.to_string().parse::<PersonId>().unwrap(), person);
    assert!(
        format!("node:{}", hex::encode(person.bytes()))
            .parse::<PersonId>()
            .is_err()
    );
    let card = ContactCard::new(public).unwrap();
    assert_eq!(
        ContactCard::from_qr_text(&card.qr_text())
            .unwrap()
            .person_id(),
        person
    );
    card.verify_fingerprint(person).unwrap();
    let other = PersonId::from_public_key(body().device()).unwrap();
    assert_eq!(card.verify_fingerprint(other), Err(Error::WrongPerson));
    assert!(!card.qr_text().contains(&hex::encode([7; 32])));
    assert!(ContactCard::from_qr_text("bbsbox-person-v1:../private").is_err());
}
#[test]
fn wrong_issuer_signature_tampering_and_excessive_inputs_are_rejected() {
    let other = SigningKey::from_bytes(&[13; 32]);
    assert!(matches!(
        issue(body(), |message| other.sign(message).to_bytes()),
        Err(Error::InvalidSignature)
    ));
    let mut cose = CoseSign1::from_slice(&envelope()).unwrap();
    cose.signature[0] ^= 1;
    assert!(matches!(
        Credential::verify(&cose.to_vec().unwrap()),
        Err(Error::InvalidSignature)
    ));
    assert!(Credential::verify(&vec![0; MAX_CREDENTIAL_BYTES + 1]).is_err());
    let mut appended = envelope();
    appended.push(0);
    assert!(Credential::verify(&appended).is_err());
}
#[test]
fn canonical_profile_rejects_unknown_headers_unprotected_data_and_nonminimal_payloads() {
    let mut cose = CoseSign1::from_slice(&envelope()).unwrap();
    cose.unprotected = HeaderBuilder::new().key_id(vec![1]).build();
    assert!(matches!(
        Credential::verify(&cose.to_vec().unwrap()),
        Err(Error::NonCanonical)
    ));
    let mut cose = CoseSign1::from_slice(&envelope()).unwrap();
    cose.protected.header.alg = Some(coset::Algorithm::Assigned(iana::Algorithm::ES256));
    cose.protected.original_data = None;
    assert!(matches!(
        Credential::verify(&cose.to_vec().unwrap()),
        Err(Error::NonCanonical)
    ));
    let mut cose = CoseSign1::from_slice(&envelope()).unwrap();
    let payload = cose.payload.as_mut().unwrap(); // Encode protocol integer 1 as nonminimal 0x18 0x01.
    let domain_size = DEVICE_DOMAIN.len();
    let version_offset = 1 + 2 + domain_size;
    assert_eq!(payload[version_offset], 1);
    payload.splice(version_offset..=version_offset, [0x18, 1]);
    assert!(matches!(
        Credential::verify(&cose.to_vec().unwrap()),
        Err(Error::NonCanonical)
    ));
}
#[test]
fn bounded_purposes_and_subject_checks_grant_no_resource_membership() {
    assert_eq!(Purposes::from_bits(0), Err(Error::UnsupportedPurpose));
    assert_eq!(Purposes::from_bits(16), Err(Error::UnsupportedPurpose));
    let credential = Credential::verify(&envelope()).unwrap();
    let person = credential.body().person_id();
    credential
        .check_subject_purpose(person, Purpose::Authorship)
        .unwrap();
    assert_eq!(
        credential.check_subject_purpose(person, Purpose::ForumGovernance),
        Err(Error::PurposeDenied)
    );
    assert_eq!(
        credential.check_subject_purpose(
            PersonId::from_public_key(credential.body().device()).unwrap(),
            Purpose::Authorship
        ),
        Err(Error::WrongPerson)
    );
}
#[test]
fn weak_reused_keys_unknown_versions_and_zero_incarnations_cannot_enter_the_profile() {
    assert!(ContactCard::new([0; 32]).is_err());
    let b = body();
    assert!(matches!(
        DeviceBody::new(*b.root(), *b.root(), [1; 16], b.purposes()),
        Err(Error::SameRootAndDevice)
    ));
    assert!(DeviceBody::new(*b.root(), *b.device(), [0; 16], b.purposes()).is_err());
    let mut cose = CoseSign1::from_slice(&envelope()).unwrap();
    let mut value: ciborium::value::Value =
        ciborium::de::from_reader(cose.payload.as_ref().unwrap().as_slice()).unwrap();
    value.as_array_mut().unwrap()[1] = ciborium::value::Value::Integer(2.into());
    let mut payload = Vec::new();
    ciborium::ser::into_writer(&value, &mut payload).unwrap();
    cose.payload = Some(payload);
    assert!(matches!(
        Credential::verify(&cose.to_vec().unwrap()),
        Err(Error::UnsupportedVersion)
    ));
}
