// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a round may conclude about whether a result is written: a write the plan cannot prove
//! unconditional, policy words the plan does not type, a conditional output required only
//! where its condition holds, a result written elsewhere or copied in, a missing receipt,
//! evidence that is a sample or cut, a run that never ran or ran past the budget, a fixture
//! outside the stated domain, and the preview's three parts.

use std::collections::BTreeMap;

use nika_compile_reader::aggregate::Shape;
use nika_compile_reader::plan::EffectPolicy;
use nika_compile_reader::rules::{Clause, Comparator, Junction, NumberPolicy, Operand, Rule};

use super::judge_tests::{budget, outcome, plan_of, proven, run, unwritten, usage};
use super::{
    Admission, Axis, Budget, Condition, Consumed, Contract, Coverage, Filter, Form, Format, Limits,
    Obligation, Outcome, Pipeline, Presence, ReadBack, Requirement, Run, RunEnd, Stages, Step,
    Target, Test, Usage, Verdict, contract_of, judge,
};

const ORDERS: &str = "./in/orders.json";
const LATE: &str = "./out/late.json";
const LARGE: &str = "./out/large.json";
const LARGE_INTENT: &str =
    "Read ./in/orders.json, keep the orders above 10 and write them to ./out/large.json";

fn above_ten_rule(policy: Option<NumberPolicy>) -> Rule {
    let rule = Rule::typed(
        "the orders above 10",
        vec![Clause::new(
            "amount",
            Comparator::Gt,
            Operand::Number("10".to_owned()),
        )],
        Junction::And,
        Shape::default(),
    );
    match policy {
        Some(policy) => rule
            .with_number_policy("amount", policy)
            .expect("a field read as a number"),
        None => rule,
    }
}

/// The orders above 10, as the plan alone states them.
fn above_ten(policy: Option<NumberPolicy>) -> Contract {
    contract_of(
        &plan_of(above_ten_rule(policy), LARGE, false),
        LARGE_INTENT,
        &BTreeMap::new(),
    )
}

const SOME_LARGE: &str = r#"[{"id":1,"amount":20},{"id":2,"amount":5}]"#;
const NONE_LARGE: &str = r#"[{"id":1,"amount":2},{"id":2,"amount":5}]"#;

#[test]
fn a_write_the_plan_cannot_prove_unconditional_is_never_certified() {
    let contract = above_ten(None);
    assert_eq!(contract.obligations[0].presence, Presence::Unproven);
    // Whether or not some order would satisfy a condition the plan cannot state, a right value
    // is not certified, an absence is not certified, and a wrong value fails.
    for (input, right) in [
        (SOME_LARGE, r#"[{"id":1,"amount":20}]"#),
        (NONE_LARGE, "[]"),
    ] {
        assert_eq!(
            outcome(&contract, run(ORDERS, input, LARGE, right)),
            Outcome::Incomplete,
            "{input}"
        );
        assert_eq!(
            outcome(
                &contract,
                unwritten(RunEnd::Completed, ORDERS, input, LARGE)
            ),
            Outcome::Incomplete,
            "{input}"
        );
        assert_eq!(
            outcome(
                &contract,
                run(ORDERS, input, LARGE, r#"[{"id":2,"amount":5}]"#)
            ),
            Outcome::Failed,
            "{input}"
        );
    }
    let report = judge(
        &contract,
        &[run(ORDERS, SOME_LARGE, LARGE, r#"[{"id":1,"amount":20}]"#)],
        &mut budget(),
    );
    assert_eq!(report.verdict(), Verdict::Incomplete);
    assert!(!report.certified());
}

#[test]
fn policy_words_the_plan_does_not_type_leave_the_write_undecided_and_keep_a_wrong_value() {
    let mut plan = plan_of(above_ten_rule(None), LARGE, false);
    plan.effects[0].policy_literal = Some("only orders up to 100 EUR".to_owned());
    let contract = contract_of(&plan, LARGE_INTENT, &BTreeMap::new());
    assert_eq!(contract.obligations[0].presence, Presence::Undecided);
    // The relation comes from the rule alone: the untyped words govern the write, not its value.
    assert!(matches!(
        contract.obligations[0].requirement,
        Requirement::Computed { .. }
    ));
    assert_eq!(
        outcome(
            &contract,
            run(ORDERS, SOME_LARGE, LARGE, r#"[{"id":1,"amount":20}]"#)
        ),
        Outcome::Incomplete
    );
    assert_eq!(
        outcome(
            &contract,
            unwritten(RunEnd::Completed, ORDERS, SOME_LARGE, LARGE)
        ),
        Outcome::Incomplete
    );
    assert_eq!(
        outcome(&contract, run(ORDERS, SOME_LARGE, LARGE, "[]")),
        Outcome::Failed
    );
}

fn late_filter() -> Filter {
    Filter::new(
        vec![Test::new(
            "status",
            Comparator::Eq,
            super::Operand::Text("late".to_owned()),
        )],
        Junction::And,
    )
}

/// The late orders written to their file, when or only when some order is late.
pub(super) fn late_orders(presence: fn(Condition) -> Presence) -> Contract {
    let condition = Condition::new(ORDERS, Format::Json, late_filter(), true);
    let pipeline = Pipeline::new(vec![Step::new(late_filter(), Stages::default())]);
    Contract::new(vec![Obligation::new(
        "write ./out/late.json",
        Some(Target::new(LATE, Format::Json)),
        presence(condition),
        Requirement::Computed {
            source: ORDERS.to_owned(),
            source_format: Format::Json,
            pipeline,
            form: Form::Rows,
        },
        "if some orders are late, write them to ./out/late.json",
    )])
}

const ON_TIME: &str = r#"[{"id":1,"status":"paid"}]"#;
const SOME_LATE: &str = r#"[{"id":1,"status":"paid"},{"id":2,"status":"late"}]"#;

#[test]
fn a_conditional_output_is_required_only_where_its_condition_holds() {
    let not_written = |contract: &Contract, input: &str| {
        outcome(contract, unwritten(RunEnd::Completed, ORDERS, input, LATE))
    };
    let written = |contract: &Contract, input: &str, output: &str| {
        outcome(contract, run(ORDERS, input, LATE, output))
    };
    let only = late_orders(Presence::OnlyWhen);
    assert_eq!(not_written(&only, ON_TIME), Outcome::Passed);
    assert_eq!(written(&only, ON_TIME, "[]"), Outcome::Failed);
    assert_eq!(not_written(&only, SOME_LATE), Outcome::Failed);
    assert_eq!(
        written(&only, SOME_LATE, r#"[{"id":2,"status":"late"}]"#),
        Outcome::Passed
    );
    assert_eq!(written(&only, SOME_LATE, "[]"), Outcome::Failed);
    let when = late_orders(Presence::When);
    assert_eq!(not_written(&when, ON_TIME), Outcome::Passed);
    assert_eq!(written(&when, ON_TIME, "[]"), Outcome::Passed);
    assert_eq!(written(&when, ON_TIME, r#"[{"id":9}]"#), Outcome::Failed);
    assert_eq!(not_written(&when, SOME_LATE), Outcome::Failed);
}

fn top_contract() -> Contract {
    let mut shape = Shape::default();
    shape.sort_by = Some(("amount".to_owned(), true));
    let rule = Rule::typed("the largest order", Vec::new(), Junction::And, shape).with_limit(1);
    proven(contract_of(
        &plan_of(rule, "./out/top.json", false),
        "Read ./in/orders.json, keep the largest order by amount and write it to ./out/top.json",
        &BTreeMap::new(),
    ))
}

const AMOUNTS: &str = r#"[{"id":1,"amount":3},{"id":2,"amount":5}]"#;
const LARGEST: &str = r#"[{"id":2,"amount":5}]"#;

#[test]
fn a_result_written_elsewhere_or_copied_in_is_not_written() {
    let contract = top_contract();
    assert_eq!(
        outcome(&contract, run(ORDERS, AMOUNTS, "./out/top.json", LARGEST)),
        Outcome::Passed
    );
    let elsewhere = unwritten(RunEnd::Completed, ORDERS, AMOUNTS, "./out/top.json")
        .with_read_back(ReadBack::new("./out/other.json", LARGEST));
    assert_eq!(outcome(&contract, elsewhere), Outcome::Failed);
    let copied = Run::new("observed", RunEnd::Completed, usage())
        .with_consumed(Consumed::new(ORDERS, AMOUNTS, Coverage::Complete))
        .with_read_back(ReadBack::new("./out/top.json", LARGEST).with_written(false));
    assert_eq!(outcome(&contract, copied), Outcome::Failed);
    let no_file = Contract::new(vec![Obligation::new(
        "result",
        None,
        Presence::Required,
        Requirement::Unsupported("the request names no file for this result".to_owned()),
        "keep the largest order",
    )]);
    assert_eq!(
        outcome(&no_file, run(ORDERS, AMOUNTS, "./out/top.json", LARGEST)),
        Outcome::Incomplete
    );
}

#[test]
fn a_missing_receipt_is_no_observation_of_a_write_or_of_its_absence() {
    let mut plan = plan_of(above_ten_rule(None), LARGE, false);
    plan.effects[0].policy = EffectPolicy::Forbidden;
    let forbidden = contract_of(&plan, LARGE_INTENT, &BTreeMap::new());
    assert_eq!(forbidden.obligations[0].presence, Presence::Forbidden);
    let silent = Run::new("observed", RunEnd::Completed, usage()).with_consumed(Consumed::new(
        ORDERS,
        SOME_LARGE,
        Coverage::Complete,
    ));
    assert_eq!(outcome(&forbidden, silent.clone()), Outcome::InvalidHarness);
    assert_eq!(
        outcome(
            &forbidden,
            silent.clone().with_read_back(ReadBack::unwritten(LARGE))
        ),
        Outcome::Passed
    );
    // A receipt for another path, or `./` spelled differently for the same one.
    assert_eq!(
        outcome(
            &forbidden,
            silent
                .clone()
                .with_read_back(ReadBack::unwritten("./out/other.json"))
        ),
        Outcome::InvalidHarness
    );
    assert_eq!(
        outcome(
            &forbidden,
            silent.with_read_back(ReadBack::unwritten("out/large.json"))
        ),
        Outcome::Passed
    );
    let required = top_contract();
    let none = Run::new("observed", RunEnd::Completed, usage()).with_consumed(Consumed::new(
        ORDERS,
        AMOUNTS,
        Coverage::Complete,
    ));
    assert_eq!(outcome(&required, none), Outcome::InvalidHarness);
}

#[test]
fn a_sample_a_cut_or_missing_evidence_proves_nothing() {
    let contract = top_contract();
    for coverage in [Coverage::Sampled, Coverage::Truncated] {
        let partial = Run::new("observed", RunEnd::Completed, usage())
            .with_consumed(Consumed::new(ORDERS, AMOUNTS, coverage))
            .with_read_back(ReadBack::new("./out/top.json", LARGEST));
        assert_eq!(outcome(&contract, partial), Outcome::Incomplete);
    }
    let cut = Run::new("observed", RunEnd::Completed, usage())
        .with_consumed(Consumed::new(ORDERS, AMOUNTS, Coverage::Complete))
        .with_read_back(ReadBack::new("./out/top.json", LARGEST).with_truncated(true));
    assert_eq!(outcome(&contract, cut), Outcome::Incomplete);
    let unobserved = Run::new("observed", RunEnd::Completed, usage())
        .with_read_back(ReadBack::new("./out/top.json", LARGEST));
    assert_eq!(outcome(&contract, unobserved), Outcome::InvalidHarness);
    let beyond = r#"[{"id":1,"amount":3},{"id":2,"amount":12345678901234567890123}]"#;
    assert_eq!(
        outcome(&contract, run(ORDERS, beyond, "./out/top.json", beyond)),
        Outcome::Incomplete
    );
}

#[test]
fn a_run_that_did_not_run_or_ran_past_the_budget_shows_nothing() {
    let contract = top_contract();
    let refused = Run::new(
        "observed",
        RunEnd::NotRun {
            reason: "a provider call".to_owned(),
        },
        usage(),
    );
    assert_eq!(outcome(&contract, refused), Outcome::NotRun);
    let none = judge(&contract, &[], &mut budget());
    assert_eq!(none.verdict(), Verdict::NotRun);
    assert!(!none.certified());
    let mut one_fixture = Budget::new(
        Limits::new(1, 16, 1_000_000, 60_000),
        Limits::new(64, 64, 4_000_000, 240_000),
        Usage::default(),
    );
    let runs = [
        run(ORDERS, AMOUNTS, "./out/top.json", LARGEST),
        run(ORDERS, AMOUNTS, "./out/top.json", LARGEST),
    ];
    let report = judge(&contract, &runs, &mut one_fixture);
    let findings: Vec<Outcome> = report.judged[0]
        .findings
        .iter()
        .map(|finding| finding.outcome)
        .collect();
    assert_eq!(findings, [Outcome::Passed, Outcome::NotRun]);
    assert_eq!(report.verdict(), Verdict::NotRun);
    assert_eq!(report.admission, Admission::RoundSpent(Axis::Fixtures));
    assert_eq!(report.round.fixtures, 2);
    let mut spent_turn = Budget::new(
        Limits::new(16, 16, 1_000_000, 60_000),
        Limits::new(64, 64, 4_000_000, 240_000),
        Usage::new(64, 0, 0, 0, 0),
    );
    let late = judge(&contract, &runs[..1], &mut spent_turn);
    assert_eq!(late.verdict(), Verdict::NotRun);
    assert_eq!(late.admission, Admission::TurnSpent(Axis::Fixtures));
}

#[test]
fn no_obligation_certifies_nothing() {
    let empty = judge(
        &Contract::default(),
        &[run(ORDERS, AMOUNTS, "./out/top.json", LARGEST)],
        &mut budget(),
    );
    assert!(empty.judged.is_empty());
    assert_eq!(empty.verdict(), Verdict::Incomplete);
    assert!(!empty.certified());
    assert!(!empty.failed());
}

#[test]
fn a_fixture_outside_the_domain_is_no_verdict_on_the_program() {
    let odd = r#"[{"id":1,"amount":20},{"id":2,"amount":"n/a"}]"#;
    let unstated = proven(above_ten(None));
    for output in [r#"[{"id":1,"amount":20}]"#, "[]"] {
        assert_eq!(
            outcome(&unstated, run(ORDERS, odd, LARGE, output)),
            Outcome::InvalidHarness,
            "{output}"
        );
    }
    assert_eq!(
        outcome(&unstated, run(ORDERS, "[{\"id\":1,", LARGE, "[]")),
        Outcome::InvalidHarness
    );
    // A stated policy makes the same record part of the domain: SKIP leaves it out, FAIL
    // forbids a completed run (its stop is judged in `stop_tests`).
    let skip = proven(above_ten(Some(NumberPolicy::Skip)));
    assert_eq!(
        outcome(&skip, run(ORDERS, odd, LARGE, r#"[{"id":1,"amount":20}]"#)),
        Outcome::Passed
    );
    let fail = proven(above_ten(Some(NumberPolicy::Fail)));
    for completed in [
        run(ORDERS, odd, LARGE, r#"[{"id":1,"amount":20}]"#),
        unwritten(RunEnd::Completed, ORDERS, odd, LARGE),
    ] {
        assert_eq!(outcome(&fail, completed), Outcome::Failed);
    }
}

#[test]
fn the_report_keeps_the_requested_result_the_assumptions_and_the_observed_proof_apart() {
    let contract = top_contract();
    let report = judge(
        &contract,
        &[run(ORDERS, AMOUNTS, "./out/top.json", LARGEST)],
        &mut budget(),
    );
    assert!(report.certified());
    assert!(report.scorable());
    let judged = &report.judged[0];
    assert_eq!(judged.obligation, "write ./out/top.json");
    assert!(
        judged.requested.contains("rows from ./in/orders.json"),
        "{}",
        judged.requested
    );
    assert!(
        judged.requested.contains("sort by amount descending"),
        "{}",
        judged.requested
    );
    assert!(
        judged
            .assumptions
            .iter()
            .any(|a| a.contains("a cut through tied rows may keep any of them")),
        "{:?}",
        judged.assumptions
    );
    let finding = &judged.findings[0];
    assert_eq!(finding.fixture, "observed");
    let source = finding
        .evidence
        .iter()
        .find(|identity| identity.path == ORDERS)
        .expect("the consumed source is identified");
    assert_eq!(
        source.sha256,
        super::formats::sha256_hex(AMOUNTS.as_bytes())
    );
    assert_eq!(source.records, Some(2));
    assert_eq!(source.coverage, Coverage::Complete);
    assert!(
        finding
            .evidence
            .iter()
            .any(|identity| identity.path == "./out/top.json")
    );
}
