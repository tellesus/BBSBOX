// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_client_identity::{ActiveDevice, DeviceKey, RootKey};
use bbsbox_credentials::{CredentialId, ForumId, Purpose, Purposes};
use bbsbox_event_codec::{
    self as events, Actor, CompleteEvent, Dependency, Distribution, Draft, EventId, Role, Scope,
};
use bbsbox_private_forum::{AUDIENCE, Action, Error, Header, Outer};
use sha2::{Digest, Sha256};

fn id(value: u8) -> EventId {
    EventId::from_bytes([value; 32])
}

fn header(action: Action) -> Header {
    Header {
        epoch: 7,
        control: id(8),
        action_id: [9; 16],
        action,
    }
}

fn draft(header: Header) -> Draft {
    Draft {
        kind: header.action.kind().into(),
        schema: 1,
        scope: Scope {
            resource: ForumId::from_digest([6; 32]).to_string(),
            audience: AUDIENCE.into(),
            distribution: Distribution::Shared,
        },
        sequence: 2,
        previous: Some(id(12)),
        dependencies: header.dependencies(),
        claimed_time: 0,
        extensions: vec![header.extension().unwrap()],
    }
}

#[test]
fn authenticated_data_has_a_fixed_independently_generated_vector() {
    let actor = Actor {
        role: Role::PersonDevice,
        subject: [2; 32],
        key: [3; 32],
        credential: Some(CredentialId::from_bytes([4; 32])),
        incarnation: [5; 16],
    };
    let edit = header(Action::Edit {
        post: id(10),
        prior: id(11),
    });
    let outer = Outer::before_encryption(&draft(edit), &actor).unwrap();
    assert_eq!(outer.authenticated_data.len(), 327);
    assert_eq!(
        hex::encode(Sha256::digest(outer.authenticated_data)),
        "0edff25433365408f722aa2221d38deaa6121ac9d57f0cea78831b85e3e5a9da"
    );
}

fn device(purpose: Purpose) -> ActiveDevice {
    let root = RootKey::generate();
    let device = DeviceKey::generate();
    let delegation = root
        .issue_device(device.public(), Purposes::new(&[purpose]).unwrap())
        .unwrap();
    device.activate(root.person_id(), delegation).unwrap()
}

fn sign(device: &ActiveDevice, purpose: Purpose, draft: Draft, body: &[u8]) -> CompleteEvent {
    events::sign(
        draft,
        Actor::device(device.credential()),
        body,
        Some(device.credential()),
        |message| {
            device
                .sign_for(purpose, message)
                .map_err(|_| events::Error::SignerUnavailable)
        },
    )
    .unwrap()
}

#[test]
fn signed_metadata_binds_epoch_author_route_and_revision_to_the_opaque_body() {
    let author = device(Purpose::Authorship);
    let actor = Actor::device(author.credential());
    let body = b"opaque fixture bytes";
    for action in [
        Action::Post,
        Action::Reply { parent: id(10) },
        Action::Edit {
            post: id(10),
            prior: id(10),
        },
        Action::Withdraw {
            post: id(10),
            prior: id(11),
        },
    ] {
        let header = header(action);
        let draft = draft(header);
        let prepared = Outer::before_encryption(&draft, &actor).unwrap();
        let signed = sign(&author, Purpose::Authorship, draft.clone(), body);
        let received = Outer::inspect(&signed).unwrap();
        assert_eq!(received.authenticated_data, prepared.authenticated_data);
        assert_eq!(received.header, header);
        assert_eq!(signed.body(), body);
        let mut changed = draft;
        changed.scope.distribution = Distribution::Local;
        let rewrapped = sign(&author, Purpose::Authorship, changed, body);
        assert_ne!(
            Outer::inspect(&rewrapped).unwrap().authenticated_data,
            received.authenticated_data
        );
    }
}

#[test]
fn rejects_malformed_or_missing_authority_inputs_before_decrypting() {
    let author = device(Purpose::Authorship);
    let actor = Actor::device(author.credential());
    let header = header(Action::Reply { parent: id(10) });
    let mut invalid = draft(header);
    invalid.extensions[0].required = false;
    assert!(matches!(
        Outer::before_encryption(&invalid, &actor),
        Err(Error::InvalidHeader)
    ));
    let mut invalid = draft(header);
    invalid.dependencies = vec![Dependency::event(header.control)];
    assert!(matches!(
        Outer::before_encryption(&invalid, &actor),
        Err(Error::InvalidDependencies)
    ));
    let mut invalid = draft(header);
    invalid.scope.audience = "public".into();
    assert!(matches!(
        Outer::before_encryption(&invalid, &actor),
        Err(Error::InvalidScope)
    ));
    let governance = device(Purpose::ForumGovernance);
    let invalid = sign(
        &governance,
        Purpose::ForumGovernance,
        draft(header),
        b"opaque",
    );
    assert!(matches!(
        Outer::inspect(&invalid),
        Err(Error::InvalidAuthor)
    ));
}
