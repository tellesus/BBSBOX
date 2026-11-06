// SPDX-License-Identifier: AGPL-3.0-or-later
//! Durable public governance indexes over M04's retained signed, opaque events.
use bbsbox_credentials::{Credential, CredentialId, ForumId};
use bbsbox_event_codec::{
    CompleteEvent, Dependency, DependencyKind, Distribution, EventId, Extension, Role,
    authority::{Decision, KindRule, Registry},
};
use bbsbox_event_core::{
    self as events, Fork, Readiness, Resolver,
    integration::{self, Module, Relevance},
};
use bbsbox_private_forum::{
    Error,
    control::{COMMIT, CONTROL_EXTENSION, Control, GENESIS},
    governance::Governance,
    membership::{MEMBERSHIP_EXTENSION, Roster},
};
use bbsbox_storage::Migration;
use rusqlite::{Connection, Transaction, params};
use std::collections::BTreeMap;

pub fn migrations() -> [Migration; 1] {
    [Migration {
        module: "private-forum",
        version: 1,
        sql: include_str!("schema.sql"),
    }]
}

/// A caller configures the forum/distribution pairs it hosts and its resource budget.
/// This initial bounded provider returns InvalidLimit when history exceeds that budget.
/// It has no transport endpoint, plaintext access or member-client key storage.
pub struct Host<'a> {
    forums: &'a [(ForumId, Distribution)],
    resolver: &'a dyn Resolver,
    control_limit: usize,
    registry: Registry,
}

impl<'a> Host<'a> {
    pub fn new(
        forums: &'a [(ForumId, Distribution)],
        resolver: &'a dyn Resolver,
        control_limit: usize,
    ) -> events::Result<Self> {
        if control_limit == 0 || control_limit > 4096 {
            return Err(events::Error::InvalidLimit);
        }
        let registry = Registry::new(
            [GENESIS, COMMIT]
                .map(|kind| KindRule {
                    kind: kind.into(),
                    schema: 1,
                    actor: Role::ForumRoot,
                    purpose: None,
                })
                .into(),
            vec![CONTROL_EXTENSION, MEMBERSHIP_EXTENSION],
        )
        .expect("valid forum control rules");
        Ok(Self {
            forums,
            resolver,
            control_limit,
            registry,
        })
    }

    fn credentials(
        &self,
        connection: &Connection,
        controls: &[Control],
    ) -> events::Result<BTreeMap<[u8; 32], Credential>> {
        let mut proofs = BTreeMap::new();
        for control in controls {
            for dependency in control.membership().dependencies() {
                let id = CredentialId::from_bytes(dependency.digest);
                if let std::collections::btree_map::Entry::Vacant(entry) = proofs.entry(*id.bytes())
                    && let Some(credential) = self.resolver.resolve(connection, id)?
                {
                    if credential.id() != id {
                        return Err(events::Error::CorruptRecord);
                    }
                    entry.insert(credential);
                }
            }
        }
        Ok(proofs)
    }

    pub fn history(
        &self,
        connection: &Connection,
        forum: ForumId,
        distribution: Distribution,
    ) -> events::Result<Governance> {
        integration::ensure_projection_current(connection, self.id())?;
        if !self.forums.contains(&(forum, distribution)) {
            return Err(events::Error::WrongModule);
        }
        let mut statement = connection.prepare("SELECT c.event_id FROM private_forum_controls c JOIN events_receipts r ON r.event_id=c.event_id AND r.module='private-forum' WHERE c.forum=?1 AND c.distribution=?2 ORDER BY c.event_id LIMIT ?3")?;
        let ids = statement
            .query_map(
                params![
                    forum.to_string(),
                    distribution as u8,
                    (self.control_limit + 1) as i64
                ],
                |row| row.get::<_, Vec<u8>>(0),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if ids.len() > self.control_limit {
            return Err(events::Error::InvalidLimit);
        }
        let mut controls = Vec::new();
        for bytes in ids {
            let id =
                EventId::from_bytes(bytes.try_into().map_err(|_| events::Error::CorruptRecord)?);
            if events::readiness(connection, id)? != Some(Readiness::Ready) {
                continue;
            }
            let event = integration::complete(connection, id, self.resolver)?
                .ok_or(events::Error::CorruptRecord)?;
            let control = Control::inspect(&event).map_err(|_| events::Error::CorruptRecord)?;
            if !control.belongs_to(forum, distribution) {
                return Err(events::Error::CorruptRecord);
            }
            controls.push(control);
        }
        let proofs = self.credentials(connection, &controls)?;
        Governance::resolve(forum, distribution, &controls, |id| {
            proofs.get(id.bytes()).cloned()
        })
        .map_err(|_| events::Error::CorruptRecord)
    }
}

impl Module for Host<'_> {
    fn id(&self) -> &str {
        "private-forum"
    }
    fn registry(&self) -> &Registry {
        &self.registry
    }

    fn action(&self, connection: &Connection, event: &CompleteEvent) -> events::Result<Decision> {
        let Ok(control) = Control::inspect(event) else {
            return Ok(Decision::Deny("invalid_forum_control"));
        };
        let Some(&(forum, distribution)) = self
            .forums
            .iter()
            .find(|(forum, distribution)| control.belongs_to(*forum, *distribution))
        else {
            return Ok(Decision::Deny("forum_not_hosted"));
        };
        let retained: i64 = connection.query_row(
            "SELECT count(*) FROM private_forum_controls WHERE forum=?1 AND distribution=?2",
            params![forum.to_string(), distribution as u8],
            |row| row.get(0),
        )?;
        if retained >= self.control_limit as i64 {
            return Err(events::Error::InvalidLimit);
        }
        let history = self.history(connection, forum, distribution)?;
        let previous = match control.header().parent {
            Some(parent) => match history
                .epochs()
                .iter()
                .find(|row| row.control().id() == parent)
            {
                Some(roster) => Some(roster),
                None => return Ok(Decision::Missing(vec![Dependency::event(parent)])),
            },
            None => None,
        };
        let proofs = self.credentials(connection, std::slice::from_ref(&control))?;
        Ok(
            match Roster::apply(control, previous, |id| proofs.get(id.bytes()).cloned()) {
                Ok(_) => Decision::Allow,
                Err(Error::MissingCredential(id)) => {
                    Decision::Missing(vec![Dependency::credential(id)])
                }
                Err(_) => Decision::Deny("invalid_forum_membership"),
            },
        )
    }

    fn extension(
        &self,
        _: &Connection,
        event: &CompleteEvent,
        _: &Extension,
    ) -> events::Result<Decision> {
        Ok(if Control::inspect(event).is_ok() {
            Decision::Allow
        } else {
            Decision::Deny("invalid_forum_control")
        })
    }

    fn dependency(
        &self,
        _: &Connection,
        _: &CompleteEvent,
        dependency: Dependency,
    ) -> events::Result<Decision> {
        Ok(match dependency.kind {
            DependencyKind::Event | DependencyKind::Credential => Decision::Allow,
            DependencyKind::Blob => Decision::Deny("unexpected_forum_blob_dependency"),
        })
    }

    fn dependency_effective(&self, connection: &Connection, id: EventId) -> events::Result<bool> {
        let Some(event) = integration::complete(connection, id, self.resolver)? else {
            return Ok(false);
        };
        let Ok(control) = Control::inspect(&event) else {
            return Ok(false);
        };
        let Some(&(forum, distribution)) = self
            .forums
            .iter()
            .find(|(forum, distribution)| control.belongs_to(*forum, *distribution))
        else {
            return Ok(false);
        };
        Ok(self
            .history(connection, forum, distribution)?
            .epochs()
            .iter()
            .any(|row| row.control().id() == id))
    }

    fn reduce(&self, tx: &Transaction<'_>, event: &CompleteEvent) -> events::Result<Relevance> {
        tx.execute(
            "INSERT INTO private_forum_controls VALUES(?1,?2,?3)",
            params![
                event.id().bytes().as_slice(),
                event.metadata().scope.resource,
                event.metadata().scope.distribution as u8
            ],
        )?;
        Ok(Relevance::DependencyOnly)
    }

    fn invalidate_fork(&self, _: &Transaction<'_>, _: &Fork) -> events::Result<()> {
        // Every history read rechecks M04 readiness before reconstructing the owner chain.
        Ok(())
    }
}
