// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Selection by behaviour against the request's own contract, on the observed world and a
//! discriminating one. The runs are synthetic: each states what a host's rehearsal of one
//! candidate read back on one world, as a double states it. No real run is claimed: the host is
//! still a stub, and the count's jq and convert stay refused there. No expected value is written
//! here: the judge derives it from the request over the records each run consumed. A candidate's
//! text is its identity only, and nothing reads it: the forms below are illustrative, never
//! parsed, checked or run (their writes create no parent folder, so they are no room fixtures),
//! and their hashes prove nothing about the observations.

use std::collections::BTreeMap;

use super::formats::sha256_hex;
use super::{
    Axis, Candidate, Choice, Consumed, Contract, Coverage, Format, Limits, Obligation, Outcome,
    Presence, ReadBack, Report, Requirement, Ruling, Run, RunEnd, Target, Usage, Verdict,
    contract_of_request, same_path, select, targets,
};

const INTENT: &str = "read ./data/input.csv, count the rows where status is paid, write the count to ./out/result.json";
const INPUT: &str = "./data/input.csv";
const RESULT: &str = "./out/result.json";
/// The world the host observed.
const OBSERVED: &str = "id,amount_usd,status\n1,5,paid\n2,20,late\n3,12,paid\n";
/// A world whose count differs from the observed one (3, not 2) and that tells an exact
/// equality on `paid` from a substring (4), a case-blind match (4) or a result fixed in advance.
const DISCRIMINATING: &str =
    "id,amount_usd,status\n1,5,paid\n2,7,unpaid\n3,9,Paid\n4,11,paid\n5,13,paid\n";

/// An illustrative good form for the request: convert, then count the paid rows under `count`.
const FORM_A: &str = r#"nika: count-paid
permits:
  fs: { read: ["./data/input.csv"], write: ["./out/result.json"] }
  tools: ["nika:read", "nika:convert", "nika:jq", "nika:write"]
tasks:
  source:
    invoke: { tool: "nika:read", args: { path: "./data/input.csv" } }
  rows:
    with: { text: "${{ tasks.source.output }}" }
    invoke: { tool: "nika:convert", args: { input: "${{ with.text }}", from: csv, to: json } }
  count:
    with: { rows: "${{ tasks.rows.output }}" }
    invoke: { tool: "nika:jq", args: { input: "${{ with.rows }}", expression: '{count: map(select(.status == "paid")) | length}' } }
  save:
    with: { result: "${{ tasks.count.output }}" }
    invoke: { tool: "nika:write", args: { path: "./out/result.json", content: "${{ with.result }}" } }
"#;

/// Another form: a reduce, under `n`. Its read-back below is a synthetic pretty observation
/// (the builtin writes an object compactly): the value is judged, so the layout is equivalent.
const FORM_B: &str = r#"nika: paid-total
permits:
  fs: { read: ["./data/input.csv"], write: ["./out/result.json"] }
  tools: ["nika:read", "nika:convert", "nika:jq", "nika:write"]
tasks:
  load:
    invoke: { tool: "nika:read", args: { path: "./data/input.csv" } }
  table:
    with: { csv: "${{ tasks.load.output }}" }
    invoke: { tool: "nika:convert", args: { input: "${{ with.csv }}", from: csv, to: json } }
  paid:
    with: { rows: "${{ tasks.table.output }}" }
    invoke: { tool: "nika:jq", args: { input: "${{ with.rows }}", expression: 'reduce .[] as $row (0; if $row.status == "paid" then . + 1 else . end) | {n: .}' } }
  out:
    with: { doc: "${{ tasks.paid.output }}" }
    invoke: { tool: "nika:write", args: { path: "./out/result.json", content: "${{ with.doc }}" } }
"#;

/// A third form, under `total`.
const FORM_D: &str = r#"nika: paid-rows
permits:
  fs: { read: ["./data/input.csv"], write: ["./out/result.json"] }
  tools: ["nika:read", "nika:convert", "nika:jq", "nika:write"]
tasks:
  read:
    invoke: { tool: "nika:read", args: { path: "./data/input.csv" } }
  json:
    with: { text: "${{ tasks.read.output }}" }
    invoke: { tool: "nika:convert", args: { input: "${{ with.text }}", from: csv, to: json } }
  total:
    with: { rows: "${{ tasks.json.output }}" }
    invoke: { tool: "nika:jq", args: { input: "${{ with.rows }}", expression: '[.[] | select(.status == "paid")] | {total: length}' } }
  write:
    with: { doc: "${{ tasks.total.output }}" }
    invoke: { tool: "nika:write", args: { path: "./out/result.json", content: "${{ with.doc }}" } }
"#;

/// The contract the request alone states.
fn contract() -> Contract {
    contract_of_request(INTENT, &BTreeMap::new())
}

/// What a host's rehearsal of a candidate read back on one world (synthetic).
fn run(fixture: &str, world: &str, written: ReadBack) -> Run {
    Run::new(fixture, RunEnd::Completed, Usage::new(1, 1, 64, 16, 20))
        .with_consumed(Consumed::new(INPUT, world, Coverage::Complete))
        .with_read_back(written)
}

/// A candidate: the identity of its text, and what it wrote on the observed and the
/// discriminating worlds.
fn candidate(text: &str, observed: ReadBack, discriminating: ReadBack) -> Candidate {
    Candidate::new(
        sha256_hex(text.as_bytes()),
        vec![
            run("observed", OBSERVED, observed),
            run("discriminating", DISCRIMINATING, discriminating),
        ],
    )
}

fn wrote(text: &str) -> ReadBack {
    ReadBack::new(RESULT, text)
}

fn good_a() -> Candidate {
    candidate(FORM_A, wrote(r#"{"count":2}"#), wrote(r#"{"count":3}"#))
}

fn good_b() -> Candidate {
    let pretty = |count: u32| format!("{{\n  \"n\": {count}\n}}\n");
    candidate(FORM_B, wrote(&pretty(2)), wrote(&pretty(3)))
}

/// A's text but for one test: a substring, right on the observed world only.
fn resembling() -> Candidate {
    let text = FORM_A.replace(
        r#"select(.status == "paid")"#,
        r#"select(.status | contains("paid"))"#,
    );
    candidate(&text, wrote(r#"{"count":2}"#), wrote(r#"{"count":4}"#))
}

/// A's text counting every row.
fn every_row() -> Candidate {
    let text = FORM_A.replace(r#"map(select(.status == "paid")) | length"#, "length");
    candidate(&text, wrote(r#"{"count":3}"#), wrote(r#"{"count":5}"#))
}

/// A's text writing a count fixed in advance: right on the observed world only.
fn constant() -> Candidate {
    let text = FORM_A.replace(r#"map(select(.status == "paid")) | length"#, "2");
    candidate(&text, wrote(r#"{"count":2}"#), wrote(r#"{"count":2}"#))
}

/// A third form whose discriminating read-back the host cut at its byte bound.
fn cut() -> Candidate {
    candidate(
        FORM_D,
        wrote(r#"{"total":2}"#),
        wrote(r#"{"total":"#).with_truncated(true),
    )
}

fn wide() -> Limits {
    Limits::new(16, 16, 1_000_000, 60_000)
}

fn choose(candidates: &[Candidate]) -> Ruling {
    select(&contract(), candidates, wide(), wide(), Usage::default())
}

/// The fixtures on which a report shows a failure.
fn failing(report: &Report) -> Vec<&str> {
    report
        .judged
        .iter()
        .flat_map(|judged| &judged.findings)
        .filter(|finding| finding.outcome == Outcome::Failed)
        .map(|finding| finding.fixture.as_str())
        .collect()
}

#[test]
fn two_good_workflows_of_different_forms_are_both_certified_and_either_is_selected() {
    let contract = contract();
    let read_back: Vec<String> = targets(&contract).collect();
    assert_eq!(read_back.len(), 1, "{read_back:?}");
    assert!(same_path(&read_back[0], RESULT));
    assert!(
        contract
            .sources
            .iter()
            .any(|source| same_path(source, INPUT))
    );
    assert_ne!(good_a().id, good_b().id);
    for good in [good_a(), good_b()] {
        let ruling = choose(&[good]);
        assert_eq!(ruling.choice, Choice::Selected(0));
        assert_eq!(ruling.reports[0].verdict(), Verdict::Certified);
    }
    assert_eq!(choose(&[good_a(), good_b()]).choice, Choice::Selected(0));
    let reversed = choose(&[good_b(), good_a()]);
    assert_eq!(reversed.choice, Choice::Selected(0));
    // The judging stops at the first certified candidate.
    assert_eq!(reversed.reports.len(), 1);
}

#[test]
fn a_resembling_source_with_a_wrong_result_fails_on_the_discriminating_world() {
    let ruling = choose(&[resembling()]);
    assert_eq!(ruling.choice, Choice::RejectAll);
    let report = &ruling.reports[0];
    assert_eq!(report.verdict(), Verdict::Defective);
    // The observed world alone cannot tell it from A: only the discriminating world fails.
    assert_eq!(failing(report), ["discriminating"]);
    assert_eq!(
        choose(&[resembling(), good_a()]).choice,
        Choice::Selected(1)
    );
}

#[test]
fn a_constant_result_fails_where_the_true_count_differs() {
    let ruling = choose(&[constant()]);
    assert_eq!(ruling.choice, Choice::RejectAll);
    // The constant is the observed world's count: only the world whose count differs shows it.
    assert_eq!(failing(&ruling.reports[0]), ["discriminating"]);
    assert_eq!(choose(&[constant(), good_b()]).choice, Choice::Selected(1));
}

#[test]
fn a_candidate_without_a_complete_proof_is_never_selected() {
    let ruling = choose(&[cut()]);
    assert_eq!(ruling.choice, Choice::Unproven);
    assert_eq!(ruling.reports[0].verdict(), Verdict::Incomplete);
    // No defect was shown, and that alone proves nothing.
    assert!(!ruling.reports[0].failed());
    assert_eq!(choose(&[cut(), good_b()]).choice, Choice::Selected(1));
}

#[test]
fn every_candidate_shown_defective_is_reject_all() {
    let ruling = choose(&[resembling(), every_row()]);
    assert_eq!(ruling.choice, Choice::RejectAll);
    assert_eq!(ruling.reports.len(), 2);
    assert!(
        ruling
            .reports
            .iter()
            .all(|report| report.verdict() == Verdict::Defective)
    );
    // One open candidate among defective ones is no rejection of all.
    assert_eq!(
        choose(&[resembling(), cut(), every_row()]).choice,
        Choice::Unproven
    );
}

#[test]
fn the_turn_budget_stops_the_judging_and_says_so() {
    let contract = contract();
    let two_fixtures = Limits::new(2, 16, 1_000_000, 60_000);
    let stopped = select(
        &contract,
        &[resembling(), good_a()],
        wide(),
        two_fixtures,
        Usage::default(),
    );
    assert_eq!(stopped.choice, Choice::Spent(Axis::Fixtures));
    // The good candidate was never judged.
    assert_eq!(stopped.reports.len(), 1);
    assert_eq!(stopped.turn.fixtures, 2);
    let first = select(
        &contract,
        &[good_a(), resembling()],
        wide(),
        two_fixtures,
        Usage::default(),
    );
    assert_eq!(first.choice, Choice::Selected(0));
    // A door judging one candidate per round carries the turn forward.
    let round_one = select(
        &contract,
        &[resembling()],
        wide(),
        two_fixtures,
        Usage::default(),
    );
    assert_eq!(round_one.choice, Choice::RejectAll);
    let round_two = select(&contract, &[good_a()], wide(), two_fixtures, round_one.turn);
    assert_eq!(round_two.choice, Choice::Spent(Axis::Fixtures));
    assert!(round_two.reports.is_empty());
}

#[test]
fn the_turn_counts_only_the_judged_prefix_and_is_carried() {
    let contract = contract();
    // Two runs of one fixture each, as `run` states them.
    let one_candidate = Usage::new(2, 2, 128, 32, 40);
    // Two candidates handed over at once: the first is selected, and whatever the host spent on
    // the second is no part of the ruling's turn. The slice is no ledger of runs already made.
    let both = select(
        &contract,
        &[good_a(), every_row()],
        wide(),
        wide(),
        Usage::default(),
    );
    assert_eq!(both.choice, Choice::Selected(0));
    assert_eq!(both.reports.len(), 1);
    assert_eq!(both.turn, one_candidate);
    // One candidate per call, the turn carried: what each candidate judged adds up.
    let first = select(&contract, &[every_row()], wide(), wide(), Usage::default());
    assert_eq!(first.choice, Choice::RejectAll);
    assert_eq!(first.turn, one_candidate);
    let second = select(&contract, &[good_a()], wide(), wide(), first.turn);
    assert_eq!(second.choice, Choice::Selected(0));
    assert_eq!(second.turn, one_candidate.plus(&one_candidate));
}

#[test]
fn negative_verdicts_alone_never_select() {
    assert_eq!(choose(&[]).choice, Choice::Unproven);
    let silent = Candidate::new(sha256_hex(FORM_A.as_bytes()), Vec::new());
    let ruling = choose(&[silent]);
    assert_eq!(ruling.choice, Choice::Unproven);
    assert_eq!(ruling.reports[0].verdict(), Verdict::NotRun);
    let refused = Run::new(
        "observed",
        RunEnd::NotRun {
            reason: "the host offers no room".to_owned(),
        },
        Usage::default(),
    );
    let ruling = choose(&[Candidate::new(sha256_hex(FORM_B.as_bytes()), vec![refused])]);
    assert_eq!(ruling.choice, Choice::Unproven);
    assert_eq!(ruling.reports[0].verdict(), Verdict::NotRun);
}

#[test]
fn the_targets_are_the_contract_paths_once() {
    let write = |id: &str, path: &str| {
        Obligation::new(
            id,
            Some(Target::new(path, Format::Json)),
            Presence::Required,
            Requirement::PresenceOnly,
            id,
        )
    };
    let unnamed = Obligation::new(
        "answer",
        None,
        Presence::Required,
        Requirement::PresenceOnly,
        "answer",
    );
    let contract = Contract::new(vec![
        write("first", "./out/a.json"),
        unnamed,
        write("again", "out/a.json"),
        write("second", "./out/b.json"),
    ]);
    let read_back: Vec<String> = targets(&contract).collect();
    assert_eq!(read_back, ["./out/a.json", "./out/b.json"]);
}
