// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]

//! One-voice error battery (#468) — the same provider failure speaks
//! the SAME wire code (`spec_code()` · `tasks.X.error.code` · what
//! `on_codes:` matches) on every verb that surfaces it.
//!
//! Same floor discipline as `spec_v2.rs`: the REAL parse → check → run
//! chain over mock seams.

use std::sync::Arc;

use nika_check::check;
use nika_error::traits::NikaErrorCode;
use nika_event::{Event, EventKind};
use nika_kernel::provider::ProviderError;
use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_runtime::{DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, VecSink};
use nika_schema::{FileId, ParseMode, parse};
use nika_verb_agent::{AgentVerb, VerbAgentError};
use nika_verb_exec::ExecVerb;
use nika_verb_infer::{InferVerb, VerbInferError};
use nika_verb_invoke::InvokeVerb;

/// The 408 the issue reported live: an HTTP timeout at the provider.
fn timeout_408() -> ProviderError {
    ProviderError::Api {
        status: 408,
        message: "HTTP request timed out after 300000ms".to_owned(),
    }
}

async fn run_agent(yaml: &str, provider: MockProvider) -> (RunOutcome, Vec<Event>) {
    let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
    let report = check(&wf);
    assert!(report.is_clean(), "fixture passes the ladder");
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        InferVerb::new(registry, "mock/echo"),
        AgentVerb::new(
            Arc::new(provider),
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
        .expect("clean run");
    (outcome, sink.into_events())
}

/// #468 · the same provider 408 through both verb error surfaces must
/// yield the same wire code — the bare-numeric `NIKA-463` was outside
/// the spec's namespace grammar, so `nika check` rejects it in
/// `on_codes:` and no author filter could ever match the agent path.
#[tokio::test]
async fn a_provider_408_speaks_one_code_on_both_verbs() {
    let infer_err = VerbInferError::ProviderCall {
        source: timeout_408(),
        spend: Box::default(),
    };
    let agent_err = VerbAgentError::Inference {
        source: timeout_408(),
        spend: Box::default(),
    };
    assert_eq!(
        agent_err.spec_code(),
        infer_err.spec_code(),
        "one provider failure · one wire language (#468)"
    );
    assert_eq!(agent_err.spec_code(), "NIKA-INFER-001");
    assert_eq!(
        agent_err.is_transient(),
        infer_err.is_transient(),
        "the transience verdict is the provider's on both verbs"
    );

    // The schema sibling (the issue's NIKA-464 comment): the agent's
    // final-message schema gate is the class `NIKA-INFER-002` names.
    let agent_schema = VerbAgentError::SchemaValidation {
        detail: "missing field".to_owned(),
        spend: Box::default(),
    };
    assert_eq!(agent_schema.spec_code(), "NIKA-INFER-002");

    // Both resolve in the embedded spec canon — the table IS the truth
    // source (a code outside it silently breaks `on_codes:` filters).
    let canon = nika_pack::error_codes();
    for code in [agent_err.spec_code(), agent_schema.spec_code()] {
        assert!(
            canon.iter().any(|row| row.code == code),
            "{code} must resolve in the embedded spec canon"
        );
    }

    // e2e · the wire: an agent task whose provider dies mid-loop carries
    // the spec class at `tasks.X.error.code` (what `on_codes:` compares).
    let yaml_fail = r#"
nika: agent-408
model: mock/echo
tasks:
  stuck:
    agent:
      prompt: "try"
"#;
    let (outcome, _) = run_agent(
        yaml_fail,
        MockProvider::new("mock").enqueue_error(timeout_408()),
    )
    .await;
    let record = outcome.records["stuck"]
        .error
        .as_ref()
        .expect("typed error");
    assert_eq!(record.code, "NIKA-INFER-001", "the wire speaks the class");
    assert!(!record.transient, "a 408 is a verdict (only 5xx/429 retry)");

    // e2e · the unlock: `retry.on_codes: [NIKA-INFER-001]` now catches
    // the agent path's provider failure (non-transient · whitelisted).
    let yaml_retry = r#"
nika: agent-408-retry
model: mock/echo
tasks:
  flaky:
    retry: { max_attempts: 2, backoff_ms: 1, backoff_strategy: fixed, jitter: false, on_codes: [NIKA-INFER-001] }
    agent:
      prompt: "try again"
"#;
    let provider = MockProvider::new("mock")
        .enqueue_error(timeout_408())
        .enqueue_text("recovered");
    let (outcome, events) = run_agent(yaml_retry, provider).await;
    assert!(
        events.iter().any(|e| e.kind == EventKind::TaskRetrying),
        "the on_codes whitelist matched the agent's provider failure"
    );
    assert!(outcome.ok, "attempt 2 recovered");
    assert_eq!(
        outcome.records["flaky"].output,
        serde_json::Value::String("recovered".to_owned())
    );
}
