// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! One ACP authoring call's bounds and the closed facts its receipt keeps: the call's own
//! deadline, the door that opens its completion stream inside it, where the call stood when it
//! ended, and the typed identity of a failure, kept before display flattens it into words.
use std::pin::Pin;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use nika_error::traits::NikaErrorCode;
use nika_kernel::ai::harness::{HarnessError, HarnessEventStream, HarnessRequest};
use serde_json::{Value, json};

use super::OneShot;

/// One call's own deadline: when the call started and how long it may take. Every bound below
/// it counts from that same start, so setup, identity probes and the spawn spend it too.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Deadline {
    started: tokio::time::Instant,
    timeout: Duration,
}

impl Deadline {
    /// The deadline of a call starting now.
    pub(crate) fn start(timeout: Duration) -> Self {
        Self {
            started: tokio::time::Instant::now(),
            timeout,
        }
    }

    /// What the deadline still leaves, read from the clock now.
    pub(crate) fn remaining(self) -> Duration {
        self.timeout.saturating_sub(self.started.elapsed())
    }

    /// Whether the deadline has passed, read from the clock now, never from a timer that may
    /// not have fired yet.
    pub(crate) fn expired(self) -> bool {
        self.remaining().is_zero()
    }

    /// Resolves once the deadline has passed: a waiting select's wake-up, never its verdict.
    pub(crate) async fn passed(self) {
        tokio::time::sleep(self.remaining()).await;
    }
}

/// What one ACP authoring completion opens inside its call's deadline: the registry adapter,
/// spawned under its audited profile, or a scripted peer. Each frame read and each write of the
/// stream is bounded by what the deadline leaves when the transport starts, counted from the
/// call's start; an expired deadline opens nothing. The deadline itself stays the caller's to
/// enforce above the stream. The unwind bounds keep the public authoring seat's auto traits.
pub(crate) trait Door:
    Send + Sync + std::panic::UnwindSafe + std::panic::RefUnwindSafe + std::fmt::Debug
{
    /// The audited one-shot the door serves, if any.
    fn one_shot(&self) -> Option<OneShot>;

    /// Open one completion stream for `request` inside `deadline`.
    fn open(
        &self,
        request: HarnessRequest,
        deadline: Deadline,
    ) -> Pin<Box<dyn Future<Output = Result<Opened, HarnessError>> + Send + '_>>;
}

/// An open completion stream and the transport bound it was actually given.
pub(crate) struct Opened {
    /// The completion's events.
    pub(crate) stream: HarnessEventStream,
    /// The bound passed to the transport: what the deadline left once the adapter was ready to
    /// start, never a fixed default.
    pub(crate) allowance: Duration,
}

/// Where an ACP authoring call stood, each phase entered at a boundary the call observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    /// No completion stream yet: scratch, identity probes, profile, spawn.
    Open,
    /// The stream was open and no answer text had arrived (handshake, selection, prompt and
    /// whatever the agent did before its first answer text).
    Session,
    /// Answer text had arrived; the turn had not completed.
    Answer,
    /// A completed turn had arrived and was judged.
    Completion,
}

impl Phase {
    const fn rank(self) -> u8 {
        match self {
            Self::Open => 0,
            Self::Session => 1,
            Self::Answer => 2,
            Self::Completion => 3,
        }
    }

    const fn from_rank(rank: u8) -> Self {
        match rank {
            0 => Self::Open,
            1 => Self::Session,
            2 => Self::Answer,
            _ => Self::Completion,
        }
    }

    /// The receipt's word for the phase.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Session => "session",
            Self::Answer => "answer",
            Self::Completion => "completion",
        }
    }
}

/// What one call has reached, kept outside it: a deadline or a Stop that drops the call can still
/// tell where it stood and which transport bound it had been given.
#[derive(Debug, Default)]
pub(crate) struct Progress {
    phase: AtomicU8,
    allowance: OnceLock<Duration>,
}

impl Progress {
    /// The call crossed into `phase`; a phase is never left for an earlier one.
    pub(crate) fn reach(&self, phase: Phase) {
        self.phase.fetch_max(phase.rank(), Ordering::AcqRel);
    }

    /// The stream opened with the transport bound it was given.
    pub(crate) fn opened(&self, allowance: Duration) {
        let _ = self.allowance.set(allowance);
        self.reach(Phase::Session);
    }

    /// The phase the call last reached.
    pub(crate) fn phase(&self) -> Phase {
        Phase::from_rank(self.phase.load(Ordering::Acquire))
    }
}

/// What every terminal record of an ACP authoring call adds: where the call stood, how long it
/// ran from its start, and its bounds (the deadline, and the transport allowance actually passed,
/// null when no stream opened). Closed facts only, never adapter text.
pub(crate) fn conclude(mut record: Value, progress: &Progress, deadline: Deadline) -> Value {
    if let Some(fields) = record.as_object_mut() {
        fields.insert("phase".into(), json!(progress.phase().as_str()));
        fields.insert(
            "elapsed_ms".into(),
            json!(millis(deadline.started.elapsed())),
        );
        let allowance = progress.allowance.get().copied().map(millis);
        fields.insert(
            "bounds".into(),
            json!({"deadline_ms": millis(deadline.timeout), "transport_allowance_ms": allowance}),
        );
    }
    record
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

/// One failed call, typed before display flattens it: the words the author always saw, beside
/// the closed identity of the harness error (its class, code and transience, never its text).
/// A cause is named only where a typed constructor names it; every other cause stays unknown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Failure {
    /// The safe human message, unchanged.
    pub(crate) message: String,
    class: &'static str,
    code: String,
    transient: bool,
    cause: &'static str,
}

impl Failure {
    /// `message` for a failure whose typed `error` decides the closed identity.
    pub(crate) fn told(message: impl Into<String>, error: &HarnessError) -> Self {
        let class = match error {
            HarnessError::Unavailable { .. } => "unavailable",
            HarnessError::Session { .. } => "session",
            HarnessError::Refused { .. } => "refused",
            HarnessError::Selection { .. } => "selection",
            _ => "unknown",
        };
        // The one cause a typed constructor names: the sign-in sentinel `signed_out` builds from
        // the adapter's typed kind, compared exactly. No text or elapsed time is read for it.
        let cause = match error {
            HarnessError::Unavailable { reason } if reason == crate::client::SIGN_IN_EXPIRED => {
                "sign_in_expired"
            }
            _ => "unknown",
        };
        Self {
            message: message.into(),
            class,
            code: error.nika_code().to_string(),
            transient: NikaErrorCode::is_transient(error),
            cause,
        }
    }

    /// Nika's own refusal (the profile's second check), in its own words.
    pub(crate) fn refused(message: &str) -> Self {
        Self::told(message, &super::refusal(message))
    }

    /// A session that ended without a completed turn, in Nika's own words.
    pub(crate) fn ended(message: &str) -> Self {
        let error = HarnessError::Session {
            reason: message.to_owned(),
        };
        Self::told(message, &error)
    }

    /// The terminal record: the unchanged message, no answer accepted, the closed identity.
    pub(crate) fn record(&self) -> Value {
        json!({"status": "failed", "reason": self.message, "answer_accepted": false,
            "failure": {"class": self.class, "code": self.code, "transient": self.transient,
                "cause": self.cause}})
    }
}
