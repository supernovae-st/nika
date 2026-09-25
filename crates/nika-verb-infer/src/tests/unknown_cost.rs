// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Real `InferVerb` + real admitted API adapter; only the kernel HTTP is canned.
use super::*;
use nika_providers::admission::{HardMonetaryCap, UnknownCostChoice, UnknownCostPolicy};
use nika_providers::{AdmissionState, InferenceAdmission};
use nika_types::cost::Cost;

const MODEL: &str = "openai/gpt-oss-120b";
const ENDPOINT: &str = "https://api.scaleway.ai/example-project/v1/chat/completions";
struct SingleAttempt(Arc<SeamHttp>);
impl HttpPostDyn for SingleAttempt {
    fn supports_single_attempt(&self) -> bool {
        true
    }
    async fn post(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
        self.0.post(request).await
    }
    async fn send_streaming(&self, request: HttpRequest) -> Result<HttpStreamResponse, HttpError> {
        self.0.send_streaming(request).await
    }
}

fn choice(bound: u32) -> UnknownCostChoice {
    UnknownCostChoice::new(
        "candidate".into(),
        "invocation".into(),
        "openai".into(),
        "gpt-oss-120b".into(),
        ENDPOINT.into(),
        bound,
        32,
        std::time::Duration::from_secs(120),
    )
    .expect("finite exact choice")
}
fn account(bound: u32) -> InferenceAdmission {
    let policy = UnknownCostPolicy::new(
        true,
        HardMonetaryCap::Absent,
        HardMonetaryCap::Absent,
        HardMonetaryCap::Absent,
        None,
        None,
    );
    InferenceAdmission::new_unknown(choice(bound), policy)
        .expect("choice")
        .for_scope("candidate", "invocation")
        .expect("bound scope")
}
fn verb(
    seam: &Arc<SeamHttp>,
    admission: &InferenceAdmission,
    endpoint: &str,
) -> InferVerb<SingleAttempt> {
    let config = ProvidersConfig::new()
        .with_key("openai", Secret::new("fixture-not-a-key"))
        .with_base_url("openai", endpoint);
    let registry = Registry::new(Arc::new(SingleAttempt(seam.clone())), config)
        .with_inference_admission(admission.clone());
    InferVerb::new(Arc::new(registry), MODEL)
}
fn input() -> InferInput {
    let mut input = InferInput::new("Return a grounded body and facts_used");
    input.max_tokens = Some(32);
    input.schema = Some(json!({"type":"object", "properties":{
        "body":{"type":"string"}, "facts_used":{"type":"array"}},
        "required":["body","facts_used"], "additionalProperties":false}));
    input
}
fn response(text: &str) -> String {
    json!({"model":"gpt-oss-120b", "id":"fixture-request",
        "choices":[{"message":{"content":text},"finish_reason":"stop"}],
        "usage":{"prompt_tokens":10,"completion_tokens":3,"total_tokens":13}})
    .to_string()
}

#[tokio::test]
async fn all_schema_reasks_use_the_same_scoped_admission_and_token_limit() {
    let count = 1 + u32::from(DEFAULT_SCHEMA_RETRY_BUDGET);
    let mut bodies = vec![response("not json"); count as usize];
    *bodies.last_mut().expect("at least one") = response(r#"{"body":"summary","facts_used":[]}"#);
    let seam = SeamHttp::with_json(&bodies.iter().map(String::as_str).collect::<Vec<_>>());
    let account = account(count);
    let verb = verb(&seam, &account, ENDPOINT);
    let out = verb.run(input()).await.expect("last allowed schema answer");
    assert_eq!(out.transport.attempts, count);
    assert_eq!(out.usage.input_tokens, 10 * u64::from(count));
    assert_eq!(out.usage.output_tokens, 3 * u64::from(count));
    assert!(
        verb.run(input()).await.is_err(),
        "choice exhausted; no next request"
    );
    let sent = seam.captured();
    assert_eq!(sent.len(), count as usize);
    for request in sent {
        assert_eq!(request.url, ENDPOINT);
        let body: serde_json::Value =
            serde_json::from_slice(request.body.as_ref().expect("body")).expect("json");
        assert_eq!(body["model"], "gpt-oss-120b");
        assert_eq!(body["max_tokens"], 32);
    }
    let receipt = account.snapshot().expect("snapshot");
    assert_eq!(receipt.unknown_calls, count as usize);
    assert!(
        receipt
            .unknown_attempts
            .iter()
            .all(|r| r.estimated.is_none())
    );
}

#[tokio::test]
async fn a_smaller_explicit_bound_stops_schema_reasks_before_extra_transport() {
    let bad = response("not json");
    let seam = SeamHttp::with_json(&[&bad, &bad, &bad]);
    let account = account(1);
    assert!(verb(&seam, &account, ENDPOINT).run(input()).await.is_err());
    assert_eq!(seam.captured().len(), 1);
    assert_eq!(account.snapshot().expect("snapshot").unknown_calls, 1);
}

#[tokio::test]
async fn wrong_endpoint_model_scope_closed_choice_and_zero_admit_no_http() {
    let seam = SeamHttp::with_json(&[]);
    let account = account(3);
    assert!(
        account
            .for_scope("changed source hash", "invocation")
            .is_err()
    );
    assert!(
        account
            .for_scope("candidate", "another invocation")
            .is_err()
    );
    assert!(
        verb(
            &seam,
            &account,
            "https://api.scaleway.ai/other-project/v1/chat/completions"
        )
        .run(input())
        .await
        .is_err()
    );
    let mut wrong_model = input();
    wrong_model.model = Some("openai/another-model".into());
    assert!(
        verb(&seam, &account, ENDPOINT)
            .run(wrong_model)
            .await
            .is_err()
    );
    account.close("declined or cancelled").expect("closed");
    assert!(verb(&seam, &account, ENDPOINT).run(input()).await.is_err());
    let zero = InferenceAdmission::new(Cost::zero()).expect("zero constraint");
    assert!(verb(&seam, &zero, ENDPOINT).run(input()).await.is_err());
    assert!(seam.captured().is_empty());
}

#[tokio::test]
async fn unsuccessful_transport_cannot_be_schema_retried() {
    for status in [429, 503] {
        let body = response("not json");
        let seam = SeamHttp::with_answers(&[(status, &body)]);
        let account = account(3);
        assert!(verb(&seam, &account, ENDPOINT).run(input()).await.is_err());
        assert_eq!(seam.captured().len(), 1);
        let receipt = account.snapshot().expect("snapshot");
        assert_eq!(receipt.unknown_calls, 1);
        assert!(receipt.unknown_attempts[0].estimated.is_none());
        assert_eq!(receipt.state, AdmissionState::Uncertain);
    }
}

#[tokio::test]
async fn complete_answers_without_usage_consume_the_explicit_unknown_request_bound() {
    let count = 1 + u32::from(DEFAULT_SCHEMA_RETRY_BUDGET);
    let mut body: serde_json::Value = serde_json::from_str(&response("not json")).expect("json");
    body.as_object_mut().expect("object").remove("usage");
    let body = body.to_string();
    let bodies = vec![body.as_str(); count as usize];
    let seam = SeamHttp::with_json(&bodies);
    let account = account(count);
    let verb = verb(&seam, &account, ENDPOINT);
    assert!(verb.run(input()).await.is_err(), "schema budget exhausted");
    assert!(
        verb.run(input()).await.is_err(),
        "no fresh request allowance"
    );
    assert_eq!(seam.captured().len(), count as usize);
    let receipt = account.snapshot().expect("snapshot");
    assert_eq!(receipt.unknown_calls, count as usize);
    assert_eq!(receipt.state, AdmissionState::Open);
    assert!(
        receipt
            .unknown_attempts
            .iter()
            .all(|attempt| attempt.estimated.is_none() && attempt.native_estimated_nano.is_none())
    );
}

#[test]
fn explicit_unknown_choice_cannot_override_any_hard_zero_cap() {
    for index in 0..3 {
        let mut caps = [HardMonetaryCap::Absent; 3];
        caps[index] = HardMonetaryCap::Capped(Cost::zero());
        let policy = UnknownCostPolicy::new(true, caps[0], caps[1], caps[2], None, None);
        assert!(InferenceAdmission::new_unknown(choice(3), policy).is_err());
    }
}
