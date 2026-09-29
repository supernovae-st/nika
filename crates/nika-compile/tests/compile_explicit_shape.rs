// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! An output shape the request states is a typed part of the plan (E38 V2). « write the sum as a
//! bare JSON number to ./out/result.json » had no typed carrier: a total the engine computes is
//! written to a JSON file as the object naming it (`{"sum": 70}`), so the plan left the sum to a
//! seat program outside the number law, and the whole-request judge refused that program without
//! placing a defect. The plan's write now states `alone`: the one typed total is written as its
//! value, under the number law, and what cannot hold one value alone is refused, never rounded
//! into another shape.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileOutcome, CompileRequest, CompileStatus, Strategy, compile};
use nika_compile_cognition::compile_with_provider;
use serde_json::{Value, json};

mod common;
use common::{Judged, Provider, policy};

/// The frozen E38 C1 request, byte for byte.
const C1: &str = "read ./data/input.csv, sum qty over the rows where status is shipped, write the sum as a bare JSON number to ./out/result.json";
const INPUT: (&str, &[&str]) = ("./data/input.csv", &["id", "item", "status", "qty"]);

/// The plan a seat states for a read, one typed computation over it and one write, with the
/// write's own words and, when given, its `alone`.
fn proposal(
    intent: &str,
    computed: &str,
    aggregations: &Value,
    write: &str,
    alone: Option<bool>,
) -> Value {
    let mut effect =
        json!({"verb": "write", "target": write, "policy": "automatic", "evidence": write});
    if let Some(alone) = alone {
        effect["alone"] = json!(alone);
    }
    let read = "read ./data/input.csv";
    assert!(
        intent.contains(computed) && intent.contains(write),
        "{intent}"
    );
    json!({
        "steps": [
            {"op": "read", "detail": "./data/input.csv", "evidence": read},
            {"op": "compute", "detail": computed, "evidence": computed,
             "computation": {"present": true, "polarity": "keep", "join": "and",
                 "clauses": [{"field": "status", "op": "eq", "value": "shipped", "value_field": ""}],
                 "group_by": "", "aggregations": aggregations, "sort_by": "", "order": "",
                 "columns": [], "derived": [], "limit": "", "renames": [], "distinct_by": []}}
        ],
        "effects": [effect],
        "obligations": [], "constraints": [], "unknowns": [],
        "regions": [
            {"text": format!("{read},"), "role": "operation"},
            {"text": format!("{computed},"), "role": "operation"},
            {"text": write, "role": "effect"}
        ],
        "approval_bypass": {"present": false, "evidence": ""}
    })
}

fn sum_of_qty() -> Value {
    json!([{"field": "qty", "op": "sum", "as": "sum", "round": ""}])
}

/// A cold compile of `intent` whose seat answers `plan`, judged by the explicit approving double
/// (R4 A11): these tests read the emitted workflow, not a judgment.
async fn cold(intent: &str, plan: &Value) -> CompileOutcome {
    let provider = Provider::new(plan);
    let request = CompileRequest::create(intent)
        .with_authoring_policy(policy())
        .with_knowledge(common::observed(&[INPUT]));
    compile_with_provider(&request, &Judged::approving(&provider))
        .await
        .unwrap()
}

/// An answer round under this round's judge, the explicit approving double over a seat that
/// settles no other choice (R4 A11).
async fn judged_replay(request: &CompileRequest) -> CompileOutcome {
    let judge = common::JudgedSeat::approving(&common::NoChoice);
    let cognition = nika_compile_cognition::Cognition::<nika_compile_cognition::NoProvider> {
        provider: None,
        seat: Some(&judge),
    };
    nika_compile_cognition::compile_with_cognition(request, cognition)
        .await
        .unwrap()
}

fn workflow(out: &CompileOutcome) -> Value {
    serde_yaml_bw::from_str(out.candidate.as_deref().expect("a candidate")).unwrap()
}

/// What the one write task carries.
fn written(out: &CompileOutcome) -> Value {
    workflow(out)["tasks"]["write_output"]["with"]["content"].clone()
}

/// The findings an outcome states about its write.
fn write_findings(out: &CompileOutcome) -> Vec<String> {
    out.diagnostics
        .iter()
        .filter(|d| d.target.starts_with("write_"))
        .map(|d| d.message.clone())
        .collect()
}

#[tokio::test]
async fn a_bare_total_the_request_states_is_written_as_its_value_under_the_number_law() {
    let computed = "sum qty over the rows where status is shipped";
    let write = "write the sum as a bare JSON number to ./out/result.json";
    let plan = proposal(C1, computed, &sum_of_qty(), write, Some(true));
    let out = cold(C1, &plan).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(out.provenance.strategy, Some(Strategy::Cold));
    // The one total, alone: the value, never the object naming it.
    assert_eq!(written(&out), json!("${{ tasks.compute.output.sum }}"));
    // The engine's typed sum, every qty read under the number law (a blank or a word stops the
    // run naming `qty`), not a seat program.
    let compute = common::compute(out.candidate.as_deref().unwrap());
    assert!(
        compute.contains("select(.status == \"shipped\")"),
        "{compute}"
    );
    assert!(
        compute.contains("{\"sum\": (map((.qty | num))"),
        "{compute}"
    );
    // The plan records what it stated, and the record replays to the same bytes: INCOMPLETE
    // until a round judges it (Q2, R4 A11), READY under the judge with the same candidate.
    let record = out.provenance.plan.clone().unwrap();
    assert_eq!(record["effects"][0]["alone"], json!(true), "{record:#}");
    let request = CompileRequest::create(C1)
        .with_knowledge(common::observed(&[INPUT]))
        .with_plan(record.clone());
    let replayed = compile(&request).unwrap();
    assert_eq!(replayed.status, CompileStatus::Incomplete, "{replayed:#?}");
    assert_eq!(replayed.candidate, out.candidate);
    let judged = judged_replay(&request).await;
    assert_eq!(judged.status, CompileStatus::Ready, "{judged:#?}");
    assert_eq!(judged.candidate, out.candidate);
    // A record whose write says nothing of `alone` keeps the object: the field is never
    // inferred from the words it was read from.
    let mut unstated = record;
    unstated["effects"][0]
        .as_object_mut()
        .unwrap()
        .remove("alone");
    let request = CompileRequest::create(C1)
        .with_knowledge(common::observed(&[INPUT]))
        .with_plan(unstated);
    let object = compile(&request).unwrap();
    assert_eq!(written(&object), json!("${{ tasks.compute.output }}"));
}

#[tokio::test]
async fn without_alone_a_total_to_json_stays_the_object_naming_it() {
    let intent = "read ./data/input.csv, sum qty over the rows where status is shipped, write the sum to ./out/result.json";
    for alone in [None, Some(false)] {
        let plan = proposal(
            intent,
            "sum qty over the rows where status is shipped",
            &sum_of_qty(),
            "write the sum to ./out/result.json",
            alone,
        );
        let out = cold(intent, &plan).await;
        assert_eq!(out.status, CompileStatus::Ready, "{alone:?}: {out:#?}");
        assert_eq!(
            written(&out),
            json!("${{ tasks.compute.output }}"),
            "{alone:?}"
        );
        let record = out.provenance.plan.clone().unwrap();
        // Unstated or false, the record carries no `alone`: older plans read and hash as before.
        assert!(record["effects"][0].get("alone").is_none(), "{record:#}");
    }
}

#[tokio::test]
async fn several_totals_cannot_be_one_value_alone() {
    let intent = "read ./data/input.csv, sum qty and count the rows where status is shipped, write the sum and the count as bare JSON numbers to ./out/result.json";
    let totals = json!([
        {"field": "qty", "op": "sum", "as": "sum", "round": ""},
        {"field": "", "op": "count", "as": "count", "round": ""}
    ]);
    let plan = proposal(
        intent,
        "sum qty and count the rows where status is shipped",
        &totals,
        "write the sum and the count as bare JSON numbers to ./out/result.json",
        Some(true),
    );
    let out = cold(intent, &plan).await;
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    let findings = write_findings(&out);
    assert!(
        findings
            .iter()
            .any(|m| m.contains("one value alone") && m.contains("`sum`") && m.contains("`count`")),
        "{findings:?}\n{out:#?}"
    );
}

#[tokio::test]
async fn a_table_or_a_document_format_cannot_hold_a_total_alone() {
    for path in ["./out/result.csv", "./out/result.yaml", "./out/result.toml"] {
        let write = format!("write the sum as a bare number to {path}");
        let intent = format!(
            "read ./data/input.csv, sum qty over the rows where status is shipped, {write}"
        );
        let plan = proposal(
            &intent,
            "sum qty over the rows where status is shipped",
            &sum_of_qty(),
            &write,
            Some(true),
        );
        let out = cold(&intent, &plan).await;
        assert_ne!(out.status, CompileStatus::Ready, "{path}: {out:#?}");
        assert!(out.candidate.is_none(), "{path}: {out:#?}");
        let findings = write_findings(&out);
        assert!(
            findings
                .iter()
                .any(|m| m.contains("one value alone") && m.contains(path)),
            "{path}: {findings:?}\n{out:#?}"
        );
    }
}

#[tokio::test]
async fn alone_changes_nothing_where_no_object_names_the_value() {
    // A prose file already takes the one total as its value (the conventions' prose law).
    let write = "write the sum as a bare number to ./out/result.txt";
    let intent =
        format!("read ./data/input.csv, sum qty over the rows where status is shipped, {write}");
    for alone in [Some(true), None] {
        let plan = proposal(
            &intent,
            "sum qty over the rows where status is shipped",
            &sum_of_qty(),
            write,
            alone,
        );
        let out = cold(&intent, &plan).await;
        assert_eq!(out.status, CompileStatus::Ready, "{alone:?}: {out:#?}");
        assert_eq!(
            written(&out),
            json!("${{ tasks.compute.output.sum }}"),
            "{alone:?}"
        );
    }
    // Rows are written as they are, an array: no object names them, `alone` removes nothing.
    let write = "write them to ./out/result.json";
    let intent = format!("read ./data/input.csv, keep the rows where status is shipped, {write}");
    let computed = "keep the rows where status is shipped";
    let stated = proposal(&intent, computed, &json!([]), write, Some(true));
    let out = cold(&intent, &stated).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(written(&out), json!("${{ tasks.compute.output }}"));
    let plain = cold(
        &intent,
        &proposal(&intent, computed, &json!([]), write, None),
    )
    .await;
    assert_eq!(plain.candidate, out.candidate);
}
