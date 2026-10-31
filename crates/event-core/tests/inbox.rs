// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_client_identity::{ActiveDevice, DeviceKey, RootKey};
use bbsbox_credentials::{CredentialId, Purpose, Purposes};
use bbsbox_event_codec::{
    self as codec, Actor, CompleteEvent, Distribution, Draft, EventId, Scope,
};
use bbsbox_event_core::{self as core, Delivery, Inbox, Readiness, Resolver};
use bbsbox_storage::{Database, disk::Class, open_reader};
use ciborium::value::Value;
use coset::{CborSerializable, CoseSign1, CoseSign1Builder};
use rusqlite::Connection;

struct Fixture {
    _temp: tempfile::TempDir,
    path: std::path::PathBuf,
    db: Database,
    device: ActiveDevice,
}
fn resolve(
    connection: &Connection,
    id: CredentialId,
) -> codec::Result<Option<bbsbox_credentials::Credential>> {
    bbsbox_identity::public::resolve_credential(connection, id)
        .map_err(|_| codec::Error::ResolverUnavailable)
}
fn missing(
    _: &Connection,
    _: CredentialId,
) -> codec::Result<Option<bbsbox_credentials::Credential>> {
    Ok(None)
}
fn device() -> ActiveDevice {
    let root = RootKey::generate();
    let key = DeviceKey::generate();
    let credential = root
        .issue_device(key.public(), Purposes::new(&[Purpose::Authorship]).unwrap())
        .unwrap();
    key.activate(root.person_id(), credential).unwrap()
}
fn migrations() -> Vec<bbsbox_storage::Migration> {
    let mut all = bbsbox_identity::migrations();
    all.extend(core::migrations());
    all
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("node.sqlite");
        Self {
            db: Database::open(&path, &migrations()).unwrap(),
            _temp: temp,
            path,
            device: device(),
        }
    }
    fn credential(&mut self) {
        self.db
            .write::<_, core::Error>(Class::Content, 4096, |tx| {
                bbsbox_identity::public::store_credential(tx, self.device.credential())
                    .map_err(|_| core::Error::CorruptRecord)?;
                Ok(())
            })
            .unwrap();
    }
    fn receive(&mut self, event: &CompleteEvent, body: bool, source: &str) -> core::Intake {
        self.receive_encoded(
            event.commitment().encoded(),
            body.then_some(event.body()),
            source,
            &resolve,
        )
        .unwrap()
    }
    fn receive_encoded(
        &mut self,
        envelope: &[u8],
        body: Option<&[u8]>,
        source: &str,
        resolver: &dyn Resolver,
    ) -> core::Result<core::Intake> {
        self.db.write(Class::Content, 128 * 1024, |tx| {
            Inbox::default().receive(tx, envelope, body, &delivery(source), resolver)
        })
    }
    fn state(&self, id: EventId) -> Readiness {
        core::readiness(&open_reader(&self.path).unwrap(), id)
            .unwrap()
            .unwrap()
    }
    fn drain(&mut self) {
        for _ in 0..10 {
            let mut cursor = None;
            let mut changed = 0;
            loop {
                let batch = self
                    .db
                    .write::<_, core::Error>(Class::Content, 4096, |tx| {
                        Inbox::default().reconcile(tx, cursor, 2, &resolve)
                    })
                    .unwrap();
                changed += batch.changed;
                cursor = batch.next;
                if cursor.is_none() {
                    break;
                }
            }
            if changed == 0 {
                return;
            }
        }
        panic!("fixture did not settle");
    }
}
fn delivery(source: &str) -> Delivery {
    Delivery {
        source: source.into(),
        transfer: "fixture-delivery".into(),
    }
}
fn event(
    device: &ActiveDevice,
    resource: &str,
    sequence: u64,
    previous: Option<EventId>,
    body: &[u8],
) -> CompleteEvent {
    codec::sign(
        Draft {
            kind: "fixture.post".into(),
            schema: 1,
            scope: Scope {
                resource: resource.into(),
                audience: "public".into(),
                distribution: Distribution::Shared,
            },
            sequence,
            previous,
            dependencies: vec![],
            claimed_time: 99,
            extensions: vec![],
        },
        Actor::device(device.credential()),
        body,
        Some(device.credential()),
        |bytes| {
            device
                .sign_for(Purpose::Authorship, bytes)
                .map_err(|_| codec::Error::SignerUnavailable)
        },
    )
    .unwrap()
}
fn rewrite(
    event: &CompleteEvent,
    device: &ActiveDevice,
    change: impl FnOnce(&mut Vec<Value>),
) -> Vec<u8> {
    let cose = CoseSign1::from_slice(event.commitment().encoded()).unwrap();
    let mut payload: Value = ciborium::de::from_reader(cose.payload.unwrap().as_slice()).unwrap();
    change(payload.as_array_mut().unwrap());
    let mut bytes = vec![];
    ciborium::ser::into_writer(&payload, &mut bytes).unwrap();
    CoseSign1Builder::new()
        .protected(cose.protected.header)
        .payload(bytes)
        .create_signature(codec::DOMAIN.as_bytes(), |bytes| {
            device
                .sign_for(Purpose::Authorship, bytes)
                .unwrap()
                .to_vec()
        })
        .build()
        .to_vec()
        .unwrap()
}

#[test]
fn durable_duplicates_keep_the_original_verified_body_delivery_after_restart() {
    let mut f = Fixture::new();
    f.credential();
    let e = event(&f.device, "board:a", 1, None, b"hello");
    let first = f.receive(&e, true, "pigeon:alice");
    assert_eq!(first.readiness, Readiness::Ready);
    assert!(!first.duplicate);
    let db = Database::open(&f.path, &migrations()).unwrap();
    drop(std::mem::replace(&mut f.db, db));
    let again = f.receive(&e, true, "pigeon:bob");
    assert!(again.duplicate);
    assert_eq!(
        core::first_body_delivery(&open_reader(&f.path).unwrap(), e.id()).unwrap(),
        Some(delivery("pigeon:alice"))
    );
    assert_eq!(
        Inbox::retained_bytes(&open_reader(&f.path).unwrap()).unwrap(),
        (e.commitment().encoded().len() + e.body().len()) as u64
    );
}
#[test]
fn hash_only_arrival_and_bad_body_never_take_the_first_full_body_slot() {
    let mut f = Fixture::new();
    f.credential();
    let e = event(&f.device, "board:a", 1, None, b"actual");
    assert_eq!(
        f.receive(&e, false, "pigeon:header").readiness,
        Readiness::CommitmentOnly
    );
    assert!(
        f.receive_encoded(
            e.commitment().encoded(),
            Some(b"wrong!"),
            "pigeon:bad",
            &resolve
        )
        .is_err()
    );
    assert_eq!(
        core::first_body_delivery(&open_reader(&f.path).unwrap(), e.id()).unwrap(),
        None
    );
    let result = f.receive(&e, true, "pigeon:body");
    assert!(!result.duplicate);
    assert_eq!(
        core::first_body_delivery(&open_reader(&f.path).unwrap(), e.id()).unwrap(),
        Some(delivery("pigeon:body"))
    );
}
#[test]
fn missing_delegation_stages_valid_bytes_and_unblocks_without_replacing_provenance() {
    let mut f = Fixture::new();
    let e = event(&f.device, "board:a", 1, None, b"actual");
    assert_eq!(
        f.receive(&e, true, "pigeon:alice").readiness,
        Readiness::MissingCredential(f.device.credential().id())
    );
    f.credential();
    f.drain();
    assert_eq!(f.state(e.id()), Readiness::Ready);
    assert_eq!(
        core::first_body_delivery(&open_reader(&f.path).unwrap(), e.id()).unwrap(),
        Some(delivery("pigeon:alice"))
    );
}
#[test]
fn reverse_history_connects_through_a_bodyless_predecessor_without_accepting_its_content() {
    let mut f = Fixture::new();
    f.credential();
    let a = event(&f.device, "board:a", 1, None, b"withheld");
    let b = event(&f.device, "board:a", 2, Some(a.id()), b"independent");
    let c = event(&f.device, "board:a", 3, Some(b.id()), b"last");
    assert_eq!(
        f.receive(&c, true, "local").readiness,
        Readiness::MissingPrevious(b.id())
    );
    f.receive(&b, true, "local");
    f.receive(&a, false, "local");
    f.drain();
    assert_eq!(f.state(a.id()), Readiness::CommitmentOnly);
    assert_eq!(f.state(b.id()), Readiness::Ready);
    assert_eq!(f.state(c.id()), Readiness::Ready);
}
#[test]
fn cross_scope_or_skipped_predecessors_cannot_connect_and_invalidity_propagates() {
    let mut f = Fixture::new();
    f.credential();
    let a = event(&f.device, "board:a", 1, None, b"a");
    f.receive(&a, true, "local");
    let b = event(&f.device, "board:b", 2, Some(a.id()), b"b");
    assert_eq!(
        f.receive(&b, true, "local").readiness,
        Readiness::InvalidChain
    );
    let c = event(&f.device, "board:b", 3, Some(b.id()), b"c");
    assert_eq!(
        f.receive(&c, true, "local").readiness,
        Readiness::InvalidChain
    );
    let d = event(&f.device, "board:a", 4, Some(a.id()), b"d");
    assert_eq!(
        f.receive(&d, true, "local").readiness,
        Readiness::InvalidChain
    );
}
#[test]
fn late_fork_quarantines_both_branches_and_descendants_but_not_earlier_or_unrelated_events() {
    let mut f = Fixture::new();
    f.credential();
    let a = event(&f.device, "board:a", 1, None, b"a");
    let b = event(&f.device, "board:a", 2, Some(a.id()), b"b");
    let c = event(&f.device, "board:a", 3, Some(b.id()), b"c");
    let fork = event(&f.device, "board:a", 2, Some(a.id()), b"fork");
    let other = event(&f.device, "board:other", 1, None, b"unrelated");
    for e in [&a, &b, &c, &other] {
        assert_eq!(f.receive(e, true, "local").readiness, Readiness::Ready);
    }
    assert_eq!(
        f.receive(&fork, true, "local").readiness,
        Readiness::Forked { first_sequence: 2 }
    );
    for e in [&b, &c, &fork] {
        assert_eq!(f.state(e.id()), Readiness::Forked { first_sequence: 2 });
    }
    assert_eq!(f.state(a.id()), Readiness::Ready);
    assert_eq!(f.state(other.id()), Readiness::Ready);
    let evidence = core::forks(&open_reader(&f.path).unwrap()).unwrap();
    assert_eq!(evidence.len(), 1);
    assert_eq!(evidence[0].first_sequence, 2);
}
#[test]
fn a_missing_or_mismatched_delegation_cannot_poison_an_authenticated_stream() {
    let mut f = Fixture::new();
    f.credential();
    let e = event(&f.device, "board:a", 1, None, b"a");
    f.receive(&e, true, "local");
    let forged = rewrite(&e, &f.device, |fields| {
        fields[5].as_array_mut().unwrap()[3] = Value::Bytes([77; 32].to_vec());
    });
    let result = f
        .receive_encoded(&forged, Some(e.body()), "untrusted", &missing)
        .unwrap();
    assert_eq!(
        result.readiness,
        Readiness::MissingCredential(CredentialId::from_bytes([77; 32]))
    );
    f.drain();
    assert_eq!(f.state(e.id()), Readiness::Ready);
    assert!(
        core::forks(&open_reader(&f.path).unwrap())
            .unwrap()
            .is_empty()
    );
    let wrong = device();
    let mismatch = |_: &Connection, _: CredentialId| Ok(Some(wrong.credential().clone()));
    let result = f
        .receive_encoded(&forged, Some(e.body()), "untrusted", &mismatch)
        .unwrap();
    assert_eq!(result.readiness, Readiness::InvalidDelegation);
    assert_eq!(f.state(e.id()), Readiness::Ready);
}
#[test]
fn unknown_major_is_retained_opaque_without_a_verified_body_or_chain_link() {
    let mut f = Fixture::new();
    f.credential();
    let e = event(&f.device, "board:a", 1, None, b"a");
    let unknown = rewrite(&e, &f.device, |fields| fields[1] = Value::Integer(2.into()));
    let result = f
        .receive_encoded(&unknown, Some(e.body()), "peer", &resolve)
        .unwrap();
    assert_eq!(result.readiness, Readiness::Unsupported);
    assert_eq!(
        core::first_body_delivery(&open_reader(&f.path).unwrap(), result.id).unwrap(),
        None
    );
    assert_eq!(f.receive(&e, true, "local").readiness, Readiness::Ready);
}
#[test]
fn quota_and_resolver_failure_roll_back_bytes_links_and_provenance_together() {
    let mut f = Fixture::new();
    f.credential();
    let e = event(&f.device, "board:a", 1, None, b"content");
    let result = f.db.write::<_, core::Error>(Class::Content, 4096, |tx| {
        Inbox::with_quota(e.commitment().encoded().len() as u64)?.receive(
            tx,
            e.commitment().encoded(),
            Some(e.body()),
            &delivery("local"),
            &resolve,
        )
    });
    assert!(matches!(result, Err(core::Error::Quota)));
    let failure = |_: &Connection, _: CredentialId| Err(codec::Error::ResolverUnavailable);
    assert!(
        f.receive_encoded(e.commitment().encoded(), Some(e.body()), "local", &failure)
            .is_err()
    );
    let reader = open_reader(&f.path).unwrap();
    assert_eq!(Inbox::retained_bytes(&reader).unwrap(), 0);
    assert_eq!(core::readiness(&reader, e.id()).unwrap(), None);
    assert_eq!(f.receive(&e, true, "local").readiness, Readiness::Ready);
}
#[test]
fn cursor_batches_progress_past_missing_proofs_and_keep_unsigned_u64_positions_exact() {
    let mut f = Fixture::new();
    let missing_event = event(&f.device, "board:missing", 1, None, b"missing");
    f.receive(&missing_event, true, "peer");
    let e = event(
        &f.device,
        "board:a",
        u64::MAX,
        Some(EventId::from_bytes([8; 32])),
        b"huge",
    );
    f.receive(&e, true, "peer");
    f.credential();
    f.drain();
    assert_eq!(
        f.state(e.id()),
        Readiness::MissingPrevious(EventId::from_bytes([8; 32]))
    );
    let stored: Vec<u8> = open_reader(&f.path)
        .unwrap()
        .query_row(
            "SELECT sequence FROM events_links WHERE id=?1",
            [e.id().bytes().as_slice()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stored, [255; 8]);
    assert!(Inbox::with_quota(0).is_err());
    assert!(
        f.db.write::<_, core::Error>(Class::Content, 4096, |tx| Inbox::default()
            .reconcile(tx, None, 101, &resolve))
            .is_err()
    );
}

#[test]
fn verified_duplicates_do_not_depend_on_a_now_unavailable_credential_resolver() {
    let mut f = Fixture::new();
    f.credential();
    let e = event(&f.device, "board:a", 1, None, b"a");
    f.receive(&e, true, "local");
    let failure = |_: &Connection, _: CredentialId| Err(codec::Error::ResolverUnavailable);
    assert!(
        f.receive_encoded(e.commitment().encoded(), Some(e.body()), "again", &failure)
            .unwrap()
            .duplicate
    );
}

#[test]
fn range_inventory_pages_both_fork_hashes_and_never_crosses_scope() {
    let mut f = Fixture::new();
    f.credential();
    let a = event(&f.device, "board:a", 1, None, b"a");
    let b = event(&f.device, "board:a", 2, Some(a.id()), b"b");
    let c = event(&f.device, "board:a", 2, Some(a.id()), b"fork");
    for e in [&a, &b, &c] {
        f.receive(e, true, "local");
    }
    let reader = open_reader(&f.path).unwrap();
    let first =
        core::inventory(&reader, &a.metadata().scope, a.metadata().stream, None, 2).unwrap();
    let second = core::inventory(
        &reader,
        &a.metadata().scope,
        a.metadata().stream,
        first.last().copied(),
        2,
    )
    .unwrap();
    assert_eq!(first.len(), 2);
    assert_eq!(second.len(), 1);
    let mut ids = first
        .into_iter()
        .chain(second)
        .map(|p| p.id)
        .collect::<Vec<_>>();
    ids.sort();
    let mut expected = vec![a.id(), b.id(), c.id()];
    expected.sort();
    assert_eq!(ids, expected);
    let mut wrong = a.metadata().scope.clone();
    wrong.audience = "private".into();
    assert!(
        core::inventory(&reader, &wrong, a.metadata().stream, None, 2)
            .unwrap()
            .is_empty()
    );
    wrong = a.metadata().scope.clone();
    wrong.distribution = Distribution::Local;
    assert!(
        core::inventory(&reader, &wrong, a.metadata().stream, None, 2)
            .unwrap()
            .is_empty()
    );
}
