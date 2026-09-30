// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Law 24: a path read on one record names a field the observed records carry. A key of the jq
//! input document read inside the iteration over its records reads null there: a window compared
//! against it keeps nothing and the total is silently 0, or a conversion of it fails at Run (both
//! regressions rebuilt here on synthetic readings and visits). A
//! field the records carry, a document bound before the iteration and a read at document level
//! stay admitted; without an observation of the records nothing is judged.
use nika_compile_fidelity::fidelity::{Diagnostic, laws_observed};
use nika_compile_reader::plan::Plan;
use serde_json::{Map, Value, json};

const READINGS: &str = "./data/readings.csv";
const VISITS: &str = "./data/visits.json";
const INTENT: &str = "Total the celsius of ./data/readings.csv and the minutes of ./data/visits.json between the two bounds.";

/// Whether `path` names a CSV file (the readings); the visits are JSON.
fn is_csv(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("csv"))
}

/// The host's observation of one file: its row (with the columns it shows) and its kinds.
fn observed(path: &str, columns: &[&str]) -> Value {
    let kind = if is_csv(path) { "csv" } else { "json" };
    let row = json!({"path": path, "state": "observed", "complete": false, "kind": kind, "columns": columns});
    let mut kinds = Map::new();
    kinds.insert(path.to_owned(), json!({"sampled": 3, "keys": {}}));
    json!({"observed": [row], "kinds": kinds})
}

/// A candidate reading `path` through a const, turning it into records (`nika:convert` for a
/// CSV, a `fromjson` `nika:jq` for JSON), then `compute` over `input` with `expression`.
fn candidate(path: &str, input: &Value, expression: &str) -> Value {
    let parse = if is_csv(path) {
        json!({"with": {"text": "${{ tasks.read_source.output }}"},
            "invoke": {"tool": "nika:convert", "args": {"input": "${{ with.text }}", "from": "csv", "to": "json"}}})
    } else {
        json!({"with": {"text": "${{ tasks.read_source.output }}"},
            "invoke": {"tool": "nika:jq", "args": {"input": "${{ with.text }}", "expression": "fromjson"}}})
    };
    json!({
        "nika": "synthetic-window",
        "const": {"source_path": path, "since": "2031-03-01T00:00:00Z", "until": "2031-03-02T00:00:00Z"},
        "permits": {"fs": {"read": [path]}, "tools": ["nika:read", "nika:convert", "nika:jq"]},
        "tasks": {
            "read_source": {"invoke": {"tool": "nika:read", "args": {"path": "${{ const.source_path }}"}}},
            "parse_source": parse,
            "compute": {
                "with": {"rows": "${{ tasks.parse_source.output }}"},
                "invoke": {"tool": "nika:jq", "args": {"input": input, "expression": expression}}
            }
        }
    })
}

/// An input document holding the records beside the two bounds.
fn windowed(first: &str, second: &str) -> Value {
    let mut input = Map::new();
    input.insert("rows".to_owned(), json!("${{ with.rows }}"));
    input.insert(first.to_owned(), json!("${{ const.since }}"));
    input.insert(second.to_owned(), json!("${{ const.until }}"));
    Value::Object(input)
}

/// The record-scope findings of the laws over `doc` with `world`.
fn scope_findings(doc: &Value, world: Option<&Value>) -> Vec<String> {
    let mut out: Vec<Diagnostic> = Vec::new();
    laws_observed(
        INTENT,
        &Plan::default(),
        doc,
        &[],
        &[],
        &[],
        world,
        &mut out,
    );
    out.into_iter()
        .filter(|d| d.message.starts_with("RECORD SCOPE"))
        .map(|d| d.message)
        .collect()
}

const WINDOW_EXPRESSION: &str = "[ .rows[] | select(.taken_at >= .since and .taken_at < .until) ] | map(.celsius | tonumber) | add // 0";

/// Both bounds are read on one record of a CSV's records; one finding names them both,
/// the file, its columns and the binding that reads them from the document.
#[test]
fn a_bound_read_on_one_record_is_a_record_scope_finding() {
    let world = observed(READINGS, &["reading_id", "taken_at", "celsius"]);
    let doc = candidate(READINGS, &windowed("since", "until"), WINDOW_EXPRESSION);
    let found = scope_findings(&doc, Some(&world));
    assert_eq!(found.len(), 1, "{found:?}");
    let message = &found[0];
    for part in [
        "the task `compute` reads `.since` and `.until` on one record of `.rows[]`",
        "`./data/readings.csv`",
        "reading_id, taken_at, celsius",
        "`since` and `until` are keys of the jq input document",
        "`$doc.since` and `$doc.until`",
    ] {
        assert!(message.contains(part), "missing {part:?} in {message}");
    }
}

/// JSON records through `fromjson`, bounds converted with `fromdateiso8601` on the
/// record (null there: a Run error). The conversion does not hide the scope.
#[test]
fn the_json_records_through_fromjson_are_judged_too() {
    let world = observed(VISITS, &["visit_id", "arrived_at", "minutes"]);
    let expression = "{ total: ([.rows[] | select((.arrived_at | fromdateiso8601) >= (.from | fromdateiso8601)) | select((.arrived_at | fromdateiso8601) < (.to | fromdateiso8601)) | .minutes] | add // 0) }";
    let doc = candidate(VISITS, &windowed("from", "to"), expression);
    let found = scope_findings(&doc, Some(&world));
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].contains("reads `.from` and `.to` on one record of `.rows[]`")
            && found[0].contains("`./data/visits.json`"),
        "{}",
        found[0]
    );
}

/// `map` takes its filter per record: the same read inside it is the same finding.
#[test]
fn a_map_over_the_records_reads_one_record() {
    let world = observed(READINGS, &["reading_id", "taken_at", "celsius"]);
    let doc = candidate(
        READINGS,
        &windowed("since", "until"),
        ".rows | map(select(.taken_at >= .since)) | length",
    );
    let found = scope_findings(&doc, Some(&world));
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].contains("reads `.since` on one record")
            && found[0].contains("`since` is a key of the jq input document")
            && found[0].contains("`$doc.since`"),
        "{}",
        found[0]
    );
}

/// Neighbour: the records carry a field of the same name, so reading it on a record is theirs.
#[test]
fn a_field_the_records_carry_stays_admitted() {
    let world = observed(READINGS, &["reading_id", "taken_at", "celsius", "since"]);
    let doc = candidate(
        READINGS,
        &windowed("since", "until"),
        "[ .rows[] | select(.taken_at >= .since) ] | length",
    );
    assert_eq!(scope_findings(&doc, Some(&world)), Vec::<String>::new());
}

/// Neighbour: the document bound before the iteration and read through its variable.
#[test]
fn a_document_bound_before_the_iteration_stays_admitted() {
    let world = observed(READINGS, &["reading_id", "taken_at", "celsius"]);
    let expression = ". as $doc | [ .rows[] | select(.taken_at >= $doc.since and .taken_at < $doc.until) ] | length";
    let doc = candidate(READINGS, &windowed("since", "until"), expression);
    assert_eq!(scope_findings(&doc, Some(&world)), Vec::<String>::new());
}

/// Neighbour: the bounds read at document level, before the iteration, then used on records.
#[test]
fn a_bound_read_at_document_level_stays_admitted() {
    let world = observed(READINGS, &["reading_id", "taken_at", "celsius"]);
    let expression = ".since as $since | .until as $until | [ .rows[] | select(.taken_at >= $since and .taken_at < $until) ] | length";
    let doc = candidate(READINGS, &windowed("since", "until"), expression);
    assert_eq!(scope_findings(&doc, Some(&world)), Vec::<String>::new());
}

/// Without an observation of the records (none, or another file), nothing is judged.
#[test]
fn without_an_observation_of_the_records_nothing_is_judged() {
    let doc = candidate(READINGS, &windowed("since", "until"), WINDOW_EXPRESSION);
    assert_eq!(scope_findings(&doc, None), Vec::<String>::new());
    let elsewhere = observed("./data/other.csv", &["reading_id"]);
    assert_eq!(scope_findings(&doc, Some(&elsewhere)), Vec::<String>::new());
}
