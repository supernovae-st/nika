// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Stated numbers stay exact through generated workflows (R4 A8). Root's public counterexample
//! (`a5-decimal-order-root`): « keep the top 2 rows by points » over the texts
//! 1.000000000000000001 / …002 / …003 checked, ran and wrote five wrong orders of six, because
//! every number crossed jq's f64 and the JSON transport between tasks carries a JSON number as an
//! f64. The one runtime jq now runs exact decimal laws the compile emits (`laws/order.jq`), and a
//! JSON source's numbers keep their exact value past the decode, or the run stops before any
//! effect naming the one that would not.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, CompileStatus, compile};
use serde_json::Value;

mod common;

const SUPPORT: &str =
    "Route support tickets, look up the customer, draft a reply, and ask me before any refund";

/// The candidate a request compiles to over the observed files, READY.
fn ready(intent: &str, files: &[(&str, &[&str])]) -> Value {
    let request = CompileRequest::create(intent).with_knowledge(common::observed(files));
    let out = compile(&request).unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    serde_yaml_bw::from_str(out.candidate.as_deref().unwrap()).unwrap()
}

fn expression(doc: &Value, task: &str) -> String {
    doc["tasks"][task]["invoke"]["args"]["expression"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

/// The laws a guarded expression carries in front of it: the readable order laws, whole.
fn carries_the_order_laws(expression: &str) {
    assert!(
        expression.starts_with("# Exact decimal order laws (R4 A8)"),
        "{expression}"
    );
    for law in [
        "def dkey:",
        "def _dkept:",
        "def dguard($fields):",
        "def dtie(",
    ] {
        assert!(expression.contains(law), "{law} missing: {expression}");
    }
}

#[test]
fn a_json_source_is_decoded_with_every_number_it_may_write_kept_exact() {
    let doc = ready(
        "Read ./rows.json and write it to ./copy.csv",
        &[("./rows.json", &["name", "points", "id", "weight"])],
    );
    let parse = expression(&doc, "parse_source");
    carries_the_order_laws(&parse);
    // Every row is written whole: every number of the source is in the guard's scope.
    assert!(parse.ends_with("\nfromjson | dguard(null)"), "{parse}");
}

#[test]
fn a_csv_source_is_converted_without_a_guard_its_cells_are_text() {
    let doc = ready(
        "Read ./rows.csv, keep only the rows whose status is open and write them to ./open.csv",
        &[("./rows.csv", &["name", "status"])],
    );
    assert_eq!(
        doc["tasks"]["parse_source"]["invoke"]["tool"],
        "nika:convert"
    );
    assert!(expression(&doc, "parse_source").is_empty());
}

#[test]
fn each_json_file_a_join_decodes_is_guarded_apart() {
    let doc = ready(
        "Read ./a.json and ./b.json, merge them on the id column and write the result to ./merged.json",
        &[("./a.json", &["id", "x"]), ("./b.json", &["id", "y"])],
    );
    assert_eq!(
        doc["tasks"]["parse_source"]["for_each"]["items"],
        "${{ with.texts }}"
    );
    let parse = expression(&doc, "parse_source");
    carries_the_order_laws(&parse);
    assert!(parse.ends_with("\nfromjson | dguard(null)"), "{parse}");
}

#[test]
fn the_record_the_support_lookup_selects_is_guarded() {
    let request = CompileRequest::create(SUPPORT)
        .answer("model", r#""mock/echo""#)
        .answer("const.customer_directory", r#""customers.json""#)
        .answer(
            "const.refund_endpoint",
            r#""https://refund.example.invalid/refunds""#,
        )
        .answer(
            "const.refund_policy",
            r#"{"cap":100,"currency":"EUR","criteria":"unused purchase within 14 days"}"#,
        );
    let out = compile(&request).unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let doc: Value = serde_yaml_bw::from_str(out.candidate.as_deref().unwrap()).unwrap();
    let pick = expression(&doc, "lookup_customer");
    carries_the_order_laws(&pick);
    assert!(
        pick.ends_with(
            "\n. as $lookup | ($lookup.directory | fromjson)[$lookup.id] | dguard(null)"
        ),
        "{pick}"
    );
}

const ROWS: (&str, &[&str]) = ("./rows.json", &["name", "team", "points", "id", "weight"]);

/// The scope the decode's guard carries for `intent` over the rows: `null` or the field list.
fn guard_scope(intent: &str) -> String {
    let parse = expression(&ready(intent, &[ROWS]), "parse_source");
    carries_the_order_laws(&parse);
    let (_, scope) = parse
        .rsplit_once("\nfromjson | dguard(")
        .expect("the decode is guarded");
    scope.trim_end_matches(')').to_owned()
}

#[test]
fn a_sole_rule_that_fixes_what_it_writes_guards_only_the_fields_it_reads() {
    // A projection writes its named columns: a precise payload it drops never stops the run.
    assert_eq!(
        guard_scope(
            "Read ./rows.json, keep only the columns name and points and write them to ./names.json"
        ),
        r#"["name","points"]"#
    );
    // Totals write only what they compute from the fields they read.
    assert_eq!(
        guard_scope(
            "Read ./rows.json, compute the total of the points column and write the total to ./total.txt"
        ),
        r#"["points"]"#
    );
    // A group writes its key and what it computes.
    assert_eq!(
        guard_scope(
            "Read ./rows.json, compute the total of the points column per team and write it to ./teams.json"
        ),
        r#"["team","points"]"#
    );
}

#[test]
fn whole_rows_or_the_records_themselves_guard_every_number() {
    for intent in [
        // A rank or a filter writes whole rows: any field of a kept row reaches the output.
        "Read ./rows.json, keep the top 2 rows by points and write them to ./top.json",
        "Read ./rows.json, keep only the rows whose points is above 1.5 and write them to ./above.json",
        // No rule: the records themselves are written.
        "Read ./rows.json and write it to ./copy.csv",
        // An endpoint payload names every fact, the records included.
        "Read ./rows.json, compute the total of the points column, write it to ./total.txt and post it to http://127.0.0.1:18471/hook",
    ] {
        assert_eq!(guard_scope(intent), "null", "{intent}");
    }
}

/// Root's rank (public counterexample `a5-decimal-order-root`).
const RANK: &str = "Read ./rows.json, keep the top 2 rows by points and write them to ./top.json";

/// The plan the frozen A7 binary (`nika 0.121.0 (4c119fb82)`) recorded for [`RANK`], byte for
/// byte: a plan recorded before R4 A8, whose rule keeps the reader's own reading.
const A7_RANK_PLAN: &str = r#"{"bindings": [{"literal": "./top.json", "role": "path"}, {"literal": "./rows.json", "role": "path"}, {"literal": "./top.json", "role": "path"}], "constraints": [], "effects": [{"evidence": "write them to ./top.json", "policy": "automatic", "policy_literal": null, "target": "./top.json", "verb": "write"}], "obligations": [], "observed_world": {"kinds": {"./rows.json": {"keys": {"name": {"text": 3}, "points": {"number_text": 3}}, "sampled": 3}}, "observed": [{"bytes": 147, "columns": ["name", "points"], "common_columns": ["name", "points"], "complete": false, "kind": "json", "path": "./rows.json", "peek_sha256": "c4f0fd7fada4224a40144a9d94645134590844aa32105b5af49f1e34ab3245e1", "state": "observed"}, {"complete": false, "path": "./top.json", "state": "absent"}]}, "operations": [{"categories": [], "detail": "./rows.json", "evidence": "Read ./rows.json", "op": "read"}, {"categories": [], "detail": "keep the top 2 rows by points", "evidence": "keep the top 2 rows by points", "op": "compute"}], "rules": [{"clauses": [], "fields": ["points"], "jq": ".records | sort_by(.points | tonumber? // .) | reverse | .[:2]", "junction": "and", "lines": false, "program": null, "shape": {"aggregations": [], "columns": [], "derived": [], "descending": true, "distinct": false, "distinct_by": [], "group_by": null, "join_on": null, "limit": 2, "renames": [], "sort_by": "points"}, "summary": false, "synthesized": true, "text": "keep the top 2 rows by points"}], "slots": [], "strategy": "hot", "trigger": null, "unknowns": []}"#;

/// The rule a READY candidate's compute runs, after the laws in front of it.
fn rule_after_laws(doc: &Value) -> String {
    let compute = expression(doc, "compute");
    carries_the_order_laws(&compute);
    compute.rsplit('\n').next().unwrap_or_default().to_owned()
}

#[test]
fn a_bound_rank_orders_by_the_exact_key_and_checks_its_cut() {
    let request = CompileRequest::create(RANK).with_knowledge(common::observed(&[ROWS]));
    let out = compile(&request).unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let doc: Value = serde_yaml_bw::from_str(out.candidate.as_deref().unwrap()).unwrap();
    let rule = common::short(&rule_after_laws(&doc));
    assert_eq!(
        rule,
        ".records | sort_by((.points | num) | dkey) | reverse | dtie(2; (.points | num) | dkey; .; \"`points`\") | .[:2]"
    );
    // The plan keeps the reader's own reading, byte for byte what A7 recorded: continuation.
    let recorded: Value = serde_json::from_str(A7_RANK_PLAN).unwrap();
    assert_eq!(
        out.provenance.plan.as_ref().unwrap()["rules"][0]["jq"],
        recorded["rules"][0]["jq"]
    );
}

#[test]
fn a_plan_recorded_before_a8_replays_ready_and_binds_the_exact_laws() {
    let plan: Value = serde_json::from_str(A7_RANK_PLAN).unwrap();
    let out = compile(&CompileRequest::create(RANK).with_plan(plan)).unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let doc: Value = serde_yaml_bw::from_str(out.candidate.as_deref().unwrap()).unwrap();
    let rule = common::short(&rule_after_laws(&doc));
    assert!(rule.contains("sort_by((.points | num) | dkey)"), "{rule}");
    assert!(rule.contains("| dtie(2; (.points | num) | dkey;"), "{rule}");
}

#[test]
fn a_bound_comparison_reads_the_literal_as_the_request_states_it() {
    let doc = ready(
        "Read ./rows.json, keep only the rows whose points is above 1.000000000000000002 and write them to ./above.json",
        &[ROWS],
    );
    assert_eq!(
        common::short(&rule_after_laws(&doc)),
        "[.records[] | select(((.points | num) | dkey) > (\"1.000000000000000002\" | dkey))]"
    );
}

#[test]
fn a_rule_that_reads_no_number_carries_no_law() {
    let doc = ready(
        "Read ./rows.json, keep only the rows whose team is x and write them to ./x.json",
        &[ROWS],
    );
    let compute = expression(&doc, "compute");
    assert!(!compute.contains("def dkey"), "{compute}");
    assert!(compute.starts_with("[.records[] | select("), "{compute}");
}
