// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Passive recovery prose over the host's selected facts. No judgments or authority live here.
use std::fmt::Write as _;
/// A host-recorded turn ended without a completed reply; this notice replays no operation.
pub const INTERRUPTED_TURN: &str = "[Interrupted turn: no completed reply was recorded.]";
/// The longest request line the card quotes before an ellipsis.
const QUOTE_CHARS: usize = 140;
/// A kept candidate is not a reviewable workflow and asks no invented question.
pub const JUDGMENT_STATUS: &str = "Not ready · candidate kept · judge must retry";
/// Explicit next step; reopening itself never asks a model.
pub const JUDGMENT_KEPT: &str = "The candidate could not be judged. Its exact bytes and your request are kept; nothing is ready to save or run. Say `continue` to ask the judge again, or describe a correction to prepare a new candidate.";
/// Render a failure from host facts without spending, retrying or selecting a connection.
#[must_use]
pub fn card(
    headline: &str,
    reason: &str,
    seat: Option<&str>,
    kept: &[String],
    not_done: &str,
) -> String {
    let mut text = format!("{headline} — {reason}");
    if let Some(seat) = seat {
        let _ = write!(text, "\n  {seat}");
    }
    if !kept.is_empty() {
        text.push_str("\n  I still have:");
        for line in kept {
            let _ = write!(text, "\n    ✓ {line}");
        }
    }
    let _ = write!(text, "\n  {not_done}");
    text.push_str(
            "\n  To continue:\n    · say it again to try once more with the same intelligence\n    · `/intelligence` to choose another one\n    · keep going without it: the facts still answer, and work Nika reads on its own compiles\n  « what happened? » repeats this card",
        );
    text
}

/// A request quoted on one line, cut with an ellipsis past a bound.
#[must_use]
pub fn quote(text: &str) -> String {
    let one_line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= QUOTE_CHARS {
        return one_line;
    }
    let cut: String = one_line.chars().take(QUOTE_CHARS).collect();
    format!("{cut}…")
}

/// A historical monetary decision, not the authority of the current preparation mode.
/// The record keeps its exact bytes; this projection never infers a present block from it.
const RECONFIRM_SHOWN: &str = "earlier API costs and any unknown charges are kept · this is a historical record, not a new spending decision · preparation uses the current mode; a saved workflow keeps its own Run limits";

/// A decision as a human reads it.
#[must_use]
pub fn decision(decision: &str, legacy_reconfirm: bool) -> String {
    if legacy_reconfirm {
        RECONFIRM_SHOWN.to_owned()
    } else {
        decision.to_owned()
    }
}

/// Explain an unfinished preparation from the compiler owner's classification and reasons.
/// This presentation neither retries the request nor changes its meaning or authority.
#[must_use]
pub fn cannot_express(authoring_failed: bool, stopped: &[String]) -> String {
    let mut text = if authoring_failed {
        "Nika could not finish building this automation — an authoring step failed on Nika's side (below), not because of how you asked; nothing was written.".to_owned()
    } else {
        "Nika cannot express this automation yet — nothing was written.".to_owned()
    };
    if !stopped.is_empty() {
        text.push_str("\n  what stopped it:");
        for reason in stopped {
            text.push_str("\n    · ");
            text.push_str(reason);
        }
    }
    // An internal failure never asks the human to rewrite or split what they asked: the
    // request stays the goal, and the same words make another attempt.
    text.push_str(if authoring_failed {
        "\n  your request is kept as the goal: send it again unchanged for another attempt, or `/intelligence` for another model · `/meaning` shows what was understood"
    } else {
        "\n  what helps: say the outcome in one sentence (what to read · what to produce · where it goes), or split the work in two requests · `/meaning` shows what was understood"
    });
    text
}

/// A newly reported conversation uncertainty or expired authority, selected by the host.
/// Historical uncertainty alone does not append a new turn; this prose settles no effect.
#[must_use]
pub const fn conversation_note(
    new_uncertainty: bool,
    expired_authority: bool,
) -> Option<&'static str> {
    if new_uncertainty {
        Some(
            "[An earlier operation has an uncertain result or API charge. Nothing was replayed; inspect effects and receipts before proposing a retry.]",
        )
    } else if expired_authority {
        Some("[The earlier proposal or gate expired. Fresh validation and consent are required.]")
    } else {
        None
    }
}
