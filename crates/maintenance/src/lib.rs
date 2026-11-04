// SPDX-License-Identifier: AGPL-3.0-or-later
//! M12 maintenance primitives. Complete-node coordination and restored activation are separate layers.
use std::{fmt, io};

pub mod archive;
pub mod checkpoints;
pub mod continuation;
pub mod game_snapshot;
pub mod inventory;
pub mod release;
pub mod release_payload;
pub mod release_trust;
pub mod snapshot;

/// Foundation owners have all durable state inside the coordinated core snapshot.
pub fn foundation_owner(module: &str) -> bool {
    matches!(
        module,
        "identity"
            | "events"
            | "settings"
            | "discussions"
            | "mail"
            | "profiles"
            | "social"
            | "gatherings"
            | "groups"
    )
}
pub use age::secrecy as age_secret;
pub use age::x25519::{Identity as BackupIdentity, Recipient as BackupRecipient};
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Storage(bbsbox_storage::Error),
    Identity(bbsbox_identity::Error),
    Io(io::Error),
    InvalidArchive,
    InvalidSignature,
    WrongNode,
    UnsafePath,
    LimitExceeded,
    ChangedSource,
    Encryption,
    Decryption,
    IncompleteSnapshot,
    InvalidContinuation,
    CheckpointUnavailable,
    CheckpointResumeFailed {
        request: [u8; 16],
        games: Vec<String>,
    },
    UnsupportedCheckpoint {
        game_id: String,
    },
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => write!(f, "maintenance storage: {error}"),
            Self::Identity(error) => write!(f, "maintenance identity: {error}"),
            Self::Io(error) => write!(f, "maintenance I/O: {error}"),
            Self::InvalidArchive => f.write_str("invalid or incomplete backup archive"),
            Self::InvalidSignature => f.write_str("invalid maintenance signature"),
            Self::WrongNode => f.write_str("maintenance record belongs to another node"),
            Self::UnsafePath => f.write_str("maintenance path is unsafe or unsupported"),
            Self::LimitExceeded => f.write_str("maintenance data exceeds configured limits"),
            Self::ChangedSource => f.write_str("staged backup files changed during export"),
            Self::Encryption => f.write_str("backup encryption failed"),
            Self::Decryption => f.write_str("backup decryption failed"),
            Self::IncompleteSnapshot => {
                f.write_str("snapshot contents are incomplete or unsupported")
            }
            Self::InvalidContinuation => {
                f.write_str("recovery decision or authorization is invalid or unsupported")
            }
            Self::CheckpointUnavailable => {
                f.write_str("an installed worker could not provide its checkpoint")
            }
            Self::CheckpointResumeFailed { .. } => {
                f.write_str("a checkpoint worker needs attention before resuming")
            }
            Self::UnsupportedCheckpoint { game_id } => write!(
                f,
                "installed game version/schema does not match checkpoint: {game_id}"
            ),
        }
    }
}
impl std::error::Error for Error {}
impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}
impl From<bbsbox_storage::Error> for Error {
    fn from(value: bbsbox_storage::Error) -> Self {
        Self::Storage(value)
    }
}
impl From<bbsbox_identity::Error> for Error {
    fn from(value: bbsbox_identity::Error) -> Self {
        Self::Identity(value)
    }
}

#[derive(Clone, Copy)]
pub struct Limits {
    pub max_files: usize,
    pub max_manifest: usize,
    pub max_member_bytes: u64,
    pub max_total_bytes: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_files: 50_128,
            max_manifest: 8 * 1024 * 1024,
            max_member_bytes: 1 << 40,
            max_total_bytes: 8 << 40,
        }
    }
}
impl Limits {
    pub(crate) fn validate(self) -> Result<()> {
        if self.max_files == 0
            || self.max_files > 50_128
            || self.max_manifest == 0
            || self.max_manifest > 8 * 1024 * 1024
            || self.max_member_bytes == 0
            || self.max_total_bytes < self.max_member_bytes
            || self.max_total_bytes > 8 << 40
        {
            return Err(Error::LimitExceeded);
        }
        Ok(())
    }
}
