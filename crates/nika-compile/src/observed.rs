// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Positive host observations, never a claim that an unobserved field cannot exist.
use crate::{ChoiceOffer, CompileOutcome, CompileRequest, DiagnosticKind};
use grounding::Grade;
use serde_json::{Value, json};

mod grounding;

/// Keys observed in one source, with exact spelling. None means no usable observation;
/// Some(empty) means the host observed records with no common keys. Neither is a schema.
#[must_use]
pub fn columns(world: Option<&Value>, path: &str) -> Option<Vec<String>> {
    let rows = world?.get("observed")?.as_array()?;
    let path = path.strip_prefix("./").unwrap_or(path);
    let mut matched = rows.iter().filter(|row| {
        row.get("path")
            .and_then(Value::as_str)
            .is_some_and(|p| p.strip_prefix("./").unwrap_or(p) == path)
    });
    let row = matched.next()?;
    if matched.next().is_some() {
        return None;
    }
    if row
        .get("state")
        .and_then(Value::as_str)
        .is_some_and(|s| s != "observed")
    {
        return None;
    }
    let values = row
        .get("common_columns")
        .or_else(|| row.get("columns"))?
        .as_array()?;
    let mut names = Vec::new();
    for value in values {
        let name = value.as_str()?;
        if name.is_empty() {
            return None;
        }
        if !names.iter().any(|n| n == name) {
            names.push(name.to_owned());
        }
    }
    Some(names)
}

/// A single stated source only: destinations and unrelated observations are not hints.
#[must_use]
pub fn for_intent(world: Option<&Value>, intent: &str) -> Option<Vec<String>> {
    let paths = crate::stated_sources(intent);
    let [path] = paths.as_slice() else {
        return None;
    };
    columns(world, path)
}

/// Ask a closed field choice and accept only a verbatim offered key.
pub fn field_answer(
    request: &CompileRequest,
    out: &mut CompileOutcome,
    key: &str,
    label: &str,
    columns: &[String],
) -> Option<Value> {
    if let Some(raw) = request.answers.get(key) {
        let value = crate::literal_answer(Some(raw), key, out);
        if let Some(value) = value
            && value
                .as_str()
                .is_some_and(|name| columns.iter().any(|c| c == name))
        {
            return Some(value);
        }
        crate::finding(
            out,
            DiagnosticKind::Missed,
            key,
            "Choose an observed field with its exact spelling.",
        );
    }
    if columns.is_empty() {
        crate::question(out, key, label, crate::QuestionType::Text);
        crate::finding(
            out,
            DiagnosticKind::Unknown,
            key,
            "The observed records have no common field; clarify the source or its shape.",
        );
    } else {
        crate::choice_question(
            out,
            key,
            label,
            &format!(
                "These keys were observed, not a complete schema. Answer one of: {}.",
                columns.join(" · ")
            ),
            columns.iter().map(|c| ChoiceOffer::new(c, c)).collect(),
        );
    }
    None
}

/// Every source key a typed rule reads is grounded (R4 S1, [`grounding`]) and recorded in the
/// decision: an admissible key is kept, any other is asked (a closed choice of the observed keys,
/// or the exact key when nothing was observed), and a key some sampled records lack keeps the
/// rule pending until the request states what happens to those records. Only typed source
/// references can be rebound; program bytes never undergo replacement.
pub(crate) fn ground_rule(
    mut rule: crate::rules::Rule,
    path: &str,
    request: &CompileRequest,
    out: &mut CompileOutcome,
    recognized: &mut std::collections::BTreeSet<String>,
) -> Option<crate::rules::Rule> {
    let intent = match &request.input {
        crate::types::Input::Create(intent) => intent.clone(),
        _ => rule.text().to_owned(),
    };
    let row = grounding::row(world(request), path);
    let seen = grounding::seen(row);
    let stated = crate::columns::columns_hint(&intent);
    let mut entries = Vec::new();
    let mut pending = false;
    for (index, field) in rule.source_fields().into_iter().enumerate() {
        let approved = crate::pending_transform::approves(request, &rule, path, &field, recognized);
        let bound_by = if approved {
            Some("approval")
        } else {
            names_field(&intent, &field).then_some("request")
        };
        let (grade, everywhere) = grounding::grade(&field, seen.as_ref(), &stated);
        let text = rule.text().to_owned();
        let entry = |key: &str, grade, everywhere, bound_by| {
            (grounding::Entry {
                rule: &text,
                key,
                source: path,
                row,
                grade,
                everywhere,
                bound_by,
            })
            .to_json()
        };
        if grade != Grade::Inferred && bound_by.is_some() {
            entries.push(entry(&field, grade, everywhere, bound_by));
            continue;
        }
        let key = format!("const.rule_field_{}", index + 1);
        recognized.insert(key.clone());
        let answered = mapped(request, out, &key, &rule, &field, path, seen.as_ref());
        let rebound = answered
            .as_deref()
            .map(|name| (name, rule.with_source_field(&field, name)));
        match rebound {
            Some((name, Some(rebound))) => {
                let (grade, everywhere) = match &seen {
                    Some(_) => grounding::grade(name, seen.as_ref(), &stated),
                    None => (Grade::UserAsserted, true),
                };
                entries.push(entry(name, grade, everywhere, Some("answer")));
                rule = rebound;
            }
            Some((_, None)) => {
                crate::finding(
                    out,
                    DiagnosticKind::Unknown,
                    &key,
                    "A program cannot be renamed safely; regenerate the computation using the selected field.",
                );
                pending = true;
            }
            None => {
                entries.push(entry(&field, grade, everywhere, bound_by));
                pending = true;
            }
        }
    }
    pending |= settle(out, entries);
    (!pending).then_some(rule)
}

/// The key an answer maps the rule's word to, in the context its question was asked: one of the
/// observed keys, or, when nothing was observed, the exact key the human states. An answer given
/// for another revision of the source is stale: refused, and the question asked again.
fn mapped(
    request: &CompileRequest,
    out: &mut CompileOutcome,
    key: &str,
    rule: &crate::rules::Rule,
    field: &str,
    path: &str,
    seen: Option<&grounding::Seen>,
) -> Option<String> {
    if request.answers.contains_key(key) && grounding::stale(request, path) {
        crate::finding(
            out,
            DiagnosticKind::Missed,
            key,
            format!(
                "`{path}` changed since this question was asked, so its answer maps another revision. Answer again."
            ),
        );
        let mut fresh = request.clone();
        fresh.answers.remove(key);
        return mapped(&fresh, out, key, rule, field, path, seen);
    }
    let text = rule.text();
    if let Some(seen) = seen {
        let label = format!("Which observed field in `{path}` does `{field}` mean in `{text}`?");
        return field_answer(request, out, key, &label, &seen.all)
            .and_then(|value| value.as_str().map(str::to_owned));
    }
    if let Some(raw) = request.answers.get(key) {
        let named = crate::literal_answer(Some(raw), key, out).and_then(|v| {
            v.as_str()
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map(str::to_owned)
        });
        if named.is_some() {
            return named;
        }
        crate::finding(out, DiagnosticKind::Missed, key, "Name the exact key.");
    }
    let label = format!(
        "`{path}` was not observed: which exact key of its records holds `{field}` in `{text}`?"
    );
    crate::question(out, key, &label, crate::QuestionType::Text);
    None
}

/// Record the grounding of this rule's keys in the decision (replacing an earlier door's); a
/// grounded key some sampled records lack opens the missing-records obligation, stated and
/// asked, never defaulted. Returns whether one is open.
fn settle(out: &mut CompileOutcome, entries: Vec<Value>) -> bool {
    let open: Vec<&Value> = entries.iter().filter(|e| !e["open"].is_null()).collect();
    for entry in &open {
        let word = |key: &str| entry[key].as_str().unwrap_or_default().to_owned();
        crate::finding(
            out,
            DiagnosticKind::Unknown,
            "grounding",
            format!(
                "`{}` is in only some sampled records of `{}`: what `{}` does with a record lacking it is not stated (a missing or null value compares, sorts and totals differently). Name it in a replacement request, or make every record carry the key.",
                word("field"),
                word("source"),
                word("rule")
            ),
        );
    }
    let opened = !open.is_empty();
    if opened {
        crate::question(
            out,
            "intent.clarification",
            "Supply a complete replacement request that states what happens to the records lacking the key. It explicitly replaces the earlier intent.",
            crate::QuestionType::Text,
        );
    }
    let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
    decision["grounding"] = Value::Array(entries);
    out.provenance.decision = Some(decision);
    opened
}

/// Prefer this round's host observation, retaining the previous question's offers on replay.
#[must_use]
pub fn world(request: &CompileRequest) -> Option<&Value> {
    request
        .knowledge
        .as_ref()
        .or_else(|| request.plan.as_ref()?.get("observed_world"))
}

/// Keep field questions closed on a zero-call answer round, including library callers.
pub fn record(request: &CompileRequest, out: &mut CompileOutcome) {
    if let (Some(world), Some(plan)) = (world(request), out.provenance.plan.as_mut()) {
        plan["observed_world"] = world.clone();
    }
    if let (Some(record), Some(plan)) = (request.plan.as_ref(), out.provenance.plan.as_mut())
        && let Some(verified) = record.get("verified_transform")
        && plan.get("pending_transform").is_none()
    {
        plan["verified_transform"] = verified.clone();
    }
}

/// Exact key occurrences, with identifier boundaries; a translated noun is not an alias.
#[must_use]
pub(crate) fn names_field(intent: &str, field: &str) -> bool {
    if field.is_empty() {
        return false;
    }
    let identifier = |c: char| c.is_alphanumeric() || c == '_';
    intent.match_indices(field).any(|(at, _)| {
        !intent[..at].chars().next_back().is_some_and(identifier)
            && !intent[at + field.len()..]
                .chars()
                .next()
                .is_some_and(identifier)
    })
}

#[cfg(test)]
mod tests;
