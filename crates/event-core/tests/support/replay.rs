// SPDX-License-Identifier: AGPL-3.0-or-later
//! Domain fixture, not M06's final post/governance schema. Immutable revision heads preserve conflicts.
use super::*;
use bbsbox_event_codec::{
    Dependency, Role,
    authority::{Decision, KindRule, Registry},
};
use bbsbox_event_core::integration::{self, Integration, Module, Relevance, ReplayModule};
use rusqlite::{OptionalExtension, Transaction, params};

struct Revisions {
    registry: Registry,
    generation: i64,
}
#[derive(Clone)]
struct Body {
    op: u8,
    post: Option<EventId>,
    previous: Option<EventId>,
    reply: Option<EventId>,
    text: String,
}
fn id_value(id: Option<EventId>) -> Value {
    id.map(|id| Value::Bytes(id.bytes().to_vec()))
        .unwrap_or(Value::Null)
}
fn body_bytes(body: &Body) -> Vec<u8> {
    let mut bytes = vec![];
    ciborium::ser::into_writer(
        &Value::Array(vec![
            Value::Integer(1.into()),
            Value::Integer(body.op.into()),
            id_value(body.post),
            id_value(body.previous),
            id_value(body.reply),
            Value::Text(body.text.clone()),
        ]),
        &mut bytes,
    )
    .unwrap();
    bytes
}
fn parse_id(value: &Value) -> Option<Option<EventId>> {
    if *value == Value::Null {
        Some(None)
    } else {
        Some(Some(EventId::from_bytes(
            value.as_bytes()?.as_slice().try_into().ok()?,
        )))
    }
}
fn body(event: &CompleteEvent) -> Option<Body> {
    let value: Value = ciborium::de::from_reader_with_recursion_limit(event.body(), 8).ok()?;
    let fields = value.as_array()?;
    if fields.len() != 6 || fields[0] != Value::Integer(1.into()) {
        return None;
    }
    let body = Body {
        op: u8::try_from(fields[1].as_integer()?).ok()?,
        post: parse_id(&fields[2])?,
        previous: parse_id(&fields[3])?,
        reply: parse_id(&fields[4])?,
        text: fields[5].as_text()?.into(),
    };
    if body_bytes(&body) != event.body() || body.text.len() > 4096 {
        return None;
    }
    match body.op {
        0 if body.post.is_none() && body.previous.is_none() => (),
        1 | 2 if body.post.is_some() && body.previous.is_some() && body.reply.is_none() => (),
        _ => return None,
    }
    if body.op == 2 && !body.text.is_empty() {
        return None;
    }
    let expected_kind = match body.op {
        0 => "revision.create",
        1 => "revision.edit",
        2 => "revision.delete",
        _ => return None,
    };
    (event.metadata().kind == expected_kind).then_some(body)
}
impl Revisions {
    fn new(f: &mut Fixture, generation: i64) -> Self {
        f.db.write::<_,core::Error>(Class::Control,16384,|tx|{
            tx.execute_batch("CREATE TABLE IF NOT EXISTS revision_facts(generation INTEGER NOT NULL,id BLOB NOT NULL,post BLOB NOT NULL,previous BLOB,reply BLOB,author BLOB NOT NULL,op INTEGER NOT NULL,text TEXT NOT NULL,valid INTEGER NOT NULL DEFAULT 1,PRIMARY KEY(generation,id));
            CREATE INDEX IF NOT EXISTS revision_previous ON revision_facts(generation,previous);
            CREATE TABLE IF NOT EXISTS revision_active(singleton INTEGER PRIMARY KEY,generation INTEGER NOT NULL);
            INSERT OR IGNORE INTO revision_active VALUES(1,1);
            CREATE TABLE IF NOT EXISTS revision_policy(singleton INTEGER PRIMARY KEY,version INTEGER NOT NULL,member INTEGER NOT NULL);
            INSERT OR IGNORE INTO revision_policy VALUES(1,1,1);
            CREATE TABLE IF NOT EXISTS revision_hooks(event BLOB PRIMARY KEY);")?;Ok(())
        }).unwrap();
        Self {
            generation,
            registry: Registry::new(
                ["revision.create", "revision.edit", "revision.delete"]
                    .into_iter()
                    .map(|kind| KindRule {
                        kind: kind.into(),
                        schema: 1,
                        actor: Role::PersonDevice,
                        purpose: Some(Purpose::Authorship),
                    })
                    .collect(),
                vec![],
            )
            .unwrap(),
        }
    }
    fn facts(&self, tx: &Transaction<'_>, event: &CompleteEvent) -> core::Result<()> {
        let b = body(event).ok_or(core::Error::CorruptRecord)?;
        tx.execute("INSERT OR IGNORE INTO revision_facts(generation,id,post,previous,reply,author,op,text) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![self.generation,event.id().bytes().as_slice(),b.post.unwrap_or(event.id()).bytes().as_slice(),b.previous.as_ref().map(|id|id.bytes().as_slice()),b.reply.as_ref().map(|id|id.bytes().as_slice()),event.metadata().actor.subject.as_slice(),b.op,b.text])?;
        Ok(())
    }
    fn publish(&self, f: &mut Fixture, events: u64, policy: i64) -> core::Result<()> {
        f.db.write(Class::Control, 4096, |tx| {
            core::require_revision(tx, events)?;
            let current: i64 = tx.query_row(
                "SELECT version FROM revision_policy WHERE singleton=1",
                [],
                |r| r.get(0),
            )?;
            if current != policy {
                return Err(core::Error::RevisionChanged);
            }
            tx.execute(
                "UPDATE revision_active SET generation=?1 WHERE singleton=1",
                [self.generation],
            )?;
            Ok(())
        })
    }
}
impl Module for Revisions {
    fn id(&self) -> &str {
        "revision-fixture"
    }
    fn registry(&self) -> &Registry {
        &self.registry
    }
    fn extension(
        &self,
        _: &Connection,
        _: &CompleteEvent,
        extension: &codec::Extension,
    ) -> core::Result<Decision> {
        Ok(
            if extension.id == 9 && extension.required && extension.data == b"feature-proof" {
                Decision::Allow
            } else {
                Decision::Deny("bad_feature_proof")
            },
        )
    }
    fn action(&self, connection: &Connection, event: &CompleteEvent) -> core::Result<Decision> {
        let member: bool = connection.query_row(
            "SELECT member FROM revision_policy WHERE singleton=1",
            [],
            |r| r.get(0),
        )?;
        if !member {
            return Ok(Decision::Deny("known_membership_revoked"));
        }
        let Some(b) = body(event) else {
            return Ok(Decision::Deny("invalid_revision_body"));
        };
        let expected = [b.post, b.previous, b.reply]
            .into_iter()
            .flatten()
            .map(Dependency::event)
            .collect::<std::collections::BTreeSet<_>>();
        if event
            .metadata()
            .dependencies
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            != expected
        {
            return Ok(Decision::Deny("revision_dependencies_mismatch"));
        }
        for required in &expected {
            let present:bool=connection.query_row("SELECT EXISTS(SELECT 1 FROM revision_facts WHERE generation=?1 AND id=?2 AND valid=1)",params![self.generation,required.digest.as_slice()],|r|r.get(0))?;
            if !present {
                return Ok(Decision::Missing(vec![*required]));
            }
        }
        if let (Some(post), Some(previous)) = (b.post, b.previous) {
            let original:Option<Vec<u8>>=connection.query_row("SELECT author FROM revision_facts WHERE generation=?1 AND id=?2 AND post=id AND op=0 AND valid=1",params![self.generation,post.bytes().as_slice()],|r|r.get(0)).optional()?;
            if original.as_deref() != Some(event.metadata().actor.subject.as_slice()) {
                return Ok(Decision::Deny("not_the_post_author"));
            }
            let parent: Option<(Vec<u8>, u8)> = connection
                .query_row(
                    "SELECT post,op FROM revision_facts WHERE generation=?1 AND id=?2 AND valid=1",
                    params![self.generation, previous.bytes().as_slice()],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            if !parent.is_some_and(|(target, op)| target == post.bytes().as_slice() && op != 2) {
                return Ok(Decision::Deny("invalid_revision_predecessor"));
            }
        }
        if let Some(parent) = b.reply {
            let is_post:bool=connection.query_row("SELECT EXISTS(SELECT 1 FROM revision_facts WHERE generation=?1 AND id=?2 AND post=id AND op=0 AND valid=1)",params![self.generation,parent.bytes().as_slice()],|r|r.get(0))?;
            if !is_post {
                return Ok(Decision::Deny("invalid_reply_parent"));
            }
        }
        Ok(Decision::Allow)
    }
    fn dependency(
        &self,
        _: &Connection,
        _: &CompleteEvent,
        _: Dependency,
    ) -> core::Result<Decision> {
        Ok(Decision::Allow)
    }
    fn dependency_effective(&self, connection: &Connection, id: EventId) -> core::Result<bool> {
        Ok(connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM revision_facts WHERE generation=?1 AND id=?2 AND valid=1)",
            params![self.generation, id.bytes().as_slice()],
            |r| r.get(0),
        )?)
    }
    fn reduce(&self, tx: &Transaction<'_>, event: &CompleteEvent) -> core::Result<Relevance> {
        self.facts(tx, event)?;
        Ok(Relevance::Relevant)
    }
    fn relevant(
        &self,
        tx: &Transaction<'_>,
        event: &CompleteEvent,
        _: &Delivery,
    ) -> core::Result<()> {
        tx.execute(
            "INSERT INTO revision_hooks VALUES(?1)",
            [event.id().bytes().as_slice()],
        )?;
        Ok(())
    }
    fn invalidate_fork(&self, tx: &Transaction<'_>, fork: &core::Fork) -> core::Result<()> {
        tx.execute("WITH RECURSIVE bad(id) AS (SELECT id FROM events_links WHERE stream=?1 AND sequence>=?2 UNION SELECT r.id FROM revision_facts r JOIN bad b ON r.previous=b.id OR r.reply=b.id OR r.post=b.id) UPDATE revision_facts SET valid=0 WHERE id IN (SELECT id FROM bad)",params![fork.stream.as_slice(),fork.first_sequence.to_be_bytes().as_slice()])?;
        Ok(())
    }
}
impl ReplayModule for Revisions {
    fn project(&self, tx: &Transaction<'_>, event: &CompleteEvent) -> core::Result<()> {
        self.facts(tx, event)
    }
}
fn install(f: &mut Fixture, devices: &[&ActiveDevice]) {
    f.db.write::<_, core::Error>(Class::Control, 16384, |tx| {
        for device in devices {
            bbsbox_identity::public::store_credential(tx, device.credential())
                .map_err(|_| core::Error::CorruptRecord)?;
        }
        Ok(())
    })
    .unwrap();
}
fn signed(
    device: &ActiveDevice,
    seq: u64,
    previous: Option<EventId>,
    b: Body,
    time: i64,
) -> CompleteEvent {
    let dependencies = [b.post, b.previous, b.reply]
        .into_iter()
        .flatten()
        .map(Dependency::event)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    codec::sign(
        Draft {
            kind: match b.op {
                0 => "revision.create",
                1 => "revision.edit",
                2 => "revision.delete",
                _ => panic!("fixture op"),
            }
            .into(),
            schema: 1,
            scope: Scope {
                resource: "board:replay".into(),
                audience: "public".into(),
                distribution: Distribution::Shared,
            },
            sequence: seq,
            previous,
            dependencies,
            claimed_time: time,
            extensions: vec![],
        },
        Actor::device(device.credential()),
        &body_bytes(&b),
        Some(device.credential()),
        |bytes| {
            device
                .sign_for(Purpose::Authorship, bytes)
                .map_err(|_| codec::Error::SignerUnavailable)
        },
    )
    .unwrap()
}
fn create(device: &ActiveDevice, text: &str) -> CompleteEvent {
    signed(
        device,
        1,
        None,
        Body {
            op: 0,
            post: None,
            previous: None,
            reply: None,
            text: text.into(),
        },
        i64::MAX,
    )
}
fn intake(f: &mut Fixture, module: &Revisions, e: &CompleteEvent) {
    f.db.write::<_, core::Error>(Class::Content, 128 * 1024, |tx| {
        integration::receive(
            tx,
            Inbox::default(),
            e.commitment().encoded(),
            Some(e.body()),
            &delivery("fixture:carrier"),
            &resolve,
            &[module],
        )
    })
    .unwrap();
}
fn settle(f: &mut Fixture, module: &Revisions) {
    for _ in 0..32 {
        let before: i64 = open_reader(&f.path)
            .unwrap()
            .query_row("SELECT count(*) FROM events_receipts", [], |r| r.get(0))
            .unwrap();
        let mut cursor = None;
        let mut changed = 0;
        loop {
            let batch =
                f.db.write::<_, core::Error>(Class::Content, 4096, |tx| {
                    integration::reconcile(tx, Inbox::default(), cursor, 2, &resolve, &[module])
                })
                .unwrap();
            changed += batch.changed;
            cursor = batch.next;
            if cursor.is_none() {
                break;
            }
        }
        let mut cursor = None;
        loop {
            let ids = integration::pending(&open_reader(&f.path).unwrap(), cursor, 2).unwrap();
            for id in &ids {
                f.db.write::<_, core::Error>(Class::Content, 16384, |tx| {
                    integration::integrate(tx, *id, module, &resolve)
                })
                .unwrap();
            }
            cursor = ids.last().copied();
            if ids.len() < 2 {
                break;
            }
        }
        let after: i64 = open_reader(&f.path)
            .unwrap()
            .query_row("SELECT count(*) FROM events_receipts", [], |r| r.get(0))
            .unwrap();
        if before == after && changed == 0 {
            return;
        }
    }
    panic!("fixture did not settle");
}
fn heads(f: &Fixture) -> Vec<(Vec<u8>, Vec<u8>, u8, String)> {
    let reader = open_reader(&f.path).unwrap();
    let mut statement=reader.prepare("SELECT r.post,r.id,r.op,r.text FROM revision_facts r JOIN revision_active a ON a.generation=r.generation WHERE r.valid=1 AND NOT EXISTS(SELECT 1 FROM revision_facts child WHERE child.generation=r.generation AND child.previous=r.id AND child.valid=1) ORDER BY r.post,r.id").unwrap();
    statement
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap()
}
fn rebuild(f: &mut Fixture, module: &Revisions) -> u64 {
    let stamp = core::revision(&open_reader(&f.path).unwrap()).unwrap();
    for _ in 0..32 {
        let before: i64 = open_reader(&f.path)
            .unwrap()
            .query_row(
                "SELECT count(*) FROM revision_facts WHERE generation=?1",
                [module.generation],
                |r| r.get(0),
            )
            .unwrap();
        let mut cursor = None;
        loop {
            let ids = integration::accepted(&open_reader(&f.path).unwrap(), module.id(), cursor, 2)
                .unwrap();
            for id in &ids {
                f.db.write::<_, core::Error>(Class::Content, 8192, |tx| {
                    integration::replay(tx, *id, module, &resolve)
                })
                .unwrap();
            }
            cursor = ids.last().copied();
            if ids.len() < 2 {
                break;
            }
        }
        let after: i64 = open_reader(&f.path)
            .unwrap()
            .query_row(
                "SELECT count(*) FROM revision_facts WHERE generation=?1",
                [module.generation],
                |r| r.get(0),
            )
            .unwrap();
        if before == after {
            assert_eq!(
                core::revision(&open_reader(&f.path).unwrap()).unwrap(),
                stamp
            );
            return stamp;
        }
    }
    panic!("replay did not settle");
}
fn permutations(values: &mut [usize], at: usize, out: &mut Vec<Vec<usize>>) {
    if at == values.len() {
        out.push(values.to_vec());
        return;
    }
    for index in at..values.len() {
        values.swap(at, index);
        permutations(values, at + 1, out);
        values.swap(at, index);
    }
}

#[test]
fn all_120_arrival_orders_converge_for_causal_edits_tombstones_and_concurrent_replies() {
    let author = device();
    let friend = device();
    let neighbor = device();
    let original = create(&author, "original");
    let edit = signed(
        &author,
        2,
        Some(original.id()),
        Body {
            op: 1,
            post: Some(original.id()),
            previous: Some(original.id()),
            reply: None,
            text: "edited".into(),
        },
        i64::MIN,
    );
    let deleted = signed(
        &author,
        3,
        Some(edit.id()),
        Body {
            op: 2,
            post: Some(original.id()),
            previous: Some(edit.id()),
            reply: None,
            text: String::new(),
        },
        0,
    );
    let reply = |device: &ActiveDevice, text: &str| {
        signed(
            device,
            1,
            None,
            Body {
                op: 0,
                post: None,
                previous: None,
                reply: Some(original.id()),
                text: text.into(),
            },
            -9,
        )
    };
    let events = [
        original.clone(),
        edit,
        deleted,
        reply(&friend, "friend"),
        reply(&neighbor, "neighbor"),
    ];
    let mut orders = vec![];
    permutations(&mut [0, 1, 2, 3, 4], 0, &mut orders);
    assert_eq!(orders.len(), 120);
    let mut expected = None;
    for order in orders {
        let mut f = Fixture::new();
        install(&mut f, &[&author, &friend, &neighbor]);
        let module = Revisions::new(&mut f, 1);
        for index in order {
            intake(&mut f, &module, &events[index]);
            settle(&mut f, &module);
        }
        let view = heads(&f);
        assert_eq!(view.len(), 3);
        assert!(view.iter().any(|(_, _, op, _)| *op == 2));
        if let Some(expected) = &expected {
            assert_eq!(&view, expected);
        } else {
            expected = Some(view);
        }
    }
}
#[test]
fn fresh_generation_replay_preserves_receipts_and_hooks_and_an_older_original_cannot_roll_back_an_edit()
 {
    let mut f = Fixture::new();
    f.credential();
    let module = Revisions::new(&mut f, 1);
    let original = create(&f.device, "original");
    let edit = signed(
        &f.device,
        2,
        Some(original.id()),
        Body {
            op: 1,
            post: Some(original.id()),
            previous: Some(original.id()),
            reply: None,
            text: "newer".into(),
        },
        i64::MIN,
    );
    intake(&mut f, &module, &edit);
    settle(&mut f, &module);
    assert!(heads(&f).is_empty());
    intake(&mut f, &module, &original);
    settle(&mut f, &module);
    let expected = heads(&f);
    assert_eq!(expected[0].3, "newer");
    let next = Revisions::new(&mut f, 2);
    let stamp = rebuild(&mut f, &next);
    next.publish(&mut f, stamp, 1).unwrap();
    assert_eq!(heads(&f), expected);
    f.db.write::<_, core::Error>(Class::Content, 8192, |tx| {
        integration::replay(tx, original.id(), &next, &resolve)
    })
    .unwrap();
    assert_eq!(heads(&f), expected);
    let reader = open_reader(&f.path).unwrap();
    assert_eq!(
        reader
            .query_row("SELECT count(*) FROM revision_hooks", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        reader
            .query_row("SELECT count(*) FROM events_receipts", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
}
#[test]
fn equal_forked_sets_have_equal_projections_even_when_one_branch_was_accepted_before_the_fork() {
    let author = device();
    let original = create(&author, "original");
    let branch = |text: &str| {
        signed(
            &author,
            2,
            Some(original.id()),
            Body {
                op: 1,
                post: Some(original.id()),
                previous: Some(original.id()),
                reply: None,
                text: text.into(),
            },
            i64::MAX,
        )
    };
    let a = branch("left");
    let b = branch("right");
    let histories = [
        vec![original.clone(), a.clone(), b.clone()],
        vec![a.clone(), b.clone(), original.clone()],
        vec![b, a, original],
    ];
    let mut expected = None;
    for history in histories {
        let mut f = Fixture::new();
        install(&mut f, &[&author]);
        let module = Revisions::new(&mut f, 1);
        for event in history {
            intake(&mut f, &module, &event);
            settle(&mut f, &module);
        }
        let next = Revisions::new(&mut f, 2);
        let stamp = rebuild(&mut f, &next);
        next.publish(&mut f, stamp, 1).unwrap();
        let view = heads(&f);
        assert_eq!(view.len(), 1);
        assert_eq!(view[0].3, "original");
        if let Some(expected) = &expected {
            assert_eq!(&view, expected);
        } else {
            expected = Some(view);
        }
    }
}
#[test]
fn a_new_event_or_policy_change_blocks_stale_projection_publication() {
    let mut f = Fixture::new();
    f.credential();
    let module = Revisions::new(&mut f, 1);
    let original = create(&f.device, "original");
    intake(&mut f, &module, &original);
    settle(&mut f, &module);
    let next = Revisions::new(&mut f, 2);
    let stamp = rebuild(&mut f, &next);
    let other = device();
    install(&mut f, &[&other]);
    let e = create(&other, "other");
    intake(&mut f, &module, &e);
    assert!(matches!(
        next.publish(&mut f, stamp, 1),
        Err(core::Error::RevisionChanged)
    ));
    assert_eq!(heads(&f).len(), 1);
    settle(&mut f, &module);
    let stamp = rebuild(&mut f, &next);
    f.db.write::<_, core::Error>(Class::Control, 4096, |tx| {
        tx.execute("UPDATE revision_policy SET version=2,member=0", [])?;
        Ok(())
    })
    .unwrap();
    assert!(matches!(
        next.publish(&mut f, stamp, 1),
        Err(core::Error::RevisionChanged)
    ));
    let revoked = Revisions::new(&mut f, 3);
    let stamp = rebuild(&mut f, &revoked);
    revoked.publish(&mut f, stamp, 2).unwrap();
    assert!(heads(&f).is_empty());
    assert_eq!(
        open_reader(&f.path)
            .unwrap()
            .query_row("SELECT count(*) FROM events_receipts", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
}
#[test]
fn another_person_cannot_edit_a_post_and_operational_policy_errors_are_not_missing_dependencies() {
    let mut f = Fixture::new();
    f.credential();
    let module = Revisions::new(&mut f, 1);
    let original = create(&f.device, "original");
    intake(&mut f, &module, &original);
    settle(&mut f, &module);
    let attacker = device();
    install(&mut f, &[&attacker]);
    let forged = signed(
        &attacker,
        1,
        None,
        Body {
            op: 1,
            post: Some(original.id()),
            previous: Some(original.id()),
            reply: None,
            text: "forged".into(),
        },
        i64::MAX,
    );
    intake(&mut f, &module, &forged);
    let result =
        f.db.write::<_, core::Error>(Class::Content, 8192, |tx| {
            integration::integrate(tx, forged.id(), &module, &resolve)
        })
        .unwrap();
    assert_eq!(result, Integration::Rejected("not_the_post_author".into()));
    f.db.write::<_, core::Error>(Class::Control, 4096, |tx| {
        tx.execute("DROP TABLE revision_policy", [])?;
        Ok(())
    })
    .unwrap();
    assert!(
        f.db.write::<_, core::Error>(Class::Content, 8192, |tx| integration::integrate(
            tx,
            forged.id(),
            &module,
            &resolve
        ))
        .is_err()
    );
    assert_eq!(heads(&f)[0].3, "original");
}

#[test]
fn concurrent_author_edits_from_different_devices_remain_visible_conflicts_without_a_clock_winner()
{
    let root = RootKey::generate();
    let activate = || {
        let key = DeviceKey::generate();
        let credential = root
            .issue_device(key.public(), Purposes::new(&[Purpose::Authorship]).unwrap())
            .unwrap();
        key.activate(root.person_id(), credential).unwrap()
    };
    let original_device = activate();
    let left_device = activate();
    let right_device = activate();
    let original = create(&original_device, "original");
    let left = signed(
        &left_device,
        1,
        None,
        Body {
            op: 1,
            post: Some(original.id()),
            previous: Some(original.id()),
            reply: None,
            text: "left".into(),
        },
        i64::MIN,
    );
    let right = signed(
        &right_device,
        1,
        None,
        Body {
            op: 1,
            post: Some(original.id()),
            previous: Some(original.id()),
            reply: None,
            text: "right".into(),
        },
        i64::MAX,
    );
    for edits in [[&left, &right], [&right, &left]] {
        let mut f = Fixture::new();
        install(&mut f, &[&original_device, &left_device, &right_device]);
        let module = Revisions::new(&mut f, 1);
        intake(&mut f, &module, &original);
        settle(&mut f, &module);
        for e in edits {
            intake(&mut f, &module, e);
            settle(&mut f, &module);
        }
        let mut text = heads(&f).into_iter().map(|row| row.3).collect::<Vec<_>>();
        text.sort();
        assert_eq!(text, ["left", "right"]);
    }
}
#[test]
fn unsupported_required_semantics_can_activate_without_rewriting_bytes_while_unknown_minor_stays_unsupported()
 {
    let mut f = Fixture::new();
    f.credential();
    let mut module = Revisions::new(&mut f, 1);
    let base = create(&f.device, "future-proof");
    let bytes = rewrite(&base, &f.device, |fields| {
        fields[16] = Value::Array(vec![Value::Array(vec![
            Value::Integer(9.into()),
            Value::Bool(true),
            Value::Bytes(b"feature-proof".to_vec()),
        ])])
    });
    let proof = codec::verify(&bytes, |_| Some(f.device.credential().clone()))
        .unwrap()
        .with_body(base.body())
        .unwrap();
    intake(&mut f, &module, &proof);
    assert_eq!(
        f.db.write::<_, core::Error>(Class::Content, 8192, |tx| integration::integrate(
            tx,
            proof.id(),
            &module,
            &resolve
        ))
        .unwrap(),
        Integration::Unsupported
    );
    assert!(heads(&f).is_empty());
    let rules = || {
        ["revision.create", "revision.edit", "revision.delete"]
            .into_iter()
            .map(|kind| KindRule {
                kind: kind.into(),
                schema: 1,
                actor: Role::PersonDevice,
                purpose: Some(Purpose::Authorship),
            })
            .collect()
    };
    module.registry = Registry::new(rules(), vec![9]).unwrap();
    settle(&mut f, &module);
    assert_eq!(heads(&f)[0].3, "future-proof");
    let next_event = signed(
        &f.device,
        2,
        Some(proof.id()),
        Body {
            op: 0,
            post: None,
            previous: None,
            reply: None,
            text: "minor-one".into(),
        },
        0,
    );
    let unknown_bytes = rewrite(&next_event, &f.device, |fields| {
        fields[2] = Value::Integer(1.into())
    });
    let unknown = codec::verify(&unknown_bytes, |_| Some(f.device.credential().clone()))
        .unwrap()
        .with_body(next_event.body())
        .unwrap();
    intake(&mut f, &module, &unknown);
    let reopened = Database::open(&f.path, &migrations()).unwrap();
    drop(std::mem::replace(&mut f.db, reopened));
    assert_eq!(
        f.db.write::<_, core::Error>(Class::Content, 8192, |tx| integration::integrate(
            tx,
            unknown.id(),
            &module,
            &resolve
        ))
        .unwrap(),
        Integration::Unsupported
    );
    let mut next = Revisions::new(&mut f, 2);
    next.registry = Registry::new(rules(), vec![9]).unwrap();
    let stamp = rebuild(&mut f, &next);
    next.publish(&mut f, stamp, 1).unwrap();
    assert_eq!(heads(&f).len(), 1);
    assert_eq!(
        integration::complete(&open_reader(&f.path).unwrap(), proof.id(), &resolve)
            .unwrap()
            .unwrap()
            .commitment()
            .encoded(),
        bytes
    );
    assert_eq!(
        open_reader(&f.path)
            .unwrap()
            .query_row("SELECT count(*) FROM revision_hooks", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
}
#[test]
fn exact_duplicate_delivery_and_acceptance_do_not_invalidate_a_projection_rebuild() {
    let mut f = Fixture::new();
    f.credential();
    let module = Revisions::new(&mut f, 1);
    let e = create(&f.device, "original");
    intake(&mut f, &module, &e);
    settle(&mut f, &module);
    let next = Revisions::new(&mut f, 2);
    let stamp = rebuild(&mut f, &next);
    intake(&mut f, &module, &e);
    settle(&mut f, &module);
    next.publish(&mut f, stamp, 1).unwrap();
    assert_eq!(heads(&f)[0].3, "original");
}
