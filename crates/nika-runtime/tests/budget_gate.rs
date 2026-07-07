// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Cost Intelligence (2026-07-08) — the run ledger + `--max-cost-usd`
//! through the REAL parse → check → run chain: totals on the terminal
//! frame, block-before budget semantics (the crossing call completes
//! and counts · unstarted work cancels · NIKA-1704 carries both
//! numbers), and the no-fake-zero law at every level.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use nika_kernel::tool_executor::ToolResult;
use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;

use nika_event::EventKind;
use nika_runtime::{DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, VecSink};
use nika_types::resource::Value as FieldValue;

/// A tool result whose structured value reports real spend — the
/// honest-spend channel (`nika:image_generate` on tick-billed
/// providers rides exactly this shape).
fn priced_result(id: &str, usd: f64) -> ToolResult {
    ToolResult::success(id, "done")
        .with_structured(serde_json::json!({ "cost_usd": usd, "ok": true }))
}

async fn run_yaml(
    yaml: &str,
    tools: MockToolExecutor,
    budget: Option<f64>,
) -> (RunOutcome, VecSink) {
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_schema::check(&wf);
    assert!(
        report.is_clean(),
        "fixture passes the ladder: {}",
        serde_json::to_string(&report).unwrap_or_default()
    );

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
        RuntimeConfig::default().with_max_cost_usd(budget),
    );
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("clean run");
    (outcome, sink)
}

fn terminal_field<'a>(sink: &'a VecSink, kind: EventKind, key: &str) -> Option<&'a FieldValue> {
    sink.events()
        .iter()
        .find(|e| e.kind == kind)
        .and_then(|e| e.fields.iter().find(|f| f.key == key))
        .map(|f| &f.value)
}

const ONE_TOOL: &str = "nika: v1\nworkflow: w\ntasks:\n  - id: render\n    invoke: { tool: \"nika:jq\", args: { input: { x: 1 }, expression: \".\" } }\n";

#[tokio::test]
async fn tool_spend_totals_ride_the_terminal_frame() {
    let tools = MockToolExecutor::new().enqueue_ok(priced_result("c1", 0.02));
    let (outcome, sink) = run_yaml(ONE_TOOL, tools, None).await;
    assert!(outcome.ok);
    assert_eq!(outcome.total_cost_usd, Some(0.02));
    assert_eq!(outcome.priced_calls, 1);
    assert!(!outcome.budget_exceeded);

    let total = terminal_field(&sink, EventKind::WorkflowCompleted, "total_cost_usd")
        .expect("the metered total rides the terminal frame");
    assert!(matches!(total, FieldValue::Float(c) if (*c - 0.02).abs() < 1e-12));
    // The point-in-time honesty stamp: WHICH prices billed this run.
    let as_of = terminal_field(&sink, EventKind::WorkflowCompleted, "pricing_as_of")
        .expect("the snapshot date rides next to the total");
    assert!(
        matches!(as_of, FieldValue::String(s) if s == nika_catalog::pricing_snapshot().as_of),
        "as_of mirrors the vendored snapshot"
    );
    let by_source = terminal_field(&sink, EventKind::WorkflowCompleted, "cost_by_source")
        .expect("the by-source breakdown rides");
    assert!(
        matches!(by_source, FieldValue::String(s) if s.contains("nika:jq")),
        "tool spend attributes to the tool id"
    );
}

#[tokio::test]
async fn unmetered_run_terminal_frame_is_field_free() {
    // A run that spends nothing METERED carries NO cost fields at
    // all — a `total_cost_usd: 0.0` nobody metered would be the
    // fake zero at the run level.
    let tools = MockToolExecutor::new()
        .enqueue_ok(ToolResult::success("c1", "plain text · no spend report"));
    let (outcome, sink) = run_yaml(ONE_TOOL, tools, None).await;
    assert!(outcome.ok);
    assert_eq!(outcome.total_cost_usd, None);
    assert_eq!(outcome.priced_calls, 0);
    assert!(
        terminal_field(&sink, EventKind::WorkflowCompleted, "total_cost_usd").is_none(),
        "absent is honest — no total field on an unmetered run"
    );
}

#[tokio::test]
async fn budget_blocks_the_next_wave_and_names_both_numbers() {
    // Two waves (b reads a) · the first task's spend crosses the
    // budget → in-flight completed and counted, the NEXT wave never
    // dispatches: b cancels, the run fails NIKA-1704 with
    // spent-vs-budget (the LiteLLM error shape).
    let yaml = "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    invoke: { tool: \"nika:jq\", args: { input: { x: 1 }, expression: \".\" } }\n  - id: b\n    depends_on: [a]\n    invoke: { tool: \"nika:jq\", args: { input: \"${{ tasks.a.output }}\", expression: \".\" } }\n";
    let tools = MockToolExecutor::new()
        .enqueue_ok(priced_result("c1", 0.06))
        .enqueue_ok(priced_result("c2", 0.06));
    let (outcome, sink) = run_yaml(yaml, tools, Some(0.05)).await;

    assert!(!outcome.ok, "a budget stop is a run failure");
    assert!(outcome.budget_exceeded);
    assert_eq!(
        outcome.total_cost_usd,
        Some(0.06),
        "the crossing call COUNTS"
    );
    assert_eq!(
        outcome.records["b"].status,
        nika_runtime::TaskStatus::Cancelled,
        "the unstarted task cancels"
    );
    let detail = terminal_field(&sink, EventKind::WorkflowFailed, "detail")
        .expect("the failed frame explains");
    let FieldValue::String(detail) = detail else {
        panic!("detail is text")
    };
    assert!(detail.contains("NIKA-1704"), "{detail}");
    assert!(detail.contains("0.06"), "spent rides the message: {detail}");
    assert!(
        detail.contains("0.05"),
        "budget rides the message: {detail}"
    );
    // The cancelled task's note names the budget.
    let cancel_note = sink
        .events()
        .iter()
        .find(|e| e.kind == EventKind::TaskCancelled)
        .and_then(|e| e.fields.iter().find(|f| f.key == "note"))
        .expect("cancelled frame carries a note");
    assert!(
        matches!(&cancel_note.value, FieldValue::String(s) if s.contains("--max-cost-usd")),
        "the WHY rides the cancel"
    );
}

#[tokio::test]
async fn budget_starves_a_serial_fan_out_mid_flight() {
    // `max_parallel: 1` serializes admission: 0.03 + 0.03 crosses
    // the 0.05 budget after the second iteration — the third is
    // never admitted, the task fails NIKA-1704 naming the shortfall.
    let yaml = "nika: v1\nworkflow: w\ntasks:\n  - id: fan\n    for_each: [1, 2, 3]\n    max_parallel: 1\n    invoke: { tool: \"nika:jq\", args: { input: \"${{ item }}\", expression: \".\" } }\n";
    let tools = MockToolExecutor::new()
        .enqueue_ok(priced_result("c1", 0.03))
        .enqueue_ok(priced_result("c2", 0.03))
        .enqueue_ok(priced_result("c3", 0.03));
    let (outcome, _sink) = run_yaml(yaml, tools, Some(0.05)).await;

    assert!(!outcome.ok);
    assert!(outcome.budget_exceeded);
    assert_eq!(
        outcome.total_cost_usd,
        Some(0.06),
        "two admitted iterations counted · the third never spent"
    );
    let error = outcome.records["fan"]
        .error
        .as_ref()
        .expect("the starved fan-out fails");
    assert_eq!(error.code, "NIKA-1704");
    assert!(
        error.message.contains("1 iteration(s)"),
        "names the denied count: {}",
        error.message
    );
}

#[tokio::test]
async fn unpriced_spend_never_trips_a_budget() {
    // A mock-provider infer reports usage but prices to NOTHING —
    // the budget bounds METERED spend only (said loudly at the
    // preflight), so the run completes and the unpriced count rides.
    let yaml = "nika: v1\nworkflow: w\nmodel: mock/echo\ntasks:\n  - id: ask\n    infer: { prompt: \"salut\" }\n";
    let (outcome, sink) = run_yaml(yaml, MockToolExecutor::new(), Some(0.0)).await;
    assert!(outcome.ok, "unmetered spend cannot cross a budget");
    assert!(!outcome.budget_exceeded);
    assert_eq!(outcome.total_cost_usd, None);
    assert_eq!(
        outcome.unpriced_calls, 1,
        "the mock infer is counted as unpriced"
    );
    let unpriced = terminal_field(&sink, EventKind::WorkflowCompleted, "unpriced_calls")
        .expect("the unpriced count rides the terminal frame");
    assert!(matches!(unpriced, FieldValue::Int(1)));
    // …and the task frame names the WHY.
    let reason = sink
        .events()
        .iter()
        .find(|e| e.kind == EventKind::TaskCompleted)
        .and_then(|e| e.fields.iter().find(|f| f.key == "cost_unpriced"))
        .expect("the task frame carries the reason");
    assert!(
        matches!(&reason.value, FieldValue::String(s) if s == "mock_provider"),
        "mock classifies as mock_provider"
    );
}
