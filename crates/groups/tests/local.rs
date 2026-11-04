// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_credentials::NodeId;
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
}
