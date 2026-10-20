// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The task's `timeout:` rides every `invoke:` call as its deadline
//! (stdlib §fetch: a fetch honors the task-level timeout), through the real
//! parse → check → run chain.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;
use std::time::Duration;

use nika_kernel::tool_executor::ToolResult;
use nika_kernel_mock::{MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_runtime::{DeterministicStamper, Runtime, RuntimeConfig, VecSink};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;

/// The deadline the one `invoke:` call of a task declaring `timeout` carries.
async fn deadline_of(timeout: &str) -> Option<Duration> {
    let source = format!(
        "nika: w\npermits: {{ tools: [\"nika:jq\"] }}\ntasks:\n  value:\n{timeout}    invoke: {{ tool: \"nika:jq\", args: {{ input: 2, expression: \".\" }} }}\n"
    );
    let wf = nika_schema::parse(
        &source,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "fixture passes the ladder: {report:?}");
    let tools = Arc::new(MockToolExecutor::new().enqueue_ok(ToolResult::success("tc", "2")));
    let invoke = Arc::new(InvokeVerb::new(Arc::clone(&tools)));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        InferVerb::new(
            Arc::new(ProviderRegistry::without_http(ProvidersConfig::default())),
            "mock/echo",
        ),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        nika_clock::DeclaredClock::system(),
        RuntimeConfig::default(),
    );
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("the run settles");
    let calls = tools.captured_calls();
    assert_eq!(calls.len(), 1, "one invoke call");
    calls[0].deadline()
}

#[tokio::test]
async fn the_task_timeout_rides_every_invoke_call() {
    assert_eq!(
        deadline_of("    timeout: \"3m\"\n").await,
        Some(Duration::from_secs(180))
    );
    assert_eq!(deadline_of("").await, None, "no task timeout, no deadline");
}
