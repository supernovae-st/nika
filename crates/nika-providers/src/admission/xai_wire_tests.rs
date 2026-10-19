// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! An xAI route settles at the charge each reply reports, within a reservation of its whole
//! generation. Controlled wire proof on the admitted catalog rows, not live qualification.
use super::*;
use crate::{ProviderRegistry, ProvidersConfig};
use nika_kernel::ai::provider::{InferRequest, Message, Role};
use nika_kernel::http::{HttpError, HttpPostDyn, HttpRequest, HttpResponse, HttpStreamResponse};
use nika_kernel::secret::Secret;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

const ENDPOINT: &str = "https://api.x.ai/v1/chat/completions";
const GROK: &str = "xai/grok-4.7";
const STALE: &str = "the catalogue tariff is below the provider's charge; refresh it";

/// Answers each post, in order, with the next usage object, echoing the requested model.
struct XaiHttp {
    calls: AtomicUsize,
    usages: Vec<Value>,
}
impl HttpPostDyn for XaiHttp {
    fn supports_single_attempt(&self) -> bool {
        true
    }
    async fn post(&self, req: HttpRequest) -> Result<HttpResponse, HttpError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(req.url, ENDPOINT);
        let body: Value = serde_json::from_slice(req.body.as_ref().expect("body")).expect("json");
        assert_eq!(body["max_tokens"], 40, "the row's output parameter");
        let reply = json!({
            "id": format!("xai-{call}"), "model": body["model"],
            "choices": [{"message": {"content": "46"}, "finish_reason": "stop"}],
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

/// A grok-4.7 reply under `max_tokens: 40`, its completion, reasoning and charge as measured
/// live on 2026-10-16: 1 completion token and 1,973 reasoning tokens beyond that bound,
/// $0.012642, which the row's rates reproduce with 1,263 prompt tokens, 1,152 of them cached.
fn reasoned(sources: u64) -> Value {
    json!({
        "prompt_tokens": 1263, "completion_tokens": 1, "total_tokens": 3237,
        "prompt_tokens_details": {
            "text_tokens": 1263, "audio_tokens": 0, "image_tokens": 0, "cached_tokens": 1152
        },
        "completion_tokens_details": {
            "reasoning_tokens": 1973, "audio_tokens": 0,
            "accepted_prediction_tokens": 0, "rejected_prediction_tokens": 0
        },
        "num_sources_used": sources,
        "cost_in_usd_ticks": 126_420_000
    })
}

fn wired(usages: Vec<Value>) -> (Arc<XaiHttp>, InferenceAdmission, ProviderRegistry<XaiHttp>) {
    let transport = Arc::new(XaiHttp {
        calls: AtomicUsize::new(0),
        usages,
    });
    let account = InferenceAdmission::new(Cost::new(5_000_000_000)).expect("five dollars");
    let registry = ProviderRegistry::new(
        transport.clone(),
        ProvidersConfig::new().with_key("xai", Secret::new("fixture")),
    )
    .with_inference_admission(account.clone());
    (transport, account, registry)
}

fn request() -> InferRequest {
    let mut request = InferRequest::new(GROK, vec![Message::text(Role::User, "primes?")]);
    request.max_tokens = Some(40);
    request
}

/// A reply that reasoned far beyond its requested bound settles KNOWN at its reported charge:
/// the reservation covered the whole generation, so the route stays admitted, and the usage
/// counts the reasoning in the output.
#[tokio::test]
async fn a_reply_reasoning_beyond_its_bound_settles_at_its_charge() {
    let (transport, account, registry) = wired(vec![reasoned(0), reasoned(0)]);
    let provider = registry.resolve(GROK).expect("admitted route");
    for _ in 0..2 {
        let (response, report) = provider.infer_reported(request()).await.expect("settled");
        assert_eq!(response.cost, Some(Cost::new(12_642_000)));
        assert_eq!(
            response.usage.output_tokens, 1_974,
            "1 completion + 1,973 reasoning"
        );
        let [call] = report.inference_calls.as_slice() else {
            panic!("one dispatch observation: {report:?}");
        };
        assert_eq!(call.known_estimate(), Some(Cost::new(12_642_000)));
        let pricing: Value =
            serde_json::from_str(call.pricing.as_deref().expect("provenance")).expect("JSON");
        assert_eq!(pricing["kind"], "provider_reported_charge");
        assert!(
            pricing["source"]
                .as_str()
                .is_some_and(|s| s.starts_with("https://docs.x.ai/")),
            "{pricing}"
        );
    }
    assert_eq!(transport.calls.load(Ordering::SeqCst), 2);
    let receipt = account.snapshot().expect("receipt");
    assert_eq!(receipt.state, AdmissionState::Open);
    assert_eq!(receipt.estimated, Cost::new(25_284_000));
    for attempt in &receipt.attempts {
        assert_eq!(
            attempt.reserved,
            Cost::new(3_399_998_000),
            "199,999 input at 2,000 + the 500,000-token window at 6,000"
        );
        assert!(!attempt.note.contains(STALE), "{}", attempt.note);
    }
}

/// A reply that used a live source carries a charge the text tariff does not cover: unknown.
#[tokio::test]
async fn a_reply_with_a_live_source_stays_unknown() {
    let (_, account, registry) = wired(vec![reasoned(3)]);
    let provider = registry.resolve(GROK).expect("admitted route");
    let (_, report) = provider
        .infer_reported(request())
        .await
        .expect_err("a sourced charge is unknown");
    let [call] = report.inference_calls.as_slice() else {
        panic!("one dispatch observation: {report:?}");
    };
    assert!(!call.usage_complete);
    assert_eq!(call.known_estimate(), None);
    assert_eq!(
        account.snapshot().expect("receipt").state,
        AdmissionState::Uncertain
    );
}

/// An allowance below a call's reservation refuses it before any byte, naming the reservation,
/// what it covers, the remaining allowance and the amount to state: the person can then choose a
/// ceiling, instead of reading an unexplained refusal.
#[test]
fn an_allowance_below_the_reservation_names_the_amount_to_state() {
    let account = InferenceAdmission::new(Cost::new(250_000_000)).expect("a quarter");
    let refused = account
        .reserve("xai", "grok-4.7", ENDPOINT, 40)
        .err()
        .expect("$0.25 cannot cover $3.40");
    let text = refused.to_string();
    for part in [
        "cannot cover the full-context reservation of $3.40",
        "($0.25 of $0.25)",
        "`xai/grok-4.7`",
        "a request cannot bound its reasoning",
        "state at least $3.40",
    ] {
        assert!(text.contains(part), "{part} · {text}");
    }
}
