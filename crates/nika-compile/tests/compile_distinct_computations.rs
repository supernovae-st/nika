// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Independent typed computations must not collapse into the first matching rule.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, CompileStatus, NativeMode};
use nika_compile_cognition::compile_with_provider;
use serde_json::{Value, json};

mod common;
use common::{Rotating, keys, policy};

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

const CANDIDATE: &str = r#"nika: paid-rows-and-total
permits:
  tools: ["nika:read", "nika:convert", "nika:jq", "nika:write"]
  fs:
    read: [sales.csv]
    write: [paid.csv, total.txt]
tasks:
  source:
    invoke:
      tool: nika:read
      args: {path: sales.csv}
  rows:
    with: {source: "${{ tasks.source.output }}"}
    invoke:
      tool: nika:convert
      args: {input: "${{ with.source }}", from: csv, to: json}
  paid:
    with: {rows: "${{ tasks.rows.output }}"}
    invoke:
      tool: nika:jq
      args: {input: "${{ with.rows }}", expression: '[.[] | select(.status == "paid")]'}
  total:
    with: {paid: "${{ tasks.paid.output }}"}
    invoke:
      tool: nika:jq
      args: {input: "${{ with.paid }}", expression: 'map(.amount | tonumber) | add // 0'}
  csv:
    with: {paid: "${{ tasks.paid.output }}"}
    invoke:
      tool: nika:convert
      args: {input: "${{ with.paid }}", from: json, to: csv, columns: [customer, status, amount]}
  write_rows:
    with: {csv: "${{ tasks.csv.output }}"}
    invoke:
      tool: nika:write
      args: {path: paid.csv, content: "${{ with.csv }}"}
  write_total:
    with: {total: "${{ tasks.total.output }}"}
    invoke:
      tool: nika:write
      args: {path: total.txt, content: "${{ with.total }}"}
"#;

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
    let provider = Rotating::new(vec![proposal().to_string(), json!({"candidate":CANDIDATE,"questions":[],"gaps":[],"notes":"Distinct filtered rows and their numeric total"}).to_string()]);
    let req = CompileRequest::create(INTENT)
        .with_authoring_policy(policy().with_native(NativeMode::Escalate));
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(keys(&out).is_empty(), "{out:#?}");
    assert_eq!(
        out.provenance.authoring.as_ref().unwrap().calls,
        2,
        "{out:#?}"
    );
    assert_eq!(out.candidate.as_deref(), Some(CANDIDATE));
}
