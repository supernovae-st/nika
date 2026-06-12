// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]

//! The agent-loop telemetry wired END-TO-END (ADR-093): an `agent:`
//! task runs through the REAL runtime and its internal decisions land
//! on the canonical event stream — routing (`agent_tools_selected`),
//! tool dispatches (`tool_invoked`), and per-turn budget checkpoints
//! (`agent_budget_checkpoint`) — each stamped with the task id, ordered
//! between the task's lifecycle frames.

use std::sync::Arc;

use nika_event::EventKind;
use nika_kernel::provider::{ContentBlock, InferResponse, StopReason, TokenUsage};
use nika_kernel::tool_executor::ToolResult;
use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::{NoHttp, ProviderRegistry, ProvidersConfig};
use nika_runtime::{DeterministicStamper, Runtime, RuntimeConfig, VecSink};
use nika_schema::{FileId, ParseMode, check, parse};
use nika_types::resource::Value as FieldValue;
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;

const AGENT_WORKFLOW: &str = r#"
nika: v1
workflow: agent-telemetry
model: mock/echo

tasks:
  - id: research
    agent:
      prompt: "read the notes file then answer"
      tools: ["nika:read"]
      max_turns: 4
"#;

fn usage(input: u64, output: u64) -> TokenUsage {
    let mut usage = TokenUsage::default();
    usage.input_tokens = input;
    usage.output_tokens = output;
    usage
}

fn tool_use_response(id: &str, name: &str, args: serde_json::Value) -> InferResponse {
    InferResponse::new(
        vec![
            ContentBlock::Text {
                text: format!("calling {name}"),
            },
            ContentBlock::ToolUse {
                id: id.to_owned(),
                name: name.to_owned(),
                input: args,
            },
        ],
        usage(10, 5),
        StopReason::ToolUse,
    )
}

fn text_response(text: &str) -> InferResponse {
    InferResponse::new(
        vec![ContentBlock::Text {
            text: text.to_owned(),
        }],
        usage(10, 5),
        StopReason::EndTurn,
    )
}

fn int_field(event: &nika_event::Event, key: &str) -> Option<i64> {
    event.fields.iter().find(|f| f.key == key).and_then(|f| {
        if let FieldValue::Int(n) = &f.value {
            Some(*n)
        } else {
            None
        }
    })
}

fn str_field<'a>(event: &'a nika_event::Event, key: &str) -> Option<&'a str> {
    event.fields.iter().find(|f| f.key == key).and_then(|f| {
        if let FieldValue::String(s) = &f.value {
            Some(s.as_str())
        } else {
            None
        }
    })
}

type AgentRuntime = Runtime<
    MockShell,
    MockToolExecutor,
    NoHttp,
    MockProvider,
    MockToolDefinitionProvider,
    MockClock,
>;

/// Assemble the runtime with the agent lane scripted: one `nika:read`
/// definition in the universe, the given provider script + tool results.
fn agent_runtime(provider: MockProvider, tools: MockToolExecutor) -> AgentRuntime {
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(tools)));
    Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        InferVerb::new(registry, "mock/echo"),
        AgentVerb::new(
            Arc::new(provider),
            invoke,
            Arc::new(MockToolDefinitionProvider::with_defs(vec![
                nika_kernel::provider::ToolDef::new(
                    "nika:read",
                    "read a file",
                    serde_json::json!({}),
                ),
            ])),
            "mock/echo",
        ),
        MockClock::new(),
        RuntimeConfig::default(),
    )
}

#[tokio::test]
async fn agent_decisions_ride_the_canonical_event_stream() {
    let wf = parse(AGENT_WORKFLOW, FileId::new(0), ParseMode::Strict).expect("fixture parses");
    let report = check(&wf);

    // Turn 1: the model calls nika:read · turn 2: it concludes.
    let provider = MockProvider::new("mock")
        .enqueue_response(tool_use_response(
            "c1",
            "nika:read",
            serde_json::json!({"path": "./notes.md"}),
        ))
        .enqueue_response(text_response("the notes say hello"));
    let tools =
        MockToolExecutor::new().enqueue_ok(ToolResult::success("c1", "hello from the notes"));
    let runtime = agent_runtime(provider, tools);

    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("clean run");
    assert!(outcome.ok, "the agent task completes");
    let events = sink.into_events();

    // ── the routing decision is ON the stream, task-stamped ──
    let selected: Vec<_> = events
        .iter()
        .filter(|e| e.kind == EventKind::AgentToolsSelected)
        .collect();
    assert_eq!(selected.len(), 2, "one routing decision per turn");
    for (turn, event) in selected.iter().enumerate() {
        assert_eq!(str_field(event, "task"), Some("research"));
        assert_eq!(
            int_field(event, "turn"),
            Some(i64::try_from(turn).expect("small") + 1)
        );
        // Small universe (1 def + the synthesized sentinel is absent —
        // nika:done not whitelisted) → passthrough: offered == universe.
        assert_eq!(int_field(event, "offered"), int_field(event, "universe"));
        assert_eq!(int_field(event, "builtin"), Some(1), "nika:read counted");
    }

    // ── the dispatched tool is visible by name, turn-stamped ──
    let invoked: Vec<_> = events
        .iter()
        .filter(|e| e.kind == EventKind::ToolInvoked)
        .collect();
    assert_eq!(invoked.len(), 1, "one real dispatch");
    assert_eq!(str_field(invoked[0], "task"), Some("research"));
    assert_eq!(str_field(invoked[0], "tool"), Some("nika:read"));
    assert_eq!(int_field(invoked[0], "turn"), Some(1));
    assert_eq!(
        invoked[0]
            .fields
            .iter()
            .find(|f| f.key == "error")
            .map(|f| f.value.clone()),
        Some(FieldValue::Bool(false))
    );

    // ── the spend curve is observable per turn ──
    let checkpoints: Vec<_> = events
        .iter()
        .filter(|e| e.kind == EventKind::AgentBudgetCheckpoint)
        .collect();
    assert_eq!(checkpoints.len(), 2, "one checkpoint per turn");
    assert_eq!(int_field(checkpoints[0], "total_tokens"), Some(15));
    assert_eq!(int_field(checkpoints[1], "total_tokens"), Some(30));

    // ── ordering: the decisions land between TaskStarted and the
    //    terminal frame of THIS task ──
    let pos = |pred: &dyn Fn(&nika_event::Event) -> bool| {
        events.iter().position(pred).expect("event present")
    };
    let started =
        pos(&|e| e.kind == EventKind::TaskStarted && str_field(e, "task") == Some("research"));
    let completed =
        pos(&|e| e.kind == EventKind::TaskCompleted && str_field(e, "task") == Some("research"));
    for e in &selected {
        let p = events
            .iter()
            .position(|x| std::ptr::eq(*e, x))
            .expect("position");
        assert!(
            started < p && p < completed,
            "decision events ride inside the task's lifecycle bracket"
        );
    }

    // ── and the task's terminal frame still carries the loop totals ──
    let terminal = &events[completed];
    assert_eq!(str_field(terminal, "note"), Some("agent · 2 turns"));
    assert_eq!(int_field(terminal, "tokens"), Some(30));
}

#[tokio::test]
async fn a_stalled_agent_puts_the_evidence_on_the_stream() {
    const STALL_WORKFLOW: &str = r#"
nika: v1
workflow: agent-stall
model: mock/echo

tasks:
  - id: looper
    agent:
      prompt: "keep reading the same file"
      tools: ["nika:read"]
      max_turns: 10
"#;
    let wf = parse(STALL_WORKFLOW, FileId::new(0), ParseMode::Strict).expect("fixture parses");
    let report = check(&wf);

    // 6 byte-identical action+observation turns → nudge at 3 · stall at 5.
    let mut provider = MockProvider::new("mock");
    let mut tools = MockToolExecutor::new();
    for _ in 0..6 {
        provider = provider.enqueue_response(tool_use_response(
            "c",
            "nika:read",
            serde_json::json!({"path": "./same.md"}),
        ));
        tools = tools.enqueue_ok(ToolResult::success("c", "identical body"));
    }

    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(tools)));
    let runtime: Runtime<
        MockShell,
        MockToolExecutor,
        NoHttp,
        MockProvider,
        MockToolDefinitionProvider,
        MockClock,
    > = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        InferVerb::new(registry, "mock/echo"),
        AgentVerb::new(
            Arc::new(provider),
            invoke,
            Arc::new(MockToolDefinitionProvider::with_defs(vec![
                nika_kernel::provider::ToolDef::new(
                    "nika:read",
                    "read a file",
                    serde_json::json!({}),
                ),
            ])),
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
        .expect("the run settles (task fails · run completes)");
    assert!(!outcome.ok, "the stalled task fails the run");
    let events = sink.into_events();

    // The corrective nudge fired once, then the stall — both on the
    // stream with their evidence (TRAIL: evidence rides IN the trace).
    let nudge = events
        .iter()
        .find(|e| e.kind == EventKind::AgentNudge)
        .expect("the nudge is on the stream");
    assert_eq!(str_field(nudge, "task"), Some("looper"));
    assert_eq!(str_field(nudge, "reason"), Some("repeated_actions"));

    let stalled = events
        .iter()
        .find(|e| e.kind == EventKind::AgentStalled)
        .expect("the stall is on the stream");
    assert_eq!(str_field(stalled, "task"), Some("looper"));
    assert_eq!(int_field(stalled, "period"), Some(1));
    assert_eq!(int_field(stalled, "repeats"), Some(5));

    // And the task's terminal frame carries the NIKA-467 verdict.
    let failed = events
        .iter()
        .find(|e| e.kind == EventKind::TaskFailed && str_field(e, "task") == Some("looper"))
        .expect("the task fails");
    assert!(
        str_field(failed, "detail").is_some_and(|d| d.contains("NIKA-467")),
        "the stall code rides the failure frame"
    );
}
