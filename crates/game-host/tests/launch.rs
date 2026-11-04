// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_credentials::NodeId;
use bbsbox_game_host::{
    Manifest, ipc,
    sessions::{Error, PlayerId, Registry},
};
use bbsbox_identity::account_by_username;
use bbsbox_storage::clock::Clock;
use rusqlite::Connection;
use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};

#[derive(Default)]
struct ClockFixture(AtomicI64);

impl Clock for ClockFixture {
    fn wall_millis(&self) -> i64 {
        0
    }
    fn monotonic_millis(&self) -> i64 {
        self.0.load(Ordering::Relaxed)
    }
}

fn manifest(id: &str, path: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "game_id": id, "worker_version": "0.1.0", "state_schema": 1, "passport_schemas": [],
        "launch_entry": path, "mode": "turn-based-pair", "profile_components": [],
    }))
    .unwrap()
}

#[test]
fn launch_ticket_is_single_use_and_bearer_is_bound_to_one_game_and_local_player() {
    let db = Connection::open_in_memory().unwrap();
    for migration in bbsbox_identity::migrations() {
        db.execute_batch(migration.sql).unwrap();
    }
    db.execute_batch(
        "INSERT INTO identity_accounts(id,username,password_hash,role) VALUES
         (x'01010101010101010101010101010101','member','fixture','member');",
    )
    .unwrap();
    let account = account_by_username(&db, "member").unwrap().id;
    let node = NodeId::from_digest([2; 32]);
    let game = Manifest::parse(&manifest("demo", "index.html")).unwrap();
    let other_game = Manifest::parse(&manifest("other", "index.html")).unwrap();
    assert!(Manifest::parse(&manifest("demo", "../private.html")).is_err());
    assert!(Manifest::parse(&manifest("demo", "https://remote.invalid/index.html")).is_err());
    let clock = Arc::new(ClockFixture::default());
    let mut grants = Registry::new(clock.clone());
    let ticket = grants.launch(node, &game, account, 1).unwrap();
    assert!(matches!(
        grants.exchange(&other_game, ticket.expose()),
        Err(Error::Denied)
    ));
    assert_eq!(grants.authorize(&game, ticket.expose()), Err(Error::Denied));
    let bearer = grants.exchange(&game, ticket.expose()).unwrap();
    assert!(matches!(
        grants.exchange(&game, ticket.expose()),
        Err(Error::Denied)
    ));
    assert_eq!(
        grants.authorize(&other_game, bearer.expose()),
        Err(Error::Denied)
    );
    let authorized = grants.authorize(&game, bearer.expose()).unwrap();
    let player = authorized.player;
    assert_eq!((authorized.account, authorized.parent), (account, 1));
    assert_eq!(player, PlayerId::for_account(node, &game, account));
    assert_ne!(player, PlayerId::for_account(node, &other_game, account));
    assert_ne!(
        player,
        PlayerId::for_account(NodeId::from_digest([3; 32]), &game, account)
    );
    let expired = grants.launch(node, &game, account, 2).unwrap();
    clock.0.store(120_000, Ordering::Relaxed);
    assert!(matches!(
        grants.exchange(&game, expired.expose()),
        Err(Error::Denied)
    ));
    assert_eq!(grants.authorize(&game, bearer.expose()), Ok(authorized));
    clock.0.store(900_000, Ordering::Relaxed);
    assert_eq!(grants.authorize(&game, bearer.expose()), Err(Error::Denied));
}

#[test]
fn framing_refuses_large_and_truncated_messages() {
    let mut stream = vec![];
    ipc::send(&mut stream, &"fixture").unwrap();
    assert_eq!(
        ipc::receive::<String>(&mut stream.as_slice())
            .unwrap()
            .as_deref(),
        Some("fixture")
    );
    assert!(ipc::receive::<String>(&mut &stream[..stream.len() - 1]).is_err());
    assert!(ipc::receive::<String>(&mut 65537u32.to_be_bytes().as_slice()).is_err());
    assert!(ipc::send(&mut Vec::new(), &"x".repeat(65536)).is_err());
}
