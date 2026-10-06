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

#[cfg(test)]
use super::QuestionType;
use serde_json::Value;

use super::{AuthoringReceipt, CompileOutcome, CompileQuestion, CompileStatus, DiagnosticKind};

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
    question.literal_for(line)
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
    /// rephrases. How the compiler tried is its own business. A candidate
    /// the compiler held for its verifier (`verify_held`) reads here too,
    /// whatever a later call met: [`held_words`] says it.
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
        // A candidate the compiler held (its verifier answered those bytes and did not accept
        // them) is read as held before anything else: a judge call that later timed out or was
        // refused stopped the localization, it settles nothing, and the outcome's words are the
        // held ones (`held_words`), never a recovery or a question about rejected bytes.
        if super::round::verify_held(&out) {
            return Self::Unsettled(out);
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

/// Passive wording of already selected findings; never used to decide compiler state.
pub use nika_display::front_door::reasons::human_reasons;

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

/// The budget headline: a deadline only when the last call records a timeout, otherwise generic.
#[must_use]
pub fn authoring_budget_headline(receipt: Option<&AuthoringReceipt>) -> &'static str {
    match receipt.and_then(|a| a.context.last()) {
        Some(call) if call["result"]["failure_kind"] == "timeout" => {
            "The authoring model did not answer within the call's time limit"
        }
        _ => "I couldn't finish a workflow I trust within the authoring budget",
    }
}

/// The compiler decision's route, seat, ledger and knowledge provenance, as recorded.
/// Pure projection shared by hosts; no inference or execution claim is added.
pub fn decision_words(decision: &Value, text: &mut String) {
    // The compiler records its route as the list of doors it tried.
    let route = match decision.get("route") {
        Some(Value::String(route)) => route.clone(),
        Some(Value::Array(steps)) => steps
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(" → "),
        _ => "none recorded".to_owned(),
    };
    let _ = write!(text, "\n  decision: route {route}");
    if let Some(seat) = decision.pointer("/seat/model").and_then(Value::as_str) {
        let _ = write!(text, " · seat {seat}");
    }
    let ledger = decision
        .get("ledger")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    if ledger > 0 {
        let _ = write!(
            text,
            " · ledger {ledger} clause{}",
            if ledger == 1 { "" } else { "s" }
        );
    }
    if let Some(record) = decision.pointer("/session/authoring") {
        crate::knowledge::pin::knowledge_lines(record, text);
    }
}

#[cfg(test)]
mod tests;

/// How a human answers a question, abandons it or asks why: the raw key stays out of the human's
/// line (« why? » names it, with what the value is for); the prompt that follows (`reply ›`) says
/// whose turn it is.
const REPLY_HINT: &str = "\n  reply on the next line · `cancel` drops this · `why?` explains";

/// Ask for a syntax clause in words, with the ordinary reply hint.
#[must_use]
pub fn syntax_prompt(clause: &str) -> String {
    format!("{}{REPLY_HINT}", syntax_question(clause))
}

/// The question as the human reads it ([`question_words`]), then how to answer or abandon it.
#[must_use]
pub fn question_text(question: &CompileQuestion, reasons: &[String]) -> String {
    format!("{}{REPLY_HINT}", question_words(question, reasons))
}

/// The card when nothing could be built, in the reading's own truth: a
/// seat's draft the compiler's fidelity check refused is an AUTHORING
/// failure (another attempt may hold every part), never a language gap;
/// the deterministic reader's unsupported clause is a gap in what Nika
/// can express. Neither is the human's ambiguity (mandate: a compiler gap
/// is never presented as user ambiguity, nor an authoring failure as a gap).
/// The way on after a revision that could not settle: an authoring failure (a seat tried and
/// failed on Nika's side) keeps the base and the change — the same words try again; a reading
/// the compiler could not settle asks for the change in other words. Never « describe the whole
/// automation again »: the base and the original request are kept.
#[must_use]
pub fn revision_way(out: &CompileOutcome) -> &'static str {
    if matches!(
        out.provenance.cognition,
        crate::compile::AuthoringCognition::ExplicitProvider
    ) {
        "an authoring step failed on Nika's side: your change is kept — send it again unchanged for another attempt, or `/intelligence` for another model"
    } else {
        "say the change another way"
    }
}

/// The honest incomplete when the rule stays code after the human's words
/// (or the clause is not in the request as quoted): the way on, no syntax.
#[must_use]
pub fn syntax_incomplete(clause: Option<&str>) -> String {
    let what = clause.map_or("this step".to_owned(), |c| format!("« {c} »"));
    format!(
        "I read this as work but cannot build {what} from your words yet: it would need a rule I can only write as code, and I never ask you for code.\n  · say the step differently — what to keep, what to compute, over which column, and where to write it\n  · or `cancel` and describe the work again\n  nothing was written"
    )
}

/// A candidate the verifier judged and rejected, with no defect it could locate: shown, never
/// proposed, and never asked again of the same verifier on the same bytes.
const DOUBTED: &str = "The workflow is built but not proposed: the verifier did not accept it and located no defect a repair could start from; nothing was written.";

/// A candidate the verifier read and neither accepted nor rejected.
const ABSTAINED: &str = "The workflow is built but not proposed: the verifier read it and abstained (it neither accepted nor rejected it); nothing was written.";

/// The ways on from a candidate its verifier did not accept: a correction, or another authoring
/// model, which also judges unless a decision model is set.
const HELD_NEXT: &str = "\n  describe a correction, or `/intelligence` for another authoring model (it also judges unless a decision model is set) · `/meaning` shows what was understood";

/// Whether the verifier answered `out`'s candidate and did not accept it: the compiler held it
/// (its applied `verify_held` finding), or its last verification contested it with no defect.
/// Such a candidate is shown, never proposed, never replayed to the same verifier.
pub(crate) fn judged_not_accepted(out: &CompileOutcome) -> bool {
    out.candidate.is_some()
        && (crate::compile::round::verify_held(out)
            || crate::compile::round::contested_judgment(out))
}

/// What a candidate its verifier did not accept says, by what the last verification found: the
/// parts it found missing that no repair settled (the first named), a rejection with no defect
/// located, or an abstention; then the ways on.
fn not_accepted_words(out: &CompileOutcome) -> String {
    let attempt = crate::compile::round::last_verification(out);
    let defects: Vec<&str> = (attempt.and_then(|a| a["defects"].as_array()).into_iter())
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let abstained = attempt.is_some_and(|a| a["declined"] == true && a["rejected"] == false);
    let said = match defects.as_slice() {
        [part] => format!(
            "The workflow is built but not proposed: the verifier found a part missing that the repairs did not settle: « {part} »; nothing was written."
        ),
        [first, ..] => format!(
            "The workflow is built but not proposed: the verifier found parts missing that the repairs did not settle: « {first} »…; nothing was written."
        ),
        [] if abstained && !crate::compile::round::contested_judgment(out) => ABSTAINED.to_owned(),
        [] => DOUBTED.to_owned(),
    };
    format!("{said}{HELD_NEXT}")
}

/// A native finish held for its round's judge (R4 A11 step 2), in words: the seat's program kept
/// as the preview while the whole request stays open (`decision.pending.open`), waiting for a
/// judge its round can permit — never an authoring failure nor a gap in the language. A candidate
/// its verifier answered and did not accept (the compiler's applied `verify_held` finding, or a
/// last verification that contested it with no defect) is said by what the verifier found
/// instead, its ways on a correction or another authoring model.
pub fn held_words(out: &CompileOutcome, has_model: bool) -> Option<String> {
    if judged_not_accepted(out) {
        return Some(not_accepted_words(out));
    }
    let open = (out.provenance.decision.as_ref()).and_then(|d| d.pointer("/pending/open"));
    open.and_then(serde_json::Value::as_array)
        .filter(|open| !open.is_empty() && out.candidate.is_some())?;
    let why = if has_model {
        "no judgment made in this round settled it; nothing was written.\n  state the request again for another attempt, or `/intelligence` for another model"
    } else {
        "this session has no authoring model to judge it; nothing was written.\n  `/intelligence` chooses one, then state the request again"
    };
    Some(format!(
        "The workflow is built but not proposed: the seat wrote this program, and only a judge this round can permit settles it against your whole request — {why} · `/meaning` shows what was understood"
    ))
}
