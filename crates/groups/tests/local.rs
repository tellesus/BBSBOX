// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_credentials::NodeId;
use bbsbox_groups::membership::{self, Join, Management};
use bbsbox_groups::{Details, Error, Kind, Role, create, directory, edit, view};
use bbsbox_identity::account_by_username;
use rusqlite::Connection;

#[test]
fn local_group_directory_and_roster_respect_node_and_member_boundaries() {
    let mut connection = Connection::open_in_memory().unwrap();
    for migration in bbsbox_identity::migrations()
        .into_iter()
        .chain(bbsbox_groups::migrations())
    {
        connection.execute_batch(migration.sql).unwrap();
    }
    connection
        .execute_batch(
            "INSERT INTO identity_accounts(id,username,password_hash,role) VALUES
         (x'01010101010101010101010101010101','creator','fixture','member'),
         (x'02020202020202020202020202020202','operator','fixture','sysop');",
        )
        .unwrap();
    let creator = account_by_username(&connection, "creator").unwrap().id;
    let operator = account_by_username(&connection, "operator").unwrap().id;
    let home = NodeId::from_digest([7; 32]);
    let foreign = NodeId::from_digest([8; 32]);
    let tx = connection.transaction().unwrap();
    let mut details = Details {
        name: "Library clan".into(),
        description: String::new(),
        public_roster: false,
    };
    let id = create(&tx, home, creator, Kind::Clan, &details).unwrap();
    assert_eq!(
        directory(&tx, home, Some(Kind::Clan), None).unwrap()[0].id,
        id
    );
    assert!(
        directory(&tx, home, Some(Kind::Cohort), None)
            .unwrap()
            .is_empty()
    );
    assert!(directory(&tx, foreign, None, None).unwrap().is_empty());
    assert!(matches!(
        view(&tx, foreign, id, creator, None),
        Err(Error::Missing)
    ));
    assert!(
        view(&tx, home, id, operator, None)
            .unwrap()
            .roster
            .is_none()
    );
    assert_eq!(
        view(&tx, home, id, creator, None).unwrap().roster.unwrap()[0].role,
        Role::Creator
    );
    details.public_roster = true;
    assert!(matches!(
        edit(&tx, home, operator, id, 1, &details),
        Err(Error::Forbidden)
    ));
    edit(&tx, home, creator, id, 1, &details).unwrap();
    assert!(matches!(
        edit(&tx, home, creator, id, 1, &details),
        Err(Error::Conflict)
    ));
    assert_eq!(
        view(&tx, home, id, operator, None).unwrap().roster.unwrap()[0].username,
        "creator"
    );
    assert!(matches!(
        membership::invite(&tx, home, id, operator, "creator"),
        Err(Error::Forbidden)
    ));
    membership::invite(&tx, home, id, creator, "operator").unwrap();
    assert!(
        membership::received(&tx, home, creator, None)
            .unwrap()
            .is_empty()
    );
    assert!(
        membership::received(&tx, foreign, operator, None)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        membership::received(&tx, home, operator, None).unwrap()[0].invited_by,
        "creator"
    );
    assert_eq!(
        membership::local_role(&tx, home, id, operator).unwrap(),
        None
    );
    let generation = view(&tx, home, id, operator, None)
        .unwrap()
        .invitation
        .unwrap();
    details.public_roster = false;
    edit(&tx, home, creator, id, 2, &details).unwrap();
    assert!(matches!(
        membership::decide(
            &tx,
            home,
            id,
            operator,
            Join {
                generation,
                displayed_profile_revision: 2,
                accept: true
            }
        ),
        Err(Error::Conflict)
    ));
    membership::decide(
        &tx,
        home,
        id,
        operator,
        Join {
            generation,
            displayed_profile_revision: 3,
            accept: true,
        },
    )
    .unwrap();
    assert_eq!(
        membership::local_role(&tx, home, id, operator).unwrap(),
        Some(Role::Member)
    );
    assert_eq!(
        membership::local_role(&tx, foreign, id, operator).unwrap(),
        None
    );
    assert!(matches!(
        membership::manage(&tx, home, id, creator, 2, Management::Leave),
        Err(Error::Conflict)
    ));
    membership::manage(&tx, home, id, creator, 2, Management::Handover("operator")).unwrap();
    assert_eq!(
        membership::local_role(&tx, home, id, creator).unwrap(),
        Some(Role::Member)
    );
    assert_eq!(
        membership::local_role(&tx, home, id, operator).unwrap(),
        Some(Role::Manager)
    );
    membership::manage(&tx, home, id, creator, 3, Management::Leave).unwrap();
    membership::manage(&tx, home, id, operator, 4, Management::Dissolve).unwrap();
    assert_eq!(
        membership::local_role(&tx, home, id, operator).unwrap(),
        None
    );
    assert!(directory(&tx, home, None, None).unwrap().is_empty());
}
