// SPDX-License-Identifier: AGPL-3.0-or-later
//! First-party game manifests and bounded local launch authority.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub mod ipc;
pub mod protocol;
pub mod sessions;
pub mod worker;

const MAX_MANIFEST: usize = 8 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    TurnBasedPair,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Component {
    CharacterSummary,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    game_id: String,
    worker_version: String,
    state_schema: u32,
    passport_schemas: Vec<u32>,
    launch_entry: String,
    mode: Mode,
    profile_components: Vec<Component>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ManifestError {
    Invalid,
}

impl Manifest {
    pub fn parse(bytes: &[u8]) -> Result<Self, ManifestError> {
        if bytes.len() > MAX_MANIFEST {
            return Err(ManifestError::Invalid);
        }
        let value: Self = serde_json::from_slice(bytes).map_err(|_| ManifestError::Invalid)?;
        if value.valid() {
            Ok(value)
        } else {
            Err(ManifestError::Invalid)
        }
    }

    pub fn valid(&self) -> bool {
        safe_id(&self.game_id)
            && !self.worker_version.is_empty()
            && self.worker_version.len() <= 40
            && self
                .worker_version
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-'))
            && self.state_schema != 0
            && self.passport_schemas.len() <= 8
            && self.passport_schemas.iter().all(|n| *n > 0)
            && self
                .passport_schemas
                .windows(2)
                .all(|pair| pair[0] < pair[1])
            && self.launch_entry.len() <= 128
            && self.launch_entry.ends_with(".html")
            && self.launch_entry.split('/').all(|part| {
                !part.is_empty()
                    && !matches!(part, "." | "..")
                    && part
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
            })
            && self
                .profile_components
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                == self.profile_components.len()
    }

    pub fn game_id(&self) -> &str {
        &self.game_id
    }
    pub fn worker_version(&self) -> &str {
        &self.worker_version
    }
    pub fn state_schema(&self) -> u32 {
        self.state_schema
    }
    pub fn launch_entry(&self) -> &str {
        &self.launch_entry
    }
    pub fn profile_components(&self) -> &[Component] {
        &self.profile_components
    }
}

fn safe_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
