#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
//! The operator's cancellation ends a run with terminal frames (#1353):
//! observed at the wave boundary, the unstarted tasks settle as cancelled by
//! the operator, the run ends with `workflow_cancelled`, and the outcome says
//! so — never a trace that stops mid-flight. An `mcp:` call in flight is
//! stopped by it, never waited for.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use nika_event::EventKind;
use nika_event::settlement::{StopOutcome as SettledStopOutcome, StoppedCall};
use nika_kernel::tool_executor::{StopOutcome, ToolCall, ToolResult};
use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_runtime::{
    DeterministicStamper, Runtime, RuntimeConfig, TaskStatus, TerminalCause, VecSink,
};
use nika_types::cancel::CancelCtx;
use nika_types::resource::Value as FieldValue;
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::{InvokeVerb, McpPlane};

const TWO_WAVES: &str = "nika: w\ntasks:\n  a:\n    invoke: { tool: \"nika:jq\", args: { input: { x: 1 }, expression: \".\" } }\n  b:\n    with: { prev: \"${{ tasks.a.output }}\" }\n    invoke: { tool: \"nika:jq\", args: { input: \"${{ with.prev }}\", expression: \".\" } }\n";

fn field<'a>(
    sink: &'a VecSink,
    kind: EventKind,
    task: Option<&str>,
    key: &str,
) -> Option<&'a FieldValue> {
    sink.events()
        .iter()
        .filter(|e| e.kind == kind)
        .find(|e| {
            task.is_none_or(|t| {
                e.fields
                    .iter()
                    .any(|f| f.key == "task" && matches!(&f.value, FieldValue::String(s) if s == t))
            })
        })
        .and_then(|e| e.fields.iter().find(|f| f.key == key))
        .map(|f| &f.value)
}

#[tokio::test]
async fn a_cancelled_run_ends_with_terminal_frames_at_the_wave_boundary() {
    let wf = nika_schema::parse(
        TWO_WAVES,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "fixture passes the ladder");
    let cancel = CancelCtx::new();
    cancel.cancel();
    let tools = MockToolExecutor::new()
        .enqueue_ok(ToolResult::success("c1", "one"))
        .enqueue_ok(ToolResult::success("c2", "two"));
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
    .with_cancel(cancel);
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("the run settles, never errors, on a cancellation");

    assert!(!outcome.ok, "a cancelled run is not a success");
    assert!(
        outcome.cancelled,
        "the outcome says the operator cancelled it"
    );
    assert!(!outcome.budget_exceeded, "not a budget stop");
    let kinds: Vec<EventKind> = sink.events().iter().map(|e| e.kind).collect();
    assert!(
        kinds.contains(&EventKind::WorkflowCancelled),
        "the run ends with a terminal frame: {kinds:?}"
    );
    assert!(
        !kinds.contains(&EventKind::WorkflowFailed),
        "never a failure frame for a cancellation: {kinds:?}"
    );
    let note = field(&sink, EventKind::TaskCancelled, Some("b"), "note")
        .expect("the unstarted task settles");
    assert!(
        matches!(note, FieldValue::String(s) if s.contains("cancelled by the operator")),
        "{note:?}"
    );
    let outcome_json =
        field(&sink, EventKind::TaskCancelled, Some("b"), "outcome").expect("its record");
    assert!(
        matches!(outcome_json, FieldValue::String(s) if s.contains("operator")),
        "the cause is the operator: {outcome_json:?}"
    );
    // The summary rides the terminal (#1247): the status, the tally, the clock.
    let summary = |key: &str| field(&sink, EventKind::WorkflowCancelled, None, key);
    assert!(
        matches!(summary("status"), Some(FieldValue::String(s)) if s == "cancelled"),
        "the status names the cancellation"
    );
    assert!(
        matches!(summary("tasks_total"), Some(FieldValue::Int(2))),
        "two tasks"
    );
    assert!(
        matches!(summary("tasks_ok"), Some(FieldValue::Int(1))),
        "one completed"
    );
    assert!(
        matches!(summary("tasks_cancelled"), Some(FieldValue::Int(1))),
        "one cancelled"
    );
    assert!(
        summary("elapsed_ms").is_some(),
        "the run clock rides the terminal"
    );
}

/// A workflow whose first task calls a slow `mcp:` tool (with `on_error`,
/// when given) and whose second task waits for it.
fn mcp_then_jq(on_error: &str) -> String {
    format!(
        "nika: w\npermits:\n  tools: [\"mcp:s/slow\", \"nika:jq\"]\ntasks:\n  a:\n{on_error}    invoke: {{ tool: \"mcp:s/slow\" }}\n  b:\n    after: {{ a: success }}\n    invoke: {{ tool: \"nika:jq\", args: {{ input: {{ x: 1 }}, expression: \".\" }} }}\n"
    )
}

/// Run `mcp_then_jq(on_error)` while the operator stops the run during the
/// `mcp:` call: the outcome, the frames, and whether the call's own stop saw
/// the operator's, within five seconds.
async fn stop_mid_call(on_error: &str) -> (nika_runtime::RunOutcome, VecSink, bool) {
    let wf = nika_schema::parse(
        &mcp_then_jq(on_error),
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(
        report.is_clean(),
        "fixture passes the ladder: {:?}",
        report.findings
    );
    let cancel = CancelCtx::new();
    let operator = cancel.clone();
    let saw_stop = Arc::new(AtomicBool::new(false));
    let seen = Arc::clone(&saw_stop);
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    invoke.install_mcp_plane(McpPlane::stoppable(
        Arc::new(move |call: ToolCall, stop: &CancelCtx| {
            let _ = stop.begin();
            // The operator stops the run while the server works.
            operator.cancel();
            let started = Instant::now();
            while !stop.is_cancelled() && started.elapsed() < Duration::from_secs(10) {
                std::thread::sleep(Duration::from_millis(5));
            }
            seen.store(stop.is_cancelled(), Ordering::SeqCst);
            Ok(ToolResult::stopped(&call, StopOutcome::Unknown))
        }),
        Arc::new(Vec::new),
    ));
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
    .with_cancel(cancel);
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let started = Instant::now();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("the run settles, never errors, on a cancellation");
    let in_time = started.elapsed() < Duration::from_secs(5);
    (outcome, sink, saw_stop.load(Ordering::SeqCst) && in_time)
}

/// The operator's Stop reaches an `mcp:` call in flight (`with_cancel`
/// installs it on the invoke seam): the plane's call sees its own stop, the
/// task settles `cancelled/operator` with the stopped call beside its
/// terminal outcome; authored `on_error:` cannot turn control into success.
/// The run ends cancelled at the boundary and the settlement names the call.
#[tokio::test]
async fn an_operator_stop_cancels_an_inflight_mcp_call() {
    for on_error in [
        "",
        "    on_error: { skip: true }\n",
        "    on_error: { recover: \"fallback\" }\n",
    ] {
        let (outcome, sink, stopped_in_time) = stop_mid_call(on_error).await;
        assert!(
            stopped_in_time,
            "the call's own stop saw the operator's, and the run never waited for it"
        );
        assert!(outcome.cancelled, "the operator cancelled the run");
        let record = &outcome.records["a"];
        assert_eq!(record.status, TaskStatus::Cancelled, "{on_error}");
        assert_eq!(record.cause, TerminalCause::Operator);
        assert!(record.error.is_none());
        assert!(record.recovered_from.is_none());
        let terminal = sink
            .events()
            .iter()
            .find(|event| {
                event.kind == EventKind::TaskCancelled && event.str_field("task") == Some("a")
            })
            .expect("the interrupted task settles as cancelled");
        let call: serde_json::Value = serde_json::from_str(
            terminal
                .str_field("stopped_call")
                .expect("the call identity"),
        )
        .expect("the call is JSON");
        let details = serde_json::json!({"stopped_call": call});
        assert!(!sink.events().iter().any(|event| {
            event.str_field("task") == Some("a")
                && matches!(
                    event.kind,
                    EventKind::TaskCompleted | EventKind::TaskRetrying
                )
        }));
        assert_eq!(details["stopped_call"]["tool"], "mcp:s/slow");
        assert_eq!(details["stopped_call"]["outcome"], "unknown");
        let call_id = details["stopped_call"]["call_id"]
            .as_str()
            .expect("the call id");
        assert_eq!(
            outcome.settlement.stopped_calls,
            vec![StoppedCall::new(
                "a",
                "mcp:s/slow",
                call_id,
                SettledStopOutcome::Unknown
            )],
            "{on_error}"
        );
        let kinds: Vec<EventKind> = sink.events().iter().map(|e| e.kind).collect();
        assert!(
            kinds.contains(&EventKind::WorkflowCancelled),
            "the run ends cancelled: {kinds:?}"
        );
        assert!(
            field(&sink, EventKind::TaskCancelled, Some("b"), "note").is_some(),
            "the next task never started"
        );
    }
}
