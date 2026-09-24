// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Controlled HTTP seam; these are mechanics, never live provider qualification.
use super::*;
use crate::{ProviderRegistry, ProvidersConfig};
use nika_kernel::ai::provider::{InferRequest, Message, ProviderInferDyn, ProviderStreamDyn, Role};
use nika_kernel::http::{HttpError, HttpPostDyn, HttpRequest, HttpResponse, HttpStreamResponse};
use nika_kernel::secret::Secret;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicUsize, Ordering};

const ENDPOINT: &str = "https://api.deepseek.com/v1/chat/completions";
const MODEL: &str = "deepseek/deepseek-v4-pro";
struct Http {
    calls: AtomicUsize,
    status: u16,
    body: String,
    final_url: String,
    hangs: bool,
    contract: bool,
}
impl HttpPostDyn for Http {
    fn supports_single_attempt(&self) -> bool {
        self.contract
    }
    async fn post(&self, req: HttpRequest) -> Result<HttpResponse, HttpError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(req.url, ENDPOINT);
        assert!(!req.follow_redirects);
        let body: Value = serde_json::from_slice(req.body.as_ref().expect("body")).expect("json");
        assert_eq!(body["model"], "deepseek-v4-pro");
        assert_eq!(body["max_tokens"], 8192);
        if self.hangs {
            std::future::pending::<()>().await;
        }
        Ok(HttpResponse::new(
            self.status,
            std::collections::BTreeMap::default(),
            self.body.clone().into(),
            &self.final_url,
        ))
    }
    async fn send_streaming(&self, _: HttpRequest) -> Result<HttpStreamResponse, HttpError> {
        panic!("streaming must refuse before the effect")
    }
}
fn body() -> Value {
    json!({"id":"admission-1", "model":"deepseek-v4-pro", "choices":[{"message":{"content":"ok"},"finish_reason":"stop"}],
        "usage":{"prompt_tokens":100, "completion_tokens":20, "prompt_cache_hit_tokens":0,
        "prompt_cache_miss_tokens":100, "total_tokens":120,"completion_tokens_details":{"reasoning_tokens":5}}})
}
fn http() -> Http {
    Http {
        calls: AtomicUsize::new(0),
        status: 200,
        body: body().to_string(),
        final_url: ENDPOINT.into(),
        hangs: false,
        contract: true,
    }
}
fn request() -> InferRequest {
    let mut r = InferRequest::new(MODEL, vec![Message::text(Role::User, "hi")]);
    r.max_tokens = Some(8192);
    r
}
fn setup(transport: Http) -> (Arc<Http>, InferenceAdmission, crate::ResolvedProvider<Http>) {
    let transport = Arc::new(transport);
    let account = InferenceAdmission::new(Cost::new(2_000_000_000)).expect("account");
    let provider = ProviderRegistry::new(
        transport.clone(),
        ProvidersConfig::new().with_key("deepseek", Secret::new("test")),
    )
    .with_inference_admission(account.clone())
    .resolve(MODEL)
    .expect("resolve");
    (transport, account, provider)
}
#[tokio::test]
async fn complete_settlement_and_next_call_refusal_are_physical() {
    let (transport, account, provider) = setup(http());
    let (r, report) = provider
        .infer_reported(request())
        .await
        .expect("paid response");
    assert_eq!(report.attempts, 1);
    assert_eq!(r.usage_completeness, UsageCompleteness::Complete);
    let snapshot = account.snapshot().expect("snapshot");
    assert_eq!(
        snapshot.attempts[0].request_id.as_deref(),
        Some("admission-1")
    );
    assert!(snapshot.estimated.nano_usd > 0);
    assert_eq!(snapshot.billed, None);
    account.amend(snapshot.estimated).expect("lower to spent");
    let (_, report) = provider
        .infer_reported(request())
        .await
        .expect_err("no headroom");
    assert_eq!(report.attempts, 0);
    assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn incomplete_invalid_contradictory_and_overbound_usage_are_held() {
    for (key, value) in [
        ("prompt_tokens", json!(null)),
        ("completion_tokens", json!(-1)),
        ("prompt_cache_hit_tokens", json!(101)),
        ("total_tokens", json!(1)),
        ("completion_tokens_details", json!({"reasoning_tokens":21})),
        ("prompt_tokens_details", json!({"cached_tokens":1})),
        ("new_billable_axis", json!(2)),
    ] {
        let mut transport = http();
        let mut b = body();
        b["usage"][key] = value;
        transport.body = b.to_string();
        let (transport, account, provider) = setup(transport);
        assert!(provider.infer(request()).await.is_err(), "{key}");
        assert!(provider.infer(request()).await.is_err());
        assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
        let snapshot = account.snapshot().expect("snapshot");
        assert_eq!(snapshot.state, AdmissionState::Uncertain);
        assert!(snapshot.held_unknown.nano_usd > 0);
        assert_eq!(snapshot.estimated, Cost::zero());
    }
    let mut transport = http();
    let mut b = body();
    b["usage"]["completion_tokens"] = json!(9000);
    b["usage"]["total_tokens"] = json!(9100);
    transport.body = b.to_string();
    let (_, account, provider) = setup(transport);
    assert!(provider.infer(request()).await.is_err());
    let snapshot = account.snapshot().expect("snapshot");
    assert_eq!(
        snapshot.attempts[0]
            .usage
            .as_ref()
            .expect("observed")
            .output_tokens,
        9000
    );
    assert!(snapshot.attempts[0].reported_estimate.is_some());
    assert_eq!(snapshot.attempts[0].estimated, None);
}
#[tokio::test]
async fn errors_redirects_decode_and_cancellation_never_retry_or_refund() {
    for status in [307, 308, 429, 503] {
        let mut transport = http();
        transport.status = status;
        let (transport, account, provider) = setup(transport);
        let (_, r) = provider
            .infer_reported(request())
            .await
            .expect_err("unknown");
        assert_eq!(r.attempts, 1);
        assert!(r.statuses.is_empty());
        assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            account.snapshot().expect("snapshot").state,
            AdmissionState::Uncertain
        );
    }
    for bad in ["{}", "not JSON"] {
        let mut transport = http();
        transport.body = bad.into();
        let (_, account, provider) = setup(transport);
        assert!(provider.infer(request()).await.is_err());
        assert!(account.snapshot().expect("snapshot").held_unknown.nano_usd > 0);
    }
    let mut transport = http();
    transport.final_url = "https://another.example/chat/completions".into();
    let (_, account, provider) = setup(transport);
    assert!(provider.infer(request()).await.is_err());
    assert_eq!(
        account.snapshot().expect("snapshot").state,
        AdmissionState::Uncertain
    );
    let mut transport = http();
    transport.hangs = true;
    let (transport, account, provider) = setup(transport);
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(5),
            provider.infer(request())
        )
        .await
        .is_err()
    );
    assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        account.snapshot().expect("snapshot").state,
        AdmissionState::Uncertain
    );
}
#[tokio::test]
async fn unsupported_inputs_and_effects_refuse_before_post() {
    let (transport, account, provider) = setup(http());
    let mut requests = vec![];
    let mut r = request();
    r.max_tokens = None;
    requests.push(r);
    let mut r = request();
    r.max_tokens = Some(0);
    requests.push(r);
    let mut r = request();
    r.max_tokens = Some(u32::MAX);
    requests.push(r);
    let mut r = request();
    r.model = "deepseek-flash".into();
    requests.push(r);
    let mut r = request();
    r.extra
        .params
        .insert("thinking".into(), json!({"type":"enabled"}));
    requests.push(r);
    let mut r = request();
    r.messages = vec![Message::text(Role::System, "x".repeat(1_048_576))];
    requests.push(r);
    for r in requests {
        assert!(provider.infer(r).await.is_err());
    }
    assert!(provider.infer_stream(request()).await.is_err());
    assert_eq!(transport.calls.load(Ordering::SeqCst), 0);
    assert!(account.snapshot().expect("snapshot").attempts.is_empty());
    let mut transport = http();
    transport.contract = false;
    let (transport, _, provider) = setup(transport);
    assert!(provider.infer(request()).await.is_err());
    assert_eq!(transport.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn configured_gateway_or_endpoint_mismatch_refuses_before_any_effect() {
    for (provider, model, endpoint) in [
        (
            "deepseek",
            MODEL,
            "https://api.deepseek.com.evil.test/v1/chat/completions",
        ),
        (
            "deepseek",
            MODEL,
            "https://api.scaleway.ai/v1/chat/completions",
        ),
        (
            "openai",
            "openai/deepseek-v4-pro",
            "https://api.scaleway.ai/v1/chat/completions",
        ),
        ("deepseek", "deepseek/deepseek-reasoner", ENDPOINT),
    ] {
        let transport = Arc::new(http());
        let account = InferenceAdmission::new(Cost::new(2_000_000_000)).expect("account");
        let provider = ProviderRegistry::new(
            transport.clone(),
            ProvidersConfig::new()
                .with_key(provider, Secret::new("fixture"))
                .with_base_url(provider, endpoint),
        )
        .with_inference_admission(account.clone())
        .resolve(model)
        .expect("resolve config");
        let (_, report) = provider
            .infer_reported(request())
            .await
            .expect_err("unqualified binding");
        assert_eq!(report.attempts, 0);
        assert_eq!(transport.calls.load(Ordering::SeqCst), 0);
        assert!(account.snapshot().expect("receipt").attempts.is_empty());
    }
}

#[tokio::test]
async fn cache_is_a_validated_subset_and_response_identity_is_observed() {
    let mut b = body();
    b["usage"]["prompt_cache_hit_tokens"] = json!(80);
    b["usage"]["prompt_cache_miss_tokens"] = json!(20);
    b["usage"]["prompt_tokens_details"] = json!({"cached_tokens":80});
    let mut transport = http();
    transport.body = b.to_string();
    let (_, account, provider) = setup(transport);
    provider
        .infer(request())
        .await
        .expect("complete cached response");
    assert_eq!(
        account.snapshot().expect("receipt").estimated,
        Cost::new(109_120)
    );
    for response_model in [Value::Null, json!("deepseek-flash")] {
        let mut transport = http();
        let mut b = body();
        b["model"] = response_model;
        transport.body = b.to_string();
        let (_, account, provider) = setup(transport);
        assert!(provider.infer(request()).await.is_err());
        assert_eq!(
            account.snapshot().expect("receipt").state,
            AdmissionState::Uncertain
        );
    }
}

#[tokio::test]
async fn duplicate_response_fields_hold_the_reservation_without_a_retry() {
    let original = body().to_string();
    for duplicate in [
        original.replacen(
            "\"completion_tokens\":",
            "\"completion_tokens\":999999,\"completion_tokens\":",
            1,
        ),
        original.replacen(
            "\"completion_tokens\":",
            "\"completion_tokens\":999999,\"\\u0063ompletion_tokens\":",
            1,
        ),
        original.replacen(
            "\"usage\":",
            "\"usage\":{\"prompt_tokens\":999999},\"usage\":",
            1,
        ),
        original.replacen("\"model\":", "\"model\":\"different-model\",\"model\":", 1),
        original.replacen(
            "\"reasoning_tokens\":",
            "\"reasoning_tokens\":999999,\"reasoning_tokens\":",
            1,
        ),
        original.replacen(
            "\"completion_tokens\":20",
            "\"completion_tokens\":20,\"completion_tokens\":20",
            1,
        ),
    ] {
        // The existing serde_json::Value parse hides the earlier duplicate.
        assert_eq!(
            serde_json::from_str::<Value>(&duplicate).expect("last-wins JSON"),
            body()
        );
        let mut transport = http();
        transport.body = duplicate;
        let (transport, account, provider) = setup(transport);
        let (_, report) = provider
            .infer_reported(request())
            .await
            .expect_err("ambiguous usage");
        assert_eq!(report.attempts, 1);
        assert!(provider.infer(request()).await.is_err());
        assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
        let snapshot = account.snapshot().expect("receipt");
        assert_eq!(snapshot.state, AdmissionState::Uncertain);
        assert!(snapshot.held_unknown.nano_usd > 0);
        assert_eq!(snapshot.estimated, Cost::zero());
        assert_eq!(snapshot.attempts[0].estimated, None);
        assert_eq!(snapshot.billed, None);
    }
}
