// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The person's lines sent while a conversation's run was under way, typed for every host
//! (`nika/session-work@0`, additive): each with its identity, how it waited and what became of
//! it — entered as the person's cited line, or returned unsent. The loop that reads them and the
//! hosts that queue them share these words.

use serde::{Deserialize, Serialize};

/// How a line waits for a run under way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum QueueMode {
    /// It enters after the current calls.
    Steer,
    /// It enters when the run would end.
    FollowUp,
}

impl QueueMode {
    /// The mode's word on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Steer => "steer",
            Self::FollowUp => "follow_up",
        }
    }
}

/// What became of a queued line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
#[non_exhaustive]
pub enum QueuedState {
    /// It waits for the run to read it.
    Waiting,
    /// It entered the conversation as the person's line `cite`.
    Entered {
        /// The citation the line got.
        cite: String,
    },
    /// It came back unsent: the run stopped, waited for the person or ended before reading it.
    Returned,
}

/// One line the person sent while a run was under way.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Queued {
    /// Its identity within the conversation (`l1`, `l2`, …).
    pub id: String,
    /// How it waits.
    pub mode: QueueMode,
    /// The person's words, as typed.
    pub line: String,
    /// What became of it.
    #[serde(flatten)]
    pub state: QueuedState,
}

impl Queued {
    /// A line waiting under its identity (INV-019).
    #[must_use]
    pub fn new(id: impl Into<String>, mode: QueueMode, line: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            mode,
            line: line.into(),
            state: QueuedState::Waiting,
        }
    }
}
