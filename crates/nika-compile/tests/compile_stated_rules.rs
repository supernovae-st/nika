// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A rule the request states in words is the code the workflow runs: a filter ("keep only
//! the tickets whose status is open", "ne garde que les lignes dont amount dépasse 200") or
//! an aggregate over a column ("the total of the amount column") is admitted by the
//! deterministic door with zero questions and zero seat calls, and lowered to the jq the
//! provenance records. A rule the request does not state (no predicate, no column, a
//! prohibition, an unstructured source) is still asked or refused, never guessed.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{
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
fn a_stated_grouping_counts_the_rows_per_column() {
    let grouped =
        ".records | group_by(.client) | map({\"client\": (.[0] | .client), \"count\": length})";
    let intent =
        "Read ./sales.csv, count the rows per client and write the counts to ./per-client.json";
    let (out, doc) = ready(intent);
    assert_eq!(expression(&doc, "compute"), grouped);
    assert_eq!(rule_jq(&out), expression(&doc, "compute"));
    assert_eq!(
        doc["tasks"]["parse_source"]["invoke"]["args"]["from"],
        "csv"
    );
    // "the counts" refers back to the count: the write carries the grouped rows, no draft.
    assert_eq!(
        doc["tasks"]["write_output"]["with"]["content"],
        "${{ tasks.compute.output }}"
    );
    assert!(doc["tasks"].get("draft").is_none(), "{doc:#}");
    assert!(doc.get("model").is_none(), "{doc:#}");
    assert!(
        expression(&doc, "compute_guard").contains("has(\"client\")"),
        "{doc:#}"
    );
    // The French twin.
    let (_, doc) = ready(
        "Lis ./sales.csv, compte les lignes par client et écris les comptes dans ./per-client.json",
    );
    assert_eq!(expression(&doc, "compute"), grouped);
    // No key: a total count over every row, written to a prose file as the value itself.
    let (_, doc) = ready("Read ./sales.csv, count the rows and write the count to ./count.txt");
    assert_eq!(
        expression(&doc, "compute"),
        ".records | {\"count\": length}"
    );
    assert_eq!(
        doc["tasks"]["write_output"]["with"]["content"],
        "${{ tasks.compute.output.count }}"
    );
}

#[test]
fn a_stated_top_n_and_a_stated_sort_run_as_code_and_write_csv_in_header_order() {
    let intent =
        "Read ./sales.csv, keep the 2 rows with the highest amount and write them to ./top.csv";
    let (out, doc) = ready(intent);
    assert_eq!(
        expression(&doc, "compute"),
        ".records | sort_by(.amount | tonumber? // .) | reverse | .[:2]"
    );
    assert_eq!(rule_jq(&out), expression(&doc, "compute"));
    assert_eq!(
        doc["tasks"]["top_csv"]["invoke"]["args"]["columns"],
        "${{ with.columns }}"
    );
    assert_eq!(
        doc["tasks"]["write_output"]["with"]["content"],
        "${{ tasks.top_csv.output }}"
    );
    assert!(doc.get("model").is_none(), "{doc:#}");
    let (_, doc) = ready(
        "Lis ./sales.csv, garde les 2 lignes au montant le plus élevé et écris-les dans ./top.csv",
    );
    assert_eq!(
        expression(&doc, "compute"),
        ".records | sort_by(.montant | tonumber? // .) | reverse | .[:2]"
    );
    // A sort, then a write whose whole object is the path: the write carries the sorted rows.
    let intent = "Read ./sales.csv, sort the rows by amount descending and write ./sorted.csv";
    let (_, doc) = ready(intent);
    assert_eq!(
        expression(&doc, "compute"),
        ".records | sort_by(.amount | tonumber? // .) | reverse"
    );
    assert_eq!(doc["const"]["source_path"], "./sales.csv");
    assert_eq!(doc["const"]["output_path"], "./sorted.csv");
    assert_eq!(
        doc["tasks"]["sorted_csv"]["with"]["data"],
        "${{ tasks.compute.output }}"
    );
    assert_eq!(
        doc["tasks"]["write_output"]["with"]["content"],
        "${{ tasks.sorted_csv.output }}"
    );
    assert!(doc["tasks"].get("draft").is_none(), "{doc:#}");
}

#[test]
fn a_stated_projection_keeps_only_the_named_fields() {
    let intent = "Read ./tickets.json, keep only the id and title of each ticket and write them to ./slim.json";
    let (out, doc) = ready(intent);
    assert_eq!(
        expression(&doc, "compute"),
        ".records | map({\"id\": .id, \"title\": .title})"
    );
    assert_eq!(rule_jq(&out), expression(&doc, "compute"));
    assert!(
        expression(&doc, "compute_guard").contains("has(\"id\") and has(\"title\")"),
        "{doc:#}"
    );
    assert_eq!(
        doc["tasks"]["write_output"]["with"]["content"],
        "${{ tasks.compute.output }}"
    );
    assert!(doc.get("model").is_none(), "{doc:#}");
}

#[test]
fn a_stated_removal_of_duplicate_lines_runs_over_the_lines_of_a_text_file() {
    let intent =
        "Read ./emails.txt, remove the duplicate lines and write the unique ones to ./unique.txt";
    let (out, doc) = ready(intent);
    assert_eq!(
        expression(&doc, "parse_source"),
        "split(\"\\n\") | map(rtrimstr(\"\\r\")) | if .[-1] == \"\" then .[:-1] else . end"
    );
    assert_eq!(
        expression(&doc, "compute"),
        ".records | reduce .[] as $r ([]; if any(.[]; . == $r) then . else . + [$r] end) | join(\"\\n\") | if length > 0 then . + \"\\n\" else . end"
    );
    assert_eq!(rule_jq(&out), expression(&doc, "compute"));
    assert_eq!(
        expression(&doc, "compute_guard"),
        "(.records | type) == \"array\" and all(.records[]; type == \"string\")"
    );
    // "the unique ones" are the lines the computation kept: no draft, no model.
    assert_eq!(
        doc["tasks"]["write_output"]["with"]["content"],
        "${{ tasks.compute.output }}"
    );
    assert!(doc["tasks"].get("draft").is_none(), "{doc:#}");
    assert!(doc.get("model").is_none(), "{doc:#}");
}

#[test]
fn a_stated_join_of_two_csv_sources_parses_each_file_and_joins_on_the_column() {
    let intent = "Read ./a.csv and ./b.csv, merge them on the id column and write the result to ./merged.csv";
    let (out, doc) = ready(intent);
    assert_eq!(
        doc["const"]["source_paths"],
        serde_json::json!(["./a.csv", "./b.csv"])
    );
    // Each file is parsed apart, in order; the join reads one array of records per file.
    assert_eq!(
        doc["tasks"]["parse_source"]["for_each"]["items"],
        "${{ with.texts }}"
    );
    assert_eq!(
        doc["tasks"]["parse_source"]["invoke"]["args"]["from"],
        "csv"
    );
    assert_eq!(
        expression(&doc, "compute"),
        ".records | reduce .[1:][] as $right (.[0]; [.[] as $a | $right[] | select(.id == ($a | .id)) | $a + .])"
    );
    assert_eq!(rule_jq(&out), expression(&doc, "compute"));
    assert!(
        expression(&doc, "compute_guard").contains("all(.records[]; type == \"array\""),
        "{doc:#}"
    );
    // The merged CSV's header is every source's header in turn.
    assert!(
        expression(&doc, "source_columns").starts_with("[.[] | split("),
        "{doc:#}"
    );
    assert_eq!(
        doc["tasks"]["merged_csv"]["invoke"]["args"]["columns"],
        "${{ with.columns }}"
    );
    assert_eq!(
        doc["tasks"]["write_output"]["with"]["content"],
        "${{ tasks.merged_csv.output }}"
    );
    assert!(doc["tasks"].get("documents").is_none(), "{doc:#}");
    assert!(doc.get("model").is_none(), "{doc:#}");
    assert_eq!(
        doc["permits"]["fs"]["read"],
        serde_json::json!(["./a.csv", "./b.csv"])
    );
    assert!(
        !doc["tasks"]
            .as_object()
            .unwrap()
            .keys()
            .any(|k| k.starts_with("merge_")),
        "a data join is never an external merge effect: {doc:#}"
    );
}

#[test]
fn a_stage_the_request_does_not_state_whole_is_asked_never_guessed() {
    for intent in [
        // No key: on which column?
        "Read ./a.csv and ./b.csv, merge them and write the result to ./merged.csv",
        // No number and no measure.
        "Read ./sales.csv, keep the best rows and write them to ./top.csv",
        // No key.
        "Read ./sales.csv, sort the rows and write ./sorted.csv",
        // No field list.
        "Read ./tickets.json, keep only the important fields and write them to ./slim.json",
        // An exclusion is never read as a keep of the rows it names.
        "Read ./sales.csv, exclude the rows whose amount is below 100 and write them to ./big.csv",
    ] {
        let out = compile(&CompileRequest::create(intent)).unwrap();
        assert_ne!(out.status, CompileStatus::Ready, "{intent}: {out:#?}");
        assert_eq!(keys(&out), ["intent.clarification"], "{intent}: {out:#?}");
        assert!(out.candidate.is_none(), "{intent}: {out:#?}");
    }
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
