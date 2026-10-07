// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Independent typed computations must not collapse into the first matching rule.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, CompileStatus, NativeMode};
use nika_compile_cognition::compile_with_provider;
use serde_json::{Value, json};

mod common;
use common::{Judged, Rotating, keys, policy};

const INTENT: &str = "Read sales.csv, keep only rows whose status is paid, write those rows with the same CSV header to paid.csv, and write their total amount as a number to total.txt.";

fn proposal() -> Value {
    let predicate = json!({"present":true,"polarity":"keep","join":"and",
        "clauses":[{"field":"status","op":"eq","value":"paid","value_field":""}],
        "group_by":"","aggregations":[],"sort_by":"","order":"","columns":[],
        "derived":[],"limit":"","renames":[]});
    let mut total = predicate.clone();
    total["clauses"] = json!([]);
    total["aggregations"] = json!([{"field":"amount","op":"sum","as":"total","round":""}]);
    json!({"steps":[
        {"op":"read","detail":"sales.csv","evidence":"Read sales.csv"},
        {"op":"compute","detail":"keep only rows whose status is paid","evidence":"keep only rows whose status is paid","computation":predicate},
        {"op":"compute","detail":"their total amount","evidence":"their total amount","computation":total}],
        "effects":[
            {"verb":"write","target":"write those rows with the same CSV header to paid.csv","policy":"automatic","evidence":"write those rows with the same CSV header to paid.csv"},
            {"verb":"write","target":"write their total amount as a number to total.txt","policy":"automatic","evidence":"write their total amount as a number to total.txt"}],
        "obligations":[],"constraints":[],"unknowns":[]})
}

#[tokio::test]
async fn cold_cannot_claim_two_computations_realized_by_the_first_rule() {
    let provider = Rotating::new(vec![proposal().to_string()]);
    let req =
        CompileRequest::create(INTENT).with_authoring_policy(policy().with_native(NativeMode::Off));
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_eq!(
        out.provenance.plan.as_ref().unwrap()["rules"]
            .as_array()
            .unwrap()
            .len(),
        2,
        "{out:#?}"
    );
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(keys(&out).contains(&"const.rule_expression"), "{out:#?}");
}

#[tokio::test]
async fn default_route_escalates_distinct_computations_without_asking_for_jq() {
    // The plan states the two computations; the merge cannot realize both with one rule and asks
    // for a machine's rule: the default route escalates to the sketch door instead of asking.
    let task = |id: &str, tool: &str, extra: Value| {
        let mut t = json!({"id": id, "verb": "invoke", "tool": tool, "purpose": id});
        for (k, v) in extra.as_object().unwrap() {
            t[k] = v.clone();
        }
        t
    };
    let edge = |name: &str, from: &str| json!([{"name": name, "from": from}]);
    let sketch = json!({"name": "paid-rows-and-total", "tasks": [
        task("source", "nika:read", json!({"reads": ["sales.csv"]})),
        task("rows", "nika:convert", json!({"with": edge("source", "source")})),
        task("paid", "nika:jq", json!({"with": edge("rows", "rows")})),
        task("total", "nika:jq", json!({"with": edge("paid", "paid")})),
        task("csv", "nika:convert", json!({"with": edge("paid", "paid")})),
        task("write_rows", "nika:write", json!({"writes": ["paid.csv"], "with": edge("csv", "csv")})),
        task("write_total", "nika:write", json!({"writes": ["total.txt"], "with": edge("total", "total")})),
    ], "outputs": [], "questions": [], "gaps": [], "notes": "Distinct filtered rows and their numeric total"});
    let fills = json!({"fills": [
        {"task": "rows", "field": "args", "value": {"from": "csv", "to": "json"}},
        {"task": "paid", "field": "expression", "value": "[.[] | select(.status == \"paid\")]"},
        {"task": "total", "field": "expression", "value": "map(.amount | tonumber) | add // 0"},
        {"task": "csv", "field": "args", "value": {"from": "json", "to": "csv", "columns": ["customer", "status", "amount"]}}
    ], "notes": "four holes"});
    let provider = Rotating::new(vec![
        proposal().to_string(),
        sketch.to_string(),
        fills.to_string(),
    ]);
    let req = CompileRequest::create(INTENT)
        .with_authoring_policy(policy().with_native(NativeMode::Escalate));
    // Judged by the explicit approving double (R4 A11): this test reads the emitted workflow.
    let out = compile_with_provider(&req, &Judged::approving(&provider))
        .await
        .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(keys(&out).is_empty(), "no machine-rule question: {out:#?}");
    // The COLD plan, the sketch, its fills and the whole-request judgment, every one journaled.
    let receipt = out.provenance.authoring.as_ref().unwrap();
    let roles: Vec<&str> = (receipt.context.iter())
        .map(|c| c["call"].as_str().unwrap())
        .collect();
    assert_eq!(
        roles,
        ["plan", "sketch", "fill", "judge_request"],
        "{out:#?}"
    );
    assert_eq!(receipt.calls, 4, "{out:#?}");
    // The independent oracle: the filtered rows go to paid.csv; the numeric total of the SAME
    // paid subset goes to total.txt; the two computations stay distinct.
    let doc: Value = serde_yaml_bw::from_str(out.candidate.as_deref().unwrap()).unwrap();
    let source_of = |task: &str, name: &str| -> String {
        (doc["tasks"][task]["with"][name].as_str().unwrap())
            .trim_start_matches("${{ tasks.")
            .trim_end_matches(".output }}")
            .to_owned()
    };
    assert_eq!(
        doc["tasks"]["write_rows"]["invoke"]["args"]["path"],
        "paid.csv"
    );
    assert_eq!(
        doc["tasks"]["write_total"]["invoke"]["args"]["path"],
        "total.txt"
    );
    let rows_csv = source_of("write_rows", "csv");
    let paid = source_of(&rows_csv, "paid");
    assert_eq!(
        doc["tasks"][paid.as_str()]["invoke"]["args"]["expression"],
        "[.[] | select(.status == \"paid\")]",
        "{doc:#}"
    );
    let total = source_of("write_total", "total");
    assert_eq!(
        source_of(&total, "paid"),
        paid,
        "the total reads the paid subset: {doc:#}"
    );
    let sum = doc["tasks"][total.as_str()]["invoke"]["args"]["expression"]
        .as_str()
        .unwrap();
    assert!(sum.contains("add"), "a numeric total: {sum}");
    assert_ne!(total, paid, "two distinct computations");
}
