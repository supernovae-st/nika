// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Recovery: a turn the session could not finish (the provider did not
//! answer, the authoring budget ran out, the seat refused) is told as a
//! card a human can act on — what happened, what is still kept, what did
//! NOT happen, what they can do now — and « what happened? » repeats that
//! card from memory: never through another call, never a retry the human
//! did not ask for. The goal is never erased by a failure.

use std::fmt::Write as _;

use super::{SessionRuntime, TurnOutcome};
use crate::outcome::{Refusal, RefusalClass};

/// How many decisions the card lists before « … ».
const KEPT_DECISIONS: usize = 3;
/// The longest request line the card quotes before an ellipsis.
const QUOTE_CHARS: usize = 140;

impl SessionRuntime {
    /// The recovery card under the caller's own `headline` (« I couldn't
    /// use the authoring model for this part » · « I couldn't finish a
    /// workflow I trust within the authoring budget ») with `reason`; a
    /// `class` makes it a refusal, `None` a fact (a budget that ran out
    /// refuses nothing). The headline is said once: the template adds the
    /// reason, what is kept, what did not happen and the ways on. The card
    /// is kept as the last recovery so « what happened? » repeats it
    /// without a call.
    pub(super) fn recovery(
        &mut self,
        class: Option<RefusalClass>,
        headline: &str,
        reason: &str,
    ) -> TurnOutcome {
        let mut text = format!("{headline} — {reason}");
        let kept = self.kept_lines();
        if !kept.is_empty() {
            text.push_str("\n  I still have:");
            for line in kept {
                let _ = write!(text, "\n    ✓ {line}");
            }
        }
        text.push_str("\n  Nothing was written and nothing was sent elsewhere.");
        text.push_str(
            "\n  To continue:\n    · say it again to try once more with the same intelligence\n    · `/intelligence` to choose another one\n    · keep going without it: the facts still answer, and work Nika reads on its own compiles\n  « what happened? » repeats this card",
        );
        self.last_recovery = Some(text.clone());
        match class {
            Some(class) => TurnOutcome::Refusal(Refusal::new(class, text)),
            None => TurnOutcome::Facts(text),
        }
    }

    /// What survives a failed turn, in the human's own words: the request,
    /// the answers already given, the last decisions.
    fn kept_lines(&self) -> Vec<String> {
        let mut kept = Vec::new();
        if let Some(goal) = &self.intent.goal {
            kept.push(format!("your request: « {} »", quote(goal)));
        }
        if let Some(round) = &self.authoring {
            for (key, value) in &round.answers {
                kept.push(format!("your answer to `{key}`: {value}"));
            }
        }
        for decision in self.intent.decisions.iter().rev().take(KEPT_DECISIONS) {
            kept.push(decision.clone());
        }
        kept
    }

    /// « what happened? » — the last recovery card, from memory; `None`
    /// when nothing failed in this session.
    pub(super) fn last_recovery(&self) -> Option<TurnOutcome> {
        self.last_recovery.clone().map(TurnOutcome::Facts)
    }
}

/// A request quoted on one line, cut with an ellipsis past a bound.
fn quote(text: &str) -> String {
    let one_line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= QUOTE_CHARS {
        return one_line;
    }
    let cut: String = one_line.chars().take(QUOTE_CHARS).collect();
    format!("{cut}…")
}
