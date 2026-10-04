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
//! A door that meters no seat states its operator's money instead (R4 B15 · the CLI): every
//! directive of the words the compiler reads is admitted, a replacement request's on the seats'
//! door, never the words of a request it replaced.

use std::ops::Range;

use serde_json::{Value, json};

use crate::money::Directives;
use crate::types::{CompileRequest, EditChange, Input};
use crate::{CompileOutcome, CompileStatus, DiagnosticKind, QuestionType};

/// The request its reading sees and the record of what the caller admitted, or `None` when the
/// caller admitted nothing (or the input is no request in words). A revision's words are its
/// change; on a door that states its operator's money, the request its base answered is words
/// the seat reads too, and states money as well.
///
/// # Errors
/// The refusal when a span the caller admitted is not one of the request's monetary directives,
/// or when the money a caller states is malformed or conflicting.
pub fn read(request: &CompileRequest) -> Result<Option<(CompileRequest, Value)>, String> {
    let Some(intent) = words(request) else {
        return Ok(None);
    };
    if request.money.is_empty() && !request.stated_money {
        return Ok(None);
    }
    let found = crate::money::directives(intent).map_err(str::to_owned)?;
    let spans = if request.stated_money {
        every(&found)
    } else {
        request.money.clone()
    };
    let (text, mut records) =
        blank(request, intent, &found, &spans)?.unwrap_or_else(|| (intent.clone(), Vec::new()));
    let mut reading = request.clone();
    reading.money = Vec::new();
    reading.stated_money = false;
    if let (Input::Edit { .. }, Some(original), true) = (
        &request.input,
        &request.original_intent,
        request.stated_money,
    ) {
        let found = crate::money::directives(original).map_err(str::to_owned)?;
        if let Some((blanked, stated)) = blank(request, original, &found, &every(&found))? {
            reading.original_intent = Some(blanked);
            records.extend(stated.into_iter().map(|mut record| {
                record["in"] = json!("original_intent");
                record
            }));
        }
    }
    if records.is_empty() {
        return Ok(None);
    }
    // A request that names a skeleton only once its directive is blanked (« hello budget 2 USD »)
    // is no skeleton request: it is read as written, and its money still binds — recorded, and
    // read by no seat as work (R4 B15 review).
    let written = matches!(request.input, Input::Create(_)) && crate::money::skeleton(text.trim());
    if !written {
        match &mut reading.input {
            Input::Create(read)
            | Input::Edit {
                change: EditChange::Text(read),
                ..
            } => *read = text,
            Input::Edit { .. } => {}
        }
    }
    let read_sha = match &reading.input {
        Input::Create(read) => crate::intent_sha256(read),
        Input::Edit { .. } => {
            crate::intent_sha256(&crate::revise_intent(&reading).unwrap_or_default())
        }
    };
    let mut money = json!({"directives": records, "read_intent_sha256": read_sha});
    if written {
        money["read_as_written"] = json!(true);
    }
    Ok(Some((reading, money)))
}

/// The money of a request, read before any strategy, by every door that reads one (the seats'
/// door and a semantic record's replay alike): a creation's clarification replaces its request
/// under its own law, else the request's directives ([`read`]). The request it reads, and the
/// record of its directives when it states any.
///
/// # Errors
/// The refusal of a span that is no directive, or of malformed or conflicting stated money.
pub fn reading(request: &CompileRequest) -> Result<(CompileRequest, Option<Value>), String> {
    clarified(request).unwrap_or_else(|| {
        read(request).map(|read| {
            read.map_or_else(
                || (request.clone(), None),
                |(reading, money)| (reading, Some(money)),
            )
        })
    })
}

/// A creation's clarification replaces its request. A host admission belongs to those exact
/// bytes: changed words discard it, identical words keep it and their blanked reading. On a
/// door that states money, the answer's own directives are read afresh. A revision's change
/// is never replaced: its money is read by [`read`].
fn clarified(request: &CompileRequest) -> Option<Result<(CompileRequest, Option<Value>), String>> {
    let Input::Create(original) = &request.input else {
        return None;
    };
    let raw = request.answers.get("intent.clarification")?;
    let text = serde_json::from_str::<Value>(raw)
        .ok()?
        .as_str()?
        .to_owned();
    if !request.stated_money {
        if text != *original {
            return Some(Ok((request.clone().with_admitted_money(Vec::new()), None)));
        }
        return Some(read(request).map(|read| match read {
            Some((mut reading, money)) => {
                if let Input::Create(blanked) = &reading.input {
                    reading.answers.insert(
                        "intent.clarification".to_owned(),
                        json!(blanked).to_string(),
                    );
                }
                (reading, Some(money))
            }
            None => (request.clone(), None),
        }));
    }
    let mut reading = request.clone();
    reading.stated_money = false;
    Some(replacement(request, &text).map(|read| match read {
        Some((blanked, money)) => {
            reading.answers.insert(
                "intent.clarification".to_owned(),
                json!(blanked).to_string(),
            );
            (reading, Some(money))
        }
        None => (reading, None),
    }))
}

/// The words a request states: a creation's intent, a revision's change in words.
fn words(request: &CompileRequest) -> Option<&String> {
    match &request.input {
        Input::Create(intent)
        | Input::Edit {
            change: EditChange::Text(intent),
            ..
        } => Some(intent),
        Input::Edit { .. } => None,
    }
}

/// Every directive found: the money a door that meters no seat states.
fn every(found: &Directives) -> Vec<Range<usize>> {
    found.found.iter().map(|d| d.span.clone()).collect()
}

/// A replacement request the operator states on the seats' door (`intent.clarification`), read
/// by the same law: its text with its directives blanked and their record, or `None` when it
/// states none. The words of the request it replaced grant it nothing.
///
/// # Errors
/// The refusal of a malformed or conflicting directive.
pub fn replacement(
    request: &CompileRequest,
    text: &str,
) -> Result<Option<(String, Value)>, String> {
    let found = crate::money::directives(text).map_err(str::to_owned)?;
    let spans: Vec<Range<usize>> = found.found.iter().map(|d| d.span.clone()).collect();
    let Some((blanked, records)) = blank(request, text, &found, &spans)? else {
        return Ok(None);
    };
    let read_sha = crate::intent_sha256(&blanked);
    Ok(Some((
        blanked,
        json!({"directives": records, "read_intent_sha256": read_sha}),
    )))
}

/// `intent` with each admitted span blanked (the same bytes, the same offsets) and the record
/// of each, or `None` when no span was admitted.
fn blank(
    request: &CompileRequest,
    intent: &str,
    found: &Directives,
    spans: &[Range<usize>],
) -> Result<Option<(String, Vec<Value>)>, String> {
    if spans.is_empty() {
        return Ok(None);
    }
    let mut text = intent.to_owned();
    let mut records = Vec::new();
    for span in spans {
        let Some(directive) = found.found.iter().find(|d| &d.span == span) else {
            return Err(format!(
                "the admitted monetary span {}..{} is not a monetary directive of the request",
                span.start, span.end
            ));
        };
        let words = &intent[span.clone()];
        let money = crate::money::stated(words).map_err(str::to_owned)?;
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
    Ok(Some((text, records)))
}

/// Record what the caller admitted beside the outcome of the reading: the original request's
/// identity, each directive, and a question where one reads both ways.
pub fn record(request: &CompileRequest, money: Value, out: &mut CompileOutcome) {
    let intent = match &request.input {
        Input::Create(intent) => intent.clone(),
        Input::Edit { .. } => match crate::revise_intent(request) {
            Some(intent) => intent,
            None => return,
        },
    };
    let written = money["read_as_written"] == json!(true);
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
        } else if written {
            crate::finding(
                out,
                DiagnosticKind::Applied,
                "money",
                format!(
                    "`{words}` is the monetary ceiling the caller admitted; without it the request names a skeleton, so it is read as written, never as that skeleton, and no seat reads it as work; the compiler certifies no cap."
                ),
            );
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
    decision["intent_sha256"] = json!(crate::intent_sha256(&intent));
    decision["money"] = money;
    out.provenance.decision = Some(decision);
}

/// The outcome of a request whose admitted span is none of its monetary directives, or whose
/// stated money is malformed or conflicting: refused, nothing read.
#[must_use]
pub fn refused(why: &str) -> CompileOutcome {
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
