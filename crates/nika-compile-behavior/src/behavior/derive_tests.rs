// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The contract comes from the request's plan alone: the typed fields of its rule, its effects
//! with their policies and the value written alone, its operations and unknowns, the paths the
//! request names and the answers the human gave. What it does not pair or evaluate stays an
//! explicit unsupported obligation; the jq the reader lowers a rule to is never read. A fact the
//! plan does not carry stays open: no plan write is proven unconditional, and no output name
//! is traced to the request.

use std::collections::BTreeMap;

use nika_compile_reader::aggregate::{AggOp, Aggregation, Shape};
use nika_compile_reader::plan::{Effect, EffectPolicy, EffectVerb, Op, Plan, Step as PlanStep};
use nika_compile_reader::rules::{Clause, Comparator, Junction, NumberPolicy, Operand, Rule};
use serde_json::json;

use super::judge_tests::plan_of;
use super::requested::pipeline_of_record;
use super::{
    Contract, Form, Format, Naming, OnEmpty, Pipeline, Presence, Requirement, contract_of,
    pipeline_of,
};

const INTENT: &str =
    "Read ./in/orders.csv, keep the orders above 10 and write them to ./out/large.json";

fn above_ten() -> Rule {
    Rule::typed(
        "the orders above 10",
        vec![Clause::new(
            "amount",
            Comparator::Gt,
            Operand::Number("10".to_owned()),
        )],
        Junction::And,
        Shape::default(),
    )
}

fn only(contract: &Contract) -> &super::Obligation {
    assert_eq!(contract.obligations.len(), 1, "{contract:?}");
    &contract.obligations[0]
}

fn pipeline(contract: &Contract) -> &Pipeline {
    match &only(contract).requirement {
        Requirement::Computed { pipeline, .. } => pipeline,
        other => panic!("a computed requirement, not {other:?}"),
    }
}

fn unsupported(requirement: &Requirement) -> bool {
    matches!(requirement, Requirement::Unsupported(_))
}

#[test]
fn a_rule_its_stated_source_and_its_write_make_one_computed_obligation() {
    let contract = contract_of(
        &plan_of(above_ten(), "./out/large.json", false),
        INTENT,
        &BTreeMap::new(),
    );
    let obligation = only(&contract);
    assert_eq!(obligation.id, "write ./out/large.json");
    assert_eq!(obligation.presence, Presence::Unproven);
    assert_eq!(contract.sources, ["./in/orders.csv"]);
    let target = obligation.target.as_ref().expect("a stated file");
    assert_eq!(
        (target.path.as_str(), target.format),
        ("./out/large.json", Format::Json)
    );
    let Requirement::Computed {
        source,
        source_format,
        pipeline,
        form,
    } = &obligation.requirement
    else {
        panic!("a computed requirement");
    };
    assert_eq!(
        (source.as_str(), *source_format, *form),
        ("./in/orders.csv", Format::Csv, Form::Rows)
    );
    assert_eq!(pipeline.steps.len(), 1);
    assert_eq!(pipeline.steps[0].filter.tests[0].field, "amount");
    assert!(pipeline.policies.is_empty());
    assert!(!pipeline.keep_order);
}

#[test]
fn the_relation_reads_the_typed_fields_and_never_the_lowered_jq() {
    let record = above_ten().to_json();
    let clean = pipeline_of_record(&record, &BTreeMap::new()).expect("a typed rule");
    let mut observed = record.clone();
    observed["jq"] = json!("[.records[] | select(.amount < 0)]");
    observed["fields"] = json!(["something", "else"]);
    observed["guard"] = json!("false");
    assert_eq!(
        pipeline_of_record(&observed, &BTreeMap::new()).expect("a typed rule"),
        clean
    );
    assert_eq!(
        pipeline_of(&above_ten(), &BTreeMap::new()).expect("a typed rule"),
        clean
    );
}

#[test]
fn steps_after_the_first_keep_their_stated_order() {
    let record = json!({
        "clauses": [{"field": "status", "comparator": "==", "value": "paid", "value_kind": "text"}],
        "junction": "and",
        "shape": {"group_by": "customer",
                  "aggregations": [{"field": "amount", "op": "sum", "name": "total", "round": null}]},
        "then": [{"clauses": [{"field": "total", "comparator": ">", "value": "100",
                               "value_kind": "number"}],
                  "junction": "and",
                  "shape": {"sort_by": "total", "descending": true, "limit": 3}}],
        "numbers": {"amount": "skip"}
    });
    let relation = pipeline_of_record(&record, &BTreeMap::new()).expect("two steps");
    assert_eq!(relation.steps.len(), 2);
    assert_eq!(
        relation.steps[0].stages.group_by.as_deref(),
        Some("customer")
    );
    assert_eq!(relation.steps[1].filter.tests[0].field, "total");
    let sort = relation.steps[1]
        .stages
        .sort
        .as_ref()
        .expect("the later sort");
    assert!(sort.descending && !sort.stable_ties);
    assert_eq!(relation.steps[1].stages.limit, Some(3));
    assert_eq!(relation.policies.get("amount"), Some(&NumberPolicy::Skip));
}

#[test]
fn the_empty_case_of_an_aggregate_is_stated_never_invented() {
    let mut shape = Shape::default();
    shape.aggregations = vec![
        Aggregation::new(Some("amount".to_owned()), AggOp::Sum, "total", None),
        Aggregation::new(None, AggOp::Count, "count", None),
        Aggregation::new(Some("amount".to_owned()), AggOp::Avg, "average", Some(2)),
        Aggregation::new(Some("amount".to_owned()), AggOp::Min, "least", None),
    ];
    let rule = Rule::typed(
        "the summary of the amounts",
        Vec::new(),
        Junction::And,
        shape,
    );
    let empty_of = |rule: &Rule| -> Vec<OnEmpty> {
        pipeline_of(rule, &BTreeMap::new())
            .expect("a typed rule")
            .steps[0]
            .stages
            .aggregates
            .iter()
            .map(|aggregate| aggregate.on_empty)
            .collect()
    };
    assert_eq!(
        empty_of(&rule),
        [
            OnEmpty::Zero,
            OnEmpty::Zero,
            OnEmpty::Unstated,
            OnEmpty::Unstated
        ]
    );
    let bound = rule
        .with_number_policy("amount", NumberPolicy::Fail)
        .expect("a summed field");
    assert_eq!(
        empty_of(&bound),
        [OnEmpty::Zero, OnEmpty::Zero, OnEmpty::Stops, OnEmpty::Stops]
    );
    // The plan carries no provenance for an output name: each one stays unknown.
    let relation = pipeline_of(&bound, &BTreeMap::new()).expect("a typed rule");
    assert!(
        relation.steps[0]
            .stages
            .aggregates
            .iter()
            .all(|aggregate| aggregate.naming == Naming::Unknown)
    );
}

#[test]
fn a_value_written_alone_is_the_only_bare_value_form() {
    let mut shape = Shape::default();
    shape.aggregations = vec![Aggregation::new(
        Some("amount".to_owned()),
        AggOp::Sum,
        "total",
        None,
    )];
    let total = Rule::typed("the total amount", Vec::new(), Junction::And, shape);
    let form_of = |alone: bool| {
        let contract = contract_of(
            &plan_of(total.clone(), "./out/total.json", alone),
            INTENT,
            &BTreeMap::new(),
        );
        let Requirement::Computed { form, .. } = &only(&contract).requirement else {
            return None;
        };
        Some(*form)
    };
    assert_eq!(form_of(true), Some(Form::Alone));
    assert_eq!(form_of(false), Some(Form::Totals));
    let rows = contract_of(
        &plan_of(above_ten(), "./out/large.json", true),
        INTENT,
        &BTreeMap::new(),
    );
    assert!(unsupported(&only(&rows).requirement));
}

#[test]
fn every_policy_of_a_write_has_its_presence() {
    let presence_of = |policy: EffectPolicy, literal: Option<&str>| {
        let mut plan = plan_of(above_ten(), "./out/large.json", false);
        plan.effects[0].policy = policy;
        plan.effects[0].policy_literal = literal.map(str::to_owned);
        let contract = contract_of(&plan, INTENT, &BTreeMap::new());
        (
            only(&contract).presence.clone(),
            only(&contract).requirement.clone(),
        )
    };
    // An automatic authorization does not prove that no condition governs the write.
    let (automatic, computed) = presence_of(EffectPolicy::Automatic, None);
    assert_eq!(automatic, Presence::Unproven);
    assert!(matches!(computed, Requirement::Computed { .. }));
    let (literal, kept) = presence_of(EffectPolicy::Automatic, Some("up to 100 EUR per order"));
    assert_eq!(literal, Presence::Undecided);
    assert_eq!(kept, computed);
    let (forbidden, requirement) = presence_of(EffectPolicy::Forbidden, None);
    assert_eq!(forbidden, Presence::Forbidden);
    assert_eq!(requirement, Requirement::PresenceOnly);
    assert_eq!(
        presence_of(EffectPolicy::HumanFirst, None).0,
        Presence::Approval
    );
    assert_eq!(
        presence_of(EffectPolicy::Undecided, None).0,
        Presence::Undecided
    );
    assert_eq!(
        presence_of(EffectPolicy::Conflict, None).0,
        Presence::Undecided
    );
}

#[test]
fn the_sources_the_request_names_are_part_of_the_contract() {
    let plan = plan_of(above_ten(), "./out/large.json", false);
    let two = contract_of(
        &plan,
        "Read ./in/a.csv and ./in/b.csv, keep the orders above 10 and write them to ./out/large.json",
        &BTreeMap::new(),
    );
    assert_eq!(two.sources, ["./in/a.csv", "./in/b.csv"]);
    let none = contract_of(
        &plan,
        "Keep the orders above 10 and write them to ./out/large.json",
        &BTreeMap::new(),
    );
    assert!(none.sources.is_empty(), "{:?}", none.sources);
}

#[test]
fn work_this_component_cannot_verify_stays_an_explicit_obligation() {
    let mut plan = plan_of(above_ten(), "./out/large.json", false);
    plan.steps.push(PlanStep::new(
        Op::Draft,
        "draft a note for the team",
        "a note",
        Vec::new(),
    ));
    plan.unknowns.push("and archive the old ones".to_owned());
    plan.effects.push(Effect::new(
        EffectVerb::Send,
        "the team",
        "send it to the team",
        EffectPolicy::HumanFirst,
    ));
    let contract = contract_of(&plan, INTENT, &BTreeMap::new());
    let ids: Vec<&str> = contract.obligations.iter().map(|o| o.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "operation draft",
            "unknown 1",
            "send the team",
            "write ./out/large.json"
        ]
    );
    for obligation in &contract.obligations[..3] {
        assert!(unsupported(&obligation.requirement), "{obligation:?}");
        assert!(obligation.target.is_none());
    }
    assert_eq!(contract.obligations[2].presence, Presence::Approval);
}

#[test]
fn what_the_facts_do_not_pair_is_unsupported_and_never_guessed() {
    let unsupported_with = |plan: &Plan, intent: &str| {
        let contract = contract_of(plan, intent, &BTreeMap::new());
        contract
            .obligations
            .iter()
            .filter(|o| o.id.starts_with("write"))
            .all(|o| unsupported(&o.requirement))
    };
    let mut two_rules = plan_of(above_ten(), "./out/large.json", false);
    two_rules.rules.push(above_ten());
    assert!(unsupported_with(&two_rules, INTENT));
    let mut two_writes = plan_of(above_ten(), "./out/large.json", false);
    two_writes.effects.push(Effect::new(
        EffectVerb::Write,
        "./out/copy.json",
        "and a copy",
        EffectPolicy::Automatic,
    ));
    assert!(unsupported_with(&two_writes, INTENT));
    let plan = plan_of(above_ten(), "./out/large.json", false);
    assert!(unsupported_with(
        &plan,
        "Read ./in/*.csv, keep the orders above 10 and write them to ./out/large.json"
    ));
    assert!(unsupported_with(
        &plan,
        "Read ./in/a.csv and ./in/b.csv, keep the orders above 10 and write them to ./out/large.json"
    ));
    assert!(unsupported_with(
        &plan,
        "Keep the orders above 10 and write them to ./out/large.json"
    ));
    let program = Rule::program("the stated program", ".records", vec!["amount".to_owned()]);
    assert!(unsupported_with(
        &plan_of(program, "./out/large.json", false),
        INTENT
    ));
    let mut joined = Shape::default();
    joined.join_on = Some("id".to_owned());
    let join = Rule::typed("the joined rows", Vec::new(), Junction::And, joined);
    assert!(unsupported_with(
        &plan_of(join, "./out/large.json", false),
        INTENT
    ));
    let mut grouped = Shape::default();
    grouped.group_by = Some("customer".to_owned());
    let unnamed = Rule::typed("the rows per customer", Vec::new(), Junction::And, grouped);
    assert!(unsupported_with(
        &plan_of(unnamed, "./out/large.json", false),
        INTENT
    ));
    let derived = json!({"clauses": [], "junction": "and",
        "shape": {"aggregations": [{"field": "credit", "op": "sum", "name": "credit"}],
                  "derived": [{"name": "balance", "op": "sub",
                               "left": {"name": "credit"}, "right": {"number": "1"}}]}});
    assert!(pipeline_of_record(&derived, &BTreeMap::new()).is_err());
    let lines = json!({"clauses": [], "junction": "and", "shape": {"distinct": true},
                       "lines": true});
    assert!(pipeline_of_record(&lines, &BTreeMap::new()).is_err());
}

#[test]
fn an_alluded_value_is_the_answer_the_human_gave() {
    let threshold = Rule::typed(
        "the orders above the agreed threshold",
        vec![Clause::new(
            "amount",
            Comparator::Gt,
            Operand::Slot("threshold".to_owned()),
        )],
        Junction::And,
        Shape::default(),
    );
    let plan = plan_of(threshold, "./out/large.json", false);
    let unanswered = contract_of(&plan, INTENT, &BTreeMap::new());
    assert!(unsupported(&only(&unanswered).requirement));
    let answers = BTreeMap::from([("const.threshold".to_owned(), "12.5".to_owned())]);
    let answered = contract_of(&plan, INTENT, &answers);
    let test = &pipeline(&answered).steps[0].filter.tests[0];
    assert_eq!(
        test.operand,
        super::Operand::Number(super::Decimal::from_law("12.5").expect("a number"))
    );
    let region = Rule::typed(
        "the orders of the agreed region",
        vec![Clause::new(
            "region",
            Comparator::Eq,
            Operand::Slot("region".to_owned()),
        )],
        Junction::And,
        Shape::default(),
    );
    let answers = BTreeMap::from([("const.region".to_owned(), "\"north\"".to_owned())]);
    let contract = contract_of(
        &plan_of(region, "./out/large.json", false),
        INTENT,
        &answers,
    );
    assert_eq!(
        pipeline(&contract).steps[0].filter.tests[0].operand,
        super::Operand::AsText("north".to_owned())
    );
}

#[test]
fn a_kept_order_is_a_stated_fact_of_the_request() {
    let mut plan = plan_of(above_ten(), "./out/large.json", false);
    plan.constraints
        .push("keep the order of the file".to_owned());
    let contract = contract_of(&plan, INTENT, &BTreeMap::new());
    assert!(pipeline(&contract).keep_order);
}
