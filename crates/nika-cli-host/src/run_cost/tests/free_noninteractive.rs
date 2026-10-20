// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Host review and controlled transport evidence; no live provider is called.

use super::*;
use nika_kernel::ai::provider::{InferRequest, Message, Role};
use nika_kernel::http::{HttpError, HttpPostDyn, HttpRequest, HttpResponse, HttpStreamResponse};
use nika_kernel::secret::Secret;
use nika_providers::{AdmissionState, ProviderRegistry, ProvidersConfig};
use nika_types::cost::Cost;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

const WIRE_MODEL: &str = "nvidia/nemotron-3-super-120b-a12b:free";
const ENDPOINT: &str = "https://openrouter.ai/api/v1/chat/completions";

struct FreeHttp {
    calls: AtomicUsize,
    response: Value,
}

impl HttpPostDyn for FreeHttp {
    fn supports_single_attempt(&self) -> bool {
        true
    }

    async fn post(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(request.url, ENDPOINT);
        assert!(!request.follow_redirects);
        let body: Value =
            serde_json::from_slice(request.body.as_ref().expect("request body")).expect("JSON");
        assert_eq!(body["model"], WIRE_MODEL);
        assert_eq!(body["max_tokens"], 64);
        Ok(HttpResponse::new(
            200,
            BTreeMap::new(),
            self.response.to_string().into(),
            ENDPOINT,
        ))
    }

    async fn send_streaming(&self, _: HttpRequest) -> Result<HttpStreamResponse, HttpError> {
        panic!("bounded text inference must not stream");
    }
}

fn response(positive_charge: bool) -> Value {
    let mut body = json!({
        "id": "free-host-response", "model": WIRE_MODEL,
        "choices": [{"message": {"content": "ready"}, "finish_reason": "stop"}],
        "usage": {
            "prompt_tokens": 100, "completion_tokens": 20, "total_tokens": 120,
            "prompt_tokens_details": {"cached_tokens": 0},
            "completion_tokens_details": {"reasoning_tokens": 2},
            "cost": 0, "is_byok": false,
            "cost_details": {"upstream_inference_cost": null}
        }
    });
    if positive_charge {
        body["usage"]["cost"] = json!(0.01);
    }
    body
}

/// The settled charge: the provider-reported `usage.cost`, known even when it
/// exceeds the declared-free tariff's zero reservation.
fn charge(positive_charge: bool) -> Cost {
    if positive_charge {
        Cost::new(10_000_000)
    } else {
        Cost::zero()
    }
}

fn assert_admission_observation(cost: &RunCost, positive_charge: bool) {
    let expected = Some(charge(positive_charge));
    let receipt = cost.account.snapshot().expect("host observer receipt");
    assert_eq!(
        receipt,
        cost.config
            .inference_admission
            .as_ref()
            .expect("runtime observer")
            .snapshot()
            .expect("runtime observer receipt")
    );
    assert!(receipt.scoped_to_declared_free && receipt.unknown_cost.is_none());
    assert_eq!(receipt.attempts.len(), 1);
    assert!(receipt.attempts[0].sent);
    assert_eq!(receipt.attempts[0].estimated, expected);
    assert_eq!(receipt.unknown_calls, 0);
    assert_eq!(receipt.state, AdmissionState::Open, "never Uncertain");
    assert_eq!(
        receipt.attempts[0]
            .note
            .contains("the catalogue tariff is below the provider's charge; refresh it"),
        positive_charge,
        "{}",
        receipt.attempts[0].note
    );
    assert_eq!(
        receipt.billed, None,
        "reported charges do not prove an invoice"
    );
    let observation = receipt.observation();
    assert_eq!(
        observation["attempts"][0]["estimated_nano_usd"],
        json!(charge(positive_charge).nano_usd.to_string())
    );
    assert!(observation["billed_nano_usd"].is_null());
}

async fn observe_without_review_channel(positive_charge: bool) {
    let root = tempfile::tempdir().expect("isolated project root");
    let file = root.path().join("free.nika");
    let source = format!(
        "nika: free\nmodel: {FREE}\npermits: {{}}\ntasks:\n  draft:\n    infer: {{ prompt: text, max_tokens: 64 }}\n"
    );
    let cost = review_with_model(
        root.path(),
        file.to_str().expect("fixture path"),
        &source,
        "free-host-run".into(),
        &free_wf("", ""),
        None,
        &free_plan(),
        &Inputs::new(),
        None,
        ReviewChannel::Unavailable,
    )
    .expect("the exact free route never asks an unavailable channel")
    .expect("nonzero bounded text work has an observer");
    assert!(cost.account.observes_declared_free_only());
    assert!(cost.journal.is_none());
    assert!(
        !root.path().join(".nika").exists(),
        "no journal or project lease"
    );
    cost.observe("prepared").expect("journal-free observation");
    let admission = cost
        .config
        .inference_admission
        .as_ref()
        .expect("runtime observer");
    assert!(admission.observes_declared_free_only());
    let transport = Arc::new(FreeHttp {
        calls: AtomicUsize::new(0),
        response: response(positive_charge),
    });
    let provider = ProviderRegistry::new(
        transport.clone(),
        ProvidersConfig::new().with_key("openrouter", Secret::new("fixture")),
    )
    .with_inference_admission(admission.clone())
    .resolve(FREE)
    .expect("exact route with an injected transport");
    let mut request = InferRequest::new(FREE, vec![Message::text(Role::User, "text")]);
    request.max_tokens = Some(64);
    let (_, report) = provider
        .infer_reported(request)
        .await
        .expect("the answer is delivered with its settled charge");
    assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
    assert_eq!(report.attempts, 1);
    let [call] = report.inference_calls.as_slice() else {
        panic!("exactly one dispatch observation");
    };
    assert_eq!(call.requested_endpoint.as_deref(), Some(ENDPOINT));
    assert_eq!(
        call.route.as_ref().map(|route| route.endpoint.as_str()),
        Some(ENDPOINT)
    );
    let expected = Some(charge(positive_charge));
    assert_eq!(call.estimated_usd, expected);
    assert_eq!(call.known_estimate(), expected);
    assert_admission_observation(&cost, positive_charge);
    cost.finish().expect("close this observer");
    assert!(
        !file.exists(),
        "the observer path never rereads a consent file"
    );
    assert!(
        !root.path().join(".nika").exists(),
        "no journal or project lease"
    );
}

#[tokio::test]
async fn a_free_run_without_a_review_channel_observes_complete_zero_usage() {
    observe_without_review_channel(false).await;
}

#[tokio::test]
async fn a_free_run_without_a_review_channel_settles_a_positive_charge_known() {
    observe_without_review_channel(true).await;
}
