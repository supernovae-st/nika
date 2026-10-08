// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The session's state as every host reads it (ADR-133 · the portable session): one precedence
//! for what the next line answers, one routing of that line, and the typed snapshot of the work
//! ([`crate::work`]). The terminal renderer, the plain loop and a remote door call these instead
//! of keeping a routing bit of their own; what a line may decide stays the session's.

use super::{SessionRuntime, TurnOutcome};
use crate::change::{ProjectChange, ProjectChangeSet};
use crate::outcome::{Refusal, RefusalClass};
use crate::work::{Authoring, Candidate, DocumentRevision, Request, Run, Saved, Waiting, Work};

/// Said when a line arrives while a proposal waits but the host showed none: nothing that
/// was not seen is consented.
pub const NOTHING_SHOWN: &str = "no proposal is on screen to answer · nothing was applied · the proposal is shown again after this line";

/// Said when a line arrives while a run waits at a gate the host did not show: the line was
/// typed for something else, so it answers nothing there and the gate keeps waiting.
pub const GATE_NOT_SHOWN: &str = "a run is waiting at a gate this line was not typed for · nothing was answered · the gate is shown again after this line";

/// Said when a line arrives while a value is asked that the host did not show (another
/// question, an input, an activation value, or one asked again after a restore): the line
/// fills nothing and what is asked keeps waiting.
pub const VALUE_NOT_SHOWN: &str = "this line was not typed for the value asked now · nothing was filled · the question is shown again after this line";

/// A line that leaves or declines: it applies nothing, whether a proposal was shown or not.
fn declines(line: &str) -> bool {
    matches!(line.trim(), "/quit" | "/exit")
        || super::decision_answer(line) == super::DecisionAnswer::Decline
}

/// A line no waiting state takes as its answer: leaving, or a local command that reads the
/// session's own facts (`/status` · `/details` · …). It goes through whatever waits unchanged.
fn beside_any_answer(line: &str) -> bool {
    super::is_quit(line) || super::local_command_of(line).is_some()
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
        } else if let (Some(question), Some(id)) =
            (self.pending_question(), self.pending_question_id())
        {
            Waiting::Question {
                key: question.key.clone(),
                id,
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
    /// Every answer names what it answers, and a line typed for something else answers
    /// nothing. A consent answers the proposal shown, by its identity: another proposal
    /// waiting makes it stale, a decided one already consumed ([`Self::consent_to`]); with no
    /// proposal shown, a declining or leaving line still declines and any other line is
    /// refused. A gate answer names the gate shown ([`Self::answer_gate_for`]); a gate the host
    /// did not show takes no answer from the line. An authoring question takes the line only
    /// under the identity it was shown with ([`Self::answer_question_for`]): an answer typed
    /// before a restore, a revision or another question is refused as stale. A run input or an
    /// activation value takes the line only when the host showed that very value. Leaving and
    /// the session's read-only commands go through whatever waits; what waits keeps waiting
    /// after a refusal. With nothing waiting, the line is a new turn.
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
                _ if beside_any_answer(line) => self.answer_gate(line.trim()),
                _ => {
                    TurnOutcome::Refusal(Refusal::new(RefusalClass::StaleRevision, GATE_NOT_SHOWN))
                }
            },
            Waiting::Question { .. } => match shown {
                Waiting::Question { id, .. } => self.answer_question_for(id, line),
                _ if beside_any_answer(line) => self.turn(line),
                _ => {
                    TurnOutcome::Refusal(Refusal::new(RefusalClass::StaleRevision, VALUE_NOT_SHOWN))
                }
            },
            waiting @ (Waiting::Input { .. } | Waiting::Activation { .. }) => {
                if *shown == waiting || beside_any_answer(line) {
                    self.turn(line)
                } else {
                    TurnOutcome::Refusal(Refusal::new(RefusalClass::StaleRevision, VALUE_NOT_SHOWN))
                }
            }
            // The one-time cost decision reads its answer in a turn, as does a free line.
            _ => self.turn(line),
        }
    }

    /// The work as every host reads it: the request and the compiler's last word on it, what
    /// waits, the candidate under review with its audits and reach, the workflow saved last, the
    /// run requested last with the reach of its bytes, the last observed run and the rail.
    /// Reading it audits nothing, reads no file and decides nothing.
    #[must_use]
    pub fn work(&self) -> Work {
        let candidate = self.candidate().map(|c| {
            let revision = self.document_revision(c.set);
            Candidate::of(c.id, c.set, c.aside, c.rehearsed.is_some()).with_revision(revision)
        });
        let saved = self.last_workflow.clone().map(|workflow| {
            // The reach belongs to the bytes the last consent saved, not to a workflow only run.
            let world = (self.saved_reach.as_ref())
                .filter(|(path, _)| *path == workflow)
                .map(|(_, world)| world.clone());
            Saved::new(workflow, self.last_check_clean, world)
        });
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
            self.requested_run.clone(),
            run,
            (&self.lifecycle()).into(),
        )
        .with_authoring(self.last_outcome.as_ref().map(Authoring::of))
    }
}

impl SessionRuntime {
    /// How the pending candidate's workflow was revised over its complete document, while the
    /// proposing compile's record binds its bytes (the same description its work snapshot holds).
    #[must_use]
    pub fn pending_revision(&self) -> Option<DocumentRevision> {
        self.candidate().and_then(|c| self.document_revision(c.set))
    }

    /// How the proposing compile revised the candidate's workflow over its complete document,
    /// only when its record binds one of the set's workflow bytes (a later or earlier candidate
    /// is never described by it), each composed component witnessed on those bytes.
    fn document_revision(&self, set: &ProjectChangeSet) -> Option<DocumentRevision> {
        let record = self.proposed_revision.as_ref()?;
        let bound = record["candidate_sha256"].as_str()?;
        let bytes = (set.changes.iter())
            .filter(|change| change.is_workflow())
            .map(ProjectChange::content)
            .find(|content| nika_compile::surface::sha256(content) == bound)?;
        let witnesses: Vec<String> = (record["components"].as_array().into_iter().flatten())
            .map(|receipt| {
                let seen = nika_compile_seats::foundry::witness::witness(receipt, bytes);
                seen["verdict"].as_str().unwrap_or("unwitnessed").to_owned()
            })
            .collect();
        DocumentRevision::of(record, &witnesses)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests;
