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

/// The words an answered `intent.clarification` gives, which replace the request (the
/// compiler's own law): its answer's text, when that is a string that is not blank.
#[must_use]
pub fn clarified(answers: &std::collections::BTreeMap<String, String>) -> Option<String> {
    answers
        .get(CLARIFICATION_KEY)
        .and_then(|literal| serde_json::from_str::<Value>(literal).ok())
        .and_then(|value| value.as_str().map(str::to_owned))
        .filter(|text| !text.trim().is_empty())
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

/// The compiler's reasons a human can act on: its machine sentences (the
/// plan's own vocabulary, an unmapped part with nothing after the colon)
/// dropped, duplicates folded, the rest verbatim.
#[must_use]
pub fn human_reasons(reasons: Vec<String>) -> Vec<String> {
    let mut kept: Vec<String> = Vec::new();
    for reason in reasons {
        let r = reason.trim();
        let machine = r.contains("semantic plan") || r.ends_with(": .") || r.ends_with(':');
        if machine || r.is_empty() {
            continue;
        }
        let said = human_reason(r);
        if kept.contains(&said) {
            continue;
        }
        kept.push(said);
    }
    kept
}

/// One compiler reason in the human's words — the compiler's fidelity
/// grammar is a closed set (« Candidate N is not feasible: … », « dropped
/// the recognized operation `x` (evidence) », « the path `p` is no longer
/// carried … », « the literal `v` is not in the request »); any other line
/// is kept as the compiler said it.
fn human_reason(raw: &str) -> String {
    let r = raw.trim().trim_end_matches('.');
    // A cut answer is the seat's output limit, an internal cause: its command-line advice
    // (`--authoring-max-tokens`) is no gesture a conversation has, and the request is not at
    // fault.
    if r.contains("--authoring-max-tokens") {
        let tokens: String = r
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(char::is_ascii_digit)
            .collect();
        let limit = if tokens.is_empty() {
            "its output limit".to_owned()
        } else {
            format!("its {tokens}-token output limit")
        };
        return format!(
            "the model's answer was cut at {limit} before it was complete — an internal limit of this attempt, not a problem with your request"
        );
    }
    let r = match r.find("is not feasible: ") {
        Some(at) if r.starts_with("Candidate ") => &r[at + "is not feasible: ".len()..],
        _ => r,
    };
    let quoted = |s: &str| -> Option<(String, String)> {
        let start = s.find('`')?;
        let end = s[start + 1..].find('`')? + start + 1;
        Some((s[start + 1..end].to_owned(), s[end + 1..].to_owned()))
    };
    if let Some(rest) = r.strip_prefix("dropped the recognized operation ")
        && let Some((op, tail)) = quoted(rest)
    {
        let evidence = tail
            .trim()
            .trim_start_matches('(')
            .trim_end_matches(')')
            .trim_end_matches(',')
            .trim();
        return if evidence.is_empty() {
            format!("the draft lost the « {op} » step")
        } else {
            format!("the draft lost « {evidence} » (the {op} step)")
        };
    }
    if let Some(rest) = r.strip_prefix("the path ")
        && let Some((path, tail)) = quoted(rest)
        && tail.contains("no longer carried")
    {
        return format!("the draft dropped « {path} »: nothing reads or writes it any more");
    }
    if let Some(rest) = r.strip_prefix("the literal ")
        && let Some((value, tail)) = quoted(rest)
        && tail.contains("not in the request")
    {
        return format!("the draft invented a value (« {value} ») your request never gave");
    }
    r.to_owned()
}

/// How many clauses the compiler's ledger holds for this reading —
/// « understood N requirements » — `None` when the outcome carries no ledger.
#[must_use]
pub fn clauses_understood(out: &CompileOutcome) -> Option<usize> {
    let ledger = out
        .provenance
        .decision
        .as_ref()?
        .get("ledger")?
        .as_array()?;
    (!ledger.is_empty()).then_some(ledger.len())
}

/// A question that asks the human for code (a jq or CEL expression, a
/// `const.*_expression` value): a product defect when it reaches them.
#[must_use]
pub fn asks_for_syntax(question: &CompileQuestion) -> bool {
    let label = question.label.to_ascii_lowercase();
    question.key.ends_with("_expression")
        || label.contains("jq expression")
        || label.contains(" jq ")
        || label.contains("cel expression")
}

/// The clause the compiler quotes in its question (between backticks),
/// as the request carries it.
#[must_use]
pub fn clause_of(label: &str) -> Option<String> {
    let start = label.find('`')? + 1;
    let end = start + label[start..].find('`')?;
    let clause = label[start..end].trim();
    (!clause.is_empty()).then(|| clause.to_owned())
}

#[cfg(test)]
mod tests;
