// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! An output name is checked only as far as a fact states it: a stated name strictly, a name
//! proven free by its value under a key no other output reserves, an unknown name never
//! certified. A wrong value fails whatever the naming, no key serves two outputs, and no other
//! key is ignored.

use std::collections::BTreeMap;

use nika_compile_reader::aggregate::{AggOp, Aggregation, Shape};
use nika_compile_reader::rules::{Junction, Rule, synthesize};

use super::evaluate::evaluate;
use super::formats::{Format, records};
use super::judge_tests::{outcome, plan_of, run};
use super::{Contract, Naming, Outcome, Pipeline, Presence, Requirement, contract_of, pipeline_of};

const ORDERS: &str = "./in/orders.json";
const COUNT: &str = "./out/count.json";
const PAID: &str =
    r#"[{"id":1,"status":"paid"},{"id":2,"status":"late"},{"id":3,"status":"paid"}]"#;

/// The rule the reader synthesizes for « count the rows where status is paid »: its output name
/// is the verb form's alias.
fn counted() -> Rule {
    synthesize("count the rows where status is paid", &[]).expect("a rule")
}

/// The contract of `rule` written to `target`, its write proven unconditional and its output
/// names coming from `naming`, or as the plan states them when `None`.
fn written_with(rule: Rule, target: &str, naming: Option<Naming>) -> Contract {
    let intent = format!("Read ./in/orders.json, compute it and write it to {target}");
    let Contract {
        obligations,
        sources,
    } = contract_of(&plan_of(rule, target, false), &intent, &BTreeMap::new());
    let obligations = obligations
        .into_iter()
        .map(|mut obligation| {
            obligation.presence = Presence::Required;
            if let (Requirement::Computed { pipeline, .. }, Some(naming)) =
                (&mut obligation.requirement, naming)
            {
                set_naming(pipeline, naming);
            }
            obligation
        })
        .collect();
    Contract::new(obligations).with_sources(sources)
}

fn set_naming(pipeline: &mut Pipeline, naming: Naming) {
    for step in &mut pipeline.steps {
        for aggregate in &mut step.stages.aggregates {
            aggregate.naming = naming;
        }
    }
}

#[test]
fn the_reader_states_no_provenance_for_an_output_name() {
    let pipeline = pipeline_of(&counted(), &BTreeMap::new()).expect("a typed rule");
    let aggregates: Vec<_> = pipeline
        .steps
        .iter()
        .flat_map(|step| &step.stages.aggregates)
        .collect();
    assert_eq!(aggregates.len(), 1);
    assert_eq!(aggregates[0].name, "count");
    assert_eq!(aggregates[0].naming, Naming::Unknown);
    let contract = contract_of(
        &plan_of(counted(), COUNT, false),
        "Read ./in/orders.json, count the rows where status is paid and write it to ./out/count.json",
        &BTreeMap::new(),
    );
    let Requirement::Computed { pipeline, .. } = &contract.obligations[0].requirement else {
        panic!("a computed requirement");
    };
    assert!(
        pipeline
            .steps
            .iter()
            .flat_map(|step| &step.stages.aggregates)
            .all(|aggregate| aggregate.naming == Naming::Unknown)
    );
}

fn count_of(contract: &Contract, output: &str) -> Outcome {
    outcome(contract, run(ORDERS, PAID, COUNT, output))
}

#[test]
fn a_stated_name_is_strict() {
    let stated = written_with(counted(), COUNT, Some(Naming::Stated));
    assert_eq!(count_of(&stated, r#"{"count":2}"#), Outcome::Passed);
    for wrong in [r#"{"n":2}"#, r#"{"count":3}"#, r#"{"count":2,"n":2}"#, "{}"] {
        assert_eq!(count_of(&stated, wrong), Outcome::Failed, "{wrong}");
    }
}

#[test]
fn a_name_proven_free_is_matched_by_its_value_under_any_one_key() {
    // The reader's alias is the positive control once its freedom is a proven fact.
    let free = written_with(counted(), COUNT, Some(Naming::Free));
    for valid in [r#"{"count":2}"#, r#"{"n":2}"#, r#"{"paid": 2.0}"#] {
        assert_eq!(count_of(&free, valid), Outcome::Passed, "{valid}");
    }
    for wrong in [r#"{"n":3}"#, r#"{"n":2,"m":2}"#, r#"{"n":null}"#, "{}", "2"] {
        assert_eq!(count_of(&free, wrong), Outcome::Failed, "{wrong}");
    }
}

#[test]
fn an_unknown_name_is_never_certified_and_a_wrong_value_still_fails() {
    let unknown = written_with(counted(), COUNT, None);
    for right in [r#"{"count":2}"#, r#"{"n":2}"#] {
        assert_eq!(count_of(&unknown, right), Outcome::Incomplete, "{right}");
    }
    for wrong in [r#"{"n":3}"#, r#"{"count":3}"#, r#"{"count":2,"n":2}"#, "{}"] {
        assert_eq!(count_of(&unknown, wrong), Outcome::Failed, "{wrong}");
    }
}

fn per_bucket() -> Rule {
    let mut shape = Shape::default();
    shape.group_by = Some("bucket".to_owned());
    shape.aggregations = vec![Aggregation::new(None, AggOp::Count, "count", None)];
    Rule::typed("the rows per bucket", Vec::new(), Junction::And, shape)
}

#[test]
fn no_key_serves_two_outputs() {
    // The group key and the count hold the same value: one key cannot stand for both.
    let buckets = r#"[{"bucket":2},{"bucket":2}]"#;
    let judge_of = |contract: &Contract, output: &str| {
        outcome(contract, run(ORDERS, buckets, "./out/buckets.json", output))
    };
    let free = written_with(per_bucket(), "./out/buckets.json", Some(Naming::Free));
    assert_eq!(judge_of(&free, r#"[{"bucket":2,"n":2}]"#), Outcome::Passed);
    for wrong in [
        r#"[{"bucket":2}]"#,
        r#"[{"n":2}]"#,
        r#"[{"bucket":2,"n":2,"m":2}]"#,
        r#"[{"bucket":2,"n":3}]"#,
    ] {
        assert_eq!(judge_of(&free, wrong), Outcome::Failed, "{wrong}");
    }
    let unknown = written_with(per_bucket(), "./out/buckets.json", None);
    assert_eq!(
        judge_of(&unknown, r#"[{"bucket":2,"n":2}]"#),
        Outcome::Incomplete
    );
    assert_eq!(judge_of(&unknown, r#"[{"bucket":2}]"#), Outcome::Failed);
}

#[test]
fn two_outputs_without_a_stated_name_are_never_associated() {
    let mut shape = Shape::default();
    shape.aggregations = vec![
        Aggregation::new(Some("amount".to_owned()), AggOp::Sum, "total", None),
        Aggregation::new(None, AggOp::Count, "count", None),
    ];
    let rule = Rule::typed("the total and the count", Vec::new(), Junction::And, shape);
    let free = written_with(rule, "./out/summary.json", Some(Naming::Free));
    let equal = r#"[{"amount":1},{"amount":1}]"#;
    let judge_of = |output: &str| outcome(&free, run(ORDERS, equal, "./out/summary.json", output));
    for open in [r#"{"a":2,"b":2}"#, r#"{"total":2,"count":2}"#] {
        assert_eq!(judge_of(open), Outcome::Incomplete, "{open}");
    }
    for wrong in [r#"{"a":2,"b":3}"#, r#"{"a":2}"#, r#"{"a":2,"b":2,"c":2}"#] {
        assert_eq!(judge_of(wrong), Outcome::Failed, "{wrong}");
    }
}

#[test]
fn a_rename_states_the_name_it_gives_and_a_dropped_output_is_no_output() {
    let rows = || records(Format::Json, r#"[{"bucket":2},{"bucket":2}]"#).expect("records");
    let mut renamed = pipeline_of(&per_bucket(), &BTreeMap::new()).expect("a typed rule");
    renamed.steps[0].stages.renames = vec![("count".to_owned(), "n".to_owned())];
    let expected = evaluate(&renamed, rows()).expect("defined");
    assert!(expected.unstated.is_empty(), "{:?}", expected.unstated);
    let mut kept = pipeline_of(&per_bucket(), &BTreeMap::new()).expect("a typed rule");
    let expected = evaluate(&kept, rows()).expect("defined");
    assert_eq!(
        expected.unstated.get("count"),
        Some(&Naming::Unknown),
        "{:?}",
        expected.unstated
    );
    kept.steps[0].stages.columns = vec!["bucket".to_owned()];
    let projected = evaluate(&kept, rows()).expect("defined");
    assert!(projected.unstated.is_empty(), "{:?}", projected.unstated);
}

#[test]
fn two_outputs_under_one_name_are_never_reduced_to_one() {
    let buckets = r#"[{"bucket":2},{"bucket":2}]"#;
    let judge_of = |contract: &Contract, output: &str| {
        outcome(contract, run(ORDERS, buckets, "./out/buckets.json", output))
    };
    // A count whose output name is the group key's: neither one key holding both results nor
    // two keys holding them is judged against a reduced expectation.
    let mut shape = Shape::default();
    shape.group_by = Some("bucket".to_owned());
    shape.aggregations = vec![Aggregation::new(None, AggOp::Count, "bucket", None)];
    let grouped = Rule::typed("the rows per bucket", Vec::new(), Junction::And, shape);
    for naming in [Some(Naming::Free), Some(Naming::Stated), None] {
        let contract = written_with(grouped.clone(), "./out/buckets.json", naming);
        for output in [r#"[{"bucket":2}]"#, r#"[{"bucket":2,"n":2}]"#] {
            assert_eq!(
                judge_of(&contract, output),
                Outcome::Incomplete,
                "{naming:?} {output}"
            );
        }
    }
    // Two totals under one name, whatever their values.
    let mut shape = Shape::default();
    shape.aggregations = vec![
        Aggregation::new(Some("amount".to_owned()), AggOp::Sum, "total", None),
        Aggregation::new(None, AggOp::Count, "total", None),
    ];
    let totals = Rule::typed("the total and the count", Vec::new(), Junction::And, shape);
    let contract = written_with(totals, "./out/summary.json", Some(Naming::Free));
    let amounts = r#"[{"amount":1},{"amount":5}]"#;
    for output in [r#"{"total":6}"#, r#"{"a":6,"b":2}"#, r#"{"total":2}"#] {
        assert_eq!(
            outcome(
                &contract,
                run(ORDERS, amounts, "./out/summary.json", output)
            ),
            Outcome::Incomplete,
            "{output}"
        );
    }
    // The relation states why.
    let mut stages = super::Stages::default();
    stages.group_by = Some("bucket".to_owned());
    stages.aggregates = vec![super::Aggregate::new(
        AggOp::Count,
        None,
        "bucket",
        None,
        super::OnEmpty::Zero,
    )];
    let pipeline = Pipeline::new(vec![super::Step::new(super::Filter::all(), stages)]);
    let rows = records(Format::Json, buckets).expect("records");
    assert!(matches!(
        evaluate(&pipeline, rows),
        Err(super::evaluate::Undefined::Unverified(why)) if why.contains("two outputs share the name bucket")
    ));
}
