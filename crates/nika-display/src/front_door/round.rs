// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Kept-round prose over the owner's validated facts; no replay or authority.
/// A kept round in words, evidence that names no host's protocol (a host adds its own way on).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct RoundWords {
    /// The exact request and its settled answers as projected by the round owner.
    pub summary: String,
    /// Why the compiler asked the question that waited, when one waited.
    pub asked: Option<String>,
    /// Why it cannot be continued, as decided by the round owner; `None` when it can.
    pub blocked: Option<String>,
}

impl RoundWords {
    /// Build a passive projection from the round owner's three settled facts.
    #[must_use]
    pub fn new(summary: String, asked: Option<String>, blocked: Option<String>) -> Self {
        Self {
            summary,
            asked,
            blocked,
        }
    }
}
/// Restore arms the kept preparation, and itself makes no model call.
pub const ROUND_HINT: &str = "\n  → type /restore to continue it under the project as it is now · no AI asked by /restore · a kept question returns, or an unjudged candidate waits for `continue` · Save stays separate";
/// Help for explicitly restoring kept preparation evidence.
pub const ROUND_HELP: &str = "/restore            restore kept preparation without asking AI · its question returns, or `continue` retries its judge · Save stays separate";
/// Restore-notice line.
#[must_use]
pub fn line(words: Result<RoundWords, &str>) -> String {
    match words {
        Ok(words) => match words.blocked {
            None => format!("restored round: {}{ROUND_HINT}", words.summary),
            Some(why) => format!(
                "restored round: {} · it cannot be continued ({why}); it stays kept as evidence · state the request again instead",
                words.summary
            ),
        },
        Err(why) => format!(
            "a round kept by another engine version cannot be read here ({why}); it stays kept unchanged and grants nothing"
        ),
    }
}
/// Read-only meaning before restore.
#[must_use]
pub fn meaning(words: Result<RoundWords, &str>) -> String {
    match words {
        Ok(words) => format!(
            "the round kept from your last session, not continued yet: {}\n  its clause-by-clause reading is shown once /restore continues it · nothing was asked or changed",
            words.summary
        ),
        Err(why) => format!(
            "a round is kept from your last session but this engine cannot read it ({why}) · it grants nothing"
        ),
    }
}
/// Read-only explanation before restore.
#[must_use]
pub fn why(words: Result<RoundWords, &str>) -> String {
    match words {
        Ok(words) => format!(
            "nothing waits now · the round kept from your last session has not continued: {}{}\n  {} · nothing was asked or changed",
            words.summary,
            words
                .asked
                .map_or_else(String::new, |why| format!("\n  why it was asked: {why}")),
            way(words.blocked, "/restore continues it (no AI asked)")
        ),
        Err(why) => format!(
            "nothing waits now · a round is kept from your last session but this engine cannot read it ({why}) · it grants nothing"
        ),
    }
}
/// Read-only status before restore.
#[must_use]
pub fn status(words: Result<RoundWords, &str>) -> String {
    match words {
        Ok(words) => format!(
            "\n  kept round (not continued): {} · {}",
            words.summary,
            way(words.blocked, "/restore continues it")
        ),
        Err(_) => "\n  kept round: unreadable by this engine · kept unchanged".to_owned(),
    }
}
fn way(blocked: Option<String>, restore: &str) -> String {
    blocked.map_or_else(
        || restore.to_owned(),
        |why| format!("it cannot be continued ({why})"),
    )
}

/// A request in words: « request », or « typed » as you typed it · rebuilt as « request » when a
/// restatement rebuilt the sentence the human typed. A trailing line break is presentation and is
/// never shown inside « »; the kept bytes stay as they were.
#[must_use]
pub fn as_typed(typed: Option<&str>, request: &str) -> String {
    let request = request.trim_end();
    match typed
        .map(str::trim_end)
        .filter(|typed| !typed.is_empty() && *typed != request)
    {
        Some(typed) => format!("« {typed} » as you typed it · rebuilt as « {request} »"),
        None => format!("« {request} »"),
    }
}
