// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The adapter's discriminating fixtures. Each one builds the record a host hands over, maps it
//! with `judged_run` and judges it with the behavioural judge. A record that shows what the judge
//! needs is judged by its behaviour, never by the bytes of one world; a record that cannot show
//! it is an invalid harness or a run that never ran, never a completed one.

use std::collections::BTreeMap;

use nika_compile_fidelity::behavior::{
    Budget, Cause, Contract, Coverage, Failure, Format, Limits, Obligation, Presence, Requirement,
    Run, RunEnd, Target, Usage, Verdict, contract_of_request, judge,
};

use super::{
    Attempt, Bounds, CopyReceipt, Digest, EffectCounts, FailureRecord, FinalReceipt, FinalState,
    Held, LedgerFacts, Observation, RecordedCause, Refusal, Rehearsal, RehearsalReport,
    RoomEvidence, Spent, judged_run, targets_of,
};

const INPUT: &str = "./data/input.csv";
const RESULT: &str = "./out/result.json";
const COUNTED: &str = "read ./data/input.csv, count the rows where status is paid, write the count to ./out/result.json";
/// The counted request, naming its source and its result through aliases of their files.
const ALIASED: &str = "read ./data//input.csv, count the rows where status is paid, write the count to ./out/./result.json";
const TWO_PAID: &str = "id,amount_usd,status\n1,5,paid\n2,20,late\n3,12,paid\n";
const THREE_PAID: &str = "id,amount_usd,status\n1,5,paid\n2,20,paid\n3,12,paid\n4,1,late\n";
const COUNT_TWO: &str = r#"{"count":2}"#;
const COUNT_THREE: &str = r#"{"count":3}"#;

fn copy(path: &str, text: &str) -> CopyReceipt {
    let digest = Digest::of(text.as_bytes());
    CopyReceipt::new(
        path,
        digest.clone(),
        Some(digest),
        Held::Whole(text.to_owned()),
    )
}

fn file(path: &str, text: &str) -> FinalReceipt {
    let digest = Digest::of(text.as_bytes());
    let held = Held::Whole(text.to_owned());
    FinalReceipt::new(path, FinalState::File { digest, held })
}

fn absent(path: &str) -> FinalReceipt {
    FinalReceipt::new(path, FinalState::Absent)
}

/// A clean record of these copies and final states, the run having published `written`.
fn observed(copies: Vec<CopyReceipt>, finals: Vec<FinalReceipt>, written: &[&str]) -> Observation {
    let input_bytes: u64 = copies
        .iter()
        .filter_map(|copy| copy.room.as_ref())
        .map(|room| room.bytes)
        .sum();
    let read: u64 = finals
        .iter()
        .map(|read| match &read.state {
            FinalState::File { digest, .. } => digest.bytes,
            _ => 0,
        })
        .sum();
    let mut observation = Observation::none();
    observation.copies = copies;
    observation.finals = finals;
    observation.ledger =
        LedgerFacts::clean(written.iter().map(|path| (*path).to_owned()).collect());
    observation.spent = Spent::new(input_bytes, read);
    observation.bounds = Bounds::new(10_000, 1_048_576, 65_536);
    observation
}

fn passed() -> Rehearsal {
    Rehearsal::Passed {
        outputs: Vec::new(),
    }
}

/// A run that ended by itself with `outcome`, in a room prepared, drained and removed.
fn completed(outcome: Rehearsal, observation: Observation) -> RehearsalReport {
    let attempt = Attempt::Completed { elapsed_ms: 20 };
    RehearsalReport::new(outcome, attempt, EffectCounts::none(), "candidate")
        .with_room(RoomEvidence::new(true, true))
        .with_observation(observation)
}

/// A run stopped at the time bound, in a room prepared, drained and removed.
fn stopped(observation: Observation) -> RehearsalReport {
    let outcome = Rehearsal::NotRun {
        reason: "the time bound".to_owned(),
    };
    let attempt = Attempt::Stopped { elapsed_ms: 10_000 };
    RehearsalReport::new(outcome, attempt, EffectCounts::none(), "candidate")
        .with_room(RoomEvidence::new(true, true))
        .with_observation(observation)
}

fn counted() -> Contract {
    contract_of_request(COUNTED, &BTreeMap::new())
}

fn inputs() -> [String; 1] {
    [INPUT.to_owned()]
}

fn outputs() -> [String; 1] {
    [RESULT.to_owned()]
}

fn verdict(contract: &Contract, runs: &[Run]) -> Verdict {
    let mut budget = Budget::new(
        Limits::new(16, 16, 10_000_000, 600_000),
        Limits::new(64, 64, 40_000_000, 2_400_000),
        Usage::default(),
    );
    judge(contract, runs, &mut budget).verdict()
}

/// A requested result whose presence alone the request states.
fn presence(path: &str, wanted: Presence) -> Obligation {
    let target = Some(Target::new(path, Format::Json));
    let words = format!("write {path}");
    Obligation::new(
        words.clone(),
        target,
        wanted,
        Requirement::PresenceOnly,
        words,
    )
}

#[test]
fn worlds_with_different_files_are_judged_by_their_behaviour() {
    let contract = counted();
    let targets: Vec<String> = targets_of(&contract).collect();
    assert_eq!(targets, [RESULT]);
    let run = |fixture: &str, rows: &str, count: &str| {
        let observation = observed(
            vec![copy(INPUT, rows)],
            vec![file(RESULT, count)],
            &[RESULT],
        );
        let report = completed(passed(), observation);
        judged_run(fixture, &report, &inputs(), &targets, &outputs())
    };
    let correct = [
        run("observed", TWO_PAID, COUNT_TWO),
        run("more paid", THREE_PAID, COUNT_THREE),
    ];
    assert_eq!(verdict(&contract, &correct), Verdict::Certified);
    // A candidate that writes the observed answer whatever its world holds fails the variant.
    let fixed = [
        run("observed", TWO_PAID, COUNT_TWO),
        run("more paid", THREE_PAID, COUNT_TWO),
    ];
    assert_eq!(verdict(&contract, &fixed), Verdict::Defective);
}

#[test]
fn an_undeclared_target_is_read_back_and_judged_unwritten() {
    let contract = counted();
    let targets: Vec<String> = targets_of(&contract).collect();
    // The candidate declares no output and writes none; the host reads the contract's target.
    let observation = observed(vec![copy(INPUT, TWO_PAID)], vec![absent(RESULT)], &[]);
    let run = judged_run(
        "observed",
        &completed(passed(), observation),
        &inputs(),
        &targets,
        &[],
    );
    assert_eq!(run.end, RunEnd::Completed);
    assert!(
        run.read_back
            .iter()
            .any(|read| read.path == RESULT && !read.written),
        "{run:?}"
    );
    assert_eq!(verdict(&contract, &[run]), Verdict::Defective);
    // A host that reads back the declared outputs only owes the target a final state.
    let blind = observed(vec![copy(INPUT, TWO_PAID)], Vec::new(), &[]);
    let run = judged_run(
        "observed",
        &completed(passed(), blind),
        &inputs(),
        &targets,
        &[],
    );
    assert!(matches!(run.end, RunEnd::InvalidHarness { .. }), "{run:?}");
    assert_eq!(verdict(&contract, &[run]), Verdict::InvalidHarness);
}

#[test]
fn a_copied_file_at_the_target_is_no_write() {
    let contract = counted();
    let targets: Vec<String> = targets_of(&contract).collect();
    // The observed world already holds the right answer, and the run writes nothing.
    let copies = vec![copy(INPUT, TWO_PAID), copy(RESULT, COUNT_TWO)];
    let observation = observed(copies, vec![file(RESULT, COUNT_TWO)], &[]);
    let world = [INPUT.to_owned(), RESULT.to_owned()];
    let report = completed(passed(), observation);
    let run = judged_run("observed", &report, &world, &targets, &outputs());
    let read = run.read_back.iter().find(|read| read.path == RESULT);
    assert_eq!(
        read.map(|read| (read.written, read.text.as_str())),
        Some((false, COUNT_TWO))
    );
    assert_eq!(verdict(&contract, &[run]), Verdict::Defective);
}

#[test]
fn a_failure_after_a_partial_write_keeps_what_was_written() {
    let (first, second) = ("./out/first.json", "./out/second.json");
    let contract = Contract::new(vec![
        presence(first, Presence::Required),
        presence(second, Presence::Required),
    ]);
    let targets: Vec<String> = targets_of(&contract).collect();
    let finals = vec![file(first, "{}"), absent(second)];
    let mut observation = observed(vec![copy(INPUT, TWO_PAID)], finals, &[first]);
    let record = FailureRecord::new("write_second", "NIKA-TEST-001", RecordedCause::VerbError);
    observation.failure = Some(record);
    let outcome = Rehearsal::Failed {
        code: "NIKA-TEST-001".to_owned(),
        task: "write_second".to_owned(),
        message: "refused: token=s3cr3t-value".to_owned(),
    };
    let report = completed(outcome, observation);
    let run = judged_run("observed", &report, &inputs(), &targets, &targets);
    let failure = Failure::new("write_second", "NIKA-TEST-001", Cause::Unclassified);
    assert_eq!(run.end, RunEnd::Failed(failure));
    let kept: Vec<(&str, bool)> = run
        .read_back
        .iter()
        .map(|read| (read.path.as_str(), read.written))
        .collect();
    assert_eq!(kept, [(first, true), (second, false)]);
    assert!(
        !format!("{run:?}").contains("s3cr3t"),
        "the runtime's message never reaches the judge"
    );
    assert_eq!(verdict(&contract, &[run]), Verdict::Defective);
}

#[test]
fn a_stop_keeps_its_evidence_so_an_earlier_invalidity_survives_it() {
    let contract = counted();
    let targets: Vec<String> = targets_of(&contract).collect();
    let clean = observed(vec![copy(INPUT, TWO_PAID)], vec![absent(RESULT)], &[]);
    let run = judged_run("observed", &stopped(clean), &inputs(), &targets, &[]);
    let failure = Failure::new("", "", Cause::TimeBound);
    assert_eq!(run.end, RunEnd::Failed(failure));
    assert_eq!((run.consumed.len(), run.read_back.len()), (1, 1));
    assert_eq!(verdict(&contract, &[run]), Verdict::NotRun);
    // Two different final states of one path, observed before the stop: never a mere stop.
    let finals = vec![file(RESULT, COUNT_TWO), file(RESULT, COUNT_THREE)];
    let torn = observed(vec![copy(INPUT, TWO_PAID)], finals, &[RESULT]);
    let run = judged_run("observed", &stopped(torn), &inputs(), &targets, &[]);
    assert_eq!(run.read_back.len(), 2);
    assert_eq!(verdict(&contract, &[run]), Verdict::InvalidHarness);
}

#[test]
fn a_missing_copy_receipt_is_an_invalid_harness_even_under_a_pass() {
    let contract = counted();
    let targets: Vec<String> = targets_of(&contract).collect();
    let observation = observed(Vec::new(), vec![file(RESULT, COUNT_TWO)], &[RESULT]);
    let report = completed(passed(), observation);
    let run = judged_run("observed", &report, &inputs(), &targets, &outputs());
    assert!(
        matches!(&run.end, RunEnd::InvalidHarness { reason } if reason.contains(INPUT)),
        "{run:?}"
    );
    assert!(run.consumed.is_empty() && run.read_back.is_empty());
    assert_eq!(verdict(&contract, &[run]), Verdict::InvalidHarness);
}

#[test]
fn duplicate_receipts_stay_visible_to_the_judge() {
    let contract = counted();
    let targets: Vec<String> = targets_of(&contract).collect();
    let report = |copies: Vec<CopyReceipt>| {
        let observation = observed(copies, vec![file(RESULT, COUNT_TWO)], &[RESULT]);
        completed(passed(), observation)
    };
    // Two different copies of one input, spelled two ways: both reach the judge, which finds
    // them contradictory.
    let torn = report(vec![
        copy(INPUT, TWO_PAID),
        copy("data/input.csv", THREE_PAID),
    ]);
    let run = judged_run("observed", &torn, &inputs(), &targets, &outputs());
    assert_eq!(run.consumed.len(), 2);
    assert_eq!(verdict(&contract, &[run]), Verdict::InvalidHarness);
    // The same copy twice: nothing was merged, and the judge reads one world.
    let twice = report(vec![copy(INPUT, TWO_PAID), copy(INPUT, TWO_PAID)]);
    let run = judged_run("observed", &twice, &inputs(), &targets, &outputs());
    assert_eq!(run.consumed.len(), 2);
    assert_eq!(verdict(&contract, &[run]), Verdict::Certified);
}

#[test]
fn an_unproven_room_is_an_invalid_harness_never_a_completion() {
    let contract = counted();
    let targets: Vec<String> = targets_of(&contract).collect();
    let clean = || {
        observed(
            vec![copy(INPUT, TWO_PAID)],
            vec![file(RESULT, COUNT_TWO)],
            &[RESULT],
        )
    };
    let mut panicked = clean();
    panicked.ledger.panicked = 1;
    let mut abandoned = clean();
    abandoned.ledger.drained = false;
    let mut leftover = clean();
    leftover.ledger.leftovers = 1;
    let mut late = clean();
    late.ledger.late_refused = 1;
    let late_room = RoomEvidence::new(true, true).with_late_refused(1);
    let unremoved = RoomEvidence::new(true, false);
    let cases = [
        ("a panic", completed(passed(), panicked)),
        ("an abandoned drain", completed(passed(), abandoned)),
        ("a leftover", completed(passed(), leftover)),
        (
            "a late refusal",
            completed(passed(), late).with_room(late_room),
        ),
        (
            "an unverified removal",
            completed(passed(), clean()).with_room(unremoved),
        ),
    ];
    for (case, report) in cases {
        let run = judged_run("observed", &report, &inputs(), &targets, &outputs());
        assert!(
            matches!(run.end, RunEnd::InvalidHarness { .. }),
            "{case}: {run:?}"
        );
        assert_eq!(
            verdict(&contract, &[run]),
            Verdict::InvalidHarness,
            "{case}"
        );
    }
    // The control: the same room, proven, certifies.
    let report = completed(passed(), clean());
    let run = judged_run("observed", &report, &inputs(), &targets, &outputs());
    assert_eq!(verdict(&contract, &[run]), Verdict::Certified);
}

#[test]
fn ends_come_from_typed_facts_only() {
    let contract = counted();
    let targets: Vec<String> = targets_of(&contract).collect();
    // A refusal before any attempt is no stop after one.
    let outcome = Rehearsal::NotRun {
        reason: "jq".to_owned(),
    };
    let refused = RehearsalReport::new(outcome, Attempt::NeverAttempted, EffectCounts::none(), "")
        .with_observation(Observation::refused(Refusal::DataBounds));
    let run = judged_run("observed", &refused, &inputs(), &targets, &[]);
    assert!(
        matches!(&run.end, RunEnd::NotRun { reason } if reason == Refusal::DataBounds.word()),
        "{run:?}"
    );
    assert_eq!((run.usage.attempts, run.consumed.len()), (0, 0));
    assert_eq!(verdict(&contract, &[run]), Verdict::NotRun);
    // A denied effect is a run the host cannot vouch for.
    let observation = observed(
        vec![copy(INPUT, TWO_PAID)],
        vec![file(RESULT, COUNT_TWO)],
        &[RESULT],
    );
    let mut report = completed(passed(), observation);
    report.effects.network = 1;
    let run = judged_run("observed", &report, &inputs(), &targets, &outputs());
    assert!(
        matches!(&run.end, RunEnd::NotRun { reason } if reason.contains("network 1")),
        "{run:?}"
    );
    // The recorded cause decides, never the code or the message.
    for (cause, expected) in [
        (RecordedCause::VerbError, Cause::Unclassified),
        (RecordedCause::Timeout, Cause::Unclassified),
        (RecordedCause::RetryExhausted, Cause::Unclassified),
        (RecordedCause::Engine, Cause::Engine),
    ] {
        let mut observation = observed(vec![copy(INPUT, TWO_PAID)], vec![absent(RESULT)], &[]);
        observation.failure = Some(FailureRecord::new("count", "NIKA-FILE-404", cause));
        let outcome = Rehearsal::Failed {
            code: "NIKA-FILE-404".to_owned(),
            task: "count".to_owned(),
            message: "no such file: ./data/input.csv".to_owned(),
        };
        let run = judged_run(
            "observed",
            &completed(outcome, observation),
            &inputs(),
            &targets,
            &[],
        );
        let failure = Failure::new("count", "NIKA-FILE-404", expected);
        assert_eq!(run.end, RunEnd::Failed(failure));
    }
    // A record of another task contradicts the outcome.
    let mut observation = observed(vec![copy(INPUT, TWO_PAID)], vec![absent(RESULT)], &[]);
    observation.failure = Some(FailureRecord::new(
        "other",
        "NIKA-FILE-404",
        RecordedCause::Engine,
    ));
    let outcome = Rehearsal::Failed {
        code: "NIKA-FILE-404".to_owned(),
        task: "count".to_owned(),
        message: String::new(),
    };
    let run = judged_run(
        "observed",
        &completed(outcome, observation),
        &inputs(),
        &targets,
        &[],
    );
    assert!(matches!(run.end, RunEnd::InvalidHarness { .. }), "{run:?}");
}

#[test]
fn a_preview_proves_nothing_and_a_cut_copy_or_an_unrecorded_change_is_no_observation() {
    let contract = counted();
    let targets: Vec<String> = targets_of(&contract).collect();
    // The room holds the whole input; the evidence keeps its first bytes only.
    let whole = Digest::of(TWO_PAID.as_bytes());
    let held = Held::of(TWO_PAID.as_bytes(), 24);
    let preview = CopyReceipt::new(INPUT, whole.clone(), Some(whole), held);
    let observation = observed(vec![preview], vec![file(RESULT, COUNT_TWO)], &[RESULT]);
    let run = judged_run(
        "observed",
        &completed(passed(), observation),
        &inputs(),
        &targets,
        &[],
    );
    let coverage = run.consumed.first().map(|consumed| consumed.coverage);
    assert_eq!(coverage, Some(Coverage::Truncated));
    assert_eq!(verdict(&contract, &[run]), Verdict::Incomplete);
    // A room copy cut short of its source is never the observed world.
    let header = "id,amount_usd,status\n";
    let room = Some(Digest::of(header.as_bytes()));
    let cut = CopyReceipt::new(
        INPUT,
        Digest::of(TWO_PAID.as_bytes()),
        room,
        Held::Whole(header.to_owned()),
    );
    let observation = observed(vec![cut], vec![file(RESULT, COUNT_TWO)], &[RESULT]);
    let run = judged_run(
        "observed",
        &completed(passed(), observation),
        &inputs(),
        &targets,
        &[],
    );
    assert!(
        matches!(&run.end, RunEnd::InvalidHarness { reason } if reason.contains("differs")),
        "{run:?}"
    );
    // The world held the target and the run removed it, or changed it, with no publish: the
    // ledger keeps neither.
    let world = [INPUT.to_owned(), RESULT.to_owned()];
    for (final_state, words) in [
        (absent(RESULT), "removal"),
        (file(RESULT, COUNT_THREE), "without a publish"),
    ] {
        let copies = vec![copy(INPUT, TWO_PAID), copy(RESULT, COUNT_TWO)];
        let observation = observed(copies, vec![final_state], &[]);
        let report = completed(passed(), observation);
        let run = judged_run("observed", &report, &world, &targets, &[]);
        assert!(
            matches!(&run.end, RunEnd::InvalidHarness { reason } if reason.contains(words)),
            "{words}: {run:?}"
        );
    }
}

#[test]
fn a_written_file_that_is_not_text_proves_its_write_and_never_a_value() {
    // The run published bytes that are not UTF-8: the evidence keeps no text of them.
    let bytes = [0xff_u8, 0xfe, 0x00, 0x01];
    let state = FinalState::File {
        digest: Digest::of(&bytes),
        held: Held::of(&bytes, 65_536),
    };
    let finals = vec![FinalReceipt::new(RESULT, state)];
    let observation = observed(vec![copy(INPUT, TWO_PAID)], finals, &[RESULT]);
    let report = completed(passed(), observation);
    // A presence-only obligation certifies the observed write without reading its content.
    let presence_only = Contract::new(vec![presence(RESULT, Presence::Required)]);
    let targets: Vec<String> = targets_of(&presence_only).collect();
    let run = judged_run("observed", &report, &inputs(), &targets, &outputs());
    let read = run.read_back.iter().find(|read| read.path == RESULT);
    let shown = read.map(|read| (read.written, read.truncated, read.text.is_empty()));
    assert_eq!(shown, Some((true, true, true)));
    assert_eq!(verdict(&presence_only, &[run]), Verdict::Certified);
    // A computed obligation reads its value through text: nothing certifies the value.
    let computed = counted();
    let targets: Vec<String> = targets_of(&computed).collect();
    let run = judged_run("observed", &report, &inputs(), &targets, &outputs());
    assert_eq!(verdict(&computed, &[run]), Verdict::Incomplete);
}

/// A published file whose bytes are `bytes`, its evidence cut at `bound`.
fn published(bytes: &[u8], bound: u64) -> FinalReceipt {
    let state = FinalState::File {
        digest: Digest::of(bytes),
        held: Held::of(bytes, bound),
    };
    FinalReceipt::new(RESULT, state)
}

#[test]
fn two_receipts_of_one_file_that_differ_are_an_invalid_harness_whatever_text_they_show() {
    let presence_only = Contract::new(vec![presence(RESULT, Presence::Required)]);
    let targets: Vec<String> = targets_of(&presence_only).collect();
    let judged = |finals: Vec<FinalReceipt>| {
        let observation = observed(vec![copy(INPUT, TWO_PAID)], finals, &[RESULT]);
        let report = completed(passed(), observation);
        judged_run("observed", &report, &inputs(), &targets, &outputs())
    };
    // Two different binary contents: each would show the judge the same empty, cut text.
    let run = judged(vec![
        published(&[0xff, 0x00], 65_536),
        published(&[0xfe, 0x01], 65_536),
    ]);
    assert!(
        matches!(&run.end, RunEnd::InvalidHarness { reason } if reason.contains("differ")),
        "{run:?}"
    );
    assert_eq!(
        run.read_back.len(),
        2,
        "both receipts are shown, none merged"
    );
    assert_eq!(verdict(&presence_only, &[run]), Verdict::InvalidHarness);
    // Two previews with the same first bytes over different whole contents.
    let run = judged(vec![
        published(b"same-left", 4),
        published(b"same-right", 4),
    ]);
    assert!(matches!(run.end, RunEnd::InvalidHarness { .. }), "{run:?}");
    // The control: the same receipt twice is one observation, kept twice, and certifies.
    let run = judged(vec![
        published(&[0xff, 0x00], 65_536),
        published(&[0xff, 0x00], 65_536),
    ]);
    assert_eq!(run.read_back.len(), 2);
    assert_eq!(verdict(&presence_only, &[run]), Verdict::Certified);
}

#[test]
fn every_alias_of_one_file_is_one_observation_for_the_judge() {
    let contract = counted();
    let targets: Vec<String> = targets_of(&contract).collect();
    let judged = |copies: Vec<CopyReceipt>| {
        let observation = observed(copies, vec![file(RESULT, COUNT_TWO)], &[RESULT]);
        let report = completed(passed(), observation);
        judged_run("observed", &report, &inputs(), &targets, &outputs())
    };
    // The input copied under two spellings with different bytes: a contradiction.
    for alias in ["data//input.csv", "data/./input.csv"] {
        let run = judged(vec![copy(INPUT, TWO_PAID), copy(alias, THREE_PAID)]);
        assert!(
            matches!(run.end, RunEnd::InvalidHarness { .. }),
            "{alias}: {run:?}"
        );
        assert_eq!(
            verdict(&contract, &[run]),
            Verdict::InvalidHarness,
            "{alias}"
        );
    }
    // One copy under an alias is the input itself: shown under the caller's spelling, it
    // certifies.
    let run = judged(vec![copy("data/./input.csv", TWO_PAID)]);
    let shown: Vec<&str> = run
        .consumed
        .iter()
        .map(|consumed| consumed.path.as_str())
        .collect();
    assert_eq!(shown, [INPUT]);
    assert_eq!(verdict(&contract, &[run]), Verdict::Certified);
}

/// The contract of the counted request naming aliases, and the caller's world: its sources.
fn aliased() -> (Contract, Vec<String>, Vec<String>) {
    let contract = contract_of_request(ALIASED, &BTreeMap::new());
    let targets: Vec<String> = targets_of(&contract).collect();
    let world = contract.sources.clone();
    (contract, targets, world)
}

#[test]
fn a_contract_naming_aliases_is_shown_every_receipt_under_its_own_names() {
    let (contract, targets, world) = aliased();
    assert_eq!(targets, ["./out/./result.json"]);
    assert_eq!(world, ["./data//input.csv"]);
    // The host spelled every receipt another way than the contract.
    let run = |fixture: &str, rows: &str, count: &str| {
        let copies = vec![copy("data/./input.csv", rows)];
        let observation = observed(copies, vec![file("out//result.json", count)], &[RESULT]);
        let report = completed(passed(), observation);
        judged_run(
            fixture,
            &report,
            &world,
            &targets,
            &["out/result.json".to_owned()],
        )
    };
    let first = run("observed", TWO_PAID, COUNT_TWO);
    let consumed: Vec<&str> = first
        .consumed
        .iter()
        .map(|read| read.path.as_str())
        .collect();
    assert_eq!(consumed, ["./data//input.csv"]);
    let shown: Vec<(&str, bool)> = first
        .read_back
        .iter()
        .map(|read| (read.path.as_str(), read.written))
        .collect();
    assert_eq!(shown, [("./out/./result.json", true)]);
    let correct = [first, run("more paid", THREE_PAID, COUNT_THREE)];
    assert_eq!(verdict(&contract, &correct), Verdict::Certified);
    // The behaviour is still what is judged: an answer fixed whatever the world holds fails.
    let fixed = [
        run("observed", TWO_PAID, COUNT_TWO),
        run("more paid", THREE_PAID, COUNT_TWO),
    ];
    assert_eq!(verdict(&contract, &fixed), Verdict::Defective);
}

#[test]
fn receipts_of_a_named_alias_that_differ_under_other_aliases_contradict_each_other() {
    let (contract, targets, world) = aliased();
    let judged = |copies: Vec<CopyReceipt>, finals: Vec<FinalReceipt>| {
        let observation = observed(copies, finals, &[RESULT]);
        judged_run(
            "observed",
            &completed(passed(), observation),
            &world,
            &targets,
            &[],
        )
    };
    // Two different copies of the source, under two aliases the contract does not use.
    let copies = vec![copy("data/./input.csv", TWO_PAID), copy(INPUT, THREE_PAID)];
    let run = judged(copies, vec![file("out//result.json", COUNT_TWO)]);
    assert!(
        matches!(&run.end, RunEnd::InvalidHarness { reason } if reason.contains("differ")),
        "{run:?}"
    );
    assert_eq!(
        run.consumed.len(),
        2,
        "both receipts are shown, none merged"
    );
    assert_eq!(verdict(&contract, &[run]), Verdict::InvalidHarness);
    // Two different final states of the result, under two aliases the contract does not use.
    let finals = vec![
        file("out//result.json", COUNT_TWO),
        file(RESULT, COUNT_THREE),
    ];
    let run = judged(vec![copy("data/./input.csv", TWO_PAID)], finals);
    assert!(
        matches!(&run.end, RunEnd::InvalidHarness { reason } if reason.contains("differ")),
        "{run:?}"
    );
    assert_eq!(
        run.read_back.len(),
        2,
        "both receipts are shown, none merged"
    );
    assert_eq!(verdict(&contract, &[run]), Verdict::InvalidHarness);
}

#[test]
fn every_spelling_the_contract_gives_one_file_finds_the_one_observation() {
    // Two obligations name one file two ways: each finds its final state under its own name,
    // and the host still reads the file back once.
    let alias = "out/./result.json";
    let contract = Contract::new(vec![
        presence(RESULT, Presence::Required),
        presence(alias, Presence::Required),
    ]);
    let targets: Vec<String> = targets_of(&contract).collect();
    assert_eq!(targets, [RESULT, alias]);
    // A final state no name gives (a log the run also published) keeps the room's spelling.
    let finals = vec![file("out//result.json", "{}"), file("logs//run.txt", "ok")];
    let observation = observed(
        vec![copy(INPUT, TWO_PAID)],
        finals,
        &[RESULT, "logs/run.txt"],
    );
    let report = completed(passed(), observation);
    let run = judged_run("observed", &report, &inputs(), &targets, &[]);
    let shown: Vec<&str> = run
        .read_back
        .iter()
        .map(|read| read.path.as_str())
        .collect();
    assert_eq!(shown, [RESULT, alias, "logs/run.txt"]);
    assert_eq!(verdict(&contract, &[run]), Verdict::Certified);
}
