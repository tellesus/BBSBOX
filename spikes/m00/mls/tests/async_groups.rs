// SPDX-License-Identifier: AGPL-3.0-or-later
//! Real, separate OpenMLS clients; byte-only delivery is manually delayed/reordered.
//! All credentials are disposable test identities. No BBSBOX admission protocol is implied.
use aes_gcm::{
    Aes256Gcm, KeyInit,
    aead::{Aead, AeadCore, Payload},
};
use openmls::prelude::{
    tls_codec::{Deserialize as TlsDeserialize, Serialize as TlsSerialize},
    *,
};
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::OpenMlsRustCrypto;
use openmls_traits::OpenMlsProvider;
use rand_core::OsRng;
use serde::{Deserialize, Serialize};

const SUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;
const PAST_EPOCHS: usize = 32;
const BACKUP_CONTEXT: &[u8] = b"bbsbox.m00.client-snapshot/v0";

struct Client {
    label: String,
    provider: OpenMlsRustCrypto,
    signer: SignatureKeyPair,
    group: Option<MlsGroup>,
    // Lab-level recovery guard. A production client must enforce this at every mutation boundary.
    can_send: bool,
}
impl Client {
    fn new(label: &str) -> Self {
        let provider = OpenMlsRustCrypto::default();
        let signer = SignatureKeyPair::new(SUITE.signature_algorithm()).unwrap();
        signer.store(provider.storage()).unwrap();
        Self {
            label: label.into(),
            provider,
            signer,
            group: None,
            can_send: true,
        }
    }
    fn credential(&self) -> CredentialWithKey {
        CredentialWithKey {
            credential: BasicCredential::new(self.label.as_bytes().to_vec()).into(),
            signature_key: self.signer.public().into(),
        }
    }
    fn key_package(&self) -> KeyPackageBundle {
        KeyPackage::builder()
            .build(SUITE, &self.provider, &self.signer, self.credential())
            .unwrap()
    }
    fn create_group(&mut self) {
        let config = MlsGroupCreateConfig::builder()
            .ciphersuite(SUITE)
            .max_past_epochs(PAST_EPOCHS)
            .use_ratchet_tree_extension(true)
            .build();
        self.group = Some(
            MlsGroup::new_with_group_id(
                &self.provider,
                &self.signer,
                &config,
                GroupId::from_slice(b"bbsbox-m00-test-group"),
                self.credential(),
            )
            .unwrap(),
        );
    }
    fn join(&mut self, welcome: &[u8]) {
        let input = MlsMessageIn::tls_deserialize_exact(welcome).unwrap();
        let MlsMessageBodyIn::Welcome(welcome) = input.extract() else {
            panic!("expected Welcome")
        };
        let config = MlsGroupJoinConfig::builder()
            .max_past_epochs(PAST_EPOCHS)
            .use_ratchet_tree_extension(true)
            .build();
        self.group = Some(
            StagedWelcome::new_from_welcome(&self.provider, &config, welcome, None)
                .unwrap()
                .into_group(&self.provider)
                .unwrap(),
        );
    }
    fn invite(&mut self, packages: &[KeyPackage]) -> (Vec<u8>, Vec<u8>) {
        assert!(self.can_send, "restored clients cannot mutate group state");
        let group = self.group.as_mut().unwrap();
        let (commit, welcome, _) = group
            .add_members(&self.provider, &self.signer, packages)
            .unwrap();
        group.merge_pending_commit(&self.provider).unwrap();
        (
            commit.tls_serialize_detached().unwrap(),
            welcome.tls_serialize_detached().unwrap(),
        )
    }
    fn send(&mut self, text: &[u8]) -> Result<Vec<u8>, String> {
        if !self.can_send {
            return Err(
                "restored client is receive-only until fresh owner-authorized rejoin".into(),
            );
        }
        self.group
            .as_mut()
            .unwrap()
            .create_message(&self.provider, &self.signer, text)
            .map_err(|e| format!("{e:?}"))?
            .tls_serialize_detached()
            .map_err(|e| format!("{e:?}"))
    }
    fn update(&mut self) -> Vec<u8> {
        assert!(
            self.can_send,
            "restored clients cannot create governance changes"
        );
        let group = self.group.as_mut().unwrap();
        let bundle = group
            .self_update(&self.provider, &self.signer, LeafNodeParameters::default())
            .unwrap();
        let bytes = bundle.commit().tls_serialize_detached().unwrap();
        group.merge_pending_commit(&self.provider).unwrap();
        bytes
    }
    fn receive(&mut self, bytes: &[u8]) -> Result<Option<Vec<u8>>, String> {
        let message = MlsMessageIn::tls_deserialize_exact(bytes)
            .map_err(|e| format!("{e:?}"))?
            .try_into_protocol_message()
            .map_err(|e| format!("{e:?}"))?;
        let group = self.group.as_mut().ok_or("no group state")?;
        let processed = group
            .process_message(&self.provider, message)
            .map_err(|e| format!("{e:?}"))?;
        match processed.into_content() {
            ProcessedMessageContent::ApplicationMessage(message) => Ok(Some(message.into_bytes())),
            ProcessedMessageContent::StagedCommitMessage(commit) => {
                group
                    .merge_staged_commit(&self.provider, *commit)
                    .map_err(|e| format!("{e:?}"))?;
                Ok(None)
            }
            _ => Err("unexpected proposal in this fixture".into()),
        }
    }
    fn epoch(&self) -> u64 {
        self.group.as_ref().unwrap().epoch().as_u64()
    }
}
fn three() -> (Client, Client, Client) {
    let mut alice = Client::new("Alice-owner");
    let mut bob = Client::new("Bob");
    let mut carol = Client::new("Carol");
    alice.create_group();
    let (_, welcome) = alice.invite(&[
        bob.key_package().key_package().clone(),
        carol.key_package().key_package().clone(),
    ]);
    bob.join(&welcome);
    carol.join(&welcome);
    assert_eq!(alice.epoch(), bob.epoch());
    assert_eq!(bob.epoch(), carol.epoch());
    (alice, bob, carol)
}

#[test]
fn disconnected_writers_reconcile_reordered_ciphertexts_and_reject_replays() {
    let (mut alice, mut bob, mut carol) = three();
    // Nobody receives anything before all three have written locally.
    let a = alice.send(b"owner disconnected").unwrap();
    let b0 = bob.send(b"first from Bob").unwrap();
    let b1 = bob.send(b"second from Bob").unwrap();
    let c = carol.send(b"Carol disconnected").unwrap();
    assert!(
        !b0.windows(b"first from Bob".len())
            .any(|w| w == b"first from Bob")
    );
    assert_eq!(alice.receive(&b1).unwrap().unwrap(), b"second from Bob");
    assert_eq!(alice.receive(&b0).unwrap().unwrap(), b"first from Bob");
    assert!(alice.receive(&b0).is_err());
    assert_eq!(bob.receive(&c).unwrap().unwrap(), b"Carol disconnected");
    assert_eq!(carol.receive(&a).unwrap().unwrap(), b"owner disconnected");
    let mut corrupt = c.clone();
    let last = corrupt.len() - 1;
    corrupt[last] ^= 1;
    assert!(alice.receive(&corrupt).is_err());
    assert_eq!(alice.receive(&c).unwrap().unwrap(), b"Carol disconnected");
    assert!(Client::new("host-only").receive(&a).is_err());
}

#[test]
fn stale_writer_keeps_posting_and_catches_up_before_reading_new_epoch() {
    let (mut alice, mut bob, mut carol) = three();
    let commit = alice.update();
    carol.receive(&commit).unwrap();
    let future = carol.send(b"new epoch").unwrap();
    let late = bob.send(b"still writing while disconnected").unwrap();
    assert_eq!(
        alice.receive(&late).unwrap().unwrap(),
        b"still writing while disconnected"
    );
    assert!(bob.receive(&future).is_err()); // A delivery layer must retain, not discard, these bytes.
    bob.receive(&commit).unwrap();
    assert_eq!(bob.receive(&future).unwrap().unwrap(), b"new epoch");
    assert_eq!(alice.epoch(), bob.epoch());
}

#[test]
fn removal_preserves_stale_view_risk_but_excludes_future_epochs_and_rejoin_has_no_history() {
    let (mut alice, mut bob, mut carol) = three();
    let before = carol.send(b"delayed before removal").unwrap();
    let index = bob.group.as_ref().unwrap().own_leaf_index();
    let (commit, _, _) = alice
        .group
        .as_mut()
        .unwrap()
        .remove_members(&alice.provider, &alice.signer, &[index])
        .unwrap();
    alice
        .group
        .as_mut()
        .unwrap()
        .merge_pending_commit(&alice.provider)
        .unwrap();
    let commit = commit.tls_serialize_detached().unwrap();
    carol.receive(&commit).unwrap();
    assert_eq!(
        bob.receive(&before).unwrap().unwrap(),
        b"delayed before removal"
    );
    let stale = bob.send(b"stale member can still write locally").unwrap();
    // Raw MLS can decrypt retained epochs. BBSBOX admission still needs causal membership policy.
    assert_eq!(
        alice.receive(&stale).unwrap().unwrap(),
        b"stale member can still write locally"
    );
    let after = carol.send(b"after removal").unwrap();
    assert!(bob.receive(&after).is_err());
    bob.receive(&commit).unwrap();
    assert!(!bob.group.as_ref().unwrap().is_active());
    assert!(bob.send(b"after learning removal").is_err());
    let mut fresh = Client::new("Bob-new-device");
    let (new_commit, welcome) = alice.invite(&[fresh.key_package().key_package().clone()]);
    carol.receive(&new_commit).unwrap();
    fresh.join(&welcome);
    assert!(fresh.receive(&after).is_err());
    let now = carol.send(b"welcome back").unwrap();
    assert_eq!(fresh.receive(&now).unwrap().unwrap(), b"welcome back");
}

#[test]
fn retained_epoch_boundary_is_32_commits_not_a_number_of_days() {
    let (mut alice, mut bob, mut carol) = three();
    let within = bob.send(b"retained").unwrap();
    let beyond = bob.send(b"eventually unavailable").unwrap();
    for _ in 0..PAST_EPOCHS {
        let commit = alice.update();
        bob.receive(&commit).unwrap();
        carol.receive(&commit).unwrap();
    }
    assert_eq!(alice.receive(&within).unwrap().unwrap(), b"retained");
    let commit = alice.update();
    bob.receive(&commit).unwrap();
    carol.receive(&commit).unwrap();
    assert!(alice.receive(&beyond).is_err());
    assert!(carol.send(b"unrelated current work continues").is_ok());
}

#[test]
fn competing_commits_fork_and_raw_mls_does_not_enforce_a_forum_owner() {
    let (mut alice, mut bob, mut carol) = three();
    let owner_commit = alice.update();
    let other_commit = carol.update(); // MLS permits this; BBSBOX's outer owner-authority gate must decide.
    assert_eq!(alice.epoch(), carol.epoch());
    assert!(alice.receive(&other_commit).is_err());
    assert!(carol.receive(&owner_commit).is_err());
    bob.receive(&owner_commit).unwrap();
    let owner_message = alice.send(b"owner branch").unwrap();
    assert_eq!(
        bob.receive(&owner_message).unwrap().unwrap(),
        b"owner branch"
    );
    assert!(carol.receive(&owner_message).is_err());
    assert!(carol.send(b"other branch continues locally").is_ok());
    let (mut alice, _, mut carol) = three();
    let nonowner_commit = carol.update();
    assert!(alice.receive(&nonowner_commit).is_ok()); // Establish the missing app-level rule explicitly.
}

#[derive(Serialize, Deserialize)]
struct ClientSnapshot {
    label: String,
    public_key: Vec<u8>,
    group_id: Vec<u8>,
    values: Vec<(Vec<u8>, Vec<u8>)>,
}
#[derive(Serialize, Deserialize)]
struct SealedSnapshot {
    nonce: [u8; 12],
    ciphertext: Vec<u8>,
}
fn snapshot(client: &Client, key: &[u8; 32]) -> Vec<u8> {
    let payload = ClientSnapshot {
        label: client.label.clone(),
        public_key: client.signer.public().to_vec(),
        group_id: client
            .group
            .as_ref()
            .unwrap()
            .group_id()
            .as_slice()
            .to_vec(),
        values: client
            .provider
            .storage()
            .values
            .read()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    };
    let cipher = Aes256Gcm::new_from_slice(key).unwrap();
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: &serde_json::to_vec(&payload).unwrap(),
                aad: BACKUP_CONTEXT,
            },
        )
        .unwrap();
    serde_json::to_vec(&SealedSnapshot {
        nonce: nonce.into(),
        ciphertext,
    })
    .unwrap()
}
fn restore(bytes: &[u8], key: &[u8; 32], expected_label: &str) -> Result<Client, String> {
    let sealed: SealedSnapshot = serde_json::from_slice(bytes).map_err(|_| "invalid snapshot")?;
    let plain = Aes256Gcm::new_from_slice(key)
        .unwrap()
        .decrypt(
            &sealed.nonce.into(),
            Payload {
                msg: &sealed.ciphertext,
                aad: BACKUP_CONTEXT,
            },
        )
        .map_err(|_| "snapshot authentication failed")?;
    let snapshot: ClientSnapshot =
        serde_json::from_slice(&plain).map_err(|_| "invalid client state")?;
    if snapshot.label != expected_label {
        return Err("snapshot belongs to a different fixture identity".into());
    }
    let provider = OpenMlsRustCrypto::default();
    provider
        .storage()
        .values
        .write()
        .unwrap()
        .extend(snapshot.values);
    let signer = SignatureKeyPair::read(
        provider.storage(),
        &snapshot.public_key,
        SUITE.signature_algorithm(),
    )
    .ok_or("missing signature key")?;
    let group = MlsGroup::load(provider.storage(), &GroupId::from_slice(&snapshot.group_id))
        .map_err(|_| "group load failed")?
        .ok_or("missing group")?;
    Ok(Client {
        label: snapshot.label,
        provider,
        signer,
        group: Some(group),
        can_send: false,
    })
}

#[test]
fn encrypted_client_snapshot_restores_a_fresh_provider_and_rejects_tampering() {
    let (mut alice, bob, mut carol) = three();
    let key = [41u8; 32]; // Fixed test-only recovery key. Never a shipped default.
    let bytes = snapshot(&bob, &key);
    assert!(!bytes.windows(b"Bob".len()).any(|w| w == b"Bob"));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("client-recovery.json");
    std::fs::write(&path, &bytes).unwrap();
    drop(bob);
    let bytes = std::fs::read(path).unwrap();
    let mut bob = restore(&bytes, &key, "Bob").unwrap();
    assert!(bob.send(b"must not resume old sender state").is_err());
    let message = carol.send(b"after fresh provider restore").unwrap();
    assert_eq!(
        bob.receive(&message).unwrap().unwrap(),
        b"after fresh provider restore"
    );
    assert!(restore(&bytes, &[42u8; 32], "Bob").is_err());
    assert!(restore(&bytes, &key, "Alice-owner").is_err());
    let mut changed: SealedSnapshot = serde_json::from_slice(&bytes).unwrap();
    changed.ciphertext[0] ^= 1;
    assert!(restore(&serde_json::to_vec(&changed).unwrap(), &key, "Bob").is_err());
    let commit = alice.update();
    bob.receive(&commit).unwrap();
    assert!(
        bob.send(b"a replayed commit alone does not prove fresh sender state")
            .is_err()
    );
}

#[test]
fn restored_client_stays_receive_only_until_fresh_owner_authorized_join() {
    let (mut alice, bob, mut carol) = three();
    let key = [44u8; 32];
    let old_index = bob.group.as_ref().unwrap().own_leaf_index();
    let bytes = snapshot(&bob, &key);
    drop(bob);
    let mut restored = restore(&bytes, &key, "Bob").unwrap();
    assert!(restored.send(b"blocked restored sender").is_err());
    let (remove, _, _) = alice
        .group
        .as_mut()
        .unwrap()
        .remove_members(&alice.provider, &alice.signer, &[old_index])
        .unwrap();
    alice
        .group
        .as_mut()
        .unwrap()
        .merge_pending_commit(&alice.provider)
        .unwrap();
    carol
        .receive(&remove.tls_serialize_detached().unwrap())
        .unwrap();
    // Other members keep posting while recovery is pending.
    assert!(carol.send(b"the group does not stop").is_ok());
    let mut fresh = Client::new("Bob-new-device");
    let (commit, welcome) = alice.invite(&[fresh.key_package().key_package().clone()]);
    carol.receive(&commit).unwrap();
    fresh.join(&welcome);
    let message = fresh.send(b"freshly joined sender").unwrap();
    assert_eq!(
        alice.receive(&message).unwrap().unwrap(),
        b"freshly joined sender"
    );
}

#[test]
fn restoring_an_older_snapshot_loses_post_snapshot_replay_knowledge() {
    let (_, mut bob, mut carol) = three();
    let key = [43u8; 32];
    let old = snapshot(&bob, &key);
    let message = carol.send(b"received after backup").unwrap();
    assert_eq!(
        bob.receive(&message).unwrap().unwrap(),
        b"received after backup"
    );
    assert!(bob.receive(&message).is_err());
    drop(bob);
    let mut restored = restore(&old, &key, "Bob").unwrap();
    assert_eq!(
        restored.receive(&message).unwrap().unwrap(),
        b"received after backup"
    );
    // This is an observed recovery limit, not an anti-replay claim for backup rollback.
}
