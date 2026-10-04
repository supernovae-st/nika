// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The compiler's facade over the one grounding law (R4 S1), which lives in
//! `nika_compile_fidelity::grounding` (pure over the host's observation): the request-bound parts
//! stay here.
//! - An answer given for another revision of a source is stale ([`stale`]).
//! - A key the request never names is bound by the observation when the request states its value
//!   ([`witness`], F2-Q1): a literal the rule compares the key to, recorded by the host among that
//!   key's values and no other column's. A sample that never shows it proves nothing.
use crate::rules::Rule;
use nika_compile_fidelity::grounding::compares;
pub(crate) use nika_compile_fidelity::grounding::{Entry, Grade, Seen, grade, revision, row, seen};
use serde_json::Value;

/// Whether an answer round's observation of `path` differs from the one the replayed record
/// asked its questions against: its answers then map another revision. With no fresh
/// observation, the record's own is all there is to judge.
pub(crate) fn stale(request: &crate::CompileRequest, path: &str) -> bool {
    let Some(record) = request.plan.as_ref() else {
        return false;
    };
    let asked = record.get("observed_world");
    let now = request.knowledge.as_ref().or(asked);
    row(asked, path) != row(now, path)
}

/// The literal that witnesses a seat's `field` the request never names (F2-Q1): the rule compares
/// `field` to it (a typed text equality or inequality, or a verified program's literal comparison,
/// [`compares`]), the request states it with identifier boundaries, it names no observed column,
/// and the host recorded it, exactly or canonically equivalent (the one spelling law, R4 A5),
/// among the values of `field` and of no other column. A sample that never shows it, or a column
/// whose values the host did not record, proves nothing: no witness, and the question stays.
pub(crate) fn witness(
    rule: &Rule,
    field: &str,
    intent: &str,
    row: Option<&Value>,
    seen: &Seen,
) -> Option<String> {
    let recorded = row?.get("values")?.as_object()?;
    let spellings = |values: &Value| -> Vec<String> {
        let texts = values.as_array().into_iter().flatten();
        texts.filter_map(Value::as_str).map(str::to_owned).collect()
    };
    let holds = |values: &Value, literal: &str| {
        let observed = spellings(values);
        observed.iter().any(|v| v == literal)
            || !crate::surface::observed::equivalent_spellings(literal, &observed).is_empty()
    };
    let own = recorded.get(field)?;
    let compared: Vec<String> = match rule.verified_program() {
        Some(program) => (spellings(own).into_iter())
            .filter(|value| compares(&program.jq, field, value))
            .collect(),
        None => (rule.text_equalities().into_iter())
            .filter_map(|(key, literal)| (key == field).then_some(literal))
            .collect(),
    };
    compared.into_iter().find(|literal| {
        super::names_field(intent, literal)
            && !seen.all.iter().any(|column| column == literal)
            && holds(own, literal)
            && (recorded.iter())
                .filter(|(column, _)| column.as_str() != field)
                .all(|(_, values)| !holds(values, literal))
    })
}
