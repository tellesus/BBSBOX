// SPDX-License-Identifier: AGPL-3.0-or-later
//! Signed local gathering announcements and causal organizer revisions.
use bbsbox_credentials::{Credential, CredentialId};
use bbsbox_event_codec::{
    self as codec, Actor, CompleteEvent, Dependency, DependencyKind, Distribution, EventId, Role,
    Scope,
    authority::{Decision, KindRule, Registry},
};
use bbsbox_event_core::{
    self as events, Delivery, Fork, Inbox, Resolver,
    authoring::{self, Action, Authored},
    integration::{self, Integration, Module, Relevance},
};
use bbsbox_identity::{LocalAccountId, node::NodeSigner, normalize_username};
use bbsbox_storage::Migration;
use rand_core::{OsRng, RngCore};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};

mod schedule;
pub use schedule::{Occurrence, Schedule, ScheduleError};

const CREATE: &str = "gathering.create";
const REVISE: &str = "gathering.revise";
const CANCEL: &str = "gathering.cancel";
const AUDIENCE: &str = "local-public";
pub const PAGE_SIZE: usize = 30;

pub fn migrations() -> [Migration; 1] {
    [Migration {
        module: "gatherings",
        version: 1,
        sql: include_str!("schema.sql"),
    }]
}

#[derive(Debug)]
pub enum Error {
    InvalidInput,
    Forbidden,
    Missing,
    Conflict,
    NotAccepted,
    Identity(bbsbox_identity::Error),
    Storage(bbsbox_storage::Error),
    Event(events::Error),
}

impl From<bbsbox_storage::Error> for Error {
    fn from(error: bbsbox_storage::Error) -> Self {
        Self::Storage(error)
    }
}
impl From<events::Error> for Error {
    fn from(error: events::Error) -> Self {
        Self::Event(error)
    }
}
impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        Self::Event(error.into())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatheringId([u8; 16]);

impl GatheringId {
    pub fn parse(value: &str) -> Option<Self> {
        if value.len() != 32
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return None;
        }
        let mut bytes = [0; 16];
        hex::decode_to_slice(value, &mut bytes).ok()?;
        Some(Self(bytes))
    }
}

impl std::fmt::Display for GatheringId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&hex::encode(self.0))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Details {
    pub title: String,
    pub place: String,
    pub description: String,
    pub schedule: Schedule,
}

impl Details {
    fn valid(&self) -> bool {
        !self.title.trim().is_empty()
            && !self.place.trim().is_empty()
            && valid_text(&self.title, 160, false)
            && valid_text(&self.place, 400, false)
            && valid_text(&self.description, 16 * 1024, true)
            && self.schedule.valid()
    }
}

fn valid_text(value: &str, limit: usize, multiline: bool) -> bool {
    value.len() <= limit
        && value
            .chars()
            .all(|c| !c.is_control() || (multiline && matches!(c, '\n' | '\t')))
}

pub enum Change {
    Create(Details),
    Revise {
        id: GatheringId,
        previous: EventId,
        details: Details,
    },
    Cancel {
        id: GatheringId,
        previous: EventId,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Body {
    gathering: GatheringId,
    organizer: [u8; 16],
    username: String,
    previous: Option<[u8; 32]>,
    details: Option<Details>,
}

impl Body {
    fn valid(&self, kind: &str) -> bool {
        normalize_username(&self.username).as_deref().ok() == Some(self.username.as_str())
            && match kind {
                CREATE => {
                    self.previous.is_none() && self.details.as_ref().is_some_and(Details::valid)
                }
                REVISE => {
                    self.previous.is_some() && self.details.as_ref().is_some_and(Details::valid)
                }
                CANCEL => self.previous.is_some() && self.details.is_none(),
                _ => false,
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

fn body(event: &CompleteEvent) -> Option<Body> {
    let body: Body = serde_json::from_slice(event.body()).ok()?;
    (body.valid(&event.metadata().kind) && serde_json::to_vec(&body).ok()? == event.body())
        .then_some(body)
}

struct LocalResolver;
impl Resolver for LocalResolver {
    fn resolve(&self, _: &Connection, _: CredentialId) -> codec::Result<Option<Credential>> {
        Ok(None)
    }
}

pub struct Gatherings {
    node: [u8; 32],
    registry: Registry,
}

impl Gatherings {
    pub fn new(signer: &NodeSigner) -> Self {
        let rules = [CREATE, REVISE, CANCEL].map(|kind| KindRule {
            kind: kind.into(),
            schema: 1,
            actor: Role::Node,
            purpose: None,
        });
        Self {
            node: *signer.id().bytes(),
            registry: Registry::new(rules.into(), vec![]).expect("valid local gathering rules"),
        }
    }

    /// The HTTP boundary supplies the authenticated local organizer inside the write transaction.
    pub fn record(
        &self,
        tx: &Transaction<'_>,
        signer: &NodeSigner,
        organizer: LocalAccountId,
        change: Change,
        time: i64,
    ) -> Result<(GatheringId, EventId), Error> {
        let account = bbsbox_identity::account(tx, organizer).map_err(Error::Identity)?;
        if !account.active || signer.id().bytes() != &self.node {
            return Err(Error::Forbidden);
        }
        let (kind, gathering, previous, details) = match change {
            Change::Create(details) => {
                let mut id = [0; 16];
                OsRng.fill_bytes(&mut id);
                (CREATE, GatheringId(id), None, Some(details))
            }
            Change::Revise {
                id,
                previous,
                details,
            } => (REVISE, id, Some(previous), Some(details)),
            Change::Cancel { id, previous } => (CANCEL, id, Some(previous), None),
        };
        if let Some(previous) = previous {
            let current = get(tx, gathering)?.ok_or(Error::Missing)?;
            if &current.organizer != account.id.bytes() {
                return Err(Error::Forbidden);
            }
            if current.revision != previous
                || current.cancelled
                || details.as_ref() == Some(&current.details)
            {
                return Err(Error::Conflict);
            }
            if details.as_ref().is_some_and(|details| {
                details.schedule != current.details.schedule
                    && !details.schedule.matches_current_rules()
            }) {
                return Err(Error::InvalidInput);
            }
        } else if details
            .as_ref()
            .is_some_and(|details| !details.schedule.matches_current_rules())
        {
            return Err(Error::InvalidInput);
        }
        let body = Body {
            gathering,
            organizer: *account.id.bytes(),
            username: account.username,
            previous: previous.map(|id| *id.bytes()),
            details,
        };
        if !body.valid(kind) {
            return Err(Error::InvalidInput);
        }
        let action = Action {
            kind: kind.into(),
            schema: 1,
            dependencies: body.dependencies(),
            claimed_time: time,
            scope: Scope {
                resource: format!("gathering/{gathering}"),
                audience: AUDIENCE.into(),
                distribution: Distribution::Local,
            },
            extensions: vec![],
        };
        let bytes = serde_json::to_vec(&body).map_err(|_| Error::InvalidInput)?;
        let actor = Actor::root(Role::Node, signer.public(), *signer.incarnation())
            .map_err(events::Error::from)?;
        let event = authoring::author(
            tx,
            Inbox::default(),
            Authored {
                action,
                actor,
                credential: None,
                body: &bytes,
                delivery: &Delivery {
                    source: "local".into(),
                    transfer: "gatherings".into(),
                },
            },
            &LocalResolver,
            &[self],
            |bytes| {
                signer
                    .sign_current(signer.incarnation(), bytes)
                    .map_err(|_| codec::Error::SignerUnavailable)
            },
        )?;
        match integration::integrate(tx, event.id(), self, &LocalResolver)? {
            Integration::Accepted(_) => Ok((gathering, event.id())),
            _ => Err(Error::NotAccepted),
        }
    }
}

impl Module for Gatherings {
    fn id(&self) -> &str {
        "gatherings"
    }
    fn registry(&self) -> &Registry {
        &self.registry
    }

    fn action(&self, connection: &Connection, event: &CompleteEvent) -> events::Result<Decision> {
        let metadata = event.metadata();
        let Some(body) = body(event) else {
            return Ok(Decision::Deny("invalid_gathering_body"));
        };
        if metadata.actor.subject != self.node
            || metadata.scope.distribution != Distribution::Local
            || metadata.scope.audience != AUDIENCE
            || metadata.scope.resource != format!("gathering/{}", body.gathering)
            || metadata.dependencies != body.dependencies()
        {
            return Ok(Decision::Deny("foreign_or_nonlocal_gathering"));
        }
        if connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM gatherings_quarantined WHERE gathering=?1)",
            [body.gathering.0.as_slice()],
            |row| row.get::<_, bool>(0),
        )? {
            return Ok(Decision::Deny("gathering_quarantined"));
        }
        let current = get(connection, body.gathering)?;
        match (current, body.previous) {
            (None, None) => Ok(Decision::Allow),
            (None, Some(_)) => Ok(Decision::Missing(metadata.dependencies.clone())),
            (Some(current), Some(previous))
                if current.organizer == body.organizer
                    && current.username == body.username
                    && !current.cancelled
                    && current.revision.bytes() == &previous
                    && body.details.as_ref() != Some(&current.details) =>
            {
                Ok(Decision::Allow)
            }
            _ => Ok(Decision::Deny("wrong_organizer_or_gathering_revision")),
        }
    }

    fn dependency(
        &self,
        _: &Connection,
        _: &CompleteEvent,
        dependency: Dependency,
    ) -> events::Result<Decision> {
        Ok(if dependency.kind == DependencyKind::Event {
            Decision::Allow
        } else {
            Decision::Deny("unexpected_gathering_dependency")
        })
    }

    fn dependency_effective(&self, connection: &Connection, id: EventId) -> events::Result<bool> {
        Ok(connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM gatherings_revisions WHERE event=?1 AND gathering NOT IN (SELECT gathering FROM gatherings_quarantined))",
            [id.bytes().as_slice()], |row| row.get(0),
        )?)
    }

    fn reduce(&self, tx: &Transaction<'_>, event: &CompleteEvent) -> events::Result<Relevance> {
        let body = body(event).ok_or(events::Error::CorruptRecord)?;
        let key = body.gathering.0.as_slice();
        let revision = event.id();
        let id = revision.bytes().as_slice();
        if let Some(details) = body.details {
            let data = serde_json::to_string(&details).map_err(|_| events::Error::CorruptRecord)?;
            tx.execute(
                "INSERT INTO gatherings_current VALUES (?1,?2,?3,?4,?5,?6,0)
                 ON CONFLICT(id) DO UPDATE SET revision=excluded.revision,details=excluded.details,starts_at=excluded.starts_at",
                params![key, id, body.organizer.as_slice(), body.username, data, details.schedule.utc_millis()],
            )?;
        } else {
            tx.execute(
                "UPDATE gatherings_current SET revision=?2,cancelled=1 WHERE id=?1",
                params![key, id],
            )?;
        }
        let metadata = event.metadata();
        let stream = codec::stream_id(&metadata.actor, &metadata.scope)?;
        tx.execute(
            "INSERT INTO gatherings_revisions VALUES (?1,?2,?3)",
            params![id, key, stream.as_slice()],
        )?;
        Ok(Relevance::Relevant)
    }

    fn invalidate_fork(&self, tx: &Transaction<'_>, fork: &Fork) -> events::Result<()> {
        tx.execute(
            "INSERT OR IGNORE INTO gatherings_quarantined SELECT gathering FROM gatherings_revisions WHERE stream=?1",
            [fork.stream.as_slice()],
        )?;
        Ok(())
    }
}

impl integration::ReplayModule for Gatherings {
    fn project(&self, tx: &Transaction<'_>, event: &CompleteEvent) -> events::Result<()> {
        self.reduce(tx, event).map(|_| ())
    }
}

#[derive(Debug)]
pub struct Gathering {
    pub id: GatheringId,
    pub revision: EventId,
    pub organizer: [u8; 16],
    pub username: String,
    pub details: Details,
    pub cancelled: bool,
}

fn gathering(row: &rusqlite::Row<'_>) -> rusqlite::Result<Gathering> {
    let data: String = row.get(4)?;
    let details = serde_json::from_str(&data).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(Gathering {
        id: GatheringId(row.get(0)?),
        revision: EventId::from_bytes(row.get(1)?),
        organizer: row.get(2)?,
        username: row.get(3)?,
        details,
        cancelled: row.get(5)?,
    })
}

pub fn get(connection: &Connection, id: GatheringId) -> rusqlite::Result<Option<Gathering>> {
    connection
        .query_row(
            "SELECT id,revision,organizer,username,details,cancelled FROM gatherings_current
         WHERE id=?1 AND id NOT IN (SELECT gathering FROM gatherings_quarantined)",
            [id.0.as_slice()],
            gathering,
        )
        .optional()
}

pub fn list(
    connection: &Connection,
    organizer: Option<LocalAccountId>,
    after: Option<(i64, GatheringId)>,
) -> rusqlite::Result<Vec<Gathering>> {
    let mut query = connection.prepare(
        "SELECT id,revision,organizer,username,details,cancelled FROM gatherings_current
         WHERE (?1 IS NULL OR organizer=?1) AND (?2 IS NULL OR (starts_at,id)>(?2,?3))
           AND id NOT IN (SELECT gathering FROM gatherings_quarantined)
         ORDER BY starts_at,id LIMIT ?4",
    )?;
    let (time, id) = after.map_or((None, None), |(time, id)| (Some(time), Some(id)));
    query
        .query_map(
            params![
                organizer.as_ref().map(|id| id.bytes().as_slice()),
                time,
                id.as_ref().map(|id| id.0.as_slice()),
                (PAGE_SIZE + 1) as i64
            ],
            gathering,
        )?
        .collect()
}
