// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A stop is never certified: a structured terminal fact consistent with the stop a stated rule
//! predicts names its operation, not the data and policy that governed it, so it stays
//! unattested, while a stop no stated rule predicts is a defect. One FAIL fixture and one tie
//! fixture are judged under every end: a consistent stop, a stop of another occurrence, a code
//! or a cause alone, a host bound, an established engine failure, an invalid observation, and
//! a result written before the stop. A stop never hides a result already written, and an
//! invalidity established before the end survives it.

use std::collections::BTreeMap;

use nika_compile_reader::aggregate::{AggOp, Shape};
use nika_compile_reader::plan::EffectPolicy;
use nika_compile_reader::rules::{Clause, Comparator, Junction, NumberPolicy, Operand, Rule};

use super::judge_tests::{budget, failed, outcome, plan_of, proven, run, top, unwritten, usage};
use super::presence_tests::late_orders;
use super::{
    Aggregate, Cause, Condition, Consumed, Contract, Coverage, Decimal, Failure, Filter, Form,
    Format, Naming, Obligation, OnEmpty, Operation, Outcome, Pipeline, Presence, ReadBack, Report,
    Requirement, Run, RunEnd, Stages, Step, StopFact, StopReason, Target, Test, Verdict,
    contract_of, judge,
};

const ORDERS: &str = "./in/orders.json";
const LARGE: &str = "./out/large.json";
const TOP: &str = "./out/top.json";
const ODD: &str = r#"[{"id":1,"amount":20},{"id":2,"amount":"n/a"}]"#;
const NUMBERS: &str = r#"[{"id":1,"amount":20},{"id":2,"amount":5}]"#;
const TIES: &str =
    r#"[{"id":1,"amount":9},{"id":2,"amount":8},{"id":3,"amount":8},{"id":4,"amount":1}]"#;

/// The orders above 10, written unconditionally, under a stated policy or none.
fn large(policy: Option<NumberPolicy>) -> Contract {
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
    let rule = match policy {
        Some(policy) => rule
            .with_number_policy("amount", policy)
            .expect("a field read as a number"),
        None => rule,
    };
    proven(contract_of(
        &plan_of(rule, LARGE, false),
        "Read ./in/orders.json, keep the orders above 10 and write them to ./out/large.json",
        &BTreeMap::new(),
    ))
}

fn not_a_number(source: &str, operation: Operation, field: &str, value: Option<&str>) -> Cause {
    Cause::Stop(StopFact::new(
        source,
        operation,
        field,
        StopReason::NotANumber {
            value: value.map(str::to_owned),
        },
    ))
}

/// The structured fact of the stop the FAIL policy predicts on `ODD`.
fn amount_stop() -> Cause {
    not_a_number(ORDERS, Operation::Test, "amount", Some("\"n/a\""))
}

fn tied_cut(field: &str) -> Cause {
    Cause::Stop(StopFact::new(
        ORDERS,
        Operation::Rank,
        field,
        StopReason::TiedCut,
    ))
}

fn ended_with(end: RunEnd, input: &str, target: &str, output: &str) -> Run {
    Run::new("observed", end, usage())
        .with_consumed(Consumed::new(ORDERS, input, Coverage::Complete))
        .with_read_back(ReadBack::new(target, output))
}

#[test]
fn a_stop_consistent_with_the_prediction_stays_unattested() {
    let fail = large(Some(NumberPolicy::Fail));
    for consistent in [
        amount_stop(),
        not_a_number("in/orders.json", Operation::Test, "amount", Some("\"n/a\"")),
    ] {
        assert_eq!(
            outcome(
                &fail,
                unwritten(failed(consistent.clone()), ORDERS, ODD, LARGE)
            ),
            Outcome::Incomplete,
            "{consistent:?}"
        );
    }
    // A result written before the stop is one the stated rule forbids.
    let undue = ended_with(
        failed(amount_stop()),
        ODD,
        LARGE,
        r#"[{"id":1,"amount":20}]"#,
    );
    assert_eq!(outcome(&fail, undue), Outcome::Failed);
    // Under a stated policy a cut through distinct tied rows admits a stop or any valid choice:
    // the choice completes as requested, the stop stays unattested.
    let bound = top(false, Some(NumberPolicy::Fail));
    let first = r#"[{"id":1,"amount":9},{"id":2,"amount":8}]"#;
    for choice in [first, r#"[{"id":1,"amount":9},{"id":3,"amount":8}]"#] {
        assert_eq!(
            outcome(&bound, run(ORDERS, TIES, TOP, choice)),
            Outcome::Passed
        );
    }
    assert_eq!(
        outcome(
            &bound,
            unwritten(failed(tied_cut("amount")), ORDERS, TIES, TOP)
        ),
        Outcome::Incomplete
    );
    let after_tie = |output: &str| ended_with(failed(tied_cut("amount")), TIES, TOP, output);
    assert_eq!(outcome(&bound, after_tie(first)), Outcome::Incomplete);
    assert_eq!(
        outcome(
            &bound,
            after_tie(r#"[{"id":1,"amount":9},{"id":4,"amount":1}]"#)
        ),
        Outcome::Failed
    );
}

#[test]
fn a_stop_no_stated_rule_predicts_is_a_defect() {
    let fail = large(Some(NumberPolicy::Fail));
    for other in [
        not_a_number(ORDERS, Operation::Aggregate, "amount", Some("\"n/a\"")),
        not_a_number(ORDERS, Operation::Condition, "amount", Some("\"n/a\"")),
        not_a_number(
            "./in/refunds.json",
            Operation::Test,
            "amount",
            Some("\"n/a\""),
        ),
        not_a_number(ORDERS, Operation::Test, "amount", Some("\"x\"")),
        not_a_number(ORDERS, Operation::Test, "amount", None),
        not_a_number(ORDERS, Operation::Test, "qty", Some("\"n/a\"")),
        Cause::Stop(StopFact::new(
            ORDERS,
            Operation::Test,
            "amount",
            StopReason::NoNumber,
        )),
    ] {
        assert_eq!(
            outcome(&fail, unwritten(failed(other.clone()), ORDERS, ODD, LARGE)),
            Outcome::Failed,
            "{other:?}"
        );
    }
    let bound = top(false, Some(NumberPolicy::Fail));
    assert_eq!(
        outcome(&bound, unwritten(failed(tied_cut("id")), ORDERS, TIES, TOP)),
        Outcome::Failed
    );
    // Without a stated policy a cut through ties has its valid choices and no stop.
    let open = top(false, None);
    assert_eq!(
        outcome(
            &open,
            unwritten(failed(tied_cut("amount")), ORDERS, TIES, TOP)
        ),
        Outcome::Failed
    );
}

const REFUNDS: &str = "./in/refunds.json";
const REFUNDED: &str = "./out/refunds.json";

/// The amounts above 10 of `source`, written to `target`, under `policy`.
fn filtered(source: &str, target: &str, policy: NumberPolicy) -> Obligation {
    let ten = Decimal::from_law("10").expect("a number");
    let filter = Filter::new(
        vec![Test::new(
            "amount",
            Comparator::Gt,
            super::Operand::Number(ten),
        )],
        Junction::And,
    );
    let mut pipeline = Pipeline::new(vec![Step::new(filter, Stages::default())]);
    pipeline.policies.insert("amount".to_owned(), policy);
    Obligation::new(
        format!("write {target}"),
        Some(Target::new(target, Format::Json)),
        Presence::Required,
        Requirement::Computed {
            source: source.to_owned(),
            source_format: Format::Json,
            pipeline,
            form: Form::Rows,
        },
        "keep the amounts above 10",
    )
}

/// The total amount of the orders of `status`, under FAIL, written to `target`.
fn summed(status: &str, target: &str) -> Obligation {
    let filter = Filter::new(
        vec![Test::new(
            "status",
            Comparator::Eq,
            super::Operand::Text(status.to_owned()),
        )],
        Junction::And,
    );
    let total = Aggregate::new(
        AggOp::Sum,
        Some("amount".to_owned()),
        "total",
        None,
        OnEmpty::Zero,
    )
    .with_naming(Naming::Stated);
    let stages = Stages {
        aggregates: vec![total],
        ..Stages::default()
    };
    let mut pipeline = Pipeline::new(vec![Step::new(filter, stages)]);
    pipeline
        .policies
        .insert("amount".to_owned(), NumberPolicy::Fail);
    Obligation::new(
        format!("write {target}"),
        Some(Target::new(target, Format::Json)),
        Presence::Required,
        Requirement::Computed {
            source: ORDERS.to_owned(),
            source_format: Format::Json,
            pipeline,
            form: Form::Totals,
        },
        format!("sum the amounts of the {status} orders"),
    )
}

fn outcomes(report: &Report) -> Vec<Vec<Outcome>> {
    report
        .judged
        .iter()
        .map(|judged| judged.findings.iter().map(|f| f.outcome).collect())
        .collect()
}

#[test]
fn two_occurrences_of_one_field_and_value_are_never_told_apart() {
    // Two sources hold the same non-number; only the orders are read under FAIL.
    let sources = Contract::new(vec![
        filtered(ORDERS, LARGE, NumberPolicy::Fail),
        filtered(REFUNDS, REFUNDED, NumberPolicy::Skip),
    ]);
    let round = |cause: Cause, refunds: Option<&str>| {
        let refunds_read = refunds.map_or_else(
            || ReadBack::unwritten(REFUNDED),
            |text| ReadBack::new(REFUNDED, text),
        );
        let fixture = Run::new("observed", failed(cause), usage())
            .with_consumed(Consumed::new(ORDERS, ODD, Coverage::Complete))
            .with_consumed(Consumed::new(REFUNDS, ODD, Coverage::Complete))
            .with_read_back(ReadBack::unwritten(LARGE))
            .with_read_back(refunds_read);
        outcomes(&judge(&sources, &[fixture], &mut budget()))
    };
    let refunds_stop = not_a_number(REFUNDS, Operation::Test, "amount", Some("\"n/a\""));
    assert_eq!(
        round(refunds_stop, None),
        [[Outcome::Failed], [Outcome::Failed]]
    );
    let right = r#"[{"id":1,"amount":20}]"#;
    for (refunds, second) in [
        (None, Outcome::Incomplete),
        (Some(right), Outcome::Incomplete),
        (Some("[]"), Outcome::Failed),
    ] {
        assert_eq!(
            round(amount_stop(), refunds),
            [[Outcome::Incomplete], [second]],
            "{refunds:?}"
        );
    }
    // One source, one operation, two transformations: the late total stops on the value, and a
    // paid total that wrongly read every row would report the very same fact.
    let mixed = r#"[{"id":1,"status":"paid","amount":20},{"id":2,"status":"late","amount":"n/a"}]"#;
    let totals = Contract::new(vec![
        summed("paid", "./out/paid.json"),
        summed("late", "./out/late.json"),
    ]);
    let fact = not_a_number(ORDERS, Operation::Aggregate, "amount", Some("\"n/a\""));
    for (paid, first) in [
        (None, Outcome::Incomplete),
        (Some(r#"{"total":20}"#), Outcome::Incomplete),
        (Some(r#"{"total":0}"#), Outcome::Failed),
    ] {
        let paid_read = paid.map_or_else(
            || ReadBack::unwritten("./out/paid.json"),
            |text| ReadBack::new("./out/paid.json", text),
        );
        let fixture = Run::new("observed", failed(fact.clone()), usage())
            .with_consumed(Consumed::new(ORDERS, mixed, Coverage::Complete))
            .with_read_back(paid_read)
            .with_read_back(ReadBack::unwritten("./out/late.json"));
        assert_eq!(
            outcomes(&judge(&totals, &[fixture], &mut budget())),
            [[first], [Outcome::Incomplete]],
            "{paid:?}"
        );
    }
}

#[test]
fn a_code_or_a_cause_alone_never_passes() {
    let fail = large(Some(NumberPolicy::Fail));
    let on_fail = |end: RunEnd| outcome(&fail, unwritten(end, ORDERS, ODD, LARGE));
    // A code that names a stop is still no structured fact.
    let worded = RunEnd::Failed(Failure::new(
        "compute",
        "NIKA-STOP-NOT-A-NUMBER",
        Cause::Unclassified,
    ));
    for (end, expected) in [
        (failed(Cause::Unclassified), Outcome::Incomplete),
        (worded, Outcome::Incomplete),
        (failed(Cause::TimeBound), Outcome::NotRun),
        // An established cause decides even where a stated rule predicts a stop.
        (failed(Cause::Engine), Outcome::Failed),
        // The host copied the source whole: the run lost it.
        (
            failed(Cause::MissingFile {
                path: ORDERS.to_owned(),
            }),
            Outcome::Failed,
        ),
        (
            failed(Cause::MissingFile {
                path: "./in/rates.json".to_owned(),
            }),
            Outcome::Failed,
        ),
        (
            failed(Cause::Unparsable {
                path: ORDERS.to_owned(),
            }),
            Outcome::Failed,
        ),
        (
            RunEnd::InvalidHarness {
                reason: "the room was not restored".to_owned(),
            },
            Outcome::InvalidHarness,
        ),
        (
            RunEnd::NotRun {
                reason: "a provider call".to_owned(),
            },
            Outcome::NotRun,
        ),
        (RunEnd::Completed, Outcome::Failed),
    ] {
        assert_eq!(on_fail(end.clone()), expected, "{end:?}");
    }
    let malformed = unwritten(
        failed(Cause::Unparsable {
            path: ORDERS.to_owned(),
        }),
        ORDERS,
        "[{\"id\":1,",
        LARGE,
    );
    assert_eq!(outcome(&fail, malformed), Outcome::InvalidHarness);
    let bound = top(false, Some(NumberPolicy::Fail));
    for (cause, expected) in [
        (Cause::Engine, Outcome::Failed),
        (Cause::Unclassified, Outcome::Incomplete),
    ] {
        assert_eq!(
            outcome(&bound, unwritten(failed(cause.clone()), ORDERS, TIES, TOP)),
            expected,
            "{cause:?}"
        );
    }
    // Where no stop is predicted or possible, a failure is a defect, as a missing result is.
    let plain = large(None);
    for end in [
        failed(Cause::Unclassified),
        failed(Cause::Engine),
        RunEnd::Completed,
    ] {
        assert_eq!(
            outcome(&plain, unwritten(end.clone(), ORDERS, NUMBERS, LARGE)),
            Outcome::Failed,
            "{end:?}"
        );
    }
    let report = judge(
        &plain,
        &[unwritten(failed(Cause::Engine), ORDERS, NUMBERS, LARGE)],
        &mut budget(),
    );
    assert_eq!(report.verdict(), Verdict::Defective);
    assert!(report.scorable());
}

/// A write of `./out/large.json` the request forbids, its source named.
fn never_large() -> Contract {
    let mut plan = plan_of(
        Rule::typed("every order", Vec::new(), Junction::And, Shape::default()),
        LARGE,
        false,
    );
    plan.effects[0].policy = EffectPolicy::Forbidden;
    contract_of(
        &plan,
        "Read ./in/orders.json and never write ./out/large.json",
        &BTreeMap::new(),
    )
}

#[test]
fn an_open_that_finds_no_file_is_judged_by_the_recorded_copy() {
    let lost = || {
        failed(Cause::MissingFile {
            path: ORDERS.to_owned(),
        })
    };
    // A whole recorded copy: the run lost the source.
    assert_eq!(
        outcome(&large(None), unwritten(lost(), ORDERS, NUMBERS, LARGE)),
        Outcome::Failed
    );
    // A partial record attests no whole copy.
    let fail = large(Some(NumberPolicy::Fail));
    let sampled = Run::new("observed", lost(), usage())
        .with_consumed(Consumed::new(ORDERS, ODD, Coverage::Sampled))
        .with_read_back(ReadBack::unwritten(LARGE));
    assert_eq!(outcome(&fail, sampled), Outcome::Incomplete);
    // No record of the copy proves no initial absence: the failure settles nothing. A relation
    // that reads the source still has no evidence of its records, which no cause supplies.
    let unrecorded =
        Run::new("observed", lost(), usage()).with_read_back(ReadBack::unwritten(LARGE));
    assert_eq!(
        outcome(&never_large(), unrecorded.clone()),
        Outcome::Incomplete
    );
    assert_eq!(outcome(&fail, unrecorded), Outcome::InvalidHarness);
    // A fixture that lacked the source is the host's to attest.
    let attested = Run::new(
        "observed",
        RunEnd::InvalidHarness {
            reason: "the room lacks ./in/orders.json".to_owned(),
        },
        usage(),
    );
    assert_eq!(outcome(&never_large(), attested), Outcome::InvalidHarness);
}

const ON_TIME: &str = r#"[{"id":1,"status":"paid"}]"#;
const LATE: &str = "./out/late.json";

#[test]
fn an_absence_never_passes_a_run_that_failed() {
    let mut plan = plan_of(
        Rule::typed("every order", Vec::new(), Junction::And, Shape::default()),
        LARGE,
        false,
    );
    plan.effects[0].policy = EffectPolicy::Forbidden;
    let forbidden = contract_of(
        &plan,
        "Read ./in/orders.json and never write ./out/large.json",
        &BTreeMap::new(),
    );
    let only_when = late_orders(Presence::OnlyWhen);
    let ends = [
        (RunEnd::Completed, Outcome::Passed),
        (failed(Cause::Unclassified), Outcome::Failed),
        (failed(Cause::Engine), Outcome::Failed),
        (failed(amount_stop()), Outcome::Failed),
        (failed(Cause::TimeBound), Outcome::NotRun),
        (
            RunEnd::NotRun {
                reason: "a provider call".to_owned(),
            },
            Outcome::NotRun,
        ),
        (
            RunEnd::InvalidHarness {
                reason: "the room was not restored".to_owned(),
            },
            Outcome::InvalidHarness,
        ),
    ];
    for (end, expected) in ends {
        assert_eq!(
            outcome(&forbidden, unwritten(end.clone(), ORDERS, NUMBERS, LARGE)),
            expected,
            "forbidden: {end:?}"
        );
        assert_eq!(
            outcome(&only_when, unwritten(end.clone(), ORDERS, ON_TIME, LATE)),
            expected,
            "only when: {end:?}"
        );
    }
}

const MIXED: &str =
    r#"[{"id":1,"status":"paid","amount":20},{"id":2,"status":"late","amount":"n/a"}]"#;
const PAID: &str = "./out/paid.json";

/// The paid orders under `presence`, then the orders above 10 under FAIL, from one source.
fn paid_then_large(presence: Presence) -> Contract {
    let paid = Filter::new(
        vec![Test::new(
            "status",
            Comparator::Eq,
            super::Operand::Text("paid".to_owned()),
        )],
        Junction::And,
    );
    let requirement = if presence == Presence::Forbidden {
        Requirement::PresenceOnly
    } else {
        Requirement::Computed {
            source: ORDERS.to_owned(),
            source_format: Format::Json,
            pipeline: Pipeline::new(vec![Step::new(paid, Stages::default())]),
            form: Form::Rows,
        }
    };
    let first = Obligation::new(
        "write ./out/paid.json",
        Some(Target::new(PAID, Format::Json)),
        presence,
        requirement,
        "write the paid orders to ./out/paid.json",
    );
    Contract::new(vec![first, filtered(ORDERS, LARGE, NumberPolicy::Fail)])
}

fn refunded() -> Condition {
    let refunded = Filter::new(
        vec![Test::new(
            "status",
            Comparator::Eq,
            super::Operand::Text("refunded".to_owned()),
        )],
        Junction::And,
    );
    Condition::new(ORDERS, Format::Json, refunded, true)
}

#[test]
fn a_stop_never_hides_a_result_already_written() {
    let round = |presence: Presence, cause: Cause, paid: Option<&str>| {
        let paid_read = paid.map_or_else(
            || ReadBack::unwritten(PAID),
            |text| ReadBack::new(PAID, text),
        );
        let fixture = Run::new("observed", failed(cause), usage())
            .with_consumed(Consumed::new(ORDERS, MIXED, Coverage::Complete))
            .with_read_back(paid_read)
            .with_read_back(ReadBack::unwritten(LARGE));
        outcomes(&judge(
            &paid_then_large(presence),
            &[fixture],
            &mut budget(),
        ))
    };
    let right = r#"[{"id":1,"status":"paid","amount":20}]"#;
    // Whether the later stop is a structured fact or no fact at all, it is unattested: what was
    // written before it keeps its defects, and nothing is certified.
    for cause in [amount_stop(), Cause::Unclassified] {
        for (presence, paid, first) in [
            (Presence::Required, Some(right), Outcome::Incomplete),
            (Presence::Required, Some("[]"), Outcome::Failed),
            (Presence::Required, None, Outcome::Incomplete),
            (Presence::OnlyWhen(refunded()), Some(right), Outcome::Failed),
            (Presence::OnlyWhen(refunded()), None, Outcome::Incomplete),
            (Presence::Approval, Some(right), Outcome::Failed),
            (Presence::Approval, None, Outcome::Incomplete),
            (Presence::Forbidden, Some(right), Outcome::Failed),
            (Presence::Forbidden, None, Outcome::Incomplete),
        ] {
            assert_eq!(
                round(presence.clone(), cause.clone(), paid),
                [[first], [Outcome::Incomplete]],
                "{cause:?} {presence:?} {paid:?}"
            );
        }
    }
    // An established cause is a defect of every result, whatever was written.
    assert_eq!(
        round(Presence::Required, Cause::Engine, Some(right)),
        [[Outcome::Failed], [Outcome::Failed]]
    );
}

const LINES: &str = "./in/lines.json";
const LINES_OUT: &str = "./out/lines.json";

#[test]
fn an_invalidity_established_before_the_end_survives_it() {
    let mut odd_lines = filtered(LINES, LINES_OUT, NumberPolicy::Fail);
    if let Requirement::Computed { pipeline, .. } = &mut odd_lines.requirement {
        pipeline.policies = BTreeMap::from([("qty".to_owned(), NumberPolicy::Fail)]);
        pipeline.steps[0].filter.tests[0].field = "qty".to_owned();
    }
    let mut out_of_domain = filtered(ORDERS, LARGE, NumberPolicy::Fail);
    if let Requirement::Computed { pipeline, .. } = &mut out_of_domain.requirement {
        pipeline.policies.clear();
    }
    let contract = Contract::new(vec![out_of_domain, odd_lines]);
    let lines = r#"[{"id":1,"qty":"x"}]"#;
    let round = |end: RunEnd| {
        let fixture = Run::new("observed", end, usage())
            .with_consumed(Consumed::new(ORDERS, ODD, Coverage::Complete))
            .with_consumed(Consumed::new(LINES, lines, Coverage::Complete))
            .with_read_back(ReadBack::unwritten(LARGE))
            .with_read_back(ReadBack::unwritten(LINES_OUT));
        judge(&contract, &[fixture], &mut budget())
    };
    let qty_stop = not_a_number(LINES, Operation::Test, "qty", Some("\"x\""));
    for end in [failed(qty_stop), failed(Cause::Unclassified)] {
        let report = round(end);
        assert_eq!(
            outcomes(&report),
            [[Outcome::InvalidHarness], [Outcome::Incomplete]]
        );
        assert_eq!(report.verdict(), Verdict::InvalidHarness);
        assert!(report.harness_invalid() && !report.scorable() && !report.certified());
        assert!(!report.failed());
    }
    for end in [failed(Cause::Engine), RunEnd::Completed] {
        let report = round(end);
        assert_eq!(
            outcomes(&report),
            [[Outcome::InvalidHarness], [Outcome::Failed]]
        );
        assert!(report.failed() && report.harness_invalid());
        assert_eq!(report.verdict(), Verdict::InvalidHarness);
    }
}
