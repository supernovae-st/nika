// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A text file copied as is: the closed sentence `copy SOURCE as is to TARGET` proves the write
//! required and its content exactly the text the run consumed from SOURCE. The runs are
//! synthetic: each states what a host's rehearsal read back on one world, as a double states it,
//! and no real run is claimed. Each world's expected copy is its own source, never a reference
//! workflow: a final newline, CRLF line endings, a non-ASCII letter and a literal that looks
//! like a template, and the empty file over a stale target.

use std::collections::BTreeMap;

use nika_compile_reader::plan::{
    Binding, Effect, EffectPolicy, EffectVerb, Op, Plan, Step as PlanStep,
};
use nika_compile_reader::rules::synthesize;

use super::formats::sha256_hex;
use super::judge_tests::{budget, failed, outcome, usage};
use super::provenance::production;
use super::{
    Candidate, Cause, Choice, Consumed, Contract, Coverage, Format, Limits, Outcome, Presence,
    ReadBack, Requirement, Run, RunEnd, Usage, Verdict, Written, contract_of, contract_of_request,
    judge, read_request, select,
};

const INTENT: &str = "Copy ./in/source.txt as is to ./out/copied.txt";
const SOURCE: &str = "./in/source.txt";
const TARGET: &str = "./out/copied.txt";
const ALPHA: &str = "alpha\n";
const CRLF: &str = "beta\r\ncafé ${{ const.not_code }}\n";
/// The worlds: a name, the source's text, and what the target held before the run, if anything.
const WORLDS: [(&str, &str, Option<&str>); 3] = [
    ("alpha", ALPHA, None),
    ("crlf-unicode-template", CRLF, Some(ALPHA)),
    ("empty", "", Some("stale")),
];

/// The contract the request alone states.
fn contract() -> Contract {
    contract_of_request(INTENT, &BTreeMap::new())
}

/// A rehearsal named `fixture` that ended `end`, consumed `source` whole from the source, and
/// left the target as `read_back` shows it.
fn attempt(fixture: &str, end: RunEnd, source: &str, read_back: ReadBack) -> Run {
    Run::new(fixture, end, usage())
        .with_consumed(Consumed::new(SOURCE, source, Coverage::Complete))
        .with_read_back(read_back)
}

/// A completed rehearsal on a world whose source holds `source`.
fn completed(source: &str, read_back: ReadBack) -> Run {
    attempt("world", RunEnd::Completed, source, read_back)
}

/// The target as the host read it back after the run published `text` there.
fn published(text: &str) -> ReadBack {
    ReadBack::new(TARGET, text)
}

/// The target as the host read it back when the run did not write it: what it held before.
fn untouched(before: Option<&str>) -> ReadBack {
    before.map_or_else(
        || ReadBack::unwritten(TARGET),
        |text| ReadBack::new(TARGET, text).with_written(false),
    )
}

/// The outcome of the copy's one obligation on one run.
fn copy_outcome(run: Run) -> Outcome {
    outcome(&contract(), run)
}

/// The plan the reader states for a copy, built by hand: one read of `source`, one automatic
/// write of `target`, and the bindings of the two paths.
fn copy_plan(source: &str, target: &str) -> Plan {
    let mut plan = Plan::default();
    plan.steps
        .push(PlanStep::new(Op::Read, INTENT, source, Vec::new()));
    plan.effects.push(Effect::new(
        EffectVerb::Write,
        target,
        INTENT,
        EffectPolicy::Automatic,
    ));
    plan.bindings.push(Binding::new("path", source));
    plan.bindings.push(Binding::new("path", target));
    plan
}

#[test]
fn the_copy_request_proves_a_required_copy_of_its_source() {
    let (plan, provenance) = read_request(INTENT);
    assert!(provenance.admitted(), "{:?}", provenance.rejections);
    let produced = provenance.production.as_ref().expect("the copy sentence");
    assert_eq!(produced.written, Written::Copy);
    assert_eq!(
        (produced.source.as_str(), produced.target.as_str()),
        (SOURCE, TARGET)
    );
    let contract = contract();
    let [write] = contract.obligations.as_slice() else {
        panic!("one obligation: {contract:?}");
    };
    assert_eq!(write.presence, Presence::Required);
    assert_eq!(
        write.requirement,
        Requirement::CopyText {
            source: SOURCE.to_owned()
        }
    );
    let target = write.target.as_ref().expect("the copy's file");
    assert_eq!(
        (target.path.as_str(), target.format),
        (TARGET, Format::Text)
    );
    assert!(contract.sources.iter().any(|source| source == SOURCE));
    // The plan alone proves neither the write nor its content: the closed sentence does.
    let unproven = contract_of(&plan, INTENT, &BTreeMap::new());
    let [write] = unproven.obligations.as_slice() else {
        panic!("one obligation: {unproven:?}");
    };
    assert_eq!(write.presence, Presence::Unproven);
    assert!(matches!(write.requirement, Requirement::Unsupported(_)));
}

#[test]
fn an_exact_copy_is_certified_on_every_world() {
    let contract = contract();
    let runs: Vec<Run> = WORLDS
        .iter()
        .map(|(name, source, _)| attempt(name, RunEnd::Completed, source, published(source)))
        .collect();
    for run in &runs {
        assert_eq!(outcome(&contract, run.clone()), Outcome::Passed, "{run:?}");
    }
    let report = judge(&contract, &runs, &mut budget());
    assert_eq!(report.verdict(), Verdict::Certified, "{report:?}");
    // The preview states the relation and its text-only limit.
    let judged = &report.judged[0];
    assert!(judged.requested.contains(SOURCE), "{}", judged.requested);
    assert!(
        judged
            .assumptions
            .iter()
            .any(|assumption| assumption.contains("text only")),
        "{:?}",
        judged.assumptions
    );
}

#[test]
fn a_target_the_run_did_not_publish_fails_even_when_it_holds_the_text() {
    for (name, source, before) in WORLDS {
        // The run wrote nothing: what the target held before is no copy.
        let left = completed(source, untouched(before));
        assert_eq!(copy_outcome(left), Outcome::Failed, "{name}");
        // A target that already held the exact text, untouched by the run, proves nothing.
        let already = ReadBack::new(TARGET, source).with_written(false);
        assert_eq!(
            copy_outcome(completed(source, already)),
            Outcome::Failed,
            "{name}"
        );
    }
}

#[test]
fn a_lookalike_copy_fails() {
    let accent = char::from_u32(0x0301).expect("a combining acute accent");
    let mark = char::from_u32(0xFEFF).expect("a byte order mark");
    let lookalikes = [
        // The final newline trimmed, or one more.
        (ALPHA, "alpha".to_owned()),
        (ALPHA, "alpha\n\n".to_owned()),
        // Line endings normalized, the literal read as a template, the letter decomposed, a mark.
        (CRLF, CRLF.replace("\r\n", "\n")),
        (CRLF, CRLF.replace("${{ const.not_code }}", "")),
        (CRLF, CRLF.replace('é', &format!("e{accent}"))),
        (CRLF, format!("{mark}{CRLF}")),
        // Something written where nothing was to be.
        ("", "\n".to_owned()),
    ];
    for (source, lookalike) in lookalikes {
        assert_eq!(
            copy_outcome(completed(source, published(&lookalike))),
            Outcome::Failed,
            "{lookalike:?}"
        );
    }
}

#[test]
fn a_cut_or_non_text_observation_is_never_certified_and_an_empty_file_is_text() {
    let consumed = |coverage: Coverage| {
        Run::new("world", RunEnd::Completed, usage())
            .with_consumed(Consumed::new(SOURCE, "alp", coverage))
            .with_read_back(published(ALPHA))
    };
    // A source cut at the host's bound, or sampled as the host records one that is no text.
    assert_eq!(
        copy_outcome(consumed(Coverage::Truncated)),
        Outcome::Incomplete
    );
    assert_eq!(
        copy_outcome(consumed(Coverage::Sampled)),
        Outcome::Incomplete
    );
    // A result read back cut, even as a prefix of the source, or as no text: an empty cut text.
    let prefix = published("alp").with_truncated(true);
    assert_eq!(copy_outcome(completed(ALPHA, prefix)), Outcome::Incomplete);
    let opaque = published("").with_truncated(true);
    assert_eq!(copy_outcome(completed("", opaque)), Outcome::Incomplete);
    // An empty source the host read whole, and the empty file the run published: a copy.
    assert_eq!(copy_outcome(completed("", published(""))), Outcome::Passed);
}

#[test]
fn a_missing_or_contradictory_receipt_is_an_invalid_harness() {
    // An attempt with no receipt of what the run consumed from the source.
    let unread = Run::new("world", RunEnd::Completed, usage()).with_read_back(published(ALPHA));
    assert_eq!(copy_outcome(unread), Outcome::InvalidHarness);
    // An attempt with no observation of the target's final state.
    let unobserved = Run::new("world", RunEnd::Completed, usage()).with_consumed(Consumed::new(
        SOURCE,
        ALPHA,
        Coverage::Complete,
    ));
    assert_eq!(copy_outcome(unobserved), Outcome::InvalidHarness);
    // Two different observations of one path, `./` or not.
    let aliased = completed(ALPHA, published(ALPHA)).with_consumed(Consumed::new(
        "in/source.txt",
        CRLF,
        Coverage::Complete,
    ));
    assert_eq!(copy_outcome(aliased), Outcome::InvalidHarness);
    let republished =
        completed(ALPHA, published(ALPHA)).with_read_back(ReadBack::new("out/copied.txt", CRLF));
    assert_eq!(copy_outcome(republished), Outcome::InvalidHarness);
    // The same observation twice is no contradiction.
    let twice = completed(ALPHA, published(ALPHA))
        .with_consumed(Consumed::new("in/source.txt", ALPHA, Coverage::Complete))
        .with_read_back(ReadBack::new("out/copied.txt", ALPHA));
    assert_eq!(copy_outcome(twice), Outcome::Passed);
}

#[test]
fn a_copy_passes_only_on_a_completed_run() {
    // Nothing prepared, nothing attempted: no receipt is due, and none is missing.
    let refused = Run::new(
        "world",
        RunEnd::NotRun {
            reason: "the host prepared no world".to_owned(),
        },
        usage(),
    );
    assert_eq!(copy_outcome(refused), Outcome::NotRun);
    let invalid = Run::new(
        "world",
        RunEnd::InvalidHarness {
            reason: "the room could not be built".to_owned(),
        },
        usage(),
    );
    assert_eq!(copy_outcome(invalid), Outcome::InvalidHarness);
    // The right bytes after a failed run prove no copy: no rule stops a copy, so a failure is a
    // defect, and the host's time bound leaves the fixture not run.
    for (cause, expected) in [
        (Cause::Engine, Outcome::Failed),
        (Cause::Unclassified, Outcome::Failed),
        (Cause::TimeBound, Outcome::NotRun),
    ] {
        let run = attempt("world", failed(cause), ALPHA, published(ALPHA));
        assert_eq!(copy_outcome(run), expected);
    }
}

#[test]
fn only_the_closed_copy_sentence_proves_the_copy() {
    for intent in [
        "Copy ./in/source.txt as is, byte for byte, to ./out/copied.txt",
        "Copy ./in/source.txt to ./out/copied.txt",
        "Copy ./in/source.txt  as is to ./out/copied.txt",
        "Copy ./in/source.txt as-is to ./out/copied.txt",
        "Copy ./in/source.txt as is to ./out/copied.txt now",
        "Copy ./in/source.txt as is to ./out/copied.txt and ./out/again.txt",
        "copie ./in/source.txt tel quel dans ./out/copied.txt",
        "Read ./in/source.txt and write it as is to ./out/copied.txt",
        "Copy ./in/source.txt as is to in/source.txt",
        "Copy ./in/l’été.txt as is to ./out/copied.txt",
    ] {
        let (_, provenance) = read_request(intent);
        assert_eq!(provenance.production, None, "{intent}");
        let contract = contract_of_request(intent, &BTreeMap::new());
        assert!(
            contract.obligations.iter().all(|obligation| {
                obligation.presence != Presence::Required
                    && matches!(obligation.requirement, Requirement::Unsupported(_))
            }),
            "{intent}: {contract:?}"
        );
    }
}

#[test]
fn the_copy_production_holds_only_over_its_exact_plan() {
    let copied =
        |intent: &str, plan: &Plan| production(intent, plan).map(|produced| produced.written);
    // The grammar words in any ASCII case, and one terminal period.
    for intent in [
        INTENT,
        "COPY ./in/source.txt AS IS TO ./out/copied.txt",
        "copy ./in/source.txt as is to ./out/copied.txt.",
    ] {
        let plan = copy_plan(SOURCE, TARGET);
        assert_eq!(copied(intent, &plan), Some(Written::Copy), "{intent}");
    }
    // The plan holds nothing else, and exactly these files.
    let mut ruled = copy_plan(SOURCE, TARGET);
    ruled
        .rules
        .push(synthesize("keep the rows where status is paid", &[]).expect("a rule"));
    let mut computed = copy_plan(SOURCE, TARGET);
    computed
        .steps
        .push(PlanStep::new(Op::Compute, INTENT, "a summary", Vec::new()));
    let mut gated = copy_plan(SOURCE, TARGET);
    gated.effects[0].policy = EffectPolicy::HumanFirst;
    let mut bound = copy_plan(SOURCE, TARGET);
    bound.bindings.push(Binding::new("content", "hello"));
    let mut constrained = copy_plan(SOURCE, TARGET);
    constrained.constraints.push("keep it short".to_owned());
    let mut triggered = copy_plan(SOURCE, TARGET);
    triggered.trigger = Some("every morning".to_owned());
    for (what, other) in [
        ("a rule", ruled),
        ("another step", computed),
        ("a gate", gated),
        ("another binding", bound),
        ("a constraint", constrained),
        ("a trigger", triggered),
        ("another source", copy_plan("./in/other.txt", TARGET)),
        ("another target", copy_plan(SOURCE, "./out/other.txt")),
    ] {
        assert_eq!(copied(INTENT, &other), None, "{what}");
    }
    // One file named twice, `./` or not, is no copy.
    let itself = copy_plan(SOURCE, "in/source.txt");
    assert_eq!(
        copied("Copy ./in/source.txt as is to in/source.txt", &itself),
        None
    );
}

#[test]
fn an_unknown_suffix_proves_no_copy() {
    for (source, target) in [
        ("./in/source.bin", "./out/copied.bin"),
        ("./in/source.json", "./out/copied.json"),
        ("./in/source.txt", "./out/copied.bin"),
    ] {
        let intent = format!("Copy {source} as is to {target}");
        assert_eq!(
            production(&intent, &copy_plan(source, target)),
            None,
            "{intent}"
        );
        let contract = contract_of_request(&intent, &BTreeMap::new());
        assert!(
            contract.obligations.iter().all(|obligation| {
                obligation.presence != Presence::Required
                    && !matches!(obligation.requirement, Requirement::CopyText { .. })
            }),
            "{intent}: {contract:?}"
        );
        // The exact bytes at the target certify nothing.
        let run = Run::new("world", RunEnd::Completed, usage())
            .with_consumed(Consumed::new(source, ALPHA, Coverage::Complete))
            .with_read_back(ReadBack::new(target, ALPHA));
        let report = judge(&contract, &[run], &mut budget());
        assert_ne!(report.verdict(), Verdict::Certified, "{intent}: {report:?}");
    }
}

#[test]
fn selection_judges_copy_candidates_by_their_runs() {
    let contract = contract();
    // A candidate: the identity of its text, and what it published on each world.
    let candidate = |text: &str, publish: fn(&str) -> String| {
        let runs = WORLDS
            .iter()
            .map(|(name, source, _)| {
                attempt(name, RunEnd::Completed, source, published(&publish(source)))
            })
            .collect();
        Candidate::new(sha256_hex(text.as_bytes()), runs)
    };
    let exact = candidate("read, then write as is", str::to_owned);
    let normalizing = candidate("read, normalize the line endings, write", |text| {
        text.replace("\r\n", "\n")
    });
    let trimming = candidate("read, trim, write", |text| text.trim_end().to_owned());
    let limits = Limits::new(16, 16, 1_000_000, 60_000);
    let choose = |candidates: &[Candidate]| {
        select(&contract, candidates, limits, limits, Usage::default()).choice
    };
    assert_eq!(
        choose(&[normalizing.clone(), exact.clone()]),
        Choice::Selected(1)
    );
    assert_eq!(choose(&[exact, trimming.clone()]), Choice::Selected(0));
    assert_eq!(choose(&[normalizing, trimming]), Choice::RejectAll);
}
