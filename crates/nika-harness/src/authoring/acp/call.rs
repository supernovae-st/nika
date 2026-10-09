// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! One ACP authoring call's bounds and the closed facts its receipt keeps: the call's own
//! deadline, the door that opens its completion stream inside it, where the call stood when it
//! ended, and the typed identity of a failure, kept before display flattens it into words.
use std::pin::Pin;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::Duration;

use nika_error::traits::NikaErrorCode;
use nika_kernel::ai::harness::{HarnessError, HarnessEventStream, HarnessRequest};
use serde_json::{Value, json};

use super::OneShot;
use super::activity::Activity;

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

    /// Open one completion stream for `request` inside `deadline`, its driver marking the
    /// call's `progress`.
    fn open(
        &self,
        request: HarnessRequest,
        deadline: Deadline,
        progress: Progress,
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

/// The last closed ACP protocol milestone of a call, in protocol order: the transport opened, the
/// agent's initialize answer accepted here, its session created, the requested selection checked
/// (nothing is claimed selected when none was requested), the prompt written and flushed, an
/// answer chunk received. Each says what this side closed, never what the agent's model did, its
/// other activity or how long anything generated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Milestone {
    StreamOpened,
    InitializeAccepted,
    SessionCreated,
    SelectionChecked,
    PromptWritten,
    AnswerChunk,
}

impl Milestone {
    const ORDER: [Self; 6] = [
        Self::StreamOpened,
        Self::InitializeAccepted,
        Self::SessionCreated,
        Self::SelectionChecked,
        Self::PromptWritten,
        Self::AnswerChunk,
    ];

    /// The receipt's word for the milestone.
    const fn as_str(self) -> &'static str {
        match self {
            Self::StreamOpened => "stream_opened",
            Self::InitializeAccepted => "initialize_accepted",
            Self::SessionCreated => "session_created",
            Self::SelectionChecked => "selection_checked",
            Self::PromptWritten => "prompt_written",
            Self::AnswerChunk => "answer_chunk",
        }
    }

    /// Tell an authoring call's `progress`, when one listens, that this milestone closed.
    pub(crate) fn reached(self, progress: Option<&Progress>) {
        if let Some(progress) = progress {
            progress.mark(self);
        }
    }
}

/// What one call has reached, kept outside it: a deadline or a Stop that drops the call can still
/// tell where it stood, the last protocol milestone it closed, which transport bound it had been
/// given and what it received after its prompt. A handle: the stream's driver marks the same call
/// from its own task, synchronously, and nothing is published before the terminal record
/// snapshots it.
#[derive(Debug, Default, Clone)]
pub(crate) struct Progress(Arc<Marks>);

#[derive(Debug, Default)]
struct Marks {
    phase: AtomicU8,
    /// The rank of the last milestone closed (its place in [`Milestone::ORDER`], plus one).
    milestone: AtomicU8,
    allowance: OnceLock<Duration>,
    /// The completed frames received once the prompt was written (`activity`).
    activity: Mutex<Activity>,
}

impl Progress {
    /// The call crossed into `phase`; a phase is never left for an earlier one.
    pub(crate) fn reach(&self, phase: Phase) {
        self.0.phase.fetch_max(phase.rank(), Ordering::AcqRel);
    }

    /// The stream opened with the transport bound it was given. Its driver may already have closed
    /// a later milestone: the later one stays.
    pub(crate) fn opened(&self, allowance: Duration) {
        let _ = self.0.allowance.set(allowance);
        self.reach(Phase::Session);
        self.mark(Milestone::StreamOpened);
    }

    /// The call closed `milestone`; a later milestone is never replaced by an earlier one. The
    /// written prompt also opens the activity window: only frames completed after it count.
    pub(crate) fn mark(&self, milestone: Milestone) {
        let rank = (Milestone::ORDER.iter()).position(|m| *m == milestone);
        let rank = rank.and_then(|at| u8::try_from(at + 1).ok()).unwrap_or(0);
        self.0.milestone.fetch_max(rank, Ordering::AcqRel);
        if milestone == Milestone::PromptWritten {
            self.activity().open(tokio::time::Instant::now());
        }
    }

    /// What the call received after its prompt. A lock poisoned by a panic elsewhere still holds
    /// plain counts and times, so they are read rather than lost.
    pub(super) fn activity(&self) -> MutexGuard<'_, Activity> {
        self.0
            .activity
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// The phase the call last reached.
    pub(crate) fn phase(&self) -> Phase {
        Phase::from_rank(self.0.phase.load(Ordering::Acquire))
    }

    /// The last protocol milestone the call closed, if any.
    pub(crate) fn milestone(&self) -> Option<Milestone> {
        let rank = usize::from(self.0.milestone.load(Ordering::Acquire));
        rank.checked_sub(1)
            .and_then(|at| Milestone::ORDER.get(at))
            .copied()
    }
}

/// What every terminal record of an ACP authoring call adds: where the call stood and the last
/// protocol milestone it closed (null before any), how long it ran from its start, its bounds
/// (the deadline, and the transport allowance actually passed, null when no stream opened), and
/// what it received after writing its prompt (`activity`, null when it never wrote one). Closed
/// facts only, never adapter text. The activity and the elapsed time are read under one lock, so
/// a frame the driver completes meanwhile is either left out or no later than that time.
pub(crate) fn conclude(mut record: Value, progress: &Progress, deadline: Deadline) -> Value {
    if let Some(fields) = record.as_object_mut() {
        fields.insert("phase".into(), json!(progress.phase().as_str()));
        let milestone = progress.milestone().map(Milestone::as_str);
        fields.insert("last_milestone".into(), json!(milestone));
        let (activity, elapsed) = {
            let received = progress.activity();
            (
                received.record(deadline.started),
                deadline.started.elapsed(),
            )
        };
        fields.insert("elapsed_ms".into(), json!(millis(elapsed)));
        let allowance = progress.0.allowance.get().copied().map(millis);
        fields.insert(
            "bounds".into(),
            json!({"deadline_ms": millis(deadline.timeout), "transport_allowance_ms": allowance}),
        );
        fields.insert("activity".into(), activity);
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
