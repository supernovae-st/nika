// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The behavioural contract through its public surface only, as a host that rehearses
//! candidates uses it: a contract from the reader's plan of a synthetic request, rehearsals
//! built from what each run consumed and wrote, one round's report. The plan alone proves no
//! write unconditional, so its contract never certifies; once a lower layer proves the write,
//! two outputs written independently both pass and a plausible wrong one fails. A conditional
//! file is required only where its condition holds, a failed run never passes by an absence,
//! a stop is never a pass, and a run that did not run proves nothing.

// The reader's shape and this crate's stages are `#[non_exhaustive]`: a crate outside them
// builds them field by field from `Default`.
#![allow(clippy::field_reassign_with_default)]

use std::collections::BTreeMap;

use nika_compile_fidelity::behavior::{
    Budget, Cause, Condition, Consumed, Contract, Coverage, Decimal, Failure, Filter, Form, Format,
    Judged, Limits, Obligation, Operand, Operation, Pipeline, Presence, ReadBack, Report,
    Requirement, Run, RunEnd, Stages, Step, StopFact, StopReason, Target, Test, Usage, Verdict,
    contract_of, judge,
};
use nika_compile_reader::aggregate::Shape;
use nika_compile_reader::plan::{Effect, EffectPolicy, EffectVerb, Op, Plan, Step as PlanStep};
use nika_compile_reader::rules::{Comparator, Junction, NumberPolicy, Rule};

const INTENT: &str =
    "Read ./in/scores.json, keep the 2 best players by score and write them to ./out/best.json";
const SCORES: &str = r#"[{"player":"a","score":7},{"player":"b","score":9},
                         {"player":"c","score":7},{"player":"d","score":3}]"#;

fn best_two() -> Contract {
    let mut shape = Shape::default();
    shape.sort_by = Some(("score".to_owned(), true));
    let rule = Rule::typed(
        "the 2 best players by score",
        Vec::new(),
        Junction::And,
        shape,
    )
    .with_limit(2);
    let mut plan = Plan::default();
    plan.steps.push(PlanStep::new(
        Op::Read,
        "Read ./in/scores.json",
        "",
        Vec::new(),
    ));
    plan.steps.push(PlanStep::new(
        Op::Compute,
        "keep the 2 best players by score",
        "",
        Vec::new(),
    ));
    plan.effects.push(Effect::new(
        EffectVerb::Write,
        "./out/best.json",
        "write them to ./out/best.json",
        EffectPolicy::Automatic,
    ));
    plan.rules.push(rule);
    contract_of(&plan, INTENT, &BTreeMap::new())
}

fn rehearsal(end: RunEnd) -> Run {
    Run::new("observed", end, Usage::new(1, 1, 128, 64, 15)).with_consumed(Consumed::new(
        "./in/scores.json",
        SCORES,
        Coverage::Complete,
    ))
}

fn round(contract: &Contract, runs: &[Run]) -> Report {
    let mut budget = Budget::new(
        Limits::new(8, 8, 1_000_000, 30_000),
        Limits::new(32, 32, 4_000_000, 120_000),
        Usage::default(),
    );
    judge(contract, runs, &mut budget)
}

fn verdicts(report: &Report) -> Vec<Verdict> {
    report.judged.iter().map(Judged::verdict).collect()
}

#[test]
fn a_contract_from_the_request_judges_rehearsals_by_value() {
    let mut contract = best_two();
    assert_eq!(contract.obligations.len(), 1);
    assert_eq!(contract.sources, ["./in/scores.json"]);
    assert!(contract.obligations.iter().all(|obligation| matches!(
        obligation.requirement,
        Requirement::Computed {
            form: Form::Rows,
            ..
        }
    )));
    let best = |output: &str| {
        rehearsal(RunEnd::Completed).with_read_back(ReadBack::new("./out/best.json", output))
    };
    let right = r#"[{"player":"b","score":9},{"player":"a","score":7}]"#;
    let wrong = r#"[{"player":"b","score":9},{"player":"d","score":3}]"#;
    // The plan proves no write unconditional: a right value is not certified, a wrong one fails.
    assert!(matches!(
        contract.obligations[0].presence,
        Presence::Unproven
    ));
    assert_eq!(
        verdicts(&round(&contract, &[best(right)])),
        [Verdict::Incomplete]
    );
    assert_eq!(
        verdicts(&round(&contract, &[best(wrong)])),
        [Verdict::Defective]
    );
    // Once a lower layer proves the write unconditional, the value decides.
    contract.obligations[0].presence = Presence::Required;
    // Player b leads; a and c tie at 7 for the second place: either one completes the top 2.
    for valid in [
        right,
        "[{\"score\": 9.0, \"player\": \"b\"}, {\"score\": 7, \"player\": \"c\"}]",
    ] {
        let report = round(&contract, &[best(valid)]);
        assert_eq!(verdicts(&report), [Verdict::Certified], "{valid}");
        assert!(report.certified() && report.scorable());
    }
    for invalid in [
        r#"[{"player":"b","score":9}]"#,
        r#"[{"player":"a","score":7},{"player":"b","score":9}]"#,
        wrong,
    ] {
        let report = round(&contract, &[best(invalid)]);
        assert_eq!(verdicts(&report), [Verdict::Defective], "{invalid}");
        assert!(report.failed());
    }
    let absent =
        rehearsal(RunEnd::Completed).with_read_back(ReadBack::unwritten("./out/best.json"));
    assert_eq!(verdicts(&round(&contract, &[absent])), [Verdict::Defective]);
    // No receipt for the result: no observation, never a verdict on the program.
    assert_eq!(
        verdicts(&round(&contract, &[rehearsal(RunEnd::Completed)])),
        [Verdict::InvalidHarness]
    );
}

fn alerts() -> Contract {
    let low = Filter::new(
        vec![Test::new(
            "score",
            Comparator::Lt,
            Operand::Number(Decimal::from_law("5").unwrap_or_else(Decimal::zero)),
        )],
        Junction::And,
    );
    let mut stages = Stages::default();
    stages.columns = vec!["player".to_owned()];
    Contract::new(vec![Obligation::new(
        "write ./out/alerts.json",
        Some(Target::new("./out/alerts.json", Format::Json)),
        Presence::OnlyWhen(Condition::new(
            "./in/scores.json",
            Format::Json,
            low.clone(),
            true,
        )),
        Requirement::Computed {
            source: "./in/scores.json".to_owned(),
            source_format: Format::Json,
            pipeline: Pipeline::new(vec![Step::new(low, stages)]),
            form: Form::Rows,
        },
        "if a player scored under 5, write their names to ./out/alerts.json",
    )])
}

#[test]
fn a_conditional_file_and_a_run_that_did_not_run_are_never_a_pass() {
    let alerts = alerts();
    let written = rehearsal(RunEnd::Completed)
        .with_read_back(ReadBack::new("./out/alerts.json", r#"[{"player":"d"}]"#));
    assert_eq!(verdicts(&round(&alerts, &[written])), [Verdict::Certified]);
    let absent =
        |end: RunEnd| rehearsal(end).with_read_back(ReadBack::unwritten("./out/alerts.json"));
    assert_eq!(
        verdicts(&round(&alerts, &[absent(RunEnd::Completed)])),
        [Verdict::Defective]
    );
    // A failed run is no answer, whatever it did not write.
    let failure = RunEnd::Failed(Failure::new(
        "alert",
        "NIKA-RUNTIME-001",
        Cause::Unclassified,
    ));
    assert_eq!(
        verdicts(&round(&alerts, &[absent(failure)])),
        [Verdict::Defective]
    );
    let not_run = Run::new(
        "observed",
        RunEnd::NotRun {
            reason: "stopped at the time bound".to_owned(),
        },
        Usage::new(1, 1, 128, 0, 30_000),
    );
    let report = round(&alerts, &[not_run]);
    assert_eq!(verdicts(&report), [Verdict::NotRun]);
    assert!(!report.certified());
    let nothing = round(&Contract::default(), &[rehearsal(RunEnd::Completed)]);
    assert!(nothing.judged.is_empty());
    assert!(!nothing.certified());
}

#[test]
fn a_stop_is_never_a_pass_and_an_established_failure_is_a_defect() {
    let mut contract = best_two();
    contract.obligations[0].presence = Presence::Required;
    // The request states that a score that is no number stops the run.
    if let Requirement::Computed { pipeline, .. } = &mut contract.obligations[0].requirement {
        pipeline.policies = BTreeMap::from([("score".to_owned(), NumberPolicy::Fail)]);
    }
    assert!(matches!(
        &contract.obligations[0].requirement,
        Requirement::Computed { pipeline, .. } if !pipeline.policies.is_empty()
    ));
    let odd = r#"[{"player":"a","score":7},{"player":"b","score":"none"}]"#;
    let stopped = |cause: Cause| {
        Run::new(
            "observed",
            RunEnd::Failed(Failure::new("rank", "NIKA-RUNTIME-001", cause)),
            Usage::new(1, 1, 128, 0, 15),
        )
        .with_consumed(Consumed::new("./in/scores.json", odd, Coverage::Complete))
        .with_read_back(ReadBack::unwritten("./out/best.json"))
    };
    let fact = |operation: Operation| {
        Cause::Stop(StopFact::new(
            "./in/scores.json",
            operation,
            "score",
            StopReason::NotANumber {
                value: Some("\"none\"".to_owned()),
            },
        ))
    };
    // Consistent with the predicted stop, the fact still names no data independent of the
    // workflow: the stop stays unattested.
    assert_eq!(
        verdicts(&round(&contract, &[stopped(fact(Operation::Rank))])),
        [Verdict::Incomplete]
    );
    assert_eq!(
        verdicts(&round(&contract, &[stopped(fact(Operation::Aggregate))])),
        [Verdict::Defective]
    );
    assert_eq!(
        verdicts(&round(&contract, &[stopped(Cause::Unclassified)])),
        [Verdict::Incomplete]
    );
    assert_eq!(
        verdicts(&round(&contract, &[stopped(Cause::Engine)])),
        [Verdict::Defective]
    );
}
