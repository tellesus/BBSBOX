// SPDX-License-Identifier: AGPL-3.0-or-later
//! Node-attested, local-only board creation and threaded posts.
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
use bbsbox_identity::{Account, Role as AccountRole, node::NodeSigner, normalize_username};
use bbsbox_storage::Migration;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};

pub fn migrations() -> [Migration; 1] {
    [Migration {
        module: "discussions",
        version: 1,
        sql: include_str!("schema.sql"),
    }]
}

const AUDIENCE: &str = "local-public";
const CREATE: &str = "discussions.board.create";
const THREAD: &str = "discussions.thread.create";
const REPLY: &str = "discussions.reply.create";

#[derive(Debug)]
pub enum Error {
    Storage(bbsbox_storage::Error),
    Event(events::Error),
    Identity(bbsbox_identity::Error),
    InvalidInput,
    Forbidden,
    MissingBoard,
    BoardExists,
    MissingParent,
    NotAccepted,
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

pub enum Change<'a> {
    Board { title: &'a str },
    Thread { title: &'a str, text: &'a str },
    Reply { parent: EventId, text: &'a str },
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    account: [u8; 16],
    username: String,
    title: Option<String>,
    text: Option<String>,
    thread: Option<[u8; 32]>,
    parent: Option<[u8; 32]>,
}

fn slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn valid_text(value: &str, max: usize, multiline: bool) -> bool {
    !value.trim().is_empty()
        && value.len() <= max
        && value
            .chars()
            .all(|c| !c.is_control() || (multiline && matches!(c, '\n' | '\t')))
}

impl Payload {
    fn valid(&self, kind: &str) -> bool {
        let title = self
            .title
            .as_deref()
            .is_some_and(|value| valid_text(value, 120, false));
        let text = self
            .text
            .as_deref()
            .is_some_and(|value| valid_text(value, 48 * 1024, true));
        normalize_username(&self.username).as_deref().ok() == Some(self.username.as_str())
            && match kind {
                CREATE => {
                    title && self.text.is_none() && self.parent.is_none() && self.thread.is_none()
                }
                THREAD => title && text && self.parent.is_none() && self.thread.is_none(),
                REPLY => {
                    text && self.title.is_none() && self.parent.is_some() && self.thread.is_some()
                }
                _ => false,
            }
    }
}

fn resource(board: &str) -> String {
    format!("board/{board}")
}

fn live_board(connection: &Connection, board: &str) -> rusqlite::Result<Option<[u8; 32]>> {
    connection.query_row(
        "SELECT id FROM discussions_entries WHERE board=?1 AND kind='board' AND board NOT IN (SELECT board FROM discussions_quarantined)",
        [board], |row| row.get(0),
    ).optional()
}

fn parent_thread(
    connection: &Connection,
    board: &str,
    parent: &[u8; 32],
) -> rusqlite::Result<Option<[u8; 32]>> {
    connection.query_row(
        "SELECT thread FROM discussions_entries WHERE id=?1 AND board=?2 AND kind IN ('thread','reply')",
        params![parent.as_slice(), board], |row| row.get(0),
    ).optional()
}

fn required(board: [u8; 32], parent: Option<[u8; 32]>) -> Vec<Dependency> {
    let mut dependencies = vec![Dependency::event(EventId::from_bytes(board))];
    if let Some(parent) = parent {
        dependencies.push(Dependency::event(EventId::from_bytes(parent)));
    }
    dependencies.sort_unstable();
    dependencies
}

struct LocalResolver;
impl Resolver for LocalResolver {
    fn resolve(&self, _: &Connection, _: CredentialId) -> codec::Result<Option<Credential>> {
        Ok(None)
    }
}

pub struct Boards {
    node: [u8; 32],
    registry: Registry,
}

impl Boards {
    pub fn new(signer: &NodeSigner) -> Self {
        let registry = Registry::new(
            [CREATE, THREAD, REPLY]
                .map(|kind| KindRule {
                    kind: kind.into(),
                    schema: 1,
                    actor: Role::Node,
                    purpose: None,
                })
                .into(),
            vec![],
        )
        .expect("valid local board rules");
        Self {
            node: *signer.id().bytes(),
            registry,
        }
    }

    /// The transport supplies an account authorized inside this transaction and its local node signer.
    pub fn record(
        &self,
        tx: &Transaction<'_>,
        signer: &NodeSigner,
        account: &Account,
        board: &str,
        change: Change<'_>,
        time: i64,
    ) -> Result<EventId, Error> {
        let account = bbsbox_identity::account(tx, account.id).map_err(Error::Identity)?;
        if !account.active || signer.id().bytes() != &self.node {
            return Err(Error::Forbidden);
        }
        if !slug(board) {
            return Err(Error::InvalidInput);
        }
        let mut payload = Payload {
            account: *account.id.bytes(),
            username: account.username.clone(),
            title: None,
            text: None,
            thread: None,
            parent: None,
        };
        let current = live_board(tx, board)?;
        let (kind, dependencies) = match change {
            Change::Board { title } => {
                if account.role != AccountRole::Sysop {
                    return Err(Error::Forbidden);
                }
                if current.is_some() {
                    return Err(Error::BoardExists);
                }
                payload.title = Some(title.to_owned());
                (CREATE, vec![])
            }
            Change::Thread { title, text } => {
                payload.title = Some(title.to_owned());
                payload.text = Some(text.to_owned());
                (THREAD, required(current.ok_or(Error::MissingBoard)?, None))
            }
            Change::Reply { parent, text } => {
                let board_id = current.ok_or(Error::MissingBoard)?;
                payload.thread =
                    Some(parent_thread(tx, board, parent.bytes())?.ok_or(Error::MissingParent)?);
                payload.parent = Some(*parent.bytes());
                payload.text = Some(text.to_owned());
                (REPLY, required(board_id, payload.parent))
            }
        };
        if !payload.valid(kind) {
            return Err(Error::InvalidInput);
        }
        let body = serde_json::to_vec(&payload).map_err(|_| Error::InvalidInput)?;
        if body.len() > codec::MAX_BODY {
            return Err(Error::InvalidInput);
        }
        let actor = Actor::root(Role::Node, signer.public(), *signer.incarnation())
            .map_err(events::Error::from)?;
        let event = authoring::author(
            tx,
            Inbox::default(),
            Authored {
                action: Action {
                    kind: kind.into(),
                    schema: 1,
                    scope: Scope {
                        resource: resource(board),
                        audience: AUDIENCE.into(),
                        distribution: Distribution::Local,
                    },
                    dependencies,
                    claimed_time: time,
                    extensions: vec![],
                },
                actor,
                credential: None,
                body: &body,
                delivery: &Delivery {
                    source: "local".into(),
                    transfer: "discussions".into(),
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
            Integration::Accepted(_) => Ok(event.id()),
            _ => Err(Error::NotAccepted),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct BoardSummary {
    pub slug: String,
    pub title: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ThreadSummary {
    pub id: EventId,
    pub title: String,
    pub username: String,
    pub created_at: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Post {
    pub id: EventId,
    pub parent: Option<EventId>,
    pub username: String,
    pub text: String,
    pub created_at: i64,
}

pub fn boards(connection: &Connection, after: Option<&str>) -> Result<Vec<BoardSummary>, Error> {
    integration::ensure_projection_current(connection, "discussions")?;
    let mut query = connection.prepare(
        "SELECT board,title FROM discussions_entries WHERE kind='board' AND (?1 IS NULL OR board>?1) AND board NOT IN (SELECT board FROM discussions_quarantined) ORDER BY board LIMIT 100",
    )?;
    let rows = query.query_map([after], |row| {
        Ok(BoardSummary {
            slug: row.get(0)?,
            title: row.get(1)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn threads(
    connection: &Connection,
    board: &str,
    before: Option<(i64, EventId)>,
) -> Result<Vec<ThreadSummary>, Error> {
    integration::ensure_projection_current(connection, "discussions")?;
    if live_board(connection, board)?.is_none() {
        return Err(Error::MissingBoard);
    }
    let cursor = before
        .as_ref()
        .map(|(time, id)| (time, id.bytes().as_slice()));
    let mut query = connection.prepare(
        "SELECT id,title,username,created_at FROM discussions_entries WHERE board=?1 AND kind='thread' AND (?2 IS NULL OR (created_at,id)<(?2,?3)) ORDER BY created_at DESC,id DESC LIMIT 100",
    )?;
    let rows = query.query_map(
        params![
            board,
            cursor.map(|(time, _)| time),
            cursor.map(|(_, id)| id)
        ],
        |row| {
            Ok(ThreadSummary {
                id: EventId::from_bytes(row.get(0)?),
                title: row.get(1)?,
                username: row.get(2)?,
                created_at: row.get(3)?,
            })
        },
    )?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn posts(
    connection: &Connection,
    board: &str,
    thread: EventId,
    after: Option<(i64, EventId)>,
) -> Result<Vec<Post>, Error> {
    integration::ensure_projection_current(connection, "discussions")?;
    if live_board(connection, board)?.is_none() {
        return Err(Error::MissingBoard);
    }
    if parent_thread(connection, board, thread.bytes())? != Some(*thread.bytes()) {
        return Err(Error::MissingParent);
    }
    let cursor = after
        .as_ref()
        .map(|(time, id)| (time, id.bytes().as_slice()));
    let mut query = connection.prepare(
        "SELECT id,parent,username,body,created_at FROM discussions_entries WHERE thread=?1 AND (?2 IS NULL OR (created_at,id)>(?2,?3)) ORDER BY created_at,id LIMIT 100",
    )?;
    let rows = query.query_map(
        params![
            thread.bytes().as_slice(),
            cursor.map(|(time, _)| time),
            cursor.map(|(_, id)| id)
        ],
        |row| {
            Ok(Post {
                id: EventId::from_bytes(row.get(0)?),
                parent: row.get::<_, Option<[u8; 32]>>(1)?.map(EventId::from_bytes),
                username: row.get(2)?,
                text: row.get(3)?,
                created_at: row.get(4)?,
            })
        },
    )?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn payload(event: &CompleteEvent) -> Option<Payload> {
    let value: Payload = serde_json::from_slice(event.body()).ok()?;
    if value.valid(&event.metadata().kind) && serde_json::to_vec(&value).ok()? == event.body() {
        Some(value)
    } else {
        None
    }
}

impl Module for Boards {
    fn id(&self) -> &str {
        "discussions"
    }
    fn registry(&self) -> &Registry {
        &self.registry
    }

    fn action(&self, connection: &Connection, event: &CompleteEvent) -> events::Result<Decision> {
        let metadata = event.metadata();
        let scope = &metadata.scope;
        let Some(board) = scope
            .resource
            .strip_prefix("board/")
            .filter(|name| slug(name))
        else {
            return Ok(Decision::Deny("invalid_board"));
        };
        if scope.distribution != Distribution::Local
            || scope.audience != AUDIENCE
            || metadata.actor.subject != self.node
        {
            return Ok(Decision::Deny("foreign_or_nonlocal_board_event"));
        }
        if connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM discussions_quarantined WHERE board=?1)",
            [board],
            |row| row.get::<_, bool>(0),
        )? {
            return Ok(Decision::Deny("board_quarantined"));
        }
        let Some(body) = payload(event) else {
            return Ok(Decision::Deny("invalid_discussion_body"));
        };
        let current = live_board(connection, board)?;
        if metadata.kind == CREATE {
            return Ok(if current.is_none() && metadata.dependencies.is_empty() {
                Decision::Allow
            } else {
                Decision::Deny("duplicate_board_or_dependencies")
            });
        }
        let Some(board_id) = current else {
            return Ok(Decision::Missing(metadata.dependencies.clone()));
        };
        if metadata.dependencies != required(board_id, body.parent) {
            return Ok(Decision::Deny("invalid_discussion_dependencies"));
        }
        if let Some(parent) = body.parent {
            match parent_thread(connection, board, &parent)? {
                Some(thread) if Some(thread) == body.thread => (),
                Some(_) => return Ok(Decision::Deny("parent_thread_mismatch")),
                None => {
                    return Ok(Decision::Missing(vec![Dependency::event(
                        EventId::from_bytes(parent),
                    )]));
                }
            }
        }
        Ok(Decision::Allow)
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
            Decision::Deny("unsupported_discussion_dependency")
        })
    }

    fn dependency_effective(&self, connection: &Connection, id: EventId) -> events::Result<bool> {
        Ok(connection.query_row("SELECT EXISTS(SELECT 1 FROM discussions_entries WHERE id=?1 AND board NOT IN (SELECT board FROM discussions_quarantined))", [id.bytes().as_slice()], |row| row.get(0))?)
    }

    fn reduce(&self, tx: &Transaction<'_>, event: &CompleteEvent) -> events::Result<Relevance> {
        let metadata = event.metadata();
        let board = metadata
            .scope
            .resource
            .strip_prefix("board/")
            .ok_or(events::Error::CorruptRecord)?;
        let body = payload(event).ok_or(events::Error::CorruptRecord)?;
        let id = event.id();
        let (kind, thread) = match metadata.kind.as_str() {
            CREATE => ("board", None),
            THREAD => ("thread", Some(*id.bytes())),
            REPLY => ("reply", body.thread),
            _ => return Err(events::Error::CorruptRecord),
        };
        let stream = codec::stream_id(&metadata.actor, &metadata.scope)?;
        tx.execute(
            "INSERT INTO discussions_entries VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                id.bytes().as_slice(),
                board,
                kind,
                body.account.as_slice(),
                body.username,
                thread.as_ref().map(<[u8; 32]>::as_slice),
                body.parent.as_ref().map(<[u8; 32]>::as_slice),
                body.title,
                body.text,
                metadata.claimed_time,
                stream.as_slice()
            ],
        )?;
        Ok(Relevance::Relevant)
    }

    fn invalidate_fork(&self, tx: &Transaction<'_>, fork: &Fork) -> events::Result<()> {
        tx.execute("INSERT OR IGNORE INTO discussions_quarantined SELECT DISTINCT board FROM discussions_entries WHERE stream=?1", [fork.stream.as_slice()])?;
        Ok(())
    }
}
