// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A conversion's identity is a rule like any other (R4 S0): recorded, replayed on an answer
//! round and bound to its words. Measured on 007592ab9: « convert ./sales.csv into ./sales.json
//! and write a summary of it to ./summary.md » asked for its model, and the answer round could
//! never replay (`rules[0]` is not a complete recorded rule); « convert ./sales.csv into
//! ./sales.json, keeping only the rows whose amount is above the agreed threshold » went READY
//! as a plain conversion, the filter swallowed by the identity (every row written).
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileOutcome, CompileRequest, CompileStatus, compile};
use serde_json::{Value, json};

mod common;

const MODEL: &str = "\"mock/echo\"";

fn replay(intent: &str, record: Value) -> CompileOutcome {
    compile(&CompileRequest::create(intent).with_plan(record)).unwrap()
}

fn replay_refused(intent: &str, out: &CompileOutcome, why: &str) {
    assert_ne!(out.status, CompileStatus::Ready, "{intent}: {out:#?}");
    assert!(out.candidate.is_none(), "{intent}: {out:#?}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.target == "recorded_plan" && d.message.contains(why)),
        "{intent}: {out:#?}"
    );
}

#[test]
fn a_conversion_replays_on_its_answer_round() {
    let intent = "convert ./sales.csv into ./sales.json and write a summary of it to ./summary.md";
    let first = compile(&CompileRequest::create(intent)).unwrap();
    assert!(
        first.questions.iter().any(|q| q.key == "model"),
        "{first:#?}"
    );
    let record = first.provenance.plan.clone().unwrap();
    let identity = record["rules"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["clauses"] == json!([]) && r["jq"] == json!(".records"));
    assert!(identity.is_some(), "the identity is recorded: {record:#}");
    let answered = compile(
        &CompileRequest::create(intent)
            .with_plan(record)
            .answer("model", MODEL),
    )
    .unwrap();
    assert!(
        !answered
            .diagnostics
            .iter()
            .any(|d| d.target == "recorded_plan"),
        "{answered:#?}"
    );
    assert_eq!(answered.status, CompileStatus::Ready, "{answered:#?}");
}

/// An identity stands only where the words state that very conversion: a record that replaced a
/// filter by the identity (every row kept) is refused, the rule named.
#[test]
fn a_forged_identity_is_refused() {
    let intent = "Read ./tickets.json, keep only the rows whose status is open and write them to ./open.json";
    let tickets: &[&str] = &["id", "status"];
    let world = common::observed(&[("./tickets.json", tickets)]);
    let first = compile(&CompileRequest::create(intent).with_knowledge(world)).unwrap();
    assert_eq!(first.status, CompileStatus::Ready, "{first:#?}");
    let mut record = first.provenance.plan.clone().unwrap();
    let text = record["rules"][0]["text"].clone();
    record["rules"][0] = json!({"text": text, "clauses": [], "junction": "and", "summary": false,
        "shape": {"aggregations": [], "columns": [], "derived": [], "descending": false,
                  "distinct": false, "distinct_by": [], "group_by": null, "join_on": null,
                  "limit": null, "renames": [], "sort_by": null},
        "lines": false, "program": null});
    replay_refused(
        intent,
        &replay(intent, record),
        "is no conversion its words state",
    );
}

/// A flag over no clause and no stage is still no complete rule.
#[test]
fn an_empty_flagged_rule_is_no_record() {
    let intent = "convert ./sales.csv into ./sales.json and write a summary of it to ./summary.md";
    let first = compile(&CompileRequest::create(intent)).unwrap();
    let mut record = first.provenance.plan.clone().unwrap();
    record["rules"][0]["lines"] = json!(true);
    record["rules"][0].as_object_mut().unwrap().remove("jq");
    replay_refused(
        intent,
        &replay(intent, record),
        "is not a complete recorded rule",
    );
}

/// The rule a conversion clause also states is never swallowed by the identity.
#[test]
fn a_conversion_that_states_a_filter_is_no_plain_conversion() {
    for intent in [
        "convert ./sales.csv into ./sales.json, keeping only the rows whose amount is above the agreed threshold",
        "convert ./sales.csv into ./sales.json, keeping only the rows whose amount is above 10",
        "convert ./sales.csv into ./sales.json, keeping only the rows whose status is open",
        "convert ./sales.csv into ./sales.json, sort them by amount descending",
    ] {
        let out = compile(&CompileRequest::create(intent)).unwrap();
        assert_ne!(out.status, CompileStatus::Ready, "{intent}: {out:#?}");
        assert!(out.candidate.is_none(), "{intent}: {out:#?}");
    }
    // A description of the conversion itself stays one.
    let intent = "convert ./sales.csv into ./sales.json, a JSON array with one object per row, same row order";
    let out = compile(&CompileRequest::create(intent)).unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{intent}: {out:#?}");
}
