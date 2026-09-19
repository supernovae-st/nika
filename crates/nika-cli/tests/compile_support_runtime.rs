// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Real emitted source and runtime/builtins; only filesystem, provider HTTP,
//! business HTTP and human interaction are injected. No actual network or keys.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_builtin::{BuiltinDispatcher, NoWorkflow, NullEmitter, Prompter};
use nika_kernel::secret::Secret;
use nika_kernel_mock::{
    MockClock, MockFs, MockHttp, MockProvider, MockShell, MockToolDefinitionProvider,
};
use nika_onboard::compile::{CompileRequest, CompileStatus, compile};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_runtime::{DeterministicStamper, Runtime, RuntimeConfig, VecSink};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

struct Human {
    answer: Option<bool>,
    messages: Mutex<Vec<String>>,
}
impl Prompter for Human {
    fn confirm(&self, message: &str) -> Option<bool> {
        self.messages.lock().unwrap().push(message.to_owned());
        self.answer
    }
    fn input(&self, _: &str) -> Option<String> {
        None
    }
    fn choice(&self, _: &str, _: &[String]) -> Option<String> {
        None
    }
}

async fn run(request: Option<Value>, answer: Option<bool>) -> (bool, Vec<String>, Vec<Value>) {
    run_with_anchor(request, answer, "Ada").await
}

async fn run_with_anchor(
    request: Option<Value>,
    answer: Option<bool>,
    anchor: &str,
) -> (bool, Vec<String>, Vec<Value>) {
    let compiled = compile(
        &CompileRequest::create(
            "Look up the customer and draft a reply and ask me before any refund",
        )
        .answer("model", r#""openai/gpt-4o""#)
        .answer("const.customer_directory", r#""customers.json""#)
        .answer(
            "const.refund_endpoint",
            r#""https://payments.corp-tests.com/refund""#,
        )
        .answer(
            "const.refund_policy",
            r#""Unused purchases within 14 days, cap EUR 100.""#,
        ),
    )
    .unwrap();
    assert_eq!(compiled.status, CompileStatus::Ready, "{compiled:#?}");
    let wf = nika_schema::parse(
        &compiled.candidate.unwrap(),
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .unwrap();
    let report = nika_check::check(&wf);
    let draft = json!({"body":"Hello Ada, we received your question.","facts_used":[{"claim":"Customer is Ada","anchor":anchor,"source":"customer"}]});
    let provider_http = MockHttp::new().enqueue_ok(200, json!({"choices":[{"message":{"content":draft.to_string()},"finish_reason":"stop"}],"usage":{"prompt_tokens":20,"completion_tokens":30}}).to_string());
    let registry = Arc::new(ProviderRegistry::new(
        Arc::new(provider_http),
        ProvidersConfig::default().with_key("openai", Secret::new("injected-test-only")),
    ));
    let business = Arc::new(MockHttp::new().enqueue_ok(200, "{\"accepted\":true}"));
    let human = Arc::new(Human {
        answer,
        messages: Mutex::new(Vec::new()),
    });
    let tools = Arc::new(BuiltinDispatcher::new(
        Arc::new(MockFs::new().with_file("customers.json", br#"{"c1":{"name":"Ada"}}"#.to_vec())),
        Arc::clone(&business),
        Arc::new(MockClock::new()),
        Arc::new(NullEmitter::default()),
        Arc::clone(&human),
        Arc::new(NoWorkflow::default()),
    ));
    let invoke = Arc::new(InvokeVerb::new(tools));
    let mut vars = BTreeMap::from([
        ("ticket".to_owned(), json!("Private ticket text")),
        ("customer_id".to_owned(), json!("c1")),
    ]);
    if let Some(request) = request {
        vars.insert("refund_request".to_owned(), request);
    }
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        InferVerb::new(registry, "openai/gpt-4o"),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        MockClock::new(),
        RuntimeConfig::default(),
    )
    .with_var_overrides(vars);
    let mut sink = VecSink::new();
    let result = runtime
        .run(&wf, &report, &mut DeterministicStamper::new(), &mut sink)
        .await
        .unwrap();
    let messages = human.messages.lock().unwrap().clone();
    let bodies = business
        .sent_requests()
        .iter()
        .map(|r| serde_json::from_slice(r.body.as_ref().unwrap()).unwrap())
        .collect();
    (result.ok, messages, bodies)
}

#[tokio::test]
async fn no_refund_requires_no_amount_currency_or_human_and_completes() {
    let (ok, prompts, posts) = run(None, None).await;
    assert!(ok);
    assert!(prompts.is_empty());
    assert!(posts.is_empty());
}

#[tokio::test]
async fn refund_requires_exact_fresh_approval_and_minimal_payload() {
    let proposed = json!({"amount":25,"currency":"EUR"});
    let (ok, prompts, posts) = run(Some(proposed.clone()), None).await;
    assert!(!ok);
    assert_eq!(prompts.len(), 1);
    assert!(posts.is_empty());
    let (ok, prompts, posts) = run(Some(proposed.clone()), Some(false)).await;
    assert!(ok);
    assert_eq!(prompts.len(), 1);
    assert!(posts.is_empty());
    let (ok, prompts, posts) = run(Some(proposed), Some(true)).await;
    assert!(ok);
    let expected = json!({"customer_id":"c1","amount":25,"currency":"EUR"});
    assert_eq!(posts, vec![expected]);
    for value in ["c1", "25", "EUR"] {
        assert!(prompts[0].contains(value));
    }
    assert!(posts[0].get("ticket").is_none());
}

#[tokio::test]
async fn incomplete_or_invalid_refund_proposals_never_prompt_or_post() {
    for proposal in [
        json!({"amount":25}),
        json!({"currency":"EUR"}),
        json!({"amount":0,"currency":"EUR"}),
        json!({"amount":25,"currency":""}),
    ] {
        let (ok, prompts, posts) = run(Some(proposal), Some(true)).await;
        assert!(!ok);
        assert!(prompts.is_empty());
        assert!(posts.is_empty());
    }
}

#[tokio::test]
async fn invented_anchor_refuses_before_human_or_refund() {
    let (ok, prompts, posts) = run_with_anchor(
        Some(json!({"amount":25,"currency":"EUR"})),
        Some(true),
        "The customer is Ada.",
    )
    .await;
    assert!(!ok);
    assert!(prompts.is_empty());
    assert!(posts.is_empty());
}
