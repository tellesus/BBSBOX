// SPDX-License-Identifier: AGPL-3.0-or-later
//! Reconstruct the unambiguous owner-led history from retained verified controls.
use crate::{
    Error, Outer,
    control::Control,
    membership::{Member, Roster},
};
use bbsbox_credentials::{Credential, CredentialId, ForumId};
use bbsbox_event_codec::{Distribution, EventId};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    MissingGenesis,
    Tip,
    Conflict { parent: Option<EventId> },
    MissingCredential(CredentialId),
}

pub struct Governance {
    epochs: Vec<Roster>,
    stop: Stop,
}

impl Governance {
    /// Rebuild after every change in retained, chain-valid evidence. A late competing approval
    /// removes its branch and descendants from this view. The caller retains all signed evidence
    /// and enforces C02 stream validity, causal revocation and host distribution policy separately.
    pub fn resolve<'a>(
        forum: ForumId,
        distribution: Distribution,
        controls: impl IntoIterator<Item = &'a Control>,
        resolve: impl Fn(CredentialId) -> Option<Credential>,
    ) -> Result<Self, Error> {
        let mut children: BTreeMap<Option<EventId>, BTreeMap<EventId, &Control>> = BTreeMap::new();
        for control in controls {
            if control.belongs_to(forum, distribution) {
                children
                    .entry(control.header().parent)
                    .or_default()
                    .insert(control.id(), control);
            }
        }
        let mut history = Self {
            epochs: Vec::new(),
            stop: Stop::MissingGenesis,
        };
        loop {
            let prior = history.epochs.last();
            let parent = prior.map(|roster| roster.control().id());
            let candidates = children
                .get(&parent)
                .into_iter()
                .flat_map(|rows| rows.values())
                .filter(|candidate| {
                    prior.is_none_or(|roster| candidate.is_successor_of(roster.control()))
                })
                .copied()
                .collect::<Vec<_>>();
            match candidates.as_slice() {
                [] => {
                    history.stop = if prior.is_some() {
                        Stop::Tip
                    } else {
                        Stop::MissingGenesis
                    };
                    return Ok(history);
                }
                [candidate] => match Roster::apply((*candidate).clone(), prior, &resolve) {
                    Ok(roster) => history.epochs.push(roster),
                    Err(Error::MissingCredential(id)) => {
                        history.stop = Stop::MissingCredential(id);
                        return Ok(history);
                    }
                    Err(error) => return Err(error),
                },
                _ => {
                    history.stop = Stop::Conflict { parent };
                    return Ok(history);
                }
            }
        }
    }

    pub fn stop(&self) -> Stop {
        self.stop
    }

    pub fn epochs(&self) -> &[Roster] {
        &self.epochs
    }

    /// Old accepted epochs stay addressable for disconnected authors. Dependency/causal policy
    /// must still reject an author whose signed context already observes their removal.
    pub fn post_author(&self, post: &Outer) -> Option<Member> {
        self.epochs
            .iter()
            .find_map(|roster| roster.post_author(post))
    }
}
