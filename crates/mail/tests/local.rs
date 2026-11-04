// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_event_codec::{Distribution, Role as EventRole};
use bbsbox_event_core::integration;
use bbsbox_identity::{Auth, Role, bootstrap_sysop, node::NodeSigner};
use bbsbox_mail::{Error, Mailbox, Message, conversations, messages};
use bbsbox_storage::{Database, clock::SystemClock, disk::Class, open_reader};
use std::sync::Arc;
use zeroize::Zeroizing;

#[test]
fn participant_access_reply_operator_contact_and_local_scope() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("mail.sqlite");
    let mut registry = bbsbox_identity::migrations();
    registry.extend(bbsbox_event_core::migrations());
    registry.extend(bbsbox_mail::migrations());
    let mut database = Database::open(&path, &registry).unwrap();
    let node = NodeSigner::start(&mut database).unwrap();
    let auth = Auth::new(
        "https://localhost:8443",
        Arc::new(SystemClock::default()),
        Default::default(),
    )
    .unwrap();
    let password = "mail fixture password";
    let prepared = auth
        .passwords()
        .prepare(Zeroizing::new(password.into()))
        .unwrap();
    let operator = database
        .write(Class::Control, 65536, |tx| {
            bootstrap_sysop(tx, "operator", prepared)
        })
        .unwrap();
    let reader = open_reader(&path).unwrap();
    let proof = auth
        .verify_password(
            &reader,
            "operator",
            Zeroizing::new(password.into()),
            "https://localhost:8443",
        )
        .unwrap();
    let login = database
        .write(Class::Control, 65536, |tx| auth.complete_login(tx, proof))
        .unwrap();
    let [alice, bob] = ["alice", "bob"].map(|name| {
        let prepared = auth
            .passwords()
            .prepare(Zeroizing::new(password.into()))
            .unwrap();
        database
            .write(Class::Control, 65536, |tx| {
                let actor = auth.authorize_mutation(
                    tx,
                    login.session_token(),
                    login.csrf_token(),
                    "https://localhost:8443",
                )?;
                auth.create_account(tx, &actor, name, prepared, Role::Member)
            })
            .unwrap()
    });
    let mailbox = Mailbox::new(&node);
    let mut send = |sender, message, time| {
        database.write(Class::Content, 128 * 1024, |tx| {
            mailbox.send(tx, &node, sender, message, time)
        })
    };
    let (thread, original) = send(
        alice.id,
        Message::Start {
            recipient: "Bob",
            operator_only: false,
            subject: "Library key",
            text: "I can bring the key.",
        },
        1,
    )
    .unwrap();
    assert!(
        conversations(&reader, operator.id, None)
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        messages(&reader, operator.id, thread, None),
        Err(Error::Missing)
    ));
    assert!(matches!(
        send(
            operator.id,
            Message::Reply {
                thread,
                parent: original,
                text: "I should be refused"
            },
            2
        ),
        Err(Error::Missing)
    ));
    assert!(matches!(
        send(
            alice.id,
            Message::Start {
                recipient: "bob",
                operator_only: true,
                subject: "Question",
                text: "Operator only"
            },
            2
        ),
        Err(Error::Missing)
    ));
    let (_, reply) = send(
        bob.id,
        Message::Reply {
            thread,
            parent: original,
            text: "See you there.",
        },
        3,
    )
    .unwrap();
    let (box_entry, visible) = messages(&reader, alice.id, thread, None).unwrap();
    assert_eq!(box_entry.correspondent, "bob");
    assert_eq!(
        visible
            .iter()
            .map(|message| message.sender.as_str())
            .collect::<Vec<_>>(),
        ["alice", "bob"]
    );
    assert_eq!(visible[1].parent, Some(original));
    assert_eq!(
        messages(&reader, bob.id, thread, Some((1, original)))
            .unwrap()
            .1[0]
            .id,
        reply
    );
    let (contact, _) = send(
        bob.id,
        Message::Start {
            recipient: "operator",
            operator_only: true,
            subject: "Node question",
            text: "When is the node open?",
        },
        4,
    )
    .unwrap();
    assert!(matches!(
        send(
            bob.id,
            Message::Reply {
                thread: contact,
                parent: original,
                text: "Wrong conversation"
            },
            5
        ),
        Err(Error::Missing)
    ));
    assert_eq!(
        conversations(&reader, operator.id, None).unwrap()[0].id,
        contact
    );
    assert_eq!(
        bbsbox_identity::operator_usernames(&reader).unwrap(),
        ["operator"]
    );
    let no_credentials = |_: &rusqlite::Connection, _| Ok(None);
    let signed = integration::complete(&reader, reply, &no_credentials)
        .unwrap()
        .unwrap();
    assert_eq!(signed.metadata().scope.distribution, Distribution::Local);
    assert_eq!(signed.metadata().scope.audience, "mail-participants");
    assert_eq!(signed.metadata().actor.role, EventRole::Node);
    assert_eq!(
        integration::receipt(&reader, reply)
            .unwrap()
            .unwrap()
            .module,
        "mail"
    );
}
