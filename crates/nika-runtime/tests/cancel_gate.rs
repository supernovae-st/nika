#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
//! The operator's cancellation ends a run with terminal frames (#1353):
//! observed at the wave boundary, the unstarted tasks settle as cancelled by
//! the operator, the run ends with `workflow_cancelled`, and the outcome says
//! so — never a trace that stops mid-flight.

use std::sync::Arc;

use nika_event::EventKind;
use nika_kernel::tool_executor::ToolResult;
use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_runtime::{DeterministicStamper, Runtime, RuntimeConfig, VecSink};
use nika_types::cancel::CancelCtx;
use nika_types::resource::Value as FieldValue;
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;

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
