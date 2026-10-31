// SPDX-License-Identifier: AGPL-3.0-or-later
//! Locally authored actions allocate scoped positions and persist their signed bytes in one transaction.
use crate::{
    Delivery, Error, Inbox, Resolver, Result,
    integration::{self, Module},
};
use bbsbox_credentials::Credential;
use bbsbox_event_codec::{
    self as codec, Actor, CompleteEvent, Dependency, Draft, Extension, Scope,
};
use rusqlite::{OptionalExtension, Transaction, params};

pub struct Action {
    pub kind: String,
    pub schema: u16,
    pub scope: Scope,
    pub dependencies: Vec<Dependency>,
    pub claimed_time: i64,
    pub extensions: Vec<Extension>,
}
pub struct Authored<'a> {
    pub action: Action,
    pub actor: Actor,
    pub credential: Option<&'a Credential>,
    pub body: &'a [u8],
    pub delivery: &'a Delivery,
}

/// The actor/incarnation comes from the owning current signer, never a restored database counter.
/// Call within C01's idempotent command transaction and publish the returned bytes only after commit.
/// A failed later reducer rolls this allocation back too. Credential installation stays M03-owned.
pub fn author(
    tx: &Transaction<'_>,
    inbox: Inbox,
    request: Authored<'_>,
    resolver: &dyn Resolver,
    modules: &[&dyn Module],
    sign: impl FnOnce(&[u8]) -> codec::Result<[u8; 64]>,
) -> Result<CompleteEvent> {
    let Authored {
        action,
        actor,
        credential,
        body,
        delivery,
    } = request;
    let stream = codec::stream_id(&actor, &action.scope)?;
    let forked: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM events_forks WHERE stream=?1)",
        [stream.as_slice()],
        |r| r.get(0),
    )?;
    if forked {
        return Err(Error::ForkedStream);
    }
    let head:Option<(Vec<u8>,Vec<u8>,i64)>=tx.query_row("SELECT id,sequence,connected FROM events_links WHERE stream=?1 ORDER BY sequence DESC,id DESC LIMIT 1",[stream.as_slice()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    let (sequence, previous) = if let Some((id, sequence, connected)) = head {
        let sequence = crate::sequence(&sequence)?
            .checked_add(1)
            .ok_or(Error::CounterExhausted)?;
        if connected != 1 {
            return Err(Error::StreamGap);
        }
        (sequence, Some(crate::event_id(&id)?))
    } else {
        (1, None)
    };
    let event = codec::sign(
        Draft {
            kind: action.kind,
            schema: action.schema,
            scope: action.scope,
            dependencies: action.dependencies,
            claimed_time: action.claimed_time,
            extensions: action.extensions,
            sequence,
            previous,
        },
        actor,
        body,
        credential,
        sign,
    )?;
    integration::receive(
        tx,
        inbox,
        event.commitment().encoded(),
        Some(event.body()),
        delivery,
        resolver,
        modules,
    )?;
    // The selected actor must have been resolvable into an authenticated position in this transaction.
    let stored: Option<Vec<u8>> = tx
        .query_row(
            "SELECT id FROM events_links WHERE id=?1 AND stream=?2",
            params![event.id().bytes().as_slice(), stream.as_slice()],
            |r| r.get(0),
        )
        .optional()?;
    if stored.is_none() {
        return Err(Error::CorruptRecord);
    }
    Ok(event)
}
