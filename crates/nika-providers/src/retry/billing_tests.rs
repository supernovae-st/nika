// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
use super::billing::observe;
use super::{BillingRoute, TransportReport};
use crate::admission::{HardMonetaryCap, InferenceAdmission, UnknownCostPolicy};
use nika_kernel::ai::provider::{
    InferRequest, InferResponse, ProviderInferDyn, StopReason, TokenUsage, UsageCompleteness,
};
use nika_kernel::prelude::NikaErrorCode;
use nika_types::cost::Cost;

fn route(endpoint: &str) -> BillingRoute {
    BillingRoute::new("deepseek".into(), "deepseek-v4-pro".into(), endpoint.into()).expect("safe")
}
fn response() -> InferResponse {
    let mut usage = TokenUsage::new(100, 10);
    usage.cache_read_tokens = Some(80);
    let mut r = InferResponse::new(vec![], usage, StopReason::EndTurn);
    r.usage_completeness = UsageCompleteness::Complete;
    r.gen_ai.response_model = Some("deepseek-v4-pro".into());
    r
}
const OFFICIAL: &str = "https://api.deepseek.com/v1/chat/completions";

#[test]
fn s80_peak_cache_price_requires_exact_route_and_returned_model() {
    let good = route(OFFICIAL);
    let r = response();
    let call = observe(Some(&good), Some(&r), None);
    assert_eq!(call.known_estimate(), Some(Cost::new(69_520)));
    assert_eq!(call.route.as_ref().expect("route").model, "deepseek-v4-pro");
    for endpoint in [
        "https://evil.example/v1/chat/completions",
        "https://api.deepseek.com/v1/other",
        "http://127.0.0.1:8000/v1/chat/completions",
    ] {
        assert_eq!(
            observe(Some(&route(endpoint)), Some(&r), None).known_estimate(),
            None
        );
    }
    let mut wrong = good.clone();
    wrong.model = "deepseek-v4-pro-suffix".into();
    assert_eq!(observe(Some(&wrong), Some(&r), None).known_estimate(), None);
    let mut returned = r.clone();
    returned.gen_ai.response_model = Some("deepseek-v4-flash".into());
    let mismatched = observe(Some(&good), Some(&returned), None);
    assert_eq!(mismatched.known_estimate(), None);
    assert_eq!(
        mismatched.response_model.as_deref(),
        Some("deepseek-v4-flash")
    );
    returned.gen_ai.response_model = None;
    assert_eq!(
        observe(Some(&good), Some(&returned), None).known_estimate(),
        None
    );
}

#[test]
fn s80_absent_or_partial_usage_and_absent_route_are_unknown() {
    let route = route(OFFICIAL);
    let mut r = response();
    r.usage_reported = false;
    let call = observe(Some(&route), Some(&r), None);
    assert!(call.usage.is_none());
    assert_eq!(call.known_estimate(), None);
    r.usage_reported = true;
    r.usage_completeness = UsageCompleteness::Unknown;
    assert_eq!(observe(Some(&route), Some(&r), None).known_estimate(), None);
    r.usage_completeness = UsageCompleteness::Complete;
    assert_eq!(observe(None, Some(&r), None).known_estimate(), None);
    r.usage.cache_read_tokens = Some(101);
    assert_eq!(observe(Some(&route), Some(&r), None).known_estimate(), None);
}

#[test]
fn s80_mixed_routes_keep_individual_estimates_and_eur_stays_unknown_usd() {
    let mut a = TransportReport::new();
    a.record(Some(observe(
        Some(&route(OFFICIAL)),
        Some(&response()),
        None,
    )));
    let mut b = TransportReport::new();
    let eur = BillingRoute::new(
        "openai".into(),
        "gpt-oss-120b".into(),
        "https://api.scaleway.ai/v1/chat/completions".into(),
    )
    .expect("route");
    let mut r = response();
    r.gen_ai.response_model = Some("gpt-oss-120b".into());
    let call = observe(Some(&eur), Some(&r), None);
    assert_eq!(call.known_estimate(), None);
    let provenance: serde_json::Value =
        serde_json::from_str(call.pricing.as_deref().expect("pricing")).expect("JSON");
    assert_eq!(provenance["currency"], "EUR");
    b.record(Some(call));
    a.absorb(&b);
    assert!(a.billing_route.is_none());
    assert_eq!(a.inference_calls.len(), 2);
    assert_eq!(
        a.inference_calls[0].known_estimate(),
        Some(Cost::new(69_520))
    );
    assert_eq!(a.inference_calls[1].known_estimate(), None);
}

#[test]
fn s80_declared_observation_is_never_a_catalog_estimate() {
    use crate::admission::{DeclaredTariff, TariffUnit, UnknownCostChoice};
    let choice = UnknownCostChoice::new(
        "candidate".into(),
        "invocation".into(),
        "deepseek".into(),
        "deepseek-v4-pro".into(),
        OFFICIAL.into(),
        1,
        64,
        std::time::Duration::from_secs(1),
    )
    .expect("choice");
    let declared = DeclaredTariff::new(
        &choice,
        "operator-contract".into(),
        "USD".into(),
        TariffUnit::PerMillionTokens,
        [2.0, 3.0, 1.0],
        "contract-local".into(),
        "v1".into(),
    )
    .expect("tariff");
    let call = observe(Some(&route(OFFICIAL)), Some(&response()), Some(&declared));
    assert_eq!(call.known_estimate(), Some(Cost::new(150_000)));
    let p: serde_json::Value =
        serde_json::from_str(call.pricing.as_deref().expect("pricing")).expect("JSON");
    assert_eq!(p["kind"], "user_declared_estimate_not_invoice");
    assert_eq!(p["provenance"], "contract-local");
    assert!(p.get("table_schema").is_none());
    let proxy = observe(
        Some(&route("https://proxy.example/v1/chat/completions")),
        Some(&response()),
        Some(&declared),
    );
    assert_eq!(proxy.known_estimate(), None);

    // The account's durable reservation must use the same declared label.
    let choice = choice
        .with_declared_tariff(declared)
        .expect("bound declaration");
    let policy = UnknownCostPolicy::new(
        true,
        HardMonetaryCap::Absent,
        HardMonetaryCap::Absent,
        HardMonetaryCap::Absent,
        None,
        None,
    );
    let account = InferenceAdmission::new_unknown(choice, policy)
        .expect("permitted")
        .for_scope("candidate", "invocation")
        .expect("scope");
    let _attempt = account
        .reserve("deepseek", "deepseek-v4-pro", OFFICIAL, 64)
        .expect("reserve");
    assert_eq!(
        account.snapshot().expect("receipt").unknown_attempts[0].pricing["kind"],
        "user_declared_estimate_not_invoice"
    );
}

#[tokio::test]
async fn s80_kernel_failure_after_dispatch_retains_route_and_original_error() {
    use crate::test_support::{FakeHttp, RecordingBackoff, resolved_with_backoff};
    let fake = FakeHttp::with_sequence(&[(500, r#"{"error":{"message":"failed"}}"#, &[])]);
    let clock = RecordingBackoff::new();
    let rp = resolved_with_backoff(&fake, "deepseek/deepseek-v4-pro", "fake", clock.clone());
    let error = ProviderInferDyn::infer(&rp, InferRequest::new("deepseek-v4-pro", vec![]))
        .await
        .expect_err("500");
    assert_eq!(fake.captured().len(), 1);
    assert!(clock.waits().is_empty());
    assert_eq!(error.nika_code().num, 330);
    assert!(error.is_transient());
    let calls = error.inference_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].route.as_ref().expect("response route").endpoint,
        OFFICIAL
    );
    assert!(calls[0].usage.is_none());
    assert_eq!(calls[0].known_estimate(), None);
}

#[tokio::test]
async fn s80_kernel_success_carries_complete_wire_observation() {
    use crate::test_support::{FakeHttp, RecordingBackoff, resolved_with_backoff};
    let body = r#"{"id":"r1","model":"deepseek-v4-pro","choices":[{"message":{"content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":100,"completion_tokens":10,"prompt_cache_hit_tokens":80,"prompt_cache_miss_tokens":20,"total_tokens":110}}"#;
    let fake = FakeHttp::with_sequence(&[(200, body, &[])]);
    let rp = resolved_with_backoff(
        &fake,
        "deepseek/deepseek-v4-pro",
        "fake",
        RecordingBackoff::new(),
    );
    let response = ProviderInferDyn::infer(&rp, InferRequest::new("deepseek-v4-pro", vec![]))
        .await
        .expect("wire");
    assert_eq!(response.inference_calls.len(), 1);
    assert_eq!(
        response.inference_calls[0].known_estimate(),
        Some(Cost::new(69_520))
    );
}

#[tokio::test]
async fn s80_dispatch_without_http_response_keeps_request_endpoint_but_no_final_route() {
    use crate::test_support::{FakeHttp, RecordingBackoff, resolved_with_backoff};
    let fake = FakeHttp::with_sequence(&[]);
    let rp = resolved_with_backoff(
        &fake,
        "deepseek/deepseek-v4-pro",
        "fake",
        RecordingBackoff::new(),
    );
    let error = ProviderInferDyn::infer(&rp, InferRequest::new("deepseek-v4-pro", vec![]))
        .await
        .expect_err("no HTTP response");
    assert_eq!(fake.captured().len(), 1);
    let calls = error.inference_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].requested_endpoint.as_deref(), Some(OFFICIAL));
    assert!(calls[0].route.is_none());
    assert!(calls[0].usage.is_none());
    assert_eq!(calls[0].known_estimate(), None);
}
