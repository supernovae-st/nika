// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

//! The Outcome IR battery (spec 13 · W5) — ONE table, parity-tested.
//!
//! - **Table parity** · the engine's `legal()` == the vendored pack's
//!   `canon.yaml` `outcome_transitions` (the SAME table, never a
//!   private copy) + the 4×10 complement refuses.
//! - **One test per ROW** · real mock runs (parse → check → run)
//!   producing each legal `(class, cause)` + its payload laws.
//! - **`trace_format: 2`** · the opening frame names the format; every
//!   terminal task event carries `outcome` and the mini-judge below
//!   (the `outcome_core.py` laws in Rust) accepts every one.
//! - **`.cause` reads** · check ≡ run: the reference passes the ladder
//!   AND resolves at run (terminal-observation pass-set).
//!
//! Hermetic: reads only the VENDORED pack (`nika-pack` is a dev-dep ·
//! the pack is inside the crate) — no `NIKA_SPEC_DIR`, no repo reads.

use std::sync::Arc;

use nika_event::{Event, EventKind};
use nika_kernel::tool_executor::{ToolCall, ToolExecError, ToolExecuteDyn, ToolResult};
use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_runtime::{
    DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, TaskRecord, TaskStatus,
    TerminalCause, VecSink, legal,
};
use nika_types::TraceFormatVersion;
use nika_types::resource::Value as FieldValue;
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;
use serde_json::Value;

// ─── harness (the spec_v2 floor discipline · real chain over mocks) ──────

const ALL_STATUSES: [TaskStatus; 4] = [
    TaskStatus::Success,
    TaskStatus::Failure,
    TaskStatus::Skipped,
    TaskStatus::Cancelled,
];

const ALL_CAUSES: [TerminalCause; 10] = [
    TerminalCause::Normal,
    TerminalCause::Recovered,
    TerminalCause::VerbError,
    TerminalCause::Timeout,
    TerminalCause::RetryExhausted,
    TerminalCause::Gate,
    TerminalCause::ErrorSkip,
    TerminalCause::Upstream,
    TerminalCause::Operator,
    TerminalCause::Budget,
];

fn status_of(wire: &str) -> TaskStatus {
    ALL_STATUSES
        .into_iter()
        .find(|s| s.as_str() == wire)
        .unwrap_or_else(|| panic!("unknown class {wire}"))
}

fn cause_of(wire: &str) -> TerminalCause {
    ALL_CAUSES
        .into_iter()
        .find(|c| c.as_str() == wire)
        .unwrap_or_else(|| panic!("unknown cause {wire}"))
}

async fn run_yaml<T: ToolExecuteDyn>(
    yaml: &str,
    shell: MockShell,
    tools: T,
    provider: MockProvider,
    config: RuntimeConfig,
) -> (RunOutcome, Vec<Event>) {
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(
        report.is_clean(),
        "fixture passes the ladder: {}",
        serde_json::to_string(&report).unwrap_or_default()
    );
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(tools)));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(shell)),
        Arc::clone(&invoke),
        InferVerb::new(registry, "mock/echo"),
        AgentVerb::new(
            Arc::new(provider),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        MockClock::new(),
        config,
    );
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        runtime.run(&wf, &report, &mut stamper, &mut sink),
    )
    .await
    .expect("the run settles")
    .expect("clean run");
    (outcome, sink.into_events())
}

async fn run_simple(yaml: &str, shell: MockShell) -> (RunOutcome, Vec<Event>) {
    run_yaml(
        yaml,
        shell,
        MockToolExecutor::new(),
        MockProvider::new("mock"),
        RuntimeConfig::default(),
    )
    .await
}

fn str_field<'a>(event: &'a Event, key: &str) -> Option<&'a str> {
    event.fields.iter().find_map(|f| match (&f.key, &f.value) {
        (k, FieldValue::String(s)) if k == key => Some(s.as_str()),
        _ => None,
    })
}

fn int_field(event: &Event, key: &str) -> Option<i64> {
    event.fields.iter().find_map(|f| match (&f.key, &f.value) {
        (k, FieldValue::Int(n)) if k == key => Some(*n),
        _ => None,
    })
}

/// The terminal task-event kinds (the settle frames outcome must ride).
const TERMINAL_TASK_KINDS: [EventKind; 5] = [
    EventKind::TaskCompleted,
    EventKind::TaskFailed,
    EventKind::TaskSkipped,
    EventKind::TaskCancelled,
    EventKind::TaskCacheHit,
];

/// The `outcome` field of a task's terminal frame, parsed.
fn outcome_of(events: &[Event], task: &str) -> Value {
    let event = events
        .iter()
        .find(|e| TERMINAL_TASK_KINDS.contains(&e.kind) && str_field(e, "task") == Some(task))
        .unwrap_or_else(|| panic!("no terminal frame for {task}"));
    let text = str_field(event, "outcome")
        .unwrap_or_else(|| panic!("{task}'s terminal frame carries no outcome: {event:?}"));
    serde_json::from_str(text).expect("outcome is one JSON document")
}

/// The reference judge's laws (`conformance/outcome_core.py`) in Rust —
/// validates one parsed `outcome` against the VENDORED pack table.
fn judge(outcome: &Value, table: &nika_pack::OutcomeTransitions) {
    let class = outcome["class"].as_str().expect("class is a string");
    let cause = outcome["cause"].as_str().expect("cause is a string");
    let legal_causes = table.legal.iter().find(|(k, _)| *k == class).map_or_else(
        || panic!("class {class} not a terminal class"),
        |(_, v)| v.as_slice(),
    );
    assert!(
        legal_causes.contains(&cause),
        "({class}, {cause}) · outside the normative table — an engine bug, never a state"
    );
    let payload = outcome["payload"].as_object().expect("payload object");
    let declared = table
        .payload
        .iter()
        .find(|(k, _)| *k == class)
        .map(|(_, v)| v.as_slice())
        .expect("payload law per class");
    let allowed: Vec<&str> = declared.iter().map(|f| f.trim_end_matches('?')).collect();
    let required: Vec<&str> = declared
        .iter()
        .filter(|f| !f.ends_with('?'))
        .copied()
        .collect();
    for key in payload.keys() {
        assert!(
            allowed.contains(&key.as_str()),
            "({class}, {cause}) · undeclared payload field {key} — a new fact is a new CAUSE row"
        );
    }
    for field in &required {
        assert!(
            payload.contains_key(*field),
            "({class}, {cause}) · payload missing required {field}"
        );
    }
    // per-row laws
    match (class, cause) {
        ("success", "recovered") => assert!(
            payload.contains_key("recovered_from"),
            "success(recovered) requires recovered_from"
        ),
        ("success", "normal") => assert!(
            !payload.contains_key("recovered_from"),
            "success(normal) forbids recovered_from"
        ),
        ("skipped", "error_skip") => assert!(
            payload.contains_key("error"),
            "skipped(error_skip) requires the PRESERVED error"
        ),
        ("skipped", "gate") => assert!(
            payload.get("error").is_none_or(Value::is_null),
            "skipped(gate) · error reads defined-null"
        ),
        _ => {}
    }
    if let Some(attempts) = payload.get("attempts") {
        let n = attempts.as_i64().expect("attempts is an integer");
        assert!(n >= 1, "attempts counts every attempt · ≥ 1");
    }
}

fn pack_table() -> nika_pack::OutcomeTransitions {
    nika_pack::outcome_transitions().expect("the vendored pack carries the spec-13 table")
}

// ─── the ONE-table parity gate ────────────────────────────────────────────

/// The engine's `legal()` IS the pack's `outcome_transitions.legal` —
/// exhaustive 4×10 in BOTH directions, plus the class/cause universes
/// and the trace-format binding. The gate of the wave.
#[test]
fn table_parity_with_the_vendored_pack() {
    let table = pack_table();
    // The classes are exactly the four statuses, in canon order — and
    // every pack class resolves to an engine status (panics otherwise).
    assert_eq!(
        table.classes,
        ALL_STATUSES.map(TaskStatus::as_str),
        "the terminal classes are the spec's four, verbatim"
    );
    let resolved: Vec<TaskStatus> = table.classes.iter().map(|c| status_of(c)).collect();
    assert_eq!(resolved, ALL_STATUSES);
    // Every cause named by the pack exists in the engine's enum…
    let pack_causes: Vec<&str> = table
        .legal
        .iter()
        .flat_map(|(_, causes)| causes.iter().copied())
        .collect();
    assert_eq!(pack_causes.len(), 10, "the table is exactly 10 rows");
    for wire in &pack_causes {
        let _ = cause_of(wire); // panics on an unknown cause
    }
    // …and the engine grows no cause the pack does not know.
    for cause in ALL_CAUSES {
        assert!(
            pack_causes.contains(&cause.as_str()),
            "engine cause {} is not in the pack table",
            cause.as_str()
        );
    }
    // THE parity sweep: same verdict on every (class, cause) pair.
    for class in ALL_STATUSES {
        let pack_row = table
            .legal
            .iter()
            .find(|(k, _)| *k == class.as_str())
            .map(|(_, v)| v.as_slice())
            .expect("every class has a row");
        for cause in ALL_CAUSES {
            assert_eq!(
                legal(class, cause),
                pack_row.contains(&cause.as_str()),
                "parity broke on ({}, {}) — the engine table drifted from canon.yaml",
                class.as_str(),
                cause.as_str()
            );
        }
    }
    // The format the table binds to is the ONE the engine emits.
    assert_eq!(
        u32::from(TraceFormatVersion::CURRENT.version),
        table.trace_format,
        "TraceFormatVersion::CURRENT must match outcome_transitions.trace_format"
    );
}

/// The complement refuses: exactly 10 of the 40 pairs admit — every
/// pair outside the table is an engine bug, never a state.
#[test]
fn the_thirty_pair_complement_refuses() {
    let mut admitted = 0usize;
    for class in ALL_STATUSES {
        for cause in ALL_CAUSES {
            if legal(class, cause) {
                admitted += 1;
            }
        }
    }
    assert_eq!(admitted, 10, "10 legal rows · 30 refused");
}

// ─── one test per ROW (real mock runs) ────────────────────────────────────

/// Row 1 · success(normal) — the verb completed. Payload: value ·
/// attempts = 1 · NO `recovered_from`.
#[tokio::test]
async fn row_success_normal() {
    let yaml = "nika: w\npermits: { exec: true }\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n";
    let (outcome, events) = run_simple(yaml, MockShell::new().enqueue_ok("hi\n")).await;
    assert!(outcome.ok);
    assert_eq!(outcome.records["a"].cause, TerminalCause::Normal);
    assert_eq!(outcome.records["a"].attempts, Some(1));
    let o = outcome_of(&events, "a");
    judge(&o, &pack_table());
    assert_eq!(o["class"], "success");
    assert_eq!(o["cause"], "normal");
    assert_eq!(o["payload"]["value"], "hi");
    assert_eq!(o["payload"]["attempts"], 1);
    assert!(o["payload"].get("recovered_from").is_none());
}

/// Row 2 · success(recovered) — `on_error: recover` settled the task;
/// the record keeps the ORIGINAL error as `recovered_from`.
#[tokio::test]
async fn row_success_recovered() {
    let yaml = "nika: w\npermits: { exec: true }\ntasks:\n  a:\n    on_error: { recover: \"fallback\" }\n    exec: { command: [\"boom\"] }\n";
    let (outcome, events) = run_simple(yaml, MockShell::new().enqueue_fail(9, "kaboom")).await;
    assert!(outcome.ok, "the recovery settles the run green");
    let record = &outcome.records["a"];
    assert_eq!(record.status, TaskStatus::Success);
    assert_eq!(record.cause, TerminalCause::Recovered);
    let original = record.recovered_from.as_ref().expect("the original error");
    assert_eq!(outcome.records["a"].attempts, Some(1));
    let o = outcome_of(&events, "a");
    judge(&o, &pack_table());
    assert_eq!(o["cause"], "recovered");
    assert_eq!(o["payload"]["value"], "fallback");
    assert_eq!(
        o["payload"]["recovered_from"]["code"],
        Value::String(original.code.clone()),
        "the payload carries the WHOLE original error"
    );
    assert!(
        o["payload"]["recovered_from"]["message"].is_string(),
        "…message included"
    );
    // The task_recovered prefix frame still rides (D-2026-07-08-N4).
    assert!(events.iter().any(|e| e.kind == EventKind::TaskRecovered));
}

/// Row 2, reached through a FAN-OUT — the same row, the other road.
///
/// This battery says « one test per ROW ». Row 2 is covered, and it was
/// covered only for a bare task: a judge governs the collection it walks,
/// not the row it names. Measured on 0.111.0, a `for_each` whose
/// iterations recover records ·
///
/// ```json
/// "note":    "for_each · 2/5 ok · 3 recovered",
/// "outcome": {"cause":"normal","class":"success",
///             "payload":{"attempts":1,"value":["",null,"",null,null]}}
/// ```
///
/// Three of five checks died and the record says `normal`. Spec 13
/// assigns `success` × `recovered` to exactly this, and closes the table
/// with « Any (class, cause) pair outside this table is an engine bug,
/// never a state ».
///
/// The engine KNEW · it wrote « 3 recovered » into `note`, a human string,
/// while the machine-readable half beside it said `normal`. The
/// per-iteration `recovered_from` was destructured, tested with
/// `is_some()`, and dropped for a counter.
#[tokio::test]
async fn row_success_recovered_through_a_fan_out() {
    let yaml = "nika: w\nconst: { items: [1, 2] }\npermits: { exec: true }\ntasks:\n  a:\n    for_each: { items: \"${{ const.items }}\", fail_fast: false }\n    on_error: { recover: \"fallback\" }\n    exec: { command: [\"boom\"] }\n";
    // DISTINCT errors: the record claims to keep the FIRST, mirroring
    // , and a claim nothing can tell apart is untested.
    let shell = MockShell::new()
        .enqueue_fail(9, "the first one")
        .enqueue_fail(9, "the second one");
    let (outcome, events) = run_simple(yaml, shell).await;
    assert!(outcome.ok, "the recoveries settle the run green");

    let record = &outcome.records["a"];
    assert_eq!(record.status, TaskStatus::Success);
    assert_eq!(
        record.cause,
        TerminalCause::Recovered,
        "a fan-out whose iterations were repaired is NOT `normal` — spec 13 \
         calls that pair an engine bug"
    );
    let original = record
        .recovered_from
        .as_ref()
        .expect("the record keeps the original error (spec 13 §payload)");

    let o = outcome_of(&events, "a");
    judge(&o, &pack_table());
    assert_eq!(o["cause"], "recovered");
    assert_eq!(
        o["payload"]["recovered_from"]["code"],
        Value::String(original.code.clone()),
        "the payload carries the original error, not just a tally in prose"
    );
    assert!(
        original.message.contains("the first one"),
        "the FIRST original is the witness kept, not the last: {}",
        original.message
    );
    assert!(
        events.iter().any(|e| e.kind == EventKind::TaskRecovered),
        "the fan-out owes the same prefix frame the bare task emits"
    );
}

/// The guard · a fan-out where NOTHING was repaired keeps `normal`. A
/// cause that fired on every fan would be its own defect, and would make
/// the row above meaningless.
#[tokio::test]
async fn a_clean_fan_out_stays_normal() {
    let yaml = "nika: w\nconst: { items: [1, 2] }\npermits: { exec: true }\ntasks:\n  a:\n    for_each: { items: \"${{ const.items }}\" }\n    exec: { command: [\"ok\"] }\n";
    let shell = MockShell::new().enqueue_ok("fine\n").enqueue_ok("fine\n");
    let (outcome, events) = run_simple(yaml, shell).await;
    assert!(outcome.ok);
    let record = &outcome.records["a"];
    assert_eq!(record.cause, TerminalCause::Normal);
    assert!(record.recovered_from.is_none());
    let o = outcome_of(&events, "a");
    judge(&o, &pack_table());
    assert_eq!(o["cause"], "normal");
    assert!(o["payload"].get("recovered_from").is_none());
    assert!(
        !events.iter().any(|e| e.kind == EventKind::TaskRecovered),
        "nothing was repaired, so nothing announces a repair"
    );
}

/// Row 3 · `failure(verb_error)` — the verb refused and no retry
/// remained (attempts = 1 when no `retry:`).
#[tokio::test]
async fn row_failure_verb_error() {
    let yaml =
        "nika: w\npermits: { exec: true }\ntasks:\n  a:\n    exec: { command: [\"boom\"] }\n";
    let (outcome, events) = run_simple(yaml, MockShell::new().enqueue_fail(9, "kaboom")).await;
    assert!(!outcome.ok);
    assert_eq!(outcome.records["a"].cause, TerminalCause::VerbError);
    assert_eq!(outcome.records["a"].attempts, Some(1));
    let o = outcome_of(&events, "a");
    judge(&o, &pack_table());
    assert_eq!(o["class"], "failure");
    assert_eq!(o["cause"], "verb_error");
    assert_eq!(o["payload"]["attempts"], 1);
    assert!(o["payload"]["error"]["code"].is_string());
}

/// A tool that never resolves — the timeout budget is the settler.
struct HangingTools;

impl ToolExecuteDyn for HangingTools {
    async fn execute(&self, _call: ToolCall) -> Result<ToolResult, ToolExecError> {
        std::future::pending().await
    }
}

/// Row 4 · failure(timeout) — the task's `timeout:` budget elapsed
/// (NIKA-TIMEOUT-001 · distinguished from the last attempt's failure:
/// the budget RACE settles, not the verb).
#[tokio::test]
async fn row_failure_timeout() {
    let yaml = "nika: w\npermits: { tools: [\"nika:read\"], fs: { read: [\"slow.txt\"] } }\ntasks:\n  a:\n    timeout: \"50ms\"\n    invoke: { tool: \"nika:read\", args: { path: \"slow.txt\" } }\n";
    let (outcome, events) = run_yaml(
        yaml,
        MockShell::new(),
        HangingTools,
        MockProvider::new("mock"),
        RuntimeConfig::default(),
    )
    .await;
    assert!(!outcome.ok);
    assert_eq!(outcome.records["a"].cause, TerminalCause::Timeout);
    let o = outcome_of(&events, "a");
    judge(&o, &pack_table());
    assert_eq!(o["cause"], "timeout");
    assert_eq!(o["payload"]["error"]["code"], "NIKA-TIMEOUT-001");
    assert_eq!(o["payload"]["attempts"], 1);
}

/// Row 5 · `failure(retry_exhausted)` — every attempt failed and the
/// policy admitted no more (attempts counts them all).
#[tokio::test]
async fn row_failure_retry_exhausted() {
    let yaml = "nika: w\nmodel: mock/echo\ntasks:\n  a:\n    retry: { max_attempts: 2, backoff_ms: 1, backoff_strategy: fixed, jitter: false }\n    agent:\n      prompt: \"try\"\n";
    let provider = MockProvider::new("mock")
        .enqueue_error(nika_kernel::provider::ProviderError::RateLimited {
            retry_after_ms: Some(1),
        })
        .enqueue_error(nika_kernel::provider::ProviderError::RateLimited {
            retry_after_ms: Some(1),
        });
    let (outcome, events) = run_yaml(
        yaml,
        MockShell::new(),
        MockToolExecutor::new(),
        provider,
        RuntimeConfig::default(),
    )
    .await;
    assert!(!outcome.ok);
    assert_eq!(outcome.records["a"].cause, TerminalCause::RetryExhausted);
    assert_eq!(outcome.records["a"].attempts, Some(2));
    let o = outcome_of(&events, "a");
    judge(&o, &pack_table());
    assert_eq!(o["cause"], "retry_exhausted");
    assert_eq!(o["payload"]["attempts"], 2);
}

/// Row 6 · skipped(gate) — the `when:` gate evaluated false: a
/// decision, not a defect; the payload carries NO error (defined-null).
#[tokio::test]
async fn row_skipped_gate() {
    let yaml = "nika: w\npermits: { exec: true }\nconst: { go: \"no\" }\ntasks:\n  a:\n    when: ${{ const.go == 'yes' }}\n    exec: { command: [\"echo\"] }\n";
    let (outcome, events) = run_simple(yaml, MockShell::new()).await;
    assert!(outcome.ok, "a decision-skip keeps the run green");
    assert_eq!(outcome.records["a"].status, TaskStatus::Skipped);
    assert_eq!(outcome.records["a"].cause, TerminalCause::Gate);
    assert_eq!(outcome.records["a"].attempts, None, "no attempt was made");
    let o = outcome_of(&events, "a");
    judge(&o, &pack_table());
    assert_eq!(o["cause"], "gate");
    assert!(
        o["payload"].get("error").is_none(),
        "a decision-skip's error reads defined-null"
    );
}

/// Row 7 · `skipped(error_skip)` — `on_error: skip` fired; the PRESERVED
/// error rides the payload (it already rides `.error` — same record).
#[tokio::test]
async fn row_skipped_error_skip() {
    let yaml = "nika: w\npermits: { exec: true }\ntasks:\n  a:\n    on_error: { skip: true }\n    exec: { command: [\"boom\"] }\n";
    let (outcome, events) = run_simple(yaml, MockShell::new().enqueue_fail(9, "kaboom")).await;
    assert!(outcome.ok, "a skip absorbs the failure");
    let record = &outcome.records["a"];
    assert_eq!(record.status, TaskStatus::Skipped);
    assert_eq!(record.cause, TerminalCause::ErrorSkip);
    let preserved = record.error.as_ref().expect("the error stays readable");
    let o = outcome_of(&events, "a");
    judge(&o, &pack_table());
    assert_eq!(o["cause"], "error_skip");
    assert_eq!(
        o["payload"]["error"]["code"],
        Value::String(preserved.code.clone()),
        "the PRESERVED error rides the payload"
    );
}

/// Row 8 · cancelled(upstream) — the default gate became unsatisfiable
/// (an upstream failure propagated) · reason spells the cause.
#[tokio::test]
async fn row_cancelled_upstream() {
    let yaml = "nika: w\npermits: { exec: true }\ntasks:\n  a:\n    exec: { command: [\"boom\"] }\n  b:\n    with: { prev: \"${{ tasks.a.output }}\" }\n    exec: { command: [\"echo\", \"${{ with.prev }}\"] }\n";
    let (outcome, events) = run_simple(yaml, MockShell::new().enqueue_fail(9, "kaboom")).await;
    assert!(!outcome.ok);
    assert_eq!(outcome.records["b"].status, TaskStatus::Cancelled);
    assert_eq!(outcome.records["b"].cause, TerminalCause::Upstream);
    let o = outcome_of(&events, "b");
    judge(&o, &pack_table());
    assert_eq!(o["class"], "cancelled");
    assert_eq!(o["cause"], "upstream");
    assert_eq!(o["payload"]["reason"], "upstream");
}

/// Row 9 · cancelled(budget) — the run crossed `--max-cost-usd`; this
/// task was UNSTARTED when the cap hit (NIKA-RUN-1704 semantics).
#[tokio::test]
async fn row_cancelled_budget() {
    let yaml = "nika: w\npermits: { tools: [\"nika:jq\"] }\ntasks:\n  a:\n    invoke: { tool: \"nika:jq\", args: { input: { x: 1 }, expression: \".\" } }\n  b:\n    with: { prev: \"${{ tasks.a.output }}\" }\n    invoke: { tool: \"nika:jq\", args: { input: \"${{ with.prev }}\", expression: \".\" } }\n";
    let tools = MockToolExecutor::new().enqueue_ok(
        ToolResult::success("c1", "done")
            .with_structured(serde_json::json!({ "cost_usd": 0.06, "ok": true })),
    );
    let (outcome, events) = run_yaml(
        yaml,
        MockShell::new(),
        tools,
        MockProvider::new("mock"),
        RuntimeConfig::default().with_max_cost_usd(Some(0.05)),
    )
    .await;
    assert!(!outcome.ok);
    assert!(outcome.budget_exceeded);
    assert_eq!(outcome.records["b"].status, TaskStatus::Cancelled);
    assert_eq!(outcome.records["b"].cause, TerminalCause::Budget);
    let o = outcome_of(&events, "b");
    judge(&o, &pack_table());
    assert_eq!(o["cause"], "budget");
    assert_eq!(o["payload"]["reason"], "budget");
}

/// Row 10 · cancelled(operator) — the run cancelled from outside
/// (`NIKA-CANCEL-001`). The engine has NO operator-cancellation seam
/// today (no runtime settle path produces it — verified: nothing emits
/// `workflow_cancelled`), so this row is exercised at the record →
/// outcome level through the SAME machinery every live settle uses;
/// the seam inherits a ready-made row the day it lands.
#[test]
fn row_cancelled_operator_record_level() {
    let table = pack_table();
    assert!(legal(TaskStatus::Cancelled, TerminalCause::Operator));
    let record = TaskRecord::unran(TaskStatus::Cancelled, TerminalCause::Operator);
    assert_eq!(record.cause, TerminalCause::Operator);
    // The record's cause reads exactly like every other row's.
    assert_eq!(
        record.field("cause").expect("cause resolves"),
        Value::String("operator".to_owned())
    );
    // …and its outcome projection passes the pack judge like a live one
    // (built through `TaskRecord`, the one carrier the settle path uses).
    let o: Value = serde_json::from_str(&format!(
        "{{\"class\":\"cancelled\",\"cause\":\"operator\",\"payload\":{{\"reason\":\"{}\"}}}}",
        record.cause.as_str()
    ))
    .expect("well-formed");
    judge(&o, &table);
}

// ─── trace_format: 2 · outcome on EVERY terminal task event ─────────────

/// One run mixing every settle family the runtime can produce live —
/// the opening frame carries `trace_format: 2` and EVERY terminal task
/// frame carries an outcome the pack judge accepts.
#[tokio::test]
async fn trace_format_2_and_outcome_on_every_terminal() {
    let yaml = r#"
nika: mixed
permits: { exec: true }
const: { go: "no" }
tasks:
  ok:
    exec: { command: ["echo", "fine"] }
  gated:
    when: ${{ const.go == 'yes' }}
    exec: { command: ["echo", "never"] }
  dies:
    exec: { command: ["boom"] }
  absorbed:
    on_error: { skip: true }
    exec: { command: ["boom2"] }
  downstream:
    with: { prev: "${{ tasks.dies.output }}" }
    exec: { command: ["echo", "${{ with.prev }}"] }
"#;
    let shell = MockShell::new()
        .enqueue_ok("fine\n")
        .enqueue_fail(9, "kaboom")
        .enqueue_fail(9, "kaboom2");
    let (outcome, events) = run_simple(yaml, shell).await;
    assert!(!outcome.ok);

    // The header names the format — ONE source, the graph_format way.
    let started = events
        .iter()
        .find(|e| e.kind == EventKind::WorkflowStarted)
        .expect("opening frame");
    assert_eq!(
        int_field(started, "trace_format"),
        Some(i64::from(TraceFormatVersion::CURRENT.version)),
        "the trace header carries trace_format: 2"
    );
    assert_eq!(int_field(started, "trace_format"), Some(2));

    // Every terminal task frame carries a judged outcome.
    let table = pack_table();
    let mut terminals = 0usize;
    for event in &events {
        if !TERMINAL_TASK_KINDS.contains(&event.kind) {
            continue;
        }
        terminals += 1;
        let task = str_field(event, "task").expect("task field");
        let text = str_field(event, "outcome")
            .unwrap_or_else(|| panic!("terminal frame of {task} carries no outcome"));
        let o: Value = serde_json::from_str(text).expect("outcome parses");
        judge(&o, &table);
        // The frame's outcome and the record agree — one truth.
        let record = &outcome.records[task];
        assert_eq!(o["class"], record.status.as_str());
        assert_eq!(o["cause"], record.cause.as_str());
    }
    assert_eq!(terminals, 5, "every task settled with a terminal frame");

    // The five families hit the five causes this run can produce.
    assert_eq!(outcome_of(&events, "ok")["cause"], "normal");
    assert_eq!(outcome_of(&events, "gated")["cause"], "gate");
    assert_eq!(outcome_of(&events, "dies")["cause"], "verb_error");
    assert_eq!(outcome_of(&events, "absorbed")["cause"], "error_skip");
    assert_eq!(outcome_of(&events, "downstream")["cause"], "upstream");
}

/// The event stream stays deterministic with the outcome axis aboard —
/// two identical runs give byte-identical streams (the W2 law holds).
#[tokio::test]
async fn outcome_fields_are_replay_stable() {
    let yaml = "nika: w\npermits: { exec: true }\ntasks:\n  a:\n    on_error: { recover: \"fallback\" }\n    exec: { command: [\"boom\"] }\n";
    let (_, first) = run_simple(yaml, MockShell::new().enqueue_fail(9, "kaboom")).await;
    let (_, again) = run_simple(yaml, MockShell::new().enqueue_fail(9, "kaboom")).await;
    assert_eq!(first, again, "outcome fields must not break determinism");
}

// ─── `.cause` · the terminal-observation read (check ≡ run) ──────────────

/// Spec 13 §one obvious way — branching on WHY reads `${{ tasks.x.cause }}`:
/// the reference passes the ladder (terminal-observation pass-set — a
/// FAILED producer still admits) and resolves at run; the downstream
/// `when:` branches on it.
#[tokio::test]
async fn cause_reads_both_sides_when_branches_on_timeout() {
    let yaml = r#"
nika: triage
permits: { exec: true, tools: ["nika:read"], fs: { read: ["slow.txt"] } }
tasks:
  flaky:
    timeout: "50ms"
    invoke: { tool: "nika:read", args: { path: "slow.txt" } }
  on_timeout:
    with: { why: "${{ tasks.flaky.cause }}" }
    when: ${{ with.why == 'timeout' }}
    exec: { command: ["echo", "took too long"] }
  on_verb_error:
    with: { why: "${{ tasks.flaky.cause }}" }
    when: ${{ with.why == 'verb_error' }}
    exec: { command: ["echo", "the verb refused"] }
"#;
    let (outcome, events) = run_yaml(
        yaml,
        MockShell::new().enqueue_ok("took too long\n"),
        HangingTools,
        MockProvider::new("mock"),
        RuntimeConfig::default(),
    )
    .await;
    // check ≡ run: the ladder admitted the reference (run_yaml asserts a
    // clean report) AND the run resolved it.
    assert_eq!(outcome.records["flaky"].cause, TerminalCause::Timeout);
    // The timeout branch OPENED (when true → the task ran)…
    assert_eq!(outcome.records["on_timeout"].status, TaskStatus::Success);
    assert_eq!(outcome_of(&events, "on_timeout")["cause"], "normal");
    // …and the verb_error branch CLOSED (when false → skipped/gate).
    assert_eq!(outcome.records["on_verb_error"].status, TaskStatus::Skipped);
    assert_eq!(outcome.records["on_verb_error"].cause, TerminalCause::Gate);
}

/// `${{ tasks.x.cause }}` renders in a plain `with:` string position —
/// the wire word, exactly (spec 13 · never a string match on a message).
#[tokio::test]
async fn cause_renders_the_wire_word_in_templates() {
    let yaml = "nika: w\npermits: { exec: true }\ntasks:\n  a:\n    on_error: { skip: true }\n    exec: { command: [\"boom\"] }\n  b:\n    with: { why: \"${{ tasks.a.cause }}\" }\n    exec: { command: [\"echo\", \"${{ with.why }}\"] }\n";
    let shell = MockShell::new()
        .enqueue_fail(9, "kaboom")
        .enqueue_ok("error_skip\n");
    let (outcome, _) = run_simple(yaml, shell).await;
    assert!(outcome.ok);
    assert_eq!(outcome.records["a"].cause, TerminalCause::ErrorSkip);
    assert_eq!(
        outcome.records["b"].output,
        Value::String("error_skip".to_owned()),
        "the template rendered the cause wire word"
    );
}
