// SPDX-License-Identifier: AGPL-3.0-or-later
//! Private local RSVP choices, aggregate attendance and on-node reminders.
use super::{Error, GatheringId, Gatherings, get};
use bbsbox_event_codec::{
    self as codec, CompleteEvent, Dependency, Distribution, EventId, Scope, authority::Decision,
};
use bbsbox_event_core::{self as events, Fork, authoring::Action, integration::Relevance};
use bbsbox_identity::{LocalAccountId, account, node::NodeSigner};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};

pub(super) const KIND: &str = "gathering.rsvp";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Choice {
    Going,
    Maybe,
    NotGoing,
}

impl Choice {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "going" => Some(Self::Going),
            "maybe" => Some(Self::Maybe),
            "not-going" => Some(Self::NotGoing),
            _ => None,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Going => "going",
            Self::Maybe => "maybe",
            Self::NotGoing => "not-going",
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Body {
    gathering: GatheringId,
    account: [u8; 16],
    choice: Choice,
    previous: Option<[u8; 32]>,
}

impl Body {
    fn scope(&self) -> Scope {
        Scope {
            resource: format!("rsvp/{}/{}", self.gathering, hex::encode(self.account)),
            audience: "gathering-rsvp".into(),
            distribution: Distribution::Local,
        }
    }

    fn dependencies(&self) -> Vec<Dependency> {
        self.previous
            .map(EventId::from_bytes)
            .map(Dependency::event)
            .into_iter()
            .collect()
    }
}

fn decode(event: &CompleteEvent) -> Option<Body> {
    let body: Body = serde_json::from_slice(event.body()).ok()?;
    (serde_json::to_vec(&body).ok()? == event.body()).then_some(body)
}

#[derive(Clone, Copy)]
pub struct Response {
    pub gathering: GatheringId,
    pub choice: Choice,
    pub previous: Option<EventId>,
}

#[derive(Clone, Copy)]
pub struct Rsvp {
    pub choice: Choice,
    pub revision: EventId,
}

impl Gatherings {
    pub fn respond(
        &self,
        tx: &Transaction<'_>,
        signer: &NodeSigner,
        responder: LocalAccountId,
        response: Response,
        time: i64,
    ) -> Result<EventId, Error> {
        let responder = account(tx, responder).map_err(Error::Identity)?;
        if !responder.active || signer.id().bytes() != &self.node {
            return Err(Error::Forbidden);
        }
        if get(tx, response.gathering)?
            .ok_or(Error::Missing)?
            .cancelled
        {
            return Err(Error::Conflict);
        }
        let head = own(tx, response.gathering, responder.id)?;
        if head.map(|head| head.revision) != response.previous {
            return Err(Error::Conflict);
        }
        if let Some(head) = head.filter(|head| head.choice == response.choice) {
            return Ok(head.revision);
        }
        let body = Body {
            gathering: response.gathering,
            account: *responder.id.bytes(),
            choice: response.choice,
            previous: response.previous.map(|id| *id.bytes()),
        };
        let action = Action {
            kind: KIND.into(),
            schema: 1,
            scope: body.scope(),
            dependencies: body.dependencies(),
            claimed_time: time,
            extensions: vec![],
        };
        let bytes = serde_json::to_vec(&body).map_err(|_| Error::InvalidInput)?;
        self.sign(tx, signer, action, &bytes)
    }
}

fn current(
    connection: &Connection,
    gathering: GatheringId,
    account: &[u8; 16],
) -> rusqlite::Result<Option<Rsvp>> {
    connection
        .query_row(
            "SELECT choice,revision FROM gatherings_rsvps WHERE gathering=?1 AND account=?2
          AND NOT EXISTS (SELECT 1 FROM gatherings_rsvp_forks WHERE gathering=?1 AND account=?2)",
            params![gathering.0.as_slice(), account.as_slice()],
            |row| {
                let value: String = row.get(0)?;
                Ok(Rsvp {
                    choice: Choice::parse(&value).ok_or(rusqlite::Error::InvalidQuery)?,
                    revision: EventId::from_bytes(row.get(1)?),
                })
            },
        )
        .optional()
}

pub fn own(
    connection: &Connection,
    gathering: GatheringId,
    account: LocalAccountId,
) -> rusqlite::Result<Option<Rsvp>> {
    current(connection, gathering, account.bytes())
}

pub(super) fn authorize(
    connection: &Connection,
    event: &CompleteEvent,
    node: [u8; 32],
) -> events::Result<Decision> {
    let Some(body) = decode(event) else {
        return Ok(Decision::Deny("invalid_rsvp"));
    };
    let metadata = event.metadata();
    if metadata.actor.subject != node
        || metadata.scope != body.scope()
        || metadata.dependencies != body.dependencies()
    {
        return Ok(Decision::Deny("foreign_or_nonlocal_rsvp"));
    }
    let event_is_active = get(connection, body.gathering)?.is_some_and(|item| !item.cancelled);
    let head = current(connection, body.gathering, &body.account)?;
    let forked = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM gatherings_rsvp_forks WHERE gathering=?1 AND account=?2)",
        params![body.gathering.0.as_slice(), body.account.as_slice()],
        |row| row.get::<_, bool>(0),
    )?;
    Ok(
        if event_is_active
            && !forked
            && head.map(|head| *head.revision.bytes()) == body.previous
            && head.is_none_or(|head| head.choice != body.choice)
        {
            Decision::Allow
        } else {
            Decision::Deny("rsvp_state_changed")
        },
    )
}

pub(super) fn reduce(tx: &Transaction<'_>, event: &CompleteEvent) -> events::Result<Relevance> {
    let body = decode(event).ok_or(events::Error::CorruptRecord)?;
    let metadata = event.metadata();
    let stream = codec::stream_id(&metadata.actor, &metadata.scope)?;
    tx.execute(
        "INSERT INTO gatherings_rsvps VALUES (?1,?2,?3,?4)
         ON CONFLICT(gathering,account) DO UPDATE SET choice=excluded.choice,revision=excluded.revision",
        params![body.gathering.0.as_slice(), body.account.as_slice(), body.choice.key(), event.id().bytes().as_slice()],
    )?;
    tx.execute(
        "INSERT INTO gatherings_rsvp_revisions VALUES (?1,?2,?3,?4)",
        params![
            event.id().bytes().as_slice(),
            body.gathering.0.as_slice(),
            body.account.as_slice(),
            stream.as_slice()
        ],
    )?;
    Ok(Relevance::Relevant)
}

pub(super) fn invalidate_fork(tx: &Transaction<'_>, fork: &Fork) -> events::Result<()> {
    tx.execute(
        "INSERT OR IGNORE INTO gatherings_rsvp_forks SELECT gathering,account FROM gatherings_rsvp_revisions WHERE stream=?1",
        [fork.stream.as_slice()],
    )?;
    Ok(())
}

pub fn counts(connection: &Connection, gathering: GatheringId) -> rusqlite::Result<[i64; 3]> {
    connection.query_row(
        "SELECT count(*) FILTER (WHERE choice='going'),count(*) FILTER (WHERE choice='maybe'),count(*) FILTER (WHERE choice='not-going')
           FROM gatherings_rsvps r WHERE gathering=?1
             AND NOT EXISTS (SELECT 1 FROM gatherings_rsvp_forks f WHERE f.gathering=r.gathering AND f.account=r.account)",
        [gathering.0.as_slice()], |row| Ok([row.get(0)?, row.get(1)?, row.get(2)?]),
    )
}

pub fn set_reminder(
    tx: &Transaction<'_>,
    account: LocalAccountId,
    gathering: GatheringId,
    minutes: Option<i64>,
) -> Result<(), Error> {
    if get(tx, gathering)?.ok_or(Error::Missing)?.cancelled {
        return Err(Error::Conflict);
    }
    match minutes {
        Some(minutes @ (15 | 60 | 1440)) => {
            tx.execute(
                "INSERT INTO gatherings_reminders VALUES (?1,?2,?3)
                 ON CONFLICT(account,gathering) DO UPDATE SET minutes=excluded.minutes",
                params![account.bytes().as_slice(), gathering.0.as_slice(), minutes],
            )?;
        }
        None => {
            tx.execute(
                "DELETE FROM gatherings_reminders WHERE account=?1 AND gathering=?2",
                params![account.bytes().as_slice(), gathering.0.as_slice()],
            )?;
        }
        Some(_) => return Err(Error::InvalidInput),
    }
    Ok(())
}

pub fn reminder(
    connection: &Connection,
    account: LocalAccountId,
    gathering: GatheringId,
) -> rusqlite::Result<Option<i64>> {
    connection
        .query_row(
            "SELECT minutes FROM gatherings_reminders WHERE account=?1 AND gathering=?2",
            params![account.bytes().as_slice(), gathering.0.as_slice()],
            |row| row.get(0),
        )
        .optional()
}

pub fn due(
    connection: &Connection,
    account: LocalAccountId,
    now: i64,
) -> rusqlite::Result<Vec<(GatheringId, String)>> {
    let mut query = connection.prepare(
        "SELECT g.id,json_extract(g.details,'$.title')
           FROM gatherings_reminders r JOIN gatherings_current g ON g.id=r.gathering
          WHERE r.account=?1 AND g.cancelled=0 AND g.starts_at>?2 AND g.starts_at-r.minutes*60000<=?2
            AND g.id NOT IN (SELECT gathering FROM gatherings_quarantined)
          ORDER BY g.starts_at,g.id LIMIT 30",
    )?;
    query
        .query_map(params![account.bytes().as_slice(), now], |row| {
            Ok((GatheringId(row.get(0)?), row.get(1)?))
        })?
        .collect()
}
