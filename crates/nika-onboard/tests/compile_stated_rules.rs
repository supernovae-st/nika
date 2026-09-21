// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A rule the request states in words is the code the workflow runs: a filter ("keep only
//! the tickets whose status is open", "ne garde que les lignes dont amount dépasse 200") or
//! an aggregate over a column ("the total of the amount column") is admitted by the
//! deterministic door with zero questions and zero seat calls, and lowered to the jq the
//! provenance records. A rule the request does not state (no predicate, no column, a
//! prohibition, an unstructured source) is still asked or refused, never guessed.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_onboard::compile::{
    AuthoringCognition, CompileOutcome, CompileRequest, CompileStatus, Strategy, compile,
};
use serde_json::Value;

fn ready(intent: &str) -> (CompileOutcome, Value) {
    let out = compile(&CompileRequest::create(intent)).unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(out.provenance.strategy, Some(Strategy::Hot), "{out:#?}");
    assert_eq!(
        out.provenance.cognition,
        AuthoringCognition::DeterministicOnly
    );
    assert!(out.questions.is_empty(), "{out:#?}");
    assert!(
        out.check_preview.as_ref().unwrap().report.is_clean(),
        "{out:#?}"
    );
    let doc: Value = serde_yaml_bw::from_str(out.candidate.as_deref().unwrap()).unwrap();
    (out, doc)
}

fn keys(out: &CompileOutcome) -> Vec<&str> {
    out.questions.iter().map(|q| q.key.as_str()).collect()
}

fn expression(doc: &Value, task: &str) -> String {
    let jq = doc["tasks"][task]["invoke"]["args"]["expression"].as_str();
    assert!(jq.is_some(), "no jq on `{task}`: {doc:#}");
    jq.unwrap_or_default().to_owned()
}

fn rule_jq(out: &CompileOutcome) -> String {
    out.provenance.decision.as_ref().unwrap()["rule"]["jq"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn a_stated_filter_over_a_json_source_is_the_code_the_workflow_runs() {
    let intent = "Read ./tickets.json, keep only the tickets whose status is open, and write them to ./open.json";
    let (out, doc) = ready(intent);
    assert_eq!(
        expression(&doc, "compute"),
        "[.records[] | select(.status == \"open\")]"
    );
    assert_eq!(rule_jq(&out), expression(&doc, "compute"));
    assert_eq!(expression(&doc, "parse_source"), "fromjson");
    // The rows the rule kept are what the write carries; no model, no draft, no question.
    assert_eq!(
        doc["tasks"]["write_output"]["with"]["content"],
        "${{ tasks.compute.output }}"
    );
    assert!(doc["tasks"].get("draft").is_none(), "{doc:#}");
    assert!(doc.get("model").is_none(), "{doc:#}");
    assert_eq!(doc["const"]["output_path"], "./open.json");
    // The guard names the column the rule reads, so a wrong header fails loudly.
    assert!(
        expression(&doc, "compute_guard").contains("has(\"status\")"),
        "{doc:#}"
    );
}

#[test]
fn a_stated_aggregate_over_a_csv_column_is_the_code_the_workflow_runs() {
    let intent = "Read ./sales.csv, compute the total of the amount column and write the total to ./total.txt";
    let (out, doc) = ready(intent);
    assert_eq!(
        expression(&doc, "compute"),
        ".records | {\"total\": (map(.amount | tonumber) | add // 0)}"
    );
    assert_eq!(rule_jq(&out), expression(&doc, "compute"));
    assert_eq!(
        doc["tasks"]["parse_source"]["invoke"]["args"]["from"],
        "csv"
    );
    assert_eq!(doc["outputs"]["total"], "${{ tasks.compute.output.total }}");
    // "write the total": one total over every row goes to the prose file as the value
    // itself, not as the one-key object that carries it.
    assert_eq!(
        doc["tasks"]["write_output"]["with"]["content"],
        "${{ tasks.compute.output.total }}"
    );
    assert!(doc.get("model").is_none(), "{doc:#}");

    let intent =
        "Read ./sales.csv, compute the average of the amount column and write it to ./avg.txt";
    let (_, doc) = ready(intent);
    assert_eq!(
        expression(&doc, "compute"),
        ".records | {\"average\": (if length == 0 then 0 else ((map(.amount | tonumber) | add) / length) end)}"
    );
    assert_eq!(
        doc["tasks"]["write_output"]["with"]["content"],
        "${{ tasks.compute.output.average }}"
    );
}

#[test]
fn a_french_restriction_is_a_filter_and_a_csv_write_keeps_the_header_order() {
    let intent = "Lis ./sales.csv, ne garde que les lignes dont amount dépasse 200 et écris-les dans ./big.csv";
    let (_, doc) = ready(intent);
    assert_eq!(
        expression(&doc, "compute"),
        "[.records[] | select((.amount | tonumber) > 200)]"
    );
    // CSV in, CSV out: the source's own header order feeds the conversion stage.
    assert!(doc["tasks"].get("source_columns").is_some(), "{doc:#}");
    assert_eq!(
        doc["tasks"]["big_csv"]["invoke"]["args"]["columns"],
        "${{ with.columns }}"
    );
    assert_eq!(
        doc["tasks"]["big_csv"]["with"]["data"],
        "${{ tasks.compute.output }}"
    );
    assert_eq!(
        doc["tasks"]["write_output"]["with"]["content"],
        "${{ tasks.big_csv.output }}"
    );
}

#[test]
fn a_filter_and_a_total_in_one_request_run_as_one_computation() {
    let intent = "Read ./sales.csv, keep only the rows whose client is acme, compute the total of the amount column and write the total to ./total.txt";
    let (_, doc) = ready(intent);
    assert_eq!(
        expression(&doc, "compute"),
        "[.records[] | select(.client == \"acme\")] | {\"total\": (map(.amount | tonumber) | add // 0)}"
    );
    assert_eq!(
        doc["tasks"]["write_output"]["with"]["content"],
        "${{ tasks.compute.output.total }}"
    );
}

#[test]
fn a_rule_the_request_does_not_state_is_asked_or_refused_never_guessed() {
    // No predicate: which tickets? The reader has no operation for the bare instruction.
    let out = compile(&CompileRequest::create(
        "Read ./tickets.json, keep the tickets, and write them to ./open.json",
    ))
    .unwrap();
    assert_ne!(out.status, CompileStatus::Ready);
    assert_eq!(keys(&out), ["intent.clarification"], "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    // A vague predicate ("the open ones") names no column and no comparison.
    let out = compile(&CompileRequest::create(
        "Read ./tickets.json, keep only the open ones, and write them to ./open.json",
    ))
    .unwrap();
    assert_eq!(keys(&out), ["intent.clarification"], "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    // No column: "the total" of what? The computation is admitted, its expression asked.
    let out = compile(&CompileRequest::create(
        "Read ./sales.csv, compute the total and write it to ./total.txt",
    ))
    .unwrap();
    assert_eq!(keys(&out), ["const.rule_expression"], "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    // An unstructured source has no records for the rule to run over: asked, not guessed.
    let out = compile(&CompileRequest::create(
        "Read ./tickets.md, keep only the tickets whose status is open, and write them to ./open.md",
    ))
    .unwrap();
    assert_eq!(keys(&out), ["const.rule_expression"], "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    // A prohibition is a rule the prose obeys, never a filter the workflow runs, and never
    // its complement: no candidate, in either language.
    for intent in [
        "Read ./tickets.json, never keep closed tickets, and write them to ./open.json",
        "Read ./tickets.json, do not keep the tickets whose status is closed, and write them to ./open.json",
        "Lis ./sales.csv, ne garde pas les lignes dont amount dépasse 200 et écris-les dans ./big.csv",
    ] {
        let out = compile(&CompileRequest::create(intent)).unwrap();
        assert_ne!(out.status, CompileStatus::Ready, "{intent}: {out:#?}");
        assert_eq!(keys(&out), ["intent.clarification"], "{intent}: {out:#?}");
        assert!(out.candidate.is_none(), "{intent}: {out:#?}");
    }
}
