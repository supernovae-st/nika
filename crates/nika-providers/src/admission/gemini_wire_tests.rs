// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A Gemini route reserves, sends once and settles its complete usage at its tariff.
//! Controlled wire proof on the admitted catalog rows, not live provider qualification.
use super::*;
use crate::{ProviderRegistry, ProvidersConfig};
use nika_kernel::ai::provider::{InferRequest, Message, Role};
use nika_kernel::http::{HttpError, HttpPostDyn, HttpRequest, HttpResponse, HttpStreamResponse};
use nika_kernel::secret::Secret;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

const MODEL: &str = "gemini/gemini-3.1-pro-preview";
const ENDPOINT: &str = "https://generativelanguage.googleapis.com/v1beta/models/gemini-3.1-pro-preview:generateContent";

/// Answers each post, in order, with the next usage metadata.
struct GeminiHttp {
    calls: AtomicUsize,
    usages: Vec<Value>,
}
impl HttpPostDyn for GeminiHttp {
    fn supports_single_attempt(&self) -> bool {
        true
    }
    async fn post(&self, req: HttpRequest) -> Result<HttpResponse, HttpError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(req.url, ENDPOINT);
        assert!(!req.follow_redirects);
        let body: Value = serde_json::from_slice(req.body.as_ref().expect("body")).expect("json");
        assert_eq!(body["generationConfig"]["maxOutputTokens"], 4_000);
        let reply = json!({
            "candidates": [{"content": {"role": "model", "parts": [{"text": "46"}]},
                            "finishReason": "STOP"}],
            "usageMetadata": self.usages[call],
            "modelVersion": "gemini-3.1-pro-preview",
            "responseId": format!("g-{call}"),
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

/// A live reply of 2026-10-16 under `maxOutputTokens: 4000`: 24 prompt tokens, 2 candidate
/// tokens and 1,568 thoughts.
fn thought(tier: &str) -> Value {
    json!({
        "promptTokenCount": 24, "candidatesTokenCount": 2, "totalTokenCount": 1594,
        "promptTokensDetails": [{"modality": "TEXT", "tokenCount": 24}],
        "thoughtsTokenCount": 1568, "serviceTier": tier
    })
}

fn wired(
    usages: Vec<Value>,
) -> (
    Arc<GeminiHttp>,
    InferenceAdmission,
    ProviderRegistry<GeminiHttp>,
) {
    let transport = Arc::new(GeminiHttp {
        calls: AtomicUsize::new(0),
        usages,
    });
    let account = InferenceAdmission::new(Cost::new(1_000_000_000)).expect("one dollar");
    let registry = ProviderRegistry::new(
        transport.clone(),
        ProvidersConfig::new().with_key("gemini", Secret::new("fixture")),
    )
    .with_inference_admission(account.clone());
    (transport, account, registry)
}

fn request() -> InferRequest {
    let mut request = InferRequest::new(MODEL, vec![Message::text(Role::User, "primes?")]);
    request.max_tokens = Some(4_000);
    request
}

/// A Gemini reply's thoughts bill at the output rate beside its candidates: the call settles
/// KNOWN at the tariff, within the reservation of its 200k tier and its requested bound.
#[tokio::test]
async fn a_gemini_reply_settles_at_its_tariff() {
    let (transport, account, registry) = wired(vec![thought("standard")]);
    let provider = registry.resolve(MODEL).expect("admitted route");
    let (response, report) = provider.infer_reported(request()).await.expect("settled");
    assert_eq!(
        response.usage.output_tokens, 1_570,
        "2 candidates + 1,568 thoughts"
    );
    let [call] = report.inference_calls.as_slice() else {
        panic!("one dispatch observation: {report:?}");
    };
    // 24 input at 2,000 + 1,570 output at 12,000 nano-dollars.
    assert_eq!(call.known_estimate(), Some(Cost::new(18_888_000)));
    assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
    let receipt = account.snapshot().expect("receipt");
    assert_eq!(receipt.state, AdmissionState::Open);
    assert_eq!(receipt.estimated, Cost::new(18_888_000));
    assert_eq!(
        receipt.attempts[0].reserved,
        Cost::new(448_000_000),
        "200,000 input at 2,000 + 4,000 output at 12,000"
    );
}

/// A reply served on another tier bills at another rate: its charge stays unknown.
#[tokio::test]
async fn a_reply_on_another_tier_stays_unknown() {
    let (_, account, registry) = wired(vec![thought("priority")]);
    let provider = registry.resolve(MODEL).expect("admitted route");
    let (_, report) = provider
        .infer_reported(request())
        .await
        .expect_err("a priority charge is unknown");
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
