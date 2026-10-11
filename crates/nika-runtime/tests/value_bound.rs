// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The value bound a rehearsal room hands a run, through the real parse → check → run chain: a
//! value the run builds over it fails its task (NIKA-1712) and the bound keeps the first
//! crossing for the room; a run without a bound builds the same value.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;

use nika_runtime::{DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, ValueBound, VecSink};

/// One argument built from an input of 24 bytes.
const WORKFLOW: &str = r#"
nika: value-bound
permits: { exec: true }
inputs:
  text: { type: string, default: "abcdefghijklmnopqrstuvwx" }
tasks:
  say:
    exec: { command: ["echo", "${{ inputs.text }}"] }
"#;

async fn run(bound: Option<Arc<ValueBound>>) -> RunOutcome {
    let wf = nika_schema::parse(
        WORKFLOW,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "fixture passes the ladder");
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new().enqueue_ok("said\n"))),
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
    );
    let runtime = match bound {
        Some(bound) => runtime.with_value_bound(bound),
        None => runtime,
    };
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let result = runtime.run(&wf, &report, &mut stamper, &mut sink).await;
    result.expect("the run launches")
}

#[tokio::test]
async fn a_value_over_the_rooms_bound_fails_its_task_and_is_kept() {
    let bound = Arc::new(ValueBound::new(16));
    let outcome = run(Some(Arc::clone(&bound))).await;
    assert!(!outcome.ok, "the run fails");
    let error = outcome.records["say"]
        .error
        .as_ref()
        .expect("the task carries its error");
    assert_eq!(error.code, "NIKA-1712");
    assert_eq!(
        bound.crossed(),
        Some(24),
        "the first crossing is kept for the room"
    );
}

#[tokio::test]
async fn a_run_without_a_bound_builds_the_same_value() {
    let outcome = run(None).await;
    assert!(outcome.ok, "no room, no bound: the run completes");
    assert!(outcome.records["say"].error.is_none());
}
