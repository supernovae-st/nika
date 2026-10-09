// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Judgment of contracts derived from synthetic plans, each with false-accept controls (a
//! plausible but wrong output must fail) and false-reject controls (several independently
//! written valid outputs must pass). These contracts take their presence and their output
//! names as proven (`proven`), as a lower layer that traces them to the request would state
//! them; the plan alone proves neither, which `presence_tests` and `names_tests` judge. The
//! verdict reads values, never source bytes: key order, spacing and number spelling never
//! matter.

use std::collections::BTreeMap;

use nika_compile_reader::aggregate::{AggOp, Aggregation, Shape};
use nika_compile_reader::plan::{Effect, EffectPolicy, EffectVerb, Op, Plan, Step as PlanStep};
use nika_compile_reader::rules::{Clause, Comparator, Junction, NumberPolicy, Operand, Rule};

use super::{
    Budget, Cause, Consumed, Contract, Coverage, Failure, Limits, Naming, Outcome, Presence,
    ReadBack, Requirement, Run, RunEnd, Usage, contract_of, judge,
};

/// A plan that reads its source, computes `rule` and writes `target` without a human gate.
pub(super) fn plan_of(rule: Rule, target: &str, alone: bool) -> Plan {
    let mut plan = Plan::default();
    plan.steps
        .push(PlanStep::new(Op::Read, "read the source", "", Vec::new()));
    plan.steps
        .push(PlanStep::new(Op::Compute, "compute it", "", Vec::new()));
    let mut write = Effect::new(
        EffectVerb::Write,
        target,
        "write the result",
        EffectPolicy::Automatic,
    );
    write.alone = alone;
    plan.effects.push(write);
    plan.rules.push(rule);
    plan
}

/// The same contract with every write proven unconditional and every output name stated: the
/// facts a lower layer that traces them to the request would add.
pub(super) fn proven(contract: Contract) -> Contract {
    let Contract {
        obligations,
        sources,
    } = contract;
    let obligations = obligations
        .into_iter()
        .map(|mut obligation| {
            if obligation.presence == Presence::Unproven {
                obligation.presence = Presence::Required;
            }
            if let Requirement::Computed { pipeline, .. } = &mut obligation.requirement {
                for step in &mut pipeline.steps {
                    for aggregate in &mut step.stages.aggregates {
                        aggregate.naming = Naming::Stated;
                    }
                }
            }
            obligation
        })
        .collect();
    Contract::new(obligations).with_sources(sources)
}

pub(super) fn usage() -> Usage {
    Usage::new(1, 1, 256, 256, 20)
}

pub(super) fn budget() -> Budget {
    Budget::new(
        Limits::new(16, 16, 1_000_000, 60_000),
        Limits::new(64, 64, 4_000_000, 240_000),
        Usage::default(),
    )
}

/// A completed run that consumed `input` from `source` and wrote `output` at `target`.
pub(super) fn run(source: &str, input: &str, target: &str, output: &str) -> Run {
    Run::new("observed", RunEnd::Completed, usage())
        .with_consumed(Consumed::new(source, input, Coverage::Complete))
        .with_read_back(ReadBack::new(target, output))
}

/// A run that ended with `end`, consumed `input` from `source` and did not write `target`.
pub(super) fn unwritten(end: RunEnd, source: &str, input: &str, target: &str) -> Run {
    Run::new("observed", end, usage())
        .with_consumed(Consumed::new(source, input, Coverage::Complete))
        .with_read_back(ReadBack::unwritten(target))
}

/// A run that failed for `cause`.
pub(super) fn failed(cause: Cause) -> RunEnd {
    RunEnd::Failed(Failure::new("compute", "NIKA-RUNTIME-001", cause))
}

/// The outcome of the one obligation of `contract` on the one fixture `run`.
pub(super) fn outcome(contract: &Contract, run: Run) -> Outcome {
    let report = judge(contract, &[run], &mut budget());
    assert_eq!(report.judged.len(), 1, "{report:?}");
    let findings = &report.judged[0].findings;
    assert_eq!(findings.len(), 1, "{report:?}");
    findings[0].outcome
}

const ORDERS: &str = "./in/orders.json";
const TOP: &str = "./out/top.json";
const TOP_INTENT: &str =
    "Read ./in/orders.json, keep the 2 largest orders by amount and write them to ./out/top.json";

/// The 2 largest orders by amount, ties in file order or not, under a stated policy or not.
pub(super) fn top(stable: bool, policy: Option<NumberPolicy>) -> Contract {
    let mut shape = Shape::default();
    shape.sort_by = Some(("amount".to_owned(), true));
    shape.ties_first_in_file = stable;
    let rule = Rule::typed(
        "the 2 largest orders by amount",
        Vec::new(),
        Junction::And,
        shape,
    )
    .with_limit(2);
    let rule = if let Some(policy) = policy {
        rule.with_number_policy("amount", policy)
            .expect("the ranked field reads numbers")
    } else {
        rule
    };
    proven(contract_of(
        &plan_of(rule, TOP, false),
        TOP_INTENT,
        &BTreeMap::new(),
    ))
}

fn top_of(contract: &Contract, input: &str, output: &str) -> Outcome {
    outcome(contract, run(ORDERS, input, TOP, output))
}

#[test]
fn a_top_n_needs_the_sorted_prefix_its_size_and_its_members() {
    let contract = top(false, None);
    let input =
        r#"[{"id":1,"amount":9},{"id":2,"amount":8},{"id":3,"amount":7},{"id":4,"amount":1}]"#;
    // Two valid outputs written independently: spacing, key order and number spelling differ.
    for valid in [
        r#"[{"id":1,"amount":9},{"id":2,"amount":8}]"#,
        "[\n  {\"amount\": 9.0, \"id\": 1},\n  {\"amount\": 8e0, \"id\": 2.00}\n]\n",
    ] {
        assert_eq!(top_of(&contract, input, valid), Outcome::Passed, "{valid}");
    }
    for wrong in [
        r#"[{"id":1,"amount":9}]"#,
        "[]",
        r#"[{"id":3,"amount":7},{"id":4,"amount":1}]"#,
        r#"[{"id":2,"amount":8},{"id":1,"amount":9}]"#,
        r#"[{"id":1,"amount":9},{"id":2,"amount":8},{"id":3,"amount":7}]"#,
        r#"[{"id":9,"amount":99},{"id":8,"amount":98}]"#,
        r#"[{"id":1,"amount":"9"},{"id":2,"amount":"8"}]"#,
        r#"{"rows":[{"id":1,"amount":9},{"id":2,"amount":8}]}"#,
    ] {
        assert_eq!(top_of(&contract, input, wrong), Outcome::Failed, "{wrong}");
    }
}

#[test]
fn a_cut_through_tied_rows_admits_any_valid_choice_unless_file_order_is_stated() {
    let input =
        r#"[{"id":1,"amount":9},{"id":2,"amount":8},{"id":3,"amount":8},{"id":4,"amount":1}]"#;
    let first = r#"[{"id":1,"amount":9},{"id":2,"amount":8}]"#;
    let second = r#"[{"id":1,"amount":9},{"id":3,"amount":8}]"#;
    // Under a stated number policy as without one, a completed run's valid choice is no missed
    // stop: the stop through distinct ties is an alternative, never an obligation.
    for open in [top(false, None), top(false, Some(NumberPolicy::Fail))] {
        assert_eq!(top_of(&open, input, first), Outcome::Passed);
        assert_eq!(top_of(&open, input, second), Outcome::Passed);
        for wrong in [
            r#"[{"id":1,"amount":9},{"id":4,"amount":1}]"#,
            r#"[{"id":2,"amount":8},{"id":3,"amount":8}]"#,
            r#"[{"id":1,"amount":9},{"id":2,"amount":8},{"id":3,"amount":8}]"#,
        ] {
            assert_eq!(top_of(&open, input, wrong), Outcome::Failed, "{wrong}");
        }
    }
    let stable = top(true, None);
    assert_eq!(top_of(&stable, input, first), Outcome::Passed);
    assert_eq!(top_of(&stable, input, second), Outcome::Failed);
}

const ACTIVE_INTENT: &str =
    "Read ./in/users.json, keep the active users and write their id and name to ./out/active.json";

fn active(columns: &[&str]) -> Contract {
    let mut shape = Shape::default();
    shape.columns = columns.iter().map(|c| (*c).to_owned()).collect();
    let rule = Rule::typed(
        "the active users",
        vec![Clause::new("active", Comparator::Eq, Operand::Bool(true))],
        Junction::And,
        shape,
    );
    proven(contract_of(
        &plan_of(rule, "./out/active.json", false),
        ACTIVE_INTENT,
        &BTreeMap::new(),
    ))
}

#[test]
fn a_projection_drops_the_filtered_column_and_keeps_every_kept_row() {
    let contract = active(&["id", "name"]);
    let users = r#"[{"id":1,"name":"a","active":true,"email":"x"},
                    {"id":2,"name":"b","active":false,"email":"y"},
                    {"id":3,"name":"c","active":"true","email":"z"}]"#;
    let judge_of = |output: &str| {
        outcome(
            &contract,
            run("./in/users.json", users, "./out/active.json", output),
        )
    };
    for valid in [
        r#"[{"id":1,"name":"a"},{"id":3,"name":"c"}]"#,
        r#"[{"name":"c","id":3},{"name":"a","id":1}]"#,
    ] {
        assert_eq!(judge_of(valid), Outcome::Passed, "{valid}");
    }
    for wrong in [
        r#"[{"id":1,"name":"a","active":true},{"id":3,"name":"c","active":"true"}]"#,
        r#"[{"id":1,"name":"a"},{"id":2,"name":"b"},{"id":3,"name":"c"}]"#,
        r#"[{"id":1,"name":"a"}]"#,
        "[]",
    ] {
        assert_eq!(judge_of(wrong), Outcome::Failed, "{wrong}");
    }
}

#[test]
fn duplicate_rows_keep_their_multiplicity() {
    let mut shape = Shape::default();
    shape.columns = vec!["sku".to_owned()];
    let rule = Rule::typed(
        "the skus of the large lines",
        vec![Clause::new(
            "qty",
            Comparator::Gt,
            Operand::Number("5".to_owned()),
        )],
        Junction::And,
        shape,
    );
    let contract = proven(contract_of(
        &plan_of(rule, "./out/skus.json", false),
        "Read ./in/lines.csv, keep the skus of the lines above 5 and write them to ./out/skus.json",
        &BTreeMap::new(),
    ));
    let lines = "sku,qty\nA,9\nA,7\nB,1\n";
    let judge_of = |output: &str| {
        outcome(
            &contract,
            run("./in/lines.csv", lines, "./out/skus.json", output),
        )
    };
    assert_eq!(judge_of(r#"[{"sku":"A"},{"sku":"A"}]"#), Outcome::Passed);
    for wrong in [
        r#"[{"sku":"A"}]"#,
        r#"[{"sku":"A"},{"sku":"A"},{"sku":"A"}]"#,
        r#"[{"sku":"A"},{"sku":"B"}]"#,
    ] {
        assert_eq!(judge_of(wrong), Outcome::Failed, "{wrong}");
    }
}

#[test]
fn distinct_values_are_complete_and_without_duplicates() {
    let mut shape = Shape::default();
    shape.columns = vec!["email".to_owned()];
    shape.distinct = true;
    let rule = Rule::typed("the distinct emails", Vec::new(), Junction::And, shape);
    let contract = proven(contract_of(
        &plan_of(rule, "./out/emails.json", false),
        "Read ./in/signups.csv, keep the distinct emails and write them to ./out/emails.json",
        &BTreeMap::new(),
    ));
    let signups = "email,plan\na@x,free\nb@x,pro\na@x,pro\n";
    let judge_of = |output: &str| {
        outcome(
            &contract,
            run("./in/signups.csv", signups, "./out/emails.json", output),
        )
    };
    for valid in [
        r#"[{"email":"a@x"},{"email":"b@x"}]"#,
        r#"[{"email":"b@x"},{"email":"a@x"}]"#,
    ] {
        assert_eq!(judge_of(valid), Outcome::Passed, "{valid}");
    }
    for wrong in [
        "[]",
        r#"[{"email":"a@x"}]"#,
        r#"[{"email":"a@x"},{"email":"b@x"},{"email":"a@x"}]"#,
    ] {
        assert_eq!(judge_of(wrong), Outcome::Failed, "{wrong}");
    }
}

const SALES: &str = "./in/sales.csv";
const SALES_INTENT: &str =
    "Read ./in/sales.csv, sum the amount of the north rows and write the total to ./out/total.json";

fn north_total(target: &str, alone: bool) -> Contract {
    let mut shape = Shape::default();
    shape.aggregations = vec![Aggregation::new(
        Some("amount".to_owned()),
        AggOp::Sum,
        "total",
        None,
    )];
    let rule = Rule::typed(
        "the total amount of the north rows",
        vec![Clause::new(
            "region",
            Comparator::Eq,
            Operand::Text("north".to_owned()),
        )],
        Junction::And,
        shape,
    );
    let intent = SALES_INTENT.replace("./out/total.json", target);
    proven(contract_of(
        &plan_of(rule, target, alone),
        &intent,
        &BTreeMap::new(),
    ))
}

#[test]
fn a_value_stated_alone_is_a_bare_value_and_otherwise_the_named_total() {
    let sales = "region,amount\nnorth,20\nsouth,400\nnorth,50.5\n";
    let alone = north_total("./out/total.json", true);
    let judge_of = |contract: &Contract, target: &str, output: &str| {
        outcome(contract, run(SALES, sales, target, output))
    };
    for valid in ["70.5", "7.05e1\n", " 70.50 "] {
        assert_eq!(
            judge_of(&alone, "./out/total.json", valid),
            Outcome::Passed,
            "{valid}"
        );
    }
    for wrong in [r#"{"total":70.5}"#, "\"70.5\"", "470.5", "[70.5]"] {
        assert_eq!(
            judge_of(&alone, "./out/total.json", wrong),
            Outcome::Failed,
            "{wrong}"
        );
    }
    let text = north_total("./out/total.txt", true);
    assert_eq!(
        judge_of(&text, "./out/total.txt", "70.5\n"),
        Outcome::Passed
    );
    assert_eq!(
        judge_of(&text, "./out/total.txt", "the total is 70.5\n"),
        Outcome::Failed
    );
    let named = north_total("./out/total.json", false);
    for valid in [r#"{"total":70.5}"#, "{ \"total\" : 70.50 }"] {
        assert_eq!(
            judge_of(&named, "./out/total.json", valid),
            Outcome::Passed,
            "{valid}"
        );
    }
    for wrong in [
        "70.5",
        r#"{"total":"70.5"}"#,
        r#"{"total":470.5}"#,
        r#"{"total":70.5,"count":2}"#,
        r#"{"sum":70.5}"#,
    ] {
        assert_eq!(
            judge_of(&named, "./out/total.json", wrong),
            Outcome::Failed,
            "{wrong}"
        );
    }
}

#[test]
fn an_exact_total_is_not_its_binary_float_and_an_empty_input_has_its_stated_total() {
    let named = north_total("./out/total.json", false);
    let judge_of =
        |sales: &str, output: &str| outcome(&named, run(SALES, sales, "./out/total.json", output));
    let decimals = "region,amount\nnorth,0.1\nnorth,0.2\n";
    assert_eq!(judge_of(decimals, r#"{"total":0.3}"#), Outcome::Passed);
    assert_eq!(
        judge_of(decimals, r#"{"total":0.30000000000000004}"#),
        Outcome::Failed
    );
    let header_only = "region,amount\n";
    assert_eq!(judge_of(header_only, r#"{"total":0}"#), Outcome::Passed);
    for wrong in [r#"{"total":null}"#, "{}", "[]"] {
        assert_eq!(judge_of(header_only, wrong), Outcome::Failed, "{wrong}");
    }
}

#[test]
fn an_average_of_nothing_is_never_judged_against_an_invented_value() {
    let mut shape = Shape::default();
    shape.aggregations = vec![Aggregation::new(
        Some("amount".to_owned()),
        AggOp::Avg,
        "average",
        None,
    )];
    let rule = Rule::typed("the average amount", Vec::new(), Junction::And, shape);
    let contract = proven(contract_of(
        &plan_of(rule, "./out/average.json", false),
        "Read ./in/sales.csv, average the amount and write it to ./out/average.json",
        &BTreeMap::new(),
    ));
    let judge_of = |sales: &str, output: &str| {
        outcome(&contract, run(SALES, sales, "./out/average.json", output))
    };
    for any in [r#"{"average":0}"#, r#"{"average":null}"#] {
        assert_eq!(
            judge_of("region,amount\n", any),
            Outcome::Incomplete,
            "{any}"
        );
    }
    assert_eq!(
        judge_of("region,amount\nn,1\ns,2\n", r#"{"average":1.5}"#),
        Outcome::Passed
    );
    assert_eq!(
        judge_of("region,amount\nn,1\ns,2\n", r#"{"average":1}"#),
        Outcome::Failed
    );
}

#[test]
fn groups_are_judged_as_a_set_of_rows_in_any_order() {
    let mut shape = Shape::default();
    shape.group_by = Some("customer".to_owned());
    shape.aggregations = vec![Aggregation::new(
        Some("amount".to_owned()),
        AggOp::Sum,
        "total",
        None,
    )];
    let rule = Rule::typed(
        "the total amount per customer",
        Vec::new(),
        Junction::And,
        shape,
    );
    let contract = proven(contract_of(
        &plan_of(rule, "./out/totals.json", false),
        "Read ./in/orders.csv, total the amount per customer and write it to ./out/totals.json",
        &BTreeMap::new(),
    ));
    let orders = "customer,amount\na,10\nb,5\na,2.5\n";
    let judge_of = |output: &str| {
        outcome(
            &contract,
            run("./in/orders.csv", orders, "./out/totals.json", output),
        )
    };
    for valid in [
        r#"[{"customer":"a","total":12.5},{"customer":"b","total":5}]"#,
        r#"[{"total":5,"customer":"b"},{"customer":"a","total":12.50}]"#,
    ] {
        assert_eq!(judge_of(valid), Outcome::Passed, "{valid}");
    }
    for wrong in [
        r#"[{"customer":"a","total":17.5},{"customer":"b","total":17.5}]"#,
        r#"[{"customer":"a","total":12.5}]"#,
        r#"[{"customer":"a","total":10},{"customer":"b","total":5},{"customer":"a","total":2.5}]"#,
        r#"[{"customer":"a","sum":12.5},{"customer":"b","sum":5}]"#,
    ] {
        assert_eq!(judge_of(wrong), Outcome::Failed, "{wrong}");
    }
}

#[test]
fn a_csv_output_agrees_with_its_values_read_by_value() {
    let contract = active(&["id", "name"]);
    let contract = Contract::new(
        contract
            .obligations
            .into_iter()
            .map(|mut obligation| {
                if let Some(target) = obligation.target.as_mut() {
                    target.path = "./out/active.csv".to_owned();
                    target.format = super::Format::Csv;
                }
                obligation
            })
            .collect(),
    );
    let users = r#"[{"id":1,"name":"a","active":true},{"id":2,"name":"b","active":false}]"#;
    let judge_of = |output: &str| {
        outcome(
            &contract,
            run("./in/users.json", users, "./out/active.csv", output),
        )
    };
    assert_eq!(judge_of("id,name\n1,a\n"), Outcome::Passed);
    assert_eq!(judge_of("name,id\na,1.0\n"), Outcome::Passed);
    assert_eq!(judge_of("id,name\n2,b\n"), Outcome::Failed);
    assert_eq!(judge_of("id,id\n1,1\n"), Outcome::Failed);
}
