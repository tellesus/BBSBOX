// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_client_identity::{ActiveDevice, DeviceKey, RootKey};
use bbsbox_credentials::{CredentialId, ForumId, Purpose, Purposes};
use bbsbox_event_codec::{self as codec, Actor, CompleteEvent, Distribution, Draft, Role, Scope};
use bbsbox_event_core::{
    self as events, Delivery, Inbox, Resolver,
    integration::{self, Integration, Module, Relevance},
};
use bbsbox_private_forum::{
    AUDIENCE, Action, Header,
    control::ControlHeader,
    membership::{Change, Member, MemberRole, MembershipChanges},
};
use bbsbox_private_forum_host::{Host, migrations, posts::Posts};
use bbsbox_storage::{Database, disk::Class, open_reader};
use ed25519_dalek::{Signer, SigningKey};
use rusqlite::Connection;

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
    posts: &Posts<'_, '_>,
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
            &[host, posts],
        )?;
        integration::integrate(
            tx,
            event.id(),
            if event.metadata().actor.role == Role::ForumRoot {
                host as &dyn Module
            } else {
                posts as &dyn Module
            },
            resolver,
        )
    })
}

fn registry() -> Vec<bbsbox_storage::Migration> {
    let mut registry = events::migrations();
    registry.extend(migrations());
    registry
}

fn device() -> ActiveDevice {
    let root = RootKey::generate();
    let key = DeviceKey::generate();
    let credential = root
        .issue_device(key.public(), Purposes::new(&[Purpose::Authorship]).unwrap())
        .unwrap();
    key.activate(root.person_id(), credential).unwrap()
}
fn grant(device: &ActiveDevice, seed: u8) -> Change {
    Change::Grant {
        credential: device.credential().id(),
        member: Member {
            role: MemberRole::Member,
            mls_key: SigningKey::from_bytes(&[seed; 32])
                .verifying_key()
                .to_bytes(),
        },
    }
}
fn post(
    device: &ActiveDevice,
    resource: ForumId,
    header: Header,
    previous: Option<&CompleteEvent>,
) -> CompleteEvent {
    let draft = Draft {
        kind: header.action.kind().into(),
        schema: 1,
        scope: Scope {
            resource: resource.to_string(),
            audience: AUDIENCE.into(),
            distribution: Distribution::Local,
        },
        sequence: previous.map_or(1, |event| event.metadata().sequence + 1),
        previous: previous.map(CompleteEvent::id),
        dependencies: header.dependencies(),
        claimed_time: 0,
        extensions: vec![header.extension().unwrap()],
    };
    codec::sign(
        draft,
        Actor::device(device.credential()),
        b"opaque ciphertext fixture",
        Some(device.credential()),
        |message| {
            device
                .sign_for(Purpose::Authorship, message)
                .map_err(|_| codec::Error::SignerUnavailable)
        },
    )
    .unwrap()
}
fn header(control: &CompleteEvent, epoch: u64, action: Action, n: u8) -> Header {
    Header {
        epoch,
        control: control.id(),
        action_id: [n; 16],
        action,
    }
}

#[test]
fn opaque_posts_stage_authority_then_survive_restart_and_accept_disconnected_old_epoch_work() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("node.sqlite");
    let mut db = Database::open(&path, &registry()).unwrap();
    let owner = SigningKey::from_bytes(&[61; 32]);
    let resource = forum(&owner);
    let alice = device();
    let bob = device();
    let outsider = device();
    let resolver = |_: &Connection, id: CredentialId| {
        Ok([&alice, &bob, &outsider]
            .iter()
            .find(|device| device.credential().id() == id)
            .map(|device| device.credential().clone()))
    };
    let hosted = [(resource, Distribution::Local)];
    let host = Host::new(&hosted, &resolver, 32).unwrap();
    let posts = Posts::new(&host, 32).unwrap();
    let genesis = control(
        &owner,
        1,
        ControlHeader {
            epoch: 0,
            parent: None,
        },
        MembershipChanges::new(vec![grant(&alice, 71), grant(&bob, 72)]).unwrap(),
        b"genesis",
    );
    let first = post(&alice, resource, header(&genesis, 0, Action::Post, 1), None);
    assert!(matches!(
        deliver(&mut db, &host, &posts, &resolver, &first).unwrap(),
        Integration::Staged(_)
    ));
    assert!(matches!(
        deliver(&mut db, &host, &posts, &resolver, &genesis).unwrap(),
        Integration::Accepted(_)
    ));
    assert!(
        matches!(deliver(&mut db,&host,&posts,&resolver,&first).unwrap(),Integration::Accepted(receipt) if receipt.relevance==Relevance::Relevant)
    );
    let outsider_post = post(
        &outsider,
        resource,
        header(&genesis, 0, Action::Post, 1),
        None,
    );
    assert!(matches!(
        deliver(&mut db, &host, &posts, &resolver, &outsider_post).unwrap(),
        Integration::Rejected(_)
    ));
    let removed = control(
        &owner,
        2,
        ControlHeader {
            epoch: 1,
            parent: Some(genesis.id()),
        },
        MembershipChanges::new(vec![Change::Remove {
            credential: alice.credential().id(),
        }])
        .unwrap(),
        b"remove alice",
    );
    deliver(&mut db, &host, &posts, &resolver, &removed).unwrap();
    let stale = post(
        &alice,
        resource,
        header(&genesis, 0, Action::Post, 2),
        Some(&first),
    );
    assert!(matches!(
        deliver(&mut db, &host, &posts, &resolver, &stale).unwrap(),
        Integration::Accepted(_)
    ));
    let current = post(&bob, resource, header(&removed, 1, Action::Post, 1), None);
    deliver(&mut db, &host, &posts, &resolver, &current).unwrap();
    let observed = post(
        &alice,
        resource,
        header(
            &genesis,
            0,
            Action::Reply {
                parent: current.id(),
            },
            3,
        ),
        Some(&stale),
    );
    assert!(
        matches!(deliver(&mut db,&host,&posts,&resolver,&observed).unwrap(),Integration::Rejected(reason) if reason=="causally_observed_membership_removal")
    );
    let wrong_epoch = post(
        &outsider,
        resource,
        header(&genesis, 1, Action::Post, 2),
        None,
    );
    assert!(matches!(
        deliver(&mut db, &host, &posts, &resolver, &wrong_epoch).unwrap(),
        Integration::Rejected(_)
    ));
    drop(db);
    let mut db = Database::open(&path, &registry()).unwrap();
    let reader = open_reader(&path).unwrap();
    assert_eq!(
        posts
            .admitted(&reader, resource, Distribution::Local)
            .unwrap()
            .len(),
        3
    );
    assert!(matches!(
        deliver(&mut db, &host, &posts, &resolver, &stale).unwrap(),
        Integration::Duplicate(_)
    ));
    assert!(!posts.dependency_effective(&reader, observed.id()).unwrap());
}

#[test]
fn reply_dependencies_and_late_control_or_device_forks_retract_derived_visibility_without_recredit()
{
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("node.sqlite");
    let mut db = Database::open(&path, &registry()).unwrap();
    let owner = SigningKey::from_bytes(&[62; 32]);
    let resource = forum(&owner);
    let alice = device();
    let bob = device();
    let resolver = |_: &Connection, id: CredentialId| {
        Ok([&alice, &bob]
            .iter()
            .find(|device| device.credential().id() == id)
            .map(|device| device.credential().clone()))
    };
    let hosted = [(resource, Distribution::Local)];
    let host = Host::new(&hosted, &resolver, 32).unwrap();
    let posts = Posts::new(&host, 32).unwrap();
    let genesis = control(
        &owner,
        1,
        ControlHeader {
            epoch: 0,
            parent: None,
        },
        MembershipChanges::new(vec![grant(&alice, 71), grant(&bob, 72)]).unwrap(),
        b"genesis",
    );
    let next = control(
        &owner,
        2,
        ControlHeader {
            epoch: 1,
            parent: Some(genesis.id()),
        },
        Default::default(),
        b"next",
    );
    for event in [&genesis, &next] {
        deliver(&mut db, &host, &posts, &resolver, event).unwrap();
    }
    let first = post(&alice, resource, header(&next, 1, Action::Post, 1), None);
    let reply = post(
        &bob,
        resource,
        header(&next, 1, Action::Reply { parent: first.id() }, 1),
        None,
    );
    assert!(matches!(
        deliver(&mut db, &host, &posts, &resolver, &reply).unwrap(),
        Integration::Staged(_)
    ));
    for event in [&first, &reply] {
        assert!(matches!(
            deliver(&mut db, &host, &posts, &resolver, event).unwrap(),
            Integration::Accepted(_)
        ));
    }
    let reader = open_reader(&path).unwrap();
    let receipt = integration::receipt(&reader, reply.id()).unwrap().unwrap();
    assert_eq!(
        posts
            .admitted(&reader, resource, Distribution::Local)
            .unwrap()
            .len(),
        2
    );
    let fork = post(&alice, resource, header(&next, 1, Action::Post, 2), None);
    assert!(matches!(
        deliver(&mut db, &host, &posts, &resolver, &fork).unwrap(),
        Integration::Rejected(_)
    ));
    assert!(
        posts
            .admitted(&reader, resource, Distribution::Local)
            .unwrap()
            .is_empty()
    );
    assert!(!posts.dependency_effective(&reader, reply.id()).unwrap());
    assert_eq!(
        integration::receipt(&reader, reply.id()).unwrap().unwrap(),
        receipt
    );
    let branch = control(
        &owner,
        3,
        ControlHeader {
            epoch: 1,
            parent: Some(genesis.id()),
        },
        Default::default(),
        b"competing next",
    );
    deliver(&mut db, &host, &posts, &resolver, &branch).unwrap();
    assert!(
        posts
            .admitted(&reader, resource, Distribution::Local)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        integration::receipt(&reader, reply.id()).unwrap().unwrap(),
        receipt
    );
}

#[test]
fn application_index_failures_roll_back_intake_and_capacity_does_not_reject_duplicates() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("node.sqlite");
    let mut db = Database::open(&path, &registry()).unwrap();
    let owner = SigningKey::from_bytes(&[63; 32]);
    let resource = forum(&owner);
    let alice = device();
    let resolver = |_: &Connection, id: CredentialId| {
        Ok((alice.credential().id() == id).then(|| alice.credential().clone()))
    };
    let hosted = [(resource, Distribution::Local)];
    let host = Host::new(&hosted, &resolver, 32).unwrap();
    let posts = Posts::new(&host, 1).unwrap();
    let genesis = control(
        &owner,
        1,
        ControlHeader {
            epoch: 0,
            parent: None,
        },
        MembershipChanges::new(vec![grant(&alice, 71)]).unwrap(),
        b"genesis",
    );
    deliver(&mut db, &host, &posts, &resolver, &genesis).unwrap();
    let first = post(&alice, resource, header(&genesis, 0, Action::Post, 1), None);
    db.write::<_,events::Error>(Class::Control,4096,|tx|{tx.execute_batch("CREATE TRIGGER fail_post BEFORE INSERT ON private_forum_posts BEGIN SELECT RAISE(ABORT,'fixture failure'); END;")?;Ok(())}).unwrap();
    assert!(deliver(&mut db, &host, &posts, &resolver, &first).is_err());
    let reader = open_reader(&path).unwrap();
    assert!(events::readiness(&reader, first.id()).unwrap().is_none());
    assert!(integration::receipt(&reader, first.id()).unwrap().is_none());
    db.write::<_, events::Error>(Class::Control, 4096, |tx| {
        tx.execute_batch("DROP TRIGGER fail_post")?;
        Ok(())
    })
    .unwrap();
    deliver(&mut db, &host, &posts, &resolver, &first).unwrap();
    let second = post(
        &alice,
        resource,
        header(&genesis, 0, Action::Post, 2),
        Some(&first),
    );
    assert!(matches!(
        deliver(&mut db, &host, &posts, &resolver, &second),
        Err(events::Error::InvalidLimit)
    ));
    assert!(events::readiness(&reader, second.id()).unwrap().is_none());
    assert!(matches!(
        deliver(&mut db, &host, &posts, &resolver, &first).unwrap(),
        Integration::Duplicate(_)
    ));
    assert!(Posts::new(&host, 0).is_err());
    assert!(Posts::new(&host, 4097).is_err());
    assert_eq!(
        posts
            .admitted(&reader, resource, Distribution::Local)
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn a_late_owner_conflict_hides_its_posts_but_preserves_unrelated_old_epoch_work() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("node.sqlite");
    let mut db = Database::open(&path, &registry()).unwrap();
    let owner = SigningKey::from_bytes(&[64; 32]);
    let resource = forum(&owner);
    let alice = device();
    let bob = device();
    let resolver = |_: &Connection, id: CredentialId| {
        Ok([&alice, &bob]
            .iter()
            .find(|device| device.credential().id() == id)
            .map(|device| device.credential().clone()))
    };
    let hosted = [(resource, Distribution::Local)];
    let host = Host::new(&hosted, &resolver, 32).unwrap();
    let posts = Posts::new(&host, 32).unwrap();
    let genesis = control(
        &owner,
        1,
        ControlHeader {
            epoch: 0,
            parent: None,
        },
        MembershipChanges::new(vec![grant(&alice, 71), grant(&bob, 72)]).unwrap(),
        b"genesis",
    );
    let next = control(
        &owner,
        2,
        ControlHeader {
            epoch: 1,
            parent: Some(genesis.id()),
        },
        Default::default(),
        b"next",
    );
    let old = post(&alice, resource, header(&genesis, 0, Action::Post, 1), None);
    let advanced = post(&bob, resource, header(&next, 1, Action::Post, 1), None);
    for event in [&genesis, &next, &old, &advanced] {
        deliver(&mut db, &host, &posts, &resolver, event).unwrap();
    }
    let reader = open_reader(&path).unwrap();
    let receipt = integration::receipt(&reader, advanced.id())
        .unwrap()
        .unwrap();
    assert_eq!(
        posts
            .admitted(&reader, resource, Distribution::Local)
            .unwrap()
            .len(),
        2
    );
    let branch = control(
        &owner,
        3,
        ControlHeader {
            epoch: 1,
            parent: Some(genesis.id()),
        },
        Default::default(),
        b"competing next",
    );
    deliver(&mut db, &host, &posts, &resolver, &branch).unwrap();
    let visible = posts
        .admitted(&reader, resource, Distribution::Local)
        .unwrap();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].id, old.id());
    assert!(!posts.dependency_effective(&reader, advanced.id()).unwrap());
    assert_eq!(
        integration::receipt(&reader, advanced.id())
            .unwrap()
            .unwrap(),
        receipt
    );
    let foreign_owner = SigningKey::from_bytes(&[65; 32]);
    let foreign = forum(&foreign_owner);
    let denied = post(&alice, foreign, header(&genesis, 0, Action::Post, 5), None);
    assert!(
        matches!(deliver(&mut db,&host,&posts,&resolver,&denied).unwrap(),Integration::Rejected(reason) if reason=="forum_not_hosted")
    );
}
#[test]
fn missing_device_proofs_wait_cross_scope_replies_fail_and_unimplemented_revisions_stay_unsupported()
 {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("node.sqlite");
    let mut db = Database::open(&path, &registry()).unwrap();
    let owner = SigningKey::from_bytes(&[66; 32]);
    let resource = forum(&owner);
    let other_owner = SigningKey::from_bytes(&[67; 32]);
    let other = forum(&other_owner);
    let alice = device();
    let known = std::cell::Cell::new(false);
    let resolver = |_: &Connection, id: CredentialId| {
        Ok((known.get() && alice.credential().id() == id).then(|| alice.credential().clone()))
    };
    let hosted = [
        (resource, Distribution::Local),
        (other, Distribution::Local),
    ];
    let host = Host::new(&hosted, &resolver, 32).unwrap();
    let posts = Posts::new(&host, 32).unwrap();
    let genesis = control(
        &owner,
        1,
        ControlHeader {
            epoch: 0,
            parent: None,
        },
        MembershipChanges::new(vec![grant(&alice, 71)]).unwrap(),
        b"genesis",
    );
    let first = post(&alice, resource, header(&genesis, 0, Action::Post, 1), None);
    assert!(matches!(
        deliver(&mut db, &host, &posts, &resolver, &first).unwrap(),
        Integration::Staged(_)
    ));
    let reader = open_reader(&path).unwrap();
    assert!(integration::receipt(&reader, first.id()).unwrap().is_none());
    known.set(true);
    deliver(&mut db, &host, &posts, &resolver, &genesis).unwrap();
    assert!(matches!(
        deliver(&mut db, &host, &posts, &resolver, &first).unwrap(),
        Integration::Accepted(_)
    ));
    let other_genesis = control(
        &other_owner,
        1,
        ControlHeader {
            epoch: 0,
            parent: None,
        },
        MembershipChanges::new(vec![grant(&alice, 72)]).unwrap(),
        b"other genesis",
    );
    let other_post = post(
        &alice,
        other,
        header(&other_genesis, 0, Action::Post, 1),
        None,
    );
    for event in [&other_genesis, &other_post] {
        deliver(&mut db, &host, &posts, &resolver, event).unwrap();
    }
    let cross = post(
        &alice,
        resource,
        header(
            &genesis,
            0,
            Action::Reply {
                parent: other_post.id(),
            },
            2,
        ),
        Some(&first),
    );
    assert!(
        matches!(deliver(&mut db,&host,&posts,&resolver,&cross).unwrap(),Integration::Rejected(reason) if reason=="cross_scope_content_dependency")
    );
    let edit = post(
        &alice,
        resource,
        header(
            &genesis,
            0,
            Action::Edit {
                post: first.id(),
                prior: first.id(),
            },
            3,
        ),
        Some(&cross),
    );
    assert!(matches!(
        deliver(&mut db, &host, &posts, &resolver, &edit).unwrap(),
        Integration::Unsupported
    ));
    assert!(integration::receipt(&reader, edit.id()).unwrap().is_none());
}
