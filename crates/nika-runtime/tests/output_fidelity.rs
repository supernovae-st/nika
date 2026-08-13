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
    let report = nika_check::check(&wf);
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
nika: out-bind
permits: { exec: true, tools: ["nika:jq"] }
tasks:
  src:
    invoke: { tool: "nika:jq", args: { input: { count: 7 }, expression: "." } }
    extract: { c: ".count" }
  gate:
    with: { c: "${{ tasks.src.c }}" }
    when: ${{ with.c == 7 }}
    exec: { command: ["echo", "gated"] }
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
nika: out-bind-jq
permits: { exec: true, tools: ["nika:jq"] }
tasks:
  api:
    invoke: { tool: "nika:jq", args: { input: {}, expression: "." } }
    extract:
      n: ".data.users | length"
      emails: "[.data.users[].email]"
  use:
    with: { n: "${{ tasks.api.n }}" }
    when: ${{ with.n == 2 }}
    exec: { command: ["echo", "two"] }
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
nika: out-bind-card
permits: { tools: ["nika:jq"] }
tasks:
  src:
    invoke: { tool: "nika:jq", args: { input: { users: [1, 2, 3] }, expression: "." } }
    extract: { each: ".users[]" }
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
    // The message is code-less (the code rides its own field) — no double
    // render in tasks.X.error.message / the TaskFailed detail (wire_message).
    assert!(
        !err.message.contains("NIKA-VAR-002"),
        "the wire code must not double into the message: {}",
        err.message
    );
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
nika: out-bind-skip
permits: { exec: true, tools: ["nika:jq"] }
const:
  run: "no"
tasks:
  maybe:
    when: ${{ const.run == 'yes' }}
    invoke: { tool: "nika:jq", args: { input: { count: 5 }, expression: "." } }
    extract: { c: ".count" }
  join:
    with: { c: "${{ tasks.maybe.c }}" }
    when: ${{ with.c == null }}
    exec: { command: ["echo", "joined"] }
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
nika: exec-structured
permits: { exec: true }
tasks:
  probe:
    exec: { command: ["run-it"], capture: structured }
  gate:
    with: { code: "${{ tasks.probe.output.exit_code }}" }
    when: ${{ with.code == 3 }}
    exec: { command: ["echo", "branched"] }
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
nika: exec-plain
permits: { exec: true }
tasks:
  e:
    exec: { command: ["printf", "42"] }
  gate:
    with: { out: "${{ tasks.e.output }}" }
    when: ${{ with.out == '42' }}
    exec: { command: ["echo", "ok"] }
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

// ─── typed outputs: the contract is enforced at run end (NIKA-VAR-009) ──────

/// Spec 01 §engine-MUST rule 6: a typed `outputs:` value that does not match
/// its declared `type:` FAILS the run (the output half of the callable
/// contract). The task succeeds (a number), but the output declares `string`
/// → the run must fail, and the terminal frame carries the VAR-009 reason as
/// a WORKFLOW-level `detail` (no phantom `task_failed`, no spurious row).
#[tokio::test]
async fn typed_output_type_mismatch_fails_the_run_with_var009() {
    let yaml = r#"
nika: typed-out-mismatch
permits: { tools: ["nika:jq"] }
tasks:
  n:
    invoke: { tool: "nika:jq", args: { input: { x: 42 }, expression: ".x" } }
outputs:
  result:
    value: ${{ tasks.n.output }}
    type: string
"#;
    let tools = MockToolExecutor::new()
        .enqueue_ok(ToolResult::success("n", "42").with_structured(serde_json::json!(42)));
    let (outcome, events) =
        run_to_events(yaml, MockShell::new(), tools, MockProvider::new("mock")).await;
    // The one task settled OK, but the output breaks its declared type.
    assert_eq!(outcome.records["n"].status, TaskStatus::Success);
    assert!(
        !outcome.ok,
        "a number output declared `string` must fail the run (NIKA-VAR-009)"
    );
    // The reason rides the terminal frame — a workflow-level detail.
    let terminal = events
        .iter()
        .find(|e| e.kind == EventKind::WorkflowFailed)
        .expect("a workflow_failed terminal frame");
    assert!(
        str_field(terminal, "detail").is_some_and(|d| d.contains("NIKA-VAR-009")),
        "the terminal carries the NIKA-VAR-009 reason"
    );
    // No phantom task failure — the event model stays consistent.
    assert!(
        !events.iter().any(|e| e.kind == EventKind::TaskFailed),
        "the only task succeeded — no orphan task_failed for the outputs phase"
    );
}

// ─── the run banner tells the boundary truth (WorkflowStarted permits) ───────

/// A declared `permits:` block is a default-deny boundary the effects ENFORCE.
/// The run banner reads the `WorkflowStarted` `permits` field — it must NOT
/// keep reporting "engine floor (no boundary declared)" once a boundary is
/// present (the misleading display the operator could not visually trust).
#[tokio::test]
async fn run_banner_reflects_a_declared_permits_boundary() {
    let yaml = r#"
nika: permits-banner
permits:
  tools: ["nika:jq"]
tasks:
  t:
    invoke: { tool: "nika:jq", args: { input: { x: 1 }, expression: ".x" } }
"#;
    let tools = MockToolExecutor::new()
        .enqueue_ok(ToolResult::success("t", "1").with_structured(serde_json::json!(1)));
    let (_outcome, events) =
        run_to_events(yaml, MockShell::new(), tools, MockProvider::new("mock")).await;
    let started = events
        .iter()
        .find(|e| e.kind == EventKind::WorkflowStarted)
        .expect("a workflow_started frame");
    let permits = str_field(started, "permits").expect("a permits field on the frame");
    assert!(
        permits.contains("declared"),
        "a declared boundary must surface as one · got: {permits}"
    );
    assert!(
        !permits.contains("no boundary"),
        "MUST NOT claim 'no boundary declared' when permits are present · got: {permits}"
    );
}

/// The third sibling (user gauntlet 2026-07-31 · G-10 · Nina): with
/// `exec:` granted, `default-deny` would over-state — her sub-process
/// `grep` read files no `fs.read` admitted UNDER that very banner. The
/// banner names the opening instead; the fence claim returns only when
/// the engine binds sub-process I/O to the fs boundary (operator Q2 ·
/// spec-first).
#[tokio::test]
async fn run_banner_names_the_exec_opening_when_exec_granted() {
    let yaml = r#"
nika: exec-banner
permits:
  exec: true
  tools: ["nika:jq"]
tasks:
  t:
    invoke: { tool: "nika:jq", args: { input: { x: 1 }, expression: ".x" } }
"#;
    let tools = MockToolExecutor::new()
        .enqueue_ok(ToolResult::success("t", "1").with_structured(serde_json::json!(1)));
    let (_outcome, events) =
        run_to_events(yaml, MockShell::new(), tools, MockProvider::new("mock")).await;
    let started = events
        .iter()
        .find(|e| e.kind == EventKind::WorkflowStarted)
        .expect("a workflow_started frame");
    let permits = str_field(started, "permits").expect("a permits field on the frame");
    assert!(
        permits.contains("exec outside the fs bounds"),
        "an exec grant is named as the opening it is · got: {permits}"
    );
    assert!(
        !permits.contains("default-deny"),
        "MUST NOT claim default-deny while exec escapes the fs boundary · got: {permits}"
    );
}

/// The companion: with NO `permits:` block, the banner truthfully reports the
/// engine floor (the run is bounded by the engine's own default ceilings, not
/// a workflow-declared boundary) — the other half of the same display truth.
#[tokio::test]
async fn run_banner_reports_engine_floor_when_no_permits_declared() {
    let yaml = r#"
nika: no-permits-banner
tasks:
  t:
    invoke: { tool: "nika:jq", args: { input: { x: 1 }, expression: ".x" } }
"#;
    let tools = MockToolExecutor::new()
        .enqueue_ok(ToolResult::success("t", "1").with_structured(serde_json::json!(1)));
    let (_outcome, events) =
        run_to_events(yaml, MockShell::new(), tools, MockProvider::new("mock")).await;
    let started = events
        .iter()
        .find(|e| e.kind == EventKind::WorkflowStarted)
        .expect("a workflow_started frame");
    let permits = str_field(started, "permits").expect("a permits field on the frame");
    // F-O8 « absent = zero authority »: the banner names the new floor —
    // the pre-F-O8 « engine floor » (no wall at all) is retired.
    assert!(
        permits.contains("zero authority"),
        "no declared boundary → the zero-authority truth · got: {permits}"
    );
}

// ─── the run knows its source (WorkflowStarted workflow_sha256) ──────────────

/// The journal must name the DEFINITION it recorded: with a source
/// identity injected, `workflow_started` carries `workflow_sha256`
/// verbatim — replay/diff/fork surfaces can then prove « the file
/// changed since this run » instead of guessing from task shapes.
/// Without one (embedded/test callers), the field is ABSENT: no
/// source, no claim.
#[tokio::test]
async fn workflow_started_carries_the_source_identity_when_injected() {
    let yaml = r#"
nika: source-identity
permits: { tools: ["nika:jq"] }
tasks:
  t:
    invoke: { tool: "nika:jq", args: { input: { x: 1 }, expression: ".x" } }
"#;
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    let tools = MockToolExecutor::new()
        .enqueue_ok(ToolResult::success("t", "1").with_structured(serde_json::json!(1)));
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(tools)));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        InferVerb::new(registry, "mock/echo"),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        MockClock::new(),
        RuntimeConfig::default(),
    )
    .with_source_sha256("ab".repeat(32));
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("runs clean");
    let events = sink.into_events();
    let started = events
        .iter()
        .find(|e| e.kind == EventKind::WorkflowStarted)
        .expect("a workflow_started frame");
    assert_eq!(
        str_field(started, "workflow_sha256").expect("the identity field"),
        "ab".repeat(32),
    );

    // The companion truth: a PLAIN run makes no source claim.
    let (_outcome, events) = run_to_events(
        yaml,
        MockShell::new(),
        MockToolExecutor::new()
            .enqueue_ok(ToolResult::success("t", "1").with_structured(serde_json::json!(1))),
        MockProvider::new("mock"),
    )
    .await;
    let plain = events
        .iter()
        .find(|e| e.kind == EventKind::WorkflowStarted)
        .expect("a workflow_started frame");
    assert!(
        str_field(plain, "workflow_sha256").is_none(),
        "no source injected → no identity claim"
    );
}

// ─── the skip/cancel WHY rides the journal (skip-why · 2026-07-06) ───────────

/// « Why did this task not run? » must be answerable from the journal
/// alone: a `when:` gate that closes journals its own CEL text on the
/// `when` field; a default-gate cancellation names the first
/// unsatisfied dependency on `blocked_by`. Both additive.
#[tokio::test]
async fn skip_and_cancel_events_carry_their_why() {
    let yaml = r#"
nika: skip-why
permits: { exec: true, tools: ["nika:jq"] }
tasks:
  seed:
    invoke: { tool: "nika:jq", args: { input: { x: 1 }, expression: ".x" } }
  gated:
    with: { s: "${{ tasks.seed.status }}" }
    when: "${{ with.s == 'failure' }}"
    invoke: { tool: "nika:jq", args: { input: { x: 2 }, expression: ".x" } }
  doomed:
    after: { seed: success }
    exec: { command: ["false"] }
  downstream:
    after: { doomed: success }
    invoke: { tool: "nika:jq", args: { input: { x: 3 }, expression: ".x" } }
"#;
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "fixture passes the ladder");
    let tools = MockToolExecutor::new()
        .enqueue_ok(ToolResult::success("seed", "1").with_structured(serde_json::json!(1)));
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(tools)));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new().enqueue_fail(7, "boom"))),
        Arc::clone(&invoke),
        InferVerb::new(registry, "mock/echo"),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
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
        .expect("the run itself is not an error");
    assert!(!outcome.ok, "doomed fails the workflow");
    let events = sink.into_events();

    let skipped = events
        .iter()
        .find(|e| e.kind == EventKind::TaskSkipped)
        .expect("the gated task skips");
    assert_eq!(str_field(skipped, "task"), Some("gated"));
    assert_eq!(
        str_field(skipped, "when"),
        Some("${{ with.s == 'failure' }}"),
        "the gate's own CEL text answers the why (it reads the LOCAL binding)"
    );

    let cancelled = events
        .iter()
        .find(|e| e.kind == EventKind::TaskCancelled)
        .expect("the downstream task cancels");
    assert_eq!(str_field(cancelled, "task"), Some("downstream"));
    assert_eq!(
        str_field(cancelled, "blocked_by"),
        Some("doomed"),
        "the culprit upstream is named"
    );
}

// ─── the environment attestation rides the opening frame (Q11) ──────────────

/// Reproducing a failure needs WHICH engine on WHICH platform — the
/// prologue attests both, from compile-time constants only.
#[tokio::test]
async fn workflow_started_attests_engine_and_platform() {
    let yaml = "nika: attest\npermits: { exec: true }\ntasks:\n  a:\n    exec:\n      command: [\"true\"]\n";
    let (_outcome, events) = run_to_events(
        yaml,
        MockShell::new(),
        MockToolExecutor::new(),
        MockProvider::new("mock"),
    )
    .await;
    let started = events
        .iter()
        .find(|e| e.kind == EventKind::WorkflowStarted)
        .expect("a workflow_started frame");
    let version = str_field(started, "engine_version").expect("engine_version attested");
    assert_eq!(
        version.split('.').count(),
        3,
        "semver shape (got {version})"
    );
    let platform = str_field(started, "platform").expect("platform attested");
    assert!(
        platform.contains('/') && !platform.starts_with('/'),
        "os/arch shape (got {platform})"
    );
}
