// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A paid `OpenRouter` route settles at its reported `usage.cost`.
//! Controlled wire proof on the admitted catalog rows, not live provider qualification.
use super::*;
use crate::{ProviderRegistry, ProvidersConfig};
use nika_kernel::ai::provider::{InferRequest, Message, ProviderInferDyn, Role};
use nika_kernel::http::{HttpError, HttpPostDyn, HttpRequest, HttpResponse, HttpStreamResponse};
use nika_kernel::secret::Secret;
use nika_types::cost::UnpricedReason;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

const ENDPOINT: &str = "https://openrouter.ai/api/v1/chat/completions";
const GLM: &str = "openrouter/z-ai/glm-5.3-flash";
const MIMO: &str = "openrouter/xiaomi/mimo-v2.6-flash";
const STALE: &str = "the catalogue tariff is below the provider's charge; refresh it";

/// Answers each post, in order, with the next usage object, echoing the requested model.
struct PaidHttp {
    calls: AtomicUsize,
    usages: Vec<Value>,
}
impl HttpPostDyn for PaidHttp {
    fn supports_single_attempt(&self) -> bool {
        true
    }
    async fn post(&self, req: HttpRequest) -> Result<HttpResponse, HttpError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(req.url, ENDPOINT);
        assert!(!req.follow_redirects);
        let body: Value = serde_json::from_slice(req.body.as_ref().expect("body")).expect("json");
        assert_eq!(body["max_tokens"], 512, "the row's output parameter");
        assert!(body.get("max_completion_tokens").is_none());
        let reply = json!({
            "id": format!("paid-{call}"), "model": body["model"],
            "choices": [{"message": {"content": "ready"}, "finish_reason": "stop"}],
            "usage": self.usages[call],
        });
        Ok(HttpResponse::new(
            200,
            BTreeMap::new(),
            reply.to_string().into(),
            ENDPOINT,
        ))
    }
    async fn send_streaming(&self, _: HttpRequest) -> Result<HttpStreamResponse, HttpError> {
        panic!("bounded inference must not stream");
    }
}

/// `OpenRouter`'s documented paid, non-BYOK usage shape for 100 + 20 tokens.
fn paid(cost: f64) -> Value {
    json!({
        "prompt_tokens": 100, "completion_tokens": 20, "total_tokens": 120,
        "prompt_tokens_details": {"cached_tokens": 0},
        "completion_tokens_details": {"reasoning_tokens": 2},
        "cost": cost, "is_byok": false,
        "cost_details": {
            "upstream_inference_cost": null,
            "upstream_inference_prompt_cost": cost / 2.0,
            "upstream_inference_completions_cost": cost / 2.0
        }
    })
}

fn wired(
    usages: Vec<Value>,
) -> (
    Arc<PaidHttp>,
    InferenceAdmission,
    ProviderRegistry<PaidHttp>,
) {
    let transport = Arc::new(PaidHttp {
        calls: AtomicUsize::new(0),
        usages,
    });
    let account = InferenceAdmission::new(Cost::new(1_000_000_000)).expect("one dollar");
    let registry = ProviderRegistry::new(
        transport.clone(),
        ProvidersConfig::new().with_key("openrouter", Secret::new("fixture")),
    )
    .with_inference_admission(account.clone());
    (transport, account, registry)
}

fn request(model: &str) -> InferRequest {
    let mut request = InferRequest::new(model, vec![Message::text(Role::User, "ready?")]);
    request.max_tokens = Some(512);
    request
}

/// Correct paid replies settle KNOWN at their reported charges (`OpenRouter`'s routed
/// endpoint is cheaper than the peak row), the catalog price of the same usage stays beside
/// them, and the account stays Open across calls.
#[tokio::test]
async fn correct_paid_replies_settle_known_at_their_reported_charge_and_stay_open() {
    // 4.15e-5 USD = 41_500 nano; the row's peak prices this usage at 100*225 + 20*1600.
    let (transport, account, registry) = wired(vec![paid(4.15e-5), paid(4.15e-5)]);
    let provider = registry.resolve(GLM).expect("admitted route");
    assert!(
        provider.admission.is_some(),
        "the paid row admits the route"
    );
    let mut calls = Vec::new();
    for _ in 0..2 {
        let (response, report) = provider
            .infer_reported(request(GLM))
            .await
            .expect("settled");
        assert_eq!(response.cost, Some(Cost::new(41_500)));
        calls.extend(report.inference_calls);
    }
    assert_eq!(transport.calls.load(Ordering::SeqCst), 2);
    let receipt = account.snapshot().expect("receipt");
    assert_eq!(receipt.state, AdmissionState::Open);
    assert_eq!(receipt.unknown_calls, 0);
    assert_eq!(receipt.held_unknown, Cost::zero());
    assert_eq!(receipt.active, Cost::zero());
    assert_eq!(receipt.estimated, Cost::new(83_000));
    for attempt in &receipt.attempts {
        assert_eq!(attempt.estimated, Some(Cost::new(41_500)));
        assert_eq!(attempt.reported_estimate, Some(Cost::new(54_500)));
        assert_eq!(
            attempt.reserved,
            Cost::new(59_801_600),
            "262144*225 + 512*1600"
        );
        assert!(
            attempt.note.contains("provider-reported usage.cost") && !attempt.note.contains(STALE),
            "{}",
            attempt.note
        );
    }
    for call in &calls {
        assert_eq!(call.known_estimate(), Some(Cost::new(41_500)));
        let pricing: Value =
            serde_json::from_str(call.pricing.as_deref().expect("provenance")).expect("JSON");
        assert_eq!(pricing["kind"], "provider_reported_charge");
    }
    let (known, unknown) = crate::spend::spend_for_calls(&calls);
    assert!(
        known.is_some_and(|usd| (usd - 8.3e-5).abs() < 1e-12),
        "{known:?}"
    );
    assert_eq!(unknown, None);
}

/// A BYOK reply's charge is not the provider's whole bill: it stays unknown, the account goes
/// Uncertain and the next call refuses before transport.
#[tokio::test]
async fn a_byok_reply_stays_unknown() {
    let mut byok = paid(4.15e-5);
    byok["is_byok"] = json!(true);
    let (transport, account, registry) = wired(vec![byok]);
    let provider = registry.resolve(GLM).expect("admitted route");
    let (_, report) = provider
        .infer_reported(request(GLM))
        .await
        .expect_err("a BYOK charge is unknown");
    let [call] = report.inference_calls.as_slice() else {
        panic!("one dispatch observation: {report:?}");
    };
    assert!(!call.usage_complete);
    assert_eq!(call.known_estimate(), None);
    assert_eq!(
        crate::spend::spend_for_calls(std::slice::from_ref(call)),
        (None, Some(UnpricedReason::ProviderDidNotReportUsage))
    );
    let receipt = account.snapshot().expect("receipt");
    assert_eq!(receipt.state, AdmissionState::Uncertain);
    assert_eq!(receipt.attempts[0].estimated, None);
    assert_eq!(receipt.unknown_calls, 1);
    assert!(provider.infer(request(GLM)).await.is_err());
    assert_eq!(
        transport.calls.load(Ordering::SeqCst),
        1,
        "no second dispatch"
    );
}

/// On a paid row, a charge above the reservation settles KNOWN and refuses
/// only that route's next admission; another admitted route still settles.
#[tokio::test]
async fn a_charge_above_the_reservation_refuses_only_its_route() {
    let (transport, account, registry) = wired(vec![paid(0.5), paid(4.15e-5)]);
    let glm = registry.resolve(GLM).expect("admitted route");
    let (response, _) = glm
        .infer_reported(request(GLM))
        .await
        .expect("the billed answer is delivered");
    assert_eq!(response.cost, Some(Cost::new(500_000_000)));
    let receipt = account.snapshot().expect("receipt");
    assert_eq!(receipt.state, AdmissionState::Open);
    assert_eq!(receipt.attempts[0].estimated, Some(Cost::new(500_000_000)));
    assert!(
        receipt.attempts[0].note.contains(STALE),
        "{}",
        receipt.attempts[0].note
    );
    let refused = glm.infer(request(GLM)).await.expect_err("stale route");
    assert!(refused.to_string().contains(STALE), "{refused}");
    assert_eq!(
        transport.calls.load(Ordering::SeqCst),
        1,
        "refused before transport"
    );
    let mimo = registry.resolve(MIMO).expect("admitted route");
    mimo.infer(request(MIMO))
        .await
        .expect("another route settles");
    let after = account.snapshot().expect("receipt");
    assert_eq!(after.state, AdmissionState::Open, "never Uncertain");
    assert_eq!(after.estimated, Cost::new(500_041_500));
    assert_eq!(after.attempts.len(), 2);
    assert_eq!(transport.calls.load(Ordering::SeqCst), 2);
}

/// A priced gateway route qualifies as a Session selects it, at the
/// gateway's own endpoint, and reserves under a numeric account; the same model
/// behind a configured gateway does not qualify.
#[test]
fn a_priced_gateway_route_qualifies_under_a_numeric_account() {
    let luna = "openrouter/openai/gpt-6-luna";
    InferenceAdmission::qualify_selected(luna, ProvidersConfig::new()).expect("admitted route");
    let account = InferenceAdmission::new(Cost::new(1_000_000_000)).expect("one dollar");
    let attempt = account
        .reserve("openrouter", "openai/gpt-6-luna", ENDPOINT, 1024)
        .expect("reserved under the allowance");
    drop(attempt);
    let gateway = ProvidersConfig::new()
        .with_base_url("openrouter", "https://gateway.example/v1/chat/completions");
    assert!(InferenceAdmission::qualify_selected(luna, gateway).is_err());
}

/// A paid reply that wrote the cache settles at its charge: the write is inside it.
#[tokio::test]
async fn a_paid_cache_write_is_inside_the_reported_charge() {
    let mut written = paid(4.15e-5);
    written["prompt_tokens_details"] = json!({"cached_tokens": 10, "cache_write_tokens": 50});
    let (_, account, registry) = wired(vec![written]);
    let provider = registry.resolve(GLM).expect("admitted route");
    let (response, report) = provider
        .infer_reported(request(GLM))
        .await
        .expect("settled");
    assert_eq!(response.usage.cache_write_tokens, Some(50));
    assert_eq!(
        report.inference_calls[0].known_estimate(),
        Some(Cost::new(41_500))
    );
    let receipt = account.snapshot().expect("receipt");
    assert_eq!(receipt.state, AdmissionState::Open);
    assert_eq!(receipt.attempts[0].estimated, Some(Cost::new(41_500)));
}
