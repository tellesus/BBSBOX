// SPDX-License-Identifier: AGPL-3.0-or-later
//! Public local updates and each resident's chosen chronological view.
use bbsbox_credentials::{Credential, CredentialId};
use bbsbox_event_codec::{
    self as codec, Actor, CompleteEvent, Dependency, Distribution, EventId, Role, Scope,
    authority::{Decision, KindRule, Registry},
};
use bbsbox_event_core::{
    self as events, Delivery, Fork, Inbox, Resolver,
    authoring::{self, Action, Authored},
    integration::{self, Integration, Module, Relevance},
};
use bbsbox_identity::{LocalAccountId, node::NodeSigner, normalize_username};
use bbsbox_storage::Migration;
use rusqlite::{Connection, Transaction, params};
use serde::{Deserialize, Serialize};

const KIND: &str = "social.update";
const AUDIENCE: &str = "local-public";
pub const PAGE_SIZE: usize = 30;
const CLOCK_LABEL_MILLIS: i64 = 5 * 60 * 1000;

pub fn migrations() -> [Migration; 1] {
    [Migration {
        module: "social",
        version: 1,
        sql: include_str!("schema.sql"),
    }]
}

#[derive(Debug)]
pub enum Error {
    InvalidInput,
    Forbidden,
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

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    account: [u8; 16],
    username: String,
    text: String,
}

impl Payload {
    fn valid(&self) -> bool {
        normalize_username(&self.username).as_deref().ok() == Some(self.username.as_str())
            && !self.text.trim().is_empty()
            && self.text.len() <= 16 * 1024
            && self
                .text
                .chars()
                .all(|c| !c.is_control() || matches!(c, '\n' | '\t'))
    }

    fn resource(&self) -> String {
        format!("feed/{}", hex::encode(self.account))
    }
}

fn payload(event: &CompleteEvent) -> Option<Payload> {
    let body: Payload = serde_json::from_slice(event.body()).ok()?;
    (body.valid() && serde_json::to_vec(&body).ok()? == event.body()).then_some(body)
}

struct LocalResolver;
impl Resolver for LocalResolver {
    fn resolve(&self, _: &Connection, _: CredentialId) -> codec::Result<Option<Credential>> {
        Ok(None)
    }
}

pub struct Social {
    node: [u8; 32],
    registry: Registry,
}

impl Social {
    pub fn new(signer: &NodeSigner) -> Self {
        let rule = KindRule {
            kind: KIND.into(),
            schema: 1,
            actor: Role::Node,
            purpose: None,
        };
        Self {
            node: *signer.id().bytes(),
            registry: Registry::new(vec![rule], vec![]).expect("valid local update rule"),
        }
    }

    /// The HTTP boundary supplies the authorized account and the node's wall-time observations.
    pub fn post(
        &self,
        tx: &Transaction<'_>,
        signer: &NodeSigner,
        account: LocalAccountId,
        text: &str,
        authored_at: i64,
        seen_at: i64,
    ) -> Result<EventId, Error> {
        let account = bbsbox_identity::account(tx, account).map_err(Error::Identity)?;
        if !account.active || signer.id().bytes() != &self.node {
            return Err(Error::Forbidden);
        }
        let body = Payload {
            account: *account.id.bytes(),
            username: account.username,
            text: text.into(),
        };
        if !body.valid() {
            return Err(Error::InvalidInput);
        }
        let scope = Scope {
            resource: body.resource(),
            audience: AUDIENCE.into(),
            distribution: Distribution::Local,
        };
        let body = serde_json::to_vec(&body).map_err(|_| Error::InvalidInput)?;
        let actor = Actor::root(Role::Node, signer.public(), *signer.incarnation())
            .map_err(events::Error::from)?;
        let event = authoring::author(
            tx,
            Inbox::default(),
            Authored {
                action: Action {
                    kind: KIND.into(),
                    schema: 1,
                    scope,
                    dependencies: vec![],
                    claimed_time: authored_at,
                    extensions: vec![],
                },
                actor,
                credential: None,
                body: &body,
                delivery: &Delivery {
                    source: "local".into(),
                    transfer: "social".into(),
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
            Integration::Accepted(_) => {
                tx.execute(
                    "INSERT INTO social_observations VALUES (?1,?2)",
                    params![event.id().bytes().as_slice(), seen_at],
                )?;
                Ok(event.id())
            }
            _ => Err(Error::NotAccepted),
        }
    }
}

impl Module for Social {
    fn id(&self) -> &str {
        "social"
    }
    fn registry(&self) -> &Registry {
        &self.registry
    }

    fn action(&self, connection: &Connection, event: &CompleteEvent) -> events::Result<Decision> {
        let metadata = event.metadata();
        let Some(body) = payload(event) else {
            return Ok(Decision::Deny("invalid_social_body"));
        };
        if metadata.actor.subject != self.node
            || metadata.scope.distribution != Distribution::Local
            || metadata.scope.audience != AUDIENCE
            || metadata.scope.resource != body.resource()
            || !metadata.dependencies.is_empty()
        {
            return Ok(Decision::Deny("foreign_or_nonlocal_social_event"));
        }
        let stream = codec::stream_id(&metadata.actor, &metadata.scope)?;
        let forked = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM social_forked_streams WHERE stream=?1)",
            [stream.as_slice()],
            |row| row.get::<_, bool>(0),
        )?;
        Ok(if forked {
            Decision::Deny("social_stream_quarantined")
        } else {
            Decision::Allow
        })
    }

    fn dependency(
        &self,
        _: &Connection,
        _: &CompleteEvent,
        _: Dependency,
    ) -> events::Result<Decision> {
        Ok(Decision::Deny("unexpected_social_dependency"))
    }

    fn dependency_effective(&self, _: &Connection, _: EventId) -> events::Result<bool> {
        Ok(false)
    }

    fn reduce(&self, tx: &Transaction<'_>, event: &CompleteEvent) -> events::Result<Relevance> {
        let body = payload(event).ok_or(events::Error::CorruptRecord)?;
        let metadata = event.metadata();
        let stream = codec::stream_id(&metadata.actor, &metadata.scope)?;
        tx.execute(
            "INSERT INTO social_posts VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                event.id().bytes().as_slice(),
                body.account.as_slice(),
                body.username,
                body.text,
                metadata.claimed_time,
                stream.as_slice()
            ],
        )?;
        Ok(Relevance::Relevant)
    }

    fn invalidate_fork(&self, tx: &Transaction<'_>, fork: &Fork) -> events::Result<()> {
        tx.execute(
            "INSERT OR IGNORE INTO social_forked_streams
             SELECT stream FROM social_posts WHERE stream=?1 LIMIT 1",
            [fork.stream.as_slice()],
        )?;
        Ok(())
    }
}

impl integration::ReplayModule for Social {
    fn project(&self, tx: &Transaction<'_>, event: &CompleteEvent) -> events::Result<()> {
        self.reduce(tx, event).map(|_| ())
    }
}

pub fn set_following(
    tx: &Transaction<'_>,
    viewer: LocalAccountId,
    author: LocalAccountId,
    follow: bool,
) -> Result<(), Error> {
    let viewer = bbsbox_identity::account(tx, viewer).map_err(Error::Identity)?;
    let author = bbsbox_identity::account(tx, author).map_err(Error::Identity)?;
    if !viewer.active || viewer.id == author.id || (follow && !author.active) {
        return Err(Error::Forbidden);
    }
    let statement = if follow {
        "INSERT OR IGNORE INTO social_follows VALUES (?1,?2)"
    } else {
        "DELETE FROM social_follows WHERE viewer=?1 AND author=?2"
    };
    tx.execute(
        statement,
        params![viewer.id.bytes().as_slice(), author.id.bytes().as_slice()],
    )?;
    Ok(())
}

pub fn is_following(
    connection: &Connection,
    viewer: LocalAccountId,
    author: LocalAccountId,
) -> rusqlite::Result<bool> {
    connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM social_follows WHERE viewer=?1 AND author=?2)",
        params![viewer.bytes().as_slice(), author.bytes().as_slice()],
        |row| row.get(0),
    )
}

pub enum Feed {
    Following(LocalAccountId),
    Author(LocalAccountId),
    Post(LocalAccountId, EventId),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Timing {
    Ordinary,
    ArrivedLater,
    ClockUncertain,
}

#[derive(Debug)]
pub struct Post {
    pub id: EventId,
    pub username: String,
    pub text: String,
    pub authored_at: i64,
    pub utc_time: Option<String>,
    pub timing: Timing,
}

pub fn posts(
    connection: &Connection,
    feed: Feed,
    before: Option<(i64, EventId)>,
) -> rusqlite::Result<Vec<Post>> {
    let (account, following, focused) = match feed {
        Feed::Following(id) => (id, true, None),
        Feed::Author(id) => (id, false, None),
        Feed::Post(id, post) => (id, false, Some(post)),
    };
    let mut query = connection.prepare(
        "SELECT p.id,p.username,p.body,p.authored_at,o.first_seen_at,
                strftime('%Y-%m-%dT%H:%M:%SZ',p.authored_at/1000.0,'unixepoch')
           FROM social_posts p LEFT JOIN social_observations o ON o.event=p.id
          WHERE p.author IN (SELECT ?1 WHERE ?2=0
                           UNION ALL SELECT author FROM social_follows WHERE ?2=1 AND viewer=?1)
            AND p.stream NOT IN (SELECT stream FROM social_forked_streams)
            AND (?3 IS NULL OR (p.authored_at,p.id)<(?3,?4))
            AND (?6 IS NULL OR p.id=?6)
          ORDER BY p.authored_at DESC,p.id DESC LIMIT ?5",
    )?;
    let (time, id) = before.map_or((None, None), |(time, id)| (Some(time), Some(id)));
    query
        .query_map(
            params![
                account.bytes().as_slice(),
                following,
                time,
                id.as_ref().map(|id| id.bytes().as_slice()),
                (PAGE_SIZE + 1) as i64,
                focused.as_ref().map(|id| id.bytes().as_slice())
            ],
            |row| {
                let authored_at: i64 = row.get(3)?;
                let observed_at: Option<i64> = row.get(4)?;
                let timing = match observed_at.map(|seen| seen.saturating_sub(authored_at)) {
                    Some(delay) if delay >= CLOCK_LABEL_MILLIS => Timing::ArrivedLater,
                    Some(delay) if delay > -CLOCK_LABEL_MILLIS => Timing::Ordinary,
                    _ => Timing::ClockUncertain,
                };
                Ok(Post {
                    id: EventId::from_bytes(row.get(0)?),
                    username: row.get(1)?,
                    text: row.get(2)?,
                    authored_at,
                    utc_time: row.get(5)?,
                    timing,
                })
            },
        )?
        .collect()
}
