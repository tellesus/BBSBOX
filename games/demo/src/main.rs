// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_game_host::{
    Manifest, ipc,
    protocol::{Move, Operation, Reason, Reply, Request, ResultBody, State, VERSION},
    sessions::PlayerId,
};
use rusqlite::{Connection, OpenFlags, OptionalExtension, Transaction, params};
use std::{
    error::Error,
    io,
    path::{Path, PathBuf},
    time::Duration,
};

mod checkpoint;

const SCHEMA: i64 = 1;
const APPLICATION: i64 = 0x4242474d;
const MANIFEST: &[u8] = include_bytes!("../manifest.json");

fn main() {
    if run().is_err() {
        eprintln!("The local demo worker could not continue.");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let checkpoint_only = match arguments.next().as_deref().and_then(|arg| arg.to_str()) {
        Some("--state") => false,
        Some("--checkpoint-state") => true,
        _ => return Err("expected a worker state path".into()),
    };
    let state = PathBuf::from(arguments.next().ok_or("missing worker state path")?);
    if !state.is_absolute() || arguments.next().is_some() {
        return Err("invalid worker state path".into());
    }
    let mut lock_name = state.as_os_str().to_owned();
    lock_name.push(".lock");
    let worker_lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_name)?;
    worker_lock.try_lock()?;
    let manifest = Manifest::parse(MANIFEST).map_err(|_| "invalid embedded manifest")?;
    let mut db = open(&state, checkpoint_only)?;
    let mut checkpoint =
        checkpoint::Manager::new(state.parent().ok_or("missing game state directory")?);
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    while let Some(request) = ipc::receive::<Request>(&mut input)? {
        let result = if request.version != VERSION || request.id == [0; 16] {
            refused(Reason::Unsupported)
        } else {
            match request.operation {
                Operation::Pause { fence } => checkpoint.pause(fence),
                Operation::Checkpoint { fence } => checkpoint.capture(&db, &manifest, fence),
                Operation::Resume { fence } => checkpoint.resume(fence),
                Operation::Join { .. } | Operation::Act { .. }
                    if checkpoint_only || checkpoint.paused() =>
                {
                    refused(Reason::Paused)
                }
                operation => dispatch(&mut db, &manifest, operation)
                    .unwrap_or_else(|_| refused(Reason::Unavailable)),
            }
        };
        ipc::send(
            &mut output,
            &Reply {
                version: VERSION,
                id: request.id,
                result,
            },
        )?;
    }
    Ok(())
}

fn open(path: &Path, checkpoint_only: bool) -> rusqlite::Result<Connection> {
    let mut db = if checkpoint_only {
        Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?
    } else {
        Connection::open(path)?
    };
    db.busy_timeout(Duration::from_secs(2))?;
    let app: i64 = db.pragma_query_value(None, "application_id", |row| row.get(0))?;
    let schema: i64 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if !checkpoint_only && app == 0 && schema == 0 {
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if tx.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_schema)", [], |row| {
            row.get::<_, bool>(0)
        })? {
            return Err(rusqlite::Error::InvalidQuery);
        }
        tx.execute_batch(
            "CREATE TABLE players(player BLOB PRIMARY KEY CHECK(length(player)=16), seat INTEGER UNIQUE CHECK(seat IN (0,1))) STRICT;
             CREATE TABLE dungeon(turn INTEGER NOT NULL, chamber INTEGER NOT NULL, stamina INTEGER NOT NULL) STRICT;
             INSERT INTO dungeon VALUES (0,0,3);
             CREATE TABLE receipts(player BLOB NOT NULL REFERENCES players(player), id BLOB NOT NULL CHECK(length(id)=16),
                action TEXT NOT NULL, result TEXT NOT NULL, PRIMARY KEY(player,id)) STRICT;",
        )?;
        tx.pragma_update(None, "application_id", APPLICATION)?;
        tx.pragma_update(None, "user_version", SCHEMA)?;
        tx.commit()?;
    } else if app != APPLICATION || schema != SCHEMA {
        return Err(rusqlite::Error::InvalidQuery);
    }
    db.pragma_update(None, "journal_mode", "WAL")?;
    db.pragma_update(None, "synchronous", "FULL")?;
    db.pragma_update(None, "foreign_keys", "ON")?;
    Ok(db)
}

fn refused(reason: Reason) -> ResultBody {
    ResultBody::Refused { reason }
}

fn seat(db: &Connection, player: PlayerId) -> rusqlite::Result<Option<u8>> {
    db.query_row(
        "SELECT seat FROM players WHERE player=?1",
        [player.bytes().as_slice()],
        |row| row.get(0),
    )
    .optional()
}

fn read(db: &Connection, seat: u8) -> rusqlite::Result<State> {
    db.query_row(
        "SELECT turn,chamber,stamina,(SELECT count(*) FROM players) FROM dungeon",
        [],
        |row| {
            let chamber = row.get(1)?;
            Ok(State {
                seat,
                players: row.get(3)?,
                turn: row.get(0)?,
                chamber,
                stamina: row.get(2)?,
                finished: chamber == 3,
            })
        },
    )
}

fn dispatch(
    db: &mut Connection,
    manifest: &Manifest,
    operation: Operation,
) -> rusqlite::Result<ResultBody> {
    match operation {
        Operation::Describe => Ok(ResultBody::Hello {
            manifest: manifest.clone(),
        }),
        Operation::Read { player } => match seat(db, player)? {
            Some(seat) => Ok(ResultBody::State {
                state: read(db, seat)?,
            }),
            None => Ok(refused(Reason::NotJoined)),
        },
        Operation::Join { player } => {
            let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let seat = if let Some(seat) = seat(&tx, player)? {
                seat
            } else {
                let count: u8 =
                    tx.query_row("SELECT count(*) FROM players", [], |row| row.get(0))?;
                if count >= 2 {
                    return Ok(refused(Reason::Full));
                }
                tx.execute(
                    "INSERT INTO players VALUES (?1,?2)",
                    params![player.bytes().as_slice(), count],
                )?;
                count
            };
            let state = read(&tx, seat)?;
            tx.commit()?;
            Ok(ResultBody::State { state })
        }
        Operation::Act {
            player,
            command,
            action,
        } => {
            let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let result = act(&tx, player, command, action)?;
            tx.commit()?;
            Ok(result)
        }
        Operation::Pause { .. } | Operation::Checkpoint { .. } | Operation::Resume { .. } => {
            Ok(refused(Reason::Unsupported))
        }
    }
}

fn act(
    tx: &Transaction<'_>,
    player: PlayerId,
    command: [u8; 16],
    action: Move,
) -> rusqlite::Result<ResultBody> {
    if command == [0; 16] {
        return Ok(refused(Reason::InvalidMove));
    }
    let Some(seat) = seat(tx, player)? else {
        return Ok(refused(Reason::NotJoined));
    };
    let receipt: Option<(String, String)> = tx
        .query_row(
            "SELECT action,result FROM receipts WHERE player=?1 AND id=?2",
            params![player.bytes().as_slice(), command.as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    if let Some((previous, state)) = receipt {
        if previous != action.key() {
            return Ok(refused(Reason::ChangedRetry));
        }
        let state = serde_json::from_str(&state).map_err(|_| rusqlite::Error::InvalidQuery)?;
        return Ok(ResultBody::State { state });
    }
    let mut state = read(tx, seat)?;
    if state.players != 2 {
        return Ok(refused(Reason::Waiting));
    }
    if state.finished {
        return Ok(refused(Reason::Finished));
    }
    if state.turn != seat {
        return Ok(refused(Reason::OutOfTurn));
    }
    match action {
        Move::Explore if state.stamina > 0 => {
            state.stamina -= 1;
            state.chamber += 1;
        }
        Move::Rest if state.stamina < 3 => state.stamina += 1,
        _ => return Ok(refused(Reason::InvalidMove)),
    }
    state.finished = state.chamber == 3;
    state.turn = 1 - state.turn;
    tx.execute(
        "UPDATE dungeon SET turn=?1,chamber=?2,stamina=?3",
        params![state.turn, state.chamber, state.stamina],
    )?;
    let encoded = serde_json::to_string(&state).map_err(|_| rusqlite::Error::InvalidQuery)?;
    tx.execute(
        "INSERT INTO receipts VALUES(?1,?2,?3,?4)",
        params![
            player.bytes().as_slice(),
            command.as_slice(),
            action.key(),
            encoded
        ],
    )?;
    Ok(ResultBody::State { state })
}
