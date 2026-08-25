// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Regressions for N27's single run-start clock binding.

#![allow(clippy::expect_used, clippy::panic)]

use super::*;

fn clock_workflow(run: &str) -> RawWorkflow {
    nika_schema::parse(
        &format!(
            "nika: run-clock\n{run}tasks:\n  probe:\n    invoke:\n      tool: \"nika:jq\"\n      args: {{ input: null, expression: \"now\" }}\n    extract:\n      rebound: \"now\"\n"
        ),
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("clock fixture parses")
}

async fn run_clock_workflow(
    wf: &RawWorkflow,
    runtime: &SimRuntime,
    stamper: &mut dyn Stamper,
) -> (RunOutcome, Vec<Event>) {
    let report = nika_check::check(wf);
    assert!(report.is_clean(), "clock fixture checks: {report:?}");
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(wf, &report, stamper, &mut sink)
        .await
        .expect("clock fixture runs");
    (outcome, sink.into_events())
}

fn started_at(events: &[Event]) -> nika_types::timestamp::Timestamp {
    events
        .iter()
        .find(|event| event.kind == EventKind::WorkflowStarted)
        .expect("opening frame")
        .timestamp
}

fn assert_every_jq_clock(outcome: &RunOutcome, expected: f64) {
    let record = &outcome.records["probe"];
    assert_eq!(
        record.output.as_f64(),
        Some(expected),
        "nika:jq sees the opening stamp"
    );
    assert_eq!(
        record.named["rebound"].as_f64(),
        Some(expected),
        "the output-binding evaluator sees the same opening stamp"
    );
}

type ClockDispatcher = nika_builtin::BuiltinDispatcher<
    nika_kernel_mock::MockFs,
    nika_kernel_mock::MockHttp,
    nika_kernel_mock::MockClock,
    nika_builtin::NullEmitter,
    nika_builtin::NonInteractive,
    nika_builtin::NoWorkflow,
>;

fn clock_dispatcher() -> Arc<ClockDispatcher> {
    Arc::new(nika_builtin::BuiltinDispatcher::new(
        Arc::new(nika_kernel_mock::MockFs::new()),
        Arc::new(nika_kernel_mock::MockHttp::new()),
        Arc::new(nika_kernel_mock::MockClock::new()),
        Arc::new(nika_builtin::NullEmitter::default()),
        Arc::new(nika_builtin::NonInteractive::default()),
        Arc::new(nika_builtin::NoWorkflow::default()),
    ))
}

#[tokio::test]
async fn composition_delay_under_ambient_entropy_stays_exact() {
    let wf = clock_workflow("");
    let runtime = simulated_runtime("mock/echo", capabilities_of(&wf), None)
        .expect("simulated runtime composes");

    // This delay is causally between composition and execution. The former
    // implementation sampled jq above this await and therefore disagreed
    // with the opening frame minted below it.
    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    let mut stamper = RunSeams::of(None).stamper();
    let (outcome, events) = run_clock_workflow(&wf, &runtime, stamper.as_mut()).await;
    let stamp = started_at(&events);
    let expected = nika_cap::JqClock::at(stamp).unix_seconds();
    assert_every_jq_clock(&outcome, expected);
}

#[tokio::test]
async fn deterministic_virtual_run_binds_exact_opening_stamp() {
    let wf = clock_workflow("run: { entropy: none, clock: virtual }\n");
    let decl = wf.run.as_ref().map(|run| &run.value);
    let runtime = simulated_runtime("mock/echo", capabilities_of(&wf), decl)
        .expect("simulated runtime composes");
    let mut stamper = RunSeams::of(decl).stamper();
    let (outcome, events) = run_clock_workflow(&wf, &runtime, stamper.as_mut()).await;

    let stamp = started_at(&events);
    assert_eq!(
        stamp.unix_ms(),
        10,
        "the deterministic first frame is +10ms"
    );
    assert_every_jq_clock(&outcome, 0.01);
}

#[tokio::test]
async fn public_runtime_new_shares_opening_stamp_with_real_builtin_dispatcher() {
    use nika_kernel_mock::{MockClock, MockProvider, MockShell};

    let wf = clock_workflow("run: { entropy: none, clock: virtual }\n");
    let dispatcher = clock_dispatcher();
    let invoke = Arc::new(InvokeVerb::new(Arc::clone(&dispatcher)));
    let registry = Arc::new(nika_providers::ProviderRegistry::without_http(
        nika_providers::ProvidersConfig::default(),
    ));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        InferVerb::new(registry, "mock/echo"),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::clone(&dispatcher),
            "mock/echo",
        ),
        MockClock::new(),
        RuntimeConfig::default(),
    );
    let report = nika_check::check(&wf);
    assert!(report.is_clean());
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("runs");
    let opening = started_at(&sink.into_events());
    let expected = nika_cap::JqClock::at(opening).unix_seconds();

    assert_every_jq_clock(&outcome, expected);
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // the split composition is the regression itself
async fn split_agent_dispatcher_receives_the_same_opening_stamp() {
    use nika_kernel::provider::{ContentBlock, InferResponse, StopReason, TokenUsage, ToolDef};
    use nika_kernel_mock::{MockClock, MockProvider, MockShell, MockToolDefinitionProvider};

    let wf = nika_schema::parse(
        r#"
nika: split-tool-clock
model: mock/echo
run: { entropy: none, clock: virtual }
permits: { tools: ["nika:jq"] }
tasks:
  probe:
    agent:
      prompt: "read the run clock"
      tools: ["nika:jq"]
      max_turns: 3
    extract: { rebound: "now" }
"#,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "fixture checks: {report:?}");

    let response = |content, stop| InferResponse::new(content, TokenUsage::new(10, 5), stop);
    let provider = MockProvider::new("mock")
        .enqueue_response(response(
            vec![ContentBlock::ToolUse {
                id: "clock-call".to_owned(),
                name: "nika:jq".to_owned(),
                input: serde_json::json!({"input": null, "expression": "now"}),
            }],
            StopReason::ToolUse,
        ))
        .enqueue_response(response(
            vec![ContentBlock::Text {
                text: "done".to_owned(),
            }],
            StopReason::EndTurn,
        ));
    let provider_probe = provider.clone();
    let defs = MockToolDefinitionProvider::with_defs(vec![ToolDef::new(
        "nika:jq",
        "evaluate jq",
        serde_json::json!({}),
    )]);
    let runtime_invoke = Arc::new(InvokeVerb::new(clock_dispatcher()));
    let agent_invoke = Arc::new(InvokeVerb::new(clock_dispatcher()));
    let registry = Arc::new(nika_providers::ProviderRegistry::without_http(
        nika_providers::ProvidersConfig::default(),
    ));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        runtime_invoke,
        InferVerb::new(registry, "mock/echo"),
        AgentVerb::new(
            Arc::new(provider),
            agent_invoke,
            Arc::new(defs),
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
        .expect("run completes");
    let output_clock = outcome.records["probe"].named["rebound"]
        .as_f64()
        .expect("output jq clock");
    let tool_clock = provider_probe.captured_requests()[1]
        .messages
        .iter()
        .flat_map(|message| message.content.iter())
        .find_map(|block| match block {
            ContentBlock::ToolResult { content, .. } => content.parse::<f64>().ok(),
            _ => None,
        })
        .expect("agent received jq result");

    assert!((output_clock - 0.01).abs() < f64::EPSILON);
    assert!((tool_clock - output_clock).abs() < f64::EPSILON);
}

#[tokio::test]
async fn concurrent_calls_sharing_one_dispatcher_keep_their_own_run_start() {
    use nika_kernel::tool_executor::ToolRunStart;
    use nika_kernel_mock::{MockClock, MockFs, MockHttp};
    use nika_verb_invoke::InvokeInput;

    let dispatcher = Arc::new(nika_builtin::BuiltinDispatcher::new(
        Arc::new(MockFs::new()),
        Arc::new(MockHttp::new()),
        Arc::new(MockClock::new()),
        Arc::new(nika_builtin::NullEmitter::default()),
        Arc::new(nika_builtin::NonInteractive::default()),
        Arc::new(nika_builtin::NoWorkflow::default()),
    ));
    let invoke = InvokeVerb::new(dispatcher);
    let input = || {
        let mut input = InvokeInput::new("nika:jq");
        input.args = serde_json::json!({"input": null, "expression": "now"});
        input
    };

    let (first, second) = tokio::join!(
        invoke.run_at(input(), ToolRunStart::new(10_000_000)),
        invoke.run_at(input(), ToolRunStart::new(20_000_000)),
    );
    assert_eq!(first.expect("first call").content, "0.01");
    assert_eq!(second.expect("second call").content, "0.02");
}
