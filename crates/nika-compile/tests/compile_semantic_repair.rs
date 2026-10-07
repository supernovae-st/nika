// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Repair on actual evidence (slice E). The semantic CREATE route over a scripted seat and a scripted rehearsal host,
//! through the public `compile_with_cognition_rehearsed`. The oracle is independent of the
//! product: a fixed expected count computed by this file from the fixture rows, compared with
//! the value the host observed in the room. The request-derived contract is only asserted to be
//! supported (non-empty), never used as the test's own oracle.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{AuthoringPolicy, CompileOutcome, CompileRequest, CompileStatus, NativeMode};
use nika_compile_cognition::rehearse::{
    Attempt, Bounds, CopyReceipt, Digest, EffectCounts, FinalReceipt, FinalState, Held,
    LedgerFacts, Observation, Rehearsal, RehearsalFuture, RehearsalReport, Rehearse,
    RehearsedOutput, RoomEvidence, Spent,
};
use nika_compile_cognition::{Cognition, compile_with_cognition_rehearsed};
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, ResponseFormat,
    StopReason, TokenUsage,
};
use serde_json::{Value, json};
use std::sync::Mutex;
use std::time::Duration;

const INTENT: &str = "read ./data/input.csv, count the rows where status is paid, write the count to ./out/result.json";
const INPUT: &str = "./data/input.csv";
const RESULT: &str = "./out/result.json";
/// Two rows, one paid.
const ROWS: &str = "id,amount_usd,status\n1,5,paid\n2,20,late\n";

/// The count this file expects, computed from the fixture alone (independent of the product).
fn expected_count() -> usize {
    ROWS.lines()
        .skip(1)
        .filter(|row| row.rsplit(',').next() == Some("paid"))
        .count()
}

/// The graph: read, parse, one program, write. Structure only.
fn sketch() -> String {
    let task = |id: &str, tool: &str, extra: Value| {
        let mut t = json!({"id": id, "verb": "invoke", "tool": tool, "purpose": id});
        for (k, v) in extra.as_object().unwrap() {
            t[k] = v.clone();
        }
        t
    };
    json!({"name": "paid-count", "tasks": [
        task("read_input", "nika:read", json!({"reads": [INPUT]})),
        task("parse", "nika:convert", json!({"with": [{"name": "document", "from": "read_input"}]})),
        task("count", "nika:jq", json!({"with": [{"name": "rows", "from": "parse"}]})),
        task("write_result", "nika:write", json!({"writes": [RESULT], "with": [{"name": "text", "from": "count"}]})),
    ], "questions": [], "gaps": [], "notes": "read, parse, count, write"})
    .to_string()
}

const RAW_COUNT: &str = "fromjson | {count: length}";
const PAID_COUNT: &str = "fromjson | {count: (map(select(.status == \"paid\")) | length)}";

fn fills(expression: &str) -> String {
    json!({"fills": [
        {"task": "parse", "field": "args", "value": {"from": "csv", "to": "json"}},
        {"task": "count", "field": "expression", "value": expression},
    ], "notes": "two holes"})
    .to_string()
}

/// A seat answering by the schema it is asked: its queued sketches and fills in order (the last
/// one repeats), and an approving judge.
struct Seat {
    sketches: Mutex<Vec<String>>,
    fills: Mutex<Vec<String>>,
    calls: Mutex<Vec<String>>,
}

/// The next queued answer: the head, removed while another remains, else the last one again.
fn next(queue: &Mutex<Vec<String>>) -> String {
    let mut queued = queue.lock().unwrap();
    if queued.len() > 1 {
        queued.remove(0)
    } else {
        queued[0].clone()
    }
}

impl ProviderInferDyn for Seat {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        let schema = match &request.response_format {
            ResponseFormat::JsonSchema(schema) => schema.clone(),
            _ => Value::Null,
        };
        let properties = &schema["properties"];
        let text = if let Some(keys) = properties["choice"]["enum"].as_array() {
            let approve = ["faithful", "carried"]
                .into_iter()
                .find(|key| keys.iter().any(|value| value == *key))
                .unwrap_or("none");
            json!({"choice": approve}).to_string()
        } else if properties.get("fills").is_some() {
            next(&self.fills)
        } else {
            next(&self.sketches)
        };
        self.calls.lock().unwrap().push(text.clone());
        Ok(InferResponse::new(
            vec![ContentBlock::Text { text }],
            TokenUsage::new(1, 1),
            StopReason::EndTurn,
        ))
    }
}

/// A room that runs the candidate as the fixture defines it: the filtered program writes the
/// paid count, any other writes the raw row count. It records every candidate it was shown.
/// A stale room answers every later run with a success report of the first candidate it ran.
struct Room {
    shown: Mutex<Vec<String>>,
    stale: bool,
    /// The first run reaches the room's time bound: stopped, nothing observed.
    stop_first: bool,
}

fn observed_count(candidate: &str) -> usize {
    if candidate.contains("select(.status") {
        1
    } else {
        2
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
            // A stale room reports, after its first run, that first candidate's bytes with a
            // success that writes the expected count: evidence of other bytes than these.
            let stale = {
                let mut shown = self.shown.lock().unwrap();
                shown.push(candidate.to_owned());
                (self.stale && shown.len() > 1).then(|| shown[0].clone())
            };
            if self.stop_first && self.shown.lock().unwrap().len() == 1 {
                return stopped(candidate);
            }
            let reported = stale.as_deref().unwrap_or(candidate);
            let count = if stale.is_some() {
                expected_count()
            } else {
                observed_count(candidate)
            };
            let written = json!({"count": count}).to_string();
            let mut observed = Observation::none();
            observed.bounds = Bounds::new(10_000, 1_048_576, 65_536);
            let input = Digest::of(ROWS.as_bytes());
            observed.copies = vec![CopyReceipt::new(
                INPUT,
                input.clone(),
                Some(input),
                Held::Whole(ROWS.into()),
            )];
            observed.finals = vec![FinalReceipt::new(
                RESULT,
                FinalState::File {
                    digest: Digest::of(written.as_bytes()),
                    held: Held::Whole(written.clone()),
                },
            )];
            observed.ledger = LedgerFacts::clean(vec![RESULT.into()]);
            // What the room spent is exactly what its receipts hold: the copy in, the result out.
            observed.spent = Spent::new(ROWS.len() as u64, written.len() as u64);
            let digest = nika_compile::surface::sha256(reported);
            RehearsalReport::new(
                Rehearsal::Passed {
                    outputs: vec![RehearsedOutput::new(RESULT, &written)],
                },
                Attempt::Completed { elapsed_ms: 2 },
                EffectCounts::none(),
                digest,
            )
            .with_admitted_digest("synthetic-admission")
            .with_room(RoomEvidence::new(true, true))
            .with_observation(observed)
        })
    }
}

/// A run the room stopped at its time bound: no output, nothing written.
fn stopped(candidate: &str) -> RehearsalReport {
    let mut observed = Observation::none();
    observed.bounds = Bounds::new(10_000, 1_048_576, 65_536);
    observed.ledger = LedgerFacts::clean(Vec::new());
    let input = Digest::of(ROWS.as_bytes());
    observed.copies = vec![CopyReceipt::new(
        INPUT,
        input.clone(),
        Some(input),
        Held::Whole(ROWS.into()),
    )];
    observed.spent = Spent::new(ROWS.len() as u64, 0);
    observed.finals = vec![FinalReceipt::new(RESULT, FinalState::Absent)];
    RehearsalReport::new(
        Rehearsal::NotRun {
            reason: "stopped at the time bound".into(),
        },
        Attempt::Stopped { elapsed_ms: 10_000 },
        EffectCounts::none(),
        nika_compile::surface::sha256(candidate),
    )
    .with_admitted_digest("synthetic-admission")
    .with_room(RoomEvidence::new(true, true))
    .with_observation(observed)
}

fn request(repairs: u32) -> CompileRequest {
    CompileRequest::create(INTENT).with_authoring_policy(
        AuthoringPolicy::new("mock/author", 4096, Duration::from_secs(2))
            .with_native(NativeMode::Sketch)
            .with_repairs(repairs),
    )
}

async fn compiled(repairs: u32, fills_queue: Vec<String>) -> (CompileOutcome, Seat, Room) {
    let seat = Seat {
        sketches: Mutex::new(vec![sketch()]),
        fills: Mutex::new(fills_queue),
        calls: Mutex::new(Vec::new()),
    };
    let room = Room {
        shown: Mutex::new(Vec::new()),
        stale: false,
        stop_first: false,
    };
    let out = compile_with(&request(repairs), &seat, &room).await;
    (out, seat, room)
}

async fn compile_with(request: &CompileRequest, seat: &Seat, room: &Room) -> CompileOutcome {
    compile_with_cognition_rehearsed(
        request,
        Cognition {
            provider: Some(seat),
            seat: None,
        },
        Some(room),
    )
    .await
    .unwrap()
}

/// RED at the parent: a graph that runs to completion but counts every row writes 2 where the
/// request asks 1. Its READY would be a wrong READY: a candidate the room observed writing the
/// wrong count never leaves as READY, and the repaired one that writes the expected count does.
#[tokio::test]
async fn a_completed_run_with_the_wrong_count_is_never_ready_and_its_repair_is() {
    // The request's own contract is supported here (the oracle below does not use it).
    let contract = nika_compile_fidelity::behavior::contract_of_request(
        INTENT,
        &std::collections::BTreeMap::default(),
    );
    assert!(!contract.obligations.is_empty(), "a supported contract");
    assert_eq!(expected_count(), 1);
    assert_eq!(
        observed_count(&fills(RAW_COUNT)),
        2,
        "the fixture really differs"
    );
    let (out, _seat, room) = compiled(2, vec![fills(RAW_COUNT), fills(PAID_COUNT)]).await;
    let shown = room.shown.lock().unwrap().clone();
    // The first candidate counts every row, and the room observed it writing the wrong count.
    let first = shown.first().expect("the room observed a candidate");
    assert_eq!(observed_count(first), 2, "{first}");
    // Its repair is READY, and only on bytes the room observed writing the expected count.
    assert_eq!(out.status, CompileStatus::Ready, "{:#?}", out.diagnostics);
    let candidate = out.candidate.as_deref().unwrap();
    assert_eq!(
        observed_count(candidate),
        expected_count(),
        "READY only for bytes the room observed writing the expected count: {candidate}"
    );
    assert_eq!(
        shown.last().map(String::as_str),
        Some(candidate),
        "fresh evidence of these bytes"
    );
}

/// The journal's evidence entries, in order: what each rehearsed candidate showed.
fn evidence(out: &CompileOutcome) -> Vec<Value> {
    let rounds = &out.provenance.decision.as_ref().unwrap()["native"]["rounds"];
    (rounds.as_array().into_iter().flatten())
        .filter_map(|round| round.get("evidence").cloned())
        .collect()
}

fn seat(sketches: Vec<String>, fills_queue: Vec<String>) -> Seat {
    Seat {
        sketches: Mutex::new(sketches),
        fills: Mutex::new(fills_queue),
        calls: Mutex::new(Vec::new()),
    }
}

fn room(stale: bool) -> Room {
    Room {
        shown: Mutex::new(Vec::new()),
        stale,
        stop_first: false,
    }
}

/// The rehearsal's own journal: one report per run the room was asked for.
fn reports(out: &CompileOutcome) -> Vec<Value> {
    let rehearsal = &out.provenance.decision.as_ref().unwrap()["rehearsal"];
    rehearsal["reports"].as_array().cloned().unwrap_or_default()
}

/// The repaired witness keeps one contract across its attempts, records both candidates, and
/// spends one round count: the sketch, its fills, the reopened sketch, its fills, the judgment
/// (the whole request, then each of its three parts over the trial run of the faithful bytes).
#[tokio::test]
async fn a_repair_keeps_the_request_contract_and_spends_one_round_count() {
    let (out, seat, room) = compiled(2, vec![fills(RAW_COUNT), fills(PAID_COUNT)]).await;
    assert_eq!(out.status, CompileStatus::Ready, "{:#?}", out.diagnostics);
    let found = evidence(&out);
    assert_eq!(found.len(), 2, "{found:#?}");
    assert_eq!(found[0]["outcome"], "defect");
    assert_eq!(found[1]["outcome"], "holds");
    assert_eq!(
        found[0]["behaviour"]["contract_sha256"], found[1]["behaviour"]["contract_sha256"],
        "one request contract across the attempts"
    );
    let shown = room.shown.lock().unwrap().clone();
    assert_eq!(
        found[0]["candidate_sha256"],
        nika_compile::surface::sha256(&shown[0])
    );
    assert_eq!(
        found[1]["candidate_sha256"],
        nika_compile::surface::sha256(&shown[1])
    );
    assert_ne!(shown[0], shown[1], "the repair changed the bytes");
    let receipt = out.provenance.authoring.as_ref().unwrap();
    let roles: Vec<&str> = (receipt.context.iter())
        .filter_map(|call| call["call"].as_str())
        .collect();
    assert_eq!(
        roles,
        [
            "sketch",
            "fill",
            "sketch-repair",
            "fill",
            "judge_request",
            "judge_observed_part",
            "judge_observed_part",
            "judge_observed_part"
        ]
    );
    assert_eq!(receipt.calls as usize, seat.calls.lock().unwrap().len());
    // One run per candidate: the final barrier reused the last report, charged once.
    let usage = &out.provenance.decision.as_ref().unwrap()["rehearsal"]["usage"];
    assert_eq!(usage["fixtures"], 2, "{usage}");
}

/// A "repair" that fixes the count but drops the human gate the request states is refused by
/// the laws: the policy the request set survives every repair, and nothing is READY.
#[tokio::test]
async fn a_repair_that_drops_the_stated_gate_is_never_ready() {
    const GATED: &str = "read ./data/input.csv, count the rows where status is paid, then ask me before you write the count to ./out/result.json";
    let mut gated: Value = serde_json::from_str(&sketch()).unwrap();
    let tasks = gated["tasks"].as_array_mut().unwrap();
    tasks.insert(
        3,
        json!({"id": "approve", "verb": "invoke", "tool": "nika:prompt", "purpose": "ask first"}),
    );
    tasks[4]["gated_by"] = json!("approve");
    let mut fills_gated: Value = serde_json::from_str(&fills(RAW_COUNT)).unwrap();
    fills_gated["fills"]
        .as_array_mut()
        .unwrap()
        .push(json!({"task": "approve", "field": "args.message", "value": "Write the count?"}));
    let seat = seat(
        vec![gated.to_string(), sketch()],
        vec![fills_gated.to_string(), fills(PAID_COUNT)],
    );
    let room = room(false);
    let request = CompileRequest::create(GATED).with_authoring_policy(
        AuthoringPolicy::new("mock/author", 4096, Duration::from_secs(2))
            .with_native(NativeMode::Sketch)
            .with_repairs(2),
    );
    let out = compile_with(&request, &seat, &room).await;
    assert_ne!(out.status, CompileStatus::Ready, "{:#?}", out.diagnostics);
    let candidate = out.candidate.as_deref().unwrap_or_default();
    assert!(
        !candidate.contains("select(.status"),
        "the ungated repair never ships: {candidate}"
    );
    let shown = room.shown.lock().unwrap().clone();
    assert!(!shown.is_empty(), "the gated candidate ran");
    assert!(
        shown.iter().all(|ran| ran.contains("nika:prompt")),
        "only gated graphs ever ran"
    );
    // The gated graph's wrong count was demonstrated, so the repair was really attempted.
    assert_eq!(
        evidence(&out)[0]["outcome"],
        "defect",
        "{:#?}",
        evidence(&out)
    );
    let rounds = &out.provenance.decision.as_ref().unwrap()["native"]["rounds"];
    let reopened_refused = (rounds.as_array().into_iter().flatten()).any(|round| {
        round["phase"] == "sketch"
            && round["round"].as_u64() > Some(0)
            && round["diagnostics"]
                .as_array()
                .is_some_and(|d| !d.is_empty())
    });
    assert!(
        reopened_refused,
        "the ungated graph was refused: {rounds:#}"
    );
}

/// A "repair" that deletes the required output is refused by the laws, never READY.
#[tokio::test]
async fn a_repair_that_drops_the_required_output_is_never_ready() {
    let mut dropped: Value = serde_json::from_str(&sketch()).unwrap();
    dropped["tasks"].as_array_mut().unwrap().pop();
    let seat = seat(vec![sketch(), dropped.to_string()], vec![fills(RAW_COUNT)]);
    let room = room(false);
    let out = compile_with(&request(2), &seat, &room).await;
    assert_ne!(out.status, CompileStatus::Ready, "{:#?}", out.diagnostics);
    assert!(out.candidate.is_none());
    assert_eq!(
        room.shown.lock().unwrap().len(),
        1,
        "the dropped graph never ran"
    );
    let rounds = &out.provenance.decision.as_ref().unwrap()["native"]["rounds"];
    let refused = (rounds.as_array().into_iter().flatten()).any(|round| {
        round["phase"] == "sketch"
            && round["diagnostics"]
                .as_array()
                .is_some_and(|d| !d.is_empty())
    });
    assert!(
        refused,
        "the reopened graph was refused by the laws: {rounds:#}"
    );
}

/// A stale successful report (the first candidate's bytes, claiming the expected count) never
/// stands for the repaired bytes: the evidence is invalid and nothing is READY.
#[tokio::test]
async fn a_stale_success_report_never_makes_another_candidate_ready() {
    let seat = seat(vec![sketch()], vec![fills(RAW_COUNT), fills(PAID_COUNT)]);
    let room = room(true);
    let out = compile_with(&request(2), &seat, &room).await;
    assert_ne!(out.status, CompileStatus::Ready, "{:#?}", out.diagnostics);
    assert!(out.candidate.is_none());
    assert_eq!(room.shown.lock().unwrap().len(), 2);
    let found = evidence(&out);
    assert_eq!(found.last().unwrap()["outcome"], "stop", "{found:#?}");
}

/// No allowance left: the wrong candidate is withdrawn, no judge is asked, no further request or
/// run is made.
#[tokio::test]
async fn a_spent_repair_allowance_sends_nothing_more_and_is_never_ready() {
    let (out, seat, room) = compiled(0, vec![fills(RAW_COUNT), fills(PAID_COUNT)]).await;
    assert_ne!(out.status, CompileStatus::Ready, "{:#?}", out.diagnostics);
    assert!(out.candidate.is_none());
    assert_eq!(
        seat.calls.lock().unwrap().len(),
        2,
        "the sketch and its fills only"
    );
    assert_eq!(
        room.shown.lock().unwrap().len(),
        1,
        "one run, no hidden second"
    );
    assert_eq!(evidence(&out)[0]["outcome"], "defect");
    let receipt = out.provenance.authoring.as_ref().unwrap();
    assert!(
        !(receipt.context.iter()).any(|call| call["call"] == "judge_request"),
        "no judgment of a withdrawn candidate"
    );
}

/// A request the behavioural judge does not support keeps its business result UNKNOWN: the run
/// is recorded, never a pass, and no repair is made from it.
#[tokio::test]
async fn an_unsupported_request_keeps_its_business_result_unknown() {
    const SUMMARY: &str =
        "read ./data/input.csv, summarize the rows, write the summary to ./out/result.json";
    let summary = json!({"name": "summary", "tasks": [
        {"id": "read_input", "verb": "invoke", "tool": "nika:read", "purpose": "read", "reads": [INPUT]},
        {"id": "summarize", "verb": "infer", "purpose": "summarize", "with": [{"name": "rows", "from": "read_input"}]},
        {"id": "write_result", "verb": "invoke", "tool": "nika:write", "purpose": "write", "writes": [RESULT], "with": [{"name": "text", "from": "summarize"}]},
    ], "questions": [], "gaps": [], "notes": "read, summarize, write"})
    .to_string();
    let prompt = json!({"fills": [{"task": "summarize", "field": "prompt",
        "value": "Summarize these rows without inventing anything: ${{ with.rows }}"}], "notes": "one hole"})
    .to_string();
    let seat = seat(vec![summary], vec![prompt]);
    let room = room(false);
    let request = CompileRequest::create(SUMMARY)
        .with_authoring_policy(
            AuthoringPolicy::new("mock/author", 4096, Duration::from_secs(2))
                .with_native(NativeMode::Sketch)
                .with_repairs(2),
        )
        .answer("model", r#""mistral/mistral-small-latest""#);
    let out = compile_with(&request, &seat, &room).await;
    let found = evidence(&out);
    assert_eq!(
        found.len(),
        1,
        "one candidate, no repair from UNKNOWN: {found:#?}"
    );
    assert_eq!(found[0]["outcome"], "unknown");
    assert!(found[0]["behaviour"]["why"].is_string(), "{found:#?}");
    assert!(
        !evidence(&out)
            .iter()
            .any(|entry| entry["outcome"] == "holds"),
        "UNKNOWN is never a pass"
    );
}

/// A run stopped at the room's time bound is a demonstrated failure the repair starts from: the
/// repaired candidate is READY within the original allowance, and both attempts are kept.
#[tokio::test]
async fn a_time_bound_repairs_within_the_original_budget_and_keeps_both_attempts() {
    let seat = seat(vec![sketch()], vec![fills(RAW_COUNT), fills(PAID_COUNT)]);
    let room = Room {
        shown: Mutex::new(Vec::new()),
        stale: false,
        stop_first: true,
    };
    // The allowance E's repair witnesses grant one reopening with (rounds, not reopenings).
    let out = compile_with(&request(2), &seat, &room).await;
    assert_eq!(out.status, CompileStatus::Ready, "{:#?}", out.diagnostics);
    let shown = room.shown.lock().unwrap().clone();
    assert_eq!(shown.len(), 2, "the stopped run and the repaired one");
    assert_eq!(out.candidate.as_deref(), Some(shown[1].as_str()));
    let found = reports(&out);
    assert_eq!(found.len(), 2, "{found:#?}");
    assert_eq!(found[0]["attempt"], "stopped");
    assert_eq!(found[0]["decision"]["code"], "rehearsal_time_bound");
    assert_eq!(found[1]["attempt"], "completed");
    assert_eq!(found[1]["outcome"]["kind"], "passed");
    assert_eq!(
        found[0]["candidate_sha256"],
        nika_compile::surface::sha256(&shown[0])
    );
    let usage = &out.provenance.decision.as_ref().unwrap()["rehearsal"]["usage"];
    assert_eq!(usage["attempts"], 2, "{usage}");
    let receipt = out.provenance.authoring.as_ref().unwrap();
    let repairs = (receipt.context.iter())
        .filter(|call| call["call"] == "sketch-repair")
        .count();
    assert_eq!(
        repairs, 1,
        "one reopened sketch, within the original allowance"
    );
}

/// The same failed candidate again is no progress: the talk stops there, with allowance left,
/// and nothing is READY.
#[tokio::test]
async fn repeating_the_same_failed_candidate_stops_without_spending_every_repair() {
    let (out, seat, room) = compiled(
        3,
        vec![fills(RAW_COUNT), fills(RAW_COUNT), fills(PAID_COUNT)],
    )
    .await;
    assert_ne!(out.status, CompileStatus::Ready, "{:#?}", out.diagnostics);
    assert!(out.candidate.is_none());
    let shown = room.shown.lock().unwrap().clone();
    assert_eq!(
        shown.len(),
        2,
        "the failed candidate ran twice, nothing after"
    );
    assert_eq!(shown[0], shown[1]);
    assert_eq!(
        seat.calls.lock().unwrap().len(),
        4,
        "the sketch, its fills, one reopened sketch, its fills"
    );
    assert!(
        !(seat.calls.lock().unwrap().iter()).any(|call| call.contains("select(.status")),
        "the third answer was never asked for"
    );
    let found = evidence(&out);
    assert_eq!(found.len(), 2, "{found:#?}");
    assert!(found.iter().all(|entry| entry["outcome"] == "defect"));
}
