// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What the real observed room shows of a counting workflow today: nothing. Two exposed rows,
//! one paid; a graph with a real filter whose count reads the unfiltered rows (it writes 2), and
//! the same graph with the count reconnected to the filter (it writes 1). The room screens every
//! `nika:jq` step before any room exists, so neither graph runs: both reports are the same
//! refusal, and the compile entry records the behaviour as UNKNOWN without a repair. This is the
//! gap a bounded jq evaluation in the room closes; it is witnessed here, not claimed closed. The
//! author is a scripted provider double (no paid model); the room and its runtime are real; the
//! expected count is computed by this file from the rows, never by the product.
#![allow(clippy::unwrap_used, clippy::expect_used)]
mod room_support;

use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, ResponseFormat,
    StopReason, TokenUsage,
};
use nika_onboard::compile::rehearse::{RehearsalFuture, RehearsalReport, Rehearse};
use nika_onboard::compile::room::ObservedRoom;
use nika_onboard::compile::{
    AuthoringPolicy, Cognition, CompileOutcome, CompileRequest, CompileStatus, NativeMode,
    compile_with_cognition_rehearsed,
};
use serde_json::{Value, json};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

const ROOM: &str = concat!(
    module_path!(),
    "::the_room_cannot_tell_a_disconnected_filter_from_a_connected_one"
);
const COMPILE: &str = concat!(
    module_path!(),
    "::a_wrong_count_the_room_cannot_run_is_unknown_and_never_repaired"
);

/// The two exposed rows, one paid.
const ROWS: &str =
    r#"[{"id":1,"amount_usd":5,"status":"paid"},{"id":2,"amount_usd":20,"status":"late"}]"#;
const INPUT: &str = "data/input.json";
const RESULT: &str = "out/result.json";
const FILTER: &str = r#".rows | fromjson | map(select(.status == "paid"))"#;
const RAW_COUNT: &str = ".rows | fromjson | {count: length}";
const FILTERED_COUNT: &str = ".rows | {count: length}";

/// The count this file expects, from the rows alone.
fn expected_count() -> usize {
    let rows: Vec<Value> = serde_json::from_str(ROWS).unwrap();
    rows.iter().filter(|row| row["status"] == "paid").count()
}

/// Read, filter, count, write. `count_from` names the task the count reads; `count` is its
/// program. The filter task exists in both graphs.
fn graph(world: &room_support::World, count_from: &str, count: &str) -> String {
    let (input, result) = (world.path(INPUT), world.path(RESULT));
    format!(
        r#"nika: paid-count
permits:
  tools: ["nika:read", "nika:jq", "nika:write"]
  fs:
    read: ["{input}"]
    write: ["{result}"]
tasks:
  read_input:
    invoke:
      tool: "nika:read"
      args: {{ path: "{input}" }}
  filter:
    with: {{ rows: "${{{{ tasks.read_input.output }}}}" }}
    invoke:
      tool: "nika:jq"
      args:
        input: {{ rows: "${{{{ with.rows }}}}" }}
        expression: '{FILTER}'
  count:
    with: {{ rows: "${{{{ tasks.{count_from}.output }}}}" }}
    invoke:
      tool: "nika:jq"
      args:
        input: {{ rows: "${{{{ with.rows }}}}" }}
        expression: '{count}'
  write_result:
    with: {{ content: "${{{{ tasks.count.output }}}}" }}
    invoke:
      tool: "nika:write"
      args: {{ path: "{result}", content: "${{{{ with.content }}}}", overwrite: true, create_dirs: true }}
"#
    )
}

fn world() -> room_support::World {
    room_support::World::new(&[(INPUT, ROWS)])
}

/// Both graphs, the disconnected filter first. Each is admitted by the room's own door: a graph
/// the door refuses would make this file harness-invalid, never a finding.
fn graphs(world: &room_support::World) -> [String; 2] {
    let both = [
        graph(world, "read_input", RAW_COUNT),
        graph(world, "filter", FILTERED_COUNT),
    ];
    for source in &both {
        room_support::door_admits(source).expect("HARNESS_INVALID: the door admits the graph");
    }
    both
}

/// What a report shows apart from the identity of its candidate.
fn shape(report: &RehearsalReport) -> Value {
    json!({
        "outcome": format!("{:?}", report.outcome),
        "attempt": format!("{:?}", report.attempt),
        "refusal": format!("{:?}", report.observation.refusal),
        "finals": report.observation.finals.len(),
        "written": report.observation.ledger.written.len(),
        "prepared": report.room.prepared,
        "cleaned": report.room.cleaned,
    })
}

#[tokio::test]
async fn the_room_cannot_tell_a_disconnected_filter_from_a_connected_one() {
    assert_eq!(expected_count(), 1);
    let world = world();
    let before = world.files();
    let room = world.room();
    let inputs = vec![world.path(INPUT)];
    let mut reports = Vec::new();
    for (at, candidate) in graphs(&world).iter().enumerate() {
        let subrun = format!("graph-{at}");
        let report = room_support::rehearsed(ROOM, &subrun, &room, candidate, &inputs, &[]).await;
        assert!(room_support::bound_to(&report, candidate), "{report:#?}");
        // Screened before any room: no run, no room, no output read back, no effect.
        assert!(
            room_support::refused_before_any_room(&report, "nika:jq"),
            "{report:#?}"
        );
        assert!(report.observation.finals.is_empty(), "{report:#?}");
        reports.push(report);
    }
    // The count-2 graph and the count-1 graph leave the same evidence: the room cannot
    // discriminate them, so no oracle over their output can be applied.
    assert_eq!(shape(&reports[0]), shape(&reports[1]));
    assert_ne!(reports[0].candidate_sha256, reports[1].candidate_sha256);
    assert_eq!(
        world.files(),
        before,
        "the project and scratch parent are unchanged"
    );
}

/// The author's scripted answers, in order, to every call that is not a judge's closed choice;
/// a judge is approved. It records what each call asked for.
struct Author {
    answers: Vec<String>,
    asked: Mutex<Vec<&'static str>>,
}

impl ProviderInferDyn for Author {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        let schema = match &request.response_format {
            ResponseFormat::JsonSchema(schema) => schema.clone(),
            _ => Value::Null,
        };
        let approved = ["faithful", "carried"].into_iter().find(|key| {
            (schema["properties"]["choice"]["enum"].as_array())
                .is_some_and(|keys| keys.iter().any(|value| value == *key))
        });
        let mut asked = self.asked.lock().unwrap();
        let answer = if let Some(choice) = approved {
            asked.push("judge");
            json!({"choice": choice}).to_string()
        } else {
            let kind = if schema["properties"].get("fills").is_some() {
                "fills"
            } else {
                "sketch"
            };
            asked.push(kind);
            let at = asked.iter().filter(|k| **k != "judge").count() - 1;
            self.answers.get(at).cloned().unwrap_or_default()
        };
        Ok(InferResponse::new(
            vec![ContentBlock::Text { text: answer }],
            TokenUsage::new(1, 1),
            StopReason::EndTurn,
        ))
    }
}

/// The real room, each call recorded through the shared receipt helper before any assertion.
struct RecordedRoom {
    room: ObservedRoom,
    calls: AtomicUsize,
}

impl Rehearse for RecordedRoom {
    fn bound(&self) -> Duration {
        self.room.bound()
    }
    fn rehearse<'a>(&'a self, candidate: &'a str, inputs: &'a [String]) -> RehearsalFuture<'a> {
        self.rehearse_reading(candidate, inputs, &[])
    }
    fn rehearse_reading<'a>(
        &'a self,
        candidate: &'a str,
        inputs: &'a [String],
        targets: &'a [String],
    ) -> RehearsalFuture<'a> {
        Box::pin(async move {
            let at = self.calls.fetch_add(1, Ordering::SeqCst);
            let subrun = format!("author-{at}");
            room_support::rehearsed(COMPILE, &subrun, &self.room, candidate, inputs, targets).await
        })
    }
}

/// The disconnected-filter graph as a sketch: the count reads the read, not the filter.
fn sketch(world: &room_support::World) -> String {
    let task = |id: &str, tool: &str, extra: Value| {
        let mut t = json!({"id": id, "verb": "invoke", "tool": tool, "purpose": id});
        for (k, v) in extra.as_object().unwrap() {
            t[k] = v.clone();
        }
        t
    };
    json!({"name": "paid-count", "tasks": [
        task("read_input", "nika:read", json!({"reads": [world.path(INPUT)]})),
        task("filter", "nika:jq", json!({"with": [{"name": "rows", "from": "read_input"}]})),
        task("count", "nika:jq", json!({"with": [{"name": "rows", "from": "read_input"}]})),
        task("write_result", "nika:write", json!({"writes": [world.path(RESULT)],
            "with": [{"name": "text", "from": "count"}]})),
    ], "questions": [], "gaps": [], "notes": "read, filter, count, write"})
    .to_string()
}

/// The sketch's programs: the compiler hands each jq step the text its source returns, so both
/// parse it first, and the count reads the unfiltered rows.
fn fills() -> String {
    json!({"fills": [
        {"task": "filter", "field": "expression",
         "value": "fromjson | map(select(.status == \"paid\"))"},
        {"task": "count", "field": "expression", "value": "fromjson | {count: length}"},
    ], "notes": "two holes"})
    .to_string()
}

/// The evidence entries the door journalled, in order.
fn evidence(out: &CompileOutcome) -> Vec<Value> {
    let rounds = &out.provenance.decision.as_ref().unwrap()["native"]["rounds"];
    (rounds.as_array().into_iter().flatten())
        .filter_map(|round| round.get("evidence").cloned())
        .collect()
}

#[tokio::test]
async fn a_wrong_count_the_room_cannot_run_is_unknown_and_never_repaired() {
    let world = world();
    let intent = format!(
        "read {}, count the rows where status is paid, write the count to {}",
        world.path(INPUT),
        world.path(RESULT)
    );
    // The request states an obligation the behavioural judge supports: UNKNOWN below comes from
    // the missing run, not from an unsupported request.
    let contract = nika_compile_fidelity::behavior::contract_of_request(
        &intent,
        &std::collections::BTreeMap::default(),
    );
    assert!(
        !contract.obligations.is_empty(),
        "HARNESS_INVALID: {contract:?}"
    );
    let before = world.files();
    let author = Author {
        answers: vec![sketch(&world), fills()],
        asked: Mutex::new(Vec::new()),
    };
    let room = RecordedRoom {
        room: world.room(),
        calls: AtomicUsize::new(0),
    };
    let request = CompileRequest::create(intent).with_authoring_policy(
        AuthoringPolicy::new("mock/author", 4096, Duration::from_secs(2))
            .with_native(NativeMode::Sketch)
            .with_repairs(2),
    );
    let cognition = Cognition {
        provider: Some(&author),
        seat: None,
    };
    let out = compile_with_cognition_rehearsed(&request, cognition, Some(&room))
        .await
        .unwrap();
    assert_eq!(
        world.files(),
        before,
        "the project and scratch parent are unchanged"
    );
    let decision = out.provenance.decision.as_ref().unwrap();
    let reports = decision["rehearsal"]["reports"].as_array().unwrap();
    assert!(!reports.is_empty(), "{decision:#}");
    for report in reports {
        assert_eq!(report["outcome"]["kind"], "not_run", "{report:#}");
        assert_eq!(report["room"]["prepared"], false, "{report:#}");
    }
    // One candidate, never executed: its behaviour is UNKNOWN, and no repair follows from it.
    let found = evidence(&out);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0]["outcome"], "unknown", "{found:#?}");
    // The sketch, its fills and the whole-request judge: no reopened graph, no repaired fill.
    let asked = author.asked.lock().unwrap().clone();
    assert_eq!(asked, ["sketch", "fills", "judge"]);
    // One host call: the final barrier reused the report of the same bytes.
    assert_eq!(room.calls.load(Ordering::SeqCst), 1);
    assert_eq!(reports.len(), 1, "{decision:#}");
    // THE GAP, as observed today: the count-2 graph (its count reads the unfiltered rows) is
    // READY although no room ran it. A bounded jq evaluation in the room turns this into a run
    // the request's contract can judge; until then this assertion pins what a user receives.
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    // It holds the filter, and nothing reads the filter's output: the filter is disconnected.
    let candidate = out.candidate.as_deref().unwrap();
    assert!(candidate.contains("select(.status"), "{candidate}");
    assert!(!candidate.contains("tasks.filter.output"), "{candidate}");
}
