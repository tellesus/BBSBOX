// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_credentials::ForumId;
use bbsbox_event_codec::{
    self as events, Actor, CompleteEvent, Dependency, Distribution, Draft, EventId, Role, Scope,
};
use bbsbox_private_forum::{
    AUDIENCE, Error,
    control::{Control, ControlHeader, Successor, successor},
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
        dependencies: header.parent.map(Dependency::event).into_iter().collect(),
        claimed_time: i64::from(incarnation),
        extensions: vec![header.extension().unwrap()],
    };
    events::sign(draft, actor, body, None, |bytes| {
        Ok(key.sign(bytes).to_bytes())
    })
    .unwrap()
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
    assert_eq!(successor(parent, [wrong_epoch]), Successor::None);
    assert_eq!(
        successor(parent, [first, first, wrong_epoch]),
        Successor::Unique(first)
    );
    assert_eq!(successor(parent, [first, second]), Successor::Conflict);
    assert_eq!(successor(parent, [second, first]), Successor::Conflict);
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
