// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Recorded computation shape and operand invariants are rechecked before replay.
#![allow(clippy::expect_used)]

use nika_compile::{CompileRequest, CompileStatus, compile};
use serde_json::{Value, json};

const INTENT: &str =
    "Read ./tickets.json, keep only the rows whose status is open and write them to ./open.json";

fn saved() -> (Value, String) {
    let out = compile(&CompileRequest::create(INTENT)).expect("initial compilation");
    assert_eq!(out.status, CompileStatus::Ready);
    (
        out.provenance.plan.expect("recorded semantic plan"),
        out.candidate.expect("executable filter"),
    )
}

#[test]
fn a_valid_saved_filter_replays_the_identical_workflow() {
    let (record, candidate) = saved();
    let out = compile(&CompileRequest::create(INTENT).with_plan(record)).expect("replay");
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(out.candidate.as_deref(), Some(candidate.as_str()));
}

#[test]
fn malformed_saved_rules_never_become_a_weaker_ready_workflow() {
    let (record, _) = saved();
    for malformed in [
        Value::Null,
        json!({"text": "preserve the filter"}),
        json!(7),
    ] {
        let mut bad = record.clone();
        bad["rules"].as_array_mut().expect("rules").push(malformed);
        let out = compile(&CompileRequest::create(INTENT).with_plan(bad)).expect("honest refusal");
        assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
        assert!(out.candidate.is_none(), "no partial executable: {out:#?}");
        assert!(out.diagnostics.iter().any(|d| d.target == "recorded_plan"));
    }
}

#[test]
fn typed_shape_corruption_is_visible_at_the_compile_door() {
    let (mut record, _) = saved();
    record["rules"][0]["shape"]["columns"] = json!(["status", 7]);
    let out = compile(&CompileRequest::create(INTENT).with_plan(record)).expect("honest refusal");
    assert_ne!(out.status, CompileStatus::Ready);
    assert!(out.candidate.is_none());
    assert!(out.diagnostics.iter().any(|d| d.target == "recorded_plan"));
}

fn refuses_computation(intent: &str, record: Value) {
    let out = compile(&CompileRequest::create(intent).with_plan(record)).expect("replay");
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(
        out.candidate.is_none(),
        "no executable from invalid computation: {out:#?}"
    );
    assert!(
        out.diagnostics.iter().any(|d| d.target == "recorded_plan"),
        "{out:#?}"
    );
}

fn without_observations(record: &mut Value) {
    for rule in record["rules"].as_array_mut().expect("rules") {
        let fields = rule.as_object_mut().expect("rule");
        fields.remove("jq");
        fields.remove("fields");
    }
}

#[test]
fn saved_numeric_operands_are_numbers_from_the_stated_rule() {
    let intent = "Read ./tickets.json, keep only the rows whose amount is strictly greater than 50 and write them to ./open.json";
    let out = compile(&CompileRequest::create(intent)).expect("compile numeric filter");
    assert_eq!(out.status, CompileStatus::Ready);
    let record = out.provenance.plan.expect("plan");
    for literal in ["50 or true", "nan", "1", "500", "-50", "1e999", "50 | ."] {
        let mut bad = record.clone();
        without_observations(&mut bad);
        bad["rules"][0]["clauses"][0]["value"] = json!(literal);
        refuses_computation(intent, bad);
    }
    let mut bad = record;
    without_observations(&mut bad);
    bad["rules"][0]["clauses"][0]
        .as_object_mut()
        .expect("clause")
        .remove("value_kind");
    refuses_computation(intent, bad);
}

#[test]
fn a_program_cannot_replace_a_recorded_filter_and_slots_must_be_declared() {
    let (record, _) = saved();
    let mut hybrid = record.clone();
    without_observations(&mut hybrid);
    hybrid["rules"][0]["program"] = json!({"jq": ".records", "columns": ["status"]});
    refuses_computation(INTENT, hybrid);
    for slug in ["ghost", "ghost | true", "ghost[0]"] {
        let mut bad = record.clone();
        without_observations(&mut bad);
        bad["rules"][0]["clauses"][0]["value_kind"] = json!("slot");
        bad["rules"][0]["clauses"][0]["value"] = json!(slug);
        refuses_computation(INTENT, bad);
    }
}

#[test]
fn historically_required_rule_fields_are_not_silently_defaulted() {
    let (record, _) = saved();
    for field in ["summary", "shape"] {
        let mut bad = record.clone();
        without_observations(&mut bad);
        bad["rules"][0].as_object_mut().expect("rule").remove(field);
        refuses_computation(INTENT, bad);
    }
}
