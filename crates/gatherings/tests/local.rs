// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_gatherings::{
    Change, Details, Error, Gatherings, Occurrence, Schedule, ScheduleError, get, list,
};
use bbsbox_identity::{account_by_username, node::NodeSigner};
use bbsbox_storage::{Database, disk::Class, open_reader};

#[test]
fn organizer_revisions_are_causal_and_cancellation_keeps_the_stable_gathering() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("gatherings.sqlite");
    let mut registry = bbsbox_identity::migrations();
    registry.extend(bbsbox_event_core::migrations());
    registry.extend(bbsbox_gatherings::migrations());
    let mut database = Database::open(&path, &registry).unwrap();
    let signer = NodeSigner::start(&mut database).unwrap();
    let (host, visitor) = database
        .write(Class::Control, 65536, |tx| -> bbsbox_identity::Result<_> {
            tx.execute_batch(
                "INSERT INTO identity_accounts(id,username,password_hash,role) VALUES
                 (x'01010101010101010101010101010101','host','fixture','member'),
                 (x'02020202020202020202020202020202','visitor','fixture','sysop');",
            )?;
            Ok((
                account_by_username(tx, "host")?.id,
                account_by_username(tx, "visitor")?.id,
            ))
        })
        .unwrap();
    let schedule = Schedule::resolve("2026-11-07T18:30", "America/Chicago", None).unwrap();
    let original = Details {
        title: "Library game night".into(),
        place: "Meeting room A".into(),
        description: "Bring a game.".into(),
        schedule,
    };
    let service = Gatherings::new(&signer);
    let mut submit = |actor, action, now| {
        database.write(Class::Content, 128 * 1024, |tx| {
            service.record(tx, &signer, actor, action, now)
        })
    };
    let (id, created) = submit(host, Change::Create(original.clone()), 30).unwrap();
    let reader = open_reader(&path).unwrap();
    assert_eq!(list(&reader, Some(host), None).unwrap()[0].id, id);
    assert!(list(&reader, Some(visitor), None).unwrap().is_empty());
    let changed = Details {
        place: "Meeting room B".into(),
        ..original
    };
    assert!(matches!(
        submit(
            visitor,
            Change::Revise {
                id,
                previous: created,
                details: changed.clone()
            },
            40
        ),
        Err(Error::Forbidden)
    ));
    let (same_id, edited) = submit(
        host,
        Change::Revise {
            id,
            previous: created,
            details: changed.clone(),
        },
        20,
    )
    .unwrap();
    assert_eq!(id, same_id);
    assert_eq!(get(&reader, id).unwrap().unwrap().details, changed);
    assert!(matches!(
        submit(
            host,
            Change::Cancel {
                id,
                previous: created
            },
            50
        ),
        Err(Error::Conflict)
    ));
    let (_, cancelled) = submit(
        host,
        Change::Cancel {
            id,
            previous: edited,
        },
        10,
    )
    .unwrap();
    let view = get(&reader, id).unwrap().unwrap();
    assert!(view.cancelled);
    assert_eq!(view.revision, cancelled);
    assert_eq!(view.details, changed);
    assert!(matches!(
        submit(
            host,
            Change::Revise {
                id,
                previous: cancelled,
                details: changed
            },
            60
        ),
        Err(Error::Conflict)
    ));
}

#[test]
fn offline_timezone_data_distinguishes_skipped_and_repeated_times() {
    assert_eq!(
        Schedule::resolve("2026-03-08T02:30", "America/New_York", None),
        Err(ScheduleError::SkippedTime)
    );
    assert_eq!(
        Schedule::resolve("2026-11-01T01:30", "America/New_York", None),
        Err(ScheduleError::RepeatedTime)
    );
    let first = Schedule::resolve(
        "2026-11-01T01:30",
        "America/New_York",
        Some(Occurrence::First),
    )
    .unwrap();
    let second = Schedule::resolve(
        "2026-11-01T01:30",
        "America/New_York",
        Some(Occurrence::Second),
    )
    .unwrap();
    assert_eq!(first.offset_seconds(), -4 * 3600);
    assert_eq!(second.offset_seconds(), -5 * 3600);
    assert_eq!(second.utc_millis() - first.utc_millis(), 3_600_000);
    assert_eq!(
        Schedule::resolve("2026-02-30T01:30", "UTC", None),
        Err(ScheduleError::InvalidDate)
    );
    assert_eq!(
        Schedule::resolve("2026-11-07T18:30", "Missing/Zone", None),
        Err(ScheduleError::UnknownZone)
    );
    assert_eq!(
        Schedule::resolve("2026-11-07T18:30", "UTC", None)
            .unwrap()
            .offset_seconds(),
        0
    );
}
