// SPDX-License-Identifier: AGPL-3.0-or-later
//! Versioned request and response types for the bundled first-party demo.
use super::{Manifest, sessions::PlayerId};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Move {
    Explore,
    Rest,
}

impl Move {
    pub fn key(self) -> &'static str {
        match self {
            Self::Explore => "explore",
            Self::Rest => "rest",
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Operation {
    Describe,
    Join {
        player: PlayerId,
    },
    Read {
        player: PlayerId,
    },
    Act {
        player: PlayerId,
        command: [u8; 16],
        action: Move,
    },
    Pause {
        fence: [u8; 16],
    },
    Checkpoint {
        fence: [u8; 16],
    },
    Resume {
        fence: [u8; 16],
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub version: u32,
    pub id: [u8; 16],
    pub operation: Operation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub seat: u8,
    pub players: u8,
    pub turn: u8,
    pub chamber: u8,
    pub stamina: u8,
    pub finished: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Reason {
    Unsupported,
    Full,
    NotJoined,
    Waiting,
    OutOfTurn,
    InvalidMove,
    Finished,
    ChangedRetry,
    Unavailable,
    Paused,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ResultBody {
    Hello {
        manifest: Manifest,
    },
    State {
        state: State,
    },
    Refused {
        reason: Reason,
    },
    Paused {
        fence: [u8; 16],
    },
    Checkpoint {
        fence: [u8; 16],
        checkpoint_id: String,
        state_schema: u32,
        worker_version: String,
        directory: PathBuf,
    },
    Resumed {
        fence: [u8; 16],
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reply {
    pub version: u32,
    pub id: [u8; 16],
    pub result: ResultBody,
}
