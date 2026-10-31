// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_client_identity::{RootKey, recovery::Engine as Recovery};
use bbsbox_credentials::{ContactCard, NodeId, linking::Challenge};
use bbsbox_identity::{
    self as identity, Account, Auth, Error, Policy, Role,
    linking::{Linked, Linker, linked_person},
    session::Login,
};
use bbsbox_storage::{Database, clock::Clock, disk::Class, open_reader};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
};
use zeroize::Zeroizing;
const ORIGIN: &str = "https://bbsbox.local:8443";
const PASS: &str = "a local account test passphrase";
#[derive(Default)]
struct TestClock(AtomicI64);
impl Clock for TestClock {
    fn wall_millis(&self) -> i64 {
        -1000
    }
    fn monotonic_millis(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}
struct Fixture {
    dir: tempfile::TempDir,
    path: PathBuf,
    db: Database,
    auth: Auth,
    clock: Arc<TestClock>,
    node: NodeId,
    linker: Linker,
    user: Account,
    login: Login,
    admin: Login,
}
impl Fixture {
    fn new(username: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("node.sqlite");
        let mut db = Database::open(&path, &identity::migrations()).unwrap();
        let clock = Arc::new(TestClock::default());
        let auth = Auth::new(ORIGIN, clock.clone(), Policy::default()).unwrap();
        let password = auth
            .passwords()
            .prepare(Zeroizing::new(PASS.into()))
            .unwrap();
        db.write(Class::Control, 65536, |tx| {
            identity::bootstrap_sysop(tx, "admin", password)
        })
        .unwrap();
        let proof = auth
            .verify_password(
                &open_reader(&path).unwrap(),
                "admin",
                Zeroizing::new(PASS.into()),
                ORIGIN,
            )
            .unwrap();
        let admin = db
            .write(Class::Control, 65536, |tx| auth.complete_login(tx, proof))
            .unwrap();
        let password = auth
            .passwords()
            .prepare(Zeroizing::new(PASS.into()))
            .unwrap();
        let user = db
            .write(Class::Control, 65536, |tx| {
                let actor =
                    auth.authorize_mutation(tx, admin.session_token(), admin.csrf_token(), ORIGIN)?;
                auth.create_account(tx, &actor, username, password, Role::Member)
            })
            .unwrap();
        let proof = auth
            .verify_password(
                &open_reader(&path).unwrap(),
                username,
                Zeroizing::new(PASS.into()),
                ORIGIN,
            )
            .unwrap();
        let login = db
            .write(Class::Control, 65536, |tx| auth.complete_login(tx, proof))
            .unwrap();
        let node = NodeId::from_public_key(&RootKey::generate().public()).unwrap();
        let linker = Linker::new(node, clock.clone());
        Self {
            dir,
            path,
            db,
            auth,
            clock,
            node,
            linker,
            user,
            login,
            admin,
        }
    }
    fn begin(&mut self, card: &ContactCard) -> Result<Challenge, Error> {
        self.db.write(Class::Content, 65536, |tx| {
            let actor = self.auth.authorize_mutation(
                tx,
                self.login.session_token(),
                self.login.csrf_token(),
                ORIGIN,
            )?;
            self.linker.begin(tx, &self.auth, &actor, card)
        })
    }
    fn finish(&mut self, challenge: &Challenge, proof: &[u8]) -> Result<Linked, Error> {
        self.db.write(Class::Content, 65536, |tx| {
            let actor = self.auth.authorize_mutation(
                tx,
                self.login.session_token(),
                self.login.csrf_token(),
                ORIGIN,
            )?;
            self.linker
                .finish(tx, &self.auth, &actor, challenge.nonce(), proof)
        })
    }
    fn unlink(&mut self) -> Result<bool, Error> {
        self.db.write(Class::Control, 65536, |tx| {
            let actor = self.auth.authorize_mutation(
                tx,
                self.login.session_token(),
                self.login.csrf_token(),
                ORIGIN,
            )?;
            self.linker.unlink(tx, &self.auth, &actor)
        })
    }
    fn linked(&self) -> Option<bbsbox_credentials::PersonId> {
        linked_person(&open_reader(&self.path).unwrap(), self.user.id).unwrap()
    }
}
#[test]
fn member_root_proof_links_only_its_current_account_and_retries_the_durable_result() {
    let mut f = Fixture::new("neighbor");
    let root = RootKey::generate();
    let challenge = f.begin(&root.contact_card()).unwrap();
    let proof = root
        .prove_account_link(&challenge, f.node, f.user.id.bytes())
        .unwrap();
    assert!(
        RootKey::generate()
            .prove_account_link(&challenge, f.node, f.user.id.bytes())
            .is_err()
    );
    assert_eq!(f.linked(), None);
    let linked = f.finish(&challenge, &proof).unwrap();
    assert_eq!(linked.person, root.person_id());
    assert!(!linked.replayed);
    assert!(f.finish(&challenge, &proof).unwrap().replayed);
    assert_eq!(f.linked(), Some(root.person_id()));
    assert_eq!(
        identity::account(&open_reader(&f.path).unwrap(), f.user.id)
            .unwrap()
            .role,
        Role::Member
    );
    assert_eq!(
        open_reader(&f.path)
            .unwrap()
            .query_row("SELECT count(*) FROM identity_link_receipts", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    let mut altered = proof.clone();
    *altered.last_mut().unwrap() ^= 1;
    assert!(matches!(
        f.finish(&challenge, &altered),
        Err(Error::InvalidChallenge)
    ));
}
#[test]
fn proofs_cannot_move_between_nodes_accounts_or_challenges_even_for_a_sysop() {
    let mut first = Fixture::new("neighbor");
    let mut second = Fixture::new("neighbor");
    let root = RootKey::generate();
    let a = first.begin(&root.contact_card()).unwrap();
    let b = second.begin(&root.contact_card()).unwrap();
    let proof = root
        .prove_account_link(&a, first.node, first.user.id.bytes())
        .unwrap();
    assert!(matches!(
        root.prove_account_link(&a, second.node, first.user.id.bytes()),
        Err(bbsbox_credentials::Error::WrongDestination)
    ));
    assert!(matches!(
        root.prove_account_link(&a, first.node, second.user.id.bytes()),
        Err(bbsbox_credentials::Error::WrongDestination)
    ));
    assert!(second.finish(&b, &proof).is_err());
    assert_eq!(second.linked(), None);
    assert!(matches!(
        first.db.write(Class::Content, 65536, |tx| {
            let actor = first.auth.authorize_mutation(
                tx,
                first.admin.session_token(),
                first.admin.csrf_token(),
                ORIGIN,
            )?;
            first
                .linker
                .finish(tx, &first.auth, &actor, a.nonce(), &proof)
        }),
        Err(Error::InvalidChallenge)
    ));
    let another = first.begin(&root.contact_card()).unwrap();
    assert!(first.finish(&another, &proof).is_err());
    assert_eq!(first.linked(), None);
    first.finish(&a, &proof).unwrap();
}
#[test]
fn expired_or_restarted_challenges_are_rejected_and_pending_admission_is_bounded() {
    let mut f = Fixture::new("neighbor");
    let root = RootKey::generate();
    let first = f.begin(&root.contact_card()).unwrap();
    let proof = root
        .prove_account_link(&first, f.node, f.user.id.bytes())
        .unwrap();
    for _ in 0..7 {
        f.begin(&root.contact_card()).unwrap();
    }
    assert!(matches!(
        f.begin(&root.contact_card()),
        Err(Error::RateLimited)
    ));
    f.clock.0.store(300000, Ordering::SeqCst);
    assert!(matches!(
        f.finish(&first, &proof),
        Err(Error::InvalidChallenge)
    ));
    let fresh = f.begin(&root.contact_card()).unwrap();
    let proof = root
        .prove_account_link(&fresh, f.node, f.user.id.bytes())
        .unwrap();
    f.linker = Linker::new(f.node, f.clock.clone());
    assert!(matches!(
        f.finish(&fresh, &proof),
        Err(Error::InvalidChallenge)
    ));
    let fresh = f.begin(&root.contact_card()).unwrap();
    let proof = root
        .prove_account_link(&fresh, f.node, f.user.id.bytes())
        .unwrap();
    f.finish(&fresh, &proof).unwrap();
    f.linker = Linker::new(f.node, f.clock.clone());
    assert!(f.finish(&fresh, &proof).unwrap().replayed);
}
#[test]
fn explicit_unlink_keeps_signed_history_and_prevents_old_proof_resurrection() {
    let mut f = Fixture::new("neighbor");
    let original = RootKey::generate();
    let replacement = RootKey::generate();
    let first = f.begin(&original.contact_card()).unwrap();
    let old_proof = original
        .prove_account_link(&first, f.node, f.user.id.bytes())
        .unwrap();
    f.finish(&first, &old_proof).unwrap();
    assert!(matches!(
        f.begin(&replacement.contact_card()),
        Err(Error::AlreadyLinked)
    ));
    assert!(f.unlink().unwrap());
    assert!(matches!(
        f.finish(&first, &old_proof),
        Err(Error::InvalidChallenge)
    ));
    let next = f.begin(&replacement.contact_card()).unwrap();
    let new_proof = replacement
        .prove_account_link(&next, f.node, f.user.id.bytes())
        .unwrap();
    f.finish(&next, &new_proof).unwrap();
    assert!(matches!(
        f.finish(&first, &old_proof),
        Err(Error::InvalidChallenge)
    ));
    assert_eq!(f.linked(), Some(replacement.person_id()));
    assert_eq!(
        open_reader(&f.path)
            .unwrap()
            .query_row("SELECT count(*) FROM identity_link_receipts", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
}
#[test]
fn source_loss_recovery_can_link_a_new_destination_without_merging_username_collisions() {
    let mut source = Fixture::new("neighbor");
    let mut collision = Fixture::new("neighbor");
    let root = RootKey::generate();
    let other = RootKey::generate();
    let a = source.begin(&root.contact_card()).unwrap();
    let proof = root
        .prove_account_link(&a, source.node, source.user.id.bytes())
        .unwrap();
    source.finish(&a, &proof).unwrap();
    let b = collision.begin(&other.contact_card()).unwrap();
    let proof = other
        .prove_account_link(&b, collision.node, collision.user.id.bytes())
        .unwrap();
    collision.finish(&b, &proof).unwrap();
    assert_eq!(source.user.username, collision.user.username);
    assert_ne!(source.linked(), collision.linked());
    assert!(matches!(
        collision.begin(&root.contact_card()),
        Err(Error::AlreadyLinked)
    ));
    let expected = root.person_id();
    let recovery = Recovery::default();
    let media = tempfile::tempdir().unwrap();
    let kit = media.path().join("identity.bbsbox-kit");
    std::fs::write(
        &kit,
        recovery
            .export(
                &root,
                Zeroizing::new("a private recovery test passphrase".into()),
                b"receive-side state",
            )
            .unwrap(),
    )
    .unwrap();
    drop(root);
    let Fixture { dir, path, db, .. } = source;
    drop(db);
    dir.close().unwrap();
    assert!(!path.exists());
    let restored = recovery
        .import(
            &std::fs::read(kit).unwrap(),
            Zeroizing::new("a private recovery test passphrase".into()),
        )
        .unwrap();
    let mut destination = Fixture::new("moved_neighbor");
    let challenge = destination.begin(&restored.contact_card()).unwrap();
    let proof = restored
        .prove_account_link(&challenge, destination.node, destination.user.id.bytes())
        .unwrap();
    destination.finish(&challenge, &proof).unwrap();
    assert_eq!(destination.linked(), Some(expected));
    assert_eq!(collision.linked(), Some(other.person_id()));
}
#[test]
fn link_and_signed_receipt_rollback_together_and_logout_blocks_even_completed_retries() {
    let mut f = Fixture::new("neighbor");
    let root = RootKey::generate();
    let challenge = f.begin(&root.contact_card()).unwrap();
    let proof = root
        .prove_account_link(&challenge, f.node, f.user.id.bytes())
        .unwrap();
    assert!(matches!(
        f.db.write::<(), Error>(Class::Content, 65536, |tx| {
            let actor = f.auth.authorize_mutation(
                tx,
                f.login.session_token(),
                f.login.csrf_token(),
                ORIGIN,
            )?;
            f.linker
                .finish(tx, &f.auth, &actor, challenge.nonce(), &proof)?;
            Err(Error::Forbidden)
        }),
        Err(Error::Forbidden)
    ));
    assert_eq!(f.linked(), None);
    assert_eq!(
        open_reader(&f.path)
            .unwrap()
            .query_row("SELECT count(*) FROM identity_link_receipts", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(!f.finish(&challenge, &proof).unwrap().replayed);
    f.db.write(Class::Control, 65536, |tx| {
        let actor =
            f.auth
                .authorize_mutation(tx, f.login.session_token(), f.login.csrf_token(), ORIGIN)?;
        f.auth.logout(tx, &actor)
    })
    .unwrap();
    assert!(matches!(
        f.finish(&challenge, &proof),
        Err(Error::InvalidSession)
    ));
}

#[test]
fn current_challenges_reject_v1_proofs_but_accepted_legacy_receipts_remain_retryable() {
    use coset::{CborSerializable, CoseSign1Builder, HeaderBuilder, iana};
    use ed25519_dalek::{Signer, SigningKey};
    let mut f = Fixture::new("neighbor");
    let root = SigningKey::from_bytes(&[7; 32]);
    let card = ContactCard::new(root.verifying_key().to_bytes()).unwrap();
    let current = f.begin(&card).unwrap();
    let mut payload: ciborium::value::Value =
        ciborium::de::from_reader(current.encoded().unwrap().as_slice()).unwrap();
    payload.as_array_mut().unwrap()[0] =
        ciborium::value::Value::Text("bbsbox.account-link/v1".into());
    payload.as_array_mut().unwrap()[1] = ciborium::value::Value::Integer(1.into());
    let mut encoded = Vec::new();
    ciborium::ser::into_writer(&payload, &mut encoded).unwrap();
    let legacy = Challenge::from_encoded(&encoded).unwrap();
    let proof = CoseSign1Builder::new()
        .protected(
            HeaderBuilder::new()
                .algorithm(iana::Algorithm::EdDSA)
                .key_id(card.person_id().bytes().to_vec())
                .build(),
        )
        .payload(encoded)
        .create_signature(b"bbsbox.account-link/v1", |message| {
            root.sign(message).to_bytes().to_vec()
        })
        .build()
        .to_vec()
        .unwrap();
    bbsbox_credentials::linking::verify(&legacy, &card, &proof).unwrap();
    assert!(f.finish(&current, &proof).is_err());
    assert_eq!(f.linked(), None);
    // Fixture of valid state committed by the previous profile, with its original immutable proof.
    f.db.write::<_, Error>(Class::Content, 65536, |tx| {
        tx.execute(
            "INSERT INTO identity_account_links VALUES(?1,?2)",
            rusqlite::params![f.user.id.bytes().as_slice(), card.root().as_slice()],
        )?;
        tx.execute(
            "INSERT INTO identity_link_receipts VALUES(?1,?2,?3,?4,?5)",
            rusqlite::params![
                current.nonce().as_slice(),
                f.user.id.bytes().as_slice(),
                f.node.bytes().as_slice(),
                card.root().as_slice(),
                proof
            ],
        )?;
        Ok(())
    })
    .unwrap();
    assert!(f.finish(&current, &proof).unwrap().replayed);
    assert_eq!(f.linked(), Some(card.person_id()));
}
