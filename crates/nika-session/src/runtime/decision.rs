// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! What a waiting human answers: the whole-line consent words, the one
//! grammar a fresh spending decision reads (the Session's one-time cost
//! choice and a Run cost decision), the slash commands every waiting state
//! answers from the session's own facts, and the typed « not run » of a
//! declined Run review. None of it grants anything beyond the one decision
//! shown: a changed candidate or a next Run asks afresh.

use super::history::Operation;
use super::{SessionRuntime, TurnOutcome};

/// The whole-line tokens a confirm gate accepts — a protocol, not a
/// reading of language (a longer line is routed, never reduced to one).
pub(super) fn is_gate_token(line: &str) -> bool {
    matches!(
        line.trim().to_lowercase().as_str(),
        "yes" | "y" | "true" | "ok" | "oui" | "approve" | "no" | "n" | "false" | "non" | "deny"
    )
}

/// The refusal line: `no` in the few words a human types for it.
pub(super) fn is_no(answer: &str) -> bool {
    matches!(
        answer.trim().to_lowercase().as_str(),
        "no" | "n" | "non" | "discard" | "cancel" | "drop" | "nope" | "stop"
    )
}

/// The consent line, and nothing else: `yes` in the few words a human
/// types for it.
pub(super) fn is_yes(answer: &str) -> bool {
    matches!(
        answer.trim().to_lowercase().as_str(),
        "yes" | "y" | "apply" | "ok" | "oui" | "go" | "do it"
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
pub(super) fn local_command_of(line: &str) -> Option<&'static str> {
    match line.trim() {
        "/help" => Some("/help"),
        "/status" => Some("/status"),
        "/details" => Some("/details"),
        "/why" => Some("/why"),
        "/proof" => Some("/proof"),
        "/restore" => Some("/restore"),
        _ => None,
    }
}

impl SessionRuntime {
    /// A slash command answered from the session's own facts while something
    /// waits (a proposal, a gate, a choice): the waiting state is untouched.
    pub(super) fn answer_locally(&mut self, command: &str) -> TurnOutcome {
        match command {
            "/help" => TurnOutcome::Help(self.help_card()),
            "/status" => TurnOutcome::Facts(self.status()),
            "/details" => TurnOutcome::Facts(self.details()),
            "/proof" => self.proof_unrecorded(),
            "/restore" => self.restore_while_waiting(),
            _ => self.explain_pending(),
        }
    }

    /// A Run whose fresh cost decision was declined, or interrupted before
    /// any answer: nothing was sent and nothing ran, so there is no exit to
    /// observe — the last run and the status stay as they were. Recorded as
    /// an observation and typed as a fact, never as an exit code.
    pub fn observe_declined_run(&mut self) -> TurnOutcome {
        self.recorded(Operation::Observation, "(run declined)", |s| {
            let line = "not run · the Run cost decision was declined · nothing sent, nothing written · « run it » asks afresh";
            s.remember("(run declined)", line);
            TurnOutcome::Facts(line.to_owned())
        })
    }
}
