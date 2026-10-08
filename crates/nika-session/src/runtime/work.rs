// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The session's state as every host reads it (ADR-133 · the portable session): one precedence
//! for what the next line answers, one routing of that line, and the typed snapshot of the work
//! ([`crate::work`]). The terminal renderer, the plain loop and a remote door call these instead
//! of keeping a routing bit of their own; what a line may decide stays the session's.

use std::fmt::Write as _;

use super::{SessionRuntime, TurnOutcome};
use crate::authoring::AuthoringSeat;
use crate::change::{ProjectChange, ProjectChangeSet};
use crate::intelligence::IntelligenceKind;
use crate::outcome::{Refusal, RefusalClass};
use crate::work::{
    Author, Authoring, Candidate, DecisionSeat, DocumentRevision, Intelligence, Request, Run,
    Saved, Selected, Waiting, Work,
};

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

/// Said when a line arrives while a run's cost review waits that the host did not show: the
/// line was typed for something else, so it decides nothing there and the review keeps waiting.
pub const REVIEW_NOT_SHOWN: &str = "a run's cost review is waiting that this line was not typed for · nothing was sent · the review is shown again after this line";

/// Said when nothing waits but the host showed a value or a choice when the line was typed:
/// the line was typed for that, so it answers nothing and is no new request either.
const SHOWN_NO_LONGER_WAITS: &str = "this line was typed for something that no longer waits · nothing was answered or sent · type it again at the prompt if it is a new request";

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
    /// What the next line answers, by the one precedence every host shares: a requested run's
    /// cost review, the one-time cost decision, the choice of intelligence, a proposal's consent,
    /// a run's gate, then the value an authoring question, a run input or an activation asks;
    /// else a new turn.
    #[must_use]
    pub fn waiting(&self) -> Waiting {
        if let Some(review) = self.waiting_review() {
            Waiting::RunReview { review }
        } else if self.waiting_cost_choice() {
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
    /// activation value takes the line only when the host showed that very value. A run's cost
    /// review takes the line only under the identity it was shown with: one yes runs the child
    /// once, a decline sends nothing ([`Self::run_review_asked`]). Leaving and the session's
    /// read-only commands go through whatever waits; what waits keeps waiting after a refusal.
    /// With nothing waiting, a line typed at a free prompt is a new turn; one typed for what
    /// the host showed goes to that state's identity door, which refuses it as answered,
    /// decided or stale, and a value or a choice no longer asked takes nothing: an answer is
    /// never read as a new request.
    pub fn submit(&mut self, line: &str, shown: &Waiting) -> TurnOutcome {
        match self.waiting() {
            Waiting::RunReview { review } => match shown {
                Waiting::RunReview { review: seen } => self.answer_run_review(seen, line),
                // Leaving and the read-only commands go through: none of them can approve.
                _ if beside_any_answer(line) => self.answer_run_review(&review, line),
                _ => TurnOutcome::Refusal(Refusal::new(
                    RefusalClass::StaleRevision,
                    REVIEW_NOT_SHOWN,
                )),
            },
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
            // The evidence beside the one-time cost decision is read, never a turn: nothing is
            // recorded, answered or reviewed again.
            Waiting::CostChoice
                if super::decision_answer(line) == super::DecisionAnswer::Details =>
            {
                self.cost_choice_evidence()
            }
            Waiting::Free => match shown {
                Waiting::Free => self.turn(line),
                _ if beside_any_answer(line) => self.turn(line),
                Waiting::RunReview { review } => self.answer_run_review(review, line),
                Waiting::Consent { proposal } => self.consent_to(proposal, line.trim()),
                Waiting::Gate { gate } => self.answer_gate_for(gate, line.trim()),
                Waiting::Question { id, .. } => self.answer_question_for(id, line),
                _ => TurnOutcome::Refusal(Refusal::new(
                    RefusalClass::StaleRevision,
                    SHOWN_NO_LONGER_WAITS,
                )),
            },
            // The one-time cost decision reads its answer in a turn.
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
        .with_intelligence(Some(self.intelligence_work()))
        .with_question(self.pending_question())
    }

    /// Who prepares with this session, as selected and resolved here: the configured facts a
    /// host shows, never a call receipt (that one is the compiler's, in [`Authoring`]).
    fn intelligence_work(&self) -> Intelligence {
        let chosen = &self.intelligence;
        let selected = match &chosen.kind {
            IntelligenceKind::Harness { seat, transport } => Selected::new(
                "harness",
                Some(seat.clone()),
                Some(transport.to_string().to_lowercase()),
            ),
            IntelligenceKind::Api { provider } => {
                Selected::new("api", Some(provider.clone()), None)
            }
            IntelligenceKind::Local { provider } => {
                Selected::new("local", Some(provider.clone()), None)
            }
            IntelligenceKind::None => Selected::new("none", None, None),
        }
        .resolved(
            chosen.model.clone(),
            chosen.locus.line(),
            chosen.why.clone(),
            chosen.ready,
        );
        let author = match self.authoring_seat() {
            AuthoringSeat::Provider { model } => Author::new("provider", Some(model.clone()), None),
            AuthoringSeat::Harness {
                seat,
                model,
                transport,
            } => Author::new("harness", model.clone(), None)
                .through(seat.clone(), transport.to_string().to_lowercase()),
            AuthoringSeat::Deterministic { why } => Author::new("deterministic", None, why.clone()),
            AuthoringSeat::Unavailable { why } => {
                Author::new("unavailable", None, Some(why.clone()))
            }
        };
        let context = self.authoring_context();
        let decision = (context.decision())
            .map(|seat| DecisionSeat::new(seat.model().to_owned(), seat.refusal().map(Into::into)));
        let effort = context.reasoning().map(|level| level.word().to_owned());
        let selected = match self.conversation {
            Some(_) => selected.for_conversation(),
            None => selected,
        };
        let selected = self.intelligence_chosen().then_some(selected);
        Intelligence::new(selected, author, decision, effort)
    }
}

impl SessionRuntime {
    /// How the pending candidate's workflow was revised over its complete document, while the
    /// proposing compile's record binds its bytes (the same description its work snapshot holds).
    #[must_use]
    pub fn pending_revision(&self) -> Option<DocumentRevision> {
        self.candidate().and_then(|c| self.document_revision(c.set))
    }

    /// The pending proposal's document revision in the consent prompt's words, under the change
    /// line it describes: what changed in place and what its record claims of every other byte;
    /// nothing when no record binds the pending bytes. The proposal's identity is unchanged.
    pub(super) fn revision_words(&self, preview: &mut String) {
        let Some(revision) = self.pending_revision() else {
            return;
        };
        let mut words = if revision.mode == "replaced" {
            "  rewritten whole · no preservation of the earlier bytes is claimed".to_owned()
        } else {
            let claim = match revision.preservation.as_str() {
                p if p.starts_with("verified") => "every other byte verified as the base's",
                p if p.starts_with("by construction") => {
                    "component entries inserted by construction, not re-verified byte by byte"
                }
                p if p.starts_with("edits verified") => {
                    "edits verified byte by byte, component entries by construction"
                }
                p => p,
            };
            format!(
                "  revised in place · {} · {claim}",
                revision.changed.join(", ")
            )
        };
        for component in &revision.components {
            let version = component.version.as_deref().unwrap_or("unversioned");
            let _ = write!(
                words,
                "\n  component · {} {version} · {}",
                component.id, component.witness
            );
        }
        let after = (preview.find("  replaces `"))
            .and_then(|at| preview[at..].find('\n').map(|end| at + end + 1));
        if let Some(at) = after {
            preview.insert_str(at, &format!("{words}\n"));
        } else {
            preview.push('\n');
            preview.push_str(&words);
        }
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
