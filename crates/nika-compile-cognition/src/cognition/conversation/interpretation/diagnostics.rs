// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Explanations of the existing interpretation guards; no guard is relaxed by a diagnostic.

use std::collections::BTreeSet;

use serde_json::{Value, json};

pub(super) fn failure(stage: &str, kind: &str, path: &str, detail: &str) -> Value {
    json!({"stage":stage, "kind":kind, "path":path, "detail":detail})
}

fn invalid(path: &str, detail: &str) -> Value {
    failure("interpret_request", "invalid_structure", path, detail)
}

fn array<'a>(value: &'a Value, path: &str) -> Result<&'a [Value], Value> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| invalid(path, "Expected an array."))
}

fn strings<'a>(value: &'a Value, path: &str) -> Result<BTreeSet<&'a str>, Value> {
    let array = array(value, path)?;
    let values = array
        .iter()
        .map(Value::as_str)
        .collect::<Option<BTreeSet<_>>>()
        .ok_or_else(|| invalid(path, "Every reference must be a string."))?;
    if values.len() != array.len() {
        return Err(invalid(path, "A reference is repeated."));
    }
    Ok(values)
}

fn ids<'a>(entries: &'a [Value], field: &str, path: &str) -> Result<BTreeSet<&'a str>, Value> {
    let mut keys = BTreeSet::new();
    for (index, entry) in entries.iter().enumerate() {
        let at = format!("{path}[{index}].{field}");
        let key = entry[field]
            .as_str()
            .filter(|key| !key.trim().is_empty())
            .ok_or_else(|| invalid(&at, "Expected a nonempty identifier."))?;
        if !keys.insert(key) {
            return Err(invalid(&at, "This identifier is repeated."));
        }
    }
    Ok(keys)
}

/// What a reading repeats by mistake, said once: coverage rows for one span, or for one answer
/// key, become one row, their references united and an unresolved one kept unresolved, and a
/// reference repeated in one list is kept once. Nothing the reading claims is dropped or added:
/// the same claims, said once (live gauntlet wave 6 r11, 2026-10-18: « `coverage[1].span` · This
/// identifier is repeated » started a repair round of 2.7 minutes, then of 9.7 on a slow model).
/// A malformed row is left as it came, for [`validate`] to refuse.
pub(super) fn merged(reading: &mut Value) {
    const LISTS: [&str; 2] = ["runtime_ids", "preparation_ids"];
    for (rows, key) in [("coverage", "span"), ("answer_coverage", "key")] {
        let Some(rows) = reading[rows].as_array_mut() else {
            continue;
        };
        let mut said: Vec<Value> = Vec::with_capacity(rows.len());
        for mut row in rows.drain(..) {
            for list in LISTS {
                if let Some(items) = row[list].as_array_mut() {
                    let mut once: Vec<Value> = Vec::with_capacity(items.len());
                    for item in items.drain(..) {
                        if !once.contains(&item) {
                            once.push(item);
                        }
                    }
                    *items = once;
                }
            }
            let same = (row[key].as_str())
                .and_then(|id| (said.iter_mut()).find(|kept| kept[key].as_str() == Some(id)));
            let Some(kept) = same.filter(|kept| mergeable(kept, &row)) else {
                said.push(row);
                continue;
            };
            for list in LISTS {
                let added: Vec<Value> = (row[list].as_array().into_iter().flatten())
                    .filter(|item| {
                        !kept[list]
                            .as_array()
                            .is_some_and(|known| known.contains(item))
                    })
                    .cloned()
                    .collect();
                if let Some(known) = kept[list].as_array_mut() {
                    known.extend(added);
                }
            }
            let unresolved = kept["unresolved"] == true || row["unresolved"] == true;
            kept["unresolved"] = Value::Bool(unresolved);
        }
        *rows = said;
    }
}

/// Whether two rows for one identifier hold only their lists and their flag, both well formed.
fn mergeable(kept: &Value, row: &Value) -> bool {
    let shaped = |value: &Value| {
        value["runtime_ids"].is_array()
            && value["preparation_ids"].is_array()
            && value["unresolved"].is_boolean()
            && value.as_object().is_some_and(|fields| fields.len() == 4)
    };
    shaped(kept) && shaped(row)
}

/// The same exact source, reference and ambiguity laws used to admit a reading, with the
/// first failing location retained instead of erased into a boolean.
pub(super) fn validate(input: &Value, reading: &Value) -> Result<(), Value> {
    let runtime = array(&reading["runtime_obligations"], "runtime_obligations")?;
    let preparation = array(&reading["preparation_requests"], "preparation_requests")?;
    let coverage = array(&reading["coverage"], "coverage")?;
    let answers = array(&reading["answer_coverage"], "answer_coverage")?;
    let spans = array(&input["spans"], "source.spans")?;
    let answer_keys = input["answers"]
        .as_object()
        .ok_or_else(|| invalid("source.answers", "Expected an object of scoped answers."))?
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let runtime_ids = ids(runtime, "id", "runtime_obligations")?;
    let prep_ids = ids(preparation, "id", "preparation_requests")?;
    if !runtime_ids.is_disjoint(&prep_ids) {
        return Err(invalid(
            "preparation_requests",
            "An identifier also names a runtime obligation.",
        ));
    }
    if ids(coverage, "span", "coverage")? != ids(spans, "id", "source.spans")? {
        return Err(invalid(
            "coverage",
            "Name every source span exactly once, without adding a span.",
        ));
    }
    if ids(answers, "key", "answer_coverage")? != answer_keys {
        return Err(invalid(
            "answer_coverage",
            "Name every scoped answer key exactly once, without adding a key.",
        ));
    }
    evidence(runtime, preparation, spans)?;
    links(coverage, answers, &runtime_ids, &prep_ids)?;
    for (index, entry) in runtime.iter().enumerate() {
        let at = format!("runtime_obligations[{index}].answer_keys");
        if !strings(&entry["answer_keys"], &at)?.is_subset(&answer_keys) {
            return Err(invalid(&at, "A reference names no scoped human answer."));
        }
    }
    Ok(())
}

fn evidence(runtime: &[Value], preparation: &[Value], spans: &[Value]) -> Result<(), Value> {
    for (entries, text_key, path) in [
        (runtime, "requirement", "runtime_obligations"),
        (preparation, "request", "preparation_requests"),
    ] {
        for (index, entry) in entries.iter().enumerate() {
            let at = format!("{path}[{index}]");
            if entry[text_key]
                .as_str()
                .is_none_or(|text| text.trim().is_empty())
            {
                return Err(invalid(
                    &format!("{at}.{text_key}"),
                    "Expected nonempty text.",
                ));
            }
            let evidence = array(&entry["evidence"], &format!("{at}.evidence"))?;
            if evidence.is_empty() {
                return Err(invalid(
                    &format!("{at}.evidence"),
                    "An extracted requirement needs source evidence.",
                ));
            }
            for (citation, cite) in evidence.iter().enumerate() {
                let at = format!("{at}.evidence[{citation}]");
                let quote = cite["quote"]
                    .as_str()
                    .filter(|text| !text.is_empty())
                    .ok_or_else(|| {
                        invalid(
                            &format!("{at}.quote"),
                            "Expected a nonempty exact source quote.",
                        )
                    })?;
                let span = spans
                    .iter()
                    .find(|span| span["id"] == cite["span"])
                    .ok_or_else(|| {
                        invalid(
                            &format!("{at}.span"),
                            "The cited source span does not exist.",
                        )
                    })?;
                let text = span["text"]
                    .as_str()
                    .ok_or_else(|| invalid("source.spans", "A source span has no text."))?;
                if !text.contains(quote) {
                    return Err(failure(
                        "interpret_request",
                        "evidence",
                        &format!("{at}.quote"),
                        "The quote is not an exact substring of its cited source span.",
                    ));
                }
            }
        }
    }
    Ok(())
}

fn links(
    coverage: &[Value],
    answers: &[Value],
    runtime_ids: &BTreeSet<&str>,
    prep_ids: &BTreeSet<&str>,
) -> Result<(), Value> {
    let mut linked_runtime = BTreeSet::new();
    let mut linked_prep = BTreeSet::new();
    for (entries, path) in [(coverage, "coverage"), (answers, "answer_coverage")] {
        for (index, item) in entries.iter().enumerate() {
            let at = format!("{path}[{index}]");
            match item["unresolved"].as_bool() {
                Some(false) => {}
                Some(true) => {
                    return Err(failure(
                        "interpret_request",
                        "unresolved",
                        &format!("{at}.unresolved"),
                        "The reading marks this source as unresolved. Resolve only what the original request and scoped answers support. Keep genuine ambiguity unresolved; do not invent a choice or rewrite the workflow to repair this reading.",
                    ));
                }
                None => return Err(invalid(&format!("{at}.unresolved"), "Expected a boolean.")),
            }
            let linked = strings(&item["runtime_ids"], &format!("{at}.runtime_ids"))?;
            let prep = strings(&item["preparation_ids"], &format!("{at}.preparation_ids"))?;
            if !linked.is_subset(runtime_ids) {
                return Err(invalid(
                    &format!("{at}.runtime_ids"),
                    "A reference names no extracted runtime obligation.",
                ));
            }
            if !prep.is_subset(prep_ids) {
                return Err(invalid(
                    &format!("{at}.preparation_ids"),
                    "A reference names no extracted preparation request.",
                ));
            }
            linked_runtime.extend(linked);
            linked_prep.extend(prep);
        }
    }
    if &linked_runtime != runtime_ids {
        return Err(invalid(
            "coverage",
            "An extracted runtime obligation is not linked by source or answer coverage.",
        ));
    }
    if &linked_prep != prep_ids {
        return Err(invalid(
            "coverage",
            "An extracted preparation request is not linked by source or answer coverage.",
        ));
    }
    Ok(())
}

/// Model-authored semantic notes stay quoted as diagnostic evidence, never become instructions
/// to waive a guard, change the original request, or authorize a workflow effect.
pub(super) fn message(record: Option<&Value>) -> String {
    let Some(record) = record else {
        return "The request reading has no diagnostic record; no program was judged or run."
            .into();
    };
    let suffix = if record["repair_stop"]["reason"] == "repeated_feedback" {
        "Internal interpretation repair stopped after repeated feedback. No program was judged or run; this is not a workflow defect. Do not rewrite the workflow or repeat the same verification without new source information or context."
    } else {
        "No program was judged or run. Repair the request interpretation from the unchanged original request and scoped answers; this is not a workflow defect."
    };
    if let Some(failure) = record.get("failure") {
        let stage = failure["stage"].as_str().unwrap_or("interpret_request");
        let kind = failure["kind"].as_str().unwrap_or("unknown");
        let path = failure["path"].as_str().unwrap_or("response");
        let detail = failure["detail"]
            .as_str()
            .unwrap_or("No precise cause was recorded.");
        let position = match (failure["line"].as_u64(), failure["column"].as_u64()) {
            (Some(line), Some(column)) => format!(" (line {line}, column {column})"),
            _ => String::new(),
        };
        let next = if kind == "call" {
            "No program was judged or run. Resolve the reported authoring call problem before retrying; the workflow was not rejected."
        } else {
            suffix
        };
        return format!(
            "Request reading failed at {stage}.{path}: {kind}{position}. {detail} {next}"
        );
    }
    match record["audit"]["status"].as_str() {
        Some("defect") => {
            let defects: Vec<String> = record["audit"]["defects"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|defect| {
                    format!(
                        "{} at {}: {}; source evidence {}",
                        defect["kind"], defect["location"], defect["note"], defect["evidence"]
                    )
                })
                .collect();
            format!(
                "The independent reading audit reported: {}. {suffix}",
                defects.join("; ")
            )
        }
        Some("unknown") => "The independent reading audit could not resolve the supplied context. No program was judged or run. Identify the missing source information before retrying; this is not a transport failure or a workflow defect.".to_owned(),
        _ => format!(
            "The request interpretation did not complete; no precise cause was recorded. {suffix}"
        ),
    }
}
