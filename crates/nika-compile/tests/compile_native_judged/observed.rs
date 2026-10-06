// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A doubt only a trial run of the same bytes decides (R6), end to end through the sketch door
//! and the public rehearsed entry. The judge, a decision seat, doubts the whole request, carries
//! each part asked alone and names no task doing more than asked: nothing it said locates a
//! defect, and its agreeing answers decide nothing. When the room the host lends ran the
//! candidate to completion in this compile, the configured judge is shown what that run read and
//! wrote, bound to the bytes it ran, and asked over it: inputs that exercise every part and
//! outputs read whole and written by the run, found consistent, carry the request READY, and the
//! final barrier reuses that run; a restriction the bytes left contested is asked over the run
//! first, and the run shown carrying it settles it. A restriction the judge found broken in the
//! bytes (a task named) is asked over the run too, but a run of some inputs never removes a
//! located defect: shown carried, it stays contested and the candidate is held, never READY. A
//! run read only in part, or one that skipped an output it was read for, proves no whole output,
//! and a room that declined to run the candidate has no run to show: the judge is asked nothing
//! over a run, whatever it would answer, and the candidate is held with the reason.
use super::*;
use nika_compile_cognition::decide::{
    ChoiceAnswer, ChoiceFuture, ChoiceQuestion, DecisionError, DecisionSeat,
};
use nika_compile_cognition::rehearse::{
    Attempt, Bounds, CopyReceipt, Digest, EffectCounts, FinalReceipt, FinalState, Held,
    LedgerFacts, Observation, Refusal, Rehearsal, RehearsalFuture, RehearsalReport, Rehearse,
    RehearsedOutput, RoomEvidence, Spent,
};
use nika_compile_cognition::{Cognition, compile_with_cognition_rehearsed};
use std::collections::VecDeque;
use std::sync::Mutex;

/// The tickets the room copies in, and the open ones the run writes.
const SOURCE: &str = "./tickets.json";
const TARGET: &str = "./out/open.json";
const TICKETS_IN: &str = r#"[{"id":1,"status":"open"},{"id":2,"status":"closed"}]"#;
const OPEN_OUT: &str = r#"[{"id":1,"status":"open"}]"#;
/// The second output [`SPLIT`] asks (only when a ticket is urgent), which the skipping run
/// never writes.
const SKIPPED: &str = "./out/urgent.json";

/// Why a run read only in part, or one that skipped an output, decides nothing.
const PARTIAL: &str = "the trial run wrote nothing it was read for, or was read only in part: it proves no whole output";

/// The decision seat that judges here.
pub(super) const JUDGE: &str = "mock/typed-judge";

/// How much of each text the room keeps as its evidence, and what the run wrote.
#[derive(Clone, Copy)]
enum Kept {
    Whole,
    /// Only the first half of each text, as a preview cut at the room's bound would be.
    Half,
    /// Every text whole, the open tickets written, and [`SKIPPED`] never written: the run
    /// skipped that output, which stays absent.
    Skipped,
}

/// The evidence the room keeps of `text`.
fn held(text: &str, kept: Kept) -> Held {
    match kept {
        Kept::Whole | Kept::Skipped => Held::Whole(text.to_owned()),
        Kept::Half => Held::Preview(text[..text.len() / 2].to_owned()),
    }
}

/// A host whose sealed room runs each candidate it is shown (the tickets copied in, the open ones
/// written by the run), or declines every one before an attempt (`kept` none).
struct Room {
    kept: Option<Kept>,
    shown: Mutex<Vec<String>>,
}

impl Room {
    fn new(kept: Option<Kept>) -> Self {
        Self {
            kept,
            shown: Mutex::new(Vec::new()),
        }
    }
    fn shown(&self) -> Vec<String> {
        self.shown.lock().unwrap().clone()
    }
}

impl Rehearse for Room {
    fn bound(&self) -> Duration {
        Duration::from_secs(10)
    }
    fn rehearse<'a>(&'a self, candidate: &'a str, inputs: &'a [String]) -> RehearsalFuture<'a> {
        self.rehearse_reading(candidate, inputs, &[])
    }
    fn rehearse_reading<'a>(
        &'a self,
        candidate: &'a str,
        _inputs: &'a [String],
        _targets: &'a [String],
    ) -> RehearsalFuture<'a> {
        Box::pin(async move {
            self.shown.lock().unwrap().push(candidate.to_owned());
            self.kept
                .map_or_else(|| declined(candidate), |kept| ran(candidate, kept))
        })
    }
}

/// The room's report of one completed run of `candidate`: what it copied in and read back, each
/// receipt agreeing with the bytes it spent.
fn ran(candidate: &str, kept: Kept) -> RehearsalReport {
    let input = Digest::of(TICKETS_IN.as_bytes());
    let written = Digest::of(OPEN_OUT.as_bytes());
    let mut observed = Observation::none();
    observed.bounds = Bounds::new(10_000, 1_048_576, 65_536);
    let copy = CopyReceipt::new(SOURCE, input.clone(), Some(input), held(TICKETS_IN, kept));
    observed.copies = vec![copy];
    let state = FinalState::File {
        digest: written,
        held: held(OPEN_OUT, kept),
    };
    observed.finals = vec![FinalReceipt::new(TARGET, state)];
    observed.ledger = LedgerFacts::clean(vec![TARGET.into()]);
    observed.spent = Spent::new(TICKETS_IN.len() as u64, OPEN_OUT.len() as u64);
    let output = match kept {
        Kept::Whole | Kept::Skipped => RehearsedOutput::new(TARGET, OPEN_OUT),
        Kept::Half => RehearsedOutput::new(TARGET, &OPEN_OUT[..OPEN_OUT.len() / 2]).with_full(
            OPEN_OUT.len() as u64,
            nika_compile::surface::sha256(OPEN_OUT),
        ),
    };
    let mut outputs = vec![output];
    if matches!(kept, Kept::Skipped) {
        // The skipped output: absent at the end of the run, which never published it.
        observed
            .finals
            .push(FinalReceipt::new(SKIPPED, FinalState::Absent));
        outputs.push(RehearsedOutput::new(SKIPPED, "").with_written(false));
    }
    RehearsalReport::new(
        Rehearsal::Passed { outputs },
        Attempt::Completed { elapsed_ms: 2 },
        EffectCounts::none(),
        nika_compile::surface::sha256(candidate),
    )
    .with_admitted_digest("synthetic-admission")
    .with_room(RoomEvidence::new(true, true))
    .with_observation(observed)
}

/// The room's coherent refusal of `candidate` before any attempt: no run, nothing observed.
fn declined(candidate: &str) -> RehearsalReport {
    RehearsalReport::new(
        Rehearsal::NotRun {
            reason: "declined before an attempt".into(),
        },
        Attempt::NeverAttempted,
        EffectCounts::none(),
        nika_compile::surface::sha256(candidate),
    )
    .with_observation(Observation::refused(Refusal::Effect))
}

/// The judge: it answers each question its script names, in order, and keeps every question; a
/// question its script does not name is refused (an error the verdict records), never answered.
pub(super) struct Judging {
    script: Mutex<VecDeque<(&'static str, &'static str)>>,
    asked: Mutex<Vec<ChoiceQuestion>>,
}

impl Judging {
    pub(super) fn new(script: &[(&'static str, &'static str)]) -> Self {
        Self {
            script: Mutex::new(script.iter().copied().collect()),
            asked: Mutex::new(Vec::new()),
        }
    }
    pub(super) fn asked(&self) -> Vec<ChoiceQuestion> {
        self.asked.lock().unwrap().clone()
    }
    pub(super) fn ids(&self) -> Vec<String> {
        self.asked()
            .into_iter()
            .map(|question| question.id)
            .collect()
    }
    /// The scripted answers no question asked for.
    pub(super) fn left(&self) -> usize {
        self.script.lock().unwrap().len()
    }
    /// Whether each question asked showed the judge a run.
    fn shown_runs(&self) -> Vec<bool> {
        (self.asked().iter())
            .map(|question| question.state.get("observation").is_some())
            .collect()
    }
}

impl DecisionSeat for Judging {
    fn name(&self) -> &str {
        JUDGE
    }
    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        Box::pin(async move {
            self.asked.lock().unwrap().push(question.clone());
            let mut script = self.script.lock().unwrap();
            match script.front() {
                Some((id, choice)) if *id == question.id => {
                    let answer = ChoiceAnswer::new(*choice, JUDGE);
                    script.pop_front();
                    Ok(answer)
                }
                _ => Err(DecisionError(format!("unscripted: {}", question.id))),
            }
        })
    }
}

/// The doubt no part locates: the whole request unfaithful, its two parts carried, no task doing
/// more than asked.
const DOUBT: [(&str, &str); 4] = [
    ("verify-request", "unfaithful"),
    ("verify-part-0", "carried"),
    ("verify-part-1", "carried"),
    ("verify-extra", "only_requested"),
];

/// [`DOUBT`], then the trial run found consistent with the whole request.
fn over_the_run() -> Vec<(&'static str, &'static str)> {
    let mut script = DOUBT.to_vec();
    script.push(("verify-observed", "consistent"));
    script
}

/// A request and the author's faithful sketch and fill for it.
type Authored = (&'static str, String, String);

/// [`TICKETS`], its faithful sketch and fill.
fn tickets() -> Authored {
    let open = fills("fromjson | map(select(.status == \"open\"))");
    (TICKETS, sketch(), open)
}

/// The request of two outputs, the second written only when a ticket is urgent.
const SPLIT: &str = "read ./tickets.json, keep only the open tickets and write them to ./out/open.json, and if any ticket is urgent also write the urgent ones to ./out/urgent.json";

/// [`SPLIT`], its faithful sketch (the tickets read once, each output kept and written by its own
/// tasks) and the fills of its two program holes.
fn split() -> Authored {
    let read = |from: &str| json!([{"name": "document", "from": from}]);
    let sketch = json!({"name": "split-tickets", "tasks": [
        {"id": "read_tickets", "verb": "invoke", "tool": "nika:read", "reads": [SOURCE], "purpose": "the tickets"},
        {"id": "keep_open", "verb": "invoke", "tool": "nika:jq", "with": read("read_tickets"), "purpose": "the open tickets"},
        {"id": "write_open", "verb": "invoke", "tool": "nika:write", "writes": [TARGET], "with": [{"name": "text", "from": "keep_open"}], "purpose": "write the open tickets"},
        {"id": "keep_urgent", "verb": "invoke", "tool": "nika:jq", "with": read("read_tickets"), "purpose": "the urgent tickets"},
        {"id": "write_urgent", "verb": "invoke", "tool": "nika:write", "writes": [SKIPPED], "with": [{"name": "text", "from": "keep_urgent"}], "purpose": "write the urgent tickets"}
    ], "questions": [], "gaps": [], "notes": "read, two programs, two writes"});
    let fills = json!({"fills": [
        {"task": "keep_open", "field": "expression", "value": "fromjson | map(select(.status == \"open\"))"},
        {"task": "keep_urgent", "field": "expression", "value": "fromjson | map(select(.priority == \"urgent\"))"}
    ], "notes": "two holes"});
    (SPLIT, sketch.to_string(), fills.to_string())
}

/// The authored request through the sketch door, judged by `judge` and rehearsed in `room`: the
/// outcome, after the author was asked the sketch and its fill only.
async fn judged_in(
    room: &Room,
    judge: &Judging,
    (intent, sketch, fill): Authored,
) -> CompileOutcome {
    let author = Rotating::new(vec![sketch, fill]);
    let cognition = Cognition {
        provider: Some(&author),
        seat: Some(judge as &dyn DecisionSeat),
    };
    let request = CompileRequest::create(intent).with_authoring_policy(policy(NativeMode::Sketch));
    let out = compile_with_cognition_rehearsed(&request, cognition, Some(room as &dyn Rehearse))
        .await
        .unwrap();
    assert_eq!(author.calls.load(Ordering::SeqCst), 2, "{out:#?}");
    out
}

/// The run the judge is shown: the texts the room copied in and read back, bound to `candidate`.
fn observation(candidate: &str, kept: Kept) -> Value {
    let text = |text: &str| held(text, kept).text().to_owned();
    let whole = matches!(kept, Kept::Whole);
    json!({
        "candidate_sha256": nika_compile::surface::sha256(candidate),
        "inputs": [{"path": SOURCE, "text": text(TICKETS_IN), "read_whole": whole}],
        "outputs": [{"path": TARGET, "text": text(OPEN_OUT), "written": true, "read_whole": whole}],
    })
}

/// What the verdict records of the run it showed: digests and sizes, never the texts again.
fn receipts(observed: &Value) -> Value {
    let receipt = |role: &str, text: &Value, written: Value| {
        let body = text["text"].as_str().unwrap_or_default();
        json!({"role": role, "path": text["path"], "bytes": body.len(),
            "sha256": nika_compile::surface::sha256(body), "read_whole": text["read_whole"],
            "written": written})
    };
    let input = receipt("input", &observed["inputs"][0], Value::Null);
    let output = receipt("output", &observed["outputs"][0], json!(true));
    json!({
        "candidate_sha256": observed["candidate_sha256"],
        "sha256": nika_compile::surface::sha256(&observed.to_string()),
        "texts": [input, output],
    })
}

/// The question over the run: consistent, some part never exercised by these inputs, each part,
/// each task in the document's order.
const OVER_THE_RUN: [&str; 8] = [
    "consistent",
    "unexercised",
    "part-0",
    "part-1",
    "task-keep_open",
    "task-read_tickets",
    "task-write_open",
    "none",
];

#[tokio::test]
async fn a_trial_run_found_consistent_carries_a_doubted_request_ready() {
    let room = Room::new(Some(Kept::Whole));
    let judge = Judging::new(&over_the_run());
    let out = judged_in(&room, &judge, tickets()).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let candidate = out.candidate.clone().unwrap();
    assert_eq!(judge.left(), 0, "{out:#?}");
    let ids: Vec<&str> = over_the_run().iter().map(|(id, _)| *id).collect();
    assert_eq!(judge.ids(), ids);
    // The judge read the run of exactly these bytes, and only over the question that asks it.
    let asked = judge.asked();
    let shown = observation(&candidate, Kept::Whole);
    assert_eq!(asked[4].state["observation"], shown);
    assert_eq!(asked[4].keys(), OVER_THE_RUN);
    assert_eq!(judge.shown_runs(), [false, false, false, false, true]);
    // One run, the one the judge read, reused by the final barrier.
    assert_eq!(room.shown(), std::slice::from_ref(&candidate));
    let decision = out.provenance.decision.as_ref().unwrap();
    let reports = decision["rehearsal"]["reports"].as_array().map(Vec::len);
    assert_eq!(reports, Some(1), "{decision:#}");
    // The run decided the doubt: the request is carried by the question over the run.
    let attempt = &decision["semantic_verification"][0];
    assert_eq!(attempt["doubt"], json!(["unfaithful"]), "{attempt:#}");
    assert_eq!(
        attempt["settled_by"],
        json!("verify-observed"),
        "{attempt:#}"
    );
    for list in ["defects", "unknown", "contested", "unsettled", "notes"] {
        assert_eq!(attempt[list], json!([]), "{list}: {attempt:#}");
    }
    let counts = (&attempt["attempted"], &attempt["consumed"]);
    assert_eq!(counts, (&json!(5), &json!(5)), "{attempt:#}");
    let sha = nika_compile::surface::sha256(&candidate);
    assert_eq!(attempt["candidate_sha256"], json!(sha), "{attempt:#}");
    let record = &attempt["questions"][4];
    let over = (&record["question"], &record["role"], &record["choice"]);
    let expected = (
        &json!("verify-observed"),
        &json!("judge_observed"),
        &json!("consistent"),
    );
    assert_eq!(over, expected, "{record:#}");
    assert_eq!(record["observation"], receipts(&shown), "{record:#}");
    assert!(
        route(&out).contains("verify: judged (decision_seat)"),
        "{out:#?}"
    );
    assert!(out.provenance.plan.is_some(), "the READY record: {out:#?}");
}

/// A run read only in part proves no whole output: the judge, ready to find it consistent, is
/// asked nothing over it and shown none of its texts, and the doubted request is held with the
/// reason.
#[tokio::test]
async fn a_trial_run_read_only_in_part_leaves_the_doubted_request_held() {
    let room = Room::new(Some(Kept::Half));
    let judge = Judging::new(&over_the_run());
    let out = judged_in(&room, &judge, tickets()).await;
    assert_held(&out);
    // The question over the run is never asked: its scripted answer is left.
    assert_eq!(judge.left(), 1, "{out:#?}");
    let ids: Vec<&str> = DOUBT.iter().map(|(id, _)| *id).collect();
    assert_eq!(judge.ids(), ids, "no question over a run: {out:#?}");
    assert_eq!(judge.shown_runs(), [false; 4]);
    let candidate = out.candidate.clone().unwrap();
    assert_eq!(room.shown(), [candidate]);
    let attempt = &verification(&out)[0];
    assert_eq!(attempt["contested"], json!([TICKETS]), "{attempt:#}");
    assert_eq!(attempt["unsettled"], json!([PARTIAL]), "{attempt:#}");
    assert_eq!(attempt["settled_by"], Value::Null, "{attempt:#}");
    let told = findings(&out, "semantic_verification");
    assert_eq!(told, [contested_whole(PARTIAL)], "{told:?}");
}

/// A room that declined to run the candidate leaves no run of these bytes: the judge is asked
/// nothing over a run and reads no text of one, and the doubted request is held with the reason
/// no trial decided it.
#[tokio::test]
async fn a_declined_rehearsal_shows_the_judge_no_run_and_holds_the_doubted_request() {
    let room = Room::new(None);
    let judge = Judging::new(&DOUBT);
    let out = judged_in(&room, &judge, tickets()).await;
    assert_held(&out);
    assert_eq!(judge.left(), 0, "{out:#?}");
    let ids: Vec<&str> = DOUBT.iter().map(|(id, _)| *id).collect();
    assert_eq!(judge.ids(), ids, "no question over a run: {out:#?}");
    assert_eq!(judge.shown_runs(), [false; 4]);
    let candidate = out.candidate.clone().unwrap();
    assert_eq!(room.shown(), [candidate]);
    let decision = out.provenance.decision.as_ref().unwrap();
    let report = &decision["rehearsal"]["reports"][0];
    assert_eq!(report["outcome"]["kind"], "not_run", "{decision:#}");
    let attempt = &verification(&out)[0];
    assert_eq!(attempt["contested"], json!([TICKETS]), "{attempt:#}");
    assert_eq!(attempt["unsettled"], json!([NO_TRIAL]), "{attempt:#}");
    assert_eq!(attempt["unknown"], json!([]), "{attempt:#}");
    assert_eq!(attempt["settled_by"], Value::Null, "{attempt:#}");
    let told = findings(&out, "semantic_verification");
    assert_eq!(told, [contested_whole(NO_TRIAL)], "{told:?}");
}

/// A run that skipped an output it was read for proves no whole output, though everything it
/// wrote was read whole: the run's own record says which output it never wrote, the judge,
/// ready to find the run consistent, is asked nothing over it, and the doubted request is held
/// with the reason.
#[tokio::test]
async fn a_trial_run_that_skipped_an_output_leaves_the_doubted_request_held() {
    let room = Room::new(Some(Kept::Skipped));
    let script = [
        ("verify-request", "unfaithful"),
        ("verify-part-0", "carried"),
        ("verify-part-1", "carried"),
        ("verify-part-2", "carried"),
        ("verify-extra", "only_requested"),
        ("verify-observed", "consistent"),
    ];
    let judge = Judging::new(&script);
    let out = judged_in(&room, &judge, split()).await;
    assert_held(&out);
    assert_eq!(judge.left(), 1, "{out:#?}");
    let ids: Vec<&str> = script[..5].iter().map(|(id, _)| *id).collect();
    assert_eq!(judge.ids(), ids, "no question over a run: {out:#?}");
    assert_eq!(judge.shown_runs(), [false; 5]);
    let candidate = out.candidate.clone().unwrap();
    assert_eq!(room.shown(), [candidate]);
    let decision = out.provenance.decision.as_ref().unwrap();
    let report = &decision["rehearsal"]["reports"][0];
    let read_back = json!([
        {"path": TARGET, "text": OPEN_OUT, "written": true, "truncated": false},
        {"path": SKIPPED, "text": "", "written": false, "truncated": false},
    ]);
    assert_eq!(report["read_back"], read_back, "{decision:#}");
    assert_eq!(
        report["decision"],
        json!({"kind": "proceed"}),
        "{decision:#}"
    );
    let attempt = &verification(&out)[0];
    assert_eq!(attempt["contested"], json!([SPLIT]), "{attempt:#}");
    assert_eq!(attempt["unsettled"], json!([PARTIAL]), "{attempt:#}");
    assert_eq!(attempt["unknown"], json!([]), "{attempt:#}");
    assert_eq!(attempt["defects"], json!([]), "{attempt:#}");
    assert_eq!(attempt["settled_by"], Value::Null, "{attempt:#}");
    let told = findings(&out, "semantic_verification");
    assert_eq!(told, [contested_whole(PARTIAL)], "{told:?}");
}

/// What a question over the run says of a clause that restricts.
const RESTRICTS: &str = "This clause RESTRICTS (a prohibition, a condition, an exclusion or an only): it is carried when no task does what it forbids and every task honors its condition, even though no task states it.";

/// A restriction the bytes left contested (judged missing, then failed by no task) is judged
/// again over a whole run of those bytes: the run shows it carried, which settles it; nothing is
/// open, so the whole request is asked over the run, found consistent, and READY. The judge read
/// the run only in the two questions that ask over it, and each of their records keeps the run's
/// receipts.
#[tokio::test]
async fn a_whole_run_settles_a_contested_restriction_then_carries_the_request_ready() {
    let room = Room::new(Some(Kept::Whole));
    let script = [
        ("verify-request", "unfaithful"),
        ("verify-part-0", "carried"),
        ("verify-part-1", "missing"),
        ("verify-point-1", "no_task"),
        ("verify-observed-part-1", "carried"),
        ("verify-extra", "only_requested"),
        ("verify-observed", "consistent"),
    ];
    let judge = Judging::new(&script);
    let out = judged_in(&room, &judge, tickets()).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(judge.left(), 0, "{out:#?}");
    let ids: Vec<&str> = script.iter().map(|(id, _)| *id).collect();
    assert_eq!(judge.ids(), ids);
    let shown_runs = [false, false, false, false, true, false, true];
    assert_eq!(judge.shown_runs(), shown_runs);
    let candidate = out.candidate.clone().unwrap();
    let shown = observation(&candidate, Kept::Whole);
    let asked = judge.asked();
    let over_part = &asked[4];
    assert_eq!(
        over_part.keys(),
        ["carried", "missing", "unexercised", "none"]
    );
    assert_eq!(over_part.state["observation"], shown);
    assert_eq!(over_part.state["clause"], json!({"text": OPEN_ONLY}));
    assert!(over_part.instructions.contains(RESTRICTS), "{over_part:#?}");
    assert_eq!(asked[6].keys(), OVER_THE_RUN);
    assert_eq!(asked[6].state["observation"], shown);
    // One run, the one the judge read, reused by the final barrier.
    assert_eq!(room.shown(), std::slice::from_ref(&candidate));
    let attempt = &verification(&out)[0];
    assert_eq!(
        attempt["settled_by"],
        json!("verify-observed"),
        "{attempt:#}"
    );
    for list in ["defects", "unknown", "contested", "unsettled", "notes"] {
        assert_eq!(attempt[list], json!([]), "{list}: {attempt:#}");
    }
    let counts = (&attempt["attempted"], &attempt["consumed"]);
    assert_eq!(counts, (&json!(7), &json!(7)), "{attempt:#}");
    let records = attempt["questions"].as_array().cloned().unwrap_or_default();
    let roles: Vec<&Value> = records.iter().map(|record| &record["role"]).collect();
    let want = [
        "judge_request",
        "judge_part",
        "judge_part",
        "judge_point",
        "judge_observed_part",
        "judge_extra",
        "judge_observed",
    ];
    assert_eq!(
        roles,
        want.map(|role| json!(role)).iter().collect::<Vec<_>>()
    );
    let over = &records[4];
    let settled = (&over["question"], &over["choice"], &over["clause"]);
    let clause = json!({"text": OPEN_ONLY, "restricts": true});
    let expected = (&json!("verify-observed-part-1"), &json!("carried"), &clause);
    assert_eq!(settled, expected, "{over:#}");
    for at in [4, 6] {
        assert_eq!(records[at]["observation"], receipts(&shown), "{at}");
    }
    for at in [0, 1, 2, 3, 5] {
        assert!(records[at].get("observation").is_none(), "{at}");
    }
    assert!(out.provenance.plan.is_some(), "the READY record: {out:#?}");
}

/// What a defect located in the bytes adds when the trial run shows it done for its inputs.
const SHOWN_DONE: &str =
    "the trial run shows it done for its inputs, which never removes a defect located in the bytes";

/// The judge's script of the trial-run law: the request unfaithful, the restriction missing in
/// the bytes with the task failing it named (a located defect), then shown carried by the run.
const AGAINST_A_DEFECT: [(&str, &str); 5] = [
    ("verify-request", "unfaithful"),
    ("verify-part-0", "carried"),
    ("verify-part-1", "missing"),
    ("verify-point-1", "task-keep_open"),
    ("verify-observed-part-1", "carried"),
];

/// The trial-run law (R6, A1), end to end through the sketch door and the rehearsal host: the
/// judge doubts the request, finds the restriction missing in the bytes and names the task that
/// fails it, a located defect. A whole run of those bytes is shown to the judge, who answers the
/// restriction carried over it: these inputs may simply never meet the case it forbids, and a run
/// of some inputs never removes a defect located in the bytes. The defect stands, its note saying
/// what the run showed, for a repair: neither the extra question nor the whole request over the
/// run is asked, nothing is READY, and with no reopening left the candidate is withdrawn naming
/// the defect and its reasons, never told as a part no task fails.
#[tokio::test]
async fn a_run_shown_carrying_a_located_defect_keeps_the_defect() {
    let room = Room::new(Some(Kept::Whole));
    let judge = Judging::new(&AGAINST_A_DEFECT);
    let out = judged_in(&room, &judge, tickets()).await;
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert!(out.candidate.is_none(), "withdrawn: {out:#?}");
    assert_eq!(judge.left(), 0, "{out:#?}");
    let ids: Vec<&str> = AGAINST_A_DEFECT.iter().map(|(id, _)| *id).collect();
    assert_eq!(judge.ids(), ids, "no question over the whole run: {out:#?}");
    let shown_runs = [false, false, false, false, true];
    assert_eq!(judge.shown_runs(), shown_runs);
    let candidate = room.shown().first().cloned().unwrap_or_default();
    let shown = observation(&candidate, Kept::Whole);
    let over_part = &judge.asked()[4];
    let keys = ["carried", "missing", "unexercised", "none"];
    assert_eq!(over_part.keys(), keys, "{over_part:#?}");
    assert_eq!(over_part.state["observation"], shown);
    assert_eq!(over_part.state["clause"], json!({"text": OPEN_ONLY}));
    assert!(over_part.instructions.contains(RESTRICTS), "{over_part:#?}");
    let attempt = &verification(&out)[0];
    let note = format!("the judge points to the task keep_open; {SHOWN_DONE}");
    assert_eq!(attempt["defects"], json!([OPEN_ONLY]), "{attempt:#}");
    let noted = json!([{"defect": OPEN_ONLY, "note": note}]);
    assert_eq!(attempt["notes"], noted, "{attempt:#}");
    for list in ["unknown", "contested", "unsettled"] {
        assert_eq!(attempt[list], json!([]), "{list}: {attempt:#}");
    }
    assert_eq!(attempt["settled_by"], Value::Null, "{attempt:#}");
    let flags = (
        &attempt["declined"],
        &attempt["rejected"],
        &attempt["settled"],
    );
    let expected = (&json!(true), &json!(true), &json!(false));
    assert_eq!(flags, expected, "{attempt:#}");
    let counts = (&attempt["attempted"], &attempt["consumed"]);
    assert_eq!(counts, (&json!(5), &json!(5)), "{attempt:#}");
    let records = attempt["questions"].as_array().cloned().unwrap_or_default();
    let over = &records[4];
    let judged = (&over["question"], &over["choice"], &over["clause"]);
    let clause = json!({"text": OPEN_ONLY, "restricts": true});
    let expected = (&json!("verify-observed-part-1"), &json!("carried"), &clause);
    assert_eq!(judged, expected, "{over:#}");
    assert_eq!(over["observation"], receipts(&shown), "{over:#}");
    for at in [0, 1, 2, 3] {
        assert!(records[at].get("observation").is_none(), "{at}");
    }
    // The defect's finding names the part and both reasons; no part is told as failed by no task.
    let told = findings(&out, "semantic_verification");
    let defect = format!("« {OPEN_ONLY} ({note}) »");
    assert!(told.iter().any(|t| t.contains(&defect)), "{told:?}");
    assert!(
        !format!("{told:?}").contains("named no task that fails it"),
        "{told:?}"
    );
}
