// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_client_identity::{DeviceKey, RootKey};
use bbsbox_credentials::{CredentialId, ForumId, Purpose, Purposes};
use bbsbox_event_codec::{self as codec, Actor, CompleteEvent, Distribution, Draft, Role, Scope};
use bbsbox_event_core::{
    self as events, Delivery, Inbox, Resolver,
    integration::{self, Integration, Module, Relevance},
};
use bbsbox_private_forum::{
    AUDIENCE,
    control::ControlHeader,
    governance::Stop,
    membership::{Change, Member, MemberRole, MembershipChanges},
};
use bbsbox_private_forum_host::{Host, migrations};
use bbsbox_storage::{Database, disk::Class, open_reader};
use ed25519_dalek::{Signer, SigningKey};
use rusqlite::Connection;
use std::cell::RefCell;

fn forum(key: &SigningKey) -> ForumId {
    ForumId::from_public_key(&key.verifying_key().to_bytes()).unwrap()
}

fn control(
    key: &SigningKey,
    incarnation: u8,
    header: ControlHeader,
    changes: MembershipChanges,
    body: &[u8],
) -> CompleteEvent {
    let actor = Actor::root(
        Role::ForumRoot,
        key.verifying_key().to_bytes(),
        [incarnation; 16],
    )
    .unwrap();
    let mut dependencies = header
        .parent
        .map(codec::Dependency::event)
        .into_iter()
        .chain(changes.dependencies())
        .collect::<Vec<_>>();
    dependencies.sort_unstable();
    let draft = Draft {
        kind: header.kind().into(),
        schema: 1,
        scope: Scope {
            resource: forum(key).to_string(),
            audience: AUDIENCE.into(),
            distribution: Distribution::Local,
        },
        sequence: 1,
        previous: None,
        dependencies,
        claimed_time: 0,
        extensions: [Some(header.extension().unwrap()), changes.extension()]
            .into_iter()
            .flatten()
            .collect(),
    };
    codec::sign(draft, actor, body, None, |bytes| {
        Ok(key.sign(bytes).to_bytes())
    })
    .unwrap()
}

fn deliver(
    database: &mut Database,
    host: &Host<'_>,
    resolver: &dyn Resolver,
    event: &CompleteEvent,
) -> events::Result<Integration> {
    database.write(Class::Content, 65536, |tx| {
        integration::receive(
            tx,
            Inbox::default(),
            event.commitment().encoded(),
            Some(event.body()),
            &Delivery {
                source: "local-client".into(),
                transfer: "fixture".into(),
            },
            resolver,
            &[host],
        )?;
        integration::integrate(tx, event.id(), host, resolver)
    })
}

fn registry() -> Vec<bbsbox_storage::Migration> {
    let mut registry = events::migrations();
    registry.extend(migrations());
    registry
}

#[test]
fn durable_history_stages_missing_parents_deduplicates_and_withdraws_conflicted_descendants() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("node.sqlite");
    let mut database = Database::open(&path, &registry()).unwrap();
    let key = SigningKey::from_bytes(&[41; 32]);
    let resource = forum(&key);
    let resolver = |_: &Connection, _: CredentialId| Ok(None);
    let hosted = [(resource, Distribution::Local)];
    let host = Host::new(&hosted, &resolver, 32).unwrap();
    let genesis = control(
        &key,
        1,
        ControlHeader {
            epoch: 0,
            parent: None,
        },
        Default::default(),
        b"opaque genesis",
    );
    let first = control(
        &key,
        2,
        ControlHeader {
            epoch: 1,
            parent: Some(genesis.id()),
        },
        Default::default(),
        b"opaque first commit",
    );
    let second = control(
        &key,
        3,
        ControlHeader {
            epoch: 2,
            parent: Some(first.id()),
        },
        Default::default(),
        b"opaque second commit",
    );
    assert!(matches!(
        deliver(&mut database, &host, &resolver, &second).unwrap(),
        Integration::Staged(_)
    ));
    for event in [&genesis, &first, &second] {
        assert!(
            matches!(deliver(&mut database,&host,&resolver,event).unwrap(),
            Integration::Accepted(receipt) if receipt.relevance==Relevance::Relevant)
        );
    }
    assert!(matches!(
        deliver(&mut database, &host, &resolver, &first).unwrap(),
        Integration::Duplicate(_)
    ));
    drop(database);
    let mut database = Database::open(&path, &registry()).unwrap();
    let reader = open_reader(&path).unwrap();
    let before = integration::receipt(&reader, second.id()).unwrap().unwrap();
    assert_eq!(
        host.history(&reader, resource, Distribution::Local)
            .unwrap()
            .epochs()
            .len(),
        3
    );
    let competing = control(
        &key,
        4,
        ControlHeader {
            epoch: 1,
            parent: Some(genesis.id()),
        },
        Default::default(),
        b"competing owner approval",
    );
    assert!(matches!(
        deliver(&mut database, &host, &resolver, &competing).unwrap(),
        Integration::Accepted(receipt) if receipt.relevance == Relevance::DependencyOnly
    ));
    let history = host
        .history(&reader, resource, Distribution::Local)
        .unwrap();
    assert_eq!(
        history.stop(),
        Stop::Conflict {
            parent: Some(genesis.id())
        }
    );
    assert_eq!(history.epochs().len(), 1);
    assert!(!host.dependency_effective(&reader, first.id()).unwrap());
    assert!(!host.dependency_effective(&reader, second.id()).unwrap());
    assert_eq!(
        integration::receipt(&reader, second.id()).unwrap().unwrap(),
        before
    );
    let descendant = control(
        &key,
        5,
        ControlHeader {
            epoch: 3,
            parent: Some(second.id()),
        },
        Default::default(),
        b"blocked descendant",
    );
    assert!(matches!(
        deliver(&mut database, &host, &resolver, &descendant).unwrap(),
        Integration::Staged(_)
    ));
    assert_eq!(
        reader
            .query_row("SELECT count(*) FROM private_forum_controls", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        4
    );
}

#[test]
fn unknown_credentials_wait_then_bind_and_same_stream_forks_invalidate_prior_history() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("node.sqlite");
    let mut database = Database::open(&path, &registry()).unwrap();
    let key = SigningKey::from_bytes(&[42; 32]);
    let resource = forum(&key);
    let credential = RootKey::generate()
        .issue_device(
            DeviceKey::generate().public(),
            Purposes::new(&[Purpose::Authorship]).unwrap(),
        )
        .unwrap();
    let known = RefCell::new(None);
    let resolver = |_: &Connection, id: CredentialId| {
        Ok(known
            .borrow()
            .as_ref()
            .filter(|proof: &&bbsbox_credentials::Credential| proof.id() == id)
            .cloned())
    };
    let hosted = [(resource, Distribution::Local)];
    let host = Host::new(&hosted, &resolver, 32).unwrap();
    let member = Member {
        role: MemberRole::Member,
        mls_key: SigningKey::from_bytes(&[19; 32]).verifying_key().to_bytes(),
    };
    let genesis = control(
        &key,
        1,
        ControlHeader {
            epoch: 0,
            parent: None,
        },
        MembershipChanges::new(vec![Change::Grant {
            credential: credential.id(),
            member,
        }])
        .unwrap(),
        b"opaque genesis",
    );
    assert!(matches!(
        deliver(&mut database, &host, &resolver, &genesis).unwrap(),
        Integration::Staged(_)
    ));
    *known.borrow_mut() = Some(credential.clone());
    assert!(matches!(
        deliver(&mut database, &host, &resolver, &genesis).unwrap(),
        Integration::Accepted(_)
    ));
    let reader = open_reader(&path).unwrap();
    let history = host
        .history(&reader, resource, Distribution::Local)
        .unwrap();
    assert_eq!(history.epochs()[0].member(credential.id()), Some(member));
    let fork = control(
        &key,
        1,
        ControlHeader {
            epoch: 0,
            parent: None,
        },
        Default::default(),
        b"same stream position with another body",
    );
    assert!(matches!(
        deliver(&mut database, &host, &resolver, &fork).unwrap(),
        Integration::Rejected(_)
    ));
    let history = host
        .history(&reader, resource, Distribution::Local)
        .unwrap();
    assert_eq!(history.stop(), Stop::MissingGenesis);
    assert!(history.epochs().is_empty());
    assert!(
        integration::receipt(&reader, genesis.id())
            .unwrap()
            .is_some()
    );
}

#[test]
fn admission_is_scoped_bounded_and_rolls_back_intake_with_a_failed_index_write() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("node.sqlite");
    let mut database = Database::open(&path, &registry()).unwrap();
    let key = SigningKey::from_bytes(&[43; 32]);
    let resource = forum(&key);
    let resolver = |_: &Connection, _: CredentialId| Ok(None);
    let hosted = [(resource, Distribution::Local)];
    let host = Host::new(&hosted, &resolver, 1).unwrap();
    let genesis = control(
        &key,
        1,
        ControlHeader {
            epoch: 0,
            parent: None,
        },
        Default::default(),
        b"genesis",
    );
    database.write::<_,events::Error>(Class::Control,4096,|tx| {
        tx.execute_batch("CREATE TRIGGER fail_index BEFORE INSERT ON private_forum_controls BEGIN SELECT RAISE(ABORT,'fixture failure'); END;")?;Ok(())
    }).unwrap();
    assert!(deliver(&mut database, &host, &resolver, &genesis).is_err());
    let reader = open_reader(&path).unwrap();
    assert!(events::readiness(&reader, genesis.id()).unwrap().is_none());
    assert!(
        integration::receipt(&reader, genesis.id())
            .unwrap()
            .is_none()
    );
    database
        .write::<_, events::Error>(Class::Control, 4096, |tx| {
            tx.execute_batch("DROP TRIGGER fail_index")?;
            Ok(())
        })
        .unwrap();
    let foreign_key = SigningKey::from_bytes(&[44; 32]);
    let foreign = control(
        &foreign_key,
        1,
        ControlHeader {
            epoch: 0,
            parent: None,
        },
        Default::default(),
        b"unconfigured forum",
    );
    assert!(matches!(
        deliver(&mut database, &host, &resolver, &foreign).unwrap(),
        Integration::Rejected(_)
    ));
    let invalid = control(
        &key,
        2,
        ControlHeader {
            epoch: 0,
            parent: None,
        },
        MembershipChanges::new(vec![Change::Remove {
            credential: CredentialId::from_bytes([1; 32]),
        }])
        .unwrap(),
        b"invalid membership",
    );
    assert!(matches!(
        deliver(&mut database, &host, &resolver, &invalid).unwrap(),
        Integration::Rejected(_)
    ));
    assert!(matches!(
        deliver(&mut database, &host, &resolver, &genesis).unwrap(),
        Integration::Accepted(_)
    ));
    let next = control(
        &key,
        3,
        ControlHeader {
            epoch: 1,
            parent: Some(genesis.id()),
        },
        Default::default(),
        b"capacity exceeded",
    );
    assert!(matches!(
        deliver(&mut database, &host, &resolver, &next),
        Err(events::Error::InvalidLimit)
    ));
    assert!(events::readiness(&reader, next.id()).unwrap().is_none());
    assert!(matches!(
        deliver(&mut database, &host, &resolver, &genesis).unwrap(),
        Integration::Duplicate(_)
    ));
    assert_eq!(
        host.history(&reader, resource, Distribution::Local)
            .unwrap()
            .epochs()
            .len(),
        1
    );
    assert!(Host::new(&hosted, &resolver, 0).is_err());
    assert!(Host::new(&hosted, &resolver, 4097).is_err());
}
