// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]

//! The v2 spec-parity battery — bounded intra-wave concurrency with
//! ordered settlement (determinism THEOREMS made tests) + the full
//! task pipeline of spec 03/05: gates · records · retry · timeout ·
//! `on_error:` · `for_each:`.
//! `on_finally:` lives in `on_finally.rs` (the 1500-LOC ratchet).
//!
//! Every test runs the REAL parse → check → run chain over mock seams
//! (the floor discipline) — no hand-built reports, no played layers.

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use nika_check::CheckReport;
use nika_check::check;
use nika_event::{Event, EventKind};
use nika_kernel::provider::ProviderError;
use nika_kernel::tool_executor::{ToolCall, ToolExecError, ToolExecuteDyn, ToolResult};
use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_runtime::{DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, TaskStatus, VecSink};
use nika_schema::raw::RawWorkflow;
use nika_schema::{FileId, ParseMode, parse};
use nika_types::resource::Value as FieldValue;
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;

// ─── harness ─────────────────────────────────────────────────────────────

fn parse_and_check(yaml: &str) -> (RawWorkflow, CheckReport) {
    let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
    let report = check(&wf);
    (wf, report)
}

/// Build a runtime over an ARBITRARY tool seam (the battery's custom
/// executors) + optional config.
fn runtime_with_tools<T: ToolExecuteDyn>(
    shell: MockShell,
    tools: T,
    provider: MockProvider,
    config: RuntimeConfig,
) -> Runtime<
    MockShell,
    T,
    nika_providers::NoHttp,
    MockProvider,
    MockToolDefinitionProvider,
    MockClock,
> {
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(tools)));
    Runtime::new(
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
    )
}

async fn run_to_events(
    yaml: &str,
    shell: MockShell,
    tools: MockToolExecutor,
    provider: MockProvider,
    config: RuntimeConfig,
) -> (RunOutcome, Vec<Event>) {
    let (wf, report) = parse_and_check(yaml);
    assert!(report.is_clean(), "fixture passes the ladder");
    let runtime = runtime_with_tools(shell, tools, provider, config);
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("clean run");
    (outcome, sink.into_events())
}

fn field<'a>(event: &'a Event, key: &str) -> Option<&'a FieldValue> {
    event.fields.iter().find(|f| f.key == key).map(|f| &f.value)
}

fn str_field<'a>(event: &'a Event, key: &str) -> Option<&'a str> {
    match field(event, key) {
        Some(FieldValue::String(s)) => Some(s.as_str()),
        _ => None,
    }
}

fn int_field(event: &Event, key: &str) -> Option<i64> {
    match field(event, key) {
        Some(FieldValue::Int(n)) => Some(*n),
        _ => None,
    }
}

fn output_str(outcome: &RunOutcome, task: &str) -> String {
    match &outcome.records[task].output {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

// ─── 1 · cap-equivalence (the determinism theorem as a test) ─────────────

/// Same workflow · caps 1, 2, 8 and wave-width — the event stream is
/// BYTE-IDENTICAL (ordered settlement makes the cap unobservable ·
/// Blelloch/Calvin pattern · spec §3.1).
#[tokio::test]
async fn cap_equivalence_byte_identical_streams() {
    let yaml = r#"
nika: cap-eq
permits: { exec: true }
const: { publish: "no" }
tasks:
  a:
    exec: { command: ["step", "a"] }
  b:
    exec: { command: ["step", "b"] }
  c:
    exec: { command: ["step", "c"] }
  join:
    with:
      a: "${{ tasks.a.output }}"
      b: "${{ tasks.b.output }}"
      c: "${{ tasks.c.output }}"
    exec: { command: ["join", "${{ with.a }}", "${{ with.b }}", "${{ with.c }}"] }
  gated:
    after: { join: success }
    when: ${{ const.publish == 'yes' }}
    exec: { command: ["echo", "never"] }
"#;
    let mut streams: Vec<Vec<Event>> = Vec::new();
    for cap in [Some(1), Some(2), Some(8), None] {
        let shell = MockShell::new()
            .enqueue_ok("ra\n")
            .enqueue_ok("rb\n")
            .enqueue_ok("rc\n")
            .enqueue_ok("joined\n");
        let config = RuntimeConfig::new(cap.and_then(NonZeroUsize::new), 0);
        let (outcome, events) = run_to_events(
            yaml,
            shell,
            MockToolExecutor::new(),
            MockProvider::new("mock"),
            config,
        )
        .await;
        assert!(outcome.ok, "cap {cap:?} completes");
        streams.push(events);
    }
    let first = &streams[0];
    for (i, other) in streams.iter().enumerate().skip(1) {
        assert_eq!(
            first, other,
            "stream {i} diverged — the cap leaked into the contract"
        );
    }
}

// ─── 2 · true concurrency (the handshake proof) ──────────────────────────

/// Two same-wave tools each wait for the OTHER to arrive — completion
/// is only possible when BOTH are in flight simultaneously (a
/// cooperative rendezvous over `yield_now` · no tokio sync feature).
/// A sequential executor spins forever on the first arrival — the
/// outer timeout is the deadlock detector.
struct BarrierTools {
    arrived: AtomicUsize,
    parties: usize,
}

impl BarrierTools {
    fn new(parties: usize) -> Self {
        Self {
            arrived: AtomicUsize::new(0),
            parties,
        }
    }
}

impl ToolExecuteDyn for BarrierTools {
    async fn execute(&self, call: ToolCall) -> Result<ToolResult, ToolExecError> {
        self.arrived.fetch_add(1, Ordering::SeqCst);
        let mut spins = 0_u32;
        while self.arrived.load(Ordering::SeqCst) < self.parties {
            // Counted bailout: under broken concurrency (a mutant that
            // serializes dispatch) the rendezvous can never meet — die
            // by ASSERTION in milliseconds, not by spinning the full
            // 5s outer guard into a cargo-mutants timeout (the guard
            // stays as the belt; this is the suspenders).
            spins += 1;
            assert!(
                spins < 100_000,
                "rendezvous never met — dispatch is not concurrent"
            );
            tokio::task::yield_now().await;
        }
        Ok(ToolResult::success(call.id.as_str(), "met"))
    }
}

#[tokio::test]
async fn same_wave_tasks_truly_run_concurrently() {
    let yaml = r#"
nika: handshake
permits: { tools: ["nika:read"], fs: { read: ["left.txt", "right.txt"] } }
tasks:
  left:
    invoke: { tool: "nika:read", args: { path: "left.txt" } }
  right:
    invoke: { tool: "nika:read", args: { path: "right.txt" } }
"#;
    let (wf, report) = parse_and_check(yaml);
    assert!(report.is_clean());
    let runtime = runtime_with_tools(
        MockShell::new(),
        BarrierTools::new(2),
        MockProvider::new("mock"),
        RuntimeConfig::default(), // wave-width · both in flight
    );
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        runtime.run(&wf, &report, &mut stamper, &mut sink),
    )
    .await
    .expect("the handshake met — sequential execution would deadlock here")
    .expect("clean run");
    assert!(outcome.ok);
    assert_eq!(output_str(&outcome, "left"), "met");
    assert_eq!(output_str(&outcome, "right"), "met");
}

// ─── 3 · in-flight drain (spec 05 §workflow-level) ───────────────────────

/// A sibling failure never aborts a running wave member — both settle.
#[tokio::test]
async fn sibling_failure_drains_the_wave() {
    let yaml = r#"
nika: drain
permits: { exec: true }
tasks:
  dies:
    exec: { command: ["boom"] }
  survives:
    exec: { command: ["fine"] }
"#;
    let shell = MockShell::new()
        .enqueue_fail(9, "kaboom")
        .enqueue_ok("alive\n");
    let (outcome, events) = run_to_events(
        yaml,
        shell,
        MockToolExecutor::new(),
        MockProvider::new("mock"),
        RuntimeConfig::default(),
    )
    .await;
    assert!(!outcome.ok, "the failure decides the verdict");
    assert_eq!(outcome.records["dies"].status, TaskStatus::Failure);
    assert_eq!(outcome.records["survives"].status, TaskStatus::Success);
    assert_eq!(output_str(&outcome, "survives"), "alive");
    // Both terminal frames are in the stream (no silent abort).
    assert!(events.iter().any(|e| e.kind == EventKind::TaskFailed));
    assert!(events.iter().any(|e| e.kind == EventKind::TaskCompleted));
}

// ─── 4 · the always-pattern (spec 05 · explicit when over a dead dep) ────

#[tokio::test]
async fn always_pattern_runs_after_upstream_failure() {
    let yaml = r#"
nika: always
permits: { exec: true }
tasks:
  build:
    exec: { command: ["make"] }
  deploy:
    after: { build: success }
    exec: { command: ["deploy"] }
  notify:
    after: { build: terminal }
    exec: { command: ["notify", "team"] }
"#;
    let shell = MockShell::new()
        .enqueue_fail(1, "compile error")
        .enqueue_ok("notified\n");
    let (outcome, events) = run_to_events(
        yaml,
        shell,
        MockToolExecutor::new(),
        MockProvider::new("mock"),
        RuntimeConfig::default(),
    )
    .await;

    // deploy (default gate) cancels · notify (when: true) RUNS · the
    // workflow verdict stays failure (spec 05 §workflow-level).
    assert!(!outcome.ok, "the unrecovered failure decides the verdict");
    assert_eq!(outcome.records["deploy"].status, TaskStatus::Cancelled);
    assert_eq!(outcome.records["notify"].status, TaskStatus::Success);
    assert_eq!(output_str(&outcome, "notify"), "notified");
    let cancelled: Vec<&str> = events
        .iter()
        .filter(|e| e.kind == EventKind::TaskCancelled)
        .filter_map(|e| str_field(e, "task"))
        .collect();
    assert_eq!(cancelled, ["deploy"]);
}

// ─── 5 · retry (spec 05 · transient via the agent lane) ─────────────────

/// Fan-out iterations ride DISTINCT jitter streams — two iterations
/// retrying the same upstream must NOT synchronize their backoff
/// (anti-thundering-herd applies WITHIN a `for_each` · Brooker 2015 ·
/// the `task[index]` salt) · and the delays stay replay-stable.
#[tokio::test]
async fn fan_out_iterations_jitter_on_distinct_streams() {
    let yaml = r#"
nika: herd
model: mock/echo
const:
  items: ["x", "y"]
tasks:
  flaky_fan:
    for_each: { items: "${{ const.items }}", max_parallel: 1 }
    retry: { max_attempts: 2, backoff_ms: 10000, backoff_strategy: fixed, jitter: true }
    agent:
      prompt: "ask ${{ item }}"
"#;
    // FIFO under max_parallel:1 · iter0 err→ok · iter1 err→ok.
    let make_provider = || {
        MockProvider::new("mock")
            .enqueue_error(ProviderError::RateLimited {
                retry_after_ms: Some(1),
            })
            .enqueue_text("ok-0")
            .enqueue_error(ProviderError::RateLimited {
                retry_after_ms: Some(1),
            })
            .enqueue_text("ok-1")
    };
    let (outcome, events) = run_to_events(
        yaml,
        MockShell::new(),
        MockToolExecutor::new(),
        make_provider(),
        RuntimeConfig::default(),
    )
    .await;

    assert!(outcome.ok, "both iterations recovered");
    let delays: Vec<i64> = events
        .iter()
        .filter(|e| e.kind == EventKind::TaskRetrying)
        .filter_map(|e| int_field(e, "delay_ms"))
        .collect();
    assert_eq!(delays.len(), 2, "one retry per iteration");
    assert_ne!(
        delays[0], delays[1],
        "same task · same attempt · DIFFERENT iterations ⇒ distinct \
         jitter draws (the herd must not synchronize): {delays:?}"
    );
    // Replay-stable: the index is part of the deterministic coordinates.
    let (_, events_again) = run_to_events(
        yaml,
        MockShell::new(),
        MockToolExecutor::new(),
        make_provider(),
        RuntimeConfig::default(),
    )
    .await;
    let delays_again: Vec<i64> = events_again
        .iter()
        .filter(|e| e.kind == EventKind::TaskRetrying)
        .filter_map(|e| int_field(e, "delay_ms"))
        .collect();
    assert_eq!(delays, delays_again, "jittered delays replay byte-stable");
}

/// A rate-limited first attempt (transient) retries and succeeds — the
/// `TaskRetrying` frame carries `attempt`/`max_attempts`/`delay_ms` and the
/// terminal frame is success.
#[tokio::test]
async fn transient_failure_retries_and_succeeds() {
    let yaml = r#"
nika: flaky
model: mock/echo
tasks:
  ask:
    retry: { max_attempts: 3, backoff_ms: 7, backoff_strategy: fixed, jitter: false }
    agent:
      prompt: "try hard"
"#;
    let provider = MockProvider::new("mock")
        .enqueue_error(ProviderError::RateLimited {
            retry_after_ms: Some(1),
        })
        .enqueue_text("recovered answer");
    let (outcome, events) = run_to_events(
        yaml,
        MockShell::new(),
        MockToolExecutor::new(),
        provider,
        RuntimeConfig::default(),
    )
    .await;

    assert!(outcome.ok, "the retry recovered the task");
    assert_eq!(output_str(&outcome, "ask"), "recovered answer");
    let retry = events
        .iter()
        .find(|e| e.kind == EventKind::TaskRetrying)
        .expect("one TaskRetrying frame");
    assert_eq!(str_field(retry, "task"), Some("ask"));
    assert_eq!(int_field(retry, "attempt"), Some(1));
    assert_eq!(int_field(retry, "max_attempts"), Some(3));
    assert_eq!(
        int_field(retry, "delay_ms"),
        Some(7),
        "fixed · no jitter · the spec table value"
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| e.kind == EventKind::TaskRetrying)
            .count(),
        1,
        "exactly one retry was needed"
    );
    assert_eq!(outcome.records["ask"].status, TaskStatus::Success);
}

/// A terminal (non-transient) error never retries — even with a
/// `retry:` block (spec 05 conformance · transient-only).
#[tokio::test]
async fn terminal_error_never_retries() {
    let yaml = r#"
nika: terminal
permits: { tools: ["nika:fetch"], net: { http: ["example.com"] } }
tasks:
  fetch:
    retry: { max_attempts: 5 }
    invoke: { tool: "nika:fetch", args: { url: "https://example.com/data" } }
"#;
    let tools = MockToolExecutor::new().enqueue_err(ToolExecError::NotFound {
        name: "nika:fetch".to_owned(),
    });
    let (outcome, events) = run_to_events(
        yaml,
        MockShell::new(),
        tools,
        MockProvider::new("mock"),
        RuntimeConfig::default(),
    )
    .await;
    assert!(!outcome.ok);
    assert_eq!(
        events
            .iter()
            .filter(|e| e.kind == EventKind::TaskRetrying)
            .count(),
        0,
        "a terminal error must not burn retry attempts"
    );
    assert_eq!(outcome.records["fetch"].status, TaskStatus::Failure);
}

// ─── 6 · timeout (spec 03 · the one wall-clock budget) ───────────────────

/// A tool that never resolves — the timeout budget kills the attempt
/// loop deterministically (mock clock · the timer is instant).
struct HangingTools;

impl ToolExecuteDyn for HangingTools {
    async fn execute(&self, _call: ToolCall) -> Result<ToolResult, ToolExecError> {
        std::future::pending().await
    }
}

#[tokio::test]
async fn timeout_kills_a_hanging_task_with_the_spec_code() {
    let yaml = r#"
nika: hung
permits: { tools: ["nika:read"], fs: { read: ["slow.txt"] } }
tasks:
  stuck:
    timeout: "50ms"
    invoke: { tool: "nika:read", args: { path: "slow.txt" } }
  caught:
    timeout: "50ms"
    on_error: { on_codes: [NIKA-TIMEOUT-001], skip: true }
    invoke: { tool: "nika:read", args: { path: "slow.txt" } }
"#;
    let (wf, report) = parse_and_check(yaml);
    assert!(report.is_clean());
    let runtime = runtime_with_tools(
        MockShell::new(),
        HangingTools,
        MockProvider::new("mock"),
        RuntimeConfig::default(),
    );
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        runtime.run(&wf, &report, &mut stamper, &mut sink),
    )
    .await
    .expect("the budget fires — a hanging verb never wedges the run")
    .expect("clean run");
    let events = sink.into_events();

    // stuck: timeout = failure with the SPEC wire code.
    assert!(!outcome.ok);
    assert_eq!(outcome.records["stuck"].status, TaskStatus::Failure);
    let err = outcome.records["stuck"]
        .error
        .as_ref()
        .expect("typed error");
    assert_eq!(err.code, "NIKA-TIMEOUT-001");
    assert!(!err.transient, "a timeout is never retryable (spec 03)");
    let failed = events
        .iter()
        .find(|e| e.kind == EventKind::TaskFailed && str_field(e, "task") == Some("stuck"))
        .expect("stuck failed");
    assert!(
        str_field(failed, "detail")
            .expect("detail")
            .contains("NIKA-TIMEOUT-001")
    );

    // caught: the SAME timeout class is catchable by on_error (skip) ·
    // the original error stays readable (the coexist state · spec 05).
    assert_eq!(outcome.records["caught"].status, TaskStatus::Skipped);
    assert_eq!(
        outcome.records["caught"]
            .error
            .as_ref()
            .expect("error readable")
            .code,
        "NIKA-TIMEOUT-001"
    );
}

// ─── 7 · on_error (spec 05 · recover / skip / filter fall-through) ───────

#[tokio::test]
async fn on_error_recover_skip_and_filter() {
    let yaml = r#"
nika: recovery
permits: { exec: true, tools: ["nika:fetch", "nika:jq"], net: { http: ["example.com"] } }
tasks:
  cached:
    exec: { command: ["cat", "cache.json"] }
  live:
    after: { cached: success }
    on_error: { recover: "${{ tasks.cached.output }}" }
    invoke: { tool: "nika:fetch", args: { url: "https://example.com/a" } }
  optional:
    on_error: { skip: true }
    invoke: { tool: "nika:fetch", args: { url: "https://example.com/b" } }
  unmatched:
    on_error: { on_codes: [NIKA-GHOST-999], skip: true }
    invoke: { tool: "nika:fetch", args: { url: "https://example.com/c" } }
  downstream:
    # a BUILTIN consumer, not a shell: the order law (NIKA-SEC-015)
    # refuses an `exec:` downstream of a fetch, and this fixture is
    # about `on_error:` routing, never about reaching a shell.
    with: { fetched: "${{ tasks.live.output }}" }
    invoke: { tool: "nika:jq", args: { input: "${{ with.fetched }}", expression: "." } }
"#;
    let shell = MockShell::new().enqueue_ok("stale data\n");
    let tools = MockToolExecutor::new()
        .enqueue_err(ToolExecError::NotFound {
            name: "nika:fetch".to_owned(),
        })
        .enqueue_err(ToolExecError::NotFound {
            name: "nika:fetch".to_owned(),
        })
        .enqueue_err(ToolExecError::NotFound {
            name: "nika:fetch".to_owned(),
        })
        .enqueue_ok(ToolResult::success("jq", "\"stale data\\n\""));
    let (outcome, _events) = run_to_events(
        yaml,
        shell,
        tools,
        MockProvider::new("mock"),
        RuntimeConfig::new(NonZeroUsize::new(1), 0), // FIFO mock queues stay aligned
    )
    .await;

    // recover: downstream sees SUCCESS with the fallback value.
    assert_eq!(outcome.records["live"].status, TaskStatus::Success);
    assert_eq!(output_str(&outcome, "live"), "stale data");
    assert_eq!(outcome.records["downstream"].status, TaskStatus::Success);
    // skip: status skipped · original error readable (coexist state).
    assert_eq!(outcome.records["optional"].status, TaskStatus::Skipped);
    assert!(outcome.records["optional"].error.is_some());
    // filter fall-through: unlisted code → the default fail.
    assert_eq!(outcome.records["unmatched"].status, TaskStatus::Failure);
    assert!(!outcome.ok, "the unmatched failure decides the verdict");
}

// ─── 7b · on_codes selectivity over the SPEC code (BUG-C) ───────────────

/// `on_error.on_codes` / `retry.on_codes` filter on the USER-FACING SPEC
/// code (`NIKA-EXEC-001`), NOT the engine wire code (`NIKA-440`). The
/// `nika check` regex forces the author to write the spec form
/// (`^NIKA-[A-Z]{2,9}…`), and `tasks.X.error.code` exposes the same — so
/// selective recovery/retry must compare against the spec code or it is
/// inert (BUG-C: the author was forced to write `NIKA-EXEC-001` while the
/// matcher compared `NIKA-440` → no match ever fired).
#[tokio::test]
async fn on_codes_matches_the_user_facing_spec_code() {
    // 1 · on_error.on_codes:[NIKA-EXEC-001] on a non-zero exit → recovery FIRES.
    let yaml = r#"
nika: oncodes-catch
permits: { exec: true }
tasks:
  boom:
    exec: { shell: "exit 7" }
    on_error:
      on_codes: [NIKA-EXEC-001]
      recover: "recovered"
"#;
    let shell = MockShell::new().enqueue_fail(7, "boom");
    let (outcome, _events) = run_to_events(
        yaml,
        shell,
        MockToolExecutor::new(),
        MockProvider::new("mock"),
        RuntimeConfig::default(),
    )
    .await;
    assert!(outcome.ok, "the spec-code filter matched → recovery fired");
    assert_eq!(outcome.records["boom"].status, TaskStatus::Success);
    assert_eq!(output_str(&outcome, "boom"), "recovered");

    // 2 · the SAME failure surfaces the SPEC code at tasks.X.error.code (so a
    //     downstream `on_codes`/CEL filter reads the form it was forced to write).
    let yaml_skip = r#"
nika: oncodes-skip
permits: { exec: true }
tasks:
  boom:
    exec: { shell: "exit 7" }
    on_error: { skip: true }
"#;
    let (outcome, _events) = run_to_events(
        yaml_skip,
        MockShell::new().enqueue_fail(7, "boom"),
        MockToolExecutor::new(),
        MockProvider::new("mock"),
        RuntimeConfig::default(),
    )
    .await;
    assert_eq!(outcome.records["boom"].status, TaskStatus::Skipped);
    assert_eq!(
        outcome.records["boom"]
            .error
            .as_ref()
            .expect("error readable")
            .code,
        "NIKA-EXEC-001",
        "the user-facing code is the spec code, not NIKA-440"
    );
}

/// Part two of the spec-code filter proof: `retry.on_codes` compares the
/// same user-facing form, and an unlisted code falls through untouched.
#[tokio::test]
async fn on_codes_retry_filter_and_selectivity_use_the_spec_code() {
    // 3 · retry.on_codes:[NIKA-EXEC-001] selectively retries a NON-transient
    //     exit (the override's whole point) → a TaskRetrying frame is emitted.
    let yaml_retry = r#"
nika: oncodes-retry
permits: { exec: true }
tasks:
  boom:
    retry: { max_attempts: 2, backoff_ms: 1, backoff_strategy: fixed, jitter: false, on_codes: [NIKA-EXEC-001] }
    exec: { shell: "exit 7" }
"#;
    let shell = MockShell::new()
        .enqueue_fail(7, "boom")
        .enqueue_fail(7, "boom");
    let (outcome, events) = run_to_events(
        yaml_retry,
        shell,
        MockToolExecutor::new(),
        MockProvider::new("mock"),
        RuntimeConfig::default(),
    )
    .await;
    assert_eq!(
        events
            .iter()
            .filter(|e| e.kind == EventKind::TaskRetrying)
            .count(),
        1,
        "the spec-code retry filter matched a non-transient exit → one retry"
    );
    assert_eq!(outcome.records["boom"].status, TaskStatus::Failure);

    // 4 · selectivity intact: a NON-matching code does NOT recover.
    let yaml_miss = r#"
nika: oncodes-miss
permits: { exec: true }
tasks:
  boom:
    exec: { shell: "exit 7" }
    on_error:
      on_codes: [NIKA-INFER-001]
      recover: "should-not-fire"
"#;
    let (outcome, _events) = run_to_events(
        yaml_miss,
        MockShell::new().enqueue_fail(7, "boom"),
        MockToolExecutor::new(),
        MockProvider::new("mock"),
        RuntimeConfig::default(),
    )
    .await;
    assert_eq!(
        outcome.records["boom"].status,
        TaskStatus::Failure,
        "an unlisted code falls through to the default fail (selectivity intact)"
    );
}

// ─── 7c · a skipped task's error has a CONSUMER (edge admission) ─────────

/// `EdgeKind::FailureObservation` admits {failure, skipped}, and `skipped`
/// is in that set solely so this hatch has a reader. The assertions above
/// pin the RECORD; nothing pinned that anything downstream can READ it —
/// narrow the pass-set to {failure}, a plausible-looking tightening, and
/// every documented `on_error: skip` router goes dead with a fully green
/// suite (issue 1233, split out of 1198). Three assertions, not one: the
/// consumer is ADMITTED (success, never cancelled) · the value it bound
/// is the typed record with its code intact (not null, not the string
/// "null") · and the SAME consumer against a SUCCEEDED producer settles
/// cancelled, so the test cannot pass on a pass-set widened to everything
/// either, which is the opposite failure.
#[tokio::test]
async fn on_error_skip_error_record_reaches_a_downstream_consumer() {
    let yaml = r#"
nika: skip-error-consumer
permits: { exec: true }
tasks:
  boom:
    exec: { shell: "exit 7" }
    on_error: { skip: true }
  reads_error:
    with: { e: "${{ tasks.boom.error }}" }
    exec: { shell: "printf 'ERROR_SEEN=[%s]' '${{ with.e }}'" }
"#;
    let shell = MockShell::new()
        .enqueue_fail(7, "boom")
        .enqueue_ok("ERROR_SEEN");
    let (outcome, _events) = run_to_events(
        yaml,
        shell.clone(),
        MockToolExecutor::new(),
        MockProvider::new("mock"),
        RuntimeConfig::new(NonZeroUsize::new(1), 0), // FIFO mock queues stay aligned
    )
    .await;

    // 1 · ADMITTED: the observation edge lets the consumer run to success.
    assert_eq!(
        outcome.records["reads_error"].status,
        TaskStatus::Success,
        "the consumer of tasks.boom.error is admitted, not cancelled"
    );

    // 2 · the bound value is the TYPED record, code intact — observed at
    //     the seam, in the exact bytes the consumer's command carried (the
    //     shell form holds the whole line in `program`, argv in `args`).
    let rendered = shell
        .executed_commands()
        .iter()
        .map(|c| format!("{} {}", c.program, c.args.join(" ")))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        rendered.contains("\"code\":\"NIKA-EXEC-001\""),
        "the consumer received the typed record with its code intact · got: {rendered}"
    );
    assert!(
        !rendered.contains("null"),
        "the record never degenerates to null · got: {rendered}"
    );

    // 3 · negative control: `.error` must NOT admit a success — the SAME
    //     consumer under a SUCCEEDED producer settles cancelled. Without
    //     this leg the test still passes on a pass-set widened to admit
    //     everything, which is the opposite failure.
    let yaml_live = r#"
nika: skip-error-consumer-control
permits: { exec: true }
tasks:
  fine:
    exec: { shell: "printf ok" }
  reads_error:
    with: { e: "${{ tasks.fine.error }}" }
    exec: { shell: "printf unreachable" }
"#;
    let (outcome, _events) = run_to_events(
        yaml_live,
        MockShell::new().enqueue_ok("ok"),
        MockToolExecutor::new(),
        MockProvider::new("mock"),
        RuntimeConfig::default(),
    )
    .await;
    assert_eq!(outcome.records["fine"].status, TaskStatus::Success);
    assert_eq!(
        outcome.records["reads_error"].status,
        TaskStatus::Cancelled,
        ".error admits no success — the consumer settles cancelled"
    );
}

// ─── 8 · for_each (spec 03 · the fan-out construct) ─────────────────────

#[tokio::test]
async fn for_each_maps_items_in_order_with_locals() {
    let yaml = r#"
nika: fan
permits: { exec: true }
const:
  urls: ["alpha", "beta", "gamma"]
tasks:
  scrape:
    for_each: { items: "${{ const.urls }}", max_parallel: 1 }
    with: { page: "${{ item }}" }
    exec: { command: ["fetch", "${{ with.page }}", "at", "${{ index }}"] }
  join:
    with: { pages: "${{ tasks.scrape.output }}" }
    exec: { command: ["got", "${{ with.pages }}"] }
"#;
    let shell = MockShell::new()
        .enqueue_ok("r-alpha\n")
        .enqueue_ok("r-beta\n")
        .enqueue_ok("r-gamma\n")
        .enqueue_ok("done\n");
    let (outcome, events) = run_to_events(
        yaml,
        shell,
        MockToolExecutor::new(),
        MockProvider::new("mock"),
        RuntimeConfig::default(),
    )
    .await;

    assert!(outcome.ok);
    // The task output is the ARRAY of per-iteration outputs, input order.
    assert_eq!(
        outcome.records["scrape"].output,
        serde_json::json!(["r-alpha", "r-beta", "r-gamma"])
    );
    // The note carries the fan-out arity.
    let started = events
        .iter()
        .find(|e| e.kind == EventKind::TaskStarted && str_field(e, "task") == Some("scrape"))
        .expect("scrape started");
    assert_eq!(str_field(started, "note"), Some("for_each · 3 items"));
    // Downstream renders the array canonically (compact JSON).
    assert_eq!(
        output_str(&outcome, "join"),
        "done",
        "the join consumed the array"
    );
}

/// A fan-out of infer iterations SUMS the per-iteration token spend
/// onto the parent's `TaskCompleted` (truth accounting — the cost meter
/// must never under-count a 50-infer fan-out to zero).
#[tokio::test]
async fn for_each_infer_sums_iteration_tokens() {
    let yaml = r#"
nika: fan-tokens
model: mock/echo
const:
  prompts: ["alpha", "beta", "gamma"]
tasks:
  think_all:
    for_each: { items: "${{ const.prompts }}", max_parallel: 1 }
    infer:
      prompt: "ponder ${{ item }}"
"#;
    let (outcome, events) = run_to_events(
        yaml,
        MockShell::new(),
        MockToolExecutor::new(),
        MockProvider::new("mock"),
        RuntimeConfig::default(),
    )
    .await;

    assert!(outcome.ok);
    let completed = events
        .iter()
        .find(|e| e.kind == EventKind::TaskCompleted && str_field(e, "task") == Some("think_all"))
        .expect("fan-out completed");
    let tokens = int_field(completed, "tokens")
        .expect("a fan-out of infers reports token spend — never silently zero");
    assert!(tokens > 0, "the sum of three echo completions is positive");
    // The echo provider's per-completion usage is deterministic — three
    // identical iterations must report exactly 3× one iteration's spend.
    assert_eq!(
        tokens % 3,
        0,
        "sum of three equal iteration spends: {tokens}"
    );
}

/// `fail_fast: false` collects every iteration · failed slots
/// contribute null at their index · the parent is failure (spec 03).
#[tokio::test]
async fn for_each_fail_fast_false_nulls_at_index() {
    let yaml = r#"
nika: fan-collect
permits: { exec: true }
const:
  items: ["one", "two", "three"]
tasks:
  work:
    for_each: { items: "${{ const.items }}", max_parallel: 1, fail_fast: false }
    exec: { command: ["do", "${{ item }}"] }
"#;
    let shell = MockShell::new()
        .enqueue_ok("ok-one\n")
        .enqueue_fail(3, "two exploded")
        .enqueue_ok("ok-three\n");
    let (outcome, _events) = run_to_events(
        yaml,
        shell,
        MockToolExecutor::new(),
        MockProvider::new("mock"),
        RuntimeConfig::default(),
    )
    .await;

    assert!(!outcome.ok, "a failed iteration fails the parent");
    assert_eq!(outcome.records["work"].status, TaskStatus::Failure);
    let err = outcome.records["work"].error.as_ref().expect("first error");
    assert!(err.message.contains("two exploded") || !err.code.is_empty());
    // Positional alignment survives partial failure — but the parent
    // FAILED, so its output reads defined-null (spec 04); the
    // per-iteration array is internal. The error is the surface.
}

/// Spec 03 §null-at-index — the REAL « positional alignment survives
/// partial failure (the zip patterns stay sound) » guarantee · delivered
/// the documented way · a per-iteration `on_error: skip` turns a failing
/// element into a `null` AT ITS INDEX, the parent SUCCEEDS, and `.output`
/// is the positional array a downstream zip can read. (Contrast
/// `for_each_fail_fast_false_nulls_at_index` · an unrecovered failure
/// fails the parent and `.output` reads defined-null · spec 04.)
#[tokio::test]
async fn for_each_on_error_skip_nulls_at_index_parent_succeeds() {
    let yaml = r#"
nika: fan-skip
permits: { exec: true }
const:
  items: ["one", "two", "three"]
tasks:
  work:
    for_each: { items: "${{ const.items }}", max_parallel: 1 }
    on_error: { skip: true }
    exec: { command: ["do", "${{ item }}"] }
"#;
    let shell = MockShell::new()
        .enqueue_ok("ok-one\n")
        .enqueue_fail(3, "two exploded")
        .enqueue_ok("ok-three\n");
    let (outcome, _events) = run_to_events(
        yaml,
        shell,
        MockToolExecutor::new(),
        MockProvider::new("mock"),
        RuntimeConfig::default(),
    )
    .await;

    // The parent SUCCEEDS — every iteration was handled (skip), so the
    // fan-out has no unrecovered error.
    assert!(outcome.ok, "per-iteration skip keeps the parent successful");
    assert_eq!(outcome.records["work"].status, TaskStatus::Success);
    // `.output` is the positional array · null AT the skipped index ·
    // the zip pattern stays sound (positions align with `const.items`).
    assert_eq!(
        outcome.records["work"].output,
        serde_json::json!(["ok-one", null, "ok-three"])
    );
}

/// `fail_fast: true` (default) stops dispatching after the first
/// settled error — under `max_parallel: 1` later iterations never run.
#[tokio::test]
async fn for_each_fail_fast_true_stops_the_lane() {
    let yaml = r#"
nika: fan-abort
permits: { exec: true }
const:
  items: ["one", "two", "three"]
tasks:
  work:
    for_each: { items: "${{ const.items }}", max_parallel: 1 }
    exec: { command: ["do", "${{ item }}"] }
"#;
    // Only TWO shell results enqueued: iteration 1 ok · iteration 2
    // fails · iteration 3 must never dispatch (an empty queue would
    // panic the mock — reaching it IS the regression).
    let shell = MockShell::new()
        .enqueue_ok("ok-one\n")
        .enqueue_fail(3, "two exploded");
    let (outcome, _events) = run_to_events(
        yaml,
        shell,
        MockToolExecutor::new(),
        MockProvider::new("mock"),
        RuntimeConfig::default(),
    )
    .await;
    assert!(!outcome.ok);
    assert_eq!(outcome.records["work"].status, TaskStatus::Failure);
}

/// An empty collection skips the task (spec 03) · a non-array
/// collection fails it loudly (`NIKA-VAR-006` class).
///
/// The bad lane sources from a TASK OUTPUT, not a `const:`. Defence in
/// depth is the point: the runtime owns this refusal on its own, and the
/// fixture must stay clean for `run_to_events` to reach the run at all.
/// A `const:` scalar no longer survives the ladder — the check refuses it
/// statically now (a constant is never caller-supplied, so its literal IS
/// its run value), and that static arm is covered in
/// `nika-check`'s `for_each_over_an_untyped_non_array_const_is_caught_before_run`.
/// A `tasks.*` source is left alone there by construction, so it is the
/// honest way to hand the runtime a non-array at dispatch.
#[tokio::test]
async fn for_each_empty_skips_and_non_array_fails() {
    let yaml = r#"
nika: fan-edges
permits: { exec: true }
const:
  none: []
tasks:
  emit_scalar:
    exec: { command: ["echo", "not a list"] }
  empty_lane:
    for_each: { items: "${{ const.none }}" }
    exec: { command: ["never", "${{ item }}"] }
  bad_lane:
    with: { items: "${{ tasks.emit_scalar.output }}" }
    for_each: { items: "${{ with.items }}" }
    exec: { command: ["never", "${{ item }}"] }
"#;
    let (outcome, events) = run_to_events(
        yaml,
        MockShell::new().enqueue_ok("not a list"),
        MockToolExecutor::new(),
        MockProvider::new("mock"),
        RuntimeConfig::default(),
    )
    .await;

    assert_eq!(outcome.records["empty_lane"].status, TaskStatus::Skipped);
    let skip = events
        .iter()
        .find(|e| e.kind == EventKind::TaskSkipped && str_field(e, "task") == Some("empty_lane"))
        .expect("empty fan-out skips");
    assert_eq!(str_field(skip, "note"), Some("for_each · empty collection"));

    assert_eq!(outcome.records["bad_lane"].status, TaskStatus::Failure);
    let err = outcome.records["bad_lane"].error.as_ref().expect("typed");
    assert_eq!(err.code, "NIKA-VAR-006");
    assert!(
        err.message.contains("string"),
        "names the wrong kind: {}",
        err.message
    );
    assert!(!outcome.ok);
}

/// Iterations dispatch concurrently under `max_parallel` — two
/// rendezvous-coupled iterations only complete if both are in flight.
#[tokio::test]
async fn for_each_iterations_run_concurrently_under_the_cap() {
    let yaml = r#"
nika: fan-pair
permits: { tools: ["nika:read"] }
const:
  items: ["x", "y"]
tasks:
  pair:
    for_each: { items: "${{ const.items }}", max_parallel: 2 }
    invoke: { tool: "nika:read", args: { path: "${{ item }}" } }
"#;
    let (wf, report) = parse_and_check(yaml);
    assert!(report.is_clean());
    let runtime = runtime_with_tools(
        MockShell::new(),
        BarrierTools::new(2),
        MockProvider::new("mock"),
        RuntimeConfig::default(),
    );
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        runtime.run(&wf, &report, &mut stamper, &mut sink),
    )
    .await
    .expect("both iterations in flight — max_parallel:1 would deadlock")
    .expect("clean run");
    assert!(outcome.ok);
    assert_eq!(
        outcome.records["pair"].output,
        serde_json::json!(["met", "met"])
    );
}

// ─── 9 · records in gates (spec 04 · status routing) ───────────────────

#[tokio::test]
async fn status_gates_route_on_skipped_upstream() {
    let yaml = r#"
nika: routing
permits: { exec: true }
const: { mode: "fast" }
tasks:
  slow_path:
    when: ${{ const.mode == 'slow' }}
    exec: { command: ["slow", "work"] }
  report:
    with: { s: "${{ tasks.slow_path.status }}" }
    when: ${{ with.s != 'success' }}
    exec: { command: ["report", "skipped", "lane"] }
"#;
    let shell = MockShell::new().enqueue_ok("reported\n");
    let (outcome, _events) = run_to_events(
        yaml,
        shell,
        MockToolExecutor::new(),
        MockProvider::new("mock"),
        RuntimeConfig::default(),
    )
    .await;
    assert!(outcome.ok);
    assert_eq!(outcome.records["slow_path"].status, TaskStatus::Skipped);
    assert_eq!(output_str(&outcome, "report"), "reported");
}

// ─── 10 · spec-plane wire codes pin to the embedded canon ───────────────

/// Every spec-plane code the runtime EMITS into `TaskErrorRecord` must
/// resolve in the embedded spec table (`nika_pack::error_codes()` —
/// the one typed accessor over the canon). A drifted slug silently
/// breaks user `on_codes:` filters (review P0 · the hand-typed-code
/// class) — this pins the BEHAVIORAL emission, not just the const.
#[tokio::test]
async fn emitted_spec_codes_resolve_in_the_embedded_canon() {
    // Shape 1 · the timeout class (NIKA-TIMEOUT-001).
    let timeout_yaml = r#"
nika: pin-timeout
permits: { tools: ["nika:read"], fs: { read: ["slow.txt"] } }
tasks:
  stuck:
    timeout: "10ms"
    invoke: { tool: "nika:read", args: { path: "slow.txt" } }
"#;
    let (wf, report) = parse_and_check(timeout_yaml);
    let runtime = runtime_with_tools(
        MockShell::new(),
        HangingTools,
        MockProvider::new("mock"),
        RuntimeConfig::default(),
    );
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        runtime.run(&wf, &report, &mut stamper, &mut sink),
    )
    .await
    .expect("budget fires")
    .expect("clean run");
    let timeout_code = outcome.records["stuck"]
        .error
        .as_ref()
        .expect("typed error")
        .code
        .clone();

    // Shape 2 · the expression-type class (non-array for_each). The
    // collection rides a TASK OUTPUT: a `const:` scalar is now refused
    // statically (a constant is never caller-supplied, so its literal IS
    // its run value) and would never reach the run this shape pins.
    let var_yaml = r#"
nika: pin-var
permits: { exec: true }
tasks:
  emit_scalar:
    exec: { command: ["echo", "not a list"] }
  bad:
    with: { items: "${{ tasks.emit_scalar.output }}" }
    for_each: { items: "${{ with.items }}" }
    exec: { command: ["never", "${{ item }}"] }
"#;
    let (outcome2, _) = run_to_events(
        var_yaml,
        MockShell::new().enqueue_ok("not a list"),
        MockToolExecutor::new(),
        MockProvider::new("mock"),
        RuntimeConfig::default(),
    )
    .await;
    let var_code = outcome2.records["bad"]
        .error
        .as_ref()
        .expect("typed error")
        .code
        .clone();

    // Both resolve in the canon — the table IS the truth source.
    let canon = nika_pack::error_codes();
    for code in [&timeout_code, &var_code] {
        assert!(
            canon.iter().any(|row| row.code == code),
            "{code} must resolve in the embedded spec canon — a drifted \
             slug silently breaks on_codes: filters"
        );
    }
    assert_eq!(timeout_code, "NIKA-TIMEOUT-001");
    assert_eq!(var_code, "NIKA-VAR-006");
}

// ─── 11 · the for_each when-gate scope hazard (statically refused) ──────

/// The sunset this test's earlier form named in its own docstring
/// arrived: « pins the loud lane until the checker grows the matching
/// static rule ». The rule landed (`when:`/`for_each:` stop admitting
/// the loop locals), so the refusal moved from the first evaluated
/// gate to BEFORE anything runs — and it kept the wire code: the
/// spec-plane NIKA-VAR-001 (spec 05 §142), never the engine-internal
/// NIKA-1702. Same law, enforced one stage earlier.
#[test]
fn for_each_when_gate_referencing_item_is_refused_statically() {
    let yaml = r#"
nika: gate-item
permits: { exec: true }
const:
  items: ["a", "b"]
tasks:
  fan:
    for_each: { items: "${{ const.items }}" }
    when: ${{ item != 'skip' }}
    exec: { command: ["do", "${{ item }}"] }
"#;
    let (_wf, report) = parse_and_check(yaml);
    assert!(
        !report.is_clean(),
        "the static rule refuses the gate before any dispatch"
    );
    let v = report
        .conformance
        .iter()
        .find(|v| v.code == "NIKA-VAR-001")
        .expect("the refusal carries the spec-plane unresolved-reference code");
    assert!(
        !v.code.contains("1702") && !v.message.contains("1702"),
        "no engine-internal code leaks into the author-visible finding: {} · {}",
        v.code,
        v.message
    );
    // no valid DAG order exists for a refused workflow — nothing could
    // ever dispatch, which is the whole point of the earlier stage
    assert!(
        report.waves.is_empty(),
        "no plan exists for a refused workflow"
    );
}

// ─── 12 · structured tool output survives the seam (BUG#3) ──────────────

/// A builtin that returns a typed value (here an array, as `nika:glob`
/// does) must reach `tasks.X.output` AS the array — so a downstream
/// `for_each: ${{ tasks.X.output }}` iterates it instead of failing
/// NIKA-VAR-006 ("collection must be an array · got string"). This is the
/// localization-factory's glob→read fan-out, proven at the runtime seam
/// with a mock tool carrying a structured value (the dispatcher-agnostic
/// contract: ToolResult.structured → InvokeOutput.structured → output).
#[tokio::test]
async fn builtin_invoke_array_output_lets_for_each_iterate() {
    let yaml = r#"
nika: glob-fanout
# The glob OPENS ./docs · a bound covering the walk root is what the run needs,
# and since 2026-08-20 the audit says so too (the effect table gained the entry).
permits: { tools: ["nika:glob", "nika:read"], fs: { read: ["./docs/**"] } }
tasks:
  files:
    invoke:
      tool: "nika:glob"
      args: { pattern: "./docs/**/*.md" }
  texts:
    with: { files: "${{ tasks.files.output }}" }
    for_each: { items: "${{ with.files }}", max_parallel: 1 }
    invoke:
      tool: "nika:read"
      args: { path: "${{ item }}" }
"#;
    // FIFO: the glob settles first (a STRUCTURED array · content is the JSON
    // text the model would see), then the two per-item reads.
    let tools = MockToolExecutor::new()
        .enqueue_ok(
            ToolResult::success("c-glob", r#"["./docs/a.md","./docs/b.md"]"#)
                .with_structured(serde_json::json!(["./docs/a.md", "./docs/b.md"])),
        )
        .enqueue_ok(ToolResult::success("c-read-0", "alpha"))
        .enqueue_ok(ToolResult::success("c-read-1", "beta"));
    let (outcome, _events) = run_to_events(
        yaml,
        MockShell::new(),
        tools,
        MockProvider::new("mock"),
        RuntimeConfig::default(),
    )
    .await;

    assert!(outcome.ok, "the glob→for_each path runs (no NIKA-VAR-006)");
    // The glob output reached tasks.files.output AS an array (the fix) —
    // NOT a stringified one (the bug · would be Value::String).
    assert_eq!(
        outcome.records["files"].output,
        serde_json::json!(["./docs/a.md", "./docs/b.md"]),
        "structured array survives the seam · not Value::String"
    );
    // …and the for_each iterated both elements, one read per item.
    assert_eq!(outcome.records["texts"].status, TaskStatus::Success);
    assert_eq!(
        outcome.records["texts"].output,
        serde_json::json!(["alpha", "beta"]),
        "for_each fanned over the glob array · two iterations"
    );
}

/// A structured OBJECT survives too — so CEL navigation into a binding-less
/// tool output (`tasks.X.output.<field>`) sees the real object, not a
/// string. (`nika:jq` · `nika:fetch`'s JSON modes return objects.)
#[tokio::test]
async fn structured_object_tool_output_navigates() {
    let yaml = r#"
nika: object-nav
permits: { exec: true, tools: ["nika:jq"] }
tasks:
  api:
    invoke:
      tool: "nika:jq"
      args: { input: { count: 2 }, expression: "." }
  use:
    with: { count: "${{ tasks.api.output.count }}" }
    when: ${{ with.count > 1 }}
    exec: { command: ["echo", "${{ with.count }}"] }
"#;
    let tools = MockToolExecutor::new().enqueue_ok(
        ToolResult::success("c-jq", r#"{"count":2}"#)
            .with_structured(serde_json::json!({ "count": 2 })),
    );
    let (outcome, _events) = run_to_events(
        yaml,
        MockShell::new().enqueue_ok("2\n"),
        tools,
        MockProvider::new("mock"),
        RuntimeConfig::default(),
    )
    .await;

    assert!(
        outcome.ok,
        "object output navigates · the when-gate resolves"
    );
    assert_eq!(
        outcome.records["api"].output,
        serde_json::json!({ "count": 2 }),
        "structured object survives · field access works downstream"
    );
    assert_eq!(outcome.records["use"].status, TaskStatus::Success);
}

/// The negative half: a tool with NO structured value (an MCP-style tool
/// returning only text · `ToolResult` default `structured: None`) keeps
/// `tasks.X.output` a String — never silently JSON-coerced from text.
#[tokio::test]
async fn text_only_tool_output_stays_a_string() {
    let yaml = r#"
nika: text-output
permits: { tools: ["mcp:server/echo"] }
tasks:
  tool:
    invoke: { tool: "mcp:server/echo", args: {} }
"#;
    // A bare success — no with_structured → the MCP text path.
    let tools = MockToolExecutor::new().enqueue_ok(ToolResult::success("c", "plain text response"));
    let (outcome, _events) = run_to_events(
        yaml,
        MockShell::new(),
        tools,
        MockProvider::new("mock"),
        RuntimeConfig::default(),
    )
    .await;

    assert!(outcome.ok);
    assert_eq!(
        outcome.records["tool"].output,
        serde_json::Value::String("plain text response".to_owned()),
        "a text-only tool stays a String · the bug fix is opt-in via structured"
    );
}
