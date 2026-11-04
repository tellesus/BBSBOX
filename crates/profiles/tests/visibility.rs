// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_profiles::{
    Field, Preferences,
    components::{Display, Link, Providers, Selection},
    editor, migrations, profile, save,
};
use rusqlite::Connection;
use std::sync::Arc;

#[test]
fn display_requires_the_owners_selection_and_the_providers_audience() {
    let mut connection = Connection::open_in_memory().unwrap();
    for migration in bbsbox_identity::migrations() {
        connection.execute_batch(migration.sql).unwrap();
    }
    connection
        .execute_batch(
            "INSERT INTO identity_accounts(id,username,password_hash,role,active) VALUES
         (x'01010101010101010101010101010101','owner','fixture','member',1),
         (x'02020202020202020202020202020202','visitor','fixture','sysop',1),
         (x'03030303030303030303030303030303','inactive','fixture','member',0);",
        )
        .unwrap();
    let [initial, components] = migrations();
    connection.execute_batch(initial.sql).unwrap();
    connection
        .execute_batch(
            "INSERT INTO profiles_local VALUES
         (x'01010101010101010101010101010101',1,1,'Existing bio','','','','',1,0,0,0,0);",
        )
        .unwrap();
    connection.execute_batch(components.sql).unwrap();
    assert_eq!(
        bbsbox_identity::directory_usernames(&connection, None).unwrap(),
        ["inactive", "owner", "visitor"]
    );
    let owner = bbsbox_identity::account_by_username(&connection, "owner")
        .unwrap()
        .id;
    let visitor = bbsbox_identity::account_by_username(&connection, "visitor")
        .unwrap()
        .id;
    let tx = connection.transaction().unwrap();
    let previous = editor(&tx, owner).unwrap();
    assert_eq!(previous.revision, 1);
    assert_eq!(previous.preferences.bio.text, "Existing bio");
    assert_eq!(previous.preferences.components, Selection::default());
    let absent = profile(&tx, "owner", visitor, &Providers::default()).unwrap();
    assert_eq!(absent.components, Display::default());
    assert_eq!(absent.fields.bio.as_deref(), Some("Existing bio"));
    let providers = Providers {
        feed: Some(Arc::new(move |_, context| {
            assert_eq!(context.profile, owner);
            Ok((context.account == owner).then(|| Link::new("Feed", "/feeds/owner/").unwrap()))
        })),
        events: Some(Arc::new(|_, _| panic!("hidden events must stay unread"))),
        games: Some(Arc::new(|_, _| {
            Ok(vec![
                Link::new("Town Quest · Ada", "/games/town-quest/").unwrap(),
            ])
        })),
        drops: Some(Arc::new(|_, _| Ok(Some(0)))),
    };
    let mut settings = Preferences {
        public: true,
        email: Field {
            text: "never return this email".into(),
            publish: false,
        },
        components: Selection {
            feed: true,
            events: false,
            games: true,
            drops: true,
        },
        ..previous.preferences
    };
    save(&tx, owner, 1, &settings).unwrap();
    let visible = profile(&tx, "owner", visitor, &providers).unwrap();
    assert!(visible.fields.email.is_none());
    assert!(visible.components.feed.is_none());
    assert_eq!(visible.components.games.len(), 1);
    assert_eq!(visible.components.drops, Some(0));
    assert!(
        profile(&tx, "owner", owner, &providers)
            .unwrap()
            .components
            .feed
            .is_some()
    );

    settings.public = false;
    save(&tx, owner, 2, &settings).unwrap();
    let private = profile(&tx, "owner", visitor, &providers).unwrap();
    assert!(private.fields.bio.is_none());
    assert!(private.components.games.is_empty());
    assert!(private.components.feed.is_none());
    assert_eq!(private.components.drops, Some(0));

    settings.components.drops = false;
    save(&tx, owner, 3, &settings).unwrap();
    let private_count = profile(&tx, "owner", visitor, &providers).unwrap();
    assert_eq!(private_count.components, Display::default());
    assert!(Link::new("External", "//outside.invalid").is_none());
    assert!(Link::new("External", "/\\outside.invalid").is_none());
}
