// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! F4 — operator `--var` overrides through the REAL parse → check → run
//! chain: an override wins over a declared `default:`, and a
//! `required: true` var without one becomes runnable. Post-#603 an
//! UNSATISFIED required input refuses at ADMISSION (NIKA-1708 · before
//! the prologue); only an unbound OPTIONAL read still dies NIKA-VAR-001
//! at reference — the surviving read-time class, pinned here too.

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
nika: var-override
permits: { exec: true }
inputs:
  topic:
    type: string
    required: true
  lang: { type: string, default: "en" }
tasks:
  say:
    exec: { command: ["echo", "${{ inputs.topic }}"] }
outputs:
  topic_out: ${{ inputs.topic }}
  lang_out: ${{ inputs.lang }}
"#;

async fn try_run(
    yaml: &str,
    overrides: BTreeMap<String, Value>,
) -> (Result<RunOutcome, nika_runtime::RuntimeError>, VecSink) {
    let wf = nika_schema::parse(
        yaml,
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
    )
    .with_var_overrides(overrides);
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let result = runtime.run(&wf, &report, &mut stamper, &mut sink).await;
    (result, sink)
}

async fn run_with(overrides: BTreeMap<String, Value>) -> RunOutcome {
    try_run(WORKFLOW, overrides).await.0.expect("clean run")
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
async fn missing_required_var_is_refused_at_admission_nika_1708() {
    // No override on a `required: true` input → the admission preflight
    // (#603) refuses BEFORE the DAG: NIKA-1708, not one event — the
    // mid-DAG NIKA-VAR-001 death this suite used to pin was the bug.
    let (result, sink) = try_run(WORKFLOW, BTreeMap::new()).await;
    let err = result.expect_err("an unsatisfied required input never reaches dispatch");
    assert_eq!(err.spec_code(), "NIKA-1708");
    let msg = err.to_string();
    assert!(msg.contains("`topic`"), "the input is named: {msg}");
    assert!(
        msg.contains("--var topic=<value>"),
        "the satisfaction is taught: {msg}"
    );
    assert!(
        sink.events().is_empty(),
        "refused before the prologue — zero events"
    );
}

/// The OPTIONAL sibling of the required fixture: `note` declares no
/// `default:` and is not `required:` — declared optional.
const OPTIONAL_WORKFLOW: &str = r#"
nika: var-optional
permits: { exec: true }
inputs:
  note: { type: string }
tasks:
  say:
    exec: { command: ["echo", "${{ inputs.note }}"] }
"#;

#[tokio::test]
async fn unbound_optional_var_still_fails_var001_at_reference() {
    // A NON-required input with no default and no override is NOT an
    // admission refusal (declared optional) — the run still launches and
    // the unbound READ dies at reference, the surviving read-time class.
    let (result, sink) = try_run(OPTIONAL_WORKFLOW, BTreeMap::new()).await;
    let outcome = result.expect("an optional miss never refuses admission");
    assert!(
        !sink.events().is_empty(),
        "the OPTIONAL miss still RUNS — only the required case refuses admission"
    );
    assert!(!outcome.ok, "the unbound optional read fails the task");
    let record = &outcome.records["say"];
    let error = record.error.as_ref().expect("task carries its error");
    assert_eq!(error.code, "NIKA-VAR-001");
    assert!(
        error.message.contains("--var"),
        "the message teaches the CLI fix: {}",
        error.message
    );
}
