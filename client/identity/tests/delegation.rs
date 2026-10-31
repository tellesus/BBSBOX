// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_client_identity::{DeviceKey, RootKey};
use bbsbox_credentials::{Credential, Error, Purpose, Purposes};
use ed25519_dalek::{Signature, VerifyingKey};
#[test]
fn root_authorizes_distinct_devices_and_new_incarnations() {
    let root = RootKey::generate();
    let first = DeviceKey::generate();
    let second = DeviceKey::generate();
    let purposes = Purposes::new(&[Purpose::Authorship]).unwrap();
    let a = root.issue_device(first.public(), purposes).unwrap();
    let b = root.issue_device(second.public(), purposes).unwrap();
    let replacement = root.issue_device(first.public(), purposes).unwrap();
    assert_eq!(a.body().person_id(), b.body().person_id());
    assert_ne!(a.body().device_id(), b.body().device_id());
    assert_ne!(a.body().incarnation(), replacement.body().incarnation());
    assert_ne!(a.id(), replacement.id());
    assert_ne!(root.person_id(), RootKey::generate().person_id());
    assert_eq!(root.contact_card().person_id(), root.person_id());
}
#[test]
fn activation_requires_expected_root_and_matching_device_secret() {
    let root = RootKey::generate();
    let other = RootKey::generate();
    let device = DeviceKey::generate();
    let credential = root
        .issue_device(
            device.public(),
            Purposes::new(&[Purpose::IdentityProof]).unwrap(),
        )
        .unwrap();
    assert!(matches!(
        DeviceKey::generate().activate(root.person_id(), credential.clone()),
        Err(Error::InvalidKey)
    ));
    assert!(matches!(
        device.activate(other.person_id(), credential),
        Err(Error::WrongPerson)
    ));
}
#[test]
fn activated_device_signs_only_delegated_purposes() {
    let root = RootKey::generate();
    let device = DeviceKey::generate();
    let credential = root
        .issue_device(
            device.public(),
            Purposes::new(&[Purpose::Authorship]).unwrap(),
        )
        .unwrap();
    let active = device
        .activate(
            root.person_id(),
            Credential::verify(credential.encoded()).unwrap(),
        )
        .unwrap();
    assert!(matches!(
        active.sign_for(Purpose::ForumGovernance, b"not delegated"),
        Err(Error::PurposeDenied)
    ));
    let message = b"bbsbox.test-message/v1: resource-bound fixture";
    let signature = Signature::try_from(
        active
            .sign_for(Purpose::Authorship, message)
            .unwrap()
            .as_slice(),
    )
    .unwrap();
    VerifyingKey::from_bytes(active.credential().body().device())
        .unwrap()
        .verify_strict(message, &signature)
        .unwrap();
}
