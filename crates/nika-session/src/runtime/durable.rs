// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Durable boundaries around the existing session operations.

use super::history::{
    AuthorityState, EffectState, History, HistoryMode, Operation, RunState, Saved,
};
use std::path::Path;

use super::{IntentDraft, Refusal, RefusalClass, SessionRuntime, TurnOutcome};

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

    /// Record intent before applying a currently pending, freshly witnessed proposal.
    pub fn consent(&mut self, answer: &str) -> TurnOutcome {
        self.recorded(Operation::Consent, answer, |s| s.consent_unrecorded(answer))
    }

    /// Record the human's gate answer before returning a resume request.
    pub fn answer_gate(&mut self, line: &str) -> TurnOutcome {
        self.recorded(Operation::Gate, line, |s| s.answer_gate_unrecorded(line))
    }

    /// Record the host's observation; this never discovers or reruns an execution.
    pub fn observe_run(&mut self, exit: u8, trace: Option<&Path>) -> TurnOutcome {
        self.recorded(Operation::Observation, "(run observation)", |s| {
            s.observe_run_unrecorded(exit, trace)
        })
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
        let effect = if matches!(operation, Operation::Consent | Operation::Gate)
            && let TurnOutcome::Refusal(Refusal {
                class: RefusalClass::Io,
                text,
            }) = &outcome
        {
            self.remember("(effect)", text);
            EffectState::Unknown
        } else {
            EffectState::NoUncertaintyReported
        };
        let run = match (&outcome, operation) {
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
        let evidence = crate::broker::redact(&format!("{outcome:?}")).0;
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

fn history_refusal(error: impl std::fmt::Display) -> Refusal {
    Refusal::new(
        RefusalClass::Io,
        format!(
            "conversation history unavailable: {error} · session stopped; close and reconcile before reopening"
        ),
    )
}
