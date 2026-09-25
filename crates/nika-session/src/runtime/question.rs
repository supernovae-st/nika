// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The identity of the authoring question the next line answers — ADR-133's
//! identity doors, for questions. A question is ONE question as ONE session
//! asked it: its words, the revision of the request it belongs to, the
//! intelligence and the seat that read its answer, the ordinal of the asking,
//! and the session itself. A changed request, another intelligence or a
//! restarted session asks another question, even in the same words and for
//! the same key. An answer naming a question that no longer waits is refused
//! before anything reads it: no classifier, no reading, no compiler, no
//! record, no effect. The key stays the compiler's semantic hole; the
//! identity lives in memory only and grants nothing — no consent, no run.

use std::sync::Arc;

use super::{SessionRuntime, TurnOutcome};
use crate::authoring::AuthoringRound;
use crate::change::Witness;
use crate::outcome::{Incarnation, QuestionId, Refusal, RefusalClass};

/// How many answered or dropped questions a session remembers, to say so
/// (`already_consumed`); an older one reads as no longer waiting.
const REMEMBERED: usize = 64;

/// This session's questions: the session that asks them, how many it
/// asked, the ones it answered or dropped. Never persisted.
#[derive(Debug, Default)]
pub(super) struct Identities {
    asker: Arc<Incarnation>,
    asked: u64,
    closed: Vec<QuestionId>,
}

impl Identities {
    /// A new question waits: it takes the next ordinal.
    pub(super) fn ask(&mut self) {
        self.asked = self.asked.saturating_add(1);
    }

    /// The question was answered or dropped: it answers nothing again.
    pub(super) fn close(&mut self, id: Option<QuestionId>) {
        self.closed.extend(id);
        if self.closed.len() > REMEMBERED {
            self.closed.remove(0);
        }
    }

    fn is_closed(&self, id: &QuestionId) -> bool {
        self.closed.contains(id)
    }
}

impl SessionRuntime {
    /// The identity of the authoring question the next line answers, when
    /// one waits (beside [`Self::pending_question`]): the question, its
    /// request revision, the intelligence and the seat that read its answer,
    /// in this session. It stays the same while the question waits — through
    /// an aside, a refusal, a reply that bound nothing — and a new question, a
    /// revised request, another intelligence or another session is another
    /// identity.
    #[must_use]
    pub fn pending_question_id(&self) -> Option<QuestionId> {
        self.question_id_of(self.authoring.as_ref()?)
    }

    /// The identity `round`'s current question has in this session now.
    pub(super) fn question_id_of(&self, round: &AuthoringRound) -> Option<QuestionId> {
        let question = round.current()?;
        let asked = format!(
            "nika-session question 1\n{}\n{question:?}\n{round:?}\n{:?}\n{:?}\n{:?}",
            self.questions.asked, self.seat, self.intelligence, self.authoring_context
        );
        Some(QuestionId::new(
            Witness::of(asked.as_bytes()).0,
            &self.questions.asker,
        ))
    }

    /// An answer that names the question it answers (ADR-133), for a host
    /// that is not at the keyboard: refused as stale when another question —
    /// another revision of the request, another intelligence, another
    /// session — waits now, as already consumed when this session answered
    /// or dropped that question, as the wrong state when none waits or
    /// another prompt owns the next line. A refusal is decided before the
    /// line reaches anything — classifier, reading, compiler, record, effect
    /// — and what waits keeps waiting. The question that waits takes the
    /// line exactly as [`Self::turn`] gives it at the keyboard. Naming a
    /// question grants no consent and no run.
    pub fn answer_question_for(&mut self, id: &QuestionId, line: &str) -> TurnOutcome {
        if self.waiting_cost_choice() {
            return refused(
                RefusalClass::StaleRevision,
                format!("a cost review waits — the answer to the question {id} cannot answer it"),
            );
        }
        let elsewhere = !id.asked_by(&self.questions.asker);
        match self.pending_question_id() {
            Some(waiting) if waiting == *id => {
                if self.pending_choice || self.pending.is_some() || self.pending_gate.is_some() {
                    return refused(
                        RefusalClass::WrongState,
                        format!(
                            "another prompt owns the next line (the intelligence choice, a proposal or a paused run) — answer it first; the question {id} keeps waiting"
                        ),
                    );
                }
                self.turn(line)
            }
            Some(waiting) if elsewhere => refused(
                RefusalClass::StaleRevision,
                format!(
                    "the question {id} was asked by another session — a restarted session never takes an earlier answer · the question waiting now is {waiting}"
                ),
            ),
            Some(waiting) => refused(
                RefusalClass::StaleRevision,
                format!(
                    "the question {id} is not the one waiting ({waiting}) — the request, its revision or the intelligence changed · read the question again before answering"
                ),
            ),
            None if self.questions.is_closed(id) => refused(
                RefusalClass::AlreadyConsumed,
                format!("the question {id} was already answered or dropped — it is answered once"),
            ),
            None if elsewhere => refused(
                RefusalClass::WrongState,
                format!(
                    "the question {id} was asked by another session and no question waits here — describe the work again"
                ),
            ),
            None => refused(
                RefusalClass::WrongState,
                format!(
                    "no authoring question waits — the question {id} is neither waiting nor answered in this session"
                ),
            ),
        }
    }
}

fn refused(class: RefusalClass, text: String) -> TurnOutcome {
    TurnOutcome::Refusal(Refusal::new(class, text))
}
