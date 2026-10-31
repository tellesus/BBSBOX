// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_client_identity::{ActiveDevice, DeviceKey, RootKey};
use bbsbox_credentials::{Purpose, Purposes};
use bbsbox_event_codec::{
    authority::{Authority, Decision, KindRule, Outcome, Registry},
    *,
};
use ciborium::value::Value;
use coset::{CborSerializable, CoseSign1, CoseSign1Builder, HeaderBuilder};

fn draft() -> Draft {
    Draft {
        kind: "fixture.post".into(),
        schema: 1,
        scope: Scope {
            resource: "forum:fixture".into(),
            audience: "public".into(),
            distribution: Distribution::Shared,
        },
        sequence: 1,
        previous: None,
        dependencies: vec![],
        claimed_time: -1000,
        extensions: vec![],
    }
}
fn device() -> ActiveDevice {
    let root = RootKey::generate();
    let key = DeviceKey::generate();
    let credential = root
        .issue_device(key.public(), Purposes::new(&[Purpose::Authorship]).unwrap())
        .unwrap();
    key.activate(root.person_id(), credential).unwrap()
}
fn signed(device: &ActiveDevice, draft: Draft) -> CompleteEvent {
    sign(
        draft,
        Actor::device(device.credential()),
        b"hello",
        Some(device.credential()),
        |message| {
            device
                .sign_for(Purpose::Authorship, message)
                .map_err(|_| Error::SignerUnavailable)
        },
    )
    .unwrap()
}
fn resign(
    event: &CompleteEvent,
    device: &ActiveDevice,
    change: impl FnOnce(&mut Value),
) -> Vec<u8> {
    let cose = CoseSign1::from_slice(event.commitment().encoded()).unwrap();
    let mut payload: Value =
        ciborium::de::from_reader(cose.payload.as_ref().unwrap().as_slice()).unwrap();
    change(&mut payload);
    let mut bytes = Vec::new();
    ciborium::ser::into_writer(&payload, &mut bytes).unwrap();
    CoseSign1Builder::new()
        .protected(cose.protected.header)
        .payload(bytes)
        .create_signature(DOMAIN.as_bytes(), |message| {
            device
                .sign_for(Purpose::Authorship, message)
                .unwrap()
                .to_vec()
        })
        .build()
        .to_vec()
        .unwrap()
}
fn registry(extensions: Vec<u16>) -> Registry {
    Registry::new(
        vec![
            KindRule {
                kind: "fixture.post".into(),
                schema: 1,
                actor: Role::PersonDevice,
                purpose: Some(Purpose::Authorship),
            },
            KindRule {
                kind: "fixture.control".into(),
                schema: 1,
                actor: Role::PersonDevice,
                purpose: Some(Purpose::ForumGovernance),
            },
        ],
        extensions,
    )
    .unwrap()
}
struct Policy {
    member: bool,
    dependency: Option<EventId>,
}
impl Authority for Policy {
    fn action(&self, _: &CompleteEvent) -> Decision {
        if let Some(id) = self.dependency {
            Decision::Missing(vec![Dependency::event(id)])
        } else if self.member {
            Decision::Allow
        } else {
            Decision::Deny("not_a_member")
        }
    }
    fn extension(&self, _: &CompleteEvent, extension: &Extension) -> Decision {
        if extension.data == b"valid-proof" {
            Decision::Allow
        } else {
            Decision::Deny("invalid_extension_proof")
        }
    }
}
#[test]
fn signatures_body_integrity_and_current_resource_authority_are_distinct() {
    let key = device();
    let event = signed(&key, draft());
    let verified = verify(event.commitment().encoded(), |_| {
        Some(key.credential().clone())
    })
    .unwrap();
    assert_eq!(verified.id(), event.id());
    assert!(matches!(
        verified.clone().with_body(b"other"),
        Err(Error::BodyMismatch)
    ));
    let complete = verified.with_body(b"hello").unwrap();
    assert_eq!(
        registry(vec![]).authorize(
            &complete,
            &Policy {
                member: false,
                dependency: None
            }
        ),
        Outcome::Denied("not_a_member")
    );
    assert_eq!(
        registry(vec![]).authorize(
            &complete,
            &Policy {
                member: true,
                dependency: None
            }
        ),
        Outcome::Authorized
    );
    let missing = EventId::from_bytes([4; 32]);
    assert_eq!(
        registry(vec![]).authorize(
            &complete,
            &Policy {
                member: true,
                dependency: Some(missing)
            }
        ),
        Outcome::Missing(vec![Dependency::event(missing)])
    );
}
#[test]
fn unknown_or_mismatched_device_delegations_never_authenticate_an_author() {
    let key = device();
    let other = device();
    let event = signed(&key, draft());
    assert!(matches!(
        verify(event.commitment().encoded(), |_| None),
        Err(Error::MissingCredential(_))
    ));
    assert!(matches!(
        verify(event.commitment().encoded(), |_| Some(
            other.credential().clone()
        )),
        Err(Error::CredentialMismatch)
    ));
    let changed = resign(&event, &key, |v| {
        v.as_array_mut().unwrap()[5].as_array_mut().unwrap()[4] = Value::Bytes(vec![7; 16])
    });
    assert!(matches!(
        verify(&changed, |_| Some(key.credential().clone())),
        Err(Error::WrongStream)
    ));
}
#[test]
fn resource_audience_distribution_and_incarnation_changes_start_independent_streams() {
    let key = device();
    let public = signed(&key, draft());
    let mut private = draft();
    private.scope.audience = "private:group".into();
    private.scope.distribution = Distribution::Local;
    let private = signed(&key, private);
    assert_ne!(public.metadata().stream, private.metadata().stream);
    assert_eq!(public.metadata().sequence, 1);
    assert_eq!(public.metadata().previous, None);
    let mut resource = draft();
    resource.scope.resource = "forum:other".into();
    assert_ne!(
        public.metadata().stream,
        signed(&key, resource).metadata().stream
    );
    let mut distribution = draft();
    distribution.scope.distribution = Distribution::Local;
    assert_ne!(
        public.metadata().stream,
        signed(&key, distribution).metadata().stream
    );
    let other = device();
    assert_ne!(
        public.metadata().stream,
        signed(&other, draft()).metadata().stream
    );
}
#[test]
fn root_node_and_device_signatures_cannot_bypass_kind_role_or_purpose_rules() {
    let key = device();
    let mut control = draft();
    control.kind = "fixture.control".into();
    let event = signed(&key, control);
    assert_eq!(
        registry(vec![]).authorize(
            &event,
            &Policy {
                member: true,
                dependency: None
            }
        ),
        Outcome::Denied("purpose_not_delegated")
    );
    let root = RootKey::generate();
    let actor = Actor::root(Role::PersonRoot, root.public(), *root.control_incarnation()).unwrap();
    let event = sign(draft(), actor, b"hello", None, |message| {
        root.sign_current_control(root.control_incarnation(), message)
            .map_err(|_| Error::SignerUnavailable)
    })
    .unwrap();
    assert_eq!(
        registry(vec![]).authorize(
            &event,
            &Policy {
                member: true,
                dependency: None
            }
        ),
        Outcome::Denied("wrong_actor_role")
    );
    let dir = tempfile::tempdir().unwrap();
    let mut db = bbsbox_storage::Database::open(
        dir.path().join("node.sqlite"),
        &bbsbox_identity::migrations(),
    )
    .unwrap();
    let node = bbsbox_identity::node::NodeSigner::start(&mut db).unwrap();
    let actor = Actor::root(Role::Node, node.public(), *node.incarnation()).unwrap();
    let event = sign(draft(), actor, b"hello", None, |message| {
        node.sign_current(node.incarnation(), message)
            .map_err(|_| Error::SignerUnavailable)
    })
    .unwrap();
    assert_eq!(
        registry(vec![]).authorize(
            &event,
            &Policy {
                member: true,
                dependency: None
            }
        ),
        Outcome::Denied("wrong_actor_role")
    );
}
#[test]
fn optional_annotations_never_grant_authority_and_required_proofs_cannot_be_downgraded() {
    let key = device();
    let mut input = draft();
    input.extensions = vec![Extension {
        id: 7,
        required: false,
        data: b"grant-all".to_vec(),
    }];
    let optional = signed(&key, input.clone());
    assert_eq!(
        registry(vec![]).authorize(
            &optional,
            &Policy {
                member: false,
                dependency: None
            }
        ),
        Outcome::Denied("not_a_member")
    );
    input.extensions[0].required = true;
    let required = signed(&key, input.clone());
    assert_eq!(
        registry(vec![]).authorize(
            &required,
            &Policy {
                member: true,
                dependency: None
            }
        ),
        Outcome::UnsupportedExtension(7)
    );
    assert_eq!(
        registry(vec![7]).authorize(
            &required,
            &Policy {
                member: true,
                dependency: None
            }
        ),
        Outcome::Denied("invalid_extension_proof")
    );
    assert_eq!(
        registry(vec![7]).authorize(
            &optional,
            &Policy {
                member: true,
                dependency: None
            }
        ),
        Outcome::Denied("required_extension_downgraded")
    );
    input.extensions[0].data = b"valid-proof".to_vec();
    let valid = signed(&key, input);
    assert_eq!(
        registry(vec![7]).authorize(
            &valid,
            &Policy {
                member: false,
                dependency: None
            }
        ),
        Outcome::Denied("not_a_member")
    );
    assert_eq!(
        registry(vec![7]).authorize(
            &valid,
            &Policy {
                member: true,
                dependency: None
            }
        ),
        Outcome::Authorized
    );
}
#[test]
fn unknown_kind_schema_minor_and_major_are_explicit_and_never_authorized() {
    let key = device();
    let mut input = draft();
    input.kind = "unknown.kind".into();
    assert_eq!(
        registry(vec![]).authorize(
            &signed(&key, input),
            &Policy {
                member: true,
                dependency: None
            }
        ),
        Outcome::UnsupportedKind
    );
    let mut input = draft();
    input.schema = 2;
    assert_eq!(
        registry(vec![]).authorize(
            &signed(&key, input),
            &Policy {
                member: true,
                dependency: None
            }
        ),
        Outcome::UnsupportedSchema(2)
    );
    let event = signed(&key, draft());
    let minor = resign(&event, &key, |v| {
        v.as_array_mut().unwrap()[2] = Value::Integer(1.into())
    });
    let minor = verify(&minor, |_| Some(key.credential().clone()))
        .unwrap()
        .with_body(b"hello")
        .unwrap();
    assert_eq!(
        registry(vec![]).authorize(
            &minor,
            &Policy {
                member: true,
                dependency: None
            }
        ),
        Outcome::UnsupportedMinor(1)
    );
    let major = resign(&event, &key, |v| {
        v.as_array_mut().unwrap()[1] = Value::Integer(2.into())
    });
    assert!(matches!(
        verify(&major, |_| Some(key.credential().clone())),
        Err(Error::UnsupportedMajor(2))
    ));
    let changed = resign(&event, &key, |value| {
        value.as_array_mut().unwrap()[2] = Value::Integer(1.into());
        value.as_array_mut().unwrap().push(Value::Null);
    });
    assert!(matches!(
        verify(&changed, |_| Some(key.credential().clone())),
        Err(Error::UnsupportedMinor(1))
    ));
    let changed = resign(&event, &key, |value| {
        value.as_array_mut().unwrap()[0] = Value::Text("bbsbox.event/v2".into())
    });
    assert!(matches!(
        verify(&changed, |_| Some(key.credential().clone())),
        Err(Error::UnsupportedDomain)
    ));
}
#[test]
fn malformed_noncanonical_and_tampered_bytes_are_rejected() {
    let key = device();
    let event = signed(&key, draft());
    let bytes = event.commitment().encoded();
    for position in (0..bytes.len()).step_by(7) {
        let mut changed = bytes.to_vec();
        changed[position] ^= 1;
        assert!(
            verify(&changed, |_| Some(key.credential().clone())).is_err(),
            "changed byte {position}"
        );
    }
    let mut cose = CoseSign1::from_slice(bytes).unwrap();
    cose.unprotected = HeaderBuilder::new().key_id(vec![1]).build();
    assert!(matches!(
        verify(&cose.to_vec().unwrap(), |_| Some(key.credential().clone())),
        Err(Error::NonCanonical)
    ));
    let mut extra = bytes.to_vec();
    extra.push(0);
    assert!(verify(&extra, |_| Some(key.credential().clone())).is_err());
    let mut cose = CoseSign1::from_slice(bytes).unwrap();
    let body = cose.payload.as_mut().unwrap();
    let offset = 1 + 1 + DOMAIN.len();
    assert_eq!(body[offset], 1);
    body.splice(offset..=offset, [0x18, 1]);
    assert!(matches!(
        verify(&cose.to_vec().unwrap(), |_| Some(key.credential().clone())),
        Err(Error::NonCanonical)
    ));
}
#[test]
fn size_sequence_dependency_and_extension_bounds_fail_before_signing() {
    let key = device();
    let actor = Actor::device(key.credential());
    let signing = |_: &[u8]| -> Result<[u8; 64]> { panic!("invalid input must not reach signer") };
    assert!(matches!(
        sign(
            draft(),
            actor.clone(),
            &vec![0; MAX_BODY + 1],
            Some(key.credential()),
            signing
        ),
        Err(Error::BodyTooLarge)
    ));
    let mut input = draft();
    input.sequence = 2;
    assert!(matches!(
        sign(input, actor.clone(), b"", Some(key.credential()), signing),
        Err(Error::InvalidSequence)
    ));
    let mut input = draft();
    input.dependencies = vec![Dependency::event(EventId::from_bytes([1; 32])); 2];
    assert!(matches!(
        sign(input, actor.clone(), b"", Some(key.credential()), signing),
        Err(Error::InvalidField)
    ));
    let mut input = draft();
    input.dependencies = (0..33)
        .map(|i| Dependency::event(EventId::from_bytes([i; 32])))
        .collect();
    assert!(matches!(
        sign(input, actor.clone(), b"", Some(key.credential()), signing),
        Err(Error::InvalidField)
    ));
    let mut input = draft();
    input.extensions = vec![Extension {
        id: 1,
        required: false,
        data: vec![0; 1025],
    }];
    assert!(matches!(
        sign(input, actor.clone(), b"", Some(key.credential()), signing),
        Err(Error::InvalidField)
    ));
    let mut input = draft();
    input.extensions = (1..=4)
        .map(|id| Extension {
            id,
            required: false,
            data: vec![0; 1024],
        })
        .collect();
    assert!(matches!(
        sign(input, actor, b"", Some(key.credential()), signing),
        Err(Error::EnvelopeTooLarge)
    ));
}
#[test]
fn detached_commitment_proves_chain_bytes_without_claiming_body_or_permission() {
    let key = device();
    let first = signed(&key, draft());
    let commitment = verify(first.commitment().encoded(), |_| {
        Some(key.credential().clone())
    })
    .unwrap();
    let mut next = draft();
    next.sequence = 2;
    next.previous = Some(commitment.id());
    let next = signed(&key, next);
    assert_eq!(next.metadata().previous, Some(first.id()));
    assert!(commitment.with_body(b"suppressed or wrong bytes").is_err());
}
#[test]
fn invalid_or_duplicate_kind_rules_fail_and_missing_extension_handlers_deny() {
    let rule = || KindRule {
        kind: "fixture.post".into(),
        schema: 1,
        actor: Role::PersonDevice,
        purpose: Some(Purpose::Authorship),
    };
    assert!(Registry::new(vec![rule(), rule()], vec![]).is_err());
    assert!(
        Registry::new(
            vec![KindRule {
                kind: "fixture.post".into(),
                schema: 1,
                actor: Role::Node,
                purpose: Some(Purpose::Authorship)
            }],
            vec![]
        )
        .is_err()
    );
    let key = device();
    let mut input = draft();
    input.extensions = vec![Extension {
        id: 7,
        required: true,
        data: b"valid-proof".to_vec(),
    }];
    let event = signed(&key, input);
    struct NoExtension;
    impl Authority for NoExtension {
        fn action(&self, _: &CompleteEvent) -> Decision {
            Decision::Allow
        }
    }
    assert_eq!(
        registry(vec![7]).authorize(&event, &NoExtension),
        Outcome::Denied("extension_handler_missing")
    );
}

#[test]
fn resolver_failure_is_not_mislabeled_as_a_missing_dependency() {
    let key = device();
    let event = signed(&key, draft());
    assert!(matches!(
        verify_with_resolver(event.commitment().encoded(), |_| Err(
            Error::ResolverUnavailable
        )),
        Err(Error::ResolverUnavailable)
    ));
}

#[test]
fn staging_can_check_signed_body_bytes_before_the_device_delegation_arrives() {
    let key = device();
    let event = signed(&key, draft());
    let pending = verify_signature(event.commitment().encoded()).unwrap();
    pending.check_body(b"hello").unwrap();
    assert!(matches!(
        pending.check_body(b"wrong"),
        Err(Error::BodyMismatch)
    ));
    assert!(matches!(
        pending.clone().bind(|_| Ok(None)),
        Err(Error::MissingCredential(_))
    ));
    let complete = pending
        .bind(|_| Ok(Some(key.credential().clone())))
        .unwrap()
        .with_body(b"hello")
        .unwrap();
    assert_eq!(complete.id(), event.id());
}

#[test]
fn fixed_vector_round_trips_exact_numeric_boundaries_and_canonical_event_bytes() {
    use ed25519_dalek::Signer;
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../protocol/fixtures/event-v1.json")).unwrap();
    let credential: serde_json::Value = serde_json::from_str(include_str!(
        "../../credentials/tests/fixtures/device-v2.json"
    ))
    .unwrap();
    let credential = bbsbox_credentials::Credential::verify(
        &hex::decode(credential["cose_hex"].as_str().unwrap()).unwrap(),
    )
    .unwrap();
    let encoded = hex::decode(fixture["cose_hex"].as_str().unwrap()).unwrap();
    let body = hex::decode(fixture["body_hex"].as_str().unwrap()).unwrap();
    let complete = verify(&encoded, |_| Some(credential.clone()))
        .unwrap()
        .with_body(&body)
        .unwrap();
    assert_eq!(complete.metadata().sequence, u64::MAX);
    assert_eq!(complete.metadata().claimed_time, i64::MIN);
    assert_eq!(complete.id().to_string(), fixture["event_id"]);
    let m = complete.metadata();
    let draft = Draft {
        kind: m.kind.clone(),
        schema: m.schema,
        scope: m.scope.clone(),
        sequence: m.sequence,
        previous: m.previous,
        dependencies: m.dependencies.clone(),
        claimed_time: m.claimed_time,
        extensions: m.extensions.clone(),
    };
    let key = ed25519_dalek::SigningKey::from_bytes(&[9; 32]);
    let recreated = sign(
        draft,
        Actor::device(&credential),
        &body,
        Some(&credential),
        |message| Ok(key.sign(message).to_bytes()),
    )
    .unwrap();
    assert_eq!(recreated.commitment().encoded(), encoded);
}

#[test]
fn dependency_namespaces_do_not_alias_the_same_digest() {
    let key = device();
    let digest = [8; 32];
    let mut input = draft();
    input.dependencies = vec![
        Dependency::event(EventId::from_bytes(digest)),
        Dependency::credential(bbsbox_credentials::CredentialId::from_bytes(digest)),
        Dependency::blob(digest),
    ];
    let event = signed(&key, input);
    assert_eq!(event.metadata().dependencies.len(), 3);
    let changed = resign(&event, &key, |value| {
        value.as_array_mut().unwrap()[12].as_array_mut().unwrap()[0]
            .as_array_mut()
            .unwrap()[0] = Value::Integer(4.into())
    });
    assert!(matches!(
        verify(&changed, |_| Some(key.credential().clone())),
        Err(Error::UnsupportedDependency(4))
    ));
}
#[test]
fn oversized_deep_or_false_length_cbor_is_rejected_without_a_resolver_call() {
    let resolve = |_| -> Option<bbsbox_credentials::Credential> {
        panic!("malformed input cannot reach resolver")
    };
    assert!(matches!(
        verify(&vec![0; MAX_ENVELOPE + 1], resolve),
        Err(Error::EnvelopeTooLarge)
    ));
    for head in [0x9b, 0x5b] {
        let mut bytes = vec![head];
        bytes.extend_from_slice(&u64::MAX.to_be_bytes());
        assert!(verify(&bytes, resolve).is_err());
    }
    let mut bytes = vec![0x81; 300];
    bytes.push(0);
    assert!(verify(&bytes, resolve).is_err());
}
