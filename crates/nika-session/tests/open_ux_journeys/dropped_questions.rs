// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A cancel word at a question the author asked (gauntlet wave 3): « annuler » drops the
//! question with no model call; the author reads that the person dropped it, and asking it again
//! is refused.

use serde_json::{Value, json};

use super::{Act, REQUEST, Scenario};
use crate::peer::{Script, call, say};

/// The cancel word the person types at the question.
pub(crate) const DROP: &str = "annuler";
/// The person's next line, once the question is dropped.
pub(crate) const SIMPLE: &str = "fais au plus simple";

/// The author's question: the summary's format, typed freely.
fn format_question() -> Value {
    json!({"questions": [{"key": "format", "question": "Quel format veux-tu pour le résumé ?",
        "free_text": true, "options": []}]})
}

/// Asked, dropped with « annuler », then the person's next line: the author asks the same
/// question again, is refused, and goes on.
pub(super) fn question_dropped() -> Scenario {
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts: vec![Act::Turn(REQUEST), Act::Submit(DROP), Act::Turn(SIMPLE)],
        script: Script {
            agent: vec![
                call("ask", format_question()),
                call("ask", format_question()),
                say("D'accord, je continue sans cette précision."),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

/// The question offers « Annuler » itself: that word picks the offer, and the author reads it.
pub(super) fn cancel_offered() -> Scenario {
    let options = json!({"questions": [{"key": "suite", "question": "On continue ?",
        "free_text": true, "options": [
            {"key": "garder", "label": "Garder la demande"},
            {"key": "annuler", "label": "Annuler"}]}]});
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts: vec![Act::Turn(REQUEST), Act::Submit(DROP)],
        script: Script {
            agent: vec![call("ask", options), say("D'accord, j'annule la demande.")],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}
