// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Law 25: date-times with offsets are compared as instants, never as text. Text order is time
//! order only when every value and every bound share one offset and one form: a boundary row
//! written `+02:00` sorts after an hour's upper bound it lies inside (a first-hour
//! total of 0 instead of 8, rebuilt here on synthetic readings). The
//! values are the host's: the categorical values of the observed row, or the instants recorded
//! in the kinds entry of the path. One offset and one form against a bound of the same offset,
//! a comparison of converted instants, a date-only bound and a field that holds no dates stay
//! admitted.
use nika_compile_fidelity::fidelity::{Diagnostic, laws_observed};
use nika_compile_reader::plan::Plan;
use serde_json::{Map, Value, json};

const READINGS: &str = "./data/readings.csv";
const INTENT: &str =
    "Total the celsius of ./data/readings.csv taken in the first hour of 2031-03-01.";

/// The host's observation of the readings: the row with `values` (the categorical values it
/// recorded) and the kinds entry with `instants` (the date-time evidence it recorded), each
/// omitted when null.
fn observed(values: &Value, instants: &Value) -> Value {
    let mut row = json!({"path": READINGS, "state": "observed", "complete": false, "kind": "csv",
        "columns": ["reading_id", "taken_at", "celsius", "label"]});
    if !values.is_null() {
        row["values"] = values.clone();
    }
    let mut entry = json!({"sampled": 3, "keys": {"taken_at": {"text": 3}}});
    if !instants.is_null() {
        entry["instants"] = instants.clone();
    }
    let mut kinds = Map::new();
    kinds.insert(READINGS.to_owned(), entry);
    json!({"observed": [row], "kinds": kinds})
}

/// The readings' `taken_at` values, recorded as categorical values.
fn values(taken_at: &[&str]) -> Value {
    json!({"taken_at": taken_at})
}

/// A candidate reading the CSV, converting it and computing over its records with `expression`.
fn candidate(expression: &str) -> Value {
    json!({
        "nika": "synthetic-first-hour",
        "permits": {"fs": {"read": [READINGS]}, "tools": ["nika:read", "nika:convert", "nika:jq"]},
        "tasks": {
            "read_source": {"invoke": {"tool": "nika:read", "args": {"path": READINGS}}},
            "parse_source": {
                "with": {"text": "${{ tasks.read_source.output }}"},
                "invoke": {"tool": "nika:convert", "args": {"input": "${{ with.text }}", "from": "csv", "to": "json"}}
            },
            "compute": {
                "with": {"rows": "${{ tasks.parse_source.output }}"},
                "invoke": {"tool": "nika:jq", "args": {"input": {"rows": "${{ with.rows }}"}, "expression": expression}}
            }
        }
    })
}

/// The instant-order findings of the laws over `expression` with `world`.
fn order_findings(expression: &str, world: &Value) -> Vec<String> {
    let mut out: Vec<Diagnostic> = Vec::new();
    let doc = candidate(expression);
    laws_observed(
        INTENT,
        &Plan::default(),
        &doc,
        &[],
        &[],
        &[],
        Some(world),
        &mut out,
    );
    out.into_iter()
        .filter(|d| d.message.starts_with("TEXT ORDER ON INSTANTS"))
        .map(|d| d.message)
        .collect()
}

/// The first hour as text bounds in UTC.
const FIRST_HOUR_Z: &str = "[ .rows[] | select((.taken_at >= \"2031-03-01T00:00:00Z\") and (.taken_at < \"2031-03-01T01:00:00Z\")) ] | length";

/// The first hour as unzoned text bounds.
const FIRST_HOUR_UNZONED: &str = "[ .rows[] | select((.taken_at >= \"2031-03-01T00:00:00\") and (.taken_at < \"2031-03-01T01:00:00\")) ] | length";

/// Values in two offsets: text order is not time order, whatever the bound.
#[test]
fn text_order_over_mixed_offsets_is_a_finding() {
    let world = observed(
        &values(&["2031-03-01T00:30:00Z", "2031-03-01T02:30:00+02:00"]),
        &Value::Null,
    );
    let found = order_findings(FIRST_HOUR_Z, &world);
    assert_eq!(found.len(), 1, "{found:?}");
    for part in [
        "the task `compute` compares `.taken_at` as text",
        "`./data/readings.csv` carry the offsets +02:00 and Z",
        "(.taken_at | fromdateiso8601)",
    ] {
        assert!(found[0].contains(part), "missing {part:?} in {}", found[0]);
    }
}

/// The same evidence recorded as instants in the kinds entry (three distinct values: no
/// categorical set) is judged the same way.
#[test]
fn mixed_offsets_recorded_as_instants_are_a_finding() {
    let instants =
        json!({"taken_at": {"offsets": ["+02:00", "Z"], "forms": ["9999-99-99T99:99:99"]}});
    let found = order_findings(FIRST_HOUR_Z, &observed(&Value::Null, &instants));
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].contains("carry the offsets +02:00 and Z"),
        "{}",
        found[0]
    );
}

/// Zoned values in one offset against unzoned bounds.
#[test]
fn an_unzoned_bound_against_zoned_values_is_a_finding() {
    let instants = json!({"taken_at": {"offsets": ["+02:00"], "forms": ["9999-99-99T99:99:99"]}});
    let found = order_findings(FIRST_HOUR_UNZONED, &observed(&Value::Null, &instants));
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].contains("\"2031-03-01T00:00:00\"") && found[0].contains("no offset"),
        "{}",
        found[0]
    );
}

/// The reverse: unzoned values against a bound in UTC.
#[test]
fn a_zoned_bound_against_unzoned_values_is_a_finding() {
    let world = observed(
        &values(&["2031-03-01T00:30:00", "2031-03-01T02:30:00"]),
        &Value::Null,
    );
    assert_eq!(order_findings(FIRST_HOUR_Z, &world).len(), 1);
}

/// `Z` and `+00:00` are two offsets: a boundary row written `+00:00` sorts before a `Z` bound
/// of the same instant.
#[test]
fn z_and_plus_zero_are_two_offsets() {
    let world = observed(
        &values(&["2031-03-01T00:00:00+00:00", "2031-03-01T00:45:00Z"]),
        &Value::Null,
    );
    assert_eq!(order_findings(FIRST_HOUR_Z, &world).len(), 1);
}

/// Sorting by a field of mixed offsets orders text, not time.
#[test]
fn sorting_mixed_offsets_as_text_is_a_finding() {
    let world = observed(
        &values(&["2031-03-01T00:30:00Z", "2031-03-01T02:30:00+02:00"]),
        &Value::Null,
    );
    let found = order_findings(".rows | sort_by(.taken_at) | first", &world);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("`sort_by`"), "{}", found[0]);
}

/// Neighbour: one offset and one form, against bounds in that offset and form.
#[test]
fn one_offset_and_one_form_against_the_same_offset_stays_admitted() {
    let world = observed(
        &values(&["2031-03-01T00:30:00Z", "2031-03-01T02:30:00Z"]),
        &Value::Null,
    );
    assert_eq!(order_findings(FIRST_HOUR_Z, &world), Vec::<String>::new());
}

/// Neighbour: instants compared after `fromdateiso8601` are numbers, not text.
#[test]
fn instants_compared_after_conversion_stay_admitted() {
    let world = observed(
        &values(&["2031-03-01T00:30:00Z", "2031-03-01T02:30:00+02:00"]),
        &Value::Null,
    );
    let expression = "[ .rows[] | select((.taken_at | fromdateiso8601) >= (\"2031-03-01T00:00:00Z\" | fromdateiso8601)) ] | length";
    assert_eq!(order_findings(expression, &world), Vec::<String>::new());
}

/// Neighbour: a date-only bound against values that share one offset.
#[test]
fn a_date_only_bound_against_one_offset_stays_admitted() {
    let world = observed(
        &values(&["2031-03-01T00:30:00+02:00", "2031-03-01T02:30:00+02:00"]),
        &Value::Null,
    );
    let expression = "[ .rows[] | select(.taken_at >= \"2031-03-01\") ] | length";
    assert_eq!(order_findings(expression, &world), Vec::<String>::new());
}

/// Neighbour: a text field whose values are no date-times.
#[test]
fn a_field_that_holds_no_dates_stays_admitted() {
    let world = observed(&json!({"label": ["alpha", "beta"]}), &Value::Null);
    let expression = "[ .rows[] | select(.label >= \"m\") ] | length";
    assert_eq!(order_findings(expression, &world), Vec::<String>::new());
}
