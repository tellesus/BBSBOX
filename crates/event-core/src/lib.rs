// SPDX-License-Identifier: AGPL-3.0-or-later
//! Durable intake. Retained bytes and authenticated chain continuity are not semantic acceptance.
use bbsbox_credentials::{Credential, CredentialId};
use bbsbox_event_codec::{self as codec, Commitment, EventId, Metadata, SignedEnvelope};
use bbsbox_storage::Migration;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use sha2::{Digest, Sha256};

pub type Result<T> = std::result::Result<T, Error>;
pub const MAX_BATCH: usize = 100;
pub const DEFAULT_PENDING_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug)]
pub enum Error {
    Storage(bbsbox_storage::Error),
    Sqlite(rusqlite::Error),
    Codec(codec::Error),
    InvalidDelivery,
    InvalidLimit,
    Quota,
    CorruptRecord,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "event intake failed: {self:?}")
    }
}
impl std::error::Error for Error {}
impl From<bbsbox_storage::Error> for Error {
    fn from(value: bbsbox_storage::Error) -> Self {
        Self::Storage(value)
    }
}
impl From<rusqlite::Error> for Error {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sqlite(value)
    }
}
impl From<codec::Error> for Error {
    fn from(value: codec::Error) -> Self {
        Self::Codec(value)
    }
}

pub fn migrations() -> Vec<Migration> {
    vec![Migration {
        module: "events",
        version: 1,
        sql: include_str!("schema.sql"),
    }]
}

/// Receiver-supplied provenance after transport authentication/admission. Never trust an envelope's claim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Delivery {
    pub source: String,
    pub transfer: String,
}
fn valid_token(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-.:/".contains(&b))
}
impl Delivery {
    fn validate(&self) -> Result<()> {
        if valid_token(&self.source) && valid_token(&self.transfer) {
            Ok(())
        } else {
            Err(Error::InvalidDelivery)
        }
    }
}
pub trait Resolver {
    fn resolve(
        &self,
        connection: &Connection,
        id: CredentialId,
    ) -> codec::Result<Option<Credential>>;
}
impl<F> Resolver for F
where
    F: Fn(&Connection, CredentialId) -> codec::Result<Option<Credential>>,
{
    fn resolve(
        &self,
        connection: &Connection,
        id: CredentialId,
    ) -> codec::Result<Option<Credential>> {
        self(connection, id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Readiness {
    Unsupported,
    InvalidDelegation,
    MissingCredential(CredentialId),
    MissingPrevious(EventId),
    InvalidChain,
    Forked { first_sequence: u64 },
    CommitmentOnly,
    Ready,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Intake {
    pub id: EventId,
    /// Exact stored event repeated without adding a first verified body.
    pub duplicate: bool,
    pub readiness: Readiness,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fork {
    pub stream: [u8; 32],
    pub first_sequence: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Batch {
    pub visited: usize,
    pub changed: usize,
    /// Pass this on the next call; None means a full scan ended. Repeat scans after dependency changes.
    pub next: Option<EventId>,
}

/// Every operation uses the caller's single-writer transaction. The quota covers all retained intake
/// until a later semantic integration/retention implementation explicitly releases it.
#[derive(Clone, Copy)]
pub struct Inbox {
    pending_bytes: u64,
}
impl Default for Inbox {
    fn default() -> Self {
        Self {
            pending_bytes: DEFAULT_PENDING_BYTES,
        }
    }
}
impl Inbox {
    pub fn with_quota(pending_bytes: u64) -> Result<Self> {
        if pending_bytes == 0 || pending_bytes > i64::MAX as u64 {
            return Err(Error::InvalidLimit);
        }
        Ok(Self { pending_bytes })
    }
    pub fn retained_bytes(connection: &Connection) -> Result<u64> {
        let value: i64 = connection.query_row(
            "SELECT bytes FROM events_budget WHERE singleton=1",
            [],
            |r| r.get(0),
        )?;
        u64::try_from(value).map_err(|_| Error::CorruptRecord)
    }
    fn charge(&self, tx: &Transaction<'_>, bytes: usize) -> Result<()> {
        let old = Self::retained_bytes(tx)?;
        let next = old.checked_add(bytes as u64).ok_or(Error::Quota)?;
        if next > self.pending_bytes {
            return Err(Error::Quota);
        }
        tx.execute(
            "UPDATE events_budget SET bytes=?1 WHERE singleton=1",
            [next as i64],
        )?;
        Ok(())
    }
    pub fn receive(
        &self,
        tx: &Transaction<'_>,
        encoded: &[u8],
        body: Option<&[u8]>,
        delivery: &Delivery,
        resolver: &dyn Resolver,
    ) -> Result<Intake> {
        delivery.validate()?;
        // Unknown structural versions remain opaque and bounded; never bind their claimed actor.
        if encoded.len() > codec::MAX_ENVELOPE {
            return Err(codec::Error::EnvelopeTooLarge.into());
        }
        if body.is_some_and(|b| b.len() > codec::MAX_BODY) {
            return Err(codec::Error::BodyTooLarge.into());
        }
        let parsed = match codec::verify_signature(encoded) {
            Ok(value) => Some(value),
            Err(
                codec::Error::UnsupportedMajor(_)
                | codec::Error::UnsupportedMinor(_)
                | codec::Error::UnsupportedStructure
                | codec::Error::UnsupportedRole(_)
                | codec::Error::UnsupportedDependency(_),
            ) => None,
            Err(error) => return Err(error.into()),
        };
        if let (Some(signed), Some(body)) = (&parsed, body) {
            signed.check_body(body)?;
        }
        let id = EventId::from_bytes(Sha256::digest(encoded).into());
        let old: Option<(Vec<u8>, bool)> = tx
            .query_row(
                "SELECT envelope,body IS NOT NULL FROM events_raw WHERE id=?1",
                [id.bytes().as_slice()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let existed = old.is_some();
        if let Some((old, _)) = &old
            && old != encoded
        {
            return Err(Error::CorruptRecord);
        }
        if !existed {
            self.charge(tx, encoded.len())?;
            tx.execute("INSERT INTO events_raw(id,envelope,state,first_source,first_transfer) VALUES(?1,?2,?3,?4,?5)", params![id.bytes().as_slice(), encoded, if parsed.is_some() { 0 } else { 3 }, delivery.source, delivery.transfer])?;
        }
        let adds_body =
            parsed.is_some() && body.is_some() && !old.as_ref().is_some_and(|(_, has)| *has);
        if adds_body {
            let bytes = body.expect("checked above");
            self.charge(tx, bytes.len())?;
            tx.execute(
                "UPDATE events_raw SET body=?2,body_source=?3,body_transfer=?4 WHERE id=?1",
                params![
                    id.bytes().as_slice(),
                    bytes,
                    delivery.source,
                    delivery.transfer
                ],
            )?;
        }
        // Unknown-profile bodies cannot be checked and never occupy a valid body/provenance slot.
        if let Some(signed) = parsed {
            bind(tx, signed, resolver)?;
        }
        connect(tx, id)?;
        Ok(Intake {
            id,
            duplicate: existed && !adds_body,
            readiness: readiness(tx, id)?.ok_or(Error::CorruptRecord)?,
        })
    }
    /// Bounded, cursor-based work for the background writer. Missing proofs cannot starve later rows.
    pub fn reconcile(
        &self,
        tx: &Transaction<'_>,
        after: Option<EventId>,
        limit: usize,
        resolver: &dyn Resolver,
    ) -> Result<Batch> {
        if limit == 0 || limit > MAX_BATCH {
            return Err(Error::InvalidLimit);
        }
        let mut statement = tx.prepare("SELECT r.id,r.envelope FROM events_raw r LEFT JOIN events_links l ON l.id=r.id WHERE r.state=0 AND (l.id IS NULL OR l.connected=0) AND (?1 IS NULL OR r.id>?1) ORDER BY r.id LIMIT ?2")?;
        let rows = statement
            .query_map(
                params![after.as_ref().map(|id| id.bytes().as_slice()), limit as i64],
                |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Vec<u8>>(1)?)),
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(statement);
        let mut changed = 0;
        for (id, encoded) in &rows {
            let id = event_id(id)?;
            let was = readiness(tx, id)?;
            bind(tx, codec::verify_signature(encoded)?, resolver)?;
            connect(tx, id)?;
            if readiness(tx, id)? != was {
                changed += 1;
            }
        }
        Ok(Batch {
            visited: rows.len(),
            changed,
            next: if rows.len() == limit {
                Some(event_id(&rows.last().expect("nonempty").0)?)
            } else {
                None
            },
        })
    }
}

fn event_id(bytes: &[u8]) -> Result<EventId> {
    Ok(EventId::from_bytes(
        bytes.try_into().map_err(|_| Error::CorruptRecord)?,
    ))
}
fn sequence(bytes: &[u8]) -> Result<u64> {
    Ok(u64::from_be_bytes(
        bytes.try_into().map_err(|_| Error::CorruptRecord)?,
    ))
}

fn bind(tx: &Transaction<'_>, signed: SignedEnvelope, resolver: &dyn Resolver) -> Result<()> {
    let id = signed.id();
    let state: i64 = tx.query_row(
        "SELECT state FROM events_raw WHERE id=?1",
        [id.bytes().as_slice()],
        |r| r.get(0),
    )?;
    if state != 0 {
        return Ok(());
    }
    let bound: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM events_links WHERE id=?1)",
        [id.bytes().as_slice()],
        |r| r.get(0),
    )?;
    if bound {
        return Ok(());
    }
    match signed.bind(|id| resolver.resolve(tx, id)) {
        Ok(commitment) => {
            let m = commitment.metadata();
            tx.execute("INSERT OR IGNORE INTO events_links(id,stream,sequence,previous,resource,audience,distribution) VALUES(?1,?2,?3,?4,?5,?6,?7)", params![id.bytes().as_slice(), m.stream.as_slice(), m.sequence.to_be_bytes().as_slice(), m.previous.as_ref().map(|id| id.bytes().as_slice()), m.scope.resource, m.scope.audience, m.scope.distribution as u8])?;
            // Only verified identities can contribute equivocation evidence.
            let conflicts: i64 = tx.query_row(
                "SELECT count(*) FROM events_links WHERE stream=?1 AND sequence=?2",
                params![m.stream.as_slice(), m.sequence.to_be_bytes().as_slice()],
                |r| r.get(0),
            )?;
            if conflicts > 1 {
                tx.execute("INSERT INTO events_forks(stream,first_sequence) VALUES(?1,?2) ON CONFLICT(stream) DO UPDATE SET first_sequence=min(first_sequence,excluded.first_sequence)", params![m.stream.as_slice(), m.sequence.to_be_bytes().as_slice()])?;
            }
        }
        Err(codec::Error::MissingCredential(_)) => (),
        Err(codec::Error::ResolverUnavailable) => {
            return Err(codec::Error::ResolverUnavailable.into());
        }
        Err(_) => {
            tx.execute(
                "UPDATE events_raw SET state=2 WHERE id=?1",
                [id.bytes().as_slice()],
            )?;
        }
    }
    Ok(())
}
fn connect(tx: &Transaction<'_>, id: EventId) -> Result<()> {
    type Link = (Vec<u8>, Vec<u8>, Option<Vec<u8>>, i64);
    let row: Option<Link> = tx
        .query_row(
            "SELECT stream,sequence,previous,connected FROM events_links WHERE id=?1",
            [id.bytes().as_slice()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    let Some((stream, seq, previous, connected)) = row else {
        return Ok(());
    };
    if connected != 0 {
        return Ok(());
    }
    let number = sequence(&seq)?;
    let next = if let Some(previous) = previous {
        let parent: Option<(Vec<u8>, Vec<u8>, i64)> = tx
            .query_row(
                "SELECT stream,sequence,connected FROM events_links WHERE id=?1",
                [previous],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        match parent {
            None => 0,
            Some((parent_stream, parent_seq, parent_state)) => {
                if parent_stream != stream
                    || sequence(&parent_seq)?.checked_add(1) != Some(number)
                    || parent_state == 2
                {
                    2
                } else if parent_state == 1 {
                    1
                } else {
                    0
                }
            }
        }
    } else if number == 1 {
        1
    } else {
        2
    };
    tx.execute(
        "UPDATE events_links SET connected=?2 WHERE id=?1",
        params![id.bytes().as_slice(), next],
    )?;
    Ok(())
}

/// Readiness never grants semantic authorization, serves private content or earns delivery credit.
pub fn readiness(connection: &Connection, id: EventId) -> Result<Option<Readiness>> {
    type Row = (
        i64,
        Vec<u8>,
        bool,
        Option<Vec<u8>>,
        Option<Vec<u8>>,
        Option<Vec<u8>>,
        Option<i64>,
        Option<Vec<u8>>,
    );
    let row: Option<Row> = connection.query_row("SELECT r.state,r.envelope,r.body IS NOT NULL,l.stream,l.sequence,l.previous,l.connected,f.first_sequence FROM events_raw r LEFT JOIN events_links l ON l.id=r.id LEFT JOIN events_forks f ON f.stream=l.stream WHERE r.id=?1", [id.bytes().as_slice()], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?))).optional()?;
    let Some((state, envelope, has_body, stream, seq, previous, connected, fork)) = row else {
        return Ok(None);
    };
    if state == 3 {
        return Ok(Some(Readiness::Unsupported));
    }
    if state == 2 {
        return Ok(Some(Readiness::InvalidDelegation));
    }
    if stream.is_none() {
        let signed = codec::verify_signature(&envelope)?;
        return Ok(Some(Readiness::MissingCredential(
            signed
                .claims()
                .actor
                .credential
                .ok_or(Error::CorruptRecord)?,
        )));
    }
    let seq = sequence(&seq.ok_or(Error::CorruptRecord)?)?;
    if let Some(fork) = fork {
        let first_sequence = sequence(&fork)?;
        if seq >= first_sequence {
            return Ok(Some(Readiness::Forked { first_sequence }));
        }
    }
    Ok(Some(match connected {
        Some(2) => Readiness::InvalidChain,
        Some(0) => Readiness::MissingPrevious(event_id(&previous.ok_or(Error::CorruptRecord)?)?),
        Some(1) if has_body => Readiness::Ready,
        Some(1) => Readiness::CommitmentOnly,
        _ => return Err(Error::CorruptRecord),
    }))
}

pub fn first_body_delivery(connection: &Connection, id: EventId) -> Result<Option<Delivery>> {
    Ok(connection
        .query_row(
            "SELECT body_source,body_transfer FROM events_raw WHERE id=?1 AND body IS NOT NULL",
            [id.bytes().as_slice()],
            |r| {
                Ok(Delivery {
                    source: r.get(0)?,
                    transfer: r.get(1)?,
                })
            },
        )
        .optional()?)
}
pub fn forks(connection: &Connection) -> Result<Vec<Fork>> {
    let mut statement =
        connection.prepare("SELECT stream,first_sequence FROM events_forks ORDER BY stream")?;
    let rows = statement
        .query_map([], |r| {
            Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(stream, seq)| {
            Ok(Fork {
                stream: stream.try_into().map_err(|_| Error::CorruptRecord)?,
                first_sequence: sequence(&seq)?,
            })
        })
        .collect()
}
/// Explicit public-protocol lookup, not a feature/UI/export read or audience permission.
pub fn commitment(
    connection: &Connection,
    id: EventId,
    resolver: &dyn Resolver,
) -> Result<Option<Commitment>> {
    let encoded: Option<Vec<u8>> = connection
        .query_row(
            "SELECT envelope FROM events_raw WHERE id=?1 AND state=0",
            [id.bytes().as_slice()],
            |r| r.get(0),
        )
        .optional()?;
    encoded
        .map(|bytes| {
            codec::verify_with_resolver(&bytes, |id| resolver.resolve(connection, id))
                .map_err(Error::from)
        })
        .transpose()
}
pub fn metadata(
    connection: &Connection,
    id: EventId,
    resolver: &dyn Resolver,
) -> Result<Option<Metadata>> {
    Ok(commitment(connection, id, resolver)?.map(|c| c.metadata().clone()))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub sequence: u64,
    pub id: EventId,
}
/// Backend range inventory for an already-authorized scope. This API never authorizes export;
/// the caller must separately apply local/shared and audience policy. Both fork hashes are listed.
pub fn inventory(
    connection: &Connection,
    scope: &codec::Scope,
    stream: [u8; 32],
    after: Option<Position>,
    limit: usize,
) -> Result<Vec<Position>> {
    if limit == 0 || limit > MAX_BATCH {
        return Err(Error::InvalidLimit);
    }
    let seq = after.map(|p| p.sequence.to_be_bytes());
    let mut statement = connection.prepare("SELECT sequence,id FROM events_links WHERE resource=?1 AND audience=?2 AND distribution=?3 AND stream=?4 AND (?5 IS NULL OR (sequence,id)>(?5,?6)) ORDER BY sequence,id LIMIT ?7")?;
    let rows = statement
        .query_map(
            params![
                scope.resource,
                scope.audience,
                scope.distribution as u8,
                stream.as_slice(),
                seq.as_ref().map(|s| s.as_slice()),
                after.as_ref().map(|p| p.id.bytes().as_slice()),
                limit as i64
            ],
            |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Vec<u8>>(1)?)),
        )?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(seq, id)| {
            Ok(Position {
                sequence: sequence(&seq)?,
                id: event_id(&id)?,
            })
        })
        .collect()
}
