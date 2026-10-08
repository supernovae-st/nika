// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a waiting human answers, as words: the whole-line consent words, the one grammar a fresh
//! spending decision reads, and the slash commands every waiting state answers from the Session's
//! own facts (moved whole out of nika-session's `runtime/decision.rs`). None of it grants anything
//! beyond the one decision shown.

/// The whole-line tokens a confirm gate accepts — a protocol, not a
/// reading of language (a longer line is routed, never reduced to one).
#[must_use]
pub fn is_gate_token(line: &str) -> bool {
    matches!(
        line.trim().to_lowercase().as_str(),
        "yes" | "y" | "true" | "ok" | "oui" | "approve" | "no" | "n" | "false" | "non" | "deny"
    )
}

/// The refusal line: `no` in the few words a human types for it.
#[must_use]
pub fn is_no(answer: &str) -> bool {
    matches!(
        answer.trim().to_lowercase().as_str(),
        "no" | "n" | "non" | "discard" | "cancel" | "drop" | "nope" | "stop"
    )
}

/// The consent line, and nothing else: `yes` in the few words a human
/// types for it.
#[must_use]
pub fn is_yes(answer: &str) -> bool {
    matches!(
        answer.trim().to_lowercase().as_str(),
        "yes" | "y" | "apply" | "ok" | "oui" | "go" | "do it"
    )
}

/// The combined consent line, and nothing else: save the proposal shown and run what it saves
/// once (`save & run`, the word a host's Save & run sends), the whole line.
#[must_use]
pub fn is_save_and_run(answer: &str) -> bool {
    matches!(
        answer.trim().to_lowercase().as_str(),
        "save & run" | "save and run" | "enregistre et lance"
    )
}

/// One answer to a fresh spending decision: the Session's one-time
/// unknown-cost choice and a Run cost decision read the same grammar
/// (English and French). Approval is the whole line and nothing else; a
/// refusal may lead a longer line (« non, finalement pas maintenant »); any
/// other line is asked again: it never approves, never spends and never
/// cancels. New answers may be added: a door that does not know one asks
/// again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecisionAnswer {
    /// `yes` · `oui` · the Session's consent words, the whole line.
    Approve,
    /// `no` · `non` · `cancel` …, the whole line or its first word.
    Decline,
    /// `details` · `/details` · `/why` · `why?`: the same review's evidence.
    Details,
    /// Anything else: the decision is asked again, nothing is sent.
    Unknown,
}

/// Reads one line as the answer to a fresh spending decision.
#[must_use]
pub fn decision_answer(line: &str) -> DecisionAnswer {
    let whole = line.trim().trim_end_matches(['.', '!', ' ']).to_lowercase();
    if matches!(
        whole.as_str(),
        "details" | "/details" | "/why" | "why" | "why?"
    ) {
        return DecisionAnswer::Details;
    }
    if is_yes(&whole) {
        return DecisionAnswer::Approve;
    }
    let first = whole
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | ':'))
        .next()
        .unwrap_or("");
    if is_no(&whole) || is_no(first) {
        return DecisionAnswer::Decline;
    }
    DecisionAnswer::Unknown
}

/// The slash commands every waiting state still answers from the session's
/// own facts: never the model, never an answer to what waits.
#[must_use]
pub fn local_command_of(line: &str) -> Option<&'static str> {
    match line.trim() {
        "/help" => Some("/help"),
        "/status" => Some("/status"),
        "/details" => Some("/details"),
        "/why" => Some("/why"),
        "/meaning" => Some("/meaning"),
        "/proof" => Some("/proof"),
        "/restore" => Some("/restore"),
        _ => None,
    }
}
