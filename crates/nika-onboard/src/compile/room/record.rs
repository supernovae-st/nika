// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One rehearsal's report, written from what the host observed. The outcome comes from the
//! runtime's own records: a failed run names its first failing task in the candidate's order
//! and that task's code, and a completed run passes unless an output the candidate writes
//! unconditionally was never published. A write is what the room's ledger recorded in the run
//! phase, never a file that was copied in or already there. The receipts ride the report as the
//! internal observation the behavioural judge reads.

use nika_compile_cognition::rehearse::{
    Attempt, Bounds, CopyReceipt, EffectCounts, FailureRecord, FinalReceipt, FinalState,
    LedgerFacts, Observation, RecordedCause, Rehearsal, RehearsalReport, RehearsedOutput,
    RoomEvidence, Spent,
};
use nika_runtime::{RunOutcome, RuntimeError, TaskStatus, TerminalCause};
use nika_service_execution::DeniedEffects;

use super::screen::{Output, Refused, Screened};

/// The bytes of a runtime message a report keeps.
const MESSAGE_BOUND: usize = 1024;

/// What one started run left: its end (none when the bound stopped it first), the time from its
/// start to the end of its drain, and what its denied capabilities saw.
pub(super) struct Ran {
    pub(super) settled: Option<Result<RunOutcome, RuntimeError>>,
    pub(super) elapsed_ms: u64,
    pub(super) denied: DeniedEffects,
}

/// Everything the host observed around one rehearsal that prepared a room.
pub(super) struct Record {
    pub(super) candidate_sha256: String,
    pub(super) admitted_digest: String,
    pub(super) copies: Vec<CopyReceipt>,
    /// Every path read back, as the room spells it, beside its receipt.
    pub(super) finals: Vec<(String, FinalReceipt)>,
    pub(super) ledger: LedgerFacts,
    pub(super) cleaned: bool,
    pub(super) bounds: Bounds,
}

/// A refusal before any room: nothing prepared, nothing to clean, no effect.
pub(super) fn refused(candidate_sha256: String, refused: Refused) -> RehearsalReport {
    RehearsalReport::new(
        Rehearsal::NotRun {
            reason: refused.reason,
        },
        Attempt::NeverAttempted,
        EffectCounts::none(),
        candidate_sha256,
    )
    .with_observation(Observation::refused(refused.refusal))
}

impl Record {
    /// The report of a room prepared, then closed without a run.
    pub(super) fn withdrawn(self, refused: Refused) -> RehearsalReport {
        let (room, digest) = (self.room(), self.admitted_digest.clone());
        let candidate_sha256 = self.candidate_sha256.clone();
        let mut observation = self.observation();
        observation.refusal = Some(refused.refusal);
        RehearsalReport::new(
            Rehearsal::NotRun {
                reason: refused.reason,
            },
            Attempt::NeverAttempted,
            EffectCounts::none(),
            candidate_sha256,
        )
        .with_room(room)
        .with_admitted_digest(digest)
        .with_observation(observation)
    }

    /// The report of a run that began, settled or stopped at the bound.
    pub(super) fn ran(self, ran: &Ran, screened: &Screened) -> RehearsalReport {
        let elapsed_ms = ran.elapsed_ms;
        let (outcome, attempt, failure) = match &ran.settled {
            None => (
                Rehearsal::NotRun {
                    reason: format!(
                        "the run passed its time bound of {} ms and was stopped",
                        self.bounds.time_ms
                    ),
                },
                Attempt::Stopped { elapsed_ms },
                None,
            ),
            Some(Err(error)) => {
                let code = error.spec_code();
                let failure = FailureRecord::new("", code.clone(), RecordedCause::Engine);
                let failed = Rehearsal::Failed {
                    code,
                    task: String::new(),
                    message: cut(&error.wire_message()),
                };
                (failed, Attempt::Completed { elapsed_ms }, Some(failure))
            }
            Some(Ok(outcome)) => {
                let (settled, failure) = self.settled(outcome, screened);
                (settled, Attempt::Completed { elapsed_ms }, failure)
            }
        };
        let (room, digest) = (self.room(), self.admitted_digest.clone());
        let candidate_sha256 = self.candidate_sha256.clone();
        let mut observation = self.observation();
        observation.failure = failure;
        RehearsalReport::new(outcome, attempt, effects(ran.denied), candidate_sha256)
            .with_room(room)
            .with_admitted_digest(digest)
            .with_observation(observation)
    }

    /// A settled run's outcome: the first failing task in the candidate's order, else every
    /// output the candidate writes, or those it wrote unconditionally and never published.
    fn settled(
        &self,
        outcome: &RunOutcome,
        screened: &Screened,
    ) -> (Rehearsal, Option<FailureRecord>) {
        if !outcome.ok {
            return failed(outcome, screened);
        }
        let missing: Vec<String> = screened
            .outputs
            .iter()
            .filter(|output| !self.published(&output.at) && !allowed_absence(output, outcome))
            .map(|output| output.path.clone())
            .collect();
        if !missing.is_empty() {
            return (Rehearsal::Missing { outputs: missing }, None);
        }
        let outputs = screened
            .outputs
            .iter()
            .map(|output| self.rehearsed(output))
            .collect();
        (Rehearsal::Passed { outputs }, None)
    }

    /// Whether the run itself published the file at `at`: the room's ledger, never the room.
    fn published(&self, at: &str) -> bool {
        self.ledger.written.iter().any(|written| written == at)
    }

    /// What the room holds at one output, bounded, with whether the run published it.
    fn rehearsed(&self, output: &Output) -> RehearsedOutput {
        let state = self
            .finals
            .iter()
            .find(|(at, _)| *at == output.at)
            .map(|(_, read)| &read.state);
        let shown = match state {
            Some(FinalState::File { digest, held }) => {
                RehearsedOutput::new(output.path.clone(), held.text())
                    .with_full(digest.bytes, digest.sha256.clone())
            }
            _ => RehearsedOutput::new(output.path.clone(), "").with_full(0, ""),
        };
        shown.with_written(self.published(&output.at))
    }

    /// The room's facts: prepared, removed or not, and what arrived after its phase.
    fn room(&self) -> RoomEvidence {
        let late = u32::try_from(self.ledger.late_refused).unwrap_or(u32::MAX);
        RoomEvidence::new(true, self.cleaned).with_late_refused(late)
    }

    /// The receipts, in the host's order, and the bytes they account for.
    fn observation(self) -> Observation {
        let copied = self
            .copies
            .iter()
            .filter_map(|copy| copy.room.as_ref())
            .fold(0_u64, |total, room| total.saturating_add(room.bytes));
        let finals: Vec<FinalReceipt> = self.finals.into_iter().map(|(_, read)| read).collect();
        let read = finals.iter().fold(0_u64, |total, read| match &read.state {
            FinalState::File { digest, .. } => total.saturating_add(digest.bytes),
            _ => total,
        });
        let mut observation = Observation::none();
        observation.copies = self.copies;
        observation.finals = finals;
        observation.ledger = self.ledger;
        observation.spent = Spent::new(copied, read);
        observation.bounds = self.bounds;
        observation
    }
}

/// A failed run: its first failing task in the candidate's order, that task's code and cause.
fn failed(outcome: &RunOutcome, screened: &Screened) -> (Rehearsal, Option<FailureRecord>) {
    let first = screened.tasks.iter().find_map(|task| {
        outcome
            .records
            .get(task)
            .filter(|record| record.status == TaskStatus::Failure)
            .map(|record| (task, record))
    });
    let Some((task, record)) = first else {
        let failure = FailureRecord::new("", "", RecordedCause::Engine);
        let failed = Rehearsal::Failed {
            code: String::new(),
            task: String::new(),
            message: "the run failed, and no task recorded a failure".to_owned(),
        };
        return (failed, Some(failure));
    };
    let (code, message) = record
        .error
        .as_ref()
        .map_or((String::new(), String::new()), |error| {
            (error.code.clone(), cut(&error.message))
        });
    let failure = FailureRecord::new(task.clone(), code.clone(), recorded(record.cause));
    let failed = Rehearsal::Failed {
        code,
        task: task.clone(),
        message,
    };
    (failed, Some(failure))
}

/// Whether an output's absence is the candidate's own decision: every task that writes it is
/// guarded by a `when:` whose gate settled it skipped.
fn allowed_absence(output: &Output, outcome: &RunOutcome) -> bool {
    output.writers.iter().all(|(task, guarded)| {
        *guarded
            && outcome.records.get(task).is_some_and(|record| {
                record.status == TaskStatus::Skipped && record.cause == TerminalCause::Gate
            })
    })
}

/// The recorded cause of a failing task.
fn recorded(cause: TerminalCause) -> RecordedCause {
    match cause {
        TerminalCause::VerbError => RecordedCause::VerbError,
        TerminalCause::Timeout => RecordedCause::Timeout,
        TerminalCause::RetryExhausted => RecordedCause::RetryExhausted,
        _ => RecordedCause::Engine,
    }
}

/// The denied attempts, in the report's own counts.
fn effects(denied: DeniedEffects) -> EffectCounts {
    let mut effects = EffectCounts::none();
    effects.network = denied.network;
    effects.provider = denied.provider;
    effects.spawn = denied.spawn;
    effects.prompt = denied.prompt;
    effects.secret = denied.secret;
    effects.child = denied.child;
    effects
}

/// At most [`MESSAGE_BOUND`] bytes of `text`, cut on a character boundary.
fn cut(text: &str) -> String {
    let end = (0..=text.len().min(MESSAGE_BOUND))
        .rev()
        .find(|at| text.is_char_boundary(*at))
        .unwrap_or(0);
    text.get(..end).unwrap_or_default().to_owned()
}
