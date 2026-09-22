// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Durable boundaries around the existing session operations: the
//! conversation history under the home (the transcript), the project's
//! structured record (#1464 · `.nika/session-state.json`) and the consent
//! journal (#1465 · `.nika/consents.ndjson`).

use super::history::{
    AuthorityState, EffectState, History, HistoryMode, Operation, RunState, Saved,
};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use super::{IntentDraft, Refusal, RefusalClass, SessionRuntime, TurnOutcome};
use crate::change::{Applied, ApplyAttempt, PendingGate, ProjectChangeSet};
use crate::consent::{CONSENTS_FILE, ConsentDecision, ConsentRecord};
use crate::intelligence::now_rfc3339;
use crate::outcome::ProposalId;
use crate::state::{Pending, STATE_FILE, SessionState};

impl SessionRuntime {
    /// Enable private, project-bound conversation history below `home/.nika`.
    ///
    /// Rebuilds the conversation projection and returns a notice on recovery.
    /// Pending proposals, gates and old consent identities never regain
    /// authority. The current project and intelligence are observed afresh.
    /// Call immediately after `open` or `open_with`, before the first turn.
    ///
    /// # Errors
    /// Refuses concurrent ownership, corrupt/oversized history or failed I/O.
    /// A failed enable also blocks this instance; ignoring the error cannot
    /// silently downgrade a requested durable session to an ephemeral one.
    pub fn enable_history(&mut self, home: &Path) -> Result<Option<String>, Refusal> {
        if !matches!(self.history, HistoryMode::Ephemeral)
            || !self.recent.is_empty()
            || self.pending.is_some()
            || self.intent.goal.is_some()
        {
            self.history =
                HistoryMode::Blocked("history activation must precede the first turn".to_owned());
            return Err(Refusal::new(
                RefusalClass::WrongState,
                "enable history before the first turn",
            ));
        }
        self.history = HistoryMode::Blocked("conversation history did not open".to_owned());
        let history = History::open(home, &self.snapshot.root).map_err(history_refusal)?;
        self.intent = IntentDraft {
            goal: history.state.goal.clone(),
            decisions: history.state.decisions.clone(),
            unresolved: history.state.unresolved.clone(),
        };
        self.recent.clone_from(&history.state.recent);
        let notice = history.restored.then(|| {
            let mut text = "conversation restored · previous proposals and gates require fresh validation".to_owned();
            if history.uncertain {
                text.push_str("\ninterrupted operation: its result may be unknown; inspect effects and receipts before retrying · nothing was replayed");
            }
            text
        });
        self.history = HistoryMode::Active(Box::new(history));
        Ok(notice)
    }

    /// Process one turn, recording its boundaries when history is enabled.
    pub fn turn(&mut self, input: &str) -> TurnOutcome {
        // Closing remains possible even after storage failure.
        if matches!(input.trim(), "/quit" | "/exit") {
            self.pending = None;
            self.pending_gate = None;
            return TurnOutcome::Quit;
        }
        self.recorded(Operation::Turn, input, |s| s.turn_unrecorded(input))
    }

    /// Answer the current intelligence choice through the same durable boundary.
    pub fn choose(&mut self, answer: &str) -> TurnOutcome {
        self.recorded(Operation::Choice, answer, |s| s.choose_unrecorded(answer))
    }

    /// Record intent before applying a currently pending, freshly witnessed
    /// proposal. A consent that decided (applied · discarded · landed
    /// partially) is a decision of the durable intent and keeps the
    /// project's structured record (#1464); a held question, a stale
    /// revision or a blocked history decides nothing and writes nothing.
    pub fn consent(&mut self, answer: &str) -> TurnOutcome {
        let staged = self.pending_proposal();
        let outcome = self.recorded(Operation::Consent, answer, |s| {
            let outcome = s.consent_unrecorded(answer);
            if let (Some(id), TurnOutcome::Facts(text)) = (&staged, &outcome)
                && text.starts_with("discarded")
            {
                s.intent.decisions.push(format!("discarded proposal {id}"));
            }
            outcome
        });
        let decided = staged.is_some()
            && self.pending.is_none()
            && !matches!(
                &outcome,
                TurnOutcome::Refusal(Refusal {
                    class: RefusalClass::StaleRevision,
                    ..
                })
            );
        if decided {
            self.keep_state(outcome)
        } else {
            outcome
        }
    }

    /// Record the human's gate answer before returning a resume request;
    /// an answer that resumes is a decision of the durable intent and keeps
    /// the project's structured record (#1464).
    pub fn answer_gate(&mut self, line: &str) -> TurnOutcome {
        let waiting = self.waiting_gate();
        let outcome = self.recorded(Operation::Gate, line, |s| {
            let outcome = s.answer_gate_unrecorded(line);
            if let (Some(gate), TurnOutcome::ResumeRequested { answer, .. }) = (&waiting, &outcome)
            {
                s.intent
                    .decisions
                    .push(format!("answered the gate {gate}: {answer}"));
            }
            outcome
        });
        if matches!(outcome, TurnOutcome::ResumeRequested { .. }) {
            self.keep_state(outcome)
        } else {
            outcome
        }
    }

    /// Record the host's observation; this never discovers or reruns an
    /// execution. Every observed run keeps the project's structured record
    /// (#1464): the gate it paused on, when it did, is what waits.
    pub fn observe_run(&mut self, exit: u8, trace: Option<&Path>) -> TurnOutcome {
        let outcome = self.recorded(Operation::Observation, "(run observation)", |s| {
            s.observe_run_unrecorded(exit, trace)
        });
        self.keep_state(outcome)
    }

    /// The project's structured record, read at open (#1464) — after
    /// [`Self::enable_history`] when the door keeps one: the record wins
    /// over the transcript's projection for the goal, the decisions and the
    /// unresolved questions (the transcript keeps the dialogue). A proposal
    /// never survives a close (ADR-133 · nothing is written before its
    /// consent); a gate pending at close is the engine's own paused trace
    /// and waits again when that trace still carries the pause. A record
    /// that cannot be read is named and left in place, never rewritten.
    pub fn restore_state(&mut self) -> Option<String> {
        let state = match SessionState::load(&self.snapshot.root) {
            Ok(Some(state)) => state,
            Ok(None) => return None,
            Err(error) => {
                return Some(format!(
                    "session record unreadable (.nika/{STATE_FILE}: {error}) · left in place · this session starts from the conversation alone"
                ));
            }
        };
        self.intent = IntentDraft {
            goal: state.goal,
            decisions: state.decisions,
            unresolved: state.unresolved,
        };
        let mut notice = format!(
            "session record restored (.nika/{STATE_FILE} · written {})",
            state.updated_at
        );
        if let Some(Pending::Gate {
            workflow,
            trace,
            task,
            ..
        }) = state.pending
        {
            match PendingGate::from_trace(&workflow, &trace) {
                Some(gate) => {
                    let _ = write!(notice, "\n{}", gate.question());
                    self.last_workflow = Some(workflow);
                    self.pending_gate = Some(gate);
                }
                None => {
                    let _ = write!(
                        notice,
                        "\n  the run paused at `{task}` no longer waits: its trace `{}` carries no pause",
                        trace.display()
                    );
                }
            }
        }
        Some(notice)
    }

    /// The durable evidence of a consent that landed every change (#1465),
    /// and the decision it is: the empty string, or the warning line the
    /// report must carry when the journal refused.
    pub(super) fn evidence_applied(
        &mut self,
        set: &ProjectChangeSet,
        id: &ProposalId,
        applied: &Applied,
    ) -> String {
        self.witness_consent(set, id, ConsentDecision::Applied, &applied.written)
    }

    /// The durable evidence of a consent whose later write was refused
    /// after earlier ones landed (#1465): exactly the write loop's record.
    pub(super) fn evidence_partial(
        &mut self,
        set: &ProjectChangeSet,
        id: &ProposalId,
        attempt: &ApplyAttempt,
    ) -> String {
        self.witness_consent(set, id, ConsentDecision::Partial, &attempt.written)
    }

    fn witness_consent(
        &mut self,
        set: &ProjectChangeSet,
        id: &ProposalId,
        decision: ConsentDecision,
        written: &[PathBuf],
    ) -> String {
        let paths: Vec<String> = written
            .iter()
            .map(|p| format!("`{}`", p.display()))
            .collect();
        self.intent.decisions.push(match decision {
            ConsentDecision::Applied => {
                format!("applied proposal {id} · wrote {}", paths.join(" · "))
            }
            ConsentDecision::Partial => format!(
                "proposal {id} landed partially · wrote {} · left undecided",
                paths.join(" · ")
            ),
        });
        let record = ConsentRecord::of(set, id, decision, written, now_rfc3339());
        match record.append(&self.snapshot.root) {
            Ok(()) => String::new(),
            Err(error) => format!(
                "\n  ⚠ the consent's evidence was not written (.nika/{CONSENTS_FILE}): {error}"
            ),
        }
    }

    /// The structured record as this session would write it now — the
    /// same redaction the transcript gets.
    fn projected_state(&self) -> SessionState {
        let redact = |s: &String| crate::broker::redact(s).0;
        let mut state = SessionState::new(now_rfc3339());
        state.goal = self.intent.goal.as_ref().map(redact);
        state.decisions = self.intent.decisions.iter().map(redact).collect();
        state.unresolved = self.intent.unresolved.iter().map(redact).collect();
        state.pending = self.pending_gate.as_ref().map(|gate| Pending::Gate {
            workflow: gate.workflow.clone(),
            trace: gate.trace.clone(),
            task: gate.task.clone(),
            mode: gate.mode.clone(),
        });
        state
    }

    /// Keep the structured record after an operation that decided or
    /// observed something. A refused write rides the outcome's own text:
    /// the effect happened, and the human must know the record did not.
    fn keep_state(&self, outcome: TurnOutcome) -> TurnOutcome {
        match self.projected_state().save(&self.snapshot.root) {
            Ok(()) => outcome,
            Err(error) => with_note(
                outcome,
                &format!("the session record was not kept (.nika/{STATE_FILE}): {error}"),
            ),
        }
    }

    pub(super) fn recorded(
        &mut self,
        operation: Operation,
        input: &str,
        perform: impl FnOnce(&mut Self) -> TurnOutcome,
    ) -> TurnOutcome {
        let mut history = match std::mem::replace(
            &mut self.history,
            HistoryMode::Blocked("the previous operation did not complete".to_owned()),
        ) {
            HistoryMode::Ephemeral => {
                self.history = HistoryMode::Ephemeral;
                return perform(self);
            }
            HistoryMode::Blocked(message) => {
                self.history = HistoryMode::Blocked(message.clone());
                return TurnOutcome::Refusal(history_refusal(message));
            }
            HistoryMode::Active(history) => history,
        };
        if let Err(error) = history.begin(operation, input) {
            return self.history_failed(error);
        }
        let outcome = perform(self);
        // A choice that resumed a waiting line is judged by that line's
        // own outcome: the record sees what the human's request became.
        let judged = match &outcome {
            TurnOutcome::Resumed { outcome, .. } => outcome.as_ref(),
            other => other,
        };
        let effect = if matches!(operation, Operation::Consent | Operation::Gate)
            && let TurnOutcome::Refusal(Refusal {
                class: RefusalClass::Io,
                text,
            }) = judged
        {
            let text = text.clone();
            self.remember("(effect)", &text);
            EffectState::Unknown
        } else {
            EffectState::NoUncertaintyReported
        };
        let run = match (judged, operation) {
            (TurnOutcome::RunRequested { .. } | TurnOutcome::ResumeRequested { .. }, _) => {
                RunState::AwaitingObservation
            }
            (_, Operation::Observation) => RunState::Idle,
            _ => history.run,
        };
        let authority = if self.pending.is_some() {
            AuthorityState::Proposal
        } else if self.pending_gate.is_some() {
            AuthorityState::Gate
        } else {
            AuthorityState::None
        };
        let evidence = outcome_kind(&outcome).to_owned();
        if let Err(error) =
            history.complete(self.saved_conversation(), run, authority, evidence, effect)
        {
            return self.history_failed(error);
        }
        self.history = HistoryMode::Active(history);
        outcome
    }

    fn history_failed(&mut self, error: impl std::fmt::Display) -> TurnOutcome {
        let reason = format!(
            "{error}; the last operation may be incomplete — inspect the history and effects before retrying"
        );
        self.history = HistoryMode::Blocked(reason.clone());
        TurnOutcome::Refusal(history_refusal(reason))
    }

    fn saved_conversation(&self) -> Saved {
        let redact = |s: &String| crate::broker::redact(s).0;
        Saved {
            goal: self.intent.goal.as_ref().map(redact),
            decisions: self.intent.decisions.iter().map(redact).collect(),
            unresolved: self.intent.unresolved.iter().map(redact).collect(),
            recent: self
                .recent
                .iter()
                .map(|(a, b)| (redact(a), redact(b)))
                .collect(),
        }
    }
}

// Persist no Debug representation of the payload. Debug escapes newlines
// before the line-oriented redactor can see them, and can expose secrets
// already removed from Saved. Dialogue lives in that redacted projection;
// this diagnostic is only a category, never an executable request.
fn outcome_kind(outcome: &TurnOutcome) -> &'static str {
    match outcome {
        TurnOutcome::Reply(_) => "reply",
        TurnOutcome::Facts(_) => "facts",
        TurnOutcome::Help(_) => "help",
        TurnOutcome::Quit => "quit",
        TurnOutcome::Refusal(_) => "refusal",
        TurnOutcome::Ask(_) => "ask",
        TurnOutcome::Held { .. } => "held",
        TurnOutcome::Question { .. } => "question",
        TurnOutcome::Proposal { .. } => "proposal",
        TurnOutcome::RunRequested { .. } => "run_requested",
        TurnOutcome::GateAsk { .. } => "gate_ask",
        TurnOutcome::ResumeRequested { .. } => "resume_requested",
        TurnOutcome::Aside(_) => "aside",
        TurnOutcome::Resumed { outcome, .. } => outcome_kind(outcome),
    }
}

/// The outcome with a warning line appended to whatever text it shows; an
/// outcome without text (a quit · a resume the door runs) carries none —
/// the observation that follows a resume keeps the record again.
fn with_note(outcome: TurnOutcome, note: &str) -> TurnOutcome {
    let line = format!("\n  ⚠ {note}");
    match outcome {
        TurnOutcome::Reply(text) => TurnOutcome::Reply(text + &line),
        TurnOutcome::Facts(text) => TurnOutcome::Facts(text + &line),
        TurnOutcome::Help(text) => TurnOutcome::Help(text + &line),
        TurnOutcome::Ask(text) => TurnOutcome::Ask(text + &line),
        TurnOutcome::Aside(text) => TurnOutcome::Aside(text + &line),
        TurnOutcome::Refusal(why) => {
            TurnOutcome::Refusal(Refusal::new(why.class, why.text + &line))
        }
        TurnOutcome::Held { id, preview } => TurnOutcome::Held {
            id,
            preview: preview + &line,
        },
        TurnOutcome::Proposal { id, preview } => TurnOutcome::Proposal {
            id,
            preview: preview + &line,
        },
        TurnOutcome::Question { key, question } => TurnOutcome::Question {
            key,
            question: question + &line,
        },
        TurnOutcome::RunRequested { report, run } => TurnOutcome::RunRequested {
            report: report + &line,
            run,
        },
        TurnOutcome::GateAsk { id, question } => TurnOutcome::GateAsk {
            id,
            question: question + &line,
        },
        TurnOutcome::Resumed { notice, outcome } => TurnOutcome::Resumed {
            notice,
            outcome: Box::new(with_note(*outcome, note)),
        },
        other @ (TurnOutcome::Quit | TurnOutcome::ResumeRequested { .. }) => other,
    }
}

fn history_refusal(error: impl std::fmt::Display) -> Refusal {
    Refusal::new(
        RefusalClass::Io,
        format!(
            "conversation history unavailable: {error} · session stopped; close and reconcile before reopening"
        ),
    )
}
