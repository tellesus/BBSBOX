// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_discussions::{Boards, Change, Error, boards, posts, threads};
use bbsbox_event_codec::{self as codec, Distribution, Draft, Role as EventRole};
use bbsbox_event_core::{Delivery, Inbox, integration};
use bbsbox_identity::{Auth, Role, bootstrap_sysop, node::NodeSigner};
use bbsbox_storage::{Database, clock::SystemClock, disk::Class, open_reader};
use std::sync::Arc;
use zeroize::Zeroizing;

#[test]
fn local_board_authorization_signed_replies_and_fork_isolation() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("node.sqlite");
    let mut registry = bbsbox_identity::migrations();
    registry.extend(bbsbox_event_core::migrations());
    registry.extend(bbsbox_discussions::migrations());
    let mut database = Database::open(&path, &registry).unwrap();
    let node = NodeSigner::start(&mut database).unwrap();
    let auth = Auth::new(
        "https://localhost:8443",
        Arc::new(SystemClock::default()),
        Default::default(),
    )
    .unwrap();
    let sysop_password = "fixture sysop passphrase";
    let prepared = auth
        .passwords()
        .prepare(Zeroizing::new(sysop_password.into()))
        .unwrap();
    let sysop = database
        .write(Class::Control, 65536, |tx| {
            bootstrap_sysop(tx, "operator", prepared)
        })
        .unwrap();
    let reader = open_reader(&path).unwrap();
    let proof = auth
        .verify_password(
            &reader,
            "operator",
            Zeroizing::new(sysop_password.into()),
            "https://localhost:8443",
        )
        .unwrap();
    let login = database
        .write(Class::Control, 65536, |tx| auth.complete_login(tx, proof))
        .unwrap();
    let prepared = auth
        .passwords()
        .prepare(Zeroizing::new("fixture member passphrase".into()))
        .unwrap();
    let member = database
        .write(Class::Control, 65536, |tx| {
            let actor = auth.authorize_mutation(
                tx,
                login.session_token(),
                login.csrf_token(),
                "https://localhost:8443",
            )?;
            auth.create_account(tx, &actor, "neighbor", prepared, Role::Member)
        })
        .unwrap();
    let service = Boards::new(&node);
    let mut record = |account: &_, board: &str, change: Change<'_>, time| {
        database.write(Class::Content, 128 * 1024, |tx| {
            service.record(tx, &node, account, board, change, time)
        })
    };
    assert!(matches!(
        record(&member, "general", Change::Board { title: "General" }, 0),
        Err(Error::Forbidden)
    ));
    let general = record(&sysop, "general", Change::Board { title: "General" }, 1).unwrap();
    let other = record(&sysop, "other", Change::Board { title: "Other" }, 2).unwrap();
    let thread = record(
        &member,
        "general",
        Change::Thread {
            title: "Meetup",
            text: "At the library",
        },
        3,
    )
    .unwrap();
    assert!(matches!(
        record(
            &member,
            "other",
            Change::Reply {
                parent: thread,
                text: "Wrong board"
            },
            4
        ),
        Err(Error::MissingParent)
    ));
    let reply = record(
        &sysop,
        "general",
        Change::Reply {
            parent: thread,
            text: "What time?",
        },
        5,
    )
    .unwrap();
    let nested = record(
        &member,
        "general",
        Change::Reply {
            parent: reply,
            text: "Six",
        },
        6,
    )
    .unwrap();

    let visible = boards(&reader, None).unwrap();
    assert_eq!(
        visible
            .iter()
            .map(|board| board.slug.as_str())
            .collect::<Vec<_>>(),
        ["general", "other"]
    );
    assert_eq!(threads(&reader, "general", None).unwrap()[0].id, thread);
    assert!(
        threads(&reader, "general", Some((3, thread)))
            .unwrap()
            .is_empty()
    );
    let messages = posts(&reader, "general", thread, Some((3, thread))).unwrap();
    assert_eq!(
        messages
            .iter()
            .map(|post| (post.id, post.parent, post.username.as_str()))
            .collect::<Vec<_>>(),
        [
            (reply, Some(thread), "operator"),
            (nested, Some(reply), "neighbor")
        ]
    );
    assert!(matches!(
        posts(&reader, "general", nested, None),
        Err(Error::MissingParent)
    ));
    let no_credentials = |_: &rusqlite::Connection, _| Ok(None);
    let signed = integration::complete(&reader, reply, &no_credentials)
        .unwrap()
        .unwrap();
    let metadata = signed.metadata();
    assert_eq!(metadata.scope.distribution, Distribution::Local);
    assert_eq!(metadata.actor.role, EventRole::Node);
    assert_eq!(&metadata.actor.subject, node.id().bytes());
    assert!(signed.commitment().credential().is_none());
    assert_eq!(
        integration::receipt(&reader, reply)
            .unwrap()
            .unwrap()
            .module,
        "discussions"
    );

    assert!(matches!(
        record(
            &member,
            "general",
            Change::Edit {
                post: reply,
                previous: reply,
                text: "Someone else's post"
            },
            7
        ),
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        record(
            &sysop,
            "general",
            Change::Edit {
                post: reply,
                previous: reply,
                text: "What time?"
            },
            7
        ),
        Err(Error::NoChange)
    ));
    let edited = record(
        &sysop,
        "general",
        Change::Edit {
            post: reply,
            previous: reply,
            text: "What time on Tuesday?",
        },
        7,
    )
    .unwrap();
    let latest = posts(&reader, "general", thread, None).unwrap();
    assert_eq!(latest[1].id, reply);
    assert_eq!(latest[1].revision, edited);
    assert_eq!(latest[1].text, "What time on Tuesday?");
    assert!(matches!(
        record(
            &sysop,
            "general",
            Change::Withdraw {
                post: reply,
                previous: reply
            },
            8
        ),
        Err(Error::RevisionChanged)
    ));
    let withdrawn = record(
        &sysop,
        "general",
        Change::Withdraw {
            post: reply,
            previous: edited,
        },
        8,
    )
    .unwrap();
    let latest = posts(&reader, "general", thread, None).unwrap();
    assert!(latest[1].withdrawn && latest[1].text.is_empty());
    assert_eq!(latest[1].revision, withdrawn);
    assert_eq!(latest[2].parent, Some(reply));
    for (revision, text) in [
        (reply, Some("What time?")),
        (edited, Some("What time on Tuesday?")),
        (withdrawn, None),
    ] {
        assert_eq!(
            bbsbox_discussions::revision(&reader, "general", reply, revision)
                .unwrap()
                .text
                .as_deref(),
            text
        );
    }

    let prior_receipt = integration::receipt(&reader, edited).unwrap().unwrap();
    // Rebuild only this fixture's projection tables. The accepted journal and receipts remain intact.
    database.write::<_, bbsbox_event_core::Error>(Class::Content, 128 * 1024, |tx| {
        tx.execute_batch("DELETE FROM discussions_heads; DELETE FROM discussions_revisions; DELETE FROM discussions_entries;")?;
        assert!(matches!(integration::replay(tx, withdrawn, &service, &no_credentials)?, integration::Integration::Staged(_)));
        for id in [general, other, thread, reply, nested, edited, withdrawn] {
            assert_eq!(integration::replay(tx, id, &service, &no_credentials)?, integration::Integration::Replayed);
        }
        Ok(())
    }).unwrap();
    assert_eq!(posts(&reader, "general", thread, None).unwrap(), latest);
    assert_eq!(
        integration::receipt(&reader, edited).unwrap().unwrap(),
        prior_receipt
    );

    let fork = codec::sign(
        Draft {
            kind: metadata.kind.clone(),
            schema: metadata.schema,
            scope: metadata.scope.clone(),
            dependencies: metadata.dependencies.clone(),
            claimed_time: 7,
            extensions: vec![],
            sequence: metadata.sequence,
            previous: metadata.previous,
        },
        metadata.actor.clone(),
        signed.body(),
        None,
        |bytes| {
            node.sign_current(node.incarnation(), bytes)
                .map_err(|_| codec::Error::SignerUnavailable)
        },
    )
    .unwrap();
    database
        .write(Class::Content, 128 * 1024, |tx| {
            integration::receive(
                tx,
                Inbox::default(),
                fork.commitment().encoded(),
                Some(fork.body()),
                &Delivery {
                    source: "fixture".into(),
                    transfer: "fork".into(),
                },
                &no_credentials,
                &[&service],
            )
        })
        .unwrap();
    assert_eq!(boards(&reader, None).unwrap()[0].slug, "other");
    assert!(matches!(
        threads(&reader, "general", None),
        Err(Error::MissingBoard)
    ));
    database
        .write(Class::Content, 128 * 1024, |tx| {
            service.record(
                tx,
                &node,
                &member,
                "other",
                Change::Thread {
                    title: "Still available",
                    text: "Another scope",
                },
                8,
            )
        })
        .unwrap();
    assert!(integration::receipt(&reader, reply).unwrap().is_some());
}
