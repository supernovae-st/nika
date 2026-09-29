// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used)]

use nika_compile_reader::plan::Plan;
use serde_json::{Value, json};

fn record() -> Value {
    let mut record = Plan::default().to_json();
    record["rules"] = json!([{
        "text": "keep paid rows",
        "clauses": [{"field": "status", "comparator": "==", "value": "paid", "value_kind": "text"}],
        "junction": "and", "summary": false, "lines": false,
        "shape": {"columns": ["status", "amount"]}, "program": null
    }]);
    record["slots"] = json!([{"key": "const.threshold", "label": "threshold", "numeric": true}]);
    record
}

#[test]
fn malformed_rule_collections_cannot_drop_business_duties() {
    for value in [Value::Null, json!(true), json!({}), json!("rules")] {
        let mut bad = record();
        bad["rules"] = value;
        let error = Plan::from_json(&bad).expect_err("present malformed rules must refuse");
        assert!(error.contains("rules"), "{error}");
    }
    let mut mixed = record();
    mixed["rules"]
        .as_array_mut()
        .expect("rules")
        .push(json!({"text": "do not send refunds"}));
    let error = Plan::from_json(&mixed).expect_err("one malformed rule invalidates the whole plan");
    assert!(error.contains("rules[1]"), "{error}");
}

#[test]
fn malformed_typed_fields_cannot_become_defaults_or_partial_arrays() {
    for (pointer, value, path) in [
        ("/rules/0/shape", json!(7), "rules[0].shape"),
        ("/rules/0/junction", json!("xor"), "rules[0].junction"),
        ("/rules/0/summary", json!(1), "rules[0].summary"),
        ("/rules/0/lines", json!("false"), "rules[0].lines"),
        (
            "/rules/0/shape/columns",
            json!(["status", 7]),
            "rules[0].shape.columns[1]",
        ),
        (
            "/rules/0/clauses/0/value_kind",
            json!("mystery"),
            "rules[0].clauses[0].value_kind",
        ),
        ("/slots/0/numeric", json!("true"), "slots[0].numeric"),
        ("/slots", json!({}), "slots"),
    ] {
        let mut bad = record();
        *bad.pointer_mut(pointer).expect("existing field") = value;
        let error = Plan::from_json(&bad).expect_err("malformed field must refuse");
        assert!(error.contains(path), "{pointer}: {error}");
    }
}

#[test]
fn legacy_absence_and_complete_round_trip_preserve_the_same_rule() {
    let original = Plan::from_json(&record()).expect("valid legacy partial shape");
    let canonical = original.to_json();
    let replayed = Plan::from_json(&canonical).expect("full round trip");
    assert_eq!(replayed, original);
    assert_eq!(replayed.to_json(), canonical);
    let mut legacy = Plan::default().to_json();
    let object = legacy.as_object_mut().expect("plan");
    object.remove("rules");
    object.remove("slots");
    assert_eq!(
        Plan::from_json(&legacy).expect("pre-rule record"),
        Plan::default()
    );
}

#[test]
fn aggregation_replay_refuses_overflow_and_missing_source_columns() {
    for round in [
        json!(7),
        json!(19),
        json!(20),
        json!(u32::MAX),
        json!(-1),
        json!(2.0),
        json!("2"),
    ] {
        let mut bad = record();
        bad["rules"][0]["shape"]["aggregations"] =
            json!([{"field": "amount", "op": "sum", "name": "total", "round": round}]);
        assert!(Plan::from_json(&bad).is_err(), "bad round {round}");
    }
    let mut bad = record();
    bad["rules"][0]["shape"]["aggregations"] =
        json!([{"field": null, "op": "sum", "name": "total", "round": null}]);
    assert!(
        Plan::from_json(&bad).is_err(),
        "a sum cannot become a count"
    );
    for round in [Value::Null, json!(0), json!(2), json!(6)] {
        let mut valid = record();
        valid["rules"][0]["shape"]["aggregations"] =
            json!([{"field": "amount", "op": "sum", "name": "total", "round": round}]);
        let plan = Plan::from_json(&valid).expect("representable aggregation");
        assert_eq!(Plan::from_json(&plan.to_json()).expect("replay"), plan);
    }
}

#[test]
fn recorded_predicate_and_derived_fields_cannot_be_silently_reinterpreted() {
    let mut valid = record();
    valid["rules"][0]["clauses"][0]["value_kind"] = json!("bool");
    valid["rules"][0]["clauses"][0]["value"] = json!("false");
    let plan = Plan::from_json(&valid).expect("false is a real boolean");
    assert_eq!(Plan::from_json(&plan.to_json()).expect("round trip"), plan);
    valid["rules"][0]["clauses"][0]["value"] = json!("not-a-bool");
    assert!(Plan::from_json(&valid).is_err());
    let mut forged = record();
    forged["rules"][0]["jq"] = json!("map(.)");
    assert!(
        Plan::from_json(&forged).is_err(),
        "a forged projection disagrees"
    );
    let mut program = record();
    program["rules"][0]["program"] = json!({"jq": "map(.)", "columns": ["status", 7]});
    assert!(
        Plan::from_json(&program).is_err(),
        "program columns cannot disappear"
    );
}

#[test]
fn derived_numeric_terms_and_slot_declarations_keep_the_closed_grammar() {
    for number in ["2 or true", "nan", "2 | halt_error", "1e999"] {
        let mut bad = record();
        bad["rules"][0]["shape"]["derived"] = json!([{
            "name": "scaled", "op": "mul", "left": {"name": "amount"},
            "right": {"number": number}
        }]);
        assert!(Plan::from_json(&bad).is_err(), "{number}");
    }
    for key in ["inputs.token", "const.", "const.threshold | true"] {
        let mut bad = record();
        bad["slots"][0]["key"] = json!(key);
        assert!(Plan::from_json(&bad).is_err(), "{key}");
    }
    let mut bad = record();
    bad["slots"][0]
        .as_object_mut()
        .expect("slot")
        .remove("numeric");
    assert!(
        Plan::from_json(&bad).is_err(),
        "numeric existed from the first slot format"
    );
}

#[test]
fn public_aggregation_lowering_does_not_panic_or_wrap_its_scale() {
    use nika_compile_reader::rules::{AggOp, Aggregation, Junction, Rule, Shape};
    let mut shape = Shape::default();
    shape.aggregations.push(Aggregation::new(
        Some("amount".to_owned()),
        AggOp::Sum,
        "total",
        Some(u32::MAX),
    ));
    let rule = Rule::typed("sum amount", Vec::new(), Junction::And, shape);
    assert!(rule.jq().contains("rounding precision overflows"));
    let mut plan = Plan::default();
    plan.rules.push(rule);
    assert!(Plan::from_json(&plan.to_json()).is_err());
}
