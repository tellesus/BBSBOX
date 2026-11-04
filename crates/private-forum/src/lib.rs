// SPDX-License-Identifier: AGPL-3.0-or-later
//! The private forum's public routing metadata and client authenticated data.
//! Membership, epoch authority and delivery policy are separate checks.
use bbsbox_credentials::{ForumId, Purpose};
use bbsbox_event_codec::{
    Actor, CompleteEvent, Dependency, Draft, EventId, Extension, Role, Scope,
};

pub const AUDIENCE: &str = "forum-members";
pub const HEADER_EXTENSION: u16 = 0x1301;
const AAD_DOMAIN: &[u8] = b"bbsbox.forum-aad/v1\0";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Unsupported,
    InvalidHeader,
    InvalidScope,
    InvalidDependencies,
    InvalidAuthor,
    EmptyBody,
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "private forum metadata: {self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Post,
    Reply { parent: EventId },
    Edit { post: EventId, prior: EventId },
    Withdraw { post: EventId, prior: EventId },
}

impl Action {
    pub fn kind(self) -> &'static str {
        match self {
            Self::Post => "forum.post.create",
            Self::Reply { .. } => "forum.post.reply",
            Self::Edit { .. } => "forum.post.edit",
            Self::Withdraw { .. } => "forum.post.withdraw",
        }
    }

    fn tag(self) -> u8 {
        match self {
            Self::Post => 1,
            Self::Reply { .. } => 2,
            Self::Edit { .. } => 3,
            Self::Withdraw { .. } => 4,
        }
    }

    fn targets(self) -> [Option<EventId>; 2] {
        match self {
            Self::Post => [None, None],
            Self::Reply { parent } => [Some(parent), None],
            Self::Edit { post, prior } | Self::Withdraw { post, prior } => {
                [Some(post), Some(prior)]
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub epoch: u64,
    pub control: EventId,
    pub action_id: [u8; 16],
    pub action: Action,
}

impl Header {
    pub fn dependencies(self) -> Vec<Dependency> {
        let mut dependencies = vec![Dependency::event(self.control)];
        dependencies.extend(
            self.action
                .targets()
                .into_iter()
                .flatten()
                .map(Dependency::event),
        );
        dependencies.sort_unstable();
        dependencies.dedup();
        dependencies
    }

    fn validate(self) -> Result<(), Error> {
        if self.action_id == [0; 16]
            || self.control.bytes() == &[0; 32]
            || self
                .action
                .targets()
                .into_iter()
                .flatten()
                .any(|id| id == self.control || id.bytes() == &[0; 32])
        {
            return Err(Error::InvalidHeader);
        }
        Ok(())
    }

    pub fn extension(self) -> Result<Extension, Error> {
        self.validate()?;
        let mut data = Vec::with_capacity(120);
        data.extend_from_slice(&self.epoch.to_be_bytes());
        data.extend_from_slice(self.control.bytes());
        data.extend_from_slice(&self.action_id);
        for id in self.action.targets().into_iter().flatten() {
            data.extend_from_slice(id.bytes());
        }
        Ok(Extension {
            id: HEADER_EXTENSION,
            required: true,
            data,
        })
    }

    fn parse<'a>(kind: &str, extensions: &'a [Extension]) -> Result<(Self, &'a [u8]), Error> {
        let mut recognized = extensions
            .iter()
            .filter(|extension| extension.id == HEADER_EXTENSION);
        let extension = recognized
            .next()
            .filter(|extension| extension.required)
            .ok_or(Error::InvalidHeader)?;
        if recognized.next().is_some() {
            return Err(Error::InvalidHeader);
        }
        if extensions
            .iter()
            .any(|candidate| candidate.required && candidate.id != HEADER_EXTENSION)
        {
            return Err(Error::Unsupported);
        }
        let data = &extension.data;
        let reference = |offset: usize| {
            EventId::from_bytes(
                data[offset..offset + 32]
                    .try_into()
                    .expect("fixed reference"),
            )
        };
        let action = match (kind, data.len()) {
            ("forum.post.create", 56) => Action::Post,
            ("forum.post.reply", 88) => Action::Reply {
                parent: reference(56),
            },
            ("forum.post.edit", 120) => Action::Edit {
                post: reference(56),
                prior: reference(88),
            },
            ("forum.post.withdraw", 120) => Action::Withdraw {
                post: reference(56),
                prior: reference(88),
            },
            (
                "forum.post.create"
                | "forum.post.reply"
                | "forum.post.edit"
                | "forum.post.withdraw",
                _,
            ) => return Err(Error::InvalidHeader),
            _ => return Err(Error::Unsupported),
        };
        let header = Self {
            epoch: u64::from_be_bytes(data[..8].try_into().expect("fixed epoch")),
            control: reference(8),
            action_id: data[40..56].try_into().expect("fixed action ID"),
            action,
        };
        header.validate()?;
        Ok((header, data))
    }
}

/// The client supplies these bytes to MLS as authenticated data before encrypting a post.
/// The recipient recomputes them from the signed envelope and compares them after processing MLS.
pub struct Outer {
    pub forum: ForumId,
    pub header: Header,
    pub authenticated_data: Vec<u8>,
}

impl Outer {
    pub fn before_encryption(draft: &Draft, actor: &Actor) -> Result<Self, Error> {
        if draft.schema != 1 {
            return Err(Error::Unsupported);
        }
        Self::from_fields(
            &draft.kind,
            &draft.scope,
            actor,
            draft.sequence,
            draft.previous,
            &draft.dependencies,
            &draft.extensions,
        )
    }

    /// Inspect a verified envelope and its exact opaque body, before checking resource authority.
    pub fn inspect(event: &CompleteEvent) -> Result<Self, Error> {
        let metadata = event.metadata();
        if metadata.minor != 0 || metadata.schema != 1 {
            return Err(Error::Unsupported);
        }
        if event.body().is_empty() {
            return Err(Error::EmptyBody);
        }
        if !event
            .commitment()
            .credential()
            .is_some_and(|credential| credential.body().purposes().contains(Purpose::Authorship))
        {
            return Err(Error::InvalidAuthor);
        }
        Self::from_fields(
            &metadata.kind,
            &metadata.scope,
            &metadata.actor,
            metadata.sequence,
            metadata.previous,
            &metadata.dependencies,
            &metadata.extensions,
        )
    }

    fn from_fields(
        kind: &str,
        scope: &Scope,
        actor: &Actor,
        sequence: u64,
        previous: Option<EventId>,
        dependencies: &[Dependency],
        extensions: &[Extension],
    ) -> Result<Self, Error> {
        let (header, wire) = Header::parse(kind, extensions)?;
        if header.dependencies() != dependencies {
            return Err(Error::InvalidDependencies);
        }
        let forum: ForumId = scope.resource.parse().map_err(|_| Error::InvalidScope)?;
        if scope.audience != AUDIENCE || forum.bytes() == &[0; 32] {
            return Err(Error::InvalidScope);
        }
        let credential = actor
            .credential
            .filter(|_| actor.role == Role::PersonDevice)
            .ok_or(Error::InvalidAuthor)?;
        if sequence == 0
            || (sequence == 1) != previous.is_none()
            || previous.is_some_and(|id| id.bytes() == &[0; 32])
        {
            return Err(Error::InvalidHeader);
        }
        let mut data = Vec::with_capacity(207 + wire.len());
        data.extend_from_slice(AAD_DOMAIN);
        data.push(header.action.tag());
        data.extend_from_slice(forum.bytes());
        data.push(scope.distribution as u8);
        data.extend_from_slice(credential.bytes());
        data.extend_from_slice(&actor.subject);
        data.extend_from_slice(&actor.key);
        data.extend_from_slice(&actor.incarnation);
        data.extend_from_slice(&sequence.to_be_bytes());
        data.push(u8::from(previous.is_some()));
        if let Some(previous) = previous {
            data.extend_from_slice(previous.bytes());
        }
        data.extend_from_slice(wire);
        Ok(Self {
            forum,
            header,
            authenticated_data: data,
        })
    }
}
