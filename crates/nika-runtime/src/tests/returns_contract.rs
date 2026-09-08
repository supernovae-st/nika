// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `invoke.tool` must judge the produced value before publishing success.
//! These regressions run the checked workflow through the real jq dispatcher.

use std::sync::atomic::{AtomicUsize, Ordering};

use nika_kernel::tool_executor::{ToolCall, ToolExecError, ToolExecuteDyn, ToolResult};
use nika_kernel_mock::{
    MockClock, MockFs, MockHttp, MockProvider, MockShell, MockToolDefinitionProvider,
    MockToolExecutor,
};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use serde_json::json;

use super::*;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
type Builtins = nika_builtin::BuiltinDispatcher<
    MockFs,
    MockHttp,
    MockClock,
    nika_builtin::NullEmitter,
    nika_builtin::NonInteractive,
    nika_builtin::NoWorkflow,
>;

/// Count at the tool seam without substituting its output or its run context.
struct Counted<T> {
    inner: T,
    calls: AtomicUsize,
}

impl<T: ToolExecuteDyn> ToolExecuteDyn for Counted<T> {
    async fn execute(&self, call: ToolCall) -> Result<ToolResult, ToolExecError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.inner.execute(call).await
    }
}

fn builtins() -> Builtins {
    Builtins::new(
        Arc::new(MockFs::new()),
        Arc::new(MockHttp::new()),
        Arc::new(MockClock::new()),
        Arc::new(nika_builtin::NullEmitter::default()),
        Arc::new(nika_builtin::NonInteractive::default()),
        Arc::new(nika_builtin::NoWorkflow::default()),
    )
}

fn workflow(input: &Value, contract: Option<&str>, policy: &str) -> String {
    let returns = contract.map_or_else(String::new, |ty| format!("    returns: {ty}\n"));
    format!(
        "nika: invoke-returns\nmodel: mock/echo\npermits: {{ tools: [\"nika:jq\"] }}\ntasks:\n  probe:\n{returns}{policy}    invoke:\n      tool: \"nika:jq\"\n      args: {{ input: {input}, expression: \".\" }}\n    extract: {{ copied: \".\" }}\noutputs:\n  value: \"${{{{ tasks.probe.output }}}}\"\n"
    )
}

async fn run<T: ToolExecuteDyn>(yaml: &str, tools: T) -> TestResult<(RunOutcome, VecSink, usize)> {
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )?;
    let checked = nika_check::check(&wf);
    assert!(checked.is_clean(), "fixture checks: {:?}", checked.findings);
    let tools = Arc::new(Counted {
        inner: tools,
        calls: AtomicUsize::new(0),
    });
    let invoke = Arc::new(InvokeVerb::new(Arc::clone(&tools)));
    let provider = MockProvider::new("mock");
    let shell = MockShell::new();
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(shell.clone())),
        Arc::clone(&invoke),
        InferVerb::new(
            Arc::new(ProviderRegistry::without_http(ProvidersConfig::default())),
            "mock/echo",
        ),
        AgentVerb::new(
            Arc::new(provider.clone()),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        MockClock::new(),
        RuntimeConfig::default(),
    );
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime.run(&wf, &checked, &mut stamper, &mut sink).await?;
    assert!(provider.captured_requests().is_empty());
    assert!(shell.executed_commands().is_empty());
    Ok((outcome, sink, tools.calls.load(Ordering::Relaxed)))
}

fn field<'a>(event: &'a Event, key: &str) -> Option<&'a FieldValue> {
    event
        .fields
        .iter()
        .find(|kv| kv.key == key)
        .map(|kv| &kv.value)
}

fn text<'a>(event: &'a Event, key: &str) -> Option<&'a str> {
    match field(event, key) {
        Some(FieldValue::String(value)) => Some(value),
        _ => None,
    }
}

fn frame(sink: &VecSink, kind: EventKind) -> &Event {
    let frames: Vec<_> = sink.events().iter().filter(|e| e.kind == kind).collect();
    assert_eq!(frames.len(), 1, "exactly one {kind:?}");
    frames[0]
}

fn assert_fired(event: &Event) {
    let preview = text(event, "preview_digest");
    assert!(preview.is_some_and(|digest| digest.len() == 64));
    assert_eq!(preview, text(event, "commit_digest"));
    assert!(field(event, "divergence").is_none());
}

fn assert_success(outcome: &RunOutcome, sink: &VecSink, value: &Value) -> TestResult {
    assert!(outcome.ok);
    assert_eq!(outcome.settlement.state, RunState::Succeeded);
    let record = &outcome.records["probe"];
    assert_eq!(record.status, TaskStatus::Success);
    assert_eq!(record.cause, TerminalCause::Normal);
    assert_eq!(record.attempts, Some(1));
    assert!(record.error.is_none());
    assert_eq!(&record.output, value);
    assert_eq!(&record.named["copied"], value);
    assert_eq!(&outcome.outputs["value"], value);
    let completed = frame(sink, EventKind::TaskCompleted);
    assert_fired(completed);
    let receipt: Value =
        serde_json::from_str(text(completed, "outcome").ok_or("outcome missing")?)?;
    assert_eq!(&receipt["payload"]["value"], value);
    frame(sink, EventKind::WorkflowCompleted);
    Ok(())
}

fn assert_rejected(outcome: &RunOutcome, sink: &VecSink) -> TestResult {
    assert!(!outcome.ok);
    assert_eq!(outcome.settlement.state, RunState::Failed);
    let record = &outcome.records["probe"];
    assert_eq!(record.status, TaskStatus::Failure);
    assert_eq!(record.cause, TerminalCause::VerbError);
    assert_eq!(record.attempts, Some(1));
    let error = record.error.as_ref().ok_or("error missing")?;
    assert_eq!(error.code, "NIKA-TYPE-101");
    assert!(!error.transient);
    let code = nika_pack::error_codes()
        .into_iter()
        .find(|code| code.code == error.code)
        .ok_or("error absent from the canon")?;
    assert_eq!(code.category, "validation_error");
    assert_eq!(code.transient, "false");
    assert!(error.message.contains("does not fit `returns:`"));
    assert!(record.output.is_null());
    // Failed tasks retain defined-null bindings (spec 04), never the
    // rejected value. The binding's existence is not a success claim.
    assert_eq!(record.named["copied"], Value::Null);
    assert!(outcome.outputs.values().all(Value::is_null));
    assert!(record.recovered_from.is_none());
    let tally = outcome.settlement.tasks.as_ref().ok_or("tally missing")?;
    assert_eq!(tally.total, 1);
    assert_eq!(tally.ok, 0);
    assert_eq!(tally.failed, 1);
    assert_eq!(tally.recovered, 0);
    assert_eq!(tally.skipped, 0);
    assert!(!sink.events().iter().any(|event| matches!(
        event.kind,
        EventKind::TaskCompleted | EventKind::TaskRecovered | EventKind::TaskSkipped
    )));
    frame(sink, EventKind::TaskStarted);
    let failed = frame(sink, EventKind::TaskFailed);
    assert_fired(failed);
    let receipt: Value = serde_json::from_str(text(failed, "outcome").ok_or("outcome missing")?)?;
    assert_eq!(receipt["payload"]["error"]["code"], "NIKA-TYPE-101");
    assert_eq!(receipt["payload"]["error"]["transient"], false);
    assert_eq!(receipt["payload"]["attempts"], 1);
    assert!(receipt["payload"].get("value").is_none());
    frame(sink, EventKind::WorkflowFailed);
    Ok(())
}

const OPTIONAL_STRING: &str = "{ object: { x: { optional: string } } }";
const RETRY: &str =
    "    retry: { max_attempts: 3, backoff_ms: 1, backoff_strategy: fixed, jitter: false }\n";

#[tokio::test]
async fn optional_string_accepts_absence_and_present_string() -> TestResult {
    for value in [json!({}), json!({"x": "alpha"})] {
        let (outcome, sink, calls) =
            run(&workflow(&value, Some(OPTIONAL_STRING), ""), builtins()).await?;
        assert_eq!(calls, 1);
        assert_success(&outcome, &sink, &value)?;
        assert_eq!(outcome.priced_calls, 0);
        assert_eq!(outcome.unpriced_calls, 0);
    }
    Ok(())
}

#[tokio::test]
async fn optional_string_rejects_present_null() -> TestResult {
    let (outcome, sink, calls) = run(
        &workflow(&json!({"x": null}), Some(OPTIONAL_STRING), ""),
        builtins(),
    )
    .await?;
    assert_eq!(calls, 1);
    assert_rejected(&outcome, &sink)
}

#[tokio::test]
async fn optional_string_rejects_present_number() -> TestResult {
    let (outcome, sink, calls) = run(
        &workflow(&json!({"x": 17}), Some(OPTIONAL_STRING), ""),
        builtins(),
    )
    .await?;
    assert_eq!(calls, 1);
    assert_rejected(&outcome, &sink)
}

#[tokio::test]
async fn integer_contract_rejects_a_jq_string_without_coercion() -> TestResult {
    let (outcome, sink, calls) =
        run(&workflow(&json!("17"), Some("integer"), ""), builtins()).await?;
    assert_eq!(calls, 1);
    assert_rejected(&outcome, &sink)
}

#[tokio::test]
async fn absent_contract_preserves_builtin_values() -> TestResult {
    for value in [json!({"x": null}), json!({"x": 17}), json!("17")] {
        let (outcome, sink, calls) = run(&workflow(&value, None, ""), builtins()).await?;
        assert_eq!(calls, 1);
        assert_success(&outcome, &sink, &value)?;
    }
    Ok(())
}

#[tokio::test]
async fn explicit_nullable_union_accepts_present_null() -> TestResult {
    let value = json!({"x": null});
    let contract = "{ object: { x: { optional: { union: [string, null] } } } }";
    let (outcome, sink, calls) = run(&workflow(&value, Some(contract), ""), builtins()).await?;
    assert_eq!(calls, 1);
    assert_success(&outcome, &sink, &value)
}

#[tokio::test]
async fn validation_error_does_not_automatically_repeat_the_tool_call() -> TestResult {
    let (outcome, sink, calls) = run(
        &workflow(&json!({"x": null}), Some(OPTIONAL_STRING), RETRY),
        builtins(),
    )
    .await?;
    assert_eq!(
        calls, 1,
        "three allowed attempts do not replay this failure"
    );
    assert_rejected(&outcome, &sink)
}

#[tokio::test]
async fn on_error_recovers_type_101_and_preserves_incurred_spend() -> TestResult {
    let policy = format!(
        "{RETRY}    on_error: {{ on_codes: [NIKA-TYPE-101], recover: {{ x: repaired, cost_usd: 0 }} }}\n"
    );
    let contract = "{ object: { x: { optional: string }, cost_usd: number } }";
    let (outcome, sink, calls) = run(
        &workflow(
            &json!({"x": null, "cost_usd": 0.125}),
            Some(contract),
            &policy,
        ),
        builtins(),
    )
    .await?;
    assert_eq!(calls, 1);
    assert!(outcome.ok);
    let record = &outcome.records["probe"];
    assert_eq!(record.status, TaskStatus::Success);
    assert_eq!(record.cause, TerminalCause::Recovered);
    assert_eq!(record.attempts, Some(1));
    let original = record.recovered_from.as_ref().ok_or("recovery missing")?;
    assert_eq!(original.code, "NIKA-TYPE-101");
    assert!(!original.transient);
    assert_eq!(record.output, json!({"x": "repaired", "cost_usd": 0}));
    assert_eq!(outcome.outputs["value"], record.output);
    assert_eq!(outcome.total_cost_usd, Some(0.125));
    assert_eq!(outcome.priced_calls, 1);
    assert_eq!(outcome.unpriced_calls, 0);
    assert!((outcome.settlement.spend.by_source["nika:jq"] - 0.125).abs() < f64::EPSILON);
    let recovered = frame(&sink, EventKind::TaskRecovered);
    assert_eq!(text(recovered, "code"), Some("NIKA-TYPE-101"));
    let completed = frame(&sink, EventKind::TaskCompleted);
    assert_fired(completed);
    assert_eq!(
        field(completed, "cost_usd"),
        Some(&FieldValue::Float(0.125))
    );
    assert_eq!(
        outcome
            .settlement
            .tasks
            .as_ref()
            .ok_or("tally missing")?
            .recovered,
        1
    );
    Ok(())
}

#[tokio::test]
async fn rejected_output_keeps_the_fired_call_and_cost_in_the_receipt() -> TestResult {
    let contract = "{ object: { x: { optional: string }, cost_usd: number } }";
    for x in [json!("alpha"), Value::Null] {
        let value = json!({"x": x, "cost_usd": 0.125});
        let (outcome, sink, calls) =
            run(&workflow(&value, Some(contract), RETRY), builtins()).await?;
        assert_eq!(calls, 1);
        if x.is_null() {
            assert_rejected(&outcome, &sink)?;
        } else {
            assert_success(&outcome, &sink, &value)?;
        }
        assert_eq!(outcome.total_cost_usd, Some(0.125));
        assert_eq!(outcome.priced_calls, 1);
        assert_eq!(outcome.unpriced_calls, 0);
        assert!((outcome.settlement.spend.by_source["nika:jq"] - 0.125).abs() < f64::EPSILON);
        let kind = if x.is_null() {
            EventKind::TaskFailed
        } else {
            EventKind::TaskCompleted
        };
        assert_eq!(
            field(frame(&sink, kind), "cost_usd"),
            Some(&FieldValue::Float(0.125))
        );
    }
    Ok(())
}

#[tokio::test]
async fn text_only_tool_result_is_checked_as_text() -> TestResult {
    for contract in ["string", "integer"] {
        let executor = MockToolExecutor::new().enqueue_ok(ToolResult::success("tc1", "17"));
        let (outcome, sink, calls) =
            run(&workflow(&Value::Null, Some(contract), ""), executor).await?;
        assert_eq!(calls, 1);
        if contract == "string" {
            assert_success(&outcome, &sink, &json!("17"))?;
        } else {
            assert_rejected(&outcome, &sink)?;
        }
    }
    Ok(())
}
