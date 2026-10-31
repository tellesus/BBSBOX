// SPDX-License-Identifier: AGPL-3.0-or-later
//! Module-owned authorization/effects run inside the same transaction as immutable acceptance receipts.
use crate::{
    Delivery, Error, Fork, Inbox, Intake, MAX_BATCH, Readiness, Resolver, Result, event_id,
    first_body_delivery, readiness, sequence, valid_token,
};
use bbsbox_credentials::CredentialId;
use bbsbox_event_codec::{
    self as codec, Commitment, CompleteEvent, Dependency, DependencyKind, EventId, Extension,
    authority::{Authority, Decision, Outcome, Registry},
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Relevance {
    Obsolete = 0,
    Relevant = 1,
    DependencyOnly = 2,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub module: String,
    pub relevance: Relevance,
    pub delivery: Delivery,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Integration {
    Accepted(Receipt),
    Duplicate(Receipt),
    Staged(Vec<Dependency>),
    CommitmentOnly,
    Unsupported,
    Rejected(String),
    MissingEvent,
    RepairPending,
}

/// Compiled feature adapter. Its SQL owns projections, causal rules and relevance. All callbacks
/// execute in the supplied transaction; they must not submit nested writer work or do external I/O.
pub trait Module {
    fn id(&self) -> &str;
    fn registry(&self) -> &Registry;
    fn action(&self, connection: &Connection, event: &CompleteEvent) -> Decision;
    fn extension(
        &self,
        _connection: &Connection,
        _event: &CompleteEvent,
        _extension: &Extension,
    ) -> Decision {
        Decision::Deny("extension_handler_missing")
    }
    /// Required for blob scope/presence and any feature-specific dependency admission.
    fn dependency(
        &self,
        connection: &Connection,
        event: &CompleteEvent,
        dependency: Dependency,
    ) -> Decision;
    /// Default denies cross-scope references. An override may admit separately authorized control
    /// proofs only; never hidden content. It sees the signed commitment, without fetching the body.
    fn cross_scope_control(
        &self,
        _connection: &Connection,
        _event: &CompleteEvent,
        _proof: &Commitment,
    ) -> bool {
        false
    }
    /// True only if this accepted event still supplies valid domain state under current causal policy.
    fn dependency_effective(&self, connection: &Connection, id: EventId) -> Result<bool>;
    fn reduce(&self, tx: &Transaction<'_>, event: &CompleteEvent) -> Result<Relevance>;
    /// Atomically hide/rebuild affected projections and their semantic dependents. This includes
    /// earlier accepted effects; a later replay must not make arrival order choose the fork winner.
    fn invalidate_fork(&self, tx: &Transaction<'_>, fork: &Fork) -> Result<()>;
    /// Future M16 adapters may record earned credit here, in this transaction. This runs exactly
    /// once per relevant acceptance, never for retained bytes, dependencies, obsolete work or replay.
    fn relevant(
        &self,
        _tx: &Transaction<'_>,
        _event: &CompleteEvent,
        _delivery: &Delivery,
    ) -> Result<()> {
        Ok(())
    }
}
struct Policy<'a> {
    tx: &'a Connection,
    module: &'a dyn Module,
}
impl Authority for Policy<'_> {
    fn action(&self, event: &CompleteEvent) -> Decision {
        self.module.action(self.tx, event)
    }
    fn extension(&self, event: &CompleteEvent, extension: &Extension) -> Decision {
        self.module.extension(self.tx, event, extension)
    }
}
fn validate(module: &dyn Module) -> Result<()> {
    if valid_token(module.id()) {
        Ok(())
    } else {
        Err(Error::WrongModule)
    }
}
fn validate_modules(modules: &[&dyn Module]) -> Result<()> {
    let mut ids = std::collections::BTreeSet::new();
    for module in modules {
        validate(*module)?;
        if !ids.insert(module.id()) {
            return Err(Error::WrongModule);
        }
    }
    Ok(())
}

/// One bounded batch of previously unseen/earlier fork floors. All compiled affected modules must
/// participate, even when disabled. Propagate errors to roll back both intake and invalidation.
pub fn synchronize_forks(tx: &Transaction<'_>, module: &dyn Module) -> Result<bool> {
    validate(module)?;
    let mut statement=tx.prepare("SELECT f.stream,f.first_sequence FROM events_forks f LEFT JOIN events_module_forks m ON m.stream=f.stream AND m.module=?1 WHERE m.stream IS NULL OR f.first_sequence<m.first_sequence ORDER BY f.stream LIMIT ?2")?;
    let rows = statement
        .query_map(params![module.id(), MAX_BATCH as i64], |r| {
            Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    drop(statement);
    for (stream, seq) in &rows {
        let fork = Fork {
            stream: stream
                .as_slice()
                .try_into()
                .map_err(|_| Error::CorruptRecord)?,
            first_sequence: sequence(seq)?,
        };
        module.invalidate_fork(tx, &fork)?;
        tx.execute("INSERT INTO events_module_forks VALUES(?1,?2,?3) ON CONFLICT(module,stream) DO UPDATE SET first_sequence=excluded.first_sequence",params![module.id(),stream,seq])?;
    }
    Ok(!fork_repair_pending(tx, module.id())?)
}
pub fn fork_repair_pending(connection: &Connection, module: &str) -> Result<bool> {
    Ok(connection.query_row("SELECT EXISTS(SELECT 1 FROM events_forks f LEFT JOIN events_module_forks m ON m.stream=f.stream AND m.module=?1 WHERE m.stream IS NULL OR f.first_sequence<m.first_sequence)",[module],|r|r.get(0))?)
}
/// Use before ordinary projection reads while initializing/enabling a module over retained history.
/// Active intake/reconciliation must also include every compiled module in its atomic invalidation.
pub fn ensure_projection_current(connection: &Connection, module: &str) -> Result<()> {
    if !valid_token(module) {
        return Err(Error::WrongModule);
    }
    if fork_repair_pending(connection, module)? {
        Err(Error::RepairBacklog)
    } else {
        Ok(())
    }
}

/// Active projection databases use this entry point rather than raw Inbox::receive, so a newly
/// discovered fork and every affected module's invalidation commit together.
pub fn receive(
    tx: &Transaction<'_>,
    inbox: Inbox,
    encoded: &[u8],
    body: Option<&[u8]>,
    delivery: &Delivery,
    resolver: &dyn Resolver,
    modules: &[&dyn Module],
) -> Result<Intake> {
    validate_modules(modules)?;
    let result = inbox.receive(tx, encoded, body, delivery, resolver)?;
    for module in modules {
        if !synchronize_forks(tx, *module)? {
            return Err(Error::RepairBacklog);
        }
    }
    Ok(result)
}
pub fn reconcile(
    tx: &Transaction<'_>,
    inbox: Inbox,
    after: Option<EventId>,
    limit: usize,
    resolver: &dyn Resolver,
    modules: &[&dyn Module],
) -> Result<crate::Batch> {
    validate_modules(modules)?;
    let batch = inbox.reconcile(tx, after, limit, resolver)?;
    for module in modules {
        if !synchronize_forks(tx, *module)? {
            return Err(Error::RepairBacklog);
        }
    }
    Ok(batch)
}

pub fn receipt(connection: &Connection, id: EventId) -> Result<Option<Receipt>> {
    let row: Option<(String, u8, String, String)> = connection
        .query_row(
            "SELECT module,relevance,source,transfer FROM events_receipts WHERE event_id=?1",
            [id.bytes().as_slice()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    row.map(|(module, relevance, source, transfer)| {
        Ok(Receipt {
            module,
            relevance: match relevance {
                0 => Relevance::Obsolete,
                1 => Relevance::Relevant,
                2 => Relevance::DependencyOnly,
                _ => return Err(Error::CorruptRecord),
            },
            delivery: Delivery { source, transfer },
        })
    })
    .transpose()
}
fn missing(mut dependencies: Vec<Dependency>) -> Integration {
    dependencies.sort();
    dependencies.dedup();
    Integration::Staged(dependencies)
}
fn decision(decision: Decision) -> Option<Integration> {
    match decision {
        Decision::Allow => None,
        Decision::Deny(reason) => Some(Integration::Rejected(reason.into())),
        Decision::Missing(deps) if !deps.is_empty() && deps.len() <= codec::MAX_DEPENDENCIES => {
            Some(missing(deps))
        }
        Decision::Missing(_) => Some(Integration::Rejected("invalid_missing_dependencies".into())),
    }
}

/// Call after delivery intake, on the single writer. All pending outcomes are retryable after
/// dependencies/policy/modules change. An accepted receipt is immutable, including after a fork;
/// projection validity is separately repaired and the historical receipt never awards credit twice.
pub fn integrate(
    tx: &Transaction<'_>,
    id: EventId,
    module: &dyn Module,
    resolver: &dyn Resolver,
) -> Result<Integration> {
    validate(module)?;
    if !synchronize_forks(tx, module)? {
        return Ok(Integration::RepairPending);
    }
    if let Some(old) = receipt(tx, id)? {
        if old.module != module.id() {
            return Err(Error::WrongModule);
        }
        return Ok(Integration::Duplicate(old));
    }
    match readiness(tx, id)? {
        None => return Ok(Integration::MissingEvent),
        Some(Readiness::Unsupported) => return Ok(Integration::Unsupported),
        Some(Readiness::InvalidDelegation) => {
            return Ok(Integration::Rejected("invalid_delegation".into()));
        }
        Some(Readiness::InvalidChain) => return Ok(Integration::Rejected("invalid_chain".into())),
        Some(Readiness::Forked { .. }) => return Ok(Integration::Rejected("forked_stream".into())),
        Some(Readiness::MissingCredential(id)) => {
            return Ok(missing(vec![Dependency::credential(id)]));
        }
        Some(Readiness::MissingPrevious(id)) => return Ok(missing(vec![Dependency::event(id)])),
        Some(Readiness::CommitmentOnly) => return Ok(Integration::CommitmentOnly),
        Some(Readiness::Ready) => (),
    }
    let event = complete(tx, id, resolver)?.ok_or(Error::CorruptRecord)?;
    match module.registry().authorize(&event, &Policy { tx, module }) {
        Outcome::Authorized => (),
        Outcome::Denied(reason) => return Ok(Integration::Rejected(reason.into())),
        Outcome::Missing(deps) => return Ok(missing(deps)),
        _ => return Ok(Integration::Unsupported),
    }
    let mut absent = vec![];
    for dependency in &event.metadata().dependencies {
        match dependency.kind {
            DependencyKind::Event => {
                let target = EventId::from_bytes(dependency.digest);
                let proof = match crate::commitment(tx, target, resolver) {
                    Ok(Some(proof)) => proof,
                    Ok(None) | Err(Error::Codec(codec::Error::MissingCredential(_))) => {
                        absent.push(*dependency);
                        continue;
                    }
                    Err(error) => return Err(error),
                };
                if proof.metadata().scope != event.metadata().scope
                    && !module.cross_scope_control(tx, &event, &proof)
                {
                    return Ok(Integration::Rejected(
                        "cross_scope_content_dependency".into(),
                    ));
                }
                if let Some(
                    Readiness::Forked { .. }
                    | Readiness::InvalidChain
                    | Readiness::InvalidDelegation,
                ) = readiness(tx, target)?
                {
                    return Ok(Integration::Rejected("invalid_dependency".into()));
                }
                if receipt(tx, target)?.is_none() || !module.dependency_effective(tx, target)? {
                    absent.push(*dependency);
                    continue;
                }
            }
            DependencyKind::Credential => {
                let target = CredentialId::from_bytes(dependency.digest);
                match resolver.resolve(tx, target)? {
                    Some(credential) if credential.id() == target => (),
                    Some(_) => return Err(Error::CorruptRecord),
                    None => {
                        absent.push(*dependency);
                        continue;
                    }
                }
            }
            DependencyKind::Blob => (), // Module must check existence AND resource visibility below.
        }
        if let Some(outcome) = decision(module.dependency(tx, &event, *dependency)) {
            return Ok(outcome);
        }
    }
    if !absent.is_empty() {
        return Ok(missing(absent));
    }
    let relevance = module.reduce(tx, &event)?;
    let delivery = first_body_delivery(tx, id)?.ok_or(Error::CorruptRecord)?;
    tx.execute(
        "INSERT INTO events_receipts VALUES(?1,?2,?3,?4,?5)",
        params![
            id.bytes().as_slice(),
            module.id(),
            relevance as u8,
            delivery.source,
            delivery.transfer
        ],
    )?;
    if relevance == Relevance::Relevant {
        module.relevant(tx, &event, &delivery)?;
    }
    // Accepted history moves out of the bounded pending intake budget, never out of disk accounting.
    let charged:Option<i64>=tx.query_row("SELECT length(envelope)+coalesce(length(body),0) FROM events_raw WHERE id=?1 AND charged=1",[id.bytes().as_slice()],|r|r.get(0)).optional()?;
    if let Some(bytes) = charged {
        tx.execute(
            "UPDATE events_budget SET bytes=bytes-?1 WHERE singleton=1",
            [bytes],
        )?;
        tx.execute(
            "UPDATE events_raw SET charged=0 WHERE id=?1",
            [id.bytes().as_slice()],
        )?;
    }
    Ok(Integration::Accepted(Receipt {
        module: module.id().into(),
        relevance,
        delivery,
    }))
}

pub fn complete(
    connection: &Connection,
    id: EventId,
    resolver: &dyn Resolver,
) -> Result<Option<CompleteEvent>> {
    let row: Option<(Vec<u8>, Vec<u8>)> = connection
        .query_row(
            "SELECT envelope,body FROM events_raw WHERE id=?1 AND body IS NOT NULL AND state=0",
            [id.bytes().as_slice()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    row.map(|(envelope, body)| {
        codec::verify_with_resolver(&envelope, |id| resolver.resolve(connection, id))?
            .with_body(&body)
            .map_err(Error::from)
    })
    .transpose()
}

/// Bounded pending semantic scan; acceptance receipts, not intake order, determine which rows remain.
pub fn pending(
    connection: &Connection,
    after: Option<EventId>,
    limit: usize,
) -> Result<Vec<EventId>> {
    if limit == 0 || limit > MAX_BATCH {
        return Err(Error::InvalidLimit);
    }
    let mut statement=connection.prepare("SELECT r.id FROM events_raw r LEFT JOIN events_receipts a ON a.event_id=r.id WHERE a.event_id IS NULL AND r.state=0 AND (?1 IS NULL OR r.id>?1) ORDER BY r.id LIMIT ?2")?;
    let rows = statement
        .query_map(
            params![after.as_ref().map(|id| id.bytes().as_slice()), limit as i64],
            |r| r.get::<_, Vec<u8>>(0),
        )?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    rows.iter().map(|bytes| event_id(bytes)).collect()
}
