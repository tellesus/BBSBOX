// SPDX-License-Identifier: AGPL-3.0-or-later
//! Host-attested local invitations scoped to the individual addressee.
use super::{Error, GatheringId, Gatherings, get};
use bbsbox_event_codec::{
    self as codec, CompleteEvent, Distribution, EventId, Scope, authority::Decision,
};
use bbsbox_event_core::{self as events, Fork, authoring::Action, integration::Relevance};
use bbsbox_identity::{LocalAccountId, account, account_by_username, node::NodeSigner};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};

pub(super) const KIND: &str = "gathering.invite";
const AUDIENCE: &str = "gathering-invitees";
pub const PAGE_SIZE: usize = 30;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Invitation {
    gathering: GatheringId,
    recipient: [u8; 16],
    organizer: [u8; 16],
}

impl Invitation {
    fn scope(&self) -> Scope {
        Scope {
            resource: format!(
                "invitation/{}/{}",
                self.gathering,
                hex::encode(self.recipient)
            ),
            audience: AUDIENCE.into(),
            distribution: Distribution::Local,
        }
    }
}

fn decode(event: &CompleteEvent) -> Option<Invitation> {
    let body: Invitation = serde_json::from_slice(event.body()).ok()?;
    (body.recipient != body.organizer && serde_json::to_vec(&body).ok()? == event.body())
        .then_some(body)
}

impl Gatherings {
    /// Invite an active account as the authenticated organizer. A repeat returns the same receipt.
    pub fn invite(
        &self,
        tx: &Transaction<'_>,
        signer: &NodeSigner,
        organizer: LocalAccountId,
        gathering: GatheringId,
        username: &str,
        time: i64,
    ) -> Result<EventId, Error> {
        let organizer = account(tx, organizer).map_err(Error::Identity)?;
        let item = get(tx, gathering)?.ok_or(Error::Missing)?;
        if !organizer.active
            || signer.id().bytes() != &self.node
            || organizer.id.bytes() != &item.organizer
        {
            return Err(Error::Forbidden);
        }
        if item.cancelled {
            return Err(Error::Conflict);
        }
        let recipient = account_by_username(tx, username).map_err(Error::Identity)?;
        if !recipient.active || recipient.id == organizer.id {
            return Err(Error::InvalidInput);
        }
        let body = Invitation {
            gathering,
            recipient: *recipient.id.bytes(),
            organizer: item.organizer,
        };
        if let Some((id, forked)) = existing(tx, &body)? {
            return if forked { Err(Error::Conflict) } else { Ok(id) };
        }
        let action = Action {
            kind: KIND.into(),
            schema: 1,
            scope: body.scope(),
            dependencies: vec![],
            claimed_time: time,
            extensions: vec![],
        };
        let bytes = serde_json::to_vec(&body).map_err(|_| Error::InvalidInput)?;
        self.sign(tx, signer, action, &bytes)
    }
}

fn existing(
    connection: &Connection,
    body: &Invitation,
) -> rusqlite::Result<Option<(EventId, bool)>> {
    connection
        .query_row(
            "SELECT event,stream IN (SELECT stream FROM gatherings_invite_forks)
         FROM gatherings_invites WHERE gathering=?1 AND recipient=?2",
            params![body.gathering.0.as_slice(), body.recipient.as_slice()],
            |row| Ok((EventId::from_bytes(row.get(0)?), row.get(1)?)),
        )
        .optional()
}

pub(super) fn authorize(
    connection: &Connection,
    event: &CompleteEvent,
    node: [u8; 32],
) -> events::Result<Decision> {
    let Some(body) = decode(event) else {
        return Ok(Decision::Deny("invalid_invitation"));
    };
    let metadata = event.metadata();
    if metadata.actor.subject != node
        || metadata.scope != body.scope()
        || !metadata.dependencies.is_empty()
    {
        return Ok(Decision::Deny("foreign_or_nonlocal_invitation"));
    }
    let current = get(connection, body.gathering)?;
    Ok(
        if current.is_some_and(|current| !current.cancelled && current.organizer == body.organizer)
            && existing(connection, &body)?.is_none()
        {
            Decision::Allow
        } else {
            Decision::Deny("invitation_state_changed")
        },
    )
}

pub(super) fn reduce(tx: &Transaction<'_>, event: &CompleteEvent) -> events::Result<Relevance> {
    let body = decode(event).ok_or(events::Error::CorruptRecord)?;
    let metadata = event.metadata();
    let stream = codec::stream_id(&metadata.actor, &metadata.scope)?;
    tx.execute(
        "INSERT INTO gatherings_invites VALUES (?1,?2,?3,?4,?5,?6)",
        params![
            body.gathering.0.as_slice(),
            body.recipient.as_slice(),
            body.organizer.as_slice(),
            event.id().bytes().as_slice(),
            metadata.claimed_time,
            stream.as_slice()
        ],
    )?;
    Ok(Relevance::Relevant)
}

pub(super) fn invalidate_fork(tx: &Transaction<'_>, fork: &Fork) -> events::Result<()> {
    tx.execute("INSERT OR IGNORE INTO gatherings_invite_forks SELECT stream FROM gatherings_invites WHERE stream=?1 LIMIT 1", [fork.stream.as_slice()])?;
    Ok(())
}

#[derive(Debug)]
pub struct Received {
    pub gathering: GatheringId,
    pub event: EventId,
    pub organizer: String,
    pub title: String,
    pub cancelled: bool,
    pub sent_at: i64,
}

pub fn received(
    connection: &Connection,
    recipient: LocalAccountId,
    before: Option<(i64, EventId)>,
) -> rusqlite::Result<Vec<Received>> {
    let mut query = connection.prepare(
        "SELECT i.gathering,i.event,g.username,json_extract(g.details,'$.title'),g.cancelled,i.sent_at
         FROM gatherings_invites i JOIN gatherings_current g ON g.id=i.gathering AND g.organizer=i.organizer
         WHERE i.recipient=?1 AND (?2 IS NULL OR (i.sent_at,i.event)<(?2,?3))
           AND i.stream NOT IN (SELECT stream FROM gatherings_invite_forks)
           AND g.id NOT IN (SELECT gathering FROM gatherings_quarantined)
         ORDER BY i.sent_at DESC,i.event DESC LIMIT ?4",
    )?;
    let (time, event) = before.map_or((None, None), |(time, event)| (Some(time), Some(event)));
    query
        .query_map(
            params![
                recipient.bytes().as_slice(),
                time,
                event.as_ref().map(|id| id.bytes().as_slice()),
                (PAGE_SIZE + 1) as i64
            ],
            |row| {
                Ok(Received {
                    gathering: GatheringId(row.get(0)?),
                    event: EventId::from_bytes(row.get(1)?),
                    organizer: row.get(2)?,
                    title: row.get(3)?,
                    cancelled: row.get(4)?,
                    sent_at: row.get(5)?,
                })
            },
        )?
        .collect()
}

pub fn sent_count(
    connection: &Connection,
    organizer: LocalAccountId,
    gathering: GatheringId,
) -> rusqlite::Result<i64> {
    connection.query_row(
        "SELECT count(*) FROM gatherings_invites
         WHERE gathering=?1 AND organizer=?2 AND stream NOT IN (SELECT stream FROM gatherings_invite_forks)",
        params![gathering.0.as_slice(), organizer.bytes().as_slice()],
        |row| row.get(0),
    )
}
