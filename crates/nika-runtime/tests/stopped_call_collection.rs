// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
//! Real native parser/runtime plus fixture MCP workers; no service, credentials or provider network.
use nika_clock::SystemClock;
use nika_event::settlement::{StopOutcome, StoppedCall};
use nika_kernel::CancelCtx;
use nika_kernel::ai::provider::{ContentBlock, InferResponse, StopReason, TokenUsage, ToolDef};
use nika_kernel::tool_executor::{ToolCall, ToolResult};
use nika_kernel_mock::{MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_runtime::{DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, TaskStatus, VecSink};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::{InvokeVerb, McpPlane};
use std::sync::{Arc, Barrier, Mutex};
use std::time::Duration;

type Calls = Arc<Mutex<Vec<(String, String, serde_json::Value)>>>;
fn named(calls: &Calls, call: &ToolCall) {
    calls.lock().expect("fixture calls").push((
        call.name.clone(),
        call.id.to_string(),
        call.input.clone(),
    ));
}
fn wait_until_dropped(stop: &CancelCtx) {
    let until = std::time::Instant::now() + Duration::from_secs(5);
    while !stop.is_cancelled() && std::time::Instant::now() < until {
        std::thread::sleep(Duration::from_millis(2));
    }
}
async fn run(source: &str, plane: McpPlane, provider: Arc<MockProvider>) -> RunOutcome {
    run_with_cancel(source, plane, provider, None).await
}
async fn run_with_cancel(
    source: &str,
    plane: McpPlane,
    provider: Arc<MockProvider>,
    cancel: Option<CancelCtx>,
) -> RunOutcome {
    let wf = nika_schema::parse(
        source,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("native fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "{report:?}");
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    assert!(invoke.install_mcp_plane(plane));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        InferVerb::new(registry, "mock/echo"),
        AgentVerb::new(
            provider,
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/agent",
        ),
        SystemClock,
        RuntimeConfig::default(),
    );
    let runtime = match cancel {
        Some(cancel) => runtime.with_cancel(cancel),
        None => runtime,
    };
    runtime
        .run(
            &wf,
            &report,
            &mut DeterministicStamper::new(),
            &mut VecSink::new(),
        )
        .await
        .expect("settled run")
}
fn observed(calls: &Calls, only: Option<&str>) -> Vec<StoppedCall> {
    let mut expected: Vec<_> = calls
        .lock()
        .expect("fixture calls")
        .iter()
        .filter(|(_, _, args)| only.is_none_or(|value| args["item"] == value))
        .map(|(tool, id, _)| StoppedCall::new("a", tool, id, StopOutcome::Unknown))
        .collect();
    expected.sort_by(|a, b| a.call_id.cmp(&b.call_id));
    expected
}
fn exact_calls(mut actual: Vec<StoppedCall>, mut expected: Vec<StoppedCall>) {
    actual.sort_by(|a, b| a.call_id.cmp(&b.call_id));
    expected.sort_by(|a, b| a.call_id.cmp(&b.call_id));
    assert_eq!(
        actual, expected,
        "every actual physical call is named exactly once"
    );
}
#[tokio::test]
async fn every_timeout_in_a_multi_item_fanout_keeps_its_call() {
    let calls: Calls = Arc::default();
    let seen = Arc::clone(&calls);
    let plane = McpPlane::stoppable(
        Arc::new(move |call, stop| {
            assert!(stop.begin());
            named(&seen, &call);
            wait_until_dropped(stop);
            Ok(ToolResult::success(call.id.to_string(), "late"))
        }),
        Arc::new(Vec::new),
    );
    let source = r#"nika: fan-stops
permits: { tools: ["mcp:s/slow"] }
tasks:
  a:
    for_each: { items: ["alpha", "beta"], max_parallel: 2, fail_fast: false }
    timeout: "200ms"
    invoke: { tool: "mcp:s/slow", args: { item: "${{ item }}" } }
"#;
    let outcome = run(source, plane, Arc::new(MockProvider::new("mock"))).await;
    assert_eq!(outcome.records["a"].status, TaskStatus::Failure);
    assert_eq!(
        outcome.records["a"].error.as_ref().expect("timeout").code,
        "NIKA-TIMEOUT-001"
    );
    assert_eq!(calls.lock().unwrap().len(), 2);
    exact_calls(outcome.settlement.stopped_calls, observed(&calls, None));
}
#[tokio::test]
async fn fail_fast_keeps_dropped_sibling_calls_but_names_no_queued_call() {
    let calls: Calls = Arc::default();
    let seen = Arc::clone(&calls);
    let barrier = Arc::new(Barrier::new(2));
    let plane = McpPlane::stoppable(
        Arc::new(move |call, stop| {
            assert!(stop.begin());
            named(&seen, &call);
            barrier.wait();
            if call.input["item"] == "boom" {
                return Ok(ToolResult::error(call.id.to_string(), "ordinary failure"));
            }
            wait_until_dropped(stop);
            Ok(ToolResult::success(call.id.to_string(), "late"))
        }),
        Arc::new(Vec::new),
    );
    let source = r#"nika: fan-fail-fast
permits: { tools: ["mcp:s/slow"] }
tasks:
  a:
    for_each: { items: ["slow", "boom", "queued"], max_parallel: 2, fail_fast: true }
    invoke: { tool: "mcp:s/slow", args: { item: "${{ item }}" } }
"#;
    let outcome = run(source, plane, Arc::new(MockProvider::new("mock"))).await;
    assert_eq!(outcome.records["a"].status, TaskStatus::Failure);
    assert_eq!(calls.lock().unwrap().len(), 2);
    assert!(
        !calls
            .lock()
            .unwrap()
            .iter()
            .any(|(_, _, args)| args["item"] == "queued")
    );
    exact_calls(
        outcome.settlement.stopped_calls,
        observed(&calls, Some("slow")),
    );
}
#[tokio::test]
async fn agent_timeout_names_all_parallel_tool_calls() {
    let calls: Calls = Arc::default();
    let seen = Arc::clone(&calls);
    let plane = McpPlane::stoppable(
        Arc::new(move |call, stop| {
            assert!(stop.begin());
            named(&seen, &call);
            wait_until_dropped(stop);
            Ok(ToolResult::success(call.id.to_string(), "late"))
        }),
        Arc::new(|| {
            vec![ToolDef::new(
                "mcp:s/slow",
                "fixture tool",
                serde_json::json!({}),
            )]
        }),
    );
    let provider = Arc::new(
        MockProvider::new("mock").enqueue_response(InferResponse::new(
            vec![
                ContentBlock::ToolUse {
                    id: "agent-call-1".into(),
                    name: "mcp:s/slow".into(),
                    input: serde_json::json!({"source":"one"}),
                },
                ContentBlock::ToolUse {
                    id: "agent-call-2".into(),
                    name: "mcp:s/slow".into(),
                    input: serde_json::json!({"source":"two"}),
                },
            ],
            TokenUsage::default(),
            StopReason::ToolUse,
        )),
    );
    let source = r#"nika: agent-stops
model: mock/agent
permits: { tools: ["mcp:s/slow"] }
tasks:
  a:
    timeout: "200ms"
    agent: { prompt: "Use the two offered sources.", tools: ["mcp:s/slow"] }
"#;
    let outcome = run(source, plane, Arc::clone(&provider)).await;
    assert_eq!(outcome.records["a"].status, TaskStatus::Failure);
    assert_eq!(
        outcome.records["a"].error.as_ref().expect("timeout").code,
        "NIKA-TIMEOUT-001"
    );
    assert_eq!(provider.captured_requests().len(), 1);
    assert_eq!(calls.lock().unwrap().len(), 2);
    exact_calls(
        outcome.settlement.stopped_calls,
        vec![
            StoppedCall::new("a", "mcp:s/slow", "agent-call-1", StopOutcome::Unknown),
            StoppedCall::new("a", "mcp:s/slow", "agent-call-2", StopOutcome::Unknown),
        ],
    );
}

/// Uses only pre-existing public APIs, so its assertions are a behavioral red-first witness.
#[tokio::test]
async fn a_native_agent_observes_the_real_operator_stop_before_another_model_turn() {
    let cancel = CancelCtx::new();
    let operator = cancel.clone();
    let plane = McpPlane::stoppable(
        Arc::new(move |call, stop| {
            assert!(stop.begin());
            operator.cancel();
            Ok(ToolResult::stopped(
                &call,
                nika_kernel::tool_executor::StopOutcome::Unknown,
            ))
        }),
        Arc::new(|| {
            vec![ToolDef::new(
                "mcp:s/slow",
                "fixture tool",
                serde_json::json!({}),
            )]
        }),
    );
    let provider = Arc::new(
        MockProvider::new("mock")
            .enqueue_response(InferResponse::new(
                vec![ContentBlock::ToolUse {
                    id: "agent-operator-1".into(),
                    name: "mcp:s/slow".into(),
                    input: serde_json::json!({}),
                }],
                TokenUsage::default(),
                StopReason::ToolUse,
            ))
            .enqueue_text("must not be requested"),
    );
    let source = r#"nika: agent-operator-stop
model: mock/agent
permits: { tools: ["mcp:s/slow"] }
tasks:
  a:
    on_error: { recover: "must not become a resume success" }
    agent: { prompt: "Read the offered source.", tools: ["mcp:s/slow"] }
"#;
    let outcome = run_with_cancel(source, plane, Arc::clone(&provider), Some(cancel)).await;
    assert!(outcome.cancelled);
    assert_eq!(outcome.records["a"].status, TaskStatus::Cancelled);
    assert_eq!(
        outcome.records["a"].cause,
        nika_dataflow::TerminalCause::Operator
    );
    assert_eq!(outcome.records["a"].error, None);
    assert_eq!(outcome.records["a"].recovered_from, None);
    assert_eq!(provider.captured_requests().len(), 1);
    exact_calls(
        outcome.settlement.stopped_calls,
        vec![StoppedCall::new(
            "a",
            "mcp:s/slow",
            "agent-operator-1",
            StopOutcome::Unknown,
        )],
    );
}
