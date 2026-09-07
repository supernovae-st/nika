// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The agent twin of the infer deadline proof: a task `timeout:` reaches
//! the provider request of every loop turn. Measured on 0.118.7 (#1516):
//! four agent tasks declaring `timeout: "840s"` died on their FIRST turn
//! at 30 002 ms — the transport's cloud default — while the same seat
//! under `infer:` with the same `timeout:` completed.

#![allow(clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::Duration;

use nika_kernel::ai::provider::InferRequest;
use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_invoke::InvokeVerb;

use crate::{DeterministicStamper, Runtime, RuntimeConfig, VecSink};

/// Settle one agent workflow on a capturing mock seat (a text answer on
/// turn 1 · no tools) and hand back every provider request the loop built.
async fn agent_requests(yaml: &str) -> Vec<InferRequest> {
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "fixture passes the ladder: {report:?}");
    let provider = Arc::new(MockProvider::new("mock").enqueue_text("ok"));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    let registry = Arc::new(nika_providers::ProviderRegistry::without_http(
        nika_providers::ProvidersConfig::default(),
    ));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        nika_verb_infer::InferVerb::new(registry, "mock/echo"),
        AgentVerb::new(
            Arc::clone(&provider),
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
        .expect("the run completes");
    assert!(outcome.ok, "the canned answer settles green");
    provider.captured_requests()
}

#[tokio::test]
async fn task_timeout_governs_the_agent_turns_provider_deadline() {
    // The infer proof's exact field, on an agent task: `timeout: "7m"`.
    let captured = agent_requests(
        "nika: w\nmodel: mock/echo\ntasks:\n  ask:\n    timeout: \"7m\"\n    agent: { prompt: \"hello\" }\n",
    )
    .await;
    assert_eq!(captured.len(), 1, "one provider round-trip");
    assert_eq!(
        captured[0].timeout,
        Some(Duration::from_secs(420)),
        "the task budget rides the agent's provider request"
    );
}

#[tokio::test]
async fn an_agent_task_without_timeout_leaves_the_transport_default() {
    let captured = agent_requests(
        "nika: w\nmodel: mock/echo\ntasks:\n  ask:\n    agent: { prompt: \"hello\" }\n",
    )
    .await;
    assert_eq!(captured.len(), 1, "one provider round-trip");
    assert_eq!(
        captured[0].timeout, None,
        "no task budget ⇒ the transport applies its own per-provider default"
    );
}
