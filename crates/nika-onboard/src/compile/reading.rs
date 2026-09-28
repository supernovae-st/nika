// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What one compile outcome means for a conversation, and the literal a human line is at one of
//! its questions (descended from `nika-session` on 2026-09-28, C7; `nika_session::authoring`
//! re-exports all of it). [`Reading`](crate::compile::reading::Reading) is read from the compiler's typed fields — its status,
//! candidate, questions, route and plan — and, for a provider failure, from the provider
//! diagnostic's own words (a timeout is recognized by its text): it never parses the compiler's
//! prose back into state. Pure: an outcome or a line in, a reading out — nothing here calls,
//! reads or decides for a host, and a host's own protocol words stay with the host.

use serde_json::Value;

use super::{CompileOutcome, CompileQuestion, CompileStatus, DiagnosticKind, QuestionType};

/// The compiler's question for a whole replacement request (its own key).
pub const CLARIFICATION_KEY: &str = "intent.clarification";

/// The human's line as the JSON literal the question's shape takes: a
/// `Literal` question takes the line verbatim when it already is JSON (`5` ·
/// `true` · `["a"]`), else as a string (`./notes` is a path, not a parse
/// error); a `Text` question and a `Choice` take the line verbatim when it
/// already is a JSON string (a value in quotes — `"exports/rapport final.txt"`
/// — is that value, never its quotes; `"montant"` is the shape the compiler
/// asks of a choice), else as one string.
#[must_use]
pub fn literal_for(question: &CompileQuestion, line: &str) -> String {
    let line = line.trim();
    let already = match question.answer_type {
        QuestionType::Literal => serde_json::from_str::<Value>(line).is_ok(),
        QuestionType::Choice | QuestionType::Text => {
            matches!(serde_json::from_str::<Value>(line), Ok(Value::String(_)))
        }
        _ => false,
    };
    if already {
        line.to_owned()
    } else {
        Value::String(line.to_owned()).to_string()
    }
}

/// What one compile outcome means for the conversation — a closed reading of the compiler's
/// typed fields (a provider failure's timeout recognized by its diagnostic's words), never of
/// its prose.
#[derive(Debug)]
#[non_exhaustive]
pub enum Reading {
    /// A candidate exists and every mandatory question is answered.
    Ready(CompileOutcome),
    /// Mandatory questions remain: the next line answers the first.
    Questions(CompileOutcome),
    /// Work was read but not settled under this seat's policy (the
    /// compiler says so): a wider policy may settle it, or the human
    /// rephrases. How the compiler tried is its own business.
    Unsettled(CompileOutcome),
    /// Nothing recognizable as work: no route, no plan, no question.
    NotWork(CompileOutcome),
    /// The authoring budget (time) ran out before a trusted candidate;
    /// not a verdict on the request.
    BudgetExhausted(CompileOutcome),
    /// The authorized authoring call failed at the provider; nothing was
    /// substituted.
    ProviderFailed(CompileOutcome),
    /// The compiler refused the request under its own policy.
    Refused(CompileOutcome),
}

impl Reading {
    /// Classify an outcome by its typed fields.
    #[must_use]
    pub fn of(out: CompileOutcome) -> Self {
        if out.status == CompileStatus::Refused {
            return Self::Refused(out);
        }
        if out.status == CompileStatus::Ready && out.candidate.is_some() {
            return Self::Ready(out);
        }
        // `intent.clarification` is the compiler asking for a whole new
        // request: not a hole a line fills but a reading a seat may settle —
        // or the human rephrases. Every other mandatory key is a hole.
        if out
            .questions
            .iter()
            .any(|q| q.mandatory && q.key != CLARIFICATION_KEY)
        {
            return Self::Questions(out);
        }
        let provider_findings: Vec<&str> = out
            .diagnostics
            .iter()
            .filter(|d| d.target == "authoring_provider")
            .map(|d| d.message.as_str())
            .collect();
        if provider_findings.iter().any(|m| m.contains("timed out")) {
            return Self::BudgetExhausted(out);
        }
        if !provider_findings.is_empty() {
            return Self::ProviderFailed(out);
        }
        let routed = out
            .provenance
            .decision
            .as_ref()
            .and_then(|d| d.get("route"))
            .is_some();
        if routed || out.provenance.plan.is_some() {
            return Self::Unsettled(out);
        }
        Self::NotWork(out)
    }

    /// The outcome behind the reading.
    #[must_use]
    pub fn outcome(&self) -> &CompileOutcome {
        match self {
            Self::Ready(o)
            | Self::Questions(o)
            | Self::Unsettled(o)
            | Self::NotWork(o)
            | Self::BudgetExhausted(o)
            | Self::ProviderFailed(o)
            | Self::Refused(o) => o,
        }
    }
}

/// The compiler's own reasons in an outcome (unknown · missed · refused),
/// for the human — never parsed back into state.
#[must_use]
pub fn reasons(out: &CompileOutcome) -> Vec<String> {
    out.diagnostics
        .iter()
        .filter(|d| {
            matches!(
                d.kind,
                DiagnosticKind::Unknown | DiagnosticKind::Missed | DiagnosticKind::Refused
            )
        })
        .map(|d| d.message.clone())
        .collect()
}

#[cfg(test)]
mod tests;
