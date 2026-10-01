// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What the host observed of the copied world around one rehearsal: the bytes it copied into
//! the room before the run, the final state of every path it read back once a run began, the
//! facts of the room's ledger, the bytes it spent, the bounds it applied, and the typed reason or
//! cause of an end other than a completion. The internal record: it keeps the copied and read
//! back texts for the judge and never the runtime's message, and it is no public surface.

use nika_compile_fidelity::behavior::{Coverage, Identity};

/// The length and the sha256 of some bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Digest {
    /// The byte length.
    pub bytes: u64,
    /// The lowercase hex sha256.
    pub sha256: String,
}

impl Digest {
    /// The digest of `bytes`.
    #[must_use]
    pub fn of(bytes: &[u8]) -> Self {
        Self {
            bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            sha256: Identity::new("", bytes, Coverage::Complete).sha256,
        }
    }
}

/// The text the evidence keeps of some bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Held {
    /// Every byte, as UTF-8 text.
    Whole(String),
    /// A prefix cut on a character boundary at the host's preview bound: the bytes hold more.
    Preview(String),
    /// The bytes are not UTF-8 text, and the evidence keeps none of them.
    NotText,
}

impl Held {
    /// The evidence of `bytes` under a preview bound of `bound` bytes.
    #[must_use]
    pub fn of(bytes: &[u8], bound: u64) -> Self {
        let Ok(text) = std::str::from_utf8(bytes) else {
            return Self::NotText;
        };
        let bound = usize::try_from(bound).unwrap_or(usize::MAX);
        if text.len() <= bound {
            return Self::Whole(text.to_owned());
        }
        let cut = (0..=bound)
            .rev()
            .find(|at| text.is_char_boundary(*at))
            .unwrap_or(0);
        Self::Preview(text.get(..cut).unwrap_or_default().to_owned())
    }

    /// The text kept: empty when the bytes are not text.
    #[must_use]
    pub fn text(&self) -> &str {
        match self {
            Self::Whole(text) | Self::Preview(text) => text,
            Self::NotText => "",
        }
    }
}

/// One input the host copied into the room before the run.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CopyReceipt {
    /// The room-relative path, as the host named it.
    pub path: String,
    /// The bytes the host read from the source: the observed file, or a derived fixture's own.
    pub source: Digest,
    /// The room's copy, read back from the room before the run; `None` when the host did not
    /// verify it.
    pub room: Option<Digest>,
    /// The text the evidence keeps of the room's copy.
    pub held: Held,
}

impl CopyReceipt {
    /// The copy at `path` of source bytes of `source`, read back from the room as `room`, the
    /// evidence keeping `held`.
    #[must_use]
    pub fn new(path: impl Into<String>, source: Digest, room: Option<Digest>, held: Held) -> Self {
        Self {
            path: path.into(),
            source,
            room,
            held,
        }
    }
}

/// What the host found at one path after the run.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FinalState {
    /// No entry at the path.
    Absent,
    /// A directory: no file.
    Directory,
    /// A regular file: its digest and the text the evidence keeps.
    File { digest: Digest, held: Held },
    /// What the room's law refuses to read (a symlink or a special file), or a read that failed.
    Unreadable,
}

/// The final state of one path the host read back once a run began.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct FinalReceipt {
    /// The room-relative path, as the contract or the candidate names it.
    pub path: String,
    pub state: FinalState,
}

impl FinalReceipt {
    /// What the host found at `path`.
    #[must_use]
    pub fn new(path: impl Into<String>, state: FinalState) -> Self {
        Self {
            path: path.into(),
            state,
        }
    }
}

/// The facts of the room's ledger after the last drain.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct LedgerFacts {
    /// The room-relative names the run published, in publication order and with repeats: the
    /// only evidence of a write.
    pub written: Vec<String>,
    /// Operations refused because they arrived after their phase was sealed.
    pub late_refused: u64,
    /// Temporary names a failed cleanup left in the room.
    pub leftovers: u64,
    /// Operations that panicked or never ran.
    pub panicked: u64,
    /// Whether every drain the host awaited completed: no operation was abandoned.
    pub drained: bool,
}

impl LedgerFacts {
    /// A ledger whose every drain completed with nothing late, left behind or panicked, the run
    /// having published `written`.
    #[must_use]
    pub fn clean(written: Vec<String>) -> Self {
        Self {
            written,
            drained: true,
            ..Self::default()
        }
    }
}

/// The bytes one rehearsal spent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Spent {
    /// The bytes of the room's copies, summed over the copy receipts.
    pub copied_bytes: u64,
    /// The bytes of the final files, summed over the final receipts.
    pub read_back_bytes: u64,
}

impl Spent {
    /// `copied_bytes` copied in and `read_back_bytes` read back.
    #[must_use]
    pub const fn new(copied_bytes: u64, read_back_bytes: u64) -> Self {
        Self {
            copied_bytes,
            read_back_bytes,
        }
    }
}

/// The bounds the host applied to one rehearsal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Bounds {
    /// The time bound of the run, in milliseconds.
    pub time_ms: u64,
    /// The room's byte bound: no copied world passes it.
    pub room_bytes: u64,
    /// The bound of a text the evidence keeps.
    pub preview_bytes: u64,
}

impl Bounds {
    /// A run bounded at `time_ms`, in a room of `room_bytes`, its evidence cut at
    /// `preview_bytes`.
    #[must_use]
    pub const fn new(time_ms: u64, room_bytes: u64, preview_bytes: u64) -> Self {
        Self {
            time_ms,
            room_bytes,
            preview_bytes,
        }
    }

    /// Whether every bound is stated.
    #[must_use]
    pub const fn stated(&self) -> bool {
        self.time_ms > 0 && self.room_bytes > 0 && self.preview_bytes > 0
    }
}

/// Why the host refused to rehearse, before any attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Refusal {
    /// The host has no room yet.
    NotBuilt,
    /// The access plan refused the candidate: a pin, a model lane or a model verb.
    Plan,
    /// The candidate holds a jq or convert task, whose rehearsal bounds are not established.
    DataBounds,
    /// The candidate needs an effect a rehearsal denies: a process, the network, a gate, a
    /// secret or a nested run.
    Effect,
    /// An observed input could not be pinned and copied whole within the room's bound: a cut
    /// world is never rehearsed.
    CopyIn,
    /// The admission door refused the candidate, or admitted other bytes than its own.
    Admission,
}

impl Refusal {
    /// The refusal, in words.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::NotBuilt => "the rehearsal room is not built",
            Self::Plan => "the access plan refused the candidate",
            Self::DataBounds => {
                "the candidate holds a jq or convert task, whose rehearsal bounds are not \
                 established"
            }
            Self::Effect => "the candidate needs an effect a rehearsal denies",
            Self::CopyIn => "an observed input could not be copied whole within the room's bound",
            Self::Admission => "the admission door did not admit the candidate's own bytes",
        }
    }
}

/// The cause the runtime recorded for a failed run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RecordedCause {
    /// The failing task's recorded outcome: its verb refused or failed (`verb_error`).
    VerbError,
    /// The failing task's own `timeout:` elapsed (`timeout`).
    Timeout,
    /// Every attempt of the failing task failed (`retry_exhausted`).
    RetryExhausted,
    /// The runtime's run itself failed and left no task outcome.
    Engine,
}

/// The failing task's recorded outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct FailureRecord {
    /// The task that failed.
    pub task: String,
    /// The error code it recorded: evidence only.
    pub code: String,
    pub cause: RecordedCause,
}

impl FailureRecord {
    /// The failure of `task` with `code`, for `cause`.
    #[must_use]
    pub fn new(task: impl Into<String>, code: impl Into<String>, cause: RecordedCause) -> Self {
        Self {
            task: task.into(),
            code: code.into(),
            cause,
        }
    }
}

/// The host's record of one rehearsal, beside its outcome.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Observation {
    /// Every input the host copied into the room, in copy order and with repeats.
    pub copies: Vec<CopyReceipt>,
    /// Every path the host read back once a run began, in reading order and with repeats.
    pub finals: Vec<FinalReceipt>,
    pub ledger: LedgerFacts,
    pub spent: Spent,
    pub bounds: Bounds,
    /// Why the host refused before any attempt, when it did.
    pub refusal: Option<Refusal>,
    /// The failing task's recorded outcome, when the run failed.
    pub failure: Option<FailureRecord>,
}

impl Observation {
    /// Nothing observed: the record of a host that prepared no room.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// The record of a refusal before any attempt.
    #[must_use]
    pub fn refused(refusal: Refusal) -> Self {
        Self {
            refusal: Some(refusal),
            ..Self::default()
        }
    }
}
