// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A typed answer names the question shown. This door never uses general submit.
//! An unchanged question can retain exact answer text; closure never implies Applied.

use nika_session::runtime::TurnOutcome;
use nika_session::work;
use std::sync::Arc;

use super::Live;
use super::feed::Feed;
use crate::model::{Beat, Committed, Kind, Turn};

impl Live {
    pub(super) fn lent_answer(&mut self, feed: Feed, text: &str, witness: &str) -> Turn {
        if let Ok(mut guard) = self.busy.lock() {
            *guard = Some(feed.with_legs(Arc::clone(&self.legs)));
        }
        let turn = self.typed_answer(text, witness);
        self.fold_candidate();
        if let Ok(mut guard) = self.busy.lock() {
            *guard = None;
        }
        turn
    }

    fn typed_answer(&mut self, text: &str, witness: &str) -> Turn {
        if self.action.is_some() || witness.starts_with("action:") {
            return self.action_answer(text, witness);
        }
        if let work::Waiting::Consent { proposal } = &self.shown {
            let proposal = proposal.clone();
            return self.proposal_answer(&proposal, text, witness);
        }
        let (id, key) = match &self.shown {
            work::Waiting::Question { id, key }
                if super::asked::witness(id, self.question_epoch) == witness =>
            {
                (id.clone(), key.clone())
            }
            work::Waiting::Questions { ids } => {
                let Some(runtime) = self.runtime.as_ref() else {
                    return self.not_shown(text);
                };
                let snapshot = runtime.work();
                let Some(question) = super::asked::grouped_question(&snapshot, ids)
                    .filter(|q| super::asked::witness(&q.id, self.question_epoch) == witness)
                else {
                    return self.not_shown(text);
                };
                (question.id.clone(), question.key.clone())
            }
            _ => return self.not_shown(text),
        };
        let Some(runtime) = self.runtime.as_mut() else {
            return self.not_shown(text);
        };
        let question = runtime.work().question;
        let label = question
            .as_ref()
            .filter(|q| q.key == key)
            .map_or(key.as_str(), |q| q.label.as_str());
        let before = runtime.pending_question_id();
        let outcome = runtime.answer_question_for(&id, text);
        let after = runtime.pending_question_id();
        let taken = reporting::read(runtime.work().answered.as_ref(), &id, &key, label);
        // Narrow retention only: a Question that still owns this exact strong id
        // bound nothing. A refusal before admission or with that id still pending
        // also did not bind it. A later compilation refusal may follow binding;
        // never restore or infer acceptance from that final outcome alone.
        let retain = match &outcome {
            TurnOutcome::Question { .. } => after.as_ref() == Some(&id),
            TurnOutcome::Refusal(_) => before.as_ref() != Some(&id) || after.as_ref() == Some(&id),
            _ => false,
        };
        let (mut beats, handoff) = self.map(outcome);
        if let Some(notice) = taken {
            beats.insert(0, notice);
        }
        if retain {
            beats.push(Beat::NotTaken(text.to_owned()));
        }
        Turn { beats, handoff }
    }

    fn proposal_answer(
        &mut self,
        proposal: &nika_session::ProposalId,
        text: &str,
        witness: &str,
    ) -> Turn {
        let Some(runtime) = self.runtime.as_mut() else {
            return self.not_shown(text);
        };
        let snapshot = runtime.work();
        let Some(question) = snapshot
            .open_choices
            .iter()
            .find(|question| question.state == work::AskedState::Open)
            .filter(|question| {
                super::asked::proposal_witness(proposal, &question.id, self.question_epoch)
                    == witness
            })
        else {
            return self.not_shown(text);
        };
        let id = question.id.clone();
        let outcome = runtime.answer_proposal_choice_for(proposal, &id, text);
        let after = runtime.work();
        let remains = after
            .open_choices
            .iter()
            .any(|q| q.id == id && q.state == work::AskedState::Open);
        // A later compilation can refuse after the choice was bound. Retain only an
        // open choice or the Session's exact pre-admission refusal for this question.
        let refused = after.answered.as_ref().is_some_and(|answered| {
            answered.question == id && matches!(answered.act, work::AnswerAct::Refused { .. })
        });
        let (mut beats, handoff) = self.map(outcome);
        if remains || refused {
            beats.push(Beat::NotTaken(text.to_owned()));
        }
        Turn { beats, handoff }
    }

    pub(super) fn not_shown(&mut self, text: &str) -> Turn {
        Turn {
            beats: vec![
                Beat::Say(Committed::new(
                    Kind::Refusal,
                    "This answer belongs to a question that is no longer shown. Nothing was sent.",
                )),
                Beat::NotTaken(text.to_owned()),
                self.wait(),
            ],
            handoff: None,
        }
    }
}

mod reporting;

#[cfg(test)]
mod tests;
