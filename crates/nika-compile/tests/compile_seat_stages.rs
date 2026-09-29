// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Stages a seat states over a clause the reader also reads are joined, never dropped (E38, a C3
//! neighbour). « keep only the rows whose region is north and write them … with id as text and
//! amount as a JSON number »: the reader reads the clause as a filter, and the seat's admitted
//! typed rule for the same clause (the filter, a projection and a number column) was discarded
//! because a rule of that text already stood. The workflow wrote every row with amount as text,
//! READY behind a judge alone. The seat's stages now complete the reader's plain filter; a seat
//! filter that disagrees with the reader's is asked, never silently dropped.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileOutcome, CompileRequest, CompileStatus};
use nika_compile_cognition::compile_with_provider;
use serde_json::{Value, json};

mod common;
use common::{Judged, Provider, policy};

const ORDERS: (&str, &[&str]) = ("./data/orders.csv", &["id", "region", "amount", "item"]);
const COMPUTED: &str = "keep only the rows whose region is north";
const WRITE: &str = "write them to ./out/top.json with id as text and amount as a JSON number";

fn intent() -> String {
    format!("read ./data/orders.csv, {COMPUTED} and {WRITE}")
}

/// The plan a seat states: the read, one compute step over [`COMPUTED`] with `computation`, and
/// the write.
fn proposal(computation: &Value) -> Value {
    let read = "read ./data/orders.csv";
    json!({
        "steps": [
            {"op": "read", "detail": "./data/orders.csv", "evidence": read},
            {"op": "compute", "detail": COMPUTED, "evidence": COMPUTED, "computation": computation}
        ],
        "effects": [{"verb": "write", "target": WRITE, "policy": "automatic", "evidence": WRITE}],
        "obligations": [], "constraints": [], "unknowns": [],
        "regions": [
            {"text": format!("{read},"), "role": "operation"},
            {"text": COMPUTED, "role": "operation"},
            {"text": format!("and {WRITE}"), "role": "effect"}
        ],
        "approval_bypass": {"present": false, "evidence": ""}
    })
}

/// The seat's typed computation over the clause, with `op` comparing region to north.
fn computation(op: &str, columns: &[&str], numbers: &[&str]) -> Value {
    json!({"present": true, "polarity": "keep", "join": "and",
        "clauses": [{"field": "region", "op": op, "value": "north", "value_field": ""}],
        "group_by": "", "aggregations": [], "sort_by": "", "order": "", "columns": columns,
        "derived": [], "limit": "", "renames": [], "distinct_by": [], "numbers": numbers})
}

/// A cold compile whose seat answers `plan`, judged by the explicit approving double (R4 A11):
/// these tests read the emitted workflow, not a judgment.
async fn cold(plan: &Value) -> CompileOutcome {
    let provider = Provider::new(plan);
    let request = CompileRequest::create(intent())
        .with_authoring_policy(policy())
        .with_knowledge(common::observed(&[ORDERS]));
    compile_with_provider(&request, &Judged::approving(&provider))
        .await
        .unwrap()
}

/// The rule the candidate's compute runs, after any law in front of it, number law folded.
fn rule_of(out: &CompileOutcome) -> String {
    let compute = common::compute(out.candidate.as_deref().expect("a candidate"));
    compute.rsplit('\n').next().unwrap_or_default().to_owned()
}

#[tokio::test]
async fn the_seat_stages_over_a_filter_the_reader_reads_are_kept() {
    let out = cold(&proposal(&computation(
        "eq",
        &["id", "amount"],
        &["amount"],
    )))
    .await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let rule = rule_of(&out);
    // The reader's filter, then the seat's projection with amount under the number law.
    assert!(
        rule.starts_with(
            "[.records[] | select(.region == \"north\")] | map({\"id\": .id, \"amount\": ("
        ),
        "{rule}"
    );
    assert!(rule.contains("(.amount | num)"), "{rule}");
    let record = out.provenance.plan.clone().unwrap();
    assert_eq!(
        record["rules"].as_array().map(Vec::len),
        Some(1),
        "{record:#}"
    );
    assert_eq!(
        record["rules"][0]["shape"]["numbers"],
        json!(["amount"]),
        "{record:#}"
    );
    // The one rule of the clause is the seat's: the reader's filter with the stages stated over it
    // (the merge's own Applied finding says so where the merge's findings surface).
    assert_eq!(
        record["rules"][0]["clauses"][0]["value"],
        json!("north"),
        "{record:#}"
    );
}

#[tokio::test]
async fn a_seat_filter_that_disagrees_with_the_reader_is_asked_never_dropped() {
    // The seat keeps the rows whose region is NOT north over the same clause, with a projection.
    let out = cold(&proposal(&computation(
        "ne",
        &["id", "amount"],
        &["amount"],
    )))
    .await;
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    let told = format!("{:?}", out.diagnostics);
    assert!(told.contains("not settled by a model"), "{told}");
}

#[tokio::test]
async fn a_seat_rule_with_no_stage_of_its_own_changes_nothing() {
    // The seat restates the reader's plain filter: the reader's rule stands, no finding.
    let out = cold(&proposal(&computation("eq", &[], &[]))).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(rule_of(&out), "[.records[] | select(.region == \"north\")]");
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.message.contains("is read as a filter")),
        "{:#?}",
        out.diagnostics
    );
}
