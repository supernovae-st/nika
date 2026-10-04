// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::semantic::{BOUND_BY, facts, literal_keys};
use super::*;
use serde_json::json;

/// One observed JSON source, sampled: `columns` seen, `common` in every sampled record.
fn observed(path: &str, columns: &[&str], common: &[&str]) -> Value {
    json!({"path": path, "state": "observed", "kind": "json", "columns": columns,
           "common_columns": common, "complete": false, "peek_sha256": "a".repeat(64)})
}

fn world(rows: &[Value]) -> Value {
    json!({"observed": rows, "kinds": {}})
}

#[test]
fn the_descended_law_grades_as_it_did() {
    let w = world(&[
        observed("./inventory.json", &["sku", "stock"], &["sku"]),
        json!({"path": "people.csv", "state": "observed", "kind": "csv", "columns": ["name"]}),
        json!({"path": "gone.json", "state": "absent"}),
    ]);
    let row_of = |p| row(Some(&w), p);
    assert!(row_of("inventory.json").is_some() && row_of("./people.csv").is_some());
    assert!(seen(row_of("gone.json")).is_none());
    let sampled = seen(row_of("inventory.json")).expect("seen");
    assert_eq!(
        grade("sku", Some(&sampled), &[]),
        (Grade::ObservedPartial, true)
    );
    assert_eq!(
        grade("stock", Some(&sampled), &[]),
        (Grade::ObservedPartial, false)
    );
    // A partial sample disproves nothing; a header does.
    assert_eq!(
        grade("x", Some(&sampled), &["x".into()]),
        (Grade::UserAsserted, true)
    );
    let header = seen(row_of("people.csv")).expect("seen");
    assert_eq!(
        grade("x", Some(&header), &["x".into()]),
        (Grade::Inferred, false)
    );
    assert_eq!(grade("name", Some(&header), &[]), (Grade::Declared, true));
    assert_eq!(revision(row_of("inventory.json")), "a".repeat(64));
    assert_eq!(revision(row_of("gone.json")), "absent");
    assert_eq!(revision(None), "unobserved");
    // Two rows of one path are no row.
    let twice = world(&[observed("a.json", &[], &[]), observed("./a.json", &[], &[])]);
    assert!(row(Some(&twice), "a.json").is_none());
    assert!(compares(r#"select(.status == "open")"#, "status", "open"));
    assert!(!compares(
        r#"select(.x.status == "open")"#,
        "status",
        "open"
    ));
}

#[test]
fn an_entry_records_exactly_the_historical_shape() {
    let r = observed("inventory.json", &["stock"], &[]);
    let entry = Entry {
        rule: "r",
        key: "stock",
        source: "inventory.json",
        row: Some(&r),
        grade: Grade::ObservedPartial,
        everywhere: false,
        bound_by: Some("request"),
    };
    assert_eq!(
        entry.to_json(),
        json!({"rule": "r", "field": "stock", "source": "inventory.json",
               "revision": "a".repeat(64), "grade": "observed_partial",
               "in_every_sampled_record": false, "bound_by": "request", "admissible": true,
               "open": "records lacking the key"})
    );
}

#[test]
fn literal_reads_are_recognized_and_lookalikes_are_not() {
    let keys = |jq: &str| literal_keys(jq);
    assert_eq!(
        keys(r#"fromjson | map(select(.stock < 8)) | sort_by(."sku")"#),
        ["stock", "sku"]
    );
    assert_eq!(
        keys("map({sku, stock, reorder_qty: (12 - .stock)})"),
        ["sku", "stock", "stock"]
    );
    assert_eq!(keys(r#""\(.stock) left""#), ["stock"]);
    // Nested, computed, variable, numeric, string, comment and recursive forms never read `stock`
    // (a nested path reads only its first key).
    for jq in [
        ".meta.stock",
        ".items[].stock",
        ".[$k]",
        r#".["stock"]"#,
        "$row.stock",
        "1.5",
        r#""the .stock level""#,
        "# .stock\n.",
        "..",
        "{$stock}",
        "{stock: 1}",
        r#"."st\u006fck""#,
    ] {
        assert!(
            !keys(jq).iter().any(|k| k == "stock"),
            "{jq}: {:?}",
            keys(jq)
        );
    }
    assert_eq!(keys(".meta.stock"), ["meta"]);
    // A longer key is another key.
    assert_eq!(keys(".stock_level"), ["stock_level"]);
}

/// A semantic record over `reads` whose one jq fill is `jq`.
fn record(reads: &[&str], jq: &str) -> Value {
    json!({"semantic_record": 1, "sketch": {"tasks": [
        {"id": "read", "verb": "invoke", "tool": "nika:read", "reads": reads},
        {"id": "pick", "verb": "invoke", "tool": "nika:jq"}]},
        "fills": [{"task": "pick", "field": "expression", "value": jq},
                  {"task": "pick", "field": "prompt", "value": ".sku"}]})
}

#[test]
fn a_semantic_candidate_records_only_observed_keys_it_reads_literally() {
    let w = world(&[observed(
        "inventory.json",
        &["sku", "stock", "name"],
        &["sku", "stock"],
    )]);
    let jq =
        "fromjson | map(select(.stock < 8)) | map({sku, reorder_qty: (12 - .stock), x: .nope})";
    let got = facts(&record(&["inventory.json"], jq), Some(&w));
    let fields: Vec<&str> = got.iter().filter_map(|f| f["field"].as_str()).collect();
    // Observation order; `name` is never read, `nope` never observed, a prompt is no program.
    assert_eq!(fields, ["sku", "stock"]);
    for fact in &got {
        assert_eq!(fact["source"], "inventory.json");
        assert_eq!(fact["bound_by"], BOUND_BY);
        assert_eq!(fact["grade"], "observed_partial");
        assert_eq!(fact["in_every_sampled_record"], true);
        assert_eq!(fact["revision"], "a".repeat(64));
    }
    // A bare stated name the observation places, under its directory.
    let placed = world(&[observed("data/inventory.json", &["stock"], &["stock"])]);
    let got = facts(&record(&["inventory.json"], ".stock"), Some(&placed));
    assert_eq!(got.len(), 1);
    assert_eq!(got[0]["source"], "data/inventory.json");
}

#[test]
fn an_unresolved_source_or_unrecognized_read_records_nothing() {
    let one = world(&[observed("inventory.json", &["stock"], &["stock"])]);
    let none = |rec: Value, w: &Value| assert!(facts(&rec, Some(w)).is_empty(), "{rec}");
    // Computed only, a key read from no declared source, a glob, no world, an unobserved file.
    none(record(&["inventory.json"], ".[$k]"), &one);
    none(record(&[], ".stock"), &one);
    none(record(&["*.json"], ".stock"), &one);
    assert!(facts(&record(&["inventory.json"], ".stock"), None).is_empty());
    let absent = world(&[json!({"path": "inventory.json", "state": "absent"})]);
    none(record(&["inventory.json"], ".stock"), &absent);
    // Two files under one bare name, or two rows of one path: the request does not say which.
    let two = world(&[
        observed("a/inventory.json", &["stock"], &["stock"]),
        observed("b/inventory.json", &["stock"], &["stock"]),
    ]);
    none(record(&["inventory.json"], ".stock"), &two);
    let rows = world(&[
        observed("inventory.json", &["stock"], &[]),
        observed("./inventory.json", &["stock"], &[]),
    ]);
    none(record(&["inventory.json"], ".stock"), &rows);
}

#[test]
fn every_declared_source_showing_a_read_key_records_it() {
    let w = world(&[
        observed("a.json", &["stock"], &["stock"]),
        observed("b.json", &["stock", "sku"], &["stock"]),
    ]);
    let got = facts(&record(&["a.json", "b.json", "a.json"], ".stock"), Some(&w));
    let pairs: Vec<(&str, &str)> = (got.iter())
        .filter_map(|f| Some((f["source"].as_str()?, f["field"].as_str()?)))
        .collect();
    assert_eq!(pairs, [("a.json", "stock"), ("b.json", "stock")]);
    // A key some sampled records lack is recorded as such, never as everywhere.
    let partial = world(&[observed("a.json", &["stock"], &[])]);
    let got = facts(&record(&["a.json"], ".stock"), Some(&partial));
    assert_eq!(got[0]["in_every_sampled_record"], false);
}
