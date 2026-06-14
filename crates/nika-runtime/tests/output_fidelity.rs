// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]

//! Runtime output-fidelity battery — the two task-output value forms that
//! a `nika check` accepts but the L3 runtime must also HONOR:
//!
//! - **`output:` named bindings** (spec 04 §Output binding) · a task's jq
//!   projections bound as `tasks.X.<name>` for downstream CEL. The check
//!   side validates these references (analyzer `scan.rs` bindings) — the
//!   runtime must populate them at settle (Finding #1).
//! - **`exec: { capture: structured }`** (spec 02 §exec) · the
//!   `{ stdout, stderr, exit_code }` object reaches `tasks.X.output` AS an
//!   object (CEL field access · non-zero exit is data) — same class as the
//!   invoke structured-value seam (Finding #2).
//!
//! Every test runs the REAL parse → check → run chain over mock seams (the
//! floor discipline · no hand-built reports).

use std::sync::Arc;
use std::time::Duration;

use nika_event::{Event, EventKind};
use nika_kernel::process::ShellResult;
use nika_kernel::tool_executor::ToolResult;
use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_runtime::{DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, TaskStatus, VecSink};
use nika_types::resource::Value as FieldValue;
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;

// ─── harness (the floor: real parse → check → run over mocks) ─────────────

async fn run_to_events(
    yaml: &str,
    shell: MockShell,
    tools: MockToolExecutor,
    provider: MockProvider,
) -> (RunOutcome, Vec<Event>) {
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_schema::check(&wf);
    assert!(report.is_clean(), "fixture passes the ladder");
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
        RuntimeConfig::default(),
    );
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("clean run");
    (outcome, sink.into_events())
}

fn str_field<'a>(event: &'a Event, key: &str) -> Option<&'a str> {
    match event.fields.iter().find(|f| f.key == key).map(|f| &f.value) {
        Some(FieldValue::String(s)) => Some(s.as_str()),
        _ => None,
    }
}

// ─── output: named bindings resolve at the runtime (Finding #1) ────────────

/// A task's `output:` named bindings (spec 04 §Output binding) must be
/// evaluated at settle and bound as `tasks.X.<name>` for downstream CEL.
/// The check side already validates these references (scan.rs bindings) —
/// before the fix the L3 runtime never populated them, so `tasks.X.<name>`
/// failed NIKA-1702 at run (the check/runtime gap this closes).
#[tokio::test]
async fn output_named_bindings_resolve_downstream() {
    let yaml = r#"
nika: v1
workflow: out-bind
tasks:
  - id: src
    invoke: { tool: "nika:jq", args: { input: { count: 7 }, expression: "." } }
    output: { c: ".count" }
  - id: gate
    depends_on: [src]
    when: ${{ tasks.src.c == 7 }}
    exec: { command: "echo gated" }
"#;
    let tools = MockToolExecutor::new().enqueue_ok(
        ToolResult::success("c-jq", r#"{"count":7}"#)
            .with_structured(serde_json::json!({ "count": 7 })),
    );
    let (outcome, _events) = run_to_events(
        yaml,
        MockShell::new().enqueue_ok("gated\n"),
        tools,
        MockProvider::new("mock"),
    )
    .await;

    assert!(outcome.ok, "the named binding resolves · the gate opens");
    // The binding `c` = jq `.count` over {count:7} = 7, bound on the record.
    assert_eq!(
        outcome.records["src"].named.get("c"),
        Some(&serde_json::json!(7)),
        "tasks.src.c is the jq `.count` result · NOT a 1702"
    );
    // …and the downstream `when: ${{ tasks.src.c == 7 }}` opened → gate ran.
    assert_eq!(outcome.records["gate"].status, TaskStatus::Success);
    // The raw output is STILL dual-accessible (spec 04 §dual-accessible).
    assert_eq!(
        outcome.records["src"].output,
        serde_json::json!({ "count": 7 }),
        "tasks.src.output stays the raw value alongside the binding"
    );
}

/// A jq-PATH binding (deep path · `length` · `[ … ]`-collect) — the v0.1
/// jq conformance subset over the task's raw output (spec 04 §path grammar).
#[tokio::test]
async fn output_binding_jq_path_and_collect() {
    let yaml = r#"
nika: v1
workflow: out-bind-jq
tasks:
  - id: api
    invoke: { tool: "nika:jq", args: { input: {}, expression: "." } }
    output:
      n: ".data.users | length"
      emails: "[.data.users[].email]"
  - id: use
    depends_on: [api]
    when: ${{ tasks.api.n == 2 }}
    exec: { command: "echo two" }
"#;
    let body =
        serde_json::json!({ "data": { "users": [ { "email": "a@x" }, { "email": "b@y" } ] } });
    let tools = MockToolExecutor::new()
        .enqueue_ok(ToolResult::success("c", body.to_string()).with_structured(body));
    let (outcome, _events) = run_to_events(
        yaml,
        MockShell::new().enqueue_ok("two\n"),
        tools,
        MockProvider::new("mock"),
    )
    .await;

    assert!(outcome.ok, "jq-path bindings resolve · the gate opens");
    assert_eq!(
        outcome.records["api"].named.get("n"),
        Some(&serde_json::json!(2)),
        "`.data.users | length` = 2"
    );
    assert_eq!(
        outcome.records["api"].named.get("emails"),
        Some(&serde_json::json!(["a@x", "b@y"])),
        "`[.data.users[].email]` collects the stream into an array"
    );
}

/// A binding whose jq emits MORE than one value is NIKA-VAR-002 (the
/// single-value law · spec 04 §binding rules) — it FAILS the producing
/// task (evaluated before the terminal frame · `TaskCompleted` →
/// `TaskFailed`) even though the verb itself succeeded.
#[tokio::test]
async fn output_binding_cardinality_error_fails_the_task() {
    let yaml = r#"
nika: v1
workflow: out-bind-card
tasks:
  - id: src
    invoke: { tool: "nika:jq", args: { input: { users: [1, 2, 3] }, expression: "." } }
    output: { each: ".users[]" }
"#;
    let tools = MockToolExecutor::new().enqueue_ok(
        ToolResult::success("c", r#"{"users":[1,2,3]}"#)
            .with_structured(serde_json::json!({ "users": [1, 2, 3] })),
    );
    let (outcome, events) =
        run_to_events(yaml, MockShell::new(), tools, MockProvider::new("mock")).await;

    assert!(!outcome.ok, "a multi-value binding fails the task");
    assert_eq!(outcome.records["src"].status, TaskStatus::Failure);
    let err = outcome.records["src"]
        .error
        .as_ref()
        .expect("a failed task carries its error");
    assert_eq!(err.code, "NIKA-VAR-002", "the spec-plane cardinality code");
    assert!(!err.transient, "a binding error is never retryable");
    // The terminal frame is TaskFailed (the success was replaced before settle).
    assert!(
        events
            .iter()
            .any(|e| e.kind == EventKind::TaskFailed && str_field(e, "task") == Some("src")),
        "the success became a TaskFailed · not a TaskCompleted then an orphan error"
    );
    // The declared binding reads defined-null on the failed task (spec 04).
    assert_eq!(
        outcome.records["src"].named.get("each"),
        Some(&serde_json::Value::Null)
    );
}

/// A binding on a SKIPPED task reads defined-`null` (spec 04 §defined-null:
/// "tasks.X.<name> bindings of a skipped/cancelled task → null") — a
/// downstream read resolves to null, never a 1702.
#[tokio::test]
async fn output_binding_of_skipped_task_is_defined_null() {
    // A RUNTIME-false gate (not a statically-false `when: false`, which the
    // checker rejects as a dead branch) closes `maybe` at run · its
    // declared binding `c` must then read defined-null downstream.
    let yaml = r#"
nika: v1
workflow: out-bind-skip
vars:
  run: "no"
tasks:
  - id: maybe
    when: ${{ vars.run == 'yes' }}
    invoke: { tool: "nika:jq", args: { input: { count: 5 }, expression: "." } }
    output: { c: ".count" }
  - id: join
    depends_on: [maybe]
    when: ${{ tasks.maybe.c == null }}
    exec: { command: "echo joined" }
"#;
    let (outcome, _events) = run_to_events(
        yaml,
        MockShell::new().enqueue_ok("joined\n"),
        MockToolExecutor::new(),
        MockProvider::new("mock"),
    )
    .await;

    assert!(
        outcome.ok,
        "the null-read gate opens · skipped binding is null"
    );
    assert_eq!(outcome.records["maybe"].status, TaskStatus::Skipped);
    assert_eq!(
        outcome.records["maybe"].named.get("c"),
        Some(&serde_json::Value::Null),
        "a skipped task's binding reads defined-null (not absent → 1702)"
    );
    assert_eq!(outcome.records["join"].status, TaskStatus::Success);
}

// ─── exec capture: structured survives the seam (Finding #2) ───────────────

/// `exec: { capture: structured }` must reach `tasks.X.output` as the
/// `{ stdout, stderr, exit_code }` OBJECT (spec 02 §exec) — so CEL field
/// access resolves AND a non-zero exit is DATA (the task succeeds). Before
/// the fix the runtime ignored `capture:` entirely (every exec ran in
/// stdout mode · a non-zero exit failed the task · `.exit_code` was a 1702).
#[tokio::test]
async fn exec_capture_structured_exposes_stdout_stderr_exit_code() {
    let yaml = r#"
nika: v1
workflow: exec-structured
tasks:
  - id: probe
    exec: { command: "run-it", capture: structured }
  - id: gate
    depends_on: [probe]
    when: ${{ tasks.probe.output.exit_code == 3 }}
    exec: { command: "echo branched" }
"#;
    // A non-zero exit with BOTH streams — structured carries it as data.
    let shell = MockShell::new()
        .enqueue_result(ShellResult::new(
            3,
            "the answer\n",
            "a warning\n",
            Duration::from_millis(1),
        ))
        .enqueue_ok("branched\n");
    let (outcome, _events) = run_to_events(
        yaml,
        shell,
        MockToolExecutor::new(),
        MockProvider::new("mock"),
    )
    .await;

    assert!(
        outcome.ok,
        "structured: a non-zero exit is DATA · the task succeeds + the gate opens"
    );
    // The probe SUCCEEDED despite exit 3 (the one-obvious-way split).
    assert_eq!(outcome.records["probe"].status, TaskStatus::Success);
    // The output is the structured OBJECT — all three fields present + raw.
    assert_eq!(
        outcome.records["probe"].output,
        serde_json::json!({
            "stdout": "the answer\n",
            "stderr": "a warning\n",
            "exit_code": 3
        }),
        "tasks.probe.output is the structured object · not a stringified stdout"
    );
    // CEL field access resolved (the gate ran on `.output.exit_code == 3`).
    assert_eq!(outcome.records["gate"].status, TaskStatus::Success);
}

/// The negative half: a plain exec (no `capture: structured`) stays a
/// trailing-newline-trimmed STRING — the `tasks.X.output == '42'` ergonomic
/// is preserved (the structured object is opt-in via `capture:`).
#[tokio::test]
async fn exec_plain_capture_stays_a_trimmed_string() {
    let yaml = r#"
nika: v1
workflow: exec-plain
tasks:
  - id: e
    exec: { command: "printf 42" }
  - id: gate
    depends_on: [e]
    when: ${{ tasks.e.output == '42' }}
    exec: { command: "echo ok" }
"#;
    let shell = MockShell::new().enqueue_ok("42\n").enqueue_ok("ok\n");
    let (outcome, _events) = run_to_events(
        yaml,
        shell,
        MockToolExecutor::new(),
        MockProvider::new("mock"),
    )
    .await;

    assert!(outcome.ok, "plain exec stays a string · the gate opens");
    assert_eq!(
        outcome.records["e"].output,
        serde_json::Value::String("42".to_owned()),
        "plain exec output is a trimmed STRING · not a structured object"
    );
    assert_eq!(outcome.records["gate"].status, TaskStatus::Success);
}
