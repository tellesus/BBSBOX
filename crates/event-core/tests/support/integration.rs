// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use bbsbox_event_codec::{
    Dependency, DependencyKind,
    authority::{Decision, KindRule, Registry},
};
use bbsbox_event_core::authoring::{self, Action, Authored};
use bbsbox_event_core::integration::{self as integration, Integration, Module, Relevance};
use rusqlite::{Transaction, params};

struct Board {
    registry: Registry,
    member: bool,
    fail_reduce: bool,
    fail_relevant: bool,
    fail_invalidation: bool,
    cross_control: bool,
}
impl Board {
    fn new(f: &mut Fixture) -> Self {
        f.db.write::<_,core::Error>(Class::Control,8192,|tx|{
            tx.execute_batch("CREATE TABLE IF NOT EXISTS fixture_effects(event BLOB PRIMARY KEY,body BLOB NOT NULL,valid INTEGER NOT NULL DEFAULT 1);
            CREATE TABLE IF NOT EXISTS fixture_dependencies(event BLOB NOT NULL,target BLOB NOT NULL,PRIMARY KEY(event,target));
            CREATE TABLE IF NOT EXISTS fixture_credits(event BLOB PRIMARY KEY,source TEXT NOT NULL,transfer TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS fixture_blobs(id BLOB PRIMARY KEY,visible INTEGER NOT NULL);")?;Ok(())
        }).unwrap();
        Self {
            registry: Registry::new(
                vec![
                    KindRule {
                        kind: "fixture.post".into(),
                        schema: 1,
                        actor: codec::Role::PersonDevice,
                        purpose: Some(Purpose::Authorship),
                    },
                    KindRule {
                        kind: "fixture.control".into(),
                        schema: 1,
                        actor: codec::Role::PersonDevice,
                        purpose: Some(Purpose::Authorship),
                    },
                ],
                vec![],
            )
            .unwrap(),
            member: true,
            fail_reduce: false,
            fail_relevant: false,
            fail_invalidation: false,
            cross_control: false,
        }
    }
}
impl Module for Board {
    fn id(&self) -> &str {
        "fixture-board"
    }
    fn registry(&self) -> &Registry {
        &self.registry
    }
    fn action(&self, _: &Connection, _: &CompleteEvent) -> Decision {
        if self.member {
            Decision::Allow
        } else {
            Decision::Deny("not_a_member")
        }
    }
    fn dependency(
        &self,
        connection: &Connection,
        _: &CompleteEvent,
        dependency: Dependency,
    ) -> Decision {
        if dependency.kind == DependencyKind::Blob {
            let present: bool = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM fixture_blobs WHERE id=?1 AND visible=1)",
                    [dependency.digest.as_slice()],
                    |r| r.get(0),
                )
                .unwrap();
            if !present {
                return Decision::Missing(vec![dependency]);
            }
        }
        Decision::Allow
    }
    fn cross_scope_control(
        &self,
        _: &Connection,
        _: &CompleteEvent,
        proof: &codec::Commitment,
    ) -> bool {
        self.cross_control && proof.metadata().kind == "fixture.control"
    }
    fn dependency_effective(&self, connection: &Connection, id: EventId) -> core::Result<bool> {
        Ok(connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM fixture_effects WHERE event=?1 AND valid=1)",
            [id.bytes().as_slice()],
            |r| r.get(0),
        )?)
    }
    fn reduce(&self, tx: &Transaction<'_>, event: &CompleteEvent) -> core::Result<Relevance> {
        tx.execute(
            "INSERT INTO fixture_effects(event,body) VALUES(?1,?2)",
            params![event.id().bytes().as_slice(), event.body()],
        )?;
        for dep in &event.metadata().dependencies {
            if dep.kind == DependencyKind::Event {
                tx.execute(
                    "INSERT INTO fixture_dependencies VALUES(?1,?2)",
                    params![event.id().bytes().as_slice(), dep.digest.as_slice()],
                )?;
            }
        }
        if self.fail_reduce {
            return Err(core::Error::CorruptRecord);
        }
        Ok(match event.body() {
            b"obsolete" => Relevance::Obsolete,
            b"dependency" => Relevance::DependencyOnly,
            _ => Relevance::Relevant,
        })
    }
    fn relevant(
        &self,
        tx: &Transaction<'_>,
        event: &CompleteEvent,
        delivery: &Delivery,
    ) -> core::Result<()> {
        tx.execute(
            "INSERT INTO fixture_credits VALUES(?1,?2,?3)",
            params![
                event.id().bytes().as_slice(),
                delivery.source,
                delivery.transfer
            ],
        )?;
        if self.fail_relevant {
            return Err(core::Error::CorruptRecord);
        }
        Ok(())
    }
    fn invalidate_fork(&self, tx: &Transaction<'_>, fork: &core::Fork) -> core::Result<()> {
        tx.execute("WITH RECURSIVE bad(id) AS (SELECT id FROM events_links WHERE stream=?1 AND sequence>=?2 UNION SELECT d.event FROM fixture_dependencies d JOIN bad b ON b.id=d.target) UPDATE fixture_effects SET valid=0 WHERE event IN (SELECT id FROM bad)",params![fork.stream.as_slice(),fork.first_sequence.to_be_bytes().as_slice()])?;
        if self.fail_invalidation {
            return Err(core::Error::CorruptRecord);
        }
        Ok(())
    }
}
fn apply(f: &mut Fixture, module: &Board, id: EventId) -> core::Result<Integration> {
    f.db.write(Class::Content, 128 * 1024, |tx| {
        integration::integrate(tx, id, module, &resolve)
    })
}
fn receive(
    f: &mut Fixture,
    module: &Board,
    event: &CompleteEvent,
    source: &str,
) -> core::Result<core::Intake> {
    f.db.write(Class::Content, 128 * 1024, |tx| {
        integration::receive(
            tx,
            Inbox::default(),
            event.commitment().encoded(),
            Some(event.body()),
            &delivery(source),
            &resolve,
            &[module],
        )
    })
}
fn count(f: &Fixture, table: &str) -> i64 {
    let query = match table {
        "effects" => "SELECT count(*) FROM fixture_effects WHERE valid=1",
        "credits" => "SELECT count(*) FROM fixture_credits",
        "receipts" => "SELECT count(*) FROM events_receipts",
        _ => panic!("unknown fixture"),
    };
    open_reader(&f.path)
        .unwrap()
        .query_row(query, [], |r| r.get(0))
        .unwrap()
}
fn with_dependencies(
    device: &ActiveDevice,
    resource: &str,
    kind: &str,
    dependencies: Vec<Dependency>,
    body: &[u8],
) -> CompleteEvent {
    let base = event(device, resource, 1, None, body);
    let mut deps = dependencies;
    deps.sort();
    let bytes = rewrite(&base, device, |fields| {
        fields[3] = Value::Text(kind.into());
        fields[12] = Value::Array(
            deps.iter()
                .map(|d| {
                    Value::Array(vec![
                        Value::Integer((d.kind as u64).into()),
                        Value::Bytes(d.digest.to_vec()),
                    ])
                })
                .collect(),
        );
    });
    codec::verify(&bytes, |_| Some(device.credential().clone()))
        .unwrap()
        .with_body(body)
        .unwrap()
}

#[test]
fn acceptance_effect_receipt_credit_hook_and_pending_budget_commit_once_across_restart() {
    let mut f = Fixture::new();
    f.credential();
    let board = Board::new(&mut f);
    let e = event(&f.device, "board:a", 1, None, b"new");
    receive(&mut f, &board, &e, "pigeon:alice").unwrap();
    let first = apply(&mut f, &board, e.id()).unwrap();
    let Integration::Accepted(original) = first else {
        panic!("not accepted")
    };
    assert_eq!(original.relevance, Relevance::Relevant);
    assert_eq!(original.delivery, delivery("pigeon:alice"));
    assert_eq!(
        Inbox::retained_bytes(&open_reader(&f.path).unwrap()).unwrap(),
        0
    );
    let reopened = Database::open(&f.path, &migrations()).unwrap();
    drop(std::mem::replace(&mut f.db, reopened));
    receive(&mut f, &board, &e, "pigeon:bob").unwrap();
    assert_eq!(
        apply(&mut f, &board, e.id()).unwrap(),
        Integration::Duplicate(original)
    );
    assert_eq!(count(&f, "effects"), 1);
    assert_eq!(count(&f, "credits"), 1);
    assert_eq!(count(&f, "receipts"), 1);
}
#[test]
fn failed_reducer_or_relevance_hook_rolls_back_effect_receipt_credit_and_quota_release() {
    let mut f = Fixture::new();
    f.credential();
    let mut board = Board::new(&mut f);
    let e = event(&f.device, "board:a", 1, None, b"new");
    receive(&mut f, &board, &e, "pigeon:alice").unwrap();
    let before = Inbox::retained_bytes(&open_reader(&f.path).unwrap()).unwrap();
    for relevant in [false, true] {
        board.fail_reduce = !relevant;
        board.fail_relevant = relevant;
        assert!(apply(&mut f, &board, e.id()).is_err());
        assert_eq!(count(&f, "effects"), 0);
        assert_eq!(count(&f, "credits"), 0);
        assert_eq!(count(&f, "receipts"), 0);
        assert_eq!(
            Inbox::retained_bytes(&open_reader(&f.path).unwrap()).unwrap(),
            before
        );
    }
    board.fail_relevant = false;
    board.fail_reduce = false;
    assert!(matches!(
        apply(&mut f, &board, e.id()).unwrap(),
        Integration::Accepted(_)
    ));
}
#[test]
fn obsolete_and_dependency_only_acceptance_never_run_the_relevance_hook() {
    let mut f = Fixture::new();
    f.credential();
    let board = Board::new(&mut f);
    for (resource, body, expected) in [
        ("board:a", b"obsolete".as_slice(), Relevance::Obsolete),
        ("board:b", b"dependency", Relevance::DependencyOnly),
    ] {
        let e = event(&f.device, resource, 1, None, body);
        receive(&mut f, &board, &e, "local").unwrap();
        let Integration::Accepted(receipt) = apply(&mut f, &board, e.id()).unwrap() else {
            panic!("not accepted")
        };
        assert_eq!(receipt.relevance, expected);
    }
    assert_eq!(count(&f, "effects"), 2);
    assert_eq!(count(&f, "credits"), 0);
}
#[test]
fn valid_signature_still_needs_membership_and_supported_semantics() {
    let mut f = Fixture::new();
    f.credential();
    let mut board = Board::new(&mut f);
    board.member = false;
    let e = event(&f.device, "board:a", 1, None, b"new");
    receive(&mut f, &board, &e, "local").unwrap();
    assert_eq!(
        apply(&mut f, &board, e.id()).unwrap(),
        Integration::Rejected("not_a_member".into())
    );
    let unknown = with_dependencies(&f.device, "board:b", "unknown.kind", vec![], b"unknown");
    receive(&mut f, &board, &unknown, "local").unwrap();
    assert_eq!(
        apply(&mut f, &board, unknown.id()).unwrap(),
        Integration::Unsupported
    );
    assert_eq!(count(&f, "effects"), 0);
    assert_eq!(count(&f, "credits"), 0);
    board.member = true;
    assert!(matches!(
        apply(&mut f, &board, e.id()).unwrap(),
        Integration::Accepted(_)
    ));
}
#[test]
fn explicit_semantic_dependencies_stage_until_effective_even_when_chain_continuity_exists() {
    let mut f = Fixture::new();
    f.credential();
    let board = Board::new(&mut f);
    let a = event(&f.device, "board:a", 1, None, b"original");
    let b = with_dependencies(
        &f.device,
        "board:a",
        "fixture.post",
        vec![Dependency::event(a.id())],
        b"reply",
    );
    // Same-position reply is intentionally another actor's stream.
    let other = device();
    f.db.write::<_, core::Error>(Class::Content, 4096, |tx| {
        bbsbox_identity::public::store_credential(tx, other.credential())
            .map_err(|_| core::Error::CorruptRecord)?;
        Ok(())
    })
    .unwrap();
    let b = with_dependencies(
        &other,
        "board:a",
        "fixture.post",
        b.metadata().dependencies.clone(),
        b"reply",
    );
    receive(&mut f, &board, &b, "pigeon:early").unwrap();
    assert_eq!(
        apply(&mut f, &board, b.id()).unwrap(),
        Integration::Staged(vec![Dependency::event(a.id())])
    );
    receive(&mut f, &board, &a, "pigeon:later").unwrap();
    assert!(matches!(
        apply(&mut f, &board, b.id()).unwrap(),
        Integration::Staged(_)
    ));
    apply(&mut f, &board, a.id()).unwrap();
    let Integration::Accepted(receipt) = apply(&mut f, &board, b.id()).unwrap() else {
        panic!("not accepted")
    };
    assert_eq!(receipt.delivery, delivery("pigeon:early"));
}
#[test]
fn hidden_cross_scope_content_is_denied_and_only_explicit_control_proofs_can_cross() {
    let mut f = Fixture::new();
    f.credential();
    let mut board = Board::new(&mut f);
    board.cross_control = true;
    let hidden = event(&f.device, "private:a", 1, None, b"hidden");
    receive(&mut f, &board, &hidden, "local").unwrap();
    apply(&mut f, &board, hidden.id()).unwrap();
    let leaked = with_dependencies(
        &f.device,
        "public:a",
        "fixture.post",
        vec![Dependency::event(hidden.id())],
        b"leak",
    );
    receive(&mut f, &board, &leaked, "local").unwrap();
    assert_eq!(
        apply(&mut f, &board, leaked.id()).unwrap(),
        Integration::Rejected("cross_scope_content_dependency".into())
    );
    let proof = with_dependencies(
        &f.device,
        "control:a",
        "fixture.control",
        vec![],
        b"dependency",
    );
    receive(&mut f, &board, &proof, "local").unwrap();
    apply(&mut f, &board, proof.id()).unwrap();
    let valid = with_dependencies(
        &f.device,
        "public:b",
        "fixture.post",
        vec![Dependency::event(proof.id())],
        b"valid",
    );
    receive(&mut f, &board, &valid, "local").unwrap();
    assert!(matches!(
        apply(&mut f, &board, valid.id()).unwrap(),
        Integration::Accepted(_)
    ));
}
#[test]
fn known_credential_and_blob_dependencies_keep_their_namespaces_and_visibility_policy() {
    let mut f = Fixture::new();
    f.credential();
    let board = Board::new(&mut f);
    let credential = Dependency::credential(f.device.credential().id());
    let blob = Dependency::blob(*f.device.credential().id().bytes());
    let e = with_dependencies(
        &f.device,
        "board:a",
        "fixture.post",
        vec![credential, blob],
        b"new",
    );
    receive(&mut f, &board, &e, "local").unwrap();
    assert_eq!(
        apply(&mut f, &board, e.id()).unwrap(),
        Integration::Staged(vec![blob])
    );
    f.db.write::<_, core::Error>(Class::Content, 4096, |tx| {
        tx.execute(
            "INSERT INTO fixture_blobs VALUES(?1,0)",
            [blob.digest.as_slice()],
        )?;
        Ok(())
    })
    .unwrap();
    assert!(matches!(
        apply(&mut f, &board, e.id()).unwrap(),
        Integration::Staged(_)
    ));
    f.db.write::<_, core::Error>(Class::Content, 4096, |tx| {
        tx.execute("UPDATE fixture_blobs SET visible=1", [])?;
        Ok(())
    })
    .unwrap();
    assert!(matches!(
        apply(&mut f, &board, e.id()).unwrap(),
        Integration::Accepted(_)
    ));
}
#[test]
fn late_fork_invalidates_prior_effects_and_cross_stream_dependents_with_intake_atomically() {
    let mut f = Fixture::new();
    f.credential();
    let mut board = Board::new(&mut f);
    let a = event(&f.device, "board:a", 1, None, b"a");
    let b = event(&f.device, "board:a", 2, Some(a.id()), b"b");
    let fork = event(&f.device, "board:a", 2, Some(a.id()), b"fork");
    let other = device();
    f.db.write::<_, core::Error>(Class::Content, 4096, |tx| {
        bbsbox_identity::public::store_credential(tx, other.credential())
            .map_err(|_| core::Error::CorruptRecord)?;
        Ok(())
    })
    .unwrap();
    let reply = with_dependencies(
        &other,
        "board:a",
        "fixture.post",
        vec![Dependency::event(b.id())],
        b"reply",
    );
    let unrelated = event(&f.device, "board:other", 1, None, b"other");
    for e in [&a, &b, &reply, &unrelated] {
        receive(&mut f, &board, e, "local").unwrap();
        apply(&mut f, &board, e.id()).unwrap();
    }
    assert_eq!(count(&f, "effects"), 4);
    board.fail_invalidation = true;
    assert!(receive(&mut f, &board, &fork, "peer").is_err());
    assert_eq!(count(&f, "effects"), 4);
    assert!(
        core::forks(&open_reader(&f.path).unwrap())
            .unwrap()
            .is_empty()
    );
    board.fail_invalidation = false;
    receive(&mut f, &board, &fork, "peer").unwrap();
    assert_eq!(count(&f, "effects"), 2);
    assert!(matches!(
        apply(&mut f, &board, b.id()).unwrap(),
        Integration::Duplicate(_)
    ));
    assert_eq!(count(&f, "credits"), 4);
    assert_eq!(count(&f, "receipts"), 4);
    assert_eq!(
        apply(&mut f, &board, fork.id()).unwrap(),
        Integration::Rejected("forked_stream".into())
    );
    assert!(!integration::fork_repair_pending(&open_reader(&f.path).unwrap(), board.id()).unwrap());
}
#[test]
fn bodyless_predecessor_allows_an_independent_post_but_cannot_supply_semantic_state() {
    let mut f = Fixture::new();
    f.credential();
    let board = Board::new(&mut f);
    let a = event(&f.device, "board:a", 1, None, b"hidden");
    f.receive(&a, false, "header");
    let b = event(&f.device, "board:a", 2, Some(a.id()), b"independent");
    receive(&mut f, &board, &b, "body").unwrap();
    assert_eq!(
        apply(&mut f, &board, a.id()).unwrap(),
        Integration::CommitmentOnly
    );
    assert!(matches!(
        apply(&mut f, &board, b.id()).unwrap(),
        Integration::Accepted(_)
    ));
    assert_eq!(count(&f, "credits"), 1);
}

fn action(resource: &str) -> Action {
    Action {
        kind: "fixture.post".into(),
        schema: 1,
        scope: Scope {
            resource: resource.into(),
            audience: "public".into(),
            distribution: Distribution::Shared,
        },
        dependencies: vec![],
        claimed_time: -50,
        extensions: vec![],
    }
}
fn authored(
    tx: &Transaction<'_>,
    device: &ActiveDevice,
    board: &Board,
    resource: &str,
) -> core::Result<CompleteEvent> {
    authoring::author(
        tx,
        Inbox::default(),
        Authored {
            action: action(resource),
            actor: Actor::device(device.credential()),
            credential: Some(device.credential()),
            body: b"new",
            delivery: &delivery("local"),
        },
        &resolve,
        &[board],
        |bytes| {
            device
                .sign_for(Purpose::Authorship, bytes)
                .map_err(|_| codec::Error::SignerUnavailable)
        },
    )
}
#[test]
fn local_authoring_reserves_positions_with_the_command_and_failed_effects_do_not_consume_them() {
    let mut f = Fixture::new();
    f.credential();
    let mut board = Board::new(&mut f);
    board.fail_reduce = true;
    let failure =
        f.db.write::<_, core::Error>(Class::Content, 128 * 1024, |tx| {
            let e = authored(tx, &f.device, &board, "board:a")?;
            integration::integrate(tx, e.id(), &board, &resolve)?;
            Ok(e)
        });
    assert!(failure.is_err());
    board.fail_reduce = false;
    let run = |db: &mut Database, id: &str| {
        db.transaction_service::<core::Error>(
            bbsbox_storage::Command {
                namespace: "fixture/member/post",
                id,
                request: b"post:new",
            },
            Class::Content,
            128 * 1024,
            |tx| {
                let e = authored(tx, &f.device, &board, "board:a")?;
                assert!(matches!(
                    integration::integrate(tx, e.id(), &board, &resolve)?,
                    Integration::Accepted(_)
                ));
                Ok(e.commitment().encoded().to_vec())
            },
        )
    };
    let first = run(&mut f.db, "one").unwrap();
    let retry = run(&mut f.db, "one").unwrap();
    assert!(retry.replayed);
    assert_eq!(retry.bytes, first.bytes);
    let second = run(&mut f.db, "two").unwrap();
    let parse =
        |bytes: &[u8]| codec::verify(bytes, |_| Some(f.device.credential().clone())).unwrap();
    let a = parse(&first.bytes);
    let b = parse(&second.bytes);
    assert_eq!(a.metadata().sequence, 1);
    assert_eq!(b.metadata().sequence, 2);
    assert_eq!(b.metadata().previous, Some(a.id()));
    assert_eq!(count(&f, "effects"), 2);
    assert_eq!(count(&f, "credits"), 2);
}
#[test]
fn queued_local_authors_share_the_writer_without_reusing_stream_positions() {
    let mut f = Fixture::new();
    f.credential();
    let board = Board::new(&mut f);
    let Fixture {
        _temp,
        path,
        db,
        device,
    } = f;
    drop(db);
    let writer =
        bbsbox_storage::writer::Writer::start(&path, migrations(), Default::default()).unwrap();
    let board = std::sync::Arc::new(board);
    let device = std::sync::Arc::new(device);
    let mut pending = vec![];
    for _ in 0..20 {
        let board = board.clone();
        let device = device.clone();
        pending.push(
            writer
                .handle()
                .submit_service(bbsbox_storage::writer::Priority::Interactive, move |db| {
                    db.write::<_, core::Error>(Class::Content, 128 * 1024, |tx| {
                        let e = authored(tx, &device, &board, "board:a")?;
                        integration::integrate(tx, e.id(), board.as_ref(), &resolve)?;
                        Ok((e.id(), e.metadata().sequence, e.metadata().previous))
                    })
                })
                .unwrap(),
        );
    }
    let mut last = None;
    for (index, reply) in pending.into_iter().enumerate() {
        let (id, sequence, previous) = reply.blocking_wait().unwrap();
        assert_eq!(sequence, index as u64 + 1);
        assert_eq!(previous, last);
        last = Some(id);
    }
    writer.shutdown().unwrap();
    assert_eq!(
        open_reader(&path)
            .unwrap()
            .query_row("SELECT count(*) FROM events_receipts", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        20
    );
    drop(_temp);
}
#[test]
fn restarted_node_signing_uses_a_fresh_stream_and_rejects_old_incarnation_context() {
    let mut f = Fixture::new();
    let first = bbsbox_identity::node::NodeSigner::start(&mut f.db).unwrap();
    let sign = |db: &mut Database, signer: &bbsbox_identity::node::NodeSigner, inc: [u8; 16]| {
        db.write::<_, core::Error>(Class::Content, 128 * 1024, |tx| {
            authoring::author(
                tx,
                Inbox::default(),
                Authored {
                    action: action("node:status"),
                    actor: Actor::root(codec::Role::Node, signer.public(), inc)?,
                    credential: None,
                    body: b"status",
                    delivery: &delivery("local"),
                },
                &resolve,
                &[],
                |bytes| {
                    signer
                        .sign_current(&inc, bytes)
                        .map_err(|_| codec::Error::SignerUnavailable)
                },
            )
        })
    };
    let a = sign(&mut f.db, &first, *first.incarnation()).unwrap();
    let second = bbsbox_identity::node::NodeSigner::start(&mut f.db).unwrap();
    assert_eq!(first.id(), second.id());
    assert!(sign(&mut f.db, &second, *first.incarnation()).is_err());
    let b = sign(&mut f.db, &second, *second.incarnation()).unwrap();
    assert_eq!(a.metadata().sequence, 1);
    assert_eq!(b.metadata().sequence, 1);
    assert_ne!(a.metadata().stream, b.metadata().stream);
}

#[test]
fn integration_migration_preserves_v1_intake_and_its_first_source() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("old.sqlite");
    let old = migrations()
        .into_iter()
        .filter(|m| !(m.module == "events" && m.version == 2))
        .collect::<Vec<_>>();
    let mut db = Database::open(&path, &old).unwrap();
    let device = device();
    db.write::<_, core::Error>(Class::Content, 4096, |tx| {
        bbsbox_identity::public::store_credential(tx, device.credential())
            .map_err(|_| core::Error::CorruptRecord)?;
        Ok(())
    })
    .unwrap();
    let e = event(&device, "board:a", 1, None, b"kept");
    db.write::<_, core::Error>(Class::Content, 128 * 1024, |tx| {
        Inbox::default().receive(
            tx,
            e.commitment().encoded(),
            Some(e.body()),
            &delivery("pigeon:original"),
            &resolve,
        )
    })
    .unwrap();
    drop(db);
    let db = Database::open(&path, &migrations()).unwrap();
    let mut f = Fixture {
        _temp: temp,
        path,
        db,
        device,
    };
    let board = Board::new(&mut f);
    let Integration::Accepted(receipt) = apply(&mut f, &board, e.id()).unwrap() else {
        panic!("not accepted")
    };
    assert_eq!(receipt.delivery, delivery("pigeon:original"));
    assert_eq!(
        Inbox::retained_bytes(&open_reader(&f.path).unwrap()).unwrap(),
        0
    );
}
