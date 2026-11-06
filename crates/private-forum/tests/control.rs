// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_client_identity::{DeviceKey, RootKey};
use bbsbox_credentials::{Credential, ForumId, Purpose, Purposes};
use bbsbox_event_codec::{
    self as events, Actor, CompleteEvent, Dependency, Distribution, Draft, EventId, Role, Scope,
};
use bbsbox_private_forum::{
    AUDIENCE, Action, Error, Header, Outer,
    control::{Control, ControlHeader, Successor, successor},
    governance::{Governance, Stop},
    membership::{Change, Member, MemberRole, MembershipChanges, Roster},
};
use ed25519_dalek::{Signer, SigningKey};

fn forum(key: &SigningKey) -> ForumId {
    ForumId::from_public_key(&key.verifying_key().to_bytes()).unwrap()
}

fn signed(
    key: &SigningKey,
    forum: ForumId,
    role: Role,
    incarnation: u8,
    header: ControlHeader,
    body: &[u8],
) -> CompleteEvent {
    signed_with_changes(
        key,
        forum,
        role,
        incarnation,
        header,
        body,
        MembershipChanges::default(),
    )
}

fn signed_with_changes(
    key: &SigningKey,
    forum: ForumId,
    role: Role,
    incarnation: u8,
    header: ControlHeader,
    body: &[u8],
    membership: MembershipChanges,
) -> CompleteEvent {
    let actor = Actor::root(role, key.verifying_key().to_bytes(), [incarnation; 16]).unwrap();
    let draft = Draft {
        kind: header.kind().into(),
        schema: 1,
        scope: Scope {
            resource: forum.to_string(),
            audience: AUDIENCE.into(),
            distribution: Distribution::Local,
        },
        sequence: 1,
        previous: None,
        dependencies: header
            .parent
            .map(Dependency::event)
            .into_iter()
            .chain(membership.dependencies())
            .collect(),
        claimed_time: i64::from(incarnation),
        extensions: [Some(header.extension().unwrap()), membership.extension()]
            .into_iter()
            .flatten()
            .collect(),
    };
    events::sign(draft, actor, body, None, |bytes| {
        Ok(key.sign(bytes).to_bytes())
    })
    .unwrap()
}

fn credential(purposes: &[Purpose]) -> Credential {
    RootKey::generate()
        .issue_device(
            DeviceKey::generate().public(),
            Purposes::new(purposes).unwrap(),
        )
        .unwrap()
}

#[test]
fn owner_membership_binds_exact_device_credentials_to_an_epoch_and_mls_leaf() {
    let owner = SigningKey::from_bytes(&[17; 32]);
    let resource = forum(&owner);
    let ordinary = credential(&[Purpose::Authorship]);
    let moderator = credential(&[Purpose::Authorship, Purpose::ForumGovernance]);
    let mls = |seed| {
        SigningKey::from_bytes(&[seed; 32])
            .verifying_key()
            .to_bytes()
    };
    let member = Member {
        role: MemberRole::Member,
        mls_key: mls(11),
    };
    let mod_member = Member {
        role: MemberRole::Moderator,
        mls_key: mls(12),
    };
    let grant = |credential: &Credential, member| Change::Grant {
        credential: credential.id(),
        member,
    };
    let create = |header, changes: Vec<_>| {
        let event = signed_with_changes(
            &owner,
            resource,
            Role::ForumRoot,
            1,
            header,
            b"opaque control",
            MembershipChanges::new(changes).unwrap(),
        );
        Control::inspect(&event).unwrap()
    };
    let genesis = ControlHeader {
        epoch: 0,
        parent: None,
    };
    let active = create(
        genesis,
        vec![grant(&moderator, mod_member), grant(&ordinary, member)],
    );
    let resolve = |id| {
        [&ordinary, &moderator]
            .into_iter()
            .find(|known| known.id() == id)
            .cloned()
    };
    assert!(matches!(
        Roster::apply(active.clone(), None, |_| None),
        Err(Error::MissingCredential(_))
    ));
    let roster = Roster::apply(active.clone(), None, resolve).unwrap();
    assert_eq!(roster.member(moderator.id()), Some(mod_member));
    let next = create(
        ControlHeader {
            epoch: 1,
            parent: Some(active.id()),
        },
        vec![Change::Remove {
            credential: ordinary.id(),
        }],
    );
    let removed = Roster::apply(next.clone(), Some(&roster), resolve).unwrap();
    let post = |control: &Control| {
        let header = Header {
            epoch: control.header().epoch,
            control: control.id(),
            action_id: [1; 16],
            action: Action::Post,
        };
        let draft = Draft {
            kind: header.action.kind().into(),
            schema: 1,
            scope: Scope {
                resource: resource.to_string(),
                audience: AUDIENCE.into(),
                distribution: Distribution::Local,
            },
            sequence: 1,
            previous: None,
            dependencies: header.dependencies(),
            claimed_time: 0,
            extensions: vec![header.extension().unwrap()],
        };
        Outer::before_encryption(&draft, &Actor::device(&ordinary)).unwrap()
    };
    assert_eq!(roster.post_author(&post(&active)), Some(member));
    assert_eq!(removed.post_author(&post(&next)), None);
    assert_eq!(removed.post_author(&post(&active)), None);
    let history =
        Governance::resolve(resource, Distribution::Local, [&next, &active], resolve).unwrap();
    assert_eq!(history.stop(), Stop::Tip);
    assert_eq!(history.epochs().len(), 2);
    assert_eq!(history.post_author(&post(&active)), Some(member));
    assert_eq!(history.post_author(&post(&next)), None);
    let newcomer = credential(&[Purpose::Authorship]);
    let pending = create(
        ControlHeader {
            epoch: 2,
            parent: Some(next.id()),
        },
        vec![grant(&newcomer, member)],
    );
    let waiting = Governance::resolve(
        resource,
        Distribution::Local,
        [&pending, &next, &active],
        resolve,
    )
    .unwrap();
    assert_eq!(waiting.stop(), Stop::MissingCredential(newcomer.id()));
    assert_eq!(waiting.epochs().len(), 2);
    assert_eq!(waiting.post_author(&post(&active)), Some(member));
    let complete = Governance::resolve(
        resource,
        Distribution::Local,
        [&pending, &active, &next],
        |id| {
            if id == newcomer.id() {
                Some(newcomer.clone())
            } else {
                resolve(id)
            }
        },
    )
    .unwrap();
    assert_eq!(complete.stop(), Stop::Tip);
    assert_eq!(complete.epochs().len(), 3);
    let unauthorized_mod = create(genesis, vec![grant(&ordinary, mod_member)]);
    assert!(matches!(
        Roster::apply(unauthorized_mod, None, resolve),
        Err(Error::InvalidMembership)
    ));
    let duplicate_leaf = create(
        genesis,
        vec![grant(&ordinary, member), grant(&moderator, member)],
    );
    assert!(matches!(
        Roster::apply(duplicate_leaf, None, resolve),
        Err(Error::InvalidMembership)
    ));
}

#[test]
fn a_node_or_another_forum_root_cannot_authorize_this_forum_epoch() {
    let owner = SigningKey::from_bytes(&[1; 32]);
    let other = SigningKey::from_bytes(&[2; 32]);
    let resource = forum(&owner);
    let genesis = ControlHeader {
        epoch: 0,
        parent: None,
    };
    let good = signed(
        &owner,
        resource,
        Role::ForumRoot,
        1,
        genesis,
        b"opaque MLS creation",
    );
    assert_eq!(Control::inspect(&good).unwrap().header(), genesis);
    for role in [Role::Node, Role::PersonRoot, Role::ForumRoot] {
        let invalid = signed(
            &other,
            resource,
            role,
            1,
            genesis,
            b"same opaque MLS creation",
        );
        assert_eq!(Control::inspect(&invalid), Err(Error::InvalidAuthor));
    }
    let invalid = ControlHeader {
        epoch: 1,
        parent: None,
    };
    assert_eq!(invalid.extension(), Err(Error::InvalidHeader));
}

#[test]
fn competing_owner_approvals_freeze_advancement_independent_of_arrival_order() {
    let owner = SigningKey::from_bytes(&[5; 32]);
    let resource = forum(&owner);
    let genesis = signed(
        &owner,
        resource,
        Role::ForumRoot,
        1,
        ControlHeader {
            epoch: 0,
            parent: None,
        },
        b"genesis",
    );
    let parent = Control::inspect(&genesis).unwrap();
    let child = |epoch, incarnation, body: &[u8]| {
        let header = ControlHeader {
            epoch,
            parent: Some(genesis.id()),
        };
        Control::inspect(&signed(
            &owner,
            resource,
            Role::ForumRoot,
            incarnation,
            header,
            body,
        ))
        .unwrap()
    };
    let first = child(1, 2, b"approved MLS change A");
    let second = child(1, 3, b"approved MLS change B");
    let wrong_epoch = child(2, 4, b"skip a control epoch");
    assert_eq!(successor(&parent, [&wrong_epoch]), Successor::None);
    assert_eq!(
        successor(&parent, [&first, &first, &wrong_epoch]),
        Successor::Unique(&first)
    );
    assert_eq!(successor(&parent, [&first, &second]), Successor::Conflict);
    assert_eq!(successor(&parent, [&second, &first]), Successor::Conflict);
    assert_ne!(first.id(), second.id());
    assert!(
        ControlHeader {
            epoch: 1,
            parent: Some(EventId::from_bytes([0; 32]))
        }
        .extension()
        .is_err()
    );
}

#[test]
fn retained_control_history_reconstructs_causally_and_withdraws_a_late_conflicting_branch() {
    let owner = SigningKey::from_bytes(&[31; 32]);
    let resource = forum(&owner);
    let control = |epoch, parent, incarnation, body: &[u8]| {
        Control::inspect(&signed(
            &owner,
            resource,
            Role::ForumRoot,
            incarnation,
            ControlHeader { epoch, parent },
            body,
        ))
        .unwrap()
    };
    let genesis = control(0, None, 1, b"genesis");
    let first = control(1, Some(genesis.id()), 2, b"first");
    let second = control(2, Some(first.id()), 3, b"second");
    let orders = [
        [&genesis, &first, &second],
        [&genesis, &second, &first],
        [&first, &genesis, &second],
        [&first, &second, &genesis],
        [&second, &genesis, &first],
        [&second, &first, &genesis],
    ];
    for order in orders {
        let history = Governance::resolve(resource, Distribution::Local, order, |_| None).unwrap();
        assert_eq!(history.stop(), Stop::Tip);
        assert_eq!(
            history
                .epochs()
                .iter()
                .map(|row| row.control().id())
                .collect::<Vec<_>>(),
            vec![genesis.id(), first.id(), second.id()]
        );
    }
    let missing =
        Governance::resolve(resource, Distribution::Local, [&second, &first], |_| None).unwrap();
    assert_eq!(missing.stop(), Stop::MissingGenesis);
    assert!(missing.epochs().is_empty());
    let gap =
        Governance::resolve(resource, Distribution::Local, [&second, &genesis], |_| None).unwrap();
    assert_eq!(gap.stop(), Stop::Tip);
    assert_eq!(gap.epochs().len(), 1);
    let fork = control(1, Some(genesis.id()), 4, b"conflicting approval");
    let foreign_owner = SigningKey::from_bytes(&[32; 32]);
    let foreign = Control::inspect(&signed(
        &foreign_owner,
        forum(&foreign_owner),
        Role::ForumRoot,
        1,
        ControlHeader {
            epoch: 0,
            parent: None,
        },
        b"another forum",
    ))
    .unwrap();
    let skipped = control(3, Some(genesis.id()), 5, b"wrong epoch");
    for order in [
        vec![&second, &first, &genesis, &fork, &foreign, &skipped],
        vec![&fork, &first, &first, &genesis, &second, &foreign, &skipped],
    ] {
        let history = Governance::resolve(resource, Distribution::Local, order, |_| None).unwrap();
        assert_eq!(
            history.stop(),
            Stop::Conflict {
                parent: Some(genesis.id())
            }
        );
        assert_eq!(history.epochs().len(), 1);
        assert_eq!(history.epochs()[0].control().id(), genesis.id());
    }
    let other_genesis = control(0, None, 6, b"competing genesis");
    let conflict = Governance::resolve(
        resource,
        Distribution::Local,
        [&genesis, &other_genesis],
        |_| None,
    )
    .unwrap();
    assert_eq!(conflict.stop(), Stop::Conflict { parent: None });
    assert!(conflict.epochs().is_empty());
    let wrong_distribution =
        Governance::resolve(resource, Distribution::Shared, [&genesis], |_| None).unwrap();
    assert_eq!(wrong_distribution.stop(), Stop::MissingGenesis);
}
