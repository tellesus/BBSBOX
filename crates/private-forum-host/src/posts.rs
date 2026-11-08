// SPDX-License-Identifier: AGPL-3.0-or-later
//! Bounded opaque create/reply admission. Edits, withdrawal, transport and plaintext stay separate.
use super::Host;
use bbsbox_credentials::{ForumId, Purpose};
use bbsbox_event_codec::{
    CompleteEvent, Dependency, DependencyKind, Distribution, EventId, Extension, Role,
    authority::{Decision, KindRule, Registry},
};
use bbsbox_event_core::{
    self as events, Fork, Readiness,
    integration::{self, Module, Relevance},
};
use bbsbox_private_forum::{Action, HEADER_EXTENSION, Outer, governance::Governance};
use rusqlite::{Connection, Transaction, params};
use std::collections::{BTreeMap, BTreeSet};

pub struct Posts<'a, 'h> {
    host: &'a Host<'h>,
    limit: usize,
    registry: Registry,
}

/// Public routing state only. A valid outer envelope says nothing about decryptability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Post {
    pub id: EventId,
    pub parent: Option<EventId>,
    pub epoch: u64,
    pub observed_epoch: u64,
}

struct Record {
    outer: Outer,
    previous: Option<EventId>,
}
impl Record {
    fn inspect(event: &CompleteEvent) -> events::Result<Self> {
        let outer = Outer::inspect(event).map_err(|_| events::Error::CorruptRecord)?;
        if !matches!(outer.header.action, Action::Post | Action::Reply { .. }) {
            return Err(events::Error::CorruptRecord);
        }
        Ok(Self {
            outer,
            previous: event.metadata().previous,
        })
    }
    fn parent(&self) -> Option<EventId> {
        match self.outer.header.action {
            Action::Reply { parent } => Some(parent),
            _ => None,
        }
    }
}

#[derive(Clone)]
enum State {
    Valid(Post),
    Missing(Vec<Dependency>),
    Denied(&'static str),
}

/// Re-evaluate the complete bounded causal graph after governance or stream conflicts.
/// Receipts remain immutable; derived visibility and dependency validity may change.
struct View {
    records: BTreeMap<EventId, Record>,
    states: BTreeMap<EventId, State>,
}
impl View {
    fn evaluate(&mut self, history: &Governance) -> events::Result<()> {
        // Iterative passes avoid attacker-controlled recursion depth. Each pass finalizes at least
        // one row, or all unresolved rows are blocked on absent/cyclic semantic predecessors.
        let mut remaining = self.records.keys().copied().collect::<BTreeSet<_>>();
        while !remaining.is_empty() {
            let mut progressed = false;
            for id in remaining.clone() {
                let row = &self.records[&id];
                let control = row.outer.header.control;
                let Some(anchor) = history
                    .epochs()
                    .iter()
                    .position(|epoch| epoch.control().id() == control)
                else {
                    self.states
                        .insert(id, State::Missing(vec![Dependency::event(control)]));
                    remaining.remove(&id);
                    progressed = true;
                    continue;
                };
                if history.epochs()[anchor].post_author(&row.outer).is_none() {
                    self.states.insert(id, State::Denied("not_an_epoch_member"));
                    remaining.remove(&id);
                    progressed = true;
                    continue;
                }
                let dependencies = [row.previous, row.parent()].into_iter().flatten();
                let mut observed = row.outer.header.epoch;
                let mut missing = Vec::new();
                let mut waiting = false;
                for dependency in dependencies {
                    match self.states.get(&dependency) {
                        Some(State::Valid(prior)) => observed = observed.max(prior.observed_epoch),
                        Some(_) => missing.push(Dependency::event(dependency)),
                        None if remaining.contains(&dependency) => waiting = true,
                        None => missing.push(Dependency::event(dependency)),
                    }
                }
                if waiting {
                    continue;
                }
                let state = if !missing.is_empty() {
                    missing.sort_unstable();
                    missing.dedup();
                    State::Missing(missing)
                } else if history.epochs()[anchor..].iter().any(|epoch| {
                    epoch.control().header().epoch <= observed
                        && epoch.member(row.outer.credential).is_none()
                }) {
                    State::Denied("causally_observed_membership_removal")
                } else {
                    State::Valid(Post {
                        id,
                        parent: row.parent(),
                        epoch: row.outer.header.epoch,
                        observed_epoch: observed,
                    })
                };
                self.states.insert(id, state);
                remaining.remove(&id);
                progressed = true;
            }
            if !progressed {
                for id in &remaining {
                    let row = &self.records[id];
                    let missing = [row.previous, row.parent()]
                        .into_iter()
                        .flatten()
                        .filter(|dependency| remaining.contains(dependency))
                        .map(Dependency::event)
                        .collect();
                    self.states.insert(*id, State::Missing(missing));
                }
                break;
            }
        }
        Ok(())
    }
}

impl<'a, 'h> Posts<'a, 'h> {
    /// Initial provider keeps at most `limit` accepted create/reply records per hosted scope.
    /// This is a capacity fence; paged/unbounded history needs a later provider.
    pub fn new(host: &'a Host<'h>, limit: usize) -> events::Result<Self> {
        if limit == 0 || limit > 4096 {
            return Err(events::Error::InvalidLimit);
        }
        let registry = Registry::new(
            ["forum.post.create", "forum.post.reply"]
                .into_iter()
                .map(|kind| KindRule {
                    kind: kind.into(),
                    schema: 1,
                    actor: Role::PersonDevice,
                    purpose: Some(Purpose::Authorship),
                })
                .collect(),
            vec![HEADER_EXTENSION],
        )
        .expect("valid private post rules");
        Ok(Self {
            host,
            limit,
            registry,
        })
    }

    fn view(
        &self,
        connection: &Connection,
        forum: ForumId,
        distribution: Distribution,
        candidate: Option<&CompleteEvent>,
    ) -> events::Result<View> {
        integration::ensure_projection_current(connection, self.id())?;
        let history = self.host.history(connection, forum, distribution)?;
        let mut statement = connection.prepare("SELECT p.event_id FROM private_forum_posts p JOIN events_receipts r ON r.event_id=p.event_id AND r.module='private-forum-posts' WHERE p.forum=?1 AND p.distribution=?2 ORDER BY p.event_id LIMIT ?3")?;
        let rows = statement
            .query_map(
                params![
                    forum.to_string(),
                    distribution as u8,
                    (self.limit + 1) as i64
                ],
                |row| row.get::<_, Vec<u8>>(0),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if rows.len() > self.limit {
            return Err(events::Error::InvalidLimit);
        }
        let mut view = View {
            records: BTreeMap::new(),
            states: BTreeMap::new(),
        };
        for bytes in rows {
            let id =
                EventId::from_bytes(bytes.try_into().map_err(|_| events::Error::CorruptRecord)?);
            if events::readiness(connection, id)? != Some(Readiness::Ready) {
                continue;
            }
            let event = integration::complete(connection, id, self.host.resolver)?
                .ok_or(events::Error::CorruptRecord)?;
            let row = Record::inspect(&event)?;
            if row.outer.forum != forum || row.outer.distribution != distribution {
                return Err(events::Error::CorruptRecord);
            }
            view.records.insert(id, row);
        }
        if let Some(event) = candidate {
            view.records.insert(event.id(), Record::inspect(event)?);
        }
        view.evaluate(&history)?;
        Ok(view)
    }

    pub fn admitted(
        &self,
        connection: &Connection,
        forum: ForumId,
        distribution: Distribution,
    ) -> events::Result<Vec<Post>> {
        Ok(self
            .view(connection, forum, distribution, None)?
            .states
            .into_values()
            .filter_map(|state| {
                if let State::Valid(post) = state {
                    Some(post)
                } else {
                    None
                }
            })
            .collect())
    }
}

impl Module for Posts<'_, '_> {
    fn id(&self) -> &str {
        "private-forum-posts"
    }
    fn registry(&self) -> &Registry {
        &self.registry
    }
    fn action(&self, connection: &Connection, event: &CompleteEvent) -> events::Result<Decision> {
        let Ok(outer) = Outer::inspect(event) else {
            return Ok(Decision::Deny("invalid_forum_post"));
        };
        if !self
            .host
            .forums
            .contains(&(outer.forum, outer.distribution))
        {
            return Ok(Decision::Deny("forum_not_hosted"));
        }
        for dependency in &event.metadata().dependencies {
            let proof = match events::commitment(
                connection,
                EventId::from_bytes(dependency.digest),
                self.host.resolver,
            ) {
                Ok(proof) => proof,
                Err(events::Error::Codec(bbsbox_event_codec::Error::MissingCredential(_))) => None,
                Err(error) => return Err(error),
            };
            if let Some(proof) = proof
                && proof.metadata().scope != event.metadata().scope
            {
                return Ok(Decision::Deny("cross_scope_content_dependency"));
            }
        }
        let retained: i64 = connection.query_row(
            "SELECT count(*) FROM private_forum_posts WHERE forum=?1 AND distribution=?2",
            params![outer.forum.to_string(), outer.distribution as u8],
            |row| row.get(0),
        )?;
        if retained >= self.limit as i64 {
            return Err(events::Error::InvalidLimit);
        }
        let view = self.view(connection, outer.forum, outer.distribution, Some(event))?;
        Ok(
            match view
                .states
                .get(&event.id())
                .ok_or(events::Error::CorruptRecord)?
            {
                State::Valid(_) => Decision::Allow,
                State::Missing(dependencies) => Decision::Missing(dependencies.clone()),
                State::Denied(reason) => Decision::Deny(reason),
            },
        )
    }
    fn extension(
        &self,
        _: &Connection,
        event: &CompleteEvent,
        _: &Extension,
    ) -> events::Result<Decision> {
        Ok(if Outer::inspect(event).is_ok() {
            Decision::Allow
        } else {
            Decision::Deny("invalid_forum_post")
        })
    }
    fn dependency(
        &self,
        _: &Connection,
        _: &CompleteEvent,
        dependency: Dependency,
    ) -> events::Result<Decision> {
        Ok(match dependency.kind {
            DependencyKind::Event => Decision::Allow,
            _ => Decision::Deny("unexpected_post_dependency"),
        })
    }
    fn dependency_effective(&self, connection: &Connection, id: EventId) -> events::Result<bool> {
        if self.host.dependency_effective(connection, id)? {
            return Ok(true);
        }
        let Some(event) = integration::complete(connection, id, self.host.resolver)? else {
            return Ok(false);
        };
        let Ok(outer) = Outer::inspect(&event) else {
            return Ok(false);
        };
        if !self
            .host
            .forums
            .contains(&(outer.forum, outer.distribution))
        {
            return Ok(false);
        }
        Ok(self
            .admitted(connection, outer.forum, outer.distribution)?
            .iter()
            .any(|post| post.id == id))
    }
    fn reduce(&self, tx: &Transaction<'_>, event: &CompleteEvent) -> events::Result<Relevance> {
        let outer = Outer::inspect(event).map_err(|_| events::Error::CorruptRecord)?;
        tx.execute(
            "INSERT INTO private_forum_posts VALUES(?1,?2,?3)",
            params![
                event.id().bytes().as_slice(),
                outer.forum.to_string(),
                outer.distribution as u8
            ],
        )?;
        Ok(Relevance::Relevant)
    }
    fn invalidate_fork(&self, _: &Transaction<'_>, _: &Fork) -> events::Result<()> {
        // Reads reconstruct against both current M04 readiness and the owner-led history.
        Ok(())
    }
}
