// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_identity::{account_by_username, node::NodeSigner};
use bbsbox_social::{Feed, Social, Timing, is_following, posts, set_following};
use bbsbox_storage::{Database, disk::Class, open_reader};

#[test]
fn chosen_posts_are_chronological_even_when_older_posts_arrive_later() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("social.sqlite");
    let mut registry = bbsbox_identity::migrations();
    registry.extend(bbsbox_event_core::migrations());
    registry.extend(bbsbox_social::migrations());
    let mut database = Database::open(&path, &registry).unwrap();
    let signer = NodeSigner::start(&mut database).unwrap();
    let [alice, bob, outsider] = database
        .write(Class::Control, 65536, |tx| -> bbsbox_identity::Result<_> {
            tx.execute_batch(
                "INSERT INTO identity_accounts(id,username,password_hash,role) VALUES
                 (x'01010101010101010101010101010101','alice','fixture','member'),
                 (x'02020202020202020202020202020202','bob','fixture','member'),
                 (x'03030303030303030303030303030303','outsider','fixture','member');",
            )?;
            Ok([
                account_by_username(tx, "alice")?.id,
                account_by_username(tx, "bob")?.id,
                account_by_username(tx, "outsider")?.id,
            ])
        })
        .unwrap();
    let social = Social::new(&signer);
    let reader = open_reader(&path).unwrap();
    assert!(
        posts(&reader, Feed::Following(bob), None)
            .unwrap()
            .is_empty()
    );
    let (late, newest, uncertain, tied) = database
        .write(
            Class::Content,
            256 * 1024,
            |tx| -> Result<_, bbsbox_social::Error> {
                set_following(tx, bob, alice, true)?;
                set_following(tx, bob, alice, true)?;
                let newest =
                    social.post(tx, &signer, alice, "Newer update", 1_000_000, 1_000_000)?;
                social.post(tx, &signer, outsider, "Not chosen", 2_000_000, 2_000_000)?;
                let late = social.post(tx, &signer, alice, "Late update", 100_000, 1_000_000)?;
                let uncertain =
                    social.post(tx, &signer, alice, "Clock differs", 500_000, 100_000)?;
                let tied = social.post(tx, &signer, alice, "Same time", 1_000_000, 1_000_000)?;
                Ok((late, newest, uncertain, tied))
            },
        )
        .unwrap();
    assert!(is_following(&reader, bob, alice).unwrap());
    assert!(!is_following(&reader, alice, bob).unwrap());
    let rows = posts(&reader, Feed::Following(bob), None).unwrap();
    let mut ids = [tied, newest];
    ids.sort_unstable_by(|left, right| right.bytes().cmp(left.bytes()));
    assert_eq!(
        rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        [ids[0], ids[1], uncertain, late]
    );
    assert_eq!(rows[0].timing, Timing::Ordinary);
    assert_eq!(rows[2].timing, Timing::ClockUncertain);
    assert_eq!(rows[3].timing, Timing::ArrivedLater);
    let tail = posts(
        &reader,
        Feed::Following(bob),
        Some((rows[1].authored_at, rows[1].id)),
    )
    .unwrap();
    assert_eq!(
        tail.iter().map(|row| row.id).collect::<Vec<_>>(),
        [uncertain, late]
    );
    assert!(
        posts(&reader, Feed::Post(outsider, late), None)
            .unwrap()
            .is_empty()
    );
    let focused = posts(&reader, Feed::Post(alice, late), None).unwrap();
    assert_eq!(focused[0].text, "Late update");
    assert_eq!(focused[0].utc_time.as_deref(), Some("1970-01-01T00:01:40Z"));
    database
        .write(Class::Control, 65536, |tx| {
            set_following(tx, bob, alice, false)
        })
        .unwrap();
    assert!(
        posts(&reader, Feed::Following(bob), None)
            .unwrap()
            .is_empty()
    );
    assert_eq!(posts(&reader, Feed::Author(alice), None).unwrap().len(), 4);
}
