// SPDX-License-Identifier: AGPL-3.0-or-later
//! Forum-root signatures bind the exact MLS epoch transition.
use super::{Error, Outer, forum_scope, required_extension};
use crate::membership::{MEMBERSHIP_EXTENSION, MembershipChanges};
use bbsbox_credentials::ForumId;
use bbsbox_event_codec::{CompleteEvent, Dependency, Distribution, EventId, Extension, Role};

pub const GENESIS: &str = "forum.control.genesis";
pub const COMMIT: &str = "forum.control.commit";
pub const CONTROL_EXTENSION: u16 = 0x1302;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlHeader {
    pub epoch: u64,
    pub parent: Option<EventId>,
}

impl ControlHeader {
    pub fn kind(self) -> &'static str {
        if self.parent.is_some() {
            COMMIT
        } else {
            GENESIS
        }
    }

    pub fn extension(self) -> Result<Extension, Error> {
        self.validate()?;
        let mut data = self.epoch.to_be_bytes().to_vec();
        if let Some(parent) = self.parent {
            data.extend_from_slice(parent.bytes());
        }
        Ok(Extension {
            id: CONTROL_EXTENSION,
            required: true,
            data,
        })
    }

    fn validate(self) -> Result<(), Error> {
        match self.parent {
            None if self.epoch == 0 => Ok(()),
            Some(parent) if self.epoch > 0 && parent.bytes() != &[0; 32] => Ok(()),
            _ => Err(Error::InvalidHeader),
        }
    }

    fn parse(kind: &str, extensions: &[Extension]) -> Result<Self, Error> {
        let data = required_extension(CONTROL_EXTENSION, extensions, &[MEMBERSHIP_EXTENSION])?;
        let parent = match (kind, data.len()) {
            (GENESIS, 8) => None,
            (COMMIT, 40) => Some(EventId::from_bytes(
                data[8..].try_into().expect("fixed parent"),
            )),
            (GENESIS | COMMIT, _) => return Err(Error::InvalidHeader),
            _ => return Err(Error::Unsupported),
        };
        let header = Self {
            epoch: u64::from_be_bytes(data[..8].try_into().expect("fixed epoch")),
            parent,
        };
        header.validate()?;
        Ok(header)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Control {
    id: EventId,
    forum: ForumId,
    distribution: Distribution,
    header: ControlHeader,
    membership: MembershipChanges,
}

impl Control {
    /// This validates a root-authorized declaration before applying the MLS and epoch changes.
    pub fn inspect(event: &CompleteEvent) -> Result<Self, Error> {
        let metadata = event.metadata();
        if metadata.minor != 0 || metadata.schema != 1 {
            return Err(Error::Unsupported);
        }
        if event.body().is_empty() {
            return Err(Error::EmptyBody);
        }
        let forum = forum_scope(&metadata.scope)?;
        if metadata.actor.role != Role::ForumRoot || &metadata.actor.subject != forum.bytes() {
            return Err(Error::InvalidAuthor);
        }
        let header = ControlHeader::parse(&metadata.kind, &metadata.extensions)?;
        let membership = MembershipChanges::parse(&metadata.extensions)?;
        let mut expected = header
            .parent
            .map(Dependency::event)
            .into_iter()
            .collect::<Vec<_>>();
        expected.extend(membership.dependencies());
        expected.sort_unstable();
        if metadata.dependencies != expected {
            return Err(Error::InvalidDependencies);
        }
        if metadata.previous.is_some() && metadata.previous != header.parent {
            return Err(Error::InvalidHeader);
        }
        Ok(Self {
            id: event.id(),
            forum,
            distribution: metadata.scope.distribution,
            header,
            membership,
        })
    }

    pub fn id(&self) -> EventId {
        self.id
    }

    pub fn header(&self) -> ControlHeader {
        self.header
    }

    pub fn is_successor_of(&self, parent: &Self) -> bool {
        self.forum == parent.forum
            && self.distribution == parent.distribution
            && self.header.parent == Some(parent.id)
            && parent.header.epoch.checked_add(1) == Some(self.header.epoch)
    }

    /// The caller supplies an epoch on the accepted control chain and checks device membership.
    pub fn anchors(&self, post: &Outer) -> bool {
        self.forum == post.forum
            && self.distribution == post.distribution
            && self.id == post.header.control
            && self.header.epoch == post.header.epoch
    }

    pub fn membership(&self) -> &MembershipChanges {
        &self.membership
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Successor<'a> {
    None,
    Unique(&'a Control),
    Conflict,
}

/// Resolve all known verified direct children. The caller retains conflicting evidence.
pub fn successor<'a>(
    parent: &Control,
    candidates: impl IntoIterator<Item = &'a Control>,
) -> Successor<'a> {
    let mut selected: Option<&Control> = None;
    for candidate in candidates
        .into_iter()
        .filter(|candidate| candidate.is_successor_of(parent))
    {
        if selected.is_some_and(|prior| prior.id != candidate.id) {
            return Successor::Conflict;
        }
        selected = Some(candidate);
    }
    selected.map_or(Successor::None, Successor::Unique)
}
