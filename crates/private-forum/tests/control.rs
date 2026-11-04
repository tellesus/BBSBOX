// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_client_identity::{DeviceKey, RootKey};
use bbsbox_credentials::{Credential, ForumId, Purpose, Purposes};
use bbsbox_event_codec::{
    self as events, Actor, CompleteEvent, Dependency, Distribution, Draft, EventId, Role, Scope,
};
use bbsbox_private_forum::{
    AUDIENCE, Action, Error, Header, Outer,
    control::{Control, ControlHeader, Successor, successor},
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
