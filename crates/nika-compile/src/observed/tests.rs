// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
use super::*;
use crate::rules::{Clause, Comparator, Junction, Operand, Rule, Shape};
use serde_json::json;
use std::collections::BTreeSet;

fn request(intent: &str) -> CompileRequest {
    CompileRequest::create(intent).with_knowledge(json!({"observed": [{
        "path":"./orders.json", "columns":["id", "status"],
        "common_columns":["id", "status"], "state":"observed", "complete":false
    }]}))
}
fn filter(field: &str) -> Rule {
    Rule::typed(
        "garde les commandes dont le statut est delivered",
        vec![Clause::new(
            field,
            Comparator::Eq,
            Operand::Text("delivered".into()),
        )],
        Junction::And,
        Shape::default(),
    )
}
#[test]
fn translated_status_is_a_closed_choice_and_only_an_offered_answer_rebinds_it() {
    let req = request("Lis ./orders.json, garde les commandes dont le statut est delivered");
    let mut out = crate::initial();
    let mut recognized = BTreeSet::new();
    assert!(
        ground_rule(
            filter("statut"),
            "./orders.json",
            &req,
            &mut out,
            &mut recognized
        )
        .is_none()
    );
    let q = &out.questions[0];
    assert_eq!(q.answer_type, crate::QuestionType::Choice);
    assert_eq!(
        q.options.iter().map(|o| o.key.as_str()).collect::<Vec<_>>(),
        ["id", "status"]
    );
    assert!(recognized.contains("const.rule_field_1"));
    let accepted = req.clone().answer("const.rule_field_1", "\"status\"");
    let rebound = ground_rule(
        filter("statut"),
        "orders.json",
        &accepted,
        &mut crate::initial(),
        &mut BTreeSet::new(),
    )
    .unwrap();
    assert_eq!(rebound.source_fields(), ["status"]);
    assert!(rebound.jq().contains("status"));
    for invalid in ["\"statut\"", "\"STATUS\"", "42", "\"missing\""] {
        let wrong = req.clone().answer("const.rule_field_1", invalid);
        let mut out = crate::initial();
        assert!(
            ground_rule(
                filter("statut"),
                "orders.json",
                &wrong,
                &mut out,
                &mut BTreeSet::new()
            )
            .is_none()
        );
        assert_eq!(out.questions[0].answer_type, crate::QuestionType::Choice);
    }
}
#[test]
fn exact_id_is_preserved_but_a_ticket_to_id_mapping_is_not_invented() {
    let exact = request("Read ./orders.json and keep id equal to 42");
    let rule = Rule::typed(
        "id equal to 42",
        vec![Clause::new(
            "id",
            Comparator::Eq,
            Operand::Number("42".into()),
        )],
        Junction::And,
        Shape::default(),
    );
    let mut out = crate::initial();
    let accepted = ground_rule(
        rule.clone(),
        "orders.json",
        &exact,
        &mut out,
        &mut BTreeSet::new(),
    )
    .unwrap();
    assert_eq!(accepted.jq(), rule.jq());
    assert!(out.questions.is_empty());
    let ambiguous = request("Read ./orders.json and find ticket 42");
    assert!(
        ground_rule(
            rule,
            "orders.json",
            &ambiguous,
            &mut out,
            &mut BTreeSet::new()
        )
        .is_none()
    );
    assert_eq!(out.questions[0].answer_type, crate::QuestionType::Choice);
}
#[test]
fn absent_empty_unknown_and_unreadable_are_not_empty_complete_schemas() {
    assert_eq!(columns(None, "orders.json"), None);
    for state in ["absent", "empty", "unknown", "unreadable"] {
        let world = json!({"observed":[{"path":"orders.json", "state":state, "complete":false}]});
        assert_eq!(columns(Some(&world), "orders.json"), None);
    }
    let mixed = json!({"observed":[{"path":"orders.json", "state":"observed", "columns":["id", "status"], "common_columns":[], "complete":false}]});
    assert_eq!(columns(Some(&mixed), "orders.json"), Some(vec![]));
    assert_eq!(columns(Some(&mixed), "other.json"), None);
}
#[test]
fn parameterized_authoring_without_observation_keeps_its_existing_rule() {
    let req = CompileRequest::create("Read ./future.json and keep status equal to delivered");
    let mut out = crate::initial();
    let rule = filter("status");
    let bound = ground_rule(
        rule.clone(),
        "future.json",
        &req,
        &mut out,
        &mut BTreeSet::new(),
    )
    .unwrap();
    assert_eq!(bound.jq(), rule.jq());
    assert!(out.questions.is_empty());
}
#[test]
fn replay_retains_the_observed_choices_and_fresh_observations_take_precedence() {
    let req = request("Read ./orders.json");
    let mut out = crate::initial();
    out.provenance.plan = Some(json!({"rules":[]}));
    record(&req, &mut out);
    let replay =
        CompileRequest::create("Read ./orders.json").with_plan(out.provenance.plan.unwrap());
    assert_eq!(
        columns(world(&replay), "orders.json"),
        Some(vec!["id".into(), "status".into()])
    );
    let fresh =
        replay.with_knowledge(json!({"observed":[{"path":"orders.json", "state":"absent"}]}));
    assert_eq!(columns(world(&fresh), "orders.json"), None);
}
#[test]
fn arbitrary_program_bytes_are_never_renamed() {
    let rule = Rule::program(
        "ticket 42",
        "[.records[] | select(.ticket == 42)]",
        vec!["ticket".into()],
    );
    assert!(rule.with_source_field("ticket", "id").is_none());
    assert_eq!(
        rule.with_source_field("ticket", "ticket").unwrap().jq(),
        rule.jq()
    );
}
