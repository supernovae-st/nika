// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The rehearsal port: a host that can run a candidate in a safe room built from the observed
//! world says what the run did before the native door declares the candidate READY. The
//! compile core does no I/O, so the run is a host act behind this port, the way a decision
//! seat is ([`crate::decide::DecisionSeat`]). A rehearsal is never a provider call, never
//! records consent and never writes outside its room; a host that cannot rehearse a candidate
//! safely answers [`Rehearsal::NotRun`] with its reason. Every report names the exact bytes it
//! rehearsed and the world it admitted, whether a run began and how it ended, what the host did
//! around the run, and the effects its denied seams saw attempted. The host's [`Observation`] of
//! the copied world rides each report, and [`judged_run`] maps it to the behavioural judge's run.

use std::{future::Future, pin::Pin, time::Duration};

mod judged;
#[cfg(test)]
mod judged_tests;
mod observed;

pub use judged::{judged_run, targets_of};
pub use observed::{
    Bounds, CopyReceipt, Digest, FailureRecord, FinalReceipt, FinalState, Held, LedgerFacts,
    Observation, RecordedCause, Refusal, Spent,
};

/// One declared output a rehearsal read back from its room.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct RehearsedOutput {
    /// The declared path, relative to the project root, as the candidate writes it.
    pub path: String,
    /// A bounded preview of what the room holds at that path, cut at a character boundary.
    pub text: String,
    /// Whether the run itself published this path. A copied file, a failed publish or a file
    /// that already existed is no write.
    pub written: bool,
    /// Whether `text` is only a prefix of the content: a cut preview never proves a
    /// whole-output contract.
    pub truncated: bool,
    /// The byte length of the whole content.
    pub full_bytes: u64,
    /// The sha256 of the whole content (lowercase hex), streamed; empty when the host did not
    /// compute it.
    pub full_sha256: String,
}

impl RehearsedOutput {
    /// One output the run wrote, read back whole.
    #[must_use]
    pub fn new(path: impl Into<String>, text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            path: path.into(),
            full_bytes: text.len() as u64,
            text,
            written: true,
            truncated: false,
            full_sha256: String::new(),
        }
    }

    /// The same output, marked as published by the run or not.
    #[must_use]
    pub fn with_written(mut self, written: bool) -> Self {
        self.written = written;
        self
    }

    /// The same output, its preview cut from content of `full_bytes` bytes hashing to
    /// `full_sha256`.
    #[must_use]
    pub fn with_full(mut self, full_bytes: u64, full_sha256: impl Into<String>) -> Self {
        self.truncated = full_bytes > self.text.len() as u64;
        self.full_bytes = full_bytes;
        self.full_sha256 = full_sha256.into();
        self
    }
}

/// What one rehearsal of a candidate proved.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Rehearsal {
    /// The run completed and every declared output was read back.
    Passed {
        /// The declared outputs, in the order the candidate writes them.
        outputs: Vec<RehearsedOutput>,
    },
    /// The run failed: the failing task, its code and its message.
    Failed {
        /// The error code the runtime reported (`NIKA-…`).
        code: String,
        /// The task id that failed.
        task: String,
        /// The runtime's message, cut at the host's byte bound.
        message: String,
    },
    /// The run completed, but a declared output the contract requires was never written.
    Missing {
        /// The declared outputs the run never wrote.
        outputs: Vec<String>,
    },
    /// The host did not complete a run it could vouch for: a provider, the network, exec, a
    /// gate, a path outside the room, an input nobody observed, an effect its seams denied, an
    /// admission refusal, or the time bound. The reason is stated; [`Attempt`] says whether a
    /// run began, and [`RoomEvidence`] what the host did around it.
    NotRun {
        /// Why, in words.
        reason: String,
    },
}

/// Whether a run began, and how it ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Attempt {
    /// No run began. A room may still have been prepared and cleaned: see [`RoomEvidence`].
    NeverAttempted,
    /// The run ended by itself.
    Completed {
        /// Milliseconds from the start of the run to the end of its drain.
        elapsed_ms: u64,
    },
    /// The run was stopped at the bound or cancelled; every accepted operation was joined.
    Stopped {
        /// Milliseconds from the start of the run to the end of the drain, the join included.
        elapsed_ms: u64,
    },
}

/// What the host did around the run, stated whatever the attempt was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct RoomEvidence {
    /// A room was created and the observed inputs were copied into it.
    pub prepared: bool,
    /// Cleanup verified that no room is left behind.
    pub cleaned: bool,
    /// Operations refused because they arrived after their phase was sealed. Nothing they
    /// asked for ran; any count above zero means a producer outlived its phase, so the
    /// outcome is never a pass.
    pub late_refused: u32,
}

impl RoomEvidence {
    /// The evidence of a host that stated both facts, with no late operation.
    #[must_use]
    pub fn new(prepared: bool, cleaned: bool) -> Self {
        Self {
            prepared,
            cleaned,
            late_refused: 0,
        }
    }

    /// The same evidence with the late refusals the host counted.
    #[must_use]
    pub fn with_late_refused(mut self, late_refused: u32) -> Self {
        self.late_refused = late_refused;
        self
    }

    /// No room was ever prepared, so none is left behind.
    #[must_use]
    pub fn untouched() -> Self {
        Self::new(false, true)
    }
}

/// The effects the host's denied seams saw attempted. A safe rehearsal reports none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct EffectCounts {
    /// Network requests, the fetch and provider transports included.
    pub network: u32,
    /// Provider calls.
    pub provider: u32,
    /// Process spawns.
    pub spawn: u32,
    /// Prompts to a human (a gate); none is ever answered.
    pub prompt: u32,
    /// Secret-store reads.
    pub secret: u32,
    /// Nested workflow runs.
    pub child: u32,
}

impl EffectCounts {
    /// No effect attempted.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// Whether no effect was attempted.
    #[must_use]
    pub fn is_none(&self) -> bool {
        *self == Self::default()
    }
}

/// What one rehearsal proved, bound to the exact bytes it rehearsed.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct RehearsalReport {
    /// The outcome.
    pub outcome: Rehearsal,
    /// Whether a run began, and how it ended.
    pub attempt: Attempt,
    /// What the host did around the run.
    pub room: RoomEvidence,
    /// The effects the host's denied seams saw attempted.
    pub effects: EffectCounts,
    /// The sha256 of the candidate bytes the host rehearsed (lowercase hex).
    pub candidate_sha256: String,
    /// The digest the admission door gave the world it admitted (the candidate alone, admitted
    /// from its bytes); empty when nothing was admitted.
    pub admitted_digest: String,
    /// What the host observed of the copied world: the internal record [`judged_run`] reads.
    pub observation: Observation,
}

impl RehearsalReport {
    /// One report of a host that prepared no room and admitted nothing.
    #[must_use]
    pub fn new(
        outcome: Rehearsal,
        attempt: Attempt,
        effects: EffectCounts,
        candidate_sha256: impl Into<String>,
    ) -> Self {
        Self {
            outcome,
            attempt,
            room: RoomEvidence::untouched(),
            effects,
            candidate_sha256: candidate_sha256.into(),
            admitted_digest: String::new(),
            observation: Observation::none(),
        }
    }

    /// The same report with the host's room evidence.
    #[must_use]
    pub fn with_room(mut self, room: RoomEvidence) -> Self {
        self.room = room;
        self
    }

    /// The same report with the digest of the world the admission door admitted.
    #[must_use]
    pub fn with_admitted_digest(mut self, admitted_digest: impl Into<String>) -> Self {
        self.admitted_digest = admitted_digest.into();
        self
    }

    /// The same report with what the host observed of the copied world.
    #[must_use]
    pub fn with_observation(mut self, observation: Observation) -> Self {
        self.observation = observation;
        self
    }
}

/// The object-safe future a rehearsal answers with.
pub type RehearsalFuture<'a> = Pin<Box<dyn Future<Output = RehearsalReport> + Send + 'a>>;

/// A host that can rehearse a candidate in a safe room built from the observed world.
pub trait Rehearse: Send + Sync {
    /// Rehearse `candidate` (the exact bytes every law passed) over `inputs` (the observed
    /// paths, relative to the project root), within [`Rehearse::bound`].
    fn rehearse<'a>(&'a self, candidate: &'a str, inputs: &'a [String]) -> RehearsalFuture<'a>;
    /// The time bound the host applies to one rehearsal, recorded with it.
    fn bound(&self) -> Duration;
    /// Rehearse as [`Rehearse::rehearse`] does, and read back every path of `targets` (the
    /// results the request's contract names, declared by the candidate or not) beside the
    /// declared outputs, whatever the run's end. A host that keeps this default ignores
    /// `targets`: its report then holds no final state of an undeclared target, which the
    /// judge reads as no observation.
    fn rehearse_reading<'a>(
        &'a self,
        candidate: &'a str,
        inputs: &'a [String],
        _targets: &'a [String],
    ) -> RehearsalFuture<'a> {
        self.rehearse(candidate, inputs)
    }
}
