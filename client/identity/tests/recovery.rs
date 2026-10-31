// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_client_identity::{
    DeviceKey, RootKey,
    recovery::{Engine, Error, MAX_CLIENT_STATE},
};
use bbsbox_credentials::{Purpose, Purposes};
use zeroize::Zeroizing;
const PASS: &str = "a private offline recovery passphrase";
const OTHER: &str = "another private recovery passphrase";
fn pass(value: &str) -> Zeroizing<String> {
    Zeroizing::new(value.into())
}
#[test]
fn user_held_kit_recovers_root_and_exact_supplied_state_with_randomized_exports() {
    let engine = Engine::default();
    let root = RootKey::generate();
    let state = b"opaque receive-side client state fixture";
    let first = engine.export(&root, pass(PASS), state).unwrap();
    let second = engine.export(&root, pass(PASS), state).unwrap();
    assert_ne!(first, second);
    assert!(!first.windows(state.len()).any(|window| window == state));
    let recovered = engine.import(&first, pass(PASS)).unwrap();
    assert_eq!(recovered.person_id(), root.person_id());
    assert_eq!(recovered.client_state(), state);
    assert_eq!(
        recovered.contact_card().qr_text(),
        root.contact_card().qr_text()
    );
}
#[test]
fn wrong_passphrase_and_tampering_fail_and_leave_the_worker_usable() {
    let engine = Engine::default();
    let root = RootKey::generate();
    let kit = engine.export(&root, pass(PASS), b"state").unwrap();
    assert!(matches!(
        engine.import(&kit, pass(OTHER)),
        Err(Error::DecryptFailed)
    ));
    let mut value: ciborium::value::Value = ciborium::de::from_reader(kit.as_slice()).unwrap();
    let ciphertext = value.as_array_mut().unwrap()[5].as_bytes_mut().unwrap();
    ciphertext[0] ^= 1;
    let mut altered = Vec::new();
    ciborium::ser::into_writer(&value, &mut altered).unwrap();
    assert!(matches!(
        engine.import(&altered, pass(PASS)),
        Err(Error::DecryptFailed)
    ));
    let mut value: ciborium::value::Value = ciborium::de::from_reader(kit.as_slice()).unwrap();
    value.as_array_mut().unwrap()[4].as_bytes_mut().unwrap()[0] ^= 1;
    let mut altered = Vec::new();
    ciborium::ser::into_writer(&value, &mut altered).unwrap();
    assert!(matches!(
        engine.import(&altered, pass(PASS)),
        Err(Error::DecryptFailed)
    ));
    assert_eq!(
        engine.import(&kit, pass(PASS)).unwrap().person_id(),
        root.person_id()
    );
}
#[test]
fn restore_replaces_the_device_and_incarnation_and_retains_public_person_identity() {
    let engine = Engine::default();
    let root = RootKey::generate();
    let device = DeviceKey::generate();
    let purposes = Purposes::new(&[Purpose::Authorship, Purpose::IdentityProof]).unwrap();
    let old = root.issue_device(device.public(), purposes).unwrap();
    let _active = device.activate(root.person_id(), old.clone()).unwrap();
    let kit = engine
        .export(&root, pass(PASS), b"old state is receive-side only")
        .unwrap();
    let restored = engine.import(&kit, pass(PASS)).unwrap();
    assert_ne!(restored.control_incarnation(), root.control_incarnation());
    assert!(
        restored
            .sign_current_control(root.control_incarnation(), b"old root stream")
            .is_err()
    );
    let fresh = restored.new_device(purposes).unwrap();
    assert_eq!(fresh.credential().body().person_id(), root.person_id());
    assert_ne!(
        fresh.credential().body().device_id(),
        old.body().device_id()
    );
    assert_ne!(
        fresh.credential().body().incarnation(),
        old.body().incarnation()
    );
    assert_ne!(fresh.credential().id(), old.id());
    assert_ne!(restored.person_id(), RootKey::generate().person_id());
}
#[test]
fn malformed_excessive_and_unknown_profiles_fail_before_unbounded_work() {
    let engine = Engine::default();
    let root = RootKey::generate();
    assert!(matches!(
        engine.export(&root, pass("short"), b"state"),
        Err(Error::WeakPassphrase)
    ));
    assert!(matches!(
        engine.export(&root, pass(PASS), &vec![0; MAX_CLIENT_STATE + 1]),
        Err(Error::InvalidKit)
    ));
    assert!(matches!(
        engine.import(&vec![0; MAX_CLIENT_STATE + 257], pass(PASS)),
        Err(Error::InvalidKit)
    ));
    let kit = engine.export(&root, pass(PASS), b"").unwrap();
    let mut value: ciborium::value::Value = ciborium::de::from_reader(kit.as_slice()).unwrap();
    value.as_array_mut().unwrap()[2] = ciborium::value::Value::Integer(999999.into());
    let mut altered = Vec::new();
    ciborium::ser::into_writer(&value, &mut altered).unwrap();
    assert!(matches!(
        engine.import(&altered, pass(PASS)),
        Err(Error::UnsupportedProfile)
    ));
    let mut appended = kit.clone();
    appended.push(0);
    assert!(matches!(
        engine.import(&appended, pass(PASS)),
        Err(Error::InvalidKit)
    ));
    assert!(engine.import(&kit[..kit.len() - 1], pass(PASS)).is_err());
    let mut nested = vec![0x81; 1000];
    nested.push(0);
    assert!(matches!(
        engine.import(&nested, pass(PASS)),
        Err(Error::InvalidKit)
    ));
}
#[test]
fn changing_kit_passphrase_preserves_root_and_supplied_receive_state() {
    let engine = Engine::default();
    let root = RootKey::generate();
    let kit = engine.export(&root, pass(PASS), b"private state").unwrap();
    let restored = engine.import(&kit, pass(PASS)).unwrap();
    let changed = restored.reexport(&engine, pass(OTHER)).unwrap();
    assert!(matches!(
        engine.import(&changed, pass(PASS)),
        Err(Error::DecryptFailed)
    ));
    let again = engine.import(&changed, pass(OTHER)).unwrap();
    assert_eq!(again.person_id(), root.person_id());
    assert_eq!(again.client_state(), b"private state");
}
