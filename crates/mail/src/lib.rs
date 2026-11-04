// SPDX-License-Identifier: AGPL-3.0-or-later
//! Recipient-restricted local mail with a host-attested signed journal.
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
use bbsbox_identity::{
    Account, LocalAccountId, Role as AccountRole, node::NodeSigner, normalize_username,
};
use bbsbox_storage::Migration;
use rand_core::{OsRng, RngCore};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};

const SEND: &str = "mail.thread.start";
const REPLY: &str = "mail.thread.reply";
const AUDIENCE: &str = "mail-participants";

pub fn migrations() -> [Migration; 1] {
    [Migration {
        module: "mail",
        version: 1,
        sql: include_str!("schema.sql"),
    }]
}

#[derive(Debug)]
pub enum Error {
    Storage(bbsbox_storage::Error),
    Event(events::Error),
    Identity(bbsbox_identity::Error),
    InvalidInput,
    Forbidden,
    Missing,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThreadId([u8; 16]);
impl ThreadId {
    pub fn parse(key: &str) -> Option<Self> {
        let mut id = [0; 16];
        if key.len() != 32
            || !key
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return None;
        }
        hex::decode_to_slice(key, &mut id).ok()?;
        Some(Self(id))
    }
}
impl std::fmt::Display for ThreadId {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        output.write_str(&hex::encode(self.0))
    }
}

pub enum Message<'a> {
    Start {
        recipient: &'a str,
        operator_only: bool,
        subject: &'a str,
        text: &'a str,
    },
    Reply {
        thread: ThreadId,
        parent: EventId,
        text: &'a str,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Body {
    sender: [u8; 16],
    recipient: [u8; 16],
    sender_name: String,
    recipient_name: String,
    subject: Option<String>,
    text: String,
    parent: Option<[u8; 32]>,
}

fn valid_text(value: &str, limit: usize, multiline: bool) -> bool {
    !value.trim().is_empty()
        && value.len() <= limit
        && value
            .chars()
            .all(|c| !c.is_control() || (multiline && matches!(c, '\n' | '\t')))
}

impl Body {
    fn valid(&self, kind: &str) -> bool {
        self.sender != self.recipient
            && [&self.sender_name, &self.recipient_name]
                .iter()
                .all(|name| normalize_username(name).as_deref().ok() == Some(name.as_str()))
            && valid_text(&self.text, 48 * 1024, true)
            && match kind {
                SEND => {
                    self.parent.is_none()
                        && self
                            .subject
                            .as_deref()
                            .is_some_and(|text| valid_text(text, 120, false))
                }
                REPLY => self.parent.is_some() && self.subject.is_none(),
                _ => false,
            }
    }
}

fn decode(event: &CompleteEvent) -> Option<Body> {
    let body: Body = serde_json::from_slice(event.body()).ok()?;
    if body.valid(&event.metadata().kind) && serde_json::to_vec(&body).ok()? == event.body() {
        Some(body)
    } else {
        None
    }
}

struct Participants {
    root: [u8; 32],
    sender: [u8; 16],
    recipient: [u8; 16],
    sender_name: String,
    recipient_name: String,
}

impl Participants {
    fn other(&self, sender: &[u8; 16]) -> Option<([u8; 16], &str)> {
        if sender == &self.sender {
            Some((self.recipient, &self.recipient_name))
        } else if sender == &self.recipient {
            Some((self.sender, &self.sender_name))
        } else {
            None
        }
    }
}

fn participants(
    connection: &Connection,
    thread: ThreadId,
) -> rusqlite::Result<Option<Participants>> {
    connection.query_row("SELECT root,sender,recipient,sender_name,recipient_name FROM mail_threads WHERE id=?1 AND id NOT IN (SELECT thread FROM mail_quarantined)", [thread.0.as_slice()], |row| Ok(Participants { root: row.get(0)?, sender: row.get(1)?, recipient: row.get(2)?, sender_name: row.get(3)?, recipient_name: row.get(4)? })).optional()
}

fn parent_in_thread(
    connection: &Connection,
    thread: ThreadId,
    parent: &[u8; 32],
) -> rusqlite::Result<bool> {
    connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM mail_messages WHERE id=?1 AND thread=?2)",
        params![parent.as_slice(), thread.0.as_slice()],
        |row| row.get(0),
    )
}

fn reply_dependencies(root: [u8; 32], parent: [u8; 32]) -> Vec<Dependency> {
    let mut dependencies = [root, parent]
        .map(EventId::from_bytes)
        .map(Dependency::event)
        .to_vec();
    dependencies.sort_unstable();
    dependencies.dedup();
    dependencies
}

struct LocalResolver;
impl Resolver for LocalResolver {
    fn resolve(&self, _: &Connection, _: CredentialId) -> codec::Result<Option<Credential>> {
        Ok(None)
    }
}

pub struct Mailbox {
    node: [u8; 32],
    registry: Registry,
}

impl Mailbox {
    pub fn new(signer: &NodeSigner) -> Self {
        let rules = [SEND, REPLY].map(|kind| KindRule {
            kind: kind.into(),
            schema: 1,
            actor: Role::Node,
            purpose: None,
        });
        Self {
            node: *signer.id().bytes(),
            registry: Registry::new(rules.into(), vec![]).expect("valid local mail rules"),
        }
    }

    /// The HTTP boundary supplies an account authorized in the same writer transaction.
    pub fn send(
        &self,
        tx: &Transaction<'_>,
        signer: &NodeSigner,
        sender: LocalAccountId,
        message: Message<'_>,
        time: i64,
    ) -> Result<(ThreadId, EventId), Error> {
        let sender = active(tx, sender)?;
        if signer.id().bytes() != &self.node {
            return Err(Error::Forbidden);
        }
        let sender_id = *sender.id.bytes();
        let (kind, thread, body, dependencies) = match message {
            Message::Start {
                recipient,
                operator_only,
                subject,
                text,
            } => {
                let recipient = bbsbox_identity::account_by_username(tx, recipient).map_err(
                    |error| match error {
                        bbsbox_identity::Error::AccountMissing
                        | bbsbox_identity::Error::InvalidUsername => Error::Missing,
                        error => Error::Identity(error),
                    },
                )?;
                if !recipient.active || (operator_only && recipient.role != AccountRole::Sysop) {
                    return Err(Error::Missing);
                }
                let mut id = [0; 16];
                OsRng.fill_bytes(&mut id);
                (
                    SEND,
                    ThreadId(id),
                    Body {
                        sender: sender_id,
                        recipient: *recipient.id.bytes(),
                        sender_name: sender.username,
                        recipient_name: recipient.username,
                        subject: Some(subject.into()),
                        text: text.into(),
                        parent: None,
                    },
                    vec![],
                )
            }
            Message::Reply {
                thread,
                parent,
                text,
            } => {
                let participants = participants(tx, thread)?.ok_or(Error::Missing)?;
                let (recipient, recipient_name) =
                    participants.other(&sender_id).ok_or(Error::Missing)?;
                if !parent_in_thread(tx, thread, parent.bytes())? {
                    return Err(Error::Missing);
                }
                (
                    REPLY,
                    thread,
                    Body {
                        sender: sender_id,
                        recipient,
                        sender_name: sender.username,
                        recipient_name: recipient_name.into(),
                        subject: None,
                        text: text.into(),
                        parent: Some(*parent.bytes()),
                    },
                    reply_dependencies(participants.root, *parent.bytes()),
                )
            }
        };
        if !body.valid(kind) {
            return Err(Error::InvalidInput);
        }
        let body = serde_json::to_vec(&body).map_err(|_| Error::InvalidInput)?;
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
                        resource: format!("mail/{thread}"),
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
                    transfer: "mail".into(),
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
            Integration::Accepted(_) => Ok((thread, event.id())),
            _ => Err(Error::NotAccepted),
        }
    }
}

fn active(connection: &Connection, id: LocalAccountId) -> Result<Account, Error> {
    let account = bbsbox_identity::account(connection, id).map_err(Error::Identity)?;
    if account.active {
        Ok(account)
    } else {
        Err(Error::Forbidden)
    }
}

fn event_thread(event: &CompleteEvent) -> Option<ThreadId> {
    ThreadId::parse(event.metadata().scope.resource.strip_prefix("mail/")?)
}

impl Module for Mailbox {
    fn id(&self) -> &str {
        "mail"
    }
    fn registry(&self) -> &Registry {
        &self.registry
    }

    fn action(&self, connection: &Connection, event: &CompleteEvent) -> events::Result<Decision> {
        let metadata = event.metadata();
        if metadata.scope.distribution != Distribution::Local
            || metadata.scope.audience != AUDIENCE
            || metadata.actor.subject != self.node
        {
            return Ok(Decision::Deny("foreign_or_nonlocal_mail"));
        }
        let (Some(thread), Some(body)) = (event_thread(event), decode(event)) else {
            return Ok(Decision::Deny("invalid_mail_body"));
        };
        if connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM mail_quarantined WHERE thread=?1)",
            [thread.0.as_slice()],
            |row| row.get::<_, bool>(0),
        )? {
            return Ok(Decision::Deny("mail_thread_quarantined"));
        }
        let current = participants(connection, thread)?;
        if metadata.kind == SEND {
            return Ok(if current.is_none() && metadata.dependencies.is_empty() {
                Decision::Allow
            } else {
                Decision::Deny("mail_thread_already_exists")
            });
        }
        let Some(participants) = current else {
            return Ok(Decision::Missing(metadata.dependencies.clone()));
        };
        let parent = body.parent.ok_or(events::Error::CorruptRecord)?;
        if participants.other(&body.sender) != Some((body.recipient, body.recipient_name.as_str()))
            || participants.other(&body.recipient) != Some((body.sender, body.sender_name.as_str()))
            || metadata.dependencies != reply_dependencies(participants.root, parent)
        {
            return Ok(Decision::Deny("wrong_mail_participants_or_dependencies"));
        }
        Ok(if parent_in_thread(connection, thread, &parent)? {
            Decision::Allow
        } else {
            Decision::Missing(vec![Dependency::event(EventId::from_bytes(parent))])
        })
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
            Decision::Deny("unsupported_mail_dependency")
        })
    }

    fn dependency_effective(&self, connection: &Connection, id: EventId) -> events::Result<bool> {
        Ok(connection.query_row("SELECT EXISTS(SELECT 1 FROM mail_messages WHERE id=?1 AND thread NOT IN (SELECT thread FROM mail_quarantined))", [id.bytes().as_slice()], |row| row.get(0))?)
    }

    fn reduce(&self, tx: &Transaction<'_>, event: &CompleteEvent) -> events::Result<Relevance> {
        let metadata = event.metadata();
        let thread = event_thread(event).ok_or(events::Error::CorruptRecord)?;
        let body = decode(event).ok_or(events::Error::CorruptRecord)?;
        let id = event.id();
        if metadata.kind == SEND {
            tx.execute(
                "INSERT INTO mail_threads VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                params![
                    thread.0.as_slice(),
                    id.bytes().as_slice(),
                    body.sender.as_slice(),
                    body.recipient.as_slice(),
                    body.sender_name,
                    body.recipient_name,
                    body.subject,
                    metadata.claimed_time
                ],
            )?;
        } else {
            tx.execute(
                "UPDATE mail_threads SET last_activity=max(last_activity,?2) WHERE id=?1",
                params![thread.0.as_slice(), metadata.claimed_time],
            )?;
        }
        let stream = codec::stream_id(&metadata.actor, &metadata.scope)?;
        tx.execute(
            "INSERT INTO mail_messages VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                id.bytes().as_slice(),
                thread.0.as_slice(),
                body.parent.as_ref().map(<[u8; 32]>::as_slice),
                body.sender.as_slice(),
                body.sender_name,
                body.text,
                metadata.claimed_time,
                stream.as_slice()
            ],
        )?;
        Ok(Relevance::Relevant)
    }

    fn invalidate_fork(&self, tx: &Transaction<'_>, fork: &Fork) -> events::Result<()> {
        tx.execute("INSERT OR IGNORE INTO mail_quarantined SELECT DISTINCT thread FROM mail_messages WHERE stream=?1", [fork.stream.as_slice()])?;
        Ok(())
    }
}

impl integration::ReplayModule for Mailbox {
    fn project(&self, tx: &Transaction<'_>, event: &CompleteEvent) -> events::Result<()> {
        self.reduce(tx, event).map(|_| ())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Conversation {
    pub id: ThreadId,
    pub correspondent: String,
    pub subject: String,
    pub last_activity: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct StoredMessage {
    pub id: EventId,
    pub parent: Option<EventId>,
    pub sender: String,
    pub text: String,
    pub authored_at: i64,
}

pub fn conversations(
    connection: &Connection,
    account: LocalAccountId,
    before: Option<(i64, ThreadId)>,
) -> Result<Vec<Conversation>, Error> {
    integration::ensure_projection_current(connection, "mail")?;
    active(connection, account)?;
    let cursor = before.as_ref().map(|(time, id)| (time, id.0.as_slice()));
    let mut query = connection.prepare("SELECT id,CASE WHEN sender=?1 THEN recipient_name ELSE sender_name END,subject,last_activity FROM mail_threads WHERE (sender=?1 OR recipient=?1) AND id NOT IN (SELECT thread FROM mail_quarantined) AND (?2 IS NULL OR (last_activity,id)<(?2,?3)) ORDER BY last_activity DESC,id DESC LIMIT 50")?;
    let rows = query.query_map(
        params![
            account.bytes().as_slice(),
            cursor.map(|(time, _)| time),
            cursor.map(|(_, id)| id)
        ],
        |row| {
            Ok(Conversation {
                id: ThreadId(row.get(0)?),
                correspondent: row.get(1)?,
                subject: row.get(2)?,
                last_activity: row.get(3)?,
            })
        },
    )?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn messages(
    connection: &Connection,
    account: LocalAccountId,
    thread: ThreadId,
    after: Option<(i64, EventId)>,
) -> Result<(Conversation, Vec<StoredMessage>), Error> {
    integration::ensure_projection_current(connection, "mail")?;
    active(connection, account)?;
    let conversation = connection.query_row("SELECT CASE WHEN sender=?2 THEN recipient_name ELSE sender_name END,subject,last_activity FROM mail_threads WHERE id=?1 AND (sender=?2 OR recipient=?2) AND id NOT IN (SELECT thread FROM mail_quarantined)", params![thread.0.as_slice(), account.bytes().as_slice()], |row| Ok(Conversation { id: thread, correspondent: row.get(0)?, subject: row.get(1)?, last_activity: row.get(2)? })).optional()?.ok_or(Error::Missing)?;
    let cursor = after
        .as_ref()
        .map(|(time, id)| (time, id.bytes().as_slice()));
    let mut query = connection.prepare("SELECT m.id,m.parent,m.sender_name,m.body,m.authored_at FROM mail_messages m JOIN mail_threads t ON t.id=m.thread WHERE m.thread=?1 AND (t.sender=?4 OR t.recipient=?4) AND t.id NOT IN (SELECT thread FROM mail_quarantined) AND (?2 IS NULL OR (m.authored_at,m.id)>(?2,?3)) ORDER BY m.authored_at,m.id LIMIT 30")?;
    let rows = query.query_map(
        params![
            thread.0.as_slice(),
            cursor.map(|(time, _)| time),
            cursor.map(|(_, id)| id),
            account.bytes().as_slice()
        ],
        |row| {
            Ok(StoredMessage {
                id: EventId::from_bytes(row.get(0)?),
                parent: row.get::<_, Option<[u8; 32]>>(1)?.map(EventId::from_bytes),
                sender: row.get(2)?,
                text: row.get(3)?,
                authored_at: row.get(4)?,
            })
        },
    )?;
    Ok((conversation, rows.collect::<rusqlite::Result<_>>()?))
}

pub fn before_message(
    connection: &Connection,
    account: LocalAccountId,
    thread: ThreadId,
    target: EventId,
) -> Result<Option<(i64, EventId)>, Error> {
    integration::ensure_projection_current(connection, "mail")?;
    active(connection, account)?;
    let time = connection.query_row(
        "SELECT m.authored_at FROM mail_messages m JOIN mail_threads t ON t.id=m.thread WHERE t.id=?1 AND m.id=?2 AND (t.sender=?3 OR t.recipient=?3) AND t.id NOT IN (SELECT thread FROM mail_quarantined)",
        params![thread.0.as_slice(), target.bytes().as_slice(), account.bytes().as_slice()], |row| row.get::<_, i64>(0),
    ).optional()?.ok_or(Error::Missing)?;
    Ok(connection.query_row(
        "SELECT authored_at,id FROM mail_messages WHERE thread=?1 AND (authored_at,id)<(?2,?3) ORDER BY authored_at DESC,id DESC LIMIT 1",
        params![thread.0.as_slice(), time, target.bytes().as_slice()], |row| Ok((row.get(0)?, EventId::from_bytes(row.get(1)?))),
    ).optional()?)
}
