// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The authored selection through the real adapter over the http seam:
//! the exact body the provider received, a refusal with ZERO requests,
//! and the receipt keeping requested, sent and named apart.

use nika_types::access::{AccessProtocol, AccessRequirement};

use super::*;

const FLASH_ANSWER: &str = r#"{"model":"deepseek-flash-2026-09-30","choices":[{"message":{"content":"done"},"finish_reason":"stop"}],"usage":{"prompt_tokens":9,"completion_tokens":3}}"#;

fn deepseek(seam: &Arc<SeamHttp>) -> InferVerb<SeamHttp> {
    let registry = Registry::new(
        Arc::clone(seam),
        ProvidersConfig::new().with_key("deepseek", Secret::new("sk-test")),
    );
    InferVerb::new(Arc::new(registry), "deepseek/deepseek-flash")
}

fn effort(word: &str) -> AccessRequirement {
    AccessRequirement::new()
        .with_via(Some("deepseek".into()))
        .with_protocol(Some(AccessProtocol::Api))
        .with_effort(Some(word.into()))
}

#[tokio::test]
async fn a_declared_effort_rides_the_api_body_as_its_exact_word() {
    let seam = SeamHttp::with_json(&[FLASH_ANSWER]);
    let input = InferInput::new("plan the migration").with_requirement(Some(&effort("high")));
    let out = deepseek(&seam).run(input).await.expect("qualified route");
    let body = wire_body(&seam);
    assert_eq!(body["model"], "deepseek-flash");
    assert_eq!(body["reasoning_effort"], "high");
    assert_eq!(body["thinking"], json!({"type": "enabled"}));
    let receipt = out
        .selection
        .expect("a requirement yields a receipt")
        .to_json();
    assert_eq!(
        receipt,
        json!({
            "schema": "nika/access-selection@1",
            "protocol": "api",
            "model": {"requested": "deepseek/deepseek-flash", "option": "model",
                "transmitted": "deepseek-flash", "configured": null, "configured_source": null},
            "effort": {"requested": "high", "option": "reasoning_effort",
                "transmitted": "high", "configured": null, "configured_source": null},
            "responder": {"model": "deepseek-flash-2026-09-30", "evidence": "api_response"}
        }),
        "the API names its responder; nothing is read back"
    );
}

#[tokio::test]
async fn a_native_word_the_request_cannot_carry_refuses_with_zero_requests() {
    let seam = SeamHttp::with_json(&[FLASH_ANSWER]);
    let input = InferInput::new("plan the migration").with_requirement(Some(&effort("xhigh")));
    let err = deepseek(&seam)
        .run(input)
        .await
        .expect_err("no alias, no translation");
    assert!(
        matches!(
            err,
            VerbInferError::InvalidParam {
                param: "reasoning_effort",
                ..
            }
        ),
        "{err:?}"
    );
    assert!(
        err.to_string().contains("run.reasoning.effort: xhigh"),
        "{err}"
    );
    assert!(seam.captured().is_empty(), "zero inference requests");
}

#[tokio::test]
async fn a_level_the_exact_model_does_not_list_refuses_before_any_byte() {
    let seam = SeamHttp::with_json(&[FLASH_ANSWER]);
    let registry = Registry::new(
        Arc::clone(&seam),
        ProvidersConfig::new().with_key("deepseek", Secret::new("sk-test")),
    );
    let verb = InferVerb::new(Arc::new(registry), "deepseek/deepseek-chat");
    let input = InferInput::new("plan").with_requirement(Some(&effort("high")));
    let err = verb
        .run(input)
        .await
        .expect_err("the wire judges the exact model");
    assert!(err.to_string().contains("not qualified"), "{err}");
    assert!(seam.captured().is_empty(), "zero inference requests");
}

#[tokio::test]
async fn without_a_requirement_nothing_changes_and_no_receipt_rides() {
    let seam = SeamHttp::with_json(&[FLASH_ANSWER]);
    let out = deepseek(&seam)
        .run(InferInput::new("plan the migration"))
        .await
        .expect("route default");
    let body = wire_body(&seam);
    assert!(body.get("reasoning_effort").is_none(), "{body}");
    assert!(body.get("thinking").is_none(), "{body}");
    assert!(out.selection.is_none());
}
