// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Preserve the existing non-free wire family in a mixed registry.
use super::InferenceAdmission;
use crate::{ProviderRegistry, ProvidersConfig};
use nika_kernel::ai::provider::{InferRequest, Message, ProviderInferDyn, Role};
use nika_kernel::http::{HttpError, HttpPostDyn, HttpRequest, HttpResponse, HttpStreamResponse};
use nika_kernel::secret::Secret;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct ThinkingHttp(AtomicUsize, bool);
impl HttpPostDyn for ThinkingHttp {
    fn supports_single_attempt(&self) -> bool {
        self.1
    }
    async fn post(&self, req: HttpRequest) -> Result<HttpResponse, HttpError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        let body: Value = serde_json::from_slice(req.body.as_ref().expect("body")).expect("json");
        let response = if self.1 {
            assert_eq!(req.url, "https://openrouter.ai/api/v1/chat/completions");
            json!({
                "id":"free-fixture", "model":body["model"],
                "choices":[{"message":{"content":"observed"},"finish_reason":"stop"}],
                "usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15,"cost":0,"is_byok":false}
            })
        } else {
            assert_eq!(req.url, "https://api.anthropic.com/v1/messages");
            assert_eq!(body["thinking"]["budget_tokens"], 2048);
            json!({
            "id":"fixture", "type":"message", "role":"assistant", "model":body["model"],
            "content":[{"type":"text","text":"observed"}], "stop_reason":"end_turn",
            "usage":{"input_tokens":10,"output_tokens":5}
            })
        };
        Ok(HttpResponse::new(
            200,
            BTreeMap::new(),
            response.to_string().into(),
            req.url,
        ))
    }
    async fn send_streaming(&self, _: HttpRequest) -> Result<HttpStreamResponse, HttpError> {
        panic!("non-streaming probe");
    }
}

#[tokio::test]
async fn a_revoked_free_observer_never_rewrites_paid_thinking_into_text_only_compat() {
    let http = Arc::new(ThinkingHttp(AtomicUsize::new(0), false));
    let account = InferenceAdmission::observe_declared_free();
    account
        .close("only this observer is closed")
        .expect("close");
    let registry = ProviderRegistry::new(
        http.clone(),
        ProvidersConfig::new().with_key("anthropic", Secret::new("fixture")),
    )
    .with_inference_admission(account.clone());
    let model = "anthropic/sonnet";
    let mut request = InferRequest::new(model, vec![Message::text(Role::User, "hello")]);
    request.max_tokens = Some(4096);
    request.thinking_budget = Some(2048);
    assert!(
        registry
            .resolve(model)
            .expect("resolve")
            .infer(request)
            .await
            .is_ok()
    );
    assert_eq!(http.0.load(Ordering::SeqCst), 1);
    assert!(account.snapshot().expect("receipt").attempts.is_empty());
}

#[tokio::test]
async fn mixed_routes_keep_separate_http_retry_contracts() {
    let normal = Arc::new(ThinkingHttp(AtomicUsize::new(0), false));
    let bounded = Arc::new(ThinkingHttp(AtomicUsize::new(0), true));
    let account = InferenceAdmission::observe_declared_free();
    assert!(account.observes_declared_free_only());
    assert!(!InferenceAdmission::unbudgeted().observes_declared_free_only());
    let registry = ProviderRegistry::new(
        normal.clone(),
        ProvidersConfig::new()
            .with_key("anthropic", Secret::new("fixture"))
            .with_key("openrouter", Secret::new("fixture")),
    )
    .with_inference_admission_http(account.clone(), bounded.clone());
    for model in ["openrouter/qwen/qwen3.8-27b:free", "anthropic/sonnet"] {
        let mut request = InferRequest::new(model, vec![Message::text(Role::User, "hello")]);
        request.max_tokens = Some(4096);
        if model.starts_with("anthropic/") {
            request.thinking_budget = Some(2048);
        }
        assert!(
            registry
                .resolve(model)
                .expect("resolve")
                .infer(request)
                .await
                .is_ok(),
            "{model}"
        );
    }
    assert_eq!(normal.0.load(Ordering::SeqCst), 1);
    assert_eq!(bounded.0.load(Ordering::SeqCst), 1);
    let receipt = account.snapshot().expect("receipt");
    assert!(receipt.scoped_to_declared_free);
    assert_eq!(receipt.observation()["scoped_to_declared_free"], true);
    assert_eq!(receipt.attempts.len(), 1);
    assert_eq!(
        receipt.attempts[0].estimated,
        Some(nika_types::cost::Cost::zero())
    );
}

/// C4: a Run whose `model:` is rendered at run time keeps every route its own
/// review admits, observes the declared-free one, and refuses before any byte
/// a route that same review would call unknown-cost (one predicate).
#[tokio::test]
async fn a_run_observer_judges_the_rendered_route_with_the_review_predicate() {
    let http = Arc::new(ThinkingHttp(AtomicUsize::new(0), true));
    let config = ProvidersConfig::new()
        .with_key("openrouter", Secret::new("fixture"))
        .with_key("deepseek", Secret::new("fixture"))
        .with_key("anthropic", Secret::new("fixture"))
        .with_key("mistral", Secret::new("fixture"));
    let account = InferenceAdmission::observe_run();
    assert!(
        account.observes_declared_free_only(),
        "transport and cap stay the host's"
    );
    let registry = ProviderRegistry::new(
        Arc::new(ThinkingHttp(AtomicUsize::new(0), false)),
        config.clone(),
    )
    .with_inference_admission_http(account.clone(), http.clone());
    for (model, observed) in [
        ("openrouter/qwen/qwen3.8-27b:free", true),
        ("deepseek/deepseek-v4-pro", false),
        ("anthropic/claude-sonnet-4-5-20250929", false),
        ("ollama/llama3.2", false),
        ("mock/echo", false),
    ] {
        let resolved = registry
            .resolve(model)
            .unwrap_or_else(|e| panic!("{model}: {e}"));
        assert_eq!(resolved.admission.is_some(), observed, "{model}");
    }
    let unknown = [
        "mistral/mistral-small-latest",
        "openrouter/google/gemma-4-26b-a4b-it:free",
        "openrouter/vendor/unseen:free",
        "anthropic/claude-unpriced-fixture",
    ];
    for model in unknown {
        assert!(
            !matches!(super::unknown_cost_route(model, config.clone()), Ok(None)),
            "the review itself would not admit {model}"
        );
        let refused = registry.resolve(model).err().expect(model).to_string();
        assert!(
            refused.contains(model) && refused.contains("unknown USD cost"),
            "{refused}"
        );
    }
    let moved = config
        .clone()
        .with_base_url("deepseek", "https://gateway.invalid/v1/chat/completions");
    let moved =
        ProviderRegistry::new(http.clone(), moved).with_inference_admission(account.clone());
    assert!(
        moved.resolve("deepseek/deepseek-v4-pro").is_err(),
        "an overridden endpoint has no tariff"
    );
    // The observed free route still refuses an unqualified shape before any byte.
    let free = registry
        .resolve("openrouter/qwen/qwen3.8-27b:free")
        .expect("free");
    let image = nika_kernel::ai::provider::ContentBlock::Image {
        source: "https://example.invalid/a.png".into(),
        detail: None,
    };
    let mut request = InferRequest::new(
        "openrouter/qwen/qwen3.8-27b:free",
        vec![Message::new(Role::User, vec![image])],
    );
    request.max_tokens = Some(512);
    assert!(free.infer(request).await.is_err());
    assert_eq!(
        http.0.load(Ordering::SeqCst),
        0,
        "no refused route reached the wire"
    );
    let receipt = account.snapshot().expect("receipt");
    assert_eq!(
        receipt.state,
        super::AdmissionState::Open,
        "a refusal before bytes is no charge"
    );
    assert!(receipt.attempts.is_empty());
    assert!(receipt.refusal.is_some());
    // The library observer keeps its documented pass-through for other routes.
    let library = ProviderRegistry::new(http.clone(), config)
        .with_inference_admission(InferenceAdmission::observe_declared_free());
    for model in unknown {
        assert!(library.resolve(model).is_ok(), "{model}");
    }
}
