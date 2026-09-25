// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Positive host observations, never a claim that an unobserved field cannot exist.
use crate::{ChoiceOffer, CompileOutcome, CompileRequest, DiagnosticKind};
use serde_json::Value;

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

/// Only typed source references can be rebound. Program bytes never undergo replacement.
pub(crate) fn ground_rule(
    mut rule: crate::rules::Rule,
    path: &str,
    request: &CompileRequest,
    out: &mut CompileOutcome,
    recognized: &mut std::collections::BTreeSet<String>,
) -> Option<crate::rules::Rule> {
    let Some(columns) = columns(world(request), path) else {
        return Some(rule);
    };
    let mut pending = false;
    for (index, field) in rule.source_fields().into_iter().enumerate() {
        let intent = match &request.input {
            crate::types::Input::Create(intent) => intent.as_str(),
            _ => rule.text(),
        };
        let approved = crate::pending_transform::approves(request, &rule, path, &field, recognized);
        if columns.contains(&field) && (names_field(intent, &field) || approved) {
            continue;
        }
        let key = format!("const.rule_field_{}", index + 1);
        recognized.insert(key.clone());
        let label = format!(
            "Which observed field in `{path}` does `{field}` mean in `{}`?",
            rule.text()
        );
        let answer = field_answer(request, out, &key, &label, &columns);
        match answer.as_ref().and_then(Value::as_str) {
            Some(name) => {
                if let Some(rebound) = rule.with_source_field(&field, name) {
                    rule = rebound;
                } else {
                    crate::finding(
                        out,
                        DiagnosticKind::Unknown,
                        &key,
                        "A program cannot be renamed safely; regenerate the computation using the selected field.",
                    );
                    pending = true;
                }
            }
            None => pending = true,
        }
    }
    (!pending).then_some(rule)
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
