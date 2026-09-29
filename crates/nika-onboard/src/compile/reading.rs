// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What one compile outcome means for a conversation, and the literal a human line is at one of
//! its questions (descended from `nika-session` on 2026-09-28, C7; `nika_session::authoring`
//! re-exports all of it). [`Reading`](crate::compile::reading::Reading) is read from the compiler's typed fields — its status,
//! candidate, questions, route and plan — and, for a provider failure, from the provider
//! diagnostic's own words (a timeout is recognized by its text): it never parses the compiler's
//! prose back into state. Pure: an outcome or a line in, a reading out — nothing here calls,
//! reads or decides for a host, and a host's own protocol words stay with the host.

use std::fmt::Write as _;

use serde_json::Value;

use super::{
    AuthoringReceipt, CompileOutcome, CompileQuestion, CompileStatus, DiagnosticKind, QuestionType,
};

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

/// How many clauses the compiler's ledger records for this reading (« recorded N
/// requirements »): a count, never a verification; `None` when it carries no ledger.
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

/// A question as a human reads it, before a host's own protocol words: the compiler's label, why it
/// cannot invent the value, and what it could not settle.
#[must_use]
pub fn question_words(question: &CompileQuestion, reasons: &[String]) -> String {
    let mut text = question.label.clone();
    if !question.why.is_empty() {
        text.push_str("\n  (");
        text.push_str(&question.why);
        text.push(')');
    }
    if !reasons.is_empty() {
        text.push_str("\n  what I could not settle:");
        for reason in reasons {
            text.push_str("\n    · ");
            text.push_str(reason);
        }
    }
    text
}

/// The question in words for a rule the compiler could only ask as code ([`asks_for_syntax`]):
/// the clause, asked as a colleague would say it — never code.
#[must_use]
pub fn syntax_question(clause: &str) -> String {
    format!(
        "One thing I need from you, in words: how to do « {clause} ». Say it as you would to a colleague — what to keep, what to compute, over which column (e.g. « the total of the amount column » · « the rows whose status is paid »); your words take the place of « {clause} » in your request and Nika reads it again. No code is needed."
    )
}

/// An incomplete a human can act on: what the reader could not settle, in a human's words, and the
/// next safe step (`why` when a host names its own) — never a substitute workflow.
#[must_use]
pub fn incomplete_words(out: &CompileOutcome, why: Option<&str>) -> String {
    let mut text = "I read this as work but cannot build it yet:".to_owned();
    let reasons = human_reasons(reasons(out));
    if reasons.is_empty() {
        text.push_str("\n  · the request names no operation I can read");
    }
    for reason in reasons {
        text.push_str("\n  · ");
        text.push_str(&reason);
    }
    text.push_str("\n  ");
    text.push_str(why.unwrap_or(
        "say what to read, what to produce and where to write it, e.g. « read ./docs, draft a digest and write it to ./digest.md »",
    ));
    text
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

/// What one call's record shows of its fate (B19): answered only when the provider's response
/// is recorded (`stop_reason`), refused before sending, sent without an answer (a provider error
/// or a timeout), or unobserved (no result, or one this reader does not know).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fate {
    Answered,
    Refused,
    Unsure,
    Unobserved,
}

fn fate(call: &Value) -> Fate {
    let result = &call["result"];
    match (
        result["failure_kind"].as_str(),
        result["stop_reason"].is_string(),
    ) {
        (Some("admission_refused"), _) => Fate::Refused,
        (Some("provider_error" | "timeout"), _) => Fate::Unsure,
        (None, true) => Fate::Answered,
        // No result, or a failure this reader does not know.
        _ => Fate::Unobserved,
    }
}

/// The direct receipt's headline as its records show it (B19 F3): the model, the calls, the time
/// and the tokens. It returns what the destination line reads: the calls answered, those that may
/// have been sent (without an answer, or unobserved), and those refused before sending.
fn headline(receipt: &AuthoringReceipt, text: &mut String) -> (usize, usize, usize) {
    // Each call reads as its own record shows it (B19 F3): answered only on a recorded response,
    // never by subtraction. A counted call with no record, or none this reader knows, is
    // unobserved. When every counted call has one answered record, the words are as before.
    let count = |f: Fate| {
        receipt
            .context
            .iter()
            .filter(|call| fate(call) == f)
            .count()
    };
    let (answered, refused, unsure) = (
        count(Fate::Answered),
        count(Fate::Refused),
        count(Fate::Unsure),
    );
    let (records, counted) = (
        receipt.context.len(),
        usize::try_from(receipt.calls).unwrap_or(usize::MAX),
    );
    let unobserved = count(Fate::Unobserved) + counted.saturating_sub(records);
    let calls = format!(
        "{} call{}",
        receipt.calls,
        if receipt.calls == 1 { "" } else { "s" }
    );
    let _ = if records == counted && answered == records {
        write!(text, "\n  authoring backend: {} · {calls}", receipt.model)
    } else {
        write!(
            text,
            "\n  authoring backend: {} · {calls} attempted · {answered} answered · {unsure} without an answer, may have been sent · {refused} refused before sending",
            receipt.model
        )
    };
    if unobserved > 0 {
        let _ = write!(text, " · {unobserved} unobserved");
    }
    if records != counted {
        let plural = if records == 1 { "" } else { "s" };
        let _ = write!(text, " · the receipt records {records} call{plural}");
    }
    let _ = write!(text, " · {} ms", receipt.elapsed_ms);
    if let (Some(i), Some(o)) = (receipt.input_tokens, receipt.output_tokens) {
        let _ = write!(text, " · {i} in / {o} out tokens");
    }
    (answered, unsure + unobserved, refused)
}

/// The authoring receipt as a human reads it (descended from `nika-session`'s `/details`, C11):
/// the backend and model, the calls, tokens and time, where the calls really went (the
/// provider's own API, or the gateway its base URL is overridden to) and the cost basis the
/// receipt states; `run_cost` is the host's own words for where a metered run's cost is read.
/// A call that asked an explicit reasoning effort adds one line of its own facts
/// ([`reasoning_words`]); a call that asked none adds nothing.
#[must_use]
pub fn receipt_words(receipt: &AuthoringReceipt, run_cost: &str) -> String {
    let mut text = String::new();
    if let Some(backend) = receipt
        .backend
        .as_ref()
        .filter(|b| b["kind"] == "harness_infer")
    {
        let _ = write!(
            text,
            "\n  authoring backend: subscription {} · requested {} · {} compiler calls · {} ms",
            backend["adapter"].as_str().unwrap_or("unknown"),
            backend["requested_model"]
                .as_str()
                .unwrap_or("harness default"),
            receipt.calls,
            receipt.elapsed_ms
        );
        if backend["carried_from_authoring_round"] == true {
            text.push_str("\n    receipt carried from the authoring round; this clarification replay made zero calls");
        }
        if let Some(calls) = backend["observed"].as_array() {
            for call in calls.iter().filter(|c| c["status"] == "returned") {
                let _ = write!(
                    text,
                    "\n    responding model: {} · usage marker {}",
                    call["observed_model"].as_str().unwrap_or("not reported"),
                    call["usage_observed"].as_bool().unwrap_or(false)
                );
            }
        }
        text.push_str("\n  cost: subscription invoice unknown · no numeric token meter reported · no paid provider fallback");
        return text;
    }

    let (answered, unknown, refused) = headline(receipt, &mut text);
    if let Some(backend) = &receipt.backend {
        let _ = write!(
            text,
            "\n  {}: {} · host {}{}",
            match (answered, unknown, refused) {
                // Every call refused before sending, or none made (B19 review).
                (0, 0, _) => "nothing was sent to",
                (0, 1.., _) => "possibly sent to",
                _ => "sent to",
            },
            backend["provider"].as_str().unwrap_or("unknown provider"),
            backend["host"].as_str().unwrap_or("unknown"),
            if backend["base_url_overridden"].as_bool() == Some(true) {
                " (base URL overridden: a gateway or a local server, not the provider's own API)"
            } else {
                ""
            }
        );
    }
    for call in &receipt.context {
        text.push_str(&reasoning_words(call).unwrap_or_default());
    }
    let _ = write!(
        text,
        "\n  cost: the compiler meters tokens, not money · {run_cost}"
    );
    text
}

/// One authoring call's explicit reasoning effort, each fact apart (R4 B16 · C11): the level the
/// policy configured, the keys read back from the body the provider client sent (`unobserved`
/// when none was read back: never assumed from the level), the effort the provider served (not
/// observable here), the reasoning tokens and usage it reported, or why it gave no answer, and
/// the model it named. `None` for a call that asked no level.
#[must_use]
pub fn reasoning_words(call: &Value) -> Option<String> {
    let reasoning = &call["reasoning"];
    let configured = reasoning["configured"].as_str()?;
    let reported = |value: &Value| {
        value
            .as_str()
            .map(str::to_owned)
            .or_else(|| value.as_u64().map(|n| n.to_string()))
            .unwrap_or_else(|| "not reported".to_owned())
    };
    let sent = match &reasoning["transmitted"] {
        Value::Object(keys) => format!(
            "thinking {} · effort {}",
            keys.get("thinking")
                .and_then(Value::as_str)
                .unwrap_or("absent"),
            keys.get("effort")
                .and_then(Value::as_str)
                .unwrap_or("absent")
        ),
        _ => "unobserved".to_owned(),
    };
    let result = &call["result"];
    let answer = match result["failure_kind"].as_str() {
        Some(kind) => format!("no answer ({kind})"),
        None if !result["stop_reason"].is_string() => "result unobserved".to_owned(),
        None if result["usage_reported"] == true => format!(
            "usage {} in / {} out tokens",
            reported(&result["input_tokens"]),
            reported(&result["output_tokens"])
        ),
        None => "usage not reported".to_owned(),
    };
    Some(format!(
        "\n    {} call · reasoning effort {configured} configured · keys read back from the sent body: {sent} · effort served {} · reasoning tokens {} · {answer} · response model {}",
        call["call"].as_str().unwrap_or("authoring"),
        reasoning["served"].as_str().unwrap_or("unknown"),
        reported(&reasoning["reasoning_tokens"]),
        reported(&reasoning["response_model"])
    ))
}

#[cfg(test)]
mod tests;
