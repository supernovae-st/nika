// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The fresh cost review a requested run's child stops at (ADR-133 · the portable session). The
//! host keeps the live child and its one-use reply; the session keeps what waits and reads the
//! line with the one spending grammar ([`super::decision_answer`]). One yes runs the child once;
//! a decline, leaving or an interruption sends nothing; the evidence, the help card and the
//! status answer beside it and the review keeps waiting. No answer is invented for the human.

use super::decision::{DecisionAnswer, decision_answer, local_command_of};
use super::{SessionRuntime, TurnOutcome, is_quit};
use crate::outcome::{Refusal, RefusalClass, ReviewId};

/// Said beside a cost review that still waits.
pub(super) const RUN_STILL_WAITS: &str = "the fresh Run cost decision still waits · `yes`/`oui` runs it once · `no`/`non` cancels · `details` shows the evidence";

/// The review a requested run's child waits at: the identity an answer names and the evidence
/// behind its first screen, shown on demand.
pub(super) struct RunReview {
    pub(super) id: ReviewId,
    details: String,
}

impl SessionRuntime {
    /// A requested run's child stopped at its fresh cost challenge: the session holds the review
    /// the host shows (`question` first, `details` on demand) and returns the identity an answer
    /// names. The child, its pipe and its one-use reply stay with the host; a later review
    /// replaces this one.
    pub fn run_review_asked(&mut self, question: &str, details: &str) -> ReviewId {
        self.reviews_asked += 1;
        let id = ReviewId::new(self.reviews_asked, question, details);
        self.run_review = Some(RunReview {
            id: id.clone(),
            details: details.to_owned(),
        });
        id
    }

    /// The review a line answers now, when one waits.
    pub(super) fn waiting_review(&self) -> Option<ReviewId> {
        self.run_review.as_ref().map(|review| review.id.clone())
    }

    /// An interruption at the review: nothing is sent and the review is gone. `None` when none
    /// waited.
    pub fn decline_run_review(&mut self) -> Option<TurnOutcome> {
        let review = self.run_review.take()?;
        Some(TurnOutcome::RunReviewed {
            review: review.id,
            approve: false,
        })
    }

    /// One line at the review `shown`: the whole-line yes approves it once, a decline sends
    /// nothing, leaving closes the session and sends nothing. The evidence, the help card and
    /// the status answer beside it; any other line is asked again. An answer naming another
    /// review than the one waiting answers nothing.
    pub(super) fn answer_run_review(&mut self, shown: &ReviewId, line: &str) -> TurnOutcome {
        let Some(review) = self
            .run_review
            .as_ref()
            .filter(|review| review.id == *shown)
        else {
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::StaleRevision,
                format!("the cost review {shown} is not the one waiting · nothing was sent"),
            ));
        };
        let trimmed = line.trim();
        if is_quit(trimmed) {
            self.run_review = None;
            return TurnOutcome::Quit;
        }
        match local_command_of(trimmed) {
            Some("/help") => {
                return TurnOutcome::Aside(format!("{}\n{RUN_STILL_WAITS}", self.help_card()));
            }
            Some("/status") => {
                return TurnOutcome::Aside(format!("{}\n{RUN_STILL_WAITS}", self.status()));
            }
            _ => {}
        }
        match decision_answer(trimmed) {
            DecisionAnswer::Details => TurnOutcome::Aside(review.details.clone()),
            DecisionAnswer::Approve => self.decided(true),
            DecisionAnswer::Decline => self.decided(false),
            // Unknown, and any answer this grammar may add: asked again, never a yes.
            _ => TurnOutcome::Aside(format!(
                "« {trimmed} » is not a yes or a no · nothing was sent\n{RUN_STILL_WAITS}"
            )),
        }
    }

    /// The review decided once: it no longer waits, and the host answers its child.
    fn decided(&mut self, approve: bool) -> TurnOutcome {
        match self.run_review.take() {
            Some(review) => TurnOutcome::RunReviewed {
                review: review.id,
                approve,
            },
            None => TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                "no cost review waits · nothing was sent",
            )),
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests;
