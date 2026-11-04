// SPDX-License-Identifier: AGPL-3.0-or-later
//! Owner-signed public credential/MLS-leaf bindings for a specific forum epoch.
use super::{Error, Outer, control::Control, required_extension};
use bbsbox_credentials::{Credential, CredentialId, DeviceId, Purpose};
use bbsbox_event_codec::{Dependency, Extension};
use std::collections::{BTreeMap, BTreeSet};

pub const MEMBERSHIP_EXTENSION: u16 = 0x1303;
pub const MAX_CHANGES: usize = 12;
const ROW_BYTES: usize = 65;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum MemberRole {
    Member = 1,
    Moderator = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Member {
    pub role: MemberRole,
    pub mls_key: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Grant {
        credential: CredentialId,
        member: Member,
    },
    Remove {
        credential: CredentialId,
    },
}

impl Change {
    fn credential(self) -> CredentialId {
        match self {
            Self::Grant { credential, .. } | Self::Remove { credential } => credential,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MembershipChanges(Vec<Change>);

impl MembershipChanges {
    pub fn new(mut changes: Vec<Change>) -> Result<Self, Error> {
        changes.sort_unstable_by_key(|change| *change.credential().bytes());
        Self::check(changes)
    }

    fn check(changes: Vec<Change>) -> Result<Self, Error> {
        if changes.len() > MAX_CHANGES
            || changes
                .windows(2)
                .any(|pair| pair[0].credential().bytes() >= pair[1].credential().bytes())
            || changes
                .iter()
                .any(|change| change.credential().bytes() == &[0; 32])
        {
            return Err(Error::InvalidMembership);
        }
        for change in &changes {
            if let Change::Grant { member, .. } = change {
                DeviceId::from_public_key(&member.mls_key).map_err(|_| Error::InvalidMembership)?;
            }
        }
        Ok(Self(changes))
    }

    pub fn dependencies(&self) -> impl Iterator<Item = Dependency> + '_ {
        self.0.iter().filter_map(|change| match change {
            Change::Grant { credential, .. } => Some(Dependency::credential(*credential)),
            Change::Remove { .. } => None,
        })
    }

    pub fn extension(&self) -> Option<Extension> {
        if self.0.is_empty() {
            return None;
        }
        let mut data = Vec::with_capacity(1 + self.0.len() * ROW_BYTES);
        data.push(self.0.len() as u8);
        for change in &self.0 {
            data.extend_from_slice(change.credential().bytes());
            match change {
                Change::Grant { member, .. } => {
                    data.extend_from_slice(&member.mls_key);
                    data.push(member.role as u8);
                }
                Change::Remove { .. } => data.extend_from_slice(&[0; 33]),
            }
        }
        Some(Extension {
            id: MEMBERSHIP_EXTENSION,
            required: true,
            data,
        })
    }

    pub(crate) fn parse(extensions: &[Extension]) -> Result<Self, Error> {
        if !extensions
            .iter()
            .any(|extension| extension.id == MEMBERSHIP_EXTENSION)
        {
            return Ok(Self::default());
        }
        let data = required_extension(
            MEMBERSHIP_EXTENSION,
            extensions,
            &[super::control::CONTROL_EXTENSION],
        )?;
        let (&count, rows) = data.split_first().ok_or(Error::InvalidMembership)?;
        let count = usize::from(count);
        if !(1..=MAX_CHANGES).contains(&count) || rows.len() != count * ROW_BYTES {
            return Err(Error::InvalidMembership);
        }
        let mut changes = Vec::with_capacity(count);
        for row in rows.as_chunks::<ROW_BYTES>().0 {
            let credential =
                CredentialId::from_bytes(row[..32].try_into().expect("fixed credential"));
            let mls_key = row[32..64].try_into().expect("fixed MLS key");
            let role = match row[64] {
                0 if mls_key == [0; 32] => {
                    changes.push(Change::Remove { credential });
                    continue;
                }
                1 => MemberRole::Member,
                2 => MemberRole::Moderator,
                _ => return Err(Error::InvalidMembership),
            };
            changes.push(Change::Grant {
                credential,
                member: Member { role, mls_key },
            });
        }
        Self::check(changes)
    }
}

pub struct Roster {
    control: Control,
    members: BTreeMap<[u8; 32], Member>,
}

impl Roster {
    /// Apply an accepted control branch. The caller resolves competing approvals before calling.
    pub fn apply(
        control: Control,
        previous: Option<&Roster>,
        resolve: impl Fn(CredentialId) -> Option<Credential>,
    ) -> Result<Self, Error> {
        let mut members = match previous {
            Some(previous) if control.is_successor_of(&previous.control) => {
                previous.members.clone()
            }
            None if control.header().parent.is_none() => BTreeMap::new(),
            _ => return Err(Error::InvalidMembership),
        };
        for change in &control.membership().0 {
            match *change {
                Change::Grant { credential, member } => {
                    let proof = resolve(credential).ok_or(Error::MissingCredential(credential))?;
                    let purposes = proof.body().purposes();
                    if proof.id() != credential
                        || !purposes.contains(Purpose::Authorship)
                        || (member.role == MemberRole::Moderator
                            && !purposes.contains(Purpose::ForumGovernance))
                        || members.insert(*credential.bytes(), member) == Some(member)
                    {
                        return Err(Error::InvalidMembership);
                    }
                }
                Change::Remove { credential } => {
                    if members.remove(credential.bytes()).is_none() {
                        return Err(Error::InvalidMembership);
                    }
                }
            }
        }
        let mut keys = BTreeSet::new();
        if !members.values().all(|member| keys.insert(member.mls_key)) {
            return Err(Error::InvalidMembership);
        }
        Ok(Self { control, members })
    }

    pub fn member(&self, credential: CredentialId) -> Option<Member> {
        self.members.get(credential.bytes()).copied()
    }

    pub fn post_author(&self, post: &Outer) -> Option<Member> {
        self.control
            .anchors(post)
            .then(|| self.member(post.credential))
            .flatten()
    }
}
