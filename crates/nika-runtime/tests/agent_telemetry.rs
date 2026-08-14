// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]

//! The agent-loop telemetry wired END-TO-END (ADR-096): an `agent:`
//! task runs through the REAL runtime and its internal decisions land
//! on the canonical event stream — routing (`agent_tools_selected`),
//! tool dispatches (`tool_invoked`), and per-turn budget checkpoints
//! (`agent_budget_checkpoint`) — each stamped with the task id, ordered
//! between the task's lifecycle frames.

use std::sync::Arc;

use nika_check::check;
use nika_event::EventKind;
use nika_kernel::provider::{ContentBlock, InferResponse, StopReason, TokenUsage};
use nika_kernel::tool_executor::ToolResult;
use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::{NoHttp, ProviderRegistry, ProvidersConfig};
use nika_runtime::{DeterministicStamper, Runtime, RuntimeConfig, VecSink};
use nika_schema::{FileId, ParseMode, parse};
use nika_types::resource::Value as FieldValue;
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;

const AGENT_WORKFLOW: &str = r#"
nika: agent-telemetry
model: mock/echo

permits:
  tools: ["nika:read"]
  fs:
    read: ["./notes.md", "./never.md", "./same.md", "./f.md"]

tasks:
  research:
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
    runtime_with_universe(provider, tools, vec![("nika:read", "read a file")])
}

/// Same, but with an explicit whitelisted universe (name, description).
fn runtime_with_universe(
    provider: MockProvider,
    tools: MockToolExecutor,
    defs: Vec<(&str, &str)>,
) -> AgentRuntime {
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(tools)));
    let universe = defs
        .into_iter()
        .map(|(name, desc)| nika_kernel::provider::ToolDef::new(name, desc, serde_json::json!({})))
        .collect();
    Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        InferVerb::new(registry, "mock/echo"),
        AgentVerb::new(
            Arc::new(provider),
            invoke,
            Arc::new(MockToolDefinitionProvider::with_defs(universe)),
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

/// A tool that never completes — the timeout-evidence fixture.
struct HangingTool;

impl nika_kernel::tool_executor::ToolExecuteDyn for HangingTool {
    async fn execute(
        &self,
        _call: nika_kernel::tool_executor::ToolCall,
    ) -> Result<ToolResult, nika_kernel::tool_executor::ToolExecError> {
        std::future::pending().await
    }
}

#[tokio::test]
async fn a_timed_out_agent_keeps_its_pre_timeout_evidence() {
    // The review's F1: the buffer lives OUTSIDE the timeout-cancellable
    // region — a timed-out attempt's decisions (routing · budget) must
    // survive the drop of the attempt future and reach the stream.
    const TIMEOUT_WORKFLOW: &str = r#"
nika: agent-timeout
model: mock/echo

permits:
  tools: ["nika:read"]
  fs:
    read: ["./never.md"]

tasks:
  wedged:
    timeout: "50ms"
    agent:
      prompt: "read the file"
      tools: ["nika:read"]
"#;
    let wf = parse(TIMEOUT_WORKFLOW, FileId::new(0), ParseMode::Strict).expect("fixture parses");
    let report = check(&wf);

    // Turn 1: the model calls the tool — which hangs forever; the task
    // timeout (instant under MockClock) cuts the attempt.
    let provider = MockProvider::new("mock").enqueue_response(tool_use_response(
        "c1",
        "nika:read",
        serde_json::json!({"path": "./never.md"}),
    ));
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(HangingTool)));
    let runtime: Runtime<
        MockShell,
        HangingTool,
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
    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        runtime.run(&wf, &report, &mut stamper, &mut sink),
    )
    .await
    .expect("the budget fires — a hanging tool never wedges the run")
    .expect("the run settles");
    assert!(!outcome.ok, "the timed-out task fails the run");
    let events = sink.into_events();

    // The pre-timeout decisions SURVIVED the cancelled attempt.
    let selected = events
        .iter()
        .find(|e| e.kind == EventKind::AgentToolsSelected)
        .expect("the routing decision survived the timeout");
    assert_eq!(str_field(selected, "task"), Some("wedged"));
    assert_eq!(int_field(selected, "turn"), Some(1));
    assert_eq!(
        int_field(selected, "attempt"),
        Some(1),
        "the in-flight attempt is attributed"
    );
    assert!(
        events
            .iter()
            .any(|e| e.kind == EventKind::AgentBudgetCheckpoint),
        "the spend checkpoint survived too"
    );
    // The tool never completed: no ToolInvoked frame.
    assert!(
        !events.iter().any(|e| e.kind == EventKind::ToolInvoked),
        "a hung call has no completion to report"
    );
    // And the terminal frame carries the timeout verdict.
    let failed = events
        .iter()
        .find(|e| e.kind == EventKind::TaskFailed && str_field(e, "task") == Some("wedged"))
        .expect("the task fails");
    assert!(
        str_field(failed, "detail").is_some_and(|d| d.contains("NIKA-TIMEOUT-001")),
        "the timeout code rides the failure frame"
    );
}

#[tokio::test]
async fn a_stalled_agent_puts_the_evidence_on_the_stream() {
    const STALL_WORKFLOW: &str = r#"
nika: agent-stall
model: mock/echo

permits:
  tools: ["nika:read"]
  fs:
    read: ["./same.md"]

tasks:
  looper:
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

#[tokio::test]
async fn agent_decisions_are_attributed_to_their_retry_attempt() {
    // A retried agent: attempt 1's mid-loop inference fails (transient →
    // retryable), attempt 2 succeeds. The decisions of BOTH attempts ride
    // the stream, each stamped with its `attempt` — without the stamp a
    // retried agent and a fan-out are indistinguishable flat streams (the
    // wiring review's F3). The attempt mark is what the stamp reads, so
    // this is also the ONLY test that exercises `BufferingObserver::len`
    // returning a meaningful (non-zero, per-attempt) value.
    const RETRY_WORKFLOW: &str = r#"
nika: agent-retry
model: mock/echo

permits:
  tools: ["nika:read"]
  fs:
    read: ["./f.md"]

tasks:
  flaky:
    retry: { max_attempts: 2 }
    agent:
      prompt: "read the file"
      tools: ["nika:read"]
      max_turns: 4
"#;
    let wf = parse(RETRY_WORKFLOW, FileId::new(0), ParseMode::Strict).expect("fixture parses");
    let report = check(&wf);

    // Attempt 1: routes (ToolsSelected turn 1) then the provider rate-
    // limits → a transient provider failure (wire NIKA-INFER-001) →
    // the runtime retries. Attempt 2: routes, reads, concludes.
    let provider = MockProvider::new("mock")
        .enqueue_error(nika_kernel::provider::ProviderError::RateLimited {
            retry_after_ms: Some(1),
        })
        .enqueue_response(tool_use_response(
            "c1",
            "nika:read",
            serde_json::json!({"path": "./f.md"}),
        ))
        .enqueue_response(text_response("the file says hi"));
    let tools = MockToolExecutor::new().enqueue_ok(ToolResult::success("c1", "file body"));
    let runtime = agent_runtime(provider, tools);

    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("the retry recovers");
    assert!(outcome.ok, "attempt 2 succeeds");
    let events = sink.into_events();

    // Every routing decision carries its attempt. Attempt 1 routed once
    // (turn 1, before the failed infer); attempt 2 routed for each of its
    // turns. The attempts present MUST be exactly {1, 2}.
    let attempts: std::collections::BTreeSet<i64> = events
        .iter()
        .filter(|e| e.kind == EventKind::AgentToolsSelected)
        .filter_map(|e| int_field(e, "attempt"))
        .collect();
    assert_eq!(
        attempts,
        [1, 2].into_iter().collect(),
        "decisions are attributed to BOTH attempts, distinctly: {attempts:?}"
    );

    // Concretely: a turn-1 routing decision exists for attempt 1 AND a
    // distinct one for attempt 2 (the retry re-ran turn 1).
    let turn1_attempts: Vec<i64> = events
        .iter()
        .filter(|e| e.kind == EventKind::AgentToolsSelected && int_field(e, "turn") == Some(1))
        .filter_map(|e| int_field(e, "attempt"))
        .collect();
    assert!(
        turn1_attempts.contains(&1) && turn1_attempts.contains(&2),
        "turn 1 was routed under both attempts: {turn1_attempts:?}"
    );
    // The retry frame is on the stream too (the attempt counter the
    // agent events join against).
    assert!(
        events.iter().any(|e| e.kind == EventKind::TaskRetrying),
        "the retry is visible"
    );
}

#[tokio::test]
async fn an_agent_compose_check_rides_the_canonical_stream() {
    // The `nika:compose` intrinsic's static verdict reaches the stream
    // as `agent_compose_checked` — the ONE agent kind the prior e2e set
    // never exercised through the real runtime (mutation gap).
    const COMPOSE_WORKFLOW: &str = r#"
nika: agent-compose
model: mock/echo

permits:
  tools: ["nika:compose"]

tasks:
  drafter:
    agent:
      prompt: "draft a workflow then finish"
      tools: ["nika:compose"]
      max_turns: 4
"#;
    let wf = parse(COMPOSE_WORKFLOW, FileId::new(0), ParseMode::Strict).expect("fixture parses");
    let report = check(&wf);

    let draft = "nika: drafted\ntasks:\n  t:\n    exec:\n      command: [\"echo\", \"hi\"]\n";
    let provider = MockProvider::new("mock")
        .enqueue_response(tool_use_response(
            "d1",
            "nika:compose",
            serde_json::json!({ "workflow_yaml": draft }),
        ))
        .enqueue_response(text_response("drafted and certified"));
    // Empty executor: a compose never dispatches; any dispatch would error.
    let runtime = runtime_with_universe(provider, MockToolExecutor::new(), Vec::new());

    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("the compose run completes");
    assert!(outcome.ok);
    let events = sink.into_events();

    let checked = events
        .iter()
        .find(|e| e.kind == EventKind::AgentComposeChecked)
        .expect("the compose verdict is on the stream");
    assert_eq!(str_field(checked, "task"), Some("drafter"));
    assert_eq!(int_field(checked, "turn"), Some(1));
    assert_eq!(int_field(checked, "violations"), Some(0));
    assert_eq!(
        checked
            .fields
            .iter()
            .find(|f| f.key == "valid")
            .map(|f| f.value.clone()),
        Some(FieldValue::Bool(true)),
        "the valid draft is reported valid"
    );
    // Generation is not permission: the draft never reached the executor.
    assert!(
        !events.iter().any(|e| e.kind == EventKind::ToolInvoked),
        "compose is loop-served, never dispatched"
    );
}

#[tokio::test]
async fn an_error_streak_nudge_names_its_reason_on_the_stream() {
    // Two consecutive ALL-ERROR turns (distinct args → the cycle detector
    // stays out) draw the error-streak nudge — distinct from the stall
    // test's `repeated_actions` reason, and the ONLY runtime test that
    // pins the `error_streak` slug (mutation gap on `reason_slug`).
    const STREAK_WORKFLOW: &str = r#"
nika: agent-streak
model: mock/echo

permits:
  tools: ["nika:read"]
  fs:
    read: ["./a.md", "./b.md", "./c.md", "./f.md", "./never.md", "./same.md", "./notes.md"]

tasks:
  failer:
    agent:
      prompt: "keep trying different reads"
      tools: ["nika:read"]
      max_turns: 6
"#;
    let wf = parse(STREAK_WORKFLOW, FileId::new(0), ParseMode::Strict).expect("fixture parses");
    let report = check(&wf);

    // Turns 1-2: read with DIFFERENT args (no cycle) · empty executor →
    // every dispatch errors → streak 2 → nudge · turn 3: conclude.
    let provider = MockProvider::new("mock")
        .enqueue_response(tool_use_response(
            "a",
            "nika:read",
            serde_json::json!({"p": 1}),
        ))
        .enqueue_response(tool_use_response(
            "b",
            "nika:read",
            serde_json::json!({"p": 2}),
        ))
        .enqueue_response(text_response("giving up, here's a partial answer"));
    let runtime = agent_runtime(provider, MockToolExecutor::new());

    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("the agent recovers with an answer");
    assert!(outcome.ok);
    let events = sink.into_events();

    let nudge = events
        .iter()
        .find(|e| e.kind == EventKind::AgentNudge)
        .expect("the error-streak nudge is on the stream");
    assert_eq!(str_field(nudge, "task"), Some("failer"));
    assert_eq!(
        str_field(nudge, "reason"),
        Some("error_streak"),
        "the streak reason is named distinctly from repeated_actions"
    );
}
