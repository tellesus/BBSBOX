// SPDX-License-Identifier: AGPL-3.0-or-later
//! Immutable revision bodies and a separate ordinary-view head.
use super::{Change, Error, live_board, valid_text};
use bbsbox_event_codec::{CompleteEvent, Dependency, EventId, authority::Decision, stream_id};
use bbsbox_event_core::{self as events, integration::Relevance};
use bbsbox_identity::Account;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};

pub const EDIT: &str = "discussions.post.edit";
pub const WITHDRAW: &str = "discussions.post.withdraw";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    account: [u8; 16],
    post: [u8; 32],
    previous: [u8; 32],
    text: Option<String>,
}

impl Payload {
    fn valid(&self, kind: &str) -> bool {
        match kind {
            EDIT => self
                .text
                .as_deref()
                .is_some_and(|text| valid_text(text, 48 * 1024, true)),
            WITHDRAW => self.text.is_none(),
            _ => false,
        }
    }

    fn dependencies(&self, board_id: [u8; 32]) -> Vec<Dependency> {
        let mut dependencies = [board_id, self.post, self.previous]
            .map(EventId::from_bytes)
            .map(Dependency::event)
            .to_vec();
        dependencies.sort_unstable();
        dependencies.dedup();
        dependencies
    }
}

struct Head {
    author: [u8; 16],
    id: [u8; 32],
    withdrawn: bool,
    text: Option<String>,
}

fn head(connection: &Connection, board: &str, post: &[u8; 32]) -> rusqlite::Result<Option<Head>> {
    connection.query_row(
        "SELECT e.author,coalesce(h.revision,e.id),coalesce(r.withdrawn,0),CASE WHEN r.id IS NULL THEN e.body ELSE r.body END FROM discussions_entries e LEFT JOIN discussions_heads h ON h.post=e.id LEFT JOIN discussions_revisions r ON r.id=h.revision WHERE e.board=?1 AND e.id=?2 AND e.kind IN ('thread','reply')",
        params![board, post.as_slice()], |row| Ok(Head { author: row.get(0)?, id: row.get(1)?, withdrawn: row.get(2)?, text: row.get(3)? }),
    ).optional()
}

pub(super) fn prepare(
    tx: &Transaction<'_>,
    account: &Account,
    board: &str,
    board_id: [u8; 32],
    change: Change<'_>,
) -> Result<(&'static str, Vec<Dependency>, Vec<u8>), Error> {
    let (kind, post, previous, text) = match change {
        Change::Edit {
            post,
            previous,
            text,
        } => (EDIT, post, previous, Some(text.to_owned())),
        Change::Withdraw { post, previous } => (WITHDRAW, post, previous, None),
        _ => return Err(Error::InvalidInput),
    };
    let account = *account.id.bytes();
    let current = head(tx, board, post.bytes())?.ok_or(Error::MissingParent)?;
    if current.author != account {
        return Err(Error::Forbidden);
    }
    if current.id != *previous.bytes() || current.withdrawn {
        return Err(Error::RevisionChanged);
    }
    if kind == EDIT && current.text == text {
        return Err(Error::NoChange);
    }
    let payload = Payload {
        account,
        post: *post.bytes(),
        previous: *previous.bytes(),
        text,
    };
    if !payload.valid(kind) {
        return Err(Error::InvalidInput);
    }
    let body = serde_json::to_vec(&payload).map_err(|_| Error::InvalidInput)?;
    Ok((kind, payload.dependencies(board_id), body))
}

fn decode(event: &CompleteEvent) -> Option<Payload> {
    let payload: Payload = serde_json::from_slice(event.body()).ok()?;
    if payload.valid(&event.metadata().kind) && serde_json::to_vec(&payload).ok()? == event.body() {
        Some(payload)
    } else {
        None
    }
}

pub(super) fn authorize(
    connection: &Connection,
    board: &str,
    board_id: [u8; 32],
    event: &CompleteEvent,
) -> events::Result<Decision> {
    let Some(body) = decode(event) else {
        return Ok(Decision::Deny("invalid_revision_body"));
    };
    if event.metadata().dependencies != body.dependencies(board_id) {
        return Ok(Decision::Deny("invalid_revision_dependencies"));
    }
    let Some(current) = head(connection, board, &body.post)? else {
        return Ok(Decision::Missing(vec![Dependency::event(
            EventId::from_bytes(body.post),
        )]));
    };
    Ok(
        if current.author == body.account
            && current.id == body.previous
            && !current.withdrawn
            && (event.metadata().kind != EDIT || current.text != body.text)
        {
            Decision::Allow
        } else {
            Decision::Deny("wrong_author_or_stale_revision")
        },
    )
}

pub(super) fn reduce(tx: &Transaction<'_>, event: &CompleteEvent) -> events::Result<Relevance> {
    let payload = decode(event).ok_or(events::Error::CorruptRecord)?;
    let metadata = event.metadata();
    let stream = stream_id(&metadata.actor, &metadata.scope)?;
    let id = event.id();
    tx.execute(
        "INSERT INTO discussions_revisions VALUES(?1,?2,?3,?4,?5,?6)",
        params![
            id.bytes().as_slice(),
            payload.post.as_slice(),
            payload.previous.as_slice(),
            payload.text,
            metadata.kind == WITHDRAW,
            stream.as_slice()
        ],
    )?;
    tx.execute("INSERT INTO discussions_heads VALUES(?1,?2) ON CONFLICT(post) DO UPDATE SET revision=excluded.revision", params![payload.post.as_slice(), id.bytes().as_slice()])?;
    Ok(Relevance::Relevant)
}

#[derive(Debug, PartialEq, Eq)]
pub struct Revision {
    pub previous: Option<EventId>,
    pub text: Option<String>,
}

/// Internal exact-revision lookup. A caller exposing it must authorize access to the historical body.
pub fn revision(
    connection: &Connection,
    board: &str,
    post: EventId,
    revision: EventId,
) -> Result<Revision, Error> {
    if live_board(connection, board)?.is_none() {
        return Err(Error::MissingBoard);
    }
    if revision == post {
        return connection.query_row("SELECT body FROM discussions_entries WHERE board=?1 AND id=?2 AND kind IN ('thread','reply')", params![board, post.bytes().as_slice()], |row| Ok(Revision { previous: None, text: row.get(0)? })).optional()?.ok_or(Error::MissingParent);
    }
    connection.query_row("SELECT r.previous,r.body FROM discussions_revisions r JOIN discussions_entries e ON e.id=r.post WHERE e.board=?1 AND r.post=?2 AND r.id=?3", params![board, post.bytes().as_slice(), revision.bytes().as_slice()], |row| Ok(Revision { previous: Some(EventId::from_bytes(row.get(0)?)), text: row.get(1)? })).optional()?.ok_or(Error::MissingParent)
}
