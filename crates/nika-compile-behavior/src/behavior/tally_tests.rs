// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! No outcome outranks another: a tally counts every finding, and a verdict states its
//! dominance — an invalid harness, then a defect, then a fixture that did not run, then an open
//! case — whatever the order of the fixtures or of the obligations. A fixture whose evidence
//! contradicts itself is invalid in any order, while what a run consumed and what the host
//! read back after it are never compared.

use nika_compile_reader::rules::{Comparator, Junction, NumberPolicy};

use super::judge_tests::{budget, run, top, unwritten, usage};
use super::{
    Consumed, Contract, Coverage, Decimal, Filter, Form, Format, Obligation, Outcome, Pipeline,
    Presence, ReadBack, Requirement, Run, RunEnd, Stages, Step, Tally, Target, Test, Verdict,
    judge,
};

#[test]
fn a_tally_counts_every_outcome_and_states_its_dominance() {
    use Outcome::{Failed, Incomplete, InvalidHarness, NotRun, Passed};
    for (outcomes, verdict) in [
        (vec![Passed, InvalidHarness], Verdict::InvalidHarness),
        (vec![InvalidHarness, Passed], Verdict::InvalidHarness),
        (vec![Failed, InvalidHarness], Verdict::InvalidHarness),
        (vec![InvalidHarness, Failed], Verdict::InvalidHarness),
        (vec![Failed, NotRun], Verdict::Defective),
        (vec![NotRun, Failed], Verdict::Defective),
        (vec![NotRun, Incomplete], Verdict::NotRun),
        (vec![Incomplete, Passed], Verdict::Incomplete),
        (vec![Passed, Passed], Verdict::Certified),
        (Vec::new(), Verdict::NotRun),
    ] {
        let tally = Tally::of(outcomes.iter().copied());
        assert_eq!(tally.verdict(), verdict, "{outcomes:?}");
        assert_eq!(tally.total(), outcomes.len());
    }
    let mixed = Tally::of([Failed, InvalidHarness, Passed, Failed]);
    assert_eq!(
        (mixed.failed, mixed.invalid_harness, mixed.passed),
        (2, 1, 1)
    );
    assert_eq!(mixed.plus(&mixed).failed, 4);
}

const ORDERS: &str = "./in/orders.json";
const TOP: &str = "./out/top.json";
const AMOUNTS: &str = r#"[{"id":1,"amount":9},{"id":2,"amount":8},{"id":3,"amount":1}]"#;
const BEST: &str = r#"[{"id":1,"amount":9},{"id":2,"amount":8}]"#;
const WRONG: &str = r#"[{"id":1,"amount":9},{"id":3,"amount":1}]"#;

fn contradictory() -> Run {
    run(ORDERS, AMOUNTS, TOP, BEST).with_read_back(ReadBack::new("out/top.json", WRONG))
}

#[test]
fn a_report_keeps_a_defect_beside_an_invalid_fixture_in_either_order() {
    let contract = top(false, None);
    let defect = run(ORDERS, AMOUNTS, TOP, WRONG);
    for runs in [
        [defect.clone(), contradictory()],
        [contradictory(), defect.clone()],
    ] {
        let report = judge(&contract, &runs, &mut budget());
        let tally = report.tally();
        assert_eq!((tally.failed, tally.invalid_harness), (1, 1));
        assert_eq!(report.verdict(), Verdict::InvalidHarness);
        assert!(report.failed() && report.harness_invalid());
        assert!(!report.scorable() && !report.certified());
    }
    let two = [
        run(ORDERS, AMOUNTS, TOP, BEST),
        Run::new(
            "variant",
            RunEnd::NotRun {
                reason: "a provider call".to_owned(),
            },
            usage(),
        ),
    ];
    let report = judge(&contract, &two, &mut budget());
    assert_eq!(report.verdict(), Verdict::NotRun);
    assert!(report.scorable() && !report.failed() && !report.certified());
}

/// The amounts above 5 of the orders, written to `target`.
fn above_five(target: &str) -> Obligation {
    let five = Decimal::from_law("5").expect("a number");
    let filter = Filter::new(
        vec![Test::new(
            "amount",
            Comparator::Gt,
            super::Operand::Number(five),
        )],
        Junction::And,
    );
    Obligation::new(
        format!("write {target}"),
        Some(Target::new(target, Format::Json)),
        Presence::Required,
        Requirement::Computed {
            source: ORDERS.to_owned(),
            source_format: Format::Json,
            pipeline: Pipeline::new(vec![Step::new(filter, Stages::default())]),
            form: Form::Rows,
        },
        "keep the amounts above 5",
    )
}

fn never(target: &str, presence: Presence) -> Obligation {
    Obligation::new(
        format!("write {target}"),
        Some(Target::new(target, Format::Json)),
        presence,
        Requirement::PresenceOnly,
        "never write it",
    )
}

#[test]
fn obligations_combine_without_an_order() {
    let kept = "./out/kept.json";
    let fixture = run(ORDERS, AMOUNTS, kept, BEST);
    let pass = above_five(kept);
    // The forbidden file has no receipt: its obligation is an invalid harness.
    let unobserved = never("./out/never.json", Presence::Forbidden);
    for obligations in [
        vec![pass.clone(), unobserved.clone()],
        vec![unobserved.clone(), pass.clone()],
    ] {
        let report = judge(
            &Contract::new(obligations),
            std::slice::from_ref(&fixture),
            &mut budget(),
        );
        let mut verdicts: Vec<String> = report
            .judged
            .iter()
            .map(|judged| format!("{:?}", judged.verdict()))
            .collect();
        verdicts.sort();
        assert_eq!(verdicts, ["Certified", "InvalidHarness"]);
        assert_eq!(report.verdict(), Verdict::InvalidHarness);
    }
    let waiting = never("./out/approved.json", Presence::Approval);
    let defect = run(ORDERS, AMOUNTS, kept, WRONG)
        .with_read_back(ReadBack::unwritten("./out/approved.json"));
    for obligations in [
        vec![pass.clone(), waiting.clone()],
        vec![waiting.clone(), pass.clone()],
    ] {
        let report = judge(
            &Contract::new(obligations),
            std::slice::from_ref(&defect),
            &mut budget(),
        );
        assert_eq!(report.verdict(), Verdict::Defective);
        let tally = report.tally();
        assert_eq!((tally.failed, tally.not_run), (1, 1));
    }
}

#[test]
fn contradictory_observations_make_the_fixture_invalid_in_any_order() {
    let contract = top(false, None);
    let other_input = r#"[{"id":1,"amount":1}]"#;
    let consumed = |first: (&str, &str, Coverage), second: (&str, &str, Coverage)| {
        Run::new("observed", RunEnd::Completed, usage())
            .with_consumed(Consumed::new(first.0, first.1, first.2))
            .with_consumed(Consumed::new(second.0, second.1, second.2))
            .with_read_back(ReadBack::new(TOP, BEST))
    };
    let complete = Coverage::Complete;
    for (first, second, expected) in [
        (
            (ORDERS, AMOUNTS, complete),
            ("in/orders.json", other_input, complete),
            Outcome::InvalidHarness,
        ),
        (
            ("in/orders.json", other_input, complete),
            (ORDERS, AMOUNTS, complete),
            Outcome::InvalidHarness,
        ),
        (
            (ORDERS, AMOUNTS, complete),
            (ORDERS, AMOUNTS, Coverage::Sampled),
            Outcome::InvalidHarness,
        ),
        (
            (ORDERS, AMOUNTS, complete),
            ("in/orders.json", AMOUNTS, complete),
            Outcome::Passed,
        ),
    ] {
        let outcome = super::judge_tests::outcome(&contract, consumed(first, second));
        assert_eq!(outcome, expected, "{first:?} {second:?}");
    }
    let read_back = |first: ReadBack, second: ReadBack| {
        Run::new("observed", RunEnd::Completed, usage())
            .with_consumed(Consumed::new(ORDERS, AMOUNTS, complete))
            .with_read_back(first)
            .with_read_back(second)
    };
    for (first, second, expected) in [
        (
            ReadBack::new(TOP, BEST),
            ReadBack::new("out/top.json", WRONG),
            Outcome::InvalidHarness,
        ),
        (
            ReadBack::new("out/top.json", WRONG),
            ReadBack::new(TOP, BEST),
            Outcome::InvalidHarness,
        ),
        (
            ReadBack::new(TOP, BEST),
            ReadBack::new(TOP, BEST).with_written(false),
            Outcome::InvalidHarness,
        ),
        (
            ReadBack::new(TOP, BEST),
            ReadBack::new("out/top.json", BEST),
            Outcome::Passed,
        ),
    ] {
        let label = format!("{first:?} {second:?}");
        let outcome = super::judge_tests::outcome(&contract, read_back(first, second));
        assert_eq!(outcome, expected, "{label}");
    }
    // A file the run consumed and later rewrote: the initial and the final observation differ
    // and contradict nothing.
    let rewritten =
        run(ORDERS, AMOUNTS, TOP, BEST).with_consumed(Consumed::new(TOP, "[]", complete));
    assert_eq!(
        super::judge_tests::outcome(&contract, rewritten),
        Outcome::Passed
    );
    // A policy changes nothing here: the contradiction is judged before the end.
    let bound = top(false, Some(NumberPolicy::Fail));
    assert_eq!(
        super::judge_tests::outcome(&bound, contradictory()),
        Outcome::InvalidHarness
    );
    assert_eq!(
        super::judge_tests::outcome(
            &bound,
            unwritten(RunEnd::Completed, ORDERS, AMOUNTS, TOP)
                .with_read_back(ReadBack::new("out/top.json", BEST))
        ),
        Outcome::InvalidHarness
    );
}
