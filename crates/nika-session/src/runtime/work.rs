// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The session's state as every host reads it (ADR-133 · the portable session): one precedence
//! for what the next line answers, one routing of that line, and the typed snapshot of the work
//! ([`crate::work`]). The terminal renderer, the plain loop and a remote door call these instead
//! of keeping a routing bit of their own; what a line may decide stays the session's.

use super::{SessionRuntime, TurnOutcome};
use crate::outcome::{Refusal, RefusalClass};
use crate::work::{Candidate, Request, Run, Saved, Waiting, Work};

/// Said when a line arrives while a proposal waits but the host showed none: nothing that
/// was not seen is consented.
pub const NOTHING_SHOWN: &str = "no proposal is on screen to answer · nothing was applied · the proposal is shown again after this line";

/// A line that leaves or declines: it applies nothing, whether a proposal was shown or not.
fn declines(line: &str) -> bool {
    matches!(line.trim(), "/quit" | "/exit")
        || super::decision_answer(line) == super::DecisionAnswer::Decline
}

impl SessionRuntime {
    /// What the next line answers, by the one precedence every host shares: the one-time
    /// cost decision, the choice of intelligence, a proposal's consent, a run's gate, then the
    /// value an authoring question, a run input or an activation asks; else a new turn.
    #[must_use]
    pub fn waiting(&self) -> Waiting {
        if self.waiting_cost_choice() {
            Waiting::CostChoice
        } else if self.pending_choice {
            Waiting::IntelligenceChoice
        } else if let Some(proposal) = self.pending_proposal() {
            Waiting::Consent { proposal }
        } else if let Some(gate) = self.waiting_gate() {
            Waiting::Gate { gate }
        } else if let Some(question) = self.pending_question() {
            Waiting::Question {
                key: question.key.clone(),
            }
        } else if let Some(name) = self.pending_input() {
            Waiting::Input {
                name: name.to_owned(),
            }
        } else if let Some(key) = self.pending_activation() {
            Waiting::Activation {
                key: key.to_owned(),
            }
        } else {
            Waiting::Free
        }
    }

    /// One line, routed to what waits now. `shown` is what the host displayed when the line
    /// was typed ([`Self::waiting`] at that moment).
    ///
    /// A consent answers the proposal shown, by its identity: another proposal waiting makes
    /// it stale, a decided one already consumed ([`Self::consent_to`]). With no proposal shown,
    /// a declining or leaving line still declines and any other line is refused: nothing is
    /// consented that was not seen. A gate answer names the gate shown
    /// ([`Self::answer_gate_for`]); a host that showed none answers the waiting gate, the
    /// keyboard's contract. Every other state takes the line as a turn.
    pub fn submit(&mut self, line: &str, shown: &Waiting) -> TurnOutcome {
        match self.waiting() {
            Waiting::IntelligenceChoice => self.choose(line.trim()),
            Waiting::Consent { .. } => match shown {
                Waiting::Consent { proposal } => self.consent_to(proposal, line.trim()),
                _ if declines(line) => self.consent(line.trim()),
                _ => TurnOutcome::Refusal(Refusal::new(RefusalClass::WrongState, NOTHING_SHOWN)),
            },
            Waiting::Gate { .. } => match shown {
                Waiting::Gate { gate } => self.answer_gate_for(gate, line.trim()),
                _ => self.answer_gate(line.trim()),
            },
            // The one-time cost decision reads its answer in a turn, as does every other state.
            _ => self.turn(line),
        }
    }

    /// The work as every host reads it: the request, what waits, the candidate under review
    /// with its audits and reach, the workflow saved last, the last observed run and the rail.
    /// Reading it audits nothing, reads no file and decides nothing.
    #[must_use]
    pub fn work(&self) -> Work {
        let candidate = self
            .candidate()
            .map(|c| Candidate::of(c.id, c.set, c.aside, c.rehearsed.is_some()));
        let saved = self
            .last_workflow
            .clone()
            .map(|workflow| Saved::new(workflow, self.last_check_clean));
        let run = self.kept_run().and_then(Result::ok).map(|kept| {
            Run::new(
                self.last_run.is_some(),
                kept.workflow,
                kept.exit,
                kept.trace,
                kept.execution,
                kept.workflow_sha256,
                kept.chain_head,
                kept.chain_len,
            )
        });
        Work::new(
            self.snapshot.root.clone(),
            Request::new(
                self.intent.goal.clone(),
                self.intent.decisions.clone(),
                self.intent.unresolved.clone(),
            ),
            self.waiting(),
            candidate,
            saved,
            run,
            (&self.lifecycle()).into(),
        )
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests;
