// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A typed answer names the question shown. This door never uses general submit.
//! An unchanged question can retain exact answer text; closure never implies Applied.
//! A proposal's choice takes the person's words as the author reads them, and a cancel word
//! on its own line discards the proposal through the consent door's own `no`.

use nika_session::runtime::TurnOutcome;
use nika_session::work;
use std::sync::Arc;

use super::Live;
use super::feed::Feed;
use crate::model::{Beat, Committed, Kind, Turn};

/// What a bound answer carries: the person's words, the offered keys the native
/// control selected, or the choice left to Nika by the native control, which names
/// no offer ([`crate::composer::answer::DELEGATION`] its recorded words).
#[derive(Clone, Copy)]
pub(super) enum Reply<'a> {
    Words(&'a str),
    /// The offered keys the native control selected, never words typed.
    Picked(&'a str),
    Delegated,
}

impl Reply<'_> {
    /// Its words, as the Session's doors read them: the person's own, or the control's.
    pub(super) fn text(&self) -> &str {
        match self {
            Self::Words(text) | Self::Picked(text) => text,
            Self::Delegated => crate::composer::answer::DELEGATION,
        }
    }

    /// Its words as they come back to the box when the reply is not taken: the person's own as
    /// they are, the choice left to Nika as its echo says it, in `voice` (« Je laisse le choix à
    /// Nika. », never the control's English words in a French box).
    fn back(&self, voice: crate::lang::Voice) -> String {
        match self {
            Self::Words(text) | Self::Picked(text) => (*text).to_owned(),
            Self::Delegated => voice.say(crate::composer::answer::DELEGATION).to_owned(),
        }
    }
}

impl Live {
    pub(super) fn lent_answer(&mut self, feed: Feed, text: &str, witness: &str) -> Turn {
        self.lent_reply(feed, Reply::Words(text), witness)
    }

    /// The choice of the question painted as `witness` left to Nika: the Session's
    /// delegation doors, never its words as a pick.
    pub(super) fn lent_delegation(&mut self, feed: Feed, witness: &str) -> Turn {
        self.lent_reply(feed, Reply::Delegated, witness)
    }

    /// The offered keys the native control selected for the question painted as
    /// `witness`: an answer by its identity, as words are, except where an action tells
    /// a pick from words ([`Live::action_answer`]).
    pub(super) fn lent_pick(&mut self, feed: Feed, keys: &str, witness: &str) -> Turn {
        self.lent_reply(feed, Reply::Picked(keys), witness)
    }

    fn lent_reply(&mut self, feed: Feed, reply: Reply<'_>, witness: &str) -> Turn {
        if let Ok(mut guard) = self.busy.lock() {
            *guard = Some(feed.with_legs(Arc::clone(&self.legs)));
        }
        let turn = self.typed_answer(reply, witness);
        self.fold_candidate();
        if let Ok(mut guard) = self.busy.lock() {
            *guard = None;
        }
        turn
    }

    fn typed_answer(&mut self, reply: Reply<'_>, witness: &str) -> Turn {
        let text = reply.text();
        // What comes back to the box when nothing takes the reply, in the person's language.
        let back = reply.back(crate::lang::Voice {
            lang: self.lang(),
            ascii: self.ascii,
        });
        // `/new` at any question or decision is the new conversation's door, which refuses
        // while one waits and says how to settle it.
        if text.trim() == super::NEW {
            return self.new_conversation();
        }
        if self.action.is_some() || witness.starts_with("action:") {
            // A decision reads words: the choice left to Nika is the person's own line there.
            let reply = match reply {
                Reply::Delegated => Reply::Words(&back),
                other => other,
            };
            return self.action_answer(reply, witness);
        }
        if let work::Waiting::Consent { proposal } = &self.shown {
            let proposal = proposal.clone();
            return self.proposal_answer(&proposal, reply, witness);
        }
        let (id, key) = match &self.shown {
            work::Waiting::Question { id, key }
                if super::asked::witness(id, self.question_epoch) == witness =>
            {
                (id.clone(), key.clone())
            }
            work::Waiting::Questions { ids } => {
                let Some(runtime) = self.runtime.as_ref() else {
                    return self.not_shown(&back);
                };
                let snapshot = runtime.work();
                let Some(question) = super::asked::grouped_question(&snapshot, ids)
                    .filter(|q| super::asked::witness(&q.id, self.question_epoch) == witness)
                else {
                    return self.not_shown(&back);
                };
                (question.id.clone(), question.key.clone())
            }
            _ => return self.not_shown(&back),
        };
        let Some(runtime) = self.runtime.as_mut() else {
            return self.not_shown(&back);
        };
        let question = runtime.work().question;
        let label = question
            .as_ref()
            .filter(|q| q.key == key)
            .map_or(key.as_str(), |q| q.label.as_str());
        let before = runtime.pending_question_id();
        let outcome = match reply {
            Reply::Words(text) | Reply::Picked(text) => runtime.answer_question_for(&id, text),
            Reply::Delegated => runtime.delegate_question_for(&id),
        };
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
            beats.push(Beat::NotTaken(back));
        }
        Turn { beats, handoff }
    }

    fn proposal_answer(
        &mut self,
        proposal: &nika_session::ProposalId,
        reply: Reply<'_>,
        witness: &str,
    ) -> Turn {
        let text = reply.text();
        let back = reply.back(crate::lang::Voice {
            lang: self.lang(),
            ascii: self.ascii,
        });
        let Some(runtime) = self.runtime.as_mut() else {
            return self.not_shown(&back);
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
            return self.not_shown(&back);
        };
        let id = question.id.clone();
        // A cancel word on its own line that names none of the offers drops what the choice
        // belongs to, as its hint says: the proposal, discarded through the consent door's own
        // `no` (live gauntlet r08 B4: « annuler » and « cancel » were read as answers).
        if matches!(reply, Reply::Words(words) if cancels(words, &question.options)) {
            let outcome = runtime.consent_to(proposal, "no");
            let refused = matches!(outcome, TurnOutcome::Refusal(_));
            let (mut beats, handoff) = self.map(outcome);
            if refused {
                beats.push(Beat::NotTaken(text.to_owned()));
            }
            return Turn { beats, handoff };
        }
        let outcome = match reply {
            Reply::Words(text) | Reply::Picked(text) => {
                runtime.answer_proposal_choice_for(proposal, &id, text)
            }
            Reply::Delegated => runtime.delegate_proposal_choice_for(proposal, &id),
        };
        let after = runtime.work();
        let remains = after
            .open_choices
            .iter()
            .any(|q| q.id == id && q.state == work::AskedState::Open);
        // A later compilation can refuse after the choice was bound. Retain only the
        // Session's exact pre-admission refusal for this question, or a turn that failed with
        // the choice still open.
        let refused = after.answered.as_ref().is_some_and(|answered| {
            answered.question == id && matches!(answered.act, work::AnswerAct::Refused { .. })
        });
        let failed = matches!(outcome, TurnOutcome::Refusal(_));
        let (mut beats, handoff) = self.map(outcome);
        if kept_back(remains, refused, failed) {
            beats.push(Beat::NotTaken(back));
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

/// Whether the words sent to a proposal's choice come back to the draft: when the Session
/// refused them before the author read them (`refused`), or when the turn failed with the
/// choice still open (`remains`, `failed`). Words the author read and answered are said, even
/// when the choice stays open beside Nika's reply (live gauntlet r08 B4: « not taken · answer
/// text restored for editing » after the author had answered them).
const fn kept_back(remains: bool, refused: bool, failed: bool) -> bool {
    refused || (remains && failed)
}

/// Whether `words` drop a proposal's choice: a cancel word on its own line (`cancel`,
/// `annuler`, [`nika_session::authoring::is_cancel`]) that is none of its `offers`' keys or
/// labels, which answer the choice instead.
fn cancels(words: &str, offers: &[work::Offer]) -> bool {
    let said = words.trim();
    nika_session::authoring::is_cancel(said)
        && !(offers.iter()).any(|offer| {
            offer.key.trim().eq_ignore_ascii_case(said)
                || offer.label.trim().eq_ignore_ascii_case(said)
        })
}

mod reporting;

#[cfg(test)]
mod tests;
