// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The monetary directives a caller admitted as its own ceiling (R4 A6 · D6-S2/S3). « …, then
//! write them to ./open.csv. Budget: $0. » reached the reader whole: the ceiling Session had
//! admitted was an unresolved clause (so a seat was needed and the ceiling refused it), and
//! « …, budget=0 » became a business filter on a field named `budget`. Recognition is
//! [`crate::money`]'s and admission the caller's: an admitted span must be one of the request's
//! own directives; the request is read with those spans blanked (the same bytes, the same
//! offsets), never as business clauses, and the decision records each one beside the original
//! request's identity. A directive with no currency whose anchor names an observed field
//! (« budget=0 » over a file with a `budget` column) reads both ways: it stays text and is asked.

use serde_json::{Value, json};

use crate::types::{CompileRequest, Input};
use crate::{CompileOutcome, CompileStatus, DiagnosticKind, QuestionType};

/// The request its reading sees and the record of what the caller admitted, or `None` when the
/// caller admitted nothing (or the input is no request in words).
///
/// # Errors
/// The refusal when a span the caller admitted is not one of the request's monetary directives.
pub(crate) fn read(request: &CompileRequest) -> Result<Option<(CompileRequest, Value)>, String> {
    let Input::Create(intent) = &request.input else {
        return Ok(None);
    };
    if request.money.is_empty() {
        return Ok(None);
    }
    let found = crate::money::directives(intent).map_err(str::to_owned)?;
    let mut text = intent.clone();
    let mut records = Vec::new();
    for span in &request.money {
        let Some(directive) = found.found.iter().find(|d| &d.span == span) else {
            return Err(format!(
                "the admitted monetary span {}..{} is not a monetary directive of the request",
                span.start, span.end
            ));
        };
        let words = &intent[span.clone()];
        let money = crate::money::parse(words).map_err(str::to_owned)?;
        let mut record = json!({"text": words, "span": [span.start, span.end],
            "amount": money.amount, "literal": money.literal, "currency": directive.currency});
        let field = (!directive.currency)
            .then(|| {
                directive
                    .anchor
                    .as_deref()
                    .and_then(|a| observed_field(request, a))
            })
            .flatten();
        if let Some(field) = field {
            record["ambiguous_field"] = json!(field);
        } else {
            text.replace_range(span.clone(), &" ".repeat(span.len()));
        }
        records.push(record);
    }
    // A request that names a skeleton only once its directive is blanked (« hello budget 2 USD »)
    // is no skeleton request: it is read as written.
    let bare = text.trim();
    if matches!(bare, "hello" | "01-hello") || nika_pack::template_names().iter().any(|n| n == bare)
    {
        return Ok(None);
    }
    let read_sha = crate::intent_sha256(&text);
    let mut reading = request.clone();
    reading.input = Input::Create(text);
    reading.money = Vec::new();
    Ok(Some((
        reading,
        json!({"directives": records, "read_intent_sha256": read_sha}),
    )))
}

/// Record what the caller admitted beside the outcome of the reading: the original request's
/// identity, each directive, and a question where one reads both ways.
pub(crate) fn record(request: &CompileRequest, money: Value, out: &mut CompileOutcome) {
    let Input::Create(intent) = &request.input else {
        return;
    };
    for directive in money["directives"].as_array().into_iter().flatten() {
        let words = directive["text"].as_str().unwrap_or_default();
        if let Some(field) = directive["ambiguous_field"].as_str() {
            crate::finding(
                out,
                DiagnosticKind::Unknown,
                "intent",
                format!(
                    "`{words}` reads as the monetary ceiling the caller admitted and as a rule over the observed field `{field}`: state the ceiling with a currency (« Budget: $0 ») or the rule in words."
                ),
            );
            out.status = CompileStatus::Incomplete;
            out.candidate = None;
            out.check_preview = None;
            if !out
                .questions
                .iter()
                .any(|q| q.key == "intent.clarification")
            {
                crate::question(
                    out,
                    "intent.clarification",
                    "Supply a complete replacement request that says whether the amount is the work's monetary ceiling or a rule over that field.",
                    QuestionType::Text,
                );
            }
        } else {
            crate::finding(
                out,
                DiagnosticKind::Applied,
                "money",
                format!(
                    "`{words}` is the monetary ceiling the caller admitted, not a task: it is not read as a business clause and never enters the workflow bytes; the compiler certifies no cap."
                ),
            );
        }
    }
    let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
    decision["intent_sha256"] = json!(crate::intent_sha256(intent));
    decision["money"] = money;
    out.provenance.decision = Some(decision);
}

/// The outcome of a request whose admitted span is none of its monetary directives: refused,
/// nothing read.
#[must_use]
pub(crate) fn refused(why: &str) -> CompileOutcome {
    let mut out = crate::initial();
    out.status = CompileStatus::Refused;
    crate::finding(&mut out, DiagnosticKind::Refused, "money", why);
    out
}

/// The observed field of a source the request names that spells `word`, case aside.
fn observed_field(request: &CompileRequest, word: &str) -> Option<String> {
    let observed = crate::observed::world(request)?;
    observed["observed"]
        .as_array()?
        .iter()
        .flat_map(|row| {
            ["columns", "common_columns"]
                .into_iter()
                .filter_map(move |key| row[key].as_array())
                .flatten()
        })
        .filter_map(Value::as_str)
        .find(|column| column.eq_ignore_ascii_case(word))
        .map(str::to_owned)
}
