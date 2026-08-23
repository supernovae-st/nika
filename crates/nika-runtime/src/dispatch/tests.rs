// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The #824 check⇄run parity proofs — a templated `model:`
//! (`${{ inputs.model }}`) checked green (the MODELS rung judges the
//! DECLARED DEFAULT via `static_literal_of`) but the dispatch handed the
//! RAW template to the provider, dying NIKA-INFER-001. The dispatch now
//! renders `model:` through the SAME `${{ }}` seam as
//! `prompt:`/`system:`, so the resolved default is what reaches the
//! wire — infer AND agent (one shared line each). The infer half reuses
//! the deadline rig's capturing seam
//! (`super::infer_deadline_tests::run_and_capture` · `pub(super)` for
//! exactly this).

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_invoke::InvokeVerb;

use crate::{DeterministicStamper, Runtime, RuntimeConfig, VecSink};

/// The issue's repro shape (a deployment-supplied `inputs.model`
/// default · the ollama wire so the RESOLVED string is observable in
/// the request body the provider seam captured). The issue was filed
/// against `config.model`; that authority died with the 9-key
/// envelope and the defect it pins lives on the surviving root.
const ISSUE_824_REPRO: &str = "nika: seat-from-an-input\n\
     inputs:\n  \
     model: { type: string, required: false, default: \"ollama/llama3.2:3b\" }\n\
     tasks:\n  \
     ask:\n    \
     infer: { model: \"${{ inputs.model }}\", max_tokens: 32, prompt: \"say ok\" }\n";

#[tokio::test]
async fn infer_model_input_template_resolves_before_the_wire() {
    let captured = super::infer_deadline_tests::run_and_capture(ISSUE_824_REPRO).await;
    assert_eq!(
        captured.len(),
        1,
        "one provider round-trip — no NIKA-INFER-001 on the raw template"
    );
    let body: serde_json::Value = serde_json::from_slice(
        captured[0]
            .body
            .as_ref()
            .expect("the infer wire has a body"),
    )
    .expect("the openai-compat body is json");
    assert_eq!(
        body["model"], "llama3.2:3b",
        "the RESOLVED input default reaches the provider, never the raw `${{{{ }}}}`"
    );
}

#[tokio::test]
async fn agent_model_input_template_resolves_before_the_provider() {
    let wf = nika_schema::parse(
        "nika: agent-seat\n\
         inputs:\n  \
         model: { type: string, required: false, default: \"mock/echo\" }\n\
         tasks:\n  \
         go:\n    \
         agent: { model: \"${{ inputs.model }}\", prompt: \"hi\" }\n",
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "fixture passes the ladder: {report:?}");

    let provider = Arc::new(MockProvider::new("mock").enqueue_text("done"));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        nika_verb_infer::InferVerb::new(
            Arc::new(ProviderRegistry::without_http(ProvidersConfig::new())),
            "mock/echo",
        ),
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
        .expect("clean run");
    assert!(outcome.ok, "the agent task settles green");
    let requests = provider.captured_requests();
    assert_eq!(requests.len(), 1, "one provider round-trip");
    assert_eq!(
        requests[0].model, "mock/echo",
        "the RESOLVED config default reaches the provider, never the raw `${{{{ }}}}`"
    );
}

#[test]
fn invoke_meters_a_top_level_cost_usd_from_structured_output() {
    // The honest-spend channel: a tool reporting real spend as a
    // top-level numeric `cost_usd` is metered; junk shapes never are.
    // (Rides with the #824 parity proofs since the D1-era extraction —
    // the inline `mod tests` block it came from collided with this
    // sibling file's declaration at the #884 merge.)
    let extract = |v: serde_json::Value| {
        v.get("cost_usd")
            .and_then(serde_json::Value::as_f64)
            .filter(|c| c.is_finite() && *c >= 0.0)
    };
    assert_eq!(
        extract(serde_json::json!({ "cost_usd": 0.02, "images": [] })),
        Some(0.02)
    );
    assert_eq!(extract(serde_json::json!({ "cost_usd": null })), None);
    assert_eq!(
        extract(serde_json::json!({ "cost_usd": -1.0 })),
        None,
        "negative refused"
    );
    assert_eq!(
        extract(serde_json::json!({ "cost_usd": "0.02" })),
        None,
        "strings refused"
    );
    assert_eq!(extract(serde_json::json!({ "other": 1 })), None);
    assert_eq!(extract(serde_json::json!("just text")), None);
    assert_eq!(
        extract(serde_json::json!({ "cost_usd": f64::NAN })),
        None,
        "non-finite refused"
    );
}

/// #1135 · the filed 0.111.0 capture was text-only
/// `{"messages":[{"content":"MARKER-PROMPT-XYZ","role":"user"}]}`.
/// A URL vision part MUST appear as `image_url` through parse → check → run.
#[tokio::test]
async fn url_vision_reaches_the_openai_compat_image_url_part() {
    let captured = super::infer_deadline_tests::run_and_capture(
        "nika: vision-wire-probe\n\
         model: ollama/llama3.2\n\
         tasks:\n  \
         look:\n    \
         infer:\n      \
         prompt: \"MARKER-PROMPT-XYZ\"\n      \
         max_tokens: 16\n      \
         vision:\n        \
         - source: url\n          \
         url: \"http://127.0.0.1:8731/x.png\"\n",
    )
    .await;
    assert_eq!(captured.len(), 1, "one provider round-trip");
    let body: serde_json::Value =
        serde_json::from_slice(captured[0].body.as_ref().expect("body")).expect("json");
    let content = &body["messages"][0]["content"];
    assert!(
        content.is_array(),
        "multimodal content is an array, never a text string — the 0.111.0 drop: {content}"
    );
    let parts = content.as_array().expect("parts");
    assert!(
        parts.iter().any(|p| {
            p["type"] == "image_url" && p["image_url"]["url"] == "http://127.0.0.1:8731/x.png"
        }),
        "URL vision rides as image_url: {content}"
    );
}

/// #1135 · pointing at a file that does not exist used to run green.
#[tokio::test]
async fn missing_vision_file_fails_the_run() {
    let (outcome, captured) = super::infer_deadline_tests::run_capture(
        "nika: vision-file-probe\n\
         model: ollama/llama3.2\n\
         tasks:\n  \
         look:\n    \
         infer:\n      \
         prompt: \"MARKER-FILE-PROBE\"\n      \
         max_tokens: 16\n      \
         vision:\n        \
         - source: file\n          \
         path: \"./this-file-does-not-exist.png\"\n",
    )
    .await;
    assert!(!outcome.ok, "a missing image file is no longer a green run");
    let rec = &outcome.records["look"];
    let err = rec.error.as_ref().expect("the failure carries its record");
    assert_eq!(err.code, "NIKA-432", "InvalidParam wire form");
    assert!(
        err.message.contains("vision"),
        "the vision param is named: {}",
        err.message
    );
    assert!(
        captured.is_empty(),
        "zero provider calls — the file gate fires first"
    );
}

/// #1135 sibling · `thinking.budget_tokens` parsed then vanished before
/// `InferRequest`. The anthropic wire is the observation point (openai-compat
/// has no thinking field).
#[tokio::test]
async fn thinking_budget_reaches_the_anthropic_wire() {
    let captured = super::infer_deadline_tests::run_and_capture(
        "nika: thinking-budget-probe\n\
         model: ollama/llama3.2\n\
         tasks:\n  \
         ask:\n    \
         infer:\n      \
         model: anthropic/claude-sonnet-4-20250514\n      \
         prompt: \"hello\"\n      \
         thinking:\n        \
         enabled: true\n        \
         budget_tokens: 2048\n",
    )
    .await;
    assert_eq!(captured.len(), 1, "one provider round-trip");
    let body: serde_json::Value =
        serde_json::from_slice(captured[0].body.as_ref().expect("body")).expect("json");
    assert_eq!(
        body["thinking"]["budget_tokens"], 2048,
        "thinking.budget_tokens reaches the anthropic wire: {body}"
    );
}
