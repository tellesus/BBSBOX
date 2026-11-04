// SPDX-License-Identifier: AGPL-3.0-or-later
//! Mutual, node-local friendship. Following remains an independent choice.
use super::Error;
use bbsbox_identity::LocalAccountId;
use rand_core::{OsRng, RngCore};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Incoming,
    Outgoing,
    Friends,
}

#[derive(Debug)]
pub struct Relationship {
    pub status: Status,
    pub generation: [u8; 16],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Request,
    Accept,
    Decline,
    Cancel,
    Remove,
}

impl Action {
    pub fn parse(input: &str) -> Option<Self> {
        match input {
            "request" => Some(Self::Request),
            "accept" => Some(Self::Accept),
            "decline" => Some(Self::Decline),
            "cancel" => Some(Self::Cancel),
            "remove" => Some(Self::Remove),
            _ => None,
        }
    }
}

fn ordered(left: LocalAccountId, right: LocalAccountId) -> (LocalAccountId, LocalAccountId) {
    if left.bytes() < right.bytes() {
        (left, right)
    } else {
        (right, left)
    }
}

fn status(confirmed: bool, requester: [u8; 16], viewer: LocalAccountId) -> Status {
    if confirmed {
        Status::Friends
    } else if &requester == viewer.bytes() {
        Status::Outgoing
    } else {
        Status::Incoming
    }
}

pub fn relationship(
    connection: &Connection,
    viewer: LocalAccountId,
    other: LocalAccountId,
) -> rusqlite::Result<Option<Relationship>> {
    let (first, second) = ordered(viewer, other);
    connection.query_row(
        "SELECT requester,generation,confirmed FROM social_friends WHERE first=?1 AND second=?2",
        params![first.bytes().as_slice(), second.bytes().as_slice()],
        |row| Ok(Relationship {
            status: status(row.get(2)?, row.get(0)?, viewer),
            generation: row.get(1)?,
        }),
    ).optional()
}

/// The caller authenticates the acting account inside this writer transaction.
pub fn change(
    tx: &Transaction<'_>,
    actor: LocalAccountId,
    other: LocalAccountId,
    action: Action,
    generation: Option<[u8; 16]>,
) -> Result<(), Error> {
    let acting = bbsbox_identity::account(tx, actor).map_err(Error::Identity)?;
    let target = bbsbox_identity::account(tx, other).map_err(Error::Identity)?;
    if actor == other
        || !acting.active
        || (!target.active && matches!(action, Action::Request | Action::Accept))
    {
        return Err(Error::Forbidden);
    }
    let current = relationship(tx, actor, other)?;
    if current.as_ref().map(|row| row.generation) != generation {
        return Err(Error::Conflict);
    }
    let (first, second) = ordered(actor, other);
    match (current.map(|row| row.status), action) {
        (None, Action::Request) => {
            let mut generation = [0; 16];
            OsRng.fill_bytes(&mut generation);
            tx.execute(
                "INSERT INTO social_friends VALUES (?1,?2,?3,?4,0)",
                params![
                    first.bytes().as_slice(),
                    second.bytes().as_slice(),
                    actor.bytes().as_slice(),
                    generation.as_slice()
                ],
            )?;
        }
        (Some(Status::Incoming), Action::Accept) => {
            let mut generation = [0; 16];
            OsRng.fill_bytes(&mut generation);
            tx.execute(
                "UPDATE social_friends SET confirmed=1,generation=?3 WHERE first=?1 AND second=?2",
                params![
                    first.bytes().as_slice(),
                    second.bytes().as_slice(),
                    generation.as_slice()
                ],
            )?;
        }
        (Some(Status::Incoming), Action::Decline)
        | (Some(Status::Outgoing), Action::Cancel)
        | (Some(Status::Friends), Action::Remove) => {
            tx.execute(
                "DELETE FROM social_friends WHERE first=?1 AND second=?2",
                params![first.bytes().as_slice(), second.bytes().as_slice()],
            )?;
        }
        _ => return Err(Error::Forbidden),
    }
    Ok(())
}

pub struct Entry {
    pub username: String,
    pub status: Status,
}

pub fn list(
    connection: &Connection,
    viewer: LocalAccountId,
    after: Option<&str>,
) -> rusqlite::Result<Vec<Entry>> {
    let mut query = connection.prepare(
        "WITH connections AS (
            SELECT second AS other,requester,confirmed FROM social_friends WHERE first=?1
            UNION ALL
            SELECT first AS other,requester,confirmed FROM social_friends WHERE second=?1
         )
         SELECT a.username,c.requester,c.confirmed FROM connections c
         JOIN identity_accounts a ON a.id=c.other
         WHERE a.username>coalesce(?2,'') ORDER BY a.username LIMIT 51",
    )?;
    query
        .query_map(params![viewer.bytes().as_slice(), after], |row| {
            Ok(Entry {
                username: row.get(0)?,
                status: status(row.get(2)?, row.get(1)?, viewer),
            })
        })?
        .collect()
}
