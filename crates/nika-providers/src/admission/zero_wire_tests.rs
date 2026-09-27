// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Controlled wire proof, not live provider qualification.
use super::*;
use crate::{ProviderRegistry, ProvidersConfig};
use nika_kernel::ai::provider::{InferRequest, Message, ProviderInferDyn, Role};
use nika_kernel::http::{HttpError, HttpPostDyn, HttpRequest, HttpResponse, HttpStreamResponse};
use nika_kernel::secret::Secret;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

const MODEL: &str = "qwen/qwen3.8-27b:free";
const ENDPOINT: &str = "https://openrouter.ai/api/v1/chat/completions";

struct ZeroHttp {
    calls: AtomicUsize,
    body: Value,
}
impl HttpPostDyn for ZeroHttp {
    fn supports_single_attempt(&self) -> bool {
        true
    }
    async fn post(&self, req: HttpRequest) -> Result<HttpResponse, HttpError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(req.url, ENDPOINT);
        assert!(!req.follow_redirects);
        let body: Value = serde_json::from_slice(req.body.as_ref().expect("body")).expect("json");
        assert_eq!(body["model"], MODEL);
        assert_eq!(body["max_tokens"], 512);
        Ok(HttpResponse::new(
            200,
            BTreeMap::new(),
            self.body.to_string().into(),
            ENDPOINT,
        ))
    }
    async fn send_streaming(&self, _: HttpRequest) -> Result<HttpStreamResponse, HttpError> {
        panic!("bounded inference must not stream");
    }
}

fn response() -> Value {
    json!({
        "id":"zero-admission-1", "model":MODEL,
        "choices":[{"message":{"content":"ready"}, "finish_reason":"stop"}],
        "usage":{
            "prompt_tokens":100, "completion_tokens":20, "total_tokens":120,
            "prompt_tokens_details":{"cached_tokens":0},
            "completion_tokens_details":{"reasoning_tokens":2},
            "cost":0, "is_byok":false,
            "cost_details":{"upstream_inference_cost":null}
        }
    })
}

fn with_defect(defect: &str) -> Value {
    let mut body = response();
    match defect {
        "missing-model" => {
            body.as_object_mut().expect("object").remove("model");
        }
        "other-model" => body["model"] = json!("some-other-model"),
        "missing-cost" => {
            body["usage"].as_object_mut().expect("usage").remove("cost");
        }
        "positive-cost" => body["usage"]["cost"] = json!(0.01),
        "byok" => body["usage"]["is_byok"] = json!(true),
        "missing-byok" => {
            body["usage"]
                .as_object_mut()
                .expect("usage")
                .remove("is_byok");
        }
        "missing-input" => {
            body["usage"]
                .as_object_mut()
                .expect("usage")
                .remove("prompt_tokens");
        }
        "null-cost" => body["usage"]["cost"] = Value::Null,
        "string-cost" => body["usage"]["cost"] = json!("0"),
        "negative-cost" => body["usage"]["cost"] = json!(-0.01),
        "upstream-cost" => {
            body["usage"]["cost_details"]["upstream_inference_cost"] = json!(0.1);
        }
        "unknown-cost-detail" => body["usage"]["cost_details"]["new_fee"] = json!(0),
        "null-cost-details" => body["usage"]["cost_details"] = Value::Null,
        "tool-cost" => body["usage"]["cost_details"]["server_tool_cost"] = json!(0.1),
        "null-tool-cost" => body["usage"]["cost_details"]["server_tool_cost"] = Value::Null,
        "cache-write" => {
            body["usage"]["prompt_tokens_details"]["cache_write_tokens"] = json!(1);
        }
        "audio" => body["usage"]["completion_tokens_details"]["audio_tokens"] = json!(1),
        "server-tools" => body["usage"]["server_tool_use"] = json!({"web_search_requests":1}),
        "no-cost-details" => {
            body["usage"]
                .as_object_mut()
                .expect("usage")
                .remove("cost_details");
        }
        "documented-zero-axes" => {
            body["usage"]["prompt_tokens_details"] =
                json!({"cached_tokens":0,"cache_write_tokens":0,"audio_tokens":0,"video_tokens":0});
            body["usage"]["completion_tokens_details"] =
                json!({"reasoning_tokens":2,"audio_tokens":0,"image_tokens":0});
            body["usage"]["cost_details"]["server_tool_cost"] = json!(0);
        }
        "zero-server-tools" => body["usage"]["server_tool_use"] = json!({"web_search_requests":0}),
        "false-media" => body["usage"]["completion_tokens_details"]["audio_tokens"] = json!(false),
        "unknown-zero-axis" => body["usage"]["completion_tokens_details"]["new_axis"] = json!(0),
        "null-server-tools" => body["usage"]["server_tool_use"] = Value::Null,
        "floating-zero" => body["usage"]["cost"] = json!(0.0),
        "extra-axis" => body["usage"]["new_billable_axis"] = json!(0),
        "bad-total" => body["usage"]["total_tokens"] = json!(1),
        _ => {}
    }
    body
}

#[tokio::test]
async fn exact_zero_is_sent_and_settled_but_contradictions_never_become_free() {
    for defect in [
        "none",
        "missing-model",
        "other-model",
        "missing-cost",
        "positive-cost",
        "byok",
        "extra-axis",
        "bad-total",
        "missing-byok",
        "missing-input",
        "null-cost",
        "string-cost",
        "negative-cost",
        "upstream-cost",
        "unknown-cost-detail",
        "null-cost-details",
        "tool-cost",
        "null-tool-cost",
        "cache-write",
        "audio",
        "server-tools",
        "no-cost-details",
        "floating-zero",
        "documented-zero-axes",
        "zero-server-tools",
        "false-media",
        "unknown-zero-axis",
        "null-server-tools",
    ] {
        let body = with_defect(defect);
        let transport = Arc::new(ZeroHttp {
            calls: AtomicUsize::new(0),
            body,
        });
        let account = InferenceAdmission::observe_declared_free()
            .for_scope("candidate", "invocation")
            .expect("host binding preserves route scope");
        let model = format!("openrouter/{MODEL}");
        let provider = ProviderRegistry::new(
            transport.clone(),
            ProvidersConfig::new().with_key("openrouter", Secret::new("fixture")),
        )
        .with_inference_admission(account.clone())
        .resolve(&model)
        .expect("resolve");
        let mut request = InferRequest::new(&model, vec![Message::text(Role::User, "ready?")]);
        request.max_tokens = Some(512);
        let result = provider.infer(request.clone()).await;
        assert_eq!(transport.calls.load(Ordering::SeqCst), 1, "{defect}");
        let receipt = account.snapshot().expect("receipt");
        assert_eq!(receipt.billed, None);
        if matches!(
            defect,
            "none"
                | "no-cost-details"
                | "floating-zero"
                | "null-cost-details"
                | "documented-zero-axes"
                | "zero-server-tools"
        ) {
            assert!(result.is_ok(), "{result:?}");
            assert_eq!(receipt.attempts[0].estimated, Some(Cost::zero()));
            assert_eq!(receipt.unknown_calls, 0);
            assert!(
                receipt.attempts[0]
                    .note
                    .contains("provider-reported usage.cost=0")
            );
            assert!(
                receipt.observation()["attempts"][0]["note"]
                    .as_str()
                    .expect("note")
                    .contains("invoice unknown")
            );
        } else {
            assert!(result.is_err(), "{defect}");
            assert_eq!(receipt.state, AdmissionState::Uncertain, "{defect}");
            assert_eq!(receipt.attempts[0].estimated, None, "{defect}");
            assert_eq!(receipt.unknown_calls, 1, "{defect}");
            assert!(provider.infer(request).await.is_err());
            assert_eq!(
                transport.calls.load(Ordering::SeqCst),
                1,
                "no retry: {defect}"
            );
        }
    }
}

#[tokio::test]
async fn free_observation_preserves_mixed_routes_and_rejects_unqualified_free_shapes() {
    let http = Arc::new(ZeroHttp {
        calls: AtomicUsize::new(0),
        body: response(),
    });
    let account = InferenceAdmission::observe_declared_free()
        .for_scope("candidate", "invocation")
        .expect("scope");
    let config = ProvidersConfig::new()
        .with_key("openrouter", Secret::new("fixture"))
        .with_key("deepseek", Secret::new("fixture"));
    let registry = ProviderRegistry::new(http.clone(), config.clone())
        .with_inference_admission(account.clone());
    for model in [
        "mock/echo",
        "deepseek/deepseek-v4-pro",
        "openrouter/vendor/unseen:free",
    ] {
        assert!(
            registry
                .resolve(model)
                .expect("resolve")
                .admission
                .is_none(),
            "{model}"
        );
    }
    let altered = ProviderRegistry::new(
        http.clone(),
        config.with_base_url("openrouter", "https://gateway.invalid/v1/chat/completions"),
    )
    .with_inference_admission(account.clone());
    let model = format!("openrouter/{MODEL}");
    assert!(
        altered
            .resolve(&model)
            .expect("override")
            .admission
            .is_none()
    );
    let free = registry.resolve(&model).expect("free");
    assert!(free.admission.is_some());
    let mut request = InferRequest::new(&model, vec![Message::text(Role::User, "ready?")]);
    request.max_tokens = Some(512);
    request.thinking_budget = Some(16);
    assert!(
        free.infer(request.clone()).await.is_err(),
        "unqualified thinking refuses before transport"
    );
    assert_eq!(http.calls.load(Ordering::SeqCst), 0);
    request.thinking_budget = None;
    assert!(free.infer(request.clone()).await.is_ok());
    account.close("this account is revoked").expect("close");
    assert!(free.infer(request).await.is_err());
    let mock = registry.resolve("mock/echo").expect("mock");
    assert!(
        mock.infer(InferRequest::new(
            "mock/echo",
            vec![Message::text(Role::User, "hello")]
        ))
        .await
        .is_ok()
    );
    assert_eq!(http.calls.load(Ordering::SeqCst), 1);
    let receipt = account.snapshot().expect("receipt");
    assert_eq!(
        receipt.attempts.len(),
        1,
        "mock and excluded routes are outside this subtotal"
    );
    assert!(receipt.unbudgeted);
}

#[test]
fn all_route_accounts_keep_their_original_strict_admission() {
    let config = ProvidersConfig::new().with_key("deepseek", Secret::new("fixture"));
    let registry = ProviderRegistry::without_http(config)
        .with_inference_admission(InferenceAdmission::unbudgeted());
    assert!(
        registry
            .resolve("mock/echo")
            .expect("mock")
            .admission
            .is_some()
    );
    let selected = InferenceAdmission::observe_declared_free();
    assert!(
        selected
            .reserve(
                "deepseek",
                "deepseek-v4-pro",
                "https://api.deepseek.com/v1/chat/completions",
                512
            )
            .is_err()
    );
    assert!(
        selected
            .check_route(
                "deepseek",
                "deepseek-v4-pro",
                "https://api.deepseek.com/v1/chat/completions"
            )
            .is_err()
    );
    assert!(
        selected
            .snapshot()
            .expect("untouched observer")
            .refusal
            .is_none()
    );
}
