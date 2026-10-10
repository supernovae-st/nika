// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A disagreement over a run of these exact bytes (R6). Only a run that proves whole outputs
//! (at least one output, every output written by the run itself, every text read whole) is
//! shown to the judge, and only in the questions over it: each part the bytes left open, and
//! each restriction judged broken, is asked again over what the run did (carried settles an open
//! part but only notes a broken restriction, since a run of some inputs never removes a defect
//! located in the bytes; missing confirms a broken restriction or asks an open part's task there;
//! not exercised by these inputs decides nothing); then, when nothing is open, the whole request,
//! offered every part: consistent outputs carry it, a part or a task named there is a defect with
//! its reason, inputs that never exercise some part and anything else decide nothing and the
//! request stays contested. Every question over the run keeps the run's receipts on its record,
//! never its texts.

use super::declined::{assert_held, unresolved};
use super::*;

/// Why a run read in part, or one that did not write every output it was read for, decides
/// nothing.
const PARTIAL: &str = "the trial run wrote nothing it was read for, or was read only in part: it proves no whole output";
/// Why a run that left a part open decides nothing.
const OPEN_AFTER_RUN: &str =
    "the trial run did not decide every part: a part its inputs never exercise stays open";
/// Why a question over the run left without a choice decides nothing.
const NO_CHOICE: &str = "the judge made no choice over the trial run";
/// Why a part named in the run that no task fails decides nothing.
const UNPOINTED: &str = "the judge named a part in the trial run but no task that fails it";
/// Why a run whose inputs never exercise some part of the request decides nothing.
const UNEXERCISED: &str =
    "the trial run's inputs never exercise some part of the request: it proves no whole output";
/// The texts [`observed`] holds.
const READ: &str = r#"[{"id":1,"status":"open"},{"id":2,"status":"cancelled"}]"#;
const WROTE: &str = r#"[{"id":1,"status":"open"}]"#;
/// The options of an open part a later one follows, asked again over a run: a later part may
/// replace it there as over the bytes alone.
const OVER_A_PART: [&str; 5] = ["carried", "missing", "unexercised", "superseded", "none"];

/// The options of the question over a run of [`CANDIDATE`]: every part of [`ORDERS`], whatever
/// the bytes made of it, then each of `open`, the tasks the engine's facts leave open.
fn over_the_run(open: &[&str]) -> Value {
    let mut options = vec!["consistent", "unexercised", "part-0", "part-1", "part-2"];
    let tasks: Vec<String> = open.iter().map(|task| format!("task-{task}")).collect();
    options.extend(tasks.iter().map(String::as_str));
    options.push("none");
    json!(options)
}

/// What the record of a question over [`observed`] keeps of it: the digests and sizes of its
/// texts, never the texts.
fn receipts(observation: &Value) -> Value {
    json!({
        "candidate_sha256": sha256(CANDIDATE),
        "sha256": sha256(&observation.to_string()),
        "texts": [
            {"role": "input", "path": "./data/orders.json", "bytes": READ.len(),
                "sha256": sha256(READ), "read_whole": true, "written": null},
            {"role": "output", "path": "./out/open.json", "bytes": WROTE.len(),
                "sha256": sha256(WROTE), "read_whole": true, "written": true},
        ],
    })
}

/// Every record carries the run's receipts exactly when its question was asked over the run.
fn witnessed(verdict: &Verdict, observation: &Value) {
    for asked in &verdict.records {
        let over_the_run = asked["question"]
            .as_str()
            .is_some_and(|id| id.starts_with("verify-observed"));
        let expected = over_the_run.then(|| receipts(observation));
        assert_eq!(asked.get("observation").cloned(), expected, "{asked}");
        let kept = asked["observation"].to_string();
        assert!(
            !kept.contains("cancelled") && !kept.contains("status"),
            "{kept}"
        );
    }
}

/// The run as a question over it shows it: in its state, beside the clause it asks about.
fn shown_the_run(sent: &Sent, observation: &Value, clause: Option<&str>) {
    assert_eq!(sent.state["observation"], *observation);
    let asked = clause.map_or(Value::Null, |text| json!({"text": text}));
    assert_eq!(
        sent.state.get("clause").cloned().unwrap_or(Value::Null),
        asked
    );
    if matches!(sent.kind, ObservedPart | Observed) {
        assert!(sent.told.contains(RUN), "{}", sent.told);
    }
}

/// Whether a run proves whole outputs: at least one output, each written by the run itself, and
/// every input and output read whole. A skipped write beside a written one proves nothing of the
/// part that asked it.
#[test]
fn only_a_run_writing_every_output_it_read_whole_proves_whole_outputs() {
    let whole = observed(true, true);
    let mut skipped = observed(true, true);
    let absent = json!({"path": "./out/other.json", "text": "", "written": false,
        "read_whole": true});
    skipped["outputs"].as_array_mut().unwrap().push(absent);
    let mut unwritten = observed(true, true);
    unwritten["outputs"][0]["written"] = json!(false);
    let mut unstated = observed(true, true);
    unstated["outputs"][0]
        .as_object_mut()
        .unwrap()
        .remove("written");
    let mut silent = observed(true, true);
    silent["outputs"] = json!([]);
    let cases = [
        (whole, true),
        (skipped, false),
        (unwritten, false),
        (unstated, false),
        (silent, false),
        (observed(false, true), false),
        (observed(true, false), false),
    ];
    for (observation, proves) in cases {
        assert_eq!(
            super::super::trial_whole(&observation),
            proves,
            "{observation}"
        );
    }
}

/// A part a later correction of the request supersedes asks nothing of the candidate: it is
/// settled, never a defect or an unknown. The question over the run still offers it, every part
/// being offered there (one misread as superseded may show asked in the run). With the rest
/// carried and nothing extra, the run of these bytes the judge reads whole and finds consistent
/// carries the corrected request.
#[tokio::test]
async fn a_part_a_later_correction_supersedes_asks_nothing_of_the_candidate() {
    let split = [
        "Write the report to ./out/a.md",
        "actually write it to ./out/b.md instead",
    ];
    assert_eq!(parts(CORRECTED), split);
    let judge = Scripted::new([
        (Request, Choose("unfaithful")),
        (Part, Choose("superseded")),
        (Part, Choose("carried")),
        (Extra, Choose("only_requested")),
        (Observed, Choose("consistent")),
    ]);
    let observation = observed(true, true);
    let Judged {
        verdict, binding, ..
    } = provided(CORRECTED, &judge, Some(&observation)).await;
    let asked = [
        "verify-request",
        "verify-part-0",
        "verify-part-1",
        "verify-extra",
        "verify-observed",
    ];
    assert_eq!(ids(&verdict), asked);
    assert_eq!(record(&verdict, "verify-part-0")["choice"], "superseded");
    let run = record(&verdict, "verify-observed");
    let every = json!([
        "consistent",
        "unexercised",
        "part-0",
        "part-1",
        "task-load",
        "task-save",
        "none"
    ]);
    assert_eq!(run["options"], every);
    let judgment = carried(CORRECTED, "verify-observed", MODEL, &binding);
    assert_eq!(verdict.judgments, [judgment]);
    assert_eq!(lists(&verdict), found(&[], &[], &[], &["unfaithful"], &[]));
    assert!(verdict.settled() && !verdict.doubted());
    assert_eq!(verdict.settled_by, Some("verify-observed"));
    assert_eq!(counts(&verdict), (5, 5, 5));
    assert_eq!(judge.left(), 0);
}

/// A disagreement is decided by a whole run of these exact bytes: the judge is shown what the
/// run read and wrote, in the state of one more question and in no earlier one, and asked again
/// over it. Consistent outputs carry the request under the question that settled it. The record
/// keeps the digest of what the judge read and of each text, never the texts.
#[tokio::test]
async fn a_consistent_run_read_whole_carries_the_request() {
    let observation = observed(true, true);
    let last = Some((Observed, Choose("consistent")));
    let judge = Scripted::new(undisputed("unfaithful", 3, last));
    let Judged {
        verdict, binding, ..
    } = provided(ORDERS, &judge, Some(&observation)).await;
    // Every part carried, the engine's facts settle every task: no extra question.
    let asked = [
        "verify-request",
        "verify-part-0",
        "verify-part-1",
        "verify-part-2",
        "verify-observed",
    ];
    assert_eq!(ids(&verdict), asked);
    let judgment = carried(ORDERS, "verify-observed", MODEL, &binding);
    assert_eq!(verdict.judgments, [judgment]);
    assert_eq!(lists(&verdict), found(&[], &[], &[], &["unfaithful"], &[]));
    assert!(verdict.settled() && !verdict.doubted());
    assert_eq!(counts(&verdict), (5, 5, 5));
    let run = record(&verdict, "verify-observed");
    assert_eq!(run["role"], "judge_observed");
    assert_eq!(run["options"], over_the_run(&[]));
    witnessed(&verdict, &observation);
    let sent = judge.sent.lock().unwrap();
    let (shown, before) = sent.split_last().unwrap();
    shown_the_run(shown, &observation, None);
    assert_eq!(shown.state["candidate_nika"], CANDIDATE);
    assert!(shown.told.contains(OBSERVED), "{}", shown.told);
    let unshown = before
        .iter()
        .all(|sent| sent.state.get("observation").is_none());
    assert!(unshown);
    drop(sent);
    assert_eq!(judge.left(), 0);
}

/// The run decides the parts the bytes left open, then the whole request (R6): a part left
/// without a choice and a part judged missing that no task fails are each asked again over the
/// run, carried there, and settled; nothing open, the whole request is asked over the run and
/// consistent outputs carry it.
#[tokio::test]
async fn a_consistent_run_decides_the_parts_the_localization_left_open() {
    let judge = Scripted::new([
        (Request, Choose("unfaithful")),
        (Part, Choose("none")),
        (Part, Choose("missing")),
        (Point, Choose("no_task")),
        (Part, Choose("carried")),
        (ObservedPart, Choose("carried")),
        (ObservedPart, Choose("carried")),
        (Observed, Choose("consistent")),
    ]);
    let observation = observed(true, true);
    let Judged {
        verdict, binding, ..
    } = provided(ORDERS, &judge, Some(&observation)).await;
    let asked = [
        "verify-request",
        "verify-part-0",
        "verify-part-1",
        "verify-point-1",
        "verify-part-2",
        "verify-observed-part-0",
        "verify-observed-part-1",
        "verify-observed",
    ];
    assert_eq!(ids(&verdict), asked);
    for (k, restricts) in [(0, false), (1, true)] {
        let over = record(&verdict, &format!("verify-observed-part-{k}"));
        assert_eq!(over["role"], "judge_observed_part");
        assert_eq!(over["options"], json!(OVER_A_PART));
        let clause = json!({"text": ORDER_PARTS[k], "restricts": restricts});
        assert_eq!(over["clause"], clause);
    }
    let run = record(&verdict, "verify-observed");
    assert_eq!(run["options"], over_the_run(&[]));
    witnessed(&verdict, &observation);
    let judgment = carried(ORDERS, "verify-observed", MODEL, &binding);
    assert_eq!(verdict.judgments, [judgment]);
    assert_eq!(verdict.settled_by, Some("verify-observed"));
    assert_eq!(lists(&verdict), found(&[], &[], &[], &["unfaithful"], &[]));
    assert!(verdict.settled() && !verdict.doubted());
    assert_eq!(counts(&verdict), (8, 8, 7));
    let sent = judge.sent.lock().unwrap();
    shown_the_run(&sent[5], &observation, Some(ORDER_PARTS[0]));
    shown_the_run(&sent[6], &observation, Some(ORDER_PARTS[1]));
    assert!(sent[5].told.contains(OBSERVED_PART) && !sent[5].told.contains(RESTRICTING));
    assert!(sent[6].told.contains(OBSERVED_PART) && sent[6].told.contains(RESTRICTING));
    assert!(sent[5].told.contains(PART) && sent[6].told.contains(PART));
    drop(sent);
    assert_eq!(judge.left(), 0);
}

/// A part these inputs never exercise (answered `unexercised` over the run, or left without a
/// choice there) stays as the bytes left it: open. A partial proof stays partial: the whole
/// request is never asked over the run, and the rejected request stays contested, saying the
/// run did not decide every part.
#[tokio::test]
async fn a_part_the_run_never_exercises_stays_open_and_the_request_contested() {
    let cases = [
        (
            Choose("unexercised"),
            found(
                &[],
                &[],
                &[ORDER_PARTS[1], ORDERS],
                &["unfaithful"],
                &[OPEN_AFTER_RUN],
            ),
            (6, 6, 6),
        ),
        (
            Choose("none"),
            found(
                &[],
                &[],
                &[ORDER_PARTS[1], ORDERS],
                &["unfaithful"],
                &[OPEN_AFTER_RUN],
            ),
            (6, 6, 5),
        ),
    ];
    for (over_the_run, expected, calls) in cases {
        let judge = Scripted::new([
            (Request, Choose("unfaithful")),
            (Part, Choose("carried")),
            (Part, Choose("missing")),
            (Point, Choose("no_task")),
            (Part, Choose("carried")),
            (ObservedPart, over_the_run),
        ]);
        let observation = observed(true, true);
        let Judged { verdict, .. } = provided(ORDERS, &judge, Some(&observation)).await;
        let asked = [
            "verify-request",
            "verify-part-0",
            "verify-part-1",
            "verify-point-1",
            "verify-part-2",
            "verify-observed-part-1",
        ];
        assert_eq!(ids(&verdict), asked, "{over_the_run:?}");
        assert_eq!(lists(&verdict), expected, "{over_the_run:?}");
        assert_eq!(counts(&verdict), calls, "{over_the_run:?}");
        assert_eq!(verdict.judgments, NO_JUDGMENT);
        witnessed(&verdict, &observation);
        assert_eq!(judge.left(), 0);
    }
    // A part left without a choice by the bytes, unexercised by the run, stays unknown.
    let judge = Scripted::new([
        (Request, Choose("unfaithful")),
        (Part, Choose("none")),
        (Part, Choose("carried")),
        (Part, Choose("carried")),
        (ObservedPart, Choose("unexercised")),
    ]);
    let Judged { verdict, .. } = provided(ORDERS, &judge, Some(&observed(true, true))).await;
    let open = found(
        &[],
        &[ORDER_PARTS[0]],
        &[ORDERS],
        &["unfaithful"],
        &[OPEN_AFTER_RUN],
    );
    assert_eq!(lists(&verdict), open);
    assert_eq!(judge.left(), 0);
}

/// A restriction judged broken is asked again over the run (R6): a run of some inputs never
/// removes a defect located in the bytes, so the defect stands whatever the run shows, for a
/// repair. Carried there, its note says the run shows it done for its inputs; missing there, the
/// run confirms it; not exercised, it stands as the bytes left it. With a defect located, neither
/// the extra question nor the whole request over the run is asked. A plain part's defect is never
/// asked over the run.
#[tokio::test]
async fn a_broken_restriction_is_judged_again_over_the_run() {
    let localized = [
        (Request, Choose("unfaithful")),
        (Part, Choose("carried")),
        (Part, Choose("missing")),
        (Point, Choose("task-keep")),
        (Part, Choose("carried")),
    ];
    let asked = [
        "verify-request",
        "verify-part-0",
        "verify-part-1",
        "verify-point-1",
        "verify-part-2",
        "verify-observed-part-1",
    ];
    let observation = observed(true, true);
    let keep = points("keep");
    let shown = format!(
        "{keep}; the trial run shows it done for its inputs, which never removes a defect located in the bytes"
    );
    let confirmed = format!("{keep}; the trial run confirms it");
    for (over_the_run, note) in [
        ("carried", shown.as_str()),
        ("missing", confirmed.as_str()),
        ("unexercised", keep.as_str()),
    ] {
        let mut script = localized.to_vec();
        script.push((ObservedPart, Choose(over_the_run)));
        let judge = Scripted::new(script);
        let Judged { verdict, .. } = provided(ORDERS, &judge, Some(&observation)).await;
        assert_eq!(ids(&verdict), asked, "{over_the_run}");
        let located = found(&[(ORDER_PARTS[1], note)], &[], &[], &["unfaithful"], &[]);
        assert_eq!(lists(&verdict), located, "{over_the_run}");
        assert_eq!(counts(&verdict), (6, 6, 6), "{over_the_run}");
        assert_eq!(verdict.judgments, NO_JUDGMENT);
        assert!(verdict.rejected() && !verdict.settled() && !verdict.stopped);
        witnessed(&verdict, &observation);
        assert_eq!(judge.left(), 0);
    }
}

/// The extra question left without a decision keeps nothing open before a whole run (R6): the
/// question over the run names every task beside `consistent`, so it decides what the extra
/// question did not, and consistent outputs carry the request. Without a run the extra question
/// stays unknown in its own words.
#[tokio::test]
async fn an_undecided_extra_question_leaves_the_question_over_the_run() {
    let mut script = vec![(Request, Choose("unfaithful"))];
    script.extend(repeat_n((Part, Choose("carried")), 3));
    script.extend([(Extra, Choose("none")), (Observed, Choose("consistent"))]);
    let judge = Scripted::new(script);
    let observation = observed(true, true);
    let policy = AuthoringPolicy::new(MODEL, 256, Duration::from_secs(2));
    let provider = Judge::Provider(&policy, &judge);
    let request = CompileRequest::create(ORDERS);
    let run = Some(&observation);
    let Judged { verdict, .. } = judged(ORDERS, &request, &elsewhere(), &provider, run).await;
    let asked = [
        "verify-request",
        "verify-part-0",
        "verify-part-1",
        "verify-part-2",
        "verify-extra",
        "verify-observed",
    ];
    assert_eq!(ids(&verdict), asked);
    assert_eq!(verdict.settled_by, Some("verify-observed"));
    assert!(verdict.settled(), "{:?}", lists(&verdict));
    assert_eq!(judge.left(), 0);
}

/// A part the bytes left open that the run shows missing asks at once which task fails it over
/// the run: the task question's state holds the run beside the part alone, its record the run's
/// receipts as every question over the run does; the task it names makes the part a defect found
/// in the trial run.
#[tokio::test]
async fn an_open_part_the_run_shows_missing_asks_its_task_over_the_run() {
    let judge = Scripted::new([
        (Request, Choose("unfaithful")),
        (Part, Choose("none")),
        (Part, Choose("carried")),
        (Part, Choose("carried")),
        (ObservedPart, Choose("missing")),
        (Point, Choose("task-load")),
    ]);
    let observation = observed(true, true);
    let Judged { verdict, .. } = provided(ORDERS, &judge, Some(&observation)).await;
    let asked = [
        "verify-request",
        "verify-part-0",
        "verify-part-1",
        "verify-part-2",
        "verify-observed-part-0",
        "verify-observed-part-0-point",
    ];
    assert_eq!(ids(&verdict), asked);
    let point = record(&verdict, "verify-observed-part-0-point");
    assert_eq!(point["role"], "judge_point");
    assert_eq!(point["options"], json!(POINTER));
    let clause = json!({"text": ORDER_PARTS[0], "restricts": false});
    assert_eq!(point["clause"], clause);
    witnessed(&verdict, &observation);
    let note = "in the trial run, the judge points to the task load";
    let located = found(&[(ORDER_PARTS[0], note)], &[], &[], &["unfaithful"], &[]);
    assert_eq!(lists(&verdict), located);
    assert_eq!(counts(&verdict), (6, 6, 5));
    let sent = judge.sent.lock().unwrap();
    let (shown, _) = sent.split_last().unwrap();
    assert_eq!(shown.kind, Point);
    shown_the_run(shown, &observation, Some(ORDER_PARTS[0]));
    assert!(shown.told.contains(POINT), "{}", shown.told);
    drop(sent);
    assert_eq!(judge.left(), 0);
}

/// A consistent answer over a run read only in part (an input or an output cut at the preview
/// bound), a run that read back nothing, a run whose only output it never wrote, and a run that
/// skipped one of two outputs it was read for decide nothing: none proves a whole output. No
/// question is asked over such a run, not even of a part left open, and the rejected request
/// stays contested, the reason recorded, with no judgment.
#[tokio::test]
async fn a_run_read_in_part_or_not_writing_every_output_is_never_asked_over() {
    let mut silent = observed(true, true);
    silent["outputs"] = json!([]);
    let mut unwritten = observed(true, true);
    unwritten["outputs"][0]["written"] = json!(false);
    let mut skipped = observed(true, true);
    let absent = json!({"path": "./out/other.json", "text": "", "written": false,
        "read_whole": true});
    skipped["outputs"].as_array_mut().unwrap().push(absent);
    for observation in [
        observed(false, true),
        observed(true, false),
        silent,
        unwritten,
        skipped,
    ] {
        let located = Some((Locate, Choose("unlocated")));
        let judge = Scripted::new(undisputed("unfaithful", 3, located));
        let Judged { verdict, .. } = provided(ORDERS, &judge, Some(&observation)).await;
        assert_eq!(ids(&verdict).last(), Some(&"verify-doubt"));
        let disputed = found(&[], &[], &[ORDERS], &["unfaithful"], &[PARTIAL]);
        assert_eq!(lists(&verdict), disputed, "{observation}");
        assert_eq!(verdict.judgments, NO_JUDGMENT);
        assert_eq!(counts(&verdict), (5, 5, 5));
        assert!(verdict.doubted());
        assert_eq!(judge.left(), 0);
    }
    let judge = Scripted::new([
        (Request, Choose("unfaithful")),
        (Part, Choose("none")),
        (Part, Choose("carried")),
        (Part, Choose("carried")),
    ]);
    let Judged { verdict, .. } = provided(ORDERS, &judge, Some(&observed(true, false))).await;
    let open = found(
        &[],
        &[ORDER_PARTS[0]],
        &[ORDERS],
        &["unfaithful"],
        &[PARTIAL],
    );
    assert_eq!(lists(&verdict), open);
    assert_eq!(judge.left(), 0);
}

/// A part the judge names over the run asks at once which task fails it there, the run in that
/// question's state beside the part alone and its receipts on the record. A task or an operation
/// no task performs makes the part a defect whose note says it was found in the trial run; no
/// task failing it, or no choice, decides nothing and the request stays contested; a task
/// question that gets no answer stops: the request is unknown, never contested.
#[tokio::test]
async fn a_part_named_in_the_run_asks_its_task_over_the_run() {
    let keep = "in the trial run, the judge points to the task keep";
    let omitted = "in the trial run, the judge finds no task performing it";
    let part = ORDER_PARTS[1];
    let cases = [
        (
            Choose("task-keep"),
            found(&[(part, keep)], &[], &[], &["unfaithful"], &[]),
            (6, 6, 6),
        ),
        (
            Choose("omitted"),
            found(&[(part, omitted)], &[], &[], &["unfaithful"], &[]),
            (6, 6, 6),
        ),
        (
            Choose("no_task"),
            found(&[], &[], &[ORDERS], &["unfaithful"], &[UNPOINTED]),
            (6, 6, 6),
        ),
        (
            Choose("none"),
            found(&[], &[], &[ORDERS], &["unfaithful"], &[NO_CHOICE]),
            (6, 6, 5),
        ),
        (
            Fail,
            found(&[], &[ORDERS], &[], &["unfaithful"], &[]),
            (6, 5, 5),
        ),
    ];
    for (pointed, expected, calls) in cases {
        let observation = observed(true, true);
        let judge = Scripted::new(undisputed(
            "unfaithful",
            3,
            Some((Observed, Choose("part-1"))),
        ));
        judge.script.lock().unwrap().push_back((Point, pointed));
        let Judged { verdict, .. } = provided(ORDERS, &judge, Some(&observation)).await;
        let asked = &ids(&verdict)[4..];
        assert_eq!(asked, ["verify-observed", "verify-observed-point-1"]);
        let point = record(&verdict, "verify-observed-point-1");
        assert_eq!(point["role"], "judge_point");
        assert_eq!(point["options"], json!(POINTER));
        assert_eq!(point["clause"], json!({"text": part, "restricts": true}));
        witnessed(&verdict, &observation);
        assert_eq!(lists(&verdict), expected, "{pointed:?}");
        assert_eq!(counts(&verdict), calls, "{pointed:?}");
        assert_eq!(verdict.stopped, matches!(pointed, Fail), "{pointed:?}");
        assert_eq!(verdict.judgments, NO_JUDGMENT);
        let sent = judge.sent.lock().unwrap();
        let (shown, _) = sent.split_last().unwrap();
        assert_eq!(shown.kind, Point);
        shown_the_run(shown, &observation, Some(part));
        assert!(shown.told.contains(POINT) && shown.told.contains(RESTRICTING));
        assert!(shown.told.contains(CREATED), "{}", shown.told);
        drop(sent);
        assert_eq!(judge.left(), 0);
    }
}

/// Over the run, a task the judge names doing something the request does not ask is a defect
/// whose note says so (here a write the request never names, the one task the engine's facts
/// leave open, so the extra question asks about it first). A task the facts settle (the read of
/// the source, the write of the output a carried part states) is no option there, so naming it
/// is no choice; a question over the run left without a choice (NONE, an answer that is no JSON
/// choice) decides nothing and the request stays contested; one that gets no answer stops, the
/// request unknown, never contested.
#[tokio::test]
async fn a_task_named_in_the_run_is_a_defect_and_no_choice_decides_nothing() {
    let extra = "the judge points to the task save, which in the trial run does something the request does not ask";
    let undecided = found(&[], &[], &[ORDERS], &["unfaithful"], &[NO_CHOICE]);
    let cases = [
        (
            Choose("task-save"),
            found(&[(EXTRA_DEFECT, extra)], &[], &[], &["unfaithful"], &[]),
            (6, 6, 6),
        ),
        (Choose("task-load"), undecided.clone(), (5, 5, 4)),
        (Choose("task-save"), undecided.clone(), (5, 5, 4)),
        (Choose("none"), undecided.clone(), (5, 5, 4)),
        (
            Fail,
            found(&[], &[ORDERS], &[], &["unfaithful"], &[]),
            (5, 4, 4),
        ),
        (Prose, undecided, (5, 5, 4)),
    ];
    let elsewhere = elsewhere();
    for (k, (reply, expected, calls)) in cases.into_iter().enumerate() {
        let mut script = undisputed("unfaithful", 3, None);
        if k == 0 {
            script.push((Extra, Choose("only_requested")));
        }
        script.push((Observed, reply));
        let judge = Scripted::new(script);
        let observation = observed(true, true);
        let candidate = if k == 0 {
            elsewhere.as_str()
        } else {
            CANDIDATE
        };
        let policy = AuthoringPolicy::new(MODEL, 256, Duration::from_secs(2));
        let provider = Judge::Provider(&policy, &judge);
        let request = CompileRequest::create(ORDERS);
        let run = Some(&observation);
        let Judged { verdict, .. } = judged(ORDERS, &request, candidate, &provider, run).await;
        assert_eq!(ids(&verdict).last(), Some(&"verify-observed"), "{reply:?}");
        witnessed(&verdict, &observation);
        assert_eq!(lists(&verdict), expected, "{reply:?}");
        assert_eq!(counts(&verdict), calls, "{reply:?}");
        assert_eq!(verdict.stopped, matches!(reply, Fail), "{reply:?}");
        assert_eq!(verdict.judgments, NO_JUDGMENT);
        assert_eq!(judge.left(), 0);
    }
}

/// The whole request asked over the run offers every part, the ones the bytes answered
/// superseded or asking no operation included (the run may show them asked), then `unexercised`
/// beside `consistent`, then the tasks the engine's facts leave open (here the write of a part
/// answered as asking nothing): inputs that never exercise some part of the request prove no
/// whole output, so that answer decides nothing, nor does naming a task the facts settle, which
/// is no option. Either way the rejected request stays unresolved, contested with why, no
/// judgment is made and the candidate is held, never READY.
#[tokio::test]
async fn the_run_question_offers_every_part_and_an_unexercised_request_is_held() {
    let localized = [
        (Request, Choose("unfaithful")),
        (Part, Choose("superseded")),
        (Part, Choose("carried")),
        (Part, Choose("no_operation")),
        (Extra, Choose("only_requested")),
    ];
    let asked = [
        "verify-request",
        "verify-part-0",
        "verify-part-1",
        "verify-part-2",
        "verify-extra",
        "verify-observed",
    ];
    let observation = observed(true, true);
    for (answer, why, consumed) in [("unexercised", UNEXERCISED, 6), ("task-load", NO_CHOICE, 5)] {
        let mut script = localized.to_vec();
        script.push((Observed, Choose(answer)));
        let judge = Scripted::new(script);
        let Judged { verdict, .. } = provided(ORDERS, &judge, Some(&observation)).await;
        assert_eq!(ids(&verdict), asked, "{answer}");
        let run = record(&verdict, "verify-observed");
        assert_eq!(run["role"], "judge_observed");
        assert_eq!(run["options"], over_the_run(&["save"]), "{answer}");
        let held = found(&[], &[], &[ORDERS], &["unfaithful"], &[why]);
        assert_eq!(lists(&verdict), held, "{answer}");
        assert_eq!(verdict.judgments, NO_JUDGMENT);
        assert_eq!(verdict.settled_by, None);
        assert_eq!(counts(&verdict), (6, 6, consumed), "{answer}");
        assert_eq!(verdict.declined, Declined::Rejected);
        assert!(verdict.doubted() && !verdict.stopped, "{answer}");
        assert!(verdict.unresolved(), "{answer}");
        witnessed(&verdict, &observation);
        assert_held(&verdict, &unresolved(3));
        let sent = judge.sent.lock().unwrap();
        let (shown, _) = sent.split_last().unwrap();
        assert_eq!(shown.kind, Observed);
        shown_the_run(shown, &observation, None);
        assert!(shown.told.contains(OBSERVED), "{}", shown.told);
        drop(sent);
        assert_eq!(judge.left(), 0);
    }
}

/// « faithful » is read on the program, never on what it produced (A1): over a whole run of these
/// bytes each part is judged again over what the run read and wrote, every part asked, the
/// restriction told so, and the run's receipts on each record. Every part carried there, or
/// never exercised by these inputs, leaves the verdict standing: the whole request is carried by
/// `verify-request`.
#[tokio::test]
async fn a_faithful_verdict_stands_once_each_part_holds_over_the_run() {
    for middle in ["carried", "unexercised"] {
        let judge = Scripted::new([
            (Request, Choose("faithful")),
            (ObservedPart, Choose("carried")),
            (ObservedPart, Choose(middle)),
            (ObservedPart, Choose("carried")),
        ]);
        let observation = observed(true, true);
        let Judged {
            verdict, binding, ..
        } = provided(ORDERS, &judge, Some(&observation)).await;
        let asked = [
            "verify-request",
            "verify-observed-part-0",
            "verify-observed-part-1",
            "verify-observed-part-2",
        ];
        assert_eq!(ids(&verdict), asked);
        let judgment = carried(ORDERS, "verify-request", MODEL, &binding);
        assert_eq!(verdict.judgments, [judgment]);
        assert_eq!(verdict.settled_by, Some("verify-request"));
        assert_eq!(lists(&verdict), found(&[], &[], &[], &[], &[]));
        assert_eq!(verdict.declined, Declined::No);
        assert!(verdict.settled() && !verdict.doubted());
        assert_eq!(counts(&verdict), (4, 4, 4));
        witnessed(&verdict, &observation);
        let sent = judge.sent.lock().unwrap();
        assert!(sent[0].state.get("observation").is_none());
        for (k, part) in ORDER_PARTS.iter().enumerate() {
            shown_the_run(&sent[k + 1], &observation, Some(part));
            assert_eq!(sent[k + 1].told.contains(RESTRICTING), k == 1);
        }
        drop(sent);
        assert_eq!(judge.left(), 0);
    }
}

/// An output that contradicts a part the judge called carried on the bytes is the defect the
/// program hides (A1): over the run the part is `missing`, its pointer over the run names the
/// task that fails it, and the faithful verdict yields to a located defect a repair starts from.
/// No Carried judgment is made; the bytes are rejected, never READY.
#[tokio::test]
async fn an_output_contradicting_a_part_turns_a_faithful_verdict_into_a_defect() {
    let judge = Scripted::new([
        (Request, Choose("faithful")),
        (ObservedPart, Choose("carried")),
        (ObservedPart, Choose("missing")),
        (Point, Choose("task-keep")),
        (ObservedPart, Choose("carried")),
    ]);
    let observation = observed(true, true);
    let Judged { verdict, .. } = provided(ORDERS, &judge, Some(&observation)).await;
    let asked = [
        "verify-request",
        "verify-observed-part-0",
        "verify-observed-part-1",
        "verify-observed-part-1-point",
        "verify-observed-part-2",
    ];
    assert_eq!(ids(&verdict), asked);
    assert_eq!(verdict.judgments, NO_JUDGMENT);
    let note = format!("in the trial run, {}", points("keep"));
    let located = found(&[(ORDER_PARTS[1], note.as_str())], &[], &[], &[], &[]);
    assert_eq!(lists(&verdict), located);
    assert_eq!(verdict.settled_by, None);
    assert!(verdict.rejected() && verdict.doubted() && !verdict.settled());
    assert_eq!(counts(&verdict), (5, 5, 5));
    witnessed(&verdict, &observation);
    let sent = judge.sent.lock().unwrap();
    assert_eq!(sent[3].kind, Point);
    shown_the_run(&sent[3], &observation, Some(ORDER_PARTS[1]));
    drop(sent);
    assert_eq!(judge.left(), 0);
}

/// The answers a scripted judge gives, in order.
type Script = Vec<(Kind, Reply)>;

/// Over the run, a part shown missing that no task fails is contested, and NONE, or a pointer
/// left without a choice, keeps the part unknown: the faithful verdict never stands on an
/// uncertainty, and none of them is a defect (the judge law). NONE abstains; `missing` rejects.
#[tokio::test]
async fn an_uncertain_answer_over_the_run_never_settles_nor_locates_a_defect() {
    let cases: [(Script, Value, Declined); 3] = [
        (
            vec![(ObservedPart, Choose("none"))],
            found(&[], &[ORDER_PARTS[1]], &[], &[], &[]),
            Declined::Abstained,
        ),
        (
            vec![
                (ObservedPart, Choose("missing")),
                (Point, Choose("no_task")),
            ],
            found(&[], &[], &[ORDER_PARTS[1]], &[], &[]),
            Declined::Rejected,
        ),
        (
            vec![(ObservedPart, Choose("missing")), (Point, Choose("none"))],
            found(&[], &[ORDER_PARTS[1]], &[], &[], &[]),
            Declined::Rejected,
        ),
    ];
    for (middle, expected, declined) in cases {
        let mut script = vec![
            (Request, Choose("faithful")),
            (ObservedPart, Choose("carried")),
        ];
        script.extend(middle);
        script.push((ObservedPart, Choose("carried")));
        let judge = Scripted::new(script);
        let observation = observed(true, true);
        let Judged { verdict, .. } = provided(ORDERS, &judge, Some(&observation)).await;
        assert_eq!(verdict.judgments, NO_JUDGMENT);
        assert_eq!(lists(&verdict), expected);
        assert_eq!(verdict.declined, declined);
        assert!(verdict.defects.is_empty() && !verdict.settled() && verdict.doubted());
        assert_eq!(judge.left(), 0);
    }
}

/// A call over the run that gets no answer stops the verdict: that part and every part after it
/// stay unknown, nothing more is asked, and nothing is READY.
#[tokio::test]
async fn a_failed_call_over_the_run_stops_a_faithful_verdict() {
    let judge = Scripted::new([
        (Request, Choose("faithful")),
        (ObservedPart, Choose("carried")),
        (ObservedPart, Fail),
    ]);
    let Judged { verdict, .. } = provided(ORDERS, &judge, Some(&observed(true, true))).await;
    assert_eq!(verdict.judgments, NO_JUDGMENT);
    assert!(verdict.stopped && !verdict.settled());
    let unknown = found(&[], &[ORDER_PARTS[1], ORDER_PARTS[2]], &[], &[], &[]);
    assert_eq!(lists(&verdict), unknown);
    assert_eq!(judge.left(), 0);
}
