// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]

//! The conformance floor — the L3 rehearsal's fixture + assertions,
//! executed through the REAL runtime (`crates/nika-cli/tests/
//! e2e_pipeline.rs` played this layer before this crate existed · same
//! YAML in · same event stream out · the rehearsal's dispatch loop
//! retires in the follow-up flip).

use std::collections::BTreeMap;
use std::sync::Arc;

use nika_check::CheckReport;
use nika_check::check;
use nika_event::EventKind;
use nika_kernel::provider::{ContentBlock, InferResponse, StopReason, TokenUsage};
use nika_kernel::tool_executor::ToolResult;
use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::{NoHttp, ProviderRegistry, ProvidersConfig};
use nika_runtime::{DeterministicStamper, Runtime, RuntimeConfig, RuntimeError, VecSink};
use nika_schema::raw::RawWorkflow;
use nika_schema::{FileId, ParseMode, parse};
use nika_types::resource::Value;
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;

/// The rehearsal fixture · human-gated diamond DAG · 5 waves · 7 tasks ·
/// all three wave-dispatched verbs + a statically-closed gate. The YAML
/// mirrors `e2e_pipeline.rs`'s `WORKFLOW_OK` verbatim (same YAML in ·
/// same stream out) — when the trifecta law moves, both twins move.
const WORKFLOW_OK: &str = r#"
nika: e2e-veille

model: mock/echo

permits:
  tools: ["nika:prompt", "nika:read", "nika:write"]
  exec: ["wc", "echo"]
  fs:
    read: ["./news.json"]
    write: ["./out/report.md"]

const:
  source: "./news.json"
  publish: "no"

tasks:
  # The human gate (NEP-0002 · v2.2 made `exec:` born-ingress). `probe`'s
  # stdout is content this workflow did not author, it meets a private
  # `fs.read` and reaches `fs.write` + a second exec: the lethal trifecta.
  # The gate comes FIRST and BOTH roots descend from it — `gather` carries
  # the private read, `probe` the untrusted content — because a gate on a
  # sibling branch dominates nothing.
  approve:
    invoke:
      tool: "nika:prompt"
      args:
        mode: "confirm"
        message: "read ${{ const.source }}, summarize it and write the report?"

  gather:
    with:
      go: ${{ tasks.approve.output }}
    when: ${{ with.go == true }}
    invoke:
      tool: "nika:read"
      args: { path: "${{ const.source }}" }

  probe:
    with:
      go: ${{ tasks.approve.output }}
    when: ${{ with.go == true }}
    exec:
      command: ["wc", "-l", "./news.json"]

  extract:
    with:
      gathered: ${{ tasks.gather.output }}
    infer:
      prompt: "Extract the story fields · ${{ with.gathered }}"
      schema:
        type: object
        properties:
          headline: { type: string }
          score: { type: integer }
        required: [headline, score]

  think:
    with:
      gathered: ${{ tasks.gather.output }}
      lines: ${{ tasks.probe.output }}
    infer:
      prompt: "Summarize · ${{ with.gathered }} · lines ${{ with.lines }}"
      max_tokens: 800

  write_out:
    with:
      summary: ${{ tasks.think.output }}
    after:
      extract: success
    invoke:
      tool: "nika:write"
      args:
        path: "./out/report.md"
        content: "${{ with.summary }}"

  notify:
    after:
      write_out: terminal
    when: ${{ const.publish == 'yes' }}
    exec:
      command: ["echo", "done"]

outputs:
  report: ${{ tasks.write_out.output }}
"#;

const GATHER_JSON: &str = r#"{"headline":"Rust 2.0","score":9}"#;

fn parse_and_check(yaml: &str) -> (RawWorkflow, CheckReport) {
    let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
    let report = check(&wf);
    (wf, report)
}

type FloorRuntime = Runtime<
    MockShell,
    MockToolExecutor,
    NoHttp,
    MockProvider,
    MockToolDefinitionProvider,
    MockClock,
>;

/// Assemble the runtime exactly as the composer will: four verbs over
/// mock seams · the agent lane idle unless a fixture dispatches it.
fn floor_runtime(
    shell: MockShell,
    tools: MockToolExecutor,
    provider: MockProvider,
) -> FloorRuntime {
    floor_runtime_with_clock(shell, tools, provider, MockClock::new())
}

/// [`floor_runtime`] with the clock seam pinned by the caller — the
/// replay test runs twice and `MockClock::new()` captures wall time at
/// construction, so the approval ticket's `minted_at_ms` (and therefore
/// its digest on the `approval_decided` frame) would differ between
/// runs. One clock, cloned per run, keeps the mint an input.
fn floor_runtime_with_clock(
    shell: MockShell,
    tools: MockToolExecutor,
    provider: MockProvider,
    clock: MockClock,
) -> FloorRuntime {
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
        clock,
        RuntimeConfig::default(),
    )
}

/// A task's success output as text (the floor fixtures' string lanes).
fn output_str(outcome: &nika_runtime::RunOutcome, task: &str) -> String {
    match &outcome.records[task].output {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Field accessor · the stream's `note`/`task` string fields.
fn field<'a>(event: &'a nika_event::Event, key: &str) -> Option<&'a str> {
    event.fields.iter().find(|f| f.key == key).and_then(|f| {
        if let Value::String(s) = &f.value {
            Some(s.as_str())
        } else {
            None
        }
    })
}

/// `(kind, task, note)` storyboard rows for readable assertions.
fn storyboard(events: &[nika_event::Event]) -> Vec<(EventKind, String, String)> {
    events
        .iter()
        .map(|e| {
            (
                e.kind,
                field(e, "task").unwrap_or_default().to_owned(),
                field(e, "note").unwrap_or_default().to_owned(),
            )
        })
        .collect()
}

// ─── test 1 · the happy path · floor parity ─────────────────────────────

#[tokio::test]
async fn floor_happy_path_same_yaml_same_stream() {
    let (wf, report) = parse_and_check(WORKFLOW_OK);
    assert!(report.is_clean(), "fixture passes the ladder");

    let shell = MockShell::new().enqueue_ok("      42 ./news.json\n");
    let tools = MockToolExecutor::new()
        .enqueue_ok(
            // The affirmative gate reads the STRUCTURED plane, like the
            // real confirm (content stays the model-facing view).
            ToolResult::success("call-approve", "true").with_structured(serde_json::json!(true)),
        )
        .enqueue_ok(ToolResult::success("call-gather", GATHER_JSON))
        .enqueue_ok(ToolResult::success("call-write", "2.1 KB written"));
    let runtime = floor_runtime(shell, tools, MockProvider::new("mock"));

    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("clean run");
    let events = sink.into_events();

    assert!(outcome.ok, "happy path completes");

    // The storyboard the rehearsal locked: started · 7 scheduled · the
    // wave-ordered task frames · gated notify skipped · terminal.
    let kinds: Vec<EventKind> = events.iter().map(|e| e.kind).collect();
    assert_eq!(kinds[0], EventKind::WorkflowStarted);
    assert_eq!(kinds[1..=7], [EventKind::TaskScheduled; 7]);
    assert_eq!(
        *kinds.last().expect("non-empty"),
        EventKind::WorkflowCompleted
    );
    assert_eq!(
        kinds
            .iter()
            .filter(|k| **k == EventKind::TaskFailed)
            .count(),
        0
    );

    let rows = storyboard(&events);
    let notify_skip = rows
        .iter()
        .find(|(k, task, _)| *k == EventKind::TaskSkipped && task == "notify")
        .expect("notify gated");
    assert_eq!(notify_skip.2, "when: closed (post-gate)");

    // Per-verb notes (the display contract).
    let note_of = |task: &str| -> String {
        rows.iter()
            .find(|(k, t, _)| *k == EventKind::TaskCompleted && t == task)
            .map_or_else(|| panic!("{task} completed"), |(_, _, note)| note.clone())
    };
    assert_eq!(note_of("approve"), "invoke · nika:prompt");
    assert_eq!(note_of("gather"), "invoke · nika:read");
    assert_eq!(note_of("probe"), "exec · wc");
    assert!(note_of("extract").starts_with("infer · "));
    assert!(note_of("think").starts_with("infer · "));
    assert_eq!(note_of("write_out"), "invoke · nika:write");

    // Infer completions carry tokens.
    let think_completed = events
        .iter()
        .find(|e| e.kind == EventKind::TaskCompleted && field(e, "task") == Some("think"))
        .expect("think completed");
    assert!(
        think_completed.fields.iter().any(|f| f.key == "tokens"),
        "infer completion reports token usage"
    );

    // Dataflow proof · gather's JSON flowed through think into write_out ·
    // and the workflow output resolved from the final records.
    assert!(output_str(&outcome, "think").contains(GATHER_JSON));
    assert_eq!(outcome.outputs["report"], "2.1 KB written");

    // Records carry the spec-04 result fields (status · stamps · duration).
    let think = &outcome.records["think"];
    assert_eq!(think.status, nika_runtime::TaskStatus::Success);
    assert!(think.started_at.is_some() && think.ended_at.is_some());
    assert!(think.error.is_none());
}

// ─── test 2 · replay stability (the snapshot the runtime owns) ──────────

#[tokio::test]
async fn floor_stream_is_replay_stable() {
    // One clock for both runs — the approval mint stamps `minted_at_ms`
    // from the clock seam and signs it into the ticket digest; a fresh
    // `MockClock::new()` per run would leak wall time into the stream.
    let clock = MockClock::new();
    let run_once = || async {
        let (wf, report) = parse_and_check(WORKFLOW_OK);
        let shell = MockShell::new().enqueue_ok("      42 ./news.json\n");
        let tools = MockToolExecutor::new()
            .enqueue_ok(
                // The affirmative gate reads the STRUCTURED plane, like the
                // real confirm (content stays the model-facing view).
                ToolResult::success("call-approve", "true")
                    .with_structured(serde_json::json!(true)),
            )
            .enqueue_ok(ToolResult::success("call-gather", GATHER_JSON))
            .enqueue_ok(ToolResult::success("call-write", "2.1 KB written"));
        let runtime =
            floor_runtime_with_clock(shell, tools, MockProvider::new("mock"), clock.clone());
        let mut stamper = DeterministicStamper::new();
        let mut sink = VecSink::new();
        runtime
            .run(&wf, &report, &mut stamper, &mut sink)
            .await
            .expect("clean run");
        sink.into_events()
    };
    let first = run_once().await;
    let second = run_once().await;
    assert_eq!(first, second, "same YAML in · same stream out · forever");

    let lines: Vec<String> = storyboard(&first)
        .into_iter()
        .map(|(kind, task, note)| format!("{kind:?} · {task} · {note}"))
        .collect();
    insta::assert_yaml_snapshot!("floor_happy_path_storyboard", lines);
}

// ─── test 3 · the failure cascade ────────────────────────────────────────

#[tokio::test]
async fn floor_failure_cascades_and_terminal_is_failed() {
    let (wf, report) = parse_and_check(WORKFLOW_OK);

    // probe dies · think (needs probe) cascades · write_out (needs think)
    // cascades · notify (needs write_out) cascades BEFORE its gate ·
    // gather + extract stay alive (the other lane).
    let shell = MockShell::new().enqueue_fail(7, "disk full: /var/news");
    let tools = MockToolExecutor::new()
        .enqueue_ok(
            // The affirmative gate reads the STRUCTURED plane, like the
            // real confirm (content stays the model-facing view).
            ToolResult::success("call-approve", "true").with_structured(serde_json::json!(true)),
        )
        .enqueue_ok(ToolResult::success("call-gather", GATHER_JSON));
    let runtime = floor_runtime(shell, tools, MockProvider::new("mock"));

    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("the run itself is not an error");
    let events = sink.into_events();

    assert!(!outcome.ok, "a failed task fails the workflow");
    assert_eq!(
        *events.last().map(|e| &e.kind).expect("non-empty"),
        EventKind::WorkflowFailed
    );

    let rows = storyboard(&events);
    let failed: Vec<&str> = rows
        .iter()
        .filter(|(k, _, _)| *k == EventKind::TaskFailed)
        .map(|(_, t, _)| t.as_str())
        .collect();
    assert_eq!(failed, ["probe"], "exactly one root failure");

    // Spec 03 · a default-gate task whose deps can no longer all end
    // success/skipped is CANCELLED (the v1 floor said skipped — the
    // spec-parity engine upgraded the cascade class · §3.1 `◼`).
    let cancelled: BTreeMap<&str, &str> = rows
        .iter()
        .filter(|(k, _, _)| *k == EventKind::TaskCancelled)
        .map(|(_, t, note)| (t.as_str(), note.as_str()))
        .collect();
    assert_eq!(cancelled["think"], "gate: an edge did not admit");
    assert_eq!(cancelled["write_out"], "gate: an edge did not admit");
    assert!(
        !cancelled.contains_key("extract"),
        "the live lane completed"
    );
    // notify's explicit `when:` gets its evaluation (deps terminal ·
    // spec 05 always-pattern lane) — publish=no · the gate closes.
    let notify_skip = rows
        .iter()
        .find(|(k, t, _)| *k == EventKind::TaskSkipped && t == "notify")
        .expect("notify gate evaluated despite the dead upstream");
    assert_eq!(notify_skip.2, "when: closed (post-gate)");
    // The cancelled records read defined-null (spec 04).
    assert_eq!(
        outcome.records["think"].status,
        nika_runtime::TaskStatus::Cancelled
    );
    assert_eq!(outcome.records["think"].output, serde_json::Value::Null);

    // The failure detail carries the verb's wire code (B5 one-voice).
    let probe_failed = events
        .iter()
        .find(|e| e.kind == EventKind::TaskFailed)
        .expect("probe failed event");
    let detail = field(probe_failed, "detail").expect("detail field");
    assert!(detail.starts_with("NIKA-"), "wire code first: {detail}");
}

// ─── test 4 · audit-before-run (NIKA-1700) ──────────────────────────────

#[tokio::test]
async fn floor_dirty_report_never_executes() {
    // A cycle: a → b → a. The checker refuses · the runtime must too.
    let dirty = r"
nika: cycle
permits: { exec: ['true'] }
tasks:
  a:
    after: { b: success }
    exec: { command: ['true'] }
  b:
    after: { a: success }
    exec: { command: ['true'] }
";
    let wf = parse(dirty, FileId::new(0), ParseMode::Strict).expect("parses");
    let report = check(&wf);
    assert!(!report.is_clean(), "the cycle is caught by the ladder");

    let shell = MockShell::new();
    let tools = MockToolExecutor::new();
    let runtime = floor_runtime(shell, tools, MockProvider::new("mock"));

    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let err = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect_err("a dirty workflow never executes");
    assert!(matches!(err, RuntimeError::DirtyReport));
    assert!(sink.events().is_empty(), "zero events before the refusal");
}

// ─── test 5 · the agent lane through the YAML path ──────────────────────

#[tokio::test]
async fn floor_agent_task_dispatches_through_the_runtime() {
    let yaml = r#"
nika: agent-lane
model: mock/echo
tasks:
  ask:
    agent:
      prompt: "finish in one turn"
"#;
    let (wf, report) = parse_and_check(yaml);
    assert!(report.is_clean(), "agent fixture passes the ladder");

    // A tool-less scripted loop: one text turn ends it.
    let provider = MockProvider::new("mock").enqueue_response(InferResponse::new(
        vec![ContentBlock::Text {
            text: "done thinking".to_owned(),
        }],
        TokenUsage::new(11, 7),
        StopReason::EndTurn,
    ));
    let runtime = floor_runtime(MockShell::new(), MockToolExecutor::new(), provider);

    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("clean run");
    let events = sink.into_events();

    assert!(outcome.ok);
    assert_eq!(output_str(&outcome, "ask"), "done thinking");
    let completed = events
        .iter()
        .find(|e| e.kind == EventKind::TaskCompleted)
        .expect("ask completed");
    assert_eq!(field(completed, "note"), Some("agent · 1 turns"));
    assert!(
        completed.fields.iter().any(|f| f.key == "tokens"),
        "agent completion reports cumulative tokens"
    );
}

// ─── test 6 · when: literal gates ───────────────────────────────────────

#[tokio::test]
async fn floor_when_literal_true_runs_and_false_skips() {
    let yaml = r"
nika: gates
permits: { exec: ['echo'] }
tasks:
  always:
    when: true
    exec: { command: ['echo', 'a'] }
  never:
    when: false
    exec: { command: ['echo', 'b'] }
";
    let (wf, report) = parse_and_check(yaml);
    assert!(report.is_clean());

    let shell = MockShell::new().enqueue_ok("a\n");
    let runtime = floor_runtime(shell, MockToolExecutor::new(), MockProvider::new("mock"));

    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("clean run");

    assert!(outcome.ok);
    assert_eq!(output_str(&outcome, "always"), "a");
    // The skipped task HAS a record (defined-null reads · spec 04) —
    // status skipped · output null.
    assert_eq!(
        outcome.records["never"].status,
        nika_runtime::TaskStatus::Skipped
    );
    assert_eq!(outcome.records["never"].output, serde_json::Value::Null);
    let rows = storyboard(&sink.into_events());
    let never_skip = rows
        .iter()
        .find(|(k, t, _)| *k == EventKind::TaskSkipped && t == "never")
        .expect("never skipped");
    assert_eq!(never_skip.2, "when: closed (post-gate)");
}

// ─── test 7 · mutant killers (each MISSED mutant was a real hole) ────────

/// Kills the `envelope_strings` Typed-arm delete + the
/// `resolve_outputs` Typed-arm delete · a TYPED var default + a TYPED
/// output form must both flow.
#[tokio::test]
async fn floor_typed_var_default_and_typed_output_resolve() {
    let yaml = r"
nika: typed-forms
permits: { exec: true }
const:
  greeting:
    type: string
    value: 'bonjour'
tasks:
  say:
    exec: { shell: 'echo ${{ const.greeting }}' }
outputs:
  said:
    value: ${{ tasks.say.output }}
    type: string
";
    let (wf, report) = parse_and_check(yaml);
    assert!(report.is_clean(), "typed-forms fixture passes the ladder");

    let shell = MockShell::new().enqueue_ok("bonjour\n");
    let runtime = floor_runtime(shell, MockToolExecutor::new(), MockProvider::new("mock"));
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("clean run");

    assert!(outcome.ok);
    assert_eq!(outcome.outputs["said"], "bonjour", "typed output resolved");
}

/// The CEL milestone SUPERSEDES the old "deep path = NIKA-1703" floor:
/// a DEEP output path in a verb body (`system:` · `prompt:`) is
/// statically valid (the ladder proves it against the declared
/// `schema:` · ADR-092 #4) AND now RESOLVES at run time through
/// `nika-cel`. The task must SUCCEED · never NIKA-1703.
///
/// Also exercises `render_opt` (the `system:` optional field) on a
/// deep path: the deep reference is mirrored into the OBSERVABLE
/// `prompt:` (the mock echoes the user message, not the system), so a
/// `render`/`resolve_expr` mutant that drops or blanks the resolved
/// value is caught by the output assertion — and a `render_opt` that
/// errored would fail the task (which we assert succeeds).
#[tokio::test]
async fn floor_deep_path_system_template_resolves() {
    let yaml = r#"
nika: deep-ref
model: mock/echo
tasks:
  extract:
    infer:
      prompt: 'extract from {"headline":"Rust 2.0"}'
      schema:
        type: object
        properties:
          headline: { type: string }
        required: [headline]
  think:
    with: { headline: "${{ tasks.extract.output.headline }}" }
    infer:
      prompt: "headline is ${{ with.headline }}"
      system: "${{ with.headline }}"
"#;
    let (wf, report) = parse_and_check(yaml);
    assert!(
        report.is_clean(),
        "deep path proves against the declared schema · the evaluator owns the resolution"
    );

    let runtime = floor_runtime(
        MockShell::new(),
        MockToolExecutor::new(),
        MockProvider::new("mock"),
    );
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("clean run");
    let events = sink.into_events();

    assert!(
        outcome.ok,
        "a deep-path verb body now resolves · the workflow succeeds"
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| e.kind == EventKind::TaskFailed)
            .count(),
        0,
        "no task fails · the deep path is no longer NIKA-1703"
    );
    // The deep-path reference RESOLVED into the prompt (mock/echo with a
    // schema SYNTHESIZES the extract output — `{"headline":"mock"}` ·
    // JsonMode::Schema — so the substituted value is "mock"). The test's
    // subject is the RESOLUTION, not the mock's contents: an unresolved
    // template would echo the literal `${{ … }}` back.
    let think = output_str(&outcome, "think");
    assert!(
        think.contains("headline is mock") && !think.contains("${{"),
        "the deep-path reference resolved into think · got: {think}"
    );
}

/// The defined-null read (spec 04 §branch-join unlock) — the output of
/// a `when:`-skipped task reads as `null`, deterministically · the
/// downstream consumer RUNS (v1's floor failed this lane — the records
/// model unlocked it · the diamond-join pattern now works).
#[tokio::test]
async fn floor_skipped_upstream_reads_null_downstream() {
    let yaml = r#"
nika: gated-ref
model: mock/echo
permits: { exec: ['echo'] }
tasks:
  maybe:
    when: false
    exec: { command: ['echo', 'never'] }
  think:
    with:
      v: "${{ tasks.maybe.output }}"
      s: "${{ tasks.maybe.status }}"
    infer:
      prompt: "value is ${{ with.v }} · status ${{ with.s }}"
"#;
    let (wf, report) = parse_and_check(yaml);
    assert!(report.is_clean());

    let runtime = floor_runtime(
        MockShell::new(),
        MockToolExecutor::new(),
        MockProvider::new("mock"),
    );
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("clean run");

    assert!(outcome.ok, "defined-null · the join lane completes");
    // mock/echo echoes the prompt — both record fields resolved.
    let echoed = output_str(&outcome, "think");
    assert!(
        echoed.contains("value is null"),
        "skipped output reads null: {echoed}"
    );
    assert!(
        echoed.contains("status skipped"),
        "the status field resolves: {echoed}"
    );
}

// ─── test 8 · stress · deep chain + wide fan (the big-DAG shakeout) ─────

/// 24-deep sequential chain · each task pipes its predecessor's output ·
/// proves wave ordering, binding threading and event-count arithmetic at
/// depth (the floor fixture only reaches 4 waves).
#[tokio::test]
async fn stress_deep_chain_threads_bindings_in_order() {
    const DEPTH: usize = 24;
    use std::fmt::Write as _;
    let mut yaml = String::from("nika: deep-chain\npermits: { exec: true }\ntasks:\n");
    yaml.push_str("  t0:\n    exec: { command: ['step', '0'] }\n");
    for n in 1..DEPTH {
        let _ = writeln!(
            yaml,
            "  t{n}:\n    with: {{ up: \"${{{{ tasks.t{prev}.output }}}}\" }}\n    exec: {{ shell: 'step {n} after ${{{{ with.up }}}}' }}",
            prev = n - 1
        );
    }
    let (wf, report) = parse_and_check(&yaml);
    assert!(report.is_clean(), "deep chain passes the ladder");
    assert_eq!(report.waves.len(), DEPTH, "one wave per link");

    let mut shell = MockShell::new();
    for n in 0..DEPTH {
        shell = shell.enqueue_ok(format!("out-{n}\n"));
    }
    let runtime = floor_runtime(shell, MockToolExecutor::new(), MockProvider::new("mock"));
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("clean run");
    let events = sink.into_events();

    assert!(outcome.ok);
    assert_eq!(outcome.records.len(), DEPTH, "every link recorded");
    // Event arithmetic · 1 started + N scheduled + N×(started + the
    // NEP-0007 witness pair [exec gate + env composition] + completed)
    // + 1 terminal.
    assert_eq!(events.len(), 1 + DEPTH + 4 * DEPTH + 1);
    // Strict chain order · TaskStarted frames appear exactly t0..t23.
    let started: Vec<String> = storyboard(&events)
        .into_iter()
        .filter(|(k, _, _)| *k == EventKind::TaskStarted)
        .map(|(_, t, _)| t)
        .collect();
    let expected: Vec<String> = (0..DEPTH).map(|n| format!("t{n}")).collect();
    assert_eq!(started, expected, "the chain runs strictly in order");
    // Dataflow proof at depth · the last link bound its own output.
    assert_eq!(
        output_str(&outcome, &format!("t{}", DEPTH - 1)),
        format!("out-{}", DEPTH - 1)
    );
}

/// 12-wide fan-in · every source is independent (ONE wave) · the join
/// renders all twelve outputs into a single invoke arg.
#[tokio::test]
async fn stress_wide_fan_in_joins_every_source() {
    const WIDTH: usize = 12;
    use std::fmt::Write as _;
    let mut yaml = String::from(
        "nika: wide-fan\npermits: { exec: ['src'], tools: ['nika:write'], fs: { write: ['./out/joined.md'] } }\ntasks:\n",
    );
    for n in 0..WIDTH {
        let _ = writeln!(yaml, "  src{n}:\n    exec: {{ command: ['src', '{n}'] }}");
    }
    let bindings: Vec<String> = (0..WIDTH)
        .map(|n| format!("      s{n}: \"${{{{ tasks.src{n}.output }}}}\""))
        .collect();
    let parts: Vec<String> = (0..WIDTH)
        .map(|n| format!("${{{{ with.s{n} }}}}"))
        .collect();
    let _ = writeln!(
        yaml,
        "  join:\n    with:\n{}\n    invoke:\n      tool: \"nika:write\"\n      args: {{ path: \"./out/joined.md\", content: \"{}\" }}",
        bindings.join("\n"),
        parts.join("+")
    );
    let (wf, report) = parse_and_check(&yaml);
    assert!(report.is_clean(), "wide fan passes the ladder");
    assert_eq!(
        report.waves.len(),
        2,
        "sources are ONE parallel wave + the join"
    );
    assert_eq!(report.waves[0].len(), WIDTH);

    let mut shell = MockShell::new();
    for n in 0..WIDTH {
        shell = shell.enqueue_ok(format!("s{n}\n"));
    }
    let tools = MockToolExecutor::new().enqueue_ok(ToolResult::success("call-join", "joined"));
    let seam_tools = Arc::new(tools);
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::clone(&seam_tools)));
    let runtime: FloorRuntime = Runtime::new(
        ExecVerb::new(Arc::new(shell)),
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
        .expect("clean run");

    assert!(outcome.ok);
    // The join's rendered arg carries all twelve outputs in declaration order.
    let calls = seam_tools.captured_calls();
    assert_eq!(calls.len(), 1);
    let content = calls[0]
        .input
        .get("content")
        .and_then(serde_json::Value::as_str)
        .expect("join content");
    let expected: Vec<String> = (0..WIDTH).map(|n| format!("s{n}")).collect();
    assert_eq!(content, expected.join("+"), "all sources joined in order");
}
