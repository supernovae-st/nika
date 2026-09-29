// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Seam captures of an explicit reasoning effort (R4 B16). Every body below is the one the
//! adapter handed the kernel HTTP effect on the provider's own endpoint, captured by the test
//! double: hermetic evidence of the dispatched bytes, never a network capture.

use std::sync::Arc;

use nika_kernel::ai::provider::{
    InferRequest, Message, ProviderError, ProviderInferDyn, ProviderStreamDyn, ReasoningEffort,
    ReasoningWire, ResponseFormat, Role,
};
use nika_kernel::secret::Secret;
use serde_json::{Value, json};

use crate::registry::{ProviderRegistry, ProvidersConfig, ResolvedProvider};
use crate::test_support::{FakeHttp, collect, resolved_with};

const PRO: &str = "deepseek/deepseek-v4-pro";
const ANSWER: &str = r#"{"model":"deepseek-v4-pro","choices":[{"message":{"content":"{}"},"finish_reason":"stop"}],"usage":{"prompt_tokens":9,"completion_tokens":3}}"#;
const STREAM: &str = concat!(
    "data: {\"choices\":[{\"delta\":{\"content\":\"{}\"},\"finish_reason\":null}]}\n\n",
    "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
    "data: [DONE]\n\n",
);

/// A structured, authoring-shaped request under `cap`, naming `level` when given.
fn request(cap: u32, level: Option<ReasoningEffort>) -> InferRequest {
    let mut req = InferRequest::new("m", vec![Message::text(Role::User, "pick one")]);
    req.max_tokens = Some(cap);
    req.response_format = ResponseFormat::JsonSchema(json!({"type": "object"}));
    req.reasoning_effort = level;
    req
}

/// Every body the double captured, as JSON.
fn sent(fake: &FakeHttp) -> Vec<Value> {
    fake.captured()
        .iter()
        .map(|r| serde_json::from_slice(r.body.as_ref().expect("body")).expect("json"))
        .collect()
}

/// A route resolved under the operator's base-URL override.
fn overridden(fake: &Arc<FakeHttp>, model: &str, url: &str) -> ResolvedProvider<FakeHttp> {
    let id = model.split('/').next().unwrap_or_default();
    let config = ProvidersConfig::new()
        .with_key(id, Secret::new("fixture"))
        .with_base_url(id, url);
    ProviderRegistry::new(Arc::clone(fake), config)
        .resolve(model)
        .expect("resolves")
}

/// The refusal's reason, when the error is the local admission refusal.
fn denied(error: &ProviderError) -> Option<&str> {
    match error {
        ProviderError::AdmissionDenied { reason } => Some(reason),
        ProviderError::Observed { source, .. } => denied(source),
        _ => None,
    }
}

#[tokio::test]
async fn max_rides_the_direct_deepseek_route_as_two_structural_keys() {
    let max = || ReasoningWire::new(Some("enabled".into()), Some("max".into()));
    for cap in [2048, 16_384] {
        let fake = FakeHttp::with_json(200, ANSWER);
        let provider = resolved_with(&fake, PRO, "fixture");
        let response = provider
            .infer(request(cap, Some(ReasoningEffort::Max)))
            .await
            .expect("answer");
        let bodies = sent(&fake);
        assert_eq!(bodies.len(), 1, "{cap}");
        assert_eq!(bodies[0]["thinking"], json!({"type": "enabled"}), "{cap}");
        assert_eq!(bodies[0]["reasoning_effort"], "max", "{cap}: never low");
        assert_eq!(bodies[0]["max_tokens"], cap, "the cap is the caller's");
        assert_eq!(
            response.reasoning_wire,
            Some(max()),
            "read back from the bytes"
        );

        let fake = FakeHttp::with_stream(200, STREAM, 7);
        let provider = resolved_with(&fake, PRO, "fixture");
        let events = collect(
            provider
                .infer_stream(request(cap, Some(ReasoningEffort::Max)))
                .await
                .expect("opens"),
        )
        .await;
        assert!(events.iter().all(Result::is_ok), "{events:?}");
        let bodies = sent(&fake);
        assert_eq!(bodies.len(), 1, "stream {cap}");
        assert_eq!(bodies[0]["stream"], true);
        assert_eq!(bodies[0]["thinking"], json!({"type": "enabled"}), "stream");
        assert_eq!(bodies[0]["reasoning_effort"], "max", "stream {cap}");
    }
}

#[tokio::test]
async fn no_level_keeps_the_route_bytes_and_the_wire_says_what_went() {
    for (cap, route_default) in [(2048, Some("low")), (16_384, None)] {
        let fake = FakeHttp::with_json(200, ANSWER);
        let provider = resolved_with(&fake, PRO, "fixture");
        let response = provider.infer(request(cap, None)).await.expect("answer");
        let body = &sent(&fake)[0];
        assert!(body.get("thinking").is_none(), "{cap}");
        assert_eq!(
            body.get("reasoning_effort").and_then(Value::as_str),
            route_default,
            "{cap}"
        );
        // Nothing was configured, and the wire reports the route's own default all the same.
        assert_eq!(
            response.reasoning_wire,
            Some(ReasoningWire::new(None, route_default.map(str::to_owned)))
        );
    }
}

/// Assert both doors refuse `max` on `provider` with nothing sent.
async fn refuses_on_both_doors(
    provider: &ResolvedProvider<FakeHttp>,
    fake: &FakeHttp,
    label: &str,
) {
    let error = provider
        .infer(request(2048, Some(ReasoningEffort::Max)))
        .await
        .expect_err(label);
    let reason = denied(&error).unwrap_or_else(|| panic!("{label}: {error}"));
    assert!(
        reason.contains("`max`") && reason.contains("nothing was sent"),
        "{label}: {reason}"
    );
    let Err(error) = provider
        .infer_stream(request(2048, Some(ReasoningEffort::Max)))
        .await
    else {
        panic!("{label}: the stream opened");
    };
    assert!(denied(&error).is_some(), "{label} stream: {error}");
    assert!(fake.captured().is_empty(), "{label}: a byte left");
}

#[tokio::test]
async fn an_unqualified_route_refuses_before_any_byte_on_both_doors() {
    for model in [
        "deepseek/deepseek-flash",
        "deepseek/deepseek-chat",
        "openai/deepseek-v4-pro",
        "openrouter/deepseek/deepseek-v4-pro",
        "anthropic/claude-sonnet-4-20250514",
        "gemini/gemini-2.5-pro",
        "mock/echo",
    ] {
        let fake = FakeHttp::with_stream(200, STREAM, 7);
        let provider = resolved_with(&fake, model, "fixture");
        refuses_on_both_doors(&provider, &fake, model).await;
    }
    for url in [
        "http://127.0.0.1:9/v1/chat/completions",
        "https://api.deepseek.com/v1/chat/completions/",
        "http://api.deepseek.com/v1/chat/completions",
        "https://api.deepseek.com/v1/chat/completions?beta=true",
        "https://gateway.example/v1/chat/completions",
    ] {
        let fake = FakeHttp::with_stream(200, STREAM, 7);
        let provider = overridden(&fake, PRO, url);
        refuses_on_both_doors(&provider, &fake, url).await;
    }
}

#[tokio::test]
async fn a_level_never_rides_beside_a_budget_or_a_raw_reasoning_key() {
    let tweaks: [fn(&mut InferRequest); 3] = [
        |r| r.thinking_budget = Some(1024),
        |r| {
            r.extra
                .params
                .insert("reasoning_effort".into(), json!("low"));
        },
        |r| {
            r.extra
                .params
                .insert("thinking".into(), json!({"type": "disabled"}));
        },
    ];
    for tweak in tweaks {
        let fake = FakeHttp::with_json(200, ANSWER);
        let provider = resolved_with(&fake, PRO, "fixture");
        let mut req = request(2048, Some(ReasoningEffort::Max));
        tweak(&mut req);
        let error = provider.infer(req).await.expect_err("refused");
        assert!(denied(&error).is_some(), "{error}");
        assert!(fake.captured().is_empty());
    }
}
