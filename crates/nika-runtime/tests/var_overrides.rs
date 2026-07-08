// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! F4 — operator `--var` overrides through the REAL parse → check → run
//! chain: an override wins over a declared `default:`, and a
//! `required: true` var without one becomes runnable (before: the run
//! could only die NIKA-VAR-001 at first reference).

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeMap;
use std::sync::Arc;

use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;
use serde_json::Value;

use nika_runtime::{DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, VecSink};

const WORKFLOW: &str = r#"
nika: v1
workflow: var-override
vars:
  topic:
    type: string
    required: true
  lang: { type: string, default: "en" }
tasks:
  - id: say
    exec: { command: "echo ${{ vars.topic }}" }
outputs:
  topic_out: ${{ vars.topic }}
  lang_out: ${{ vars.lang }}
"#;

async fn run_with(overrides: BTreeMap<String, Value>) -> RunOutcome {
    let wf = nika_schema::parse(
        WORKFLOW,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_schema::check(&wf);
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
    )
    .with_var_overrides(overrides);
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("clean run")
}

#[tokio::test]
async fn override_satisfies_a_required_var_and_beats_the_default() {
    let overrides = BTreeMap::from([
        ("topic".to_owned(), Value::String("rust".to_owned())),
        ("lang".to_owned(), Value::String("fr".to_owned())),
    ]);
    let outcome = run_with(overrides).await;
    assert!(outcome.ok, "the required var is satisfied → green run");
    assert_eq!(outcome.outputs["topic_out"], "rust");
    assert_eq!(
        outcome.outputs["lang_out"], "fr",
        "an override wins over the declared default"
    );
}

#[tokio::test]
async fn missing_required_var_still_fails_var001_at_reference() {
    // No override → the pre-F4 behavior is intact: the task's
    // `${{ vars.topic }}` fails NIKA-VAR-001 (with the --var hint).
    let outcome = run_with(BTreeMap::new()).await;
    assert!(!outcome.ok, "unbound required var fails the task");
    let record = &outcome.records["say"];
    let error = record.error.as_ref().expect("task carries its error");
    assert_eq!(error.code, "NIKA-VAR-001");
    assert!(
        error.message.contains("--var"),
        "the message teaches the CLI fix: {}",
        error.message
    );
}
