// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]

//! The `unwind` battery (spec 03 · always-run cleanup) — extracted
//! from `spec_v2.rs` (the 1500-LOC ratchet) — every test runs the REAL
//! parse → check → run chain over mock seams (the floor discipline).

use std::num::NonZeroUsize;
use std::sync::Arc;

use nika_check::CheckReport;
use nika_check::check;
use nika_event::{Event, EventKind};
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

// ─── harness (same seams as spec_v2.rs) ──────────────────────────────────

fn parse_and_check(yaml: &str) -> (RawWorkflow, CheckReport) {
    let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
    let report = check(&wf);
    (wf, report)
}

fn runtime_with_tools<T: nika_kernel::tool_executor::ToolExecuteDyn>(
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

fn str_field<'a>(event: &'a Event, key: &str) -> Option<&'a str> {
    match event.fields.iter().find(|f| f.key == key).map(|f| &f.value) {
        Some(FieldValue::String(s)) => Some(s.as_str()),
        _ => None,
    }
}

// ─── the battery ─────────────────────────────────────────────────────────

#[tokio::test]
async fn unwind_runs_on_success_and_failure_and_routes_on_status() {
    let yaml = r#"
nika: cleanup
permits: { exec: true }
tasks:
  works:
    exec: { command: ["make", "thing"] }
  works_sweep:
    after: { works: unwind }
    exec: { command: ["rm", "-f", "scratch-a"] }
  breaks:
    exec: { command: ["make", "other"] }
  breaks_alert:
    after: { breaks: unwind }
    when: ${{ tasks.breaks.status == 'failure' }}
    exec: { command: ["alert", "on-call"] }
  breaks_sweep:
    after: { breaks: unwind }
    exec: { command: ["rm", "-f", "scratch-b"] }
  never_ran:
    after: { breaks: success }
    exec: { command: ["downstream"] }
  never_ran_sweep:
    after: { never_ran: unwind }
    exec: { command: ["must", "not", "run"] }
"#;
    // Queue: works · works-cleanup · breaks(FAIL) · breaks-cleanup-1
    // (alert · gate OPEN on failure) · breaks-cleanup-2 — and NOTHING
    // for never_ran (cancelled tasks run no cleanup · an extra dequeue
    // would panic the mock).
    let shell = MockShell::new()
        .enqueue_ok("made\n")
        .enqueue_ok("cleaned-a\n")
        .enqueue_fail(2, "make exploded")
        .enqueue_ok("alerted\n")
        .enqueue_ok("cleaned-b\n");
    let (outcome, _events) = run_to_events(
        yaml,
        shell,
        MockToolExecutor::new(),
        MockProvider::new("mock"),
        RuntimeConfig::new(NonZeroUsize::new(1), 0), // FIFO queue alignment
    )
    .await;

    assert!(!outcome.ok);
    assert_eq!(outcome.records["works"].status, TaskStatus::Success);
    assert_eq!(outcome.records["breaks"].status, TaskStatus::Failure);
    // The cancelled task ran NO cleanup — proven by the mock queue
    // being exactly drained (a 6th dequeue panics).
    assert_eq!(outcome.records["never_ran"].status, TaskStatus::Cancelled);
}

/// NEP-0007 law 2 · the cleanup lane ATTESTS too (the final review's
/// catch): a workflow whose only effect is an `on_finally:` exec must
/// carry `permit_checked` frames for it (the lane-local witness that
/// never drained is retired — decisions merge into the parent's stream).
#[tokio::test]
async fn unwind_decisions_are_attested_as_permit_checked() {
    let yaml = r#"
nika: attest-finally
model: mock/echo
permits: { exec: ["echo"] }
tasks:
  main:
    infer: { prompt: "hi" }
  main_sweep:
    after: { main: unwind }
    exec: { command: ["echo", "cleanup"] }
"#;
    let (_outcome, events) = run_to_events(
        yaml,
        MockShell::new().enqueue_ok("done\n"),
        MockToolExecutor::new(),
        MockProvider::new("mock"),
        RuntimeConfig::default(),
    )
    .await;
    // The lane marker rides the parent's stream (plane on_finally)…
    let lane = events
        .iter()
        .find(|e| {
            e.kind == nika_event::EventKind::PermitChecked
                && str_field(e, "plane") == Some("on_finally")
        })
        .expect("the on_finally lane marker is attested");
    assert_eq!(str_field(lane, "decision"), Some("attempt"));
    // …and the cleanup's exec decision itself (allow under the grant).
    let exec_frame = events
        .iter()
        .find(|e| {
            e.kind == nika_event::EventKind::PermitChecked && str_field(e, "plane") == Some("exec")
        })
        .expect("the cleanup exec decision is attested");
    assert_eq!(str_field(exec_frame, "decision"), Some("allow"));
    assert_eq!(str_field(exec_frame, "gate"), Some("echo"));
}

/// A cleanup error is swallowed — the parent's status reflects ONLY
/// the main verb (spec 03 · best-effort semantics).
#[tokio::test]
async fn unwind_errors_are_swallowed() {
    let yaml = r#"
nika: cleanup-err
permits: { exec: true }
tasks:
  main:
    exec: { command: ["work"] }
  main_sweep:
    after: { main: unwind }
    exec: { command: ["broken", "cleanup"] }
"#;
    let shell = MockShell::new()
        .enqueue_ok("worked\n")
        .enqueue_fail(13, "cleanup died");
    let (outcome, events) = run_to_events(
        yaml,
        shell,
        MockToolExecutor::new(),
        MockProvider::new("mock"),
        RuntimeConfig::default(),
    )
    .await;
    assert!(outcome.ok, "the cleanup failure never propagates");
    assert_eq!(outcome.records["main"].status, TaskStatus::Success);
    assert_eq!(
        events
            .iter()
            .filter(|e| e.kind == EventKind::TaskFailed)
            .count(),
        0
    );
}
