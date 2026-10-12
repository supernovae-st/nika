// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A request over the window, told apart by the route's known window and the
//! engine's own estimate, never by the provider's words.

use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, Message, ProviderHttpError, ProviderInferDyn as _,
    Role, StopReason, TokenUsage, ToolDef,
};

use super::*;
use crate::test_support::{FakeHttp, resolved_with};

/// An Anthropic 400 that names no overflow: what a prompt over the window gets.
const INVALID: &str = r#"{"type":"error","error":{"type":"invalid_request_error",
    "message":"prompt is too long"}}"#;

fn invalid_request() -> ProviderError {
    ProviderError::HttpResponse {
        details: ProviderHttpError::new(400, None, Some("invalid_request_error"), None),
    }
}

/// The text the request carries counts; an image's bytes count nothing.
#[test]
fn the_estimate_counts_text_four_bytes_a_token_and_never_an_image() {
    let mut request = InferRequest::new("m", vec![Message::text(Role::User, "a".repeat(4_000))]);
    assert_eq!(estimate_input_tokens(&request), 1_000);
    request.messages.push(Message::new(
        Role::User,
        vec![ContentBlock::Image {
            source: "b".repeat(1_000_000),
            detail: None,
        }],
    ));
    assert_eq!(
        estimate_input_tokens(&request),
        1_000,
        "an image counts nothing"
    );
    request.tools.push(ToolDef::new(
        "write",
        "d".repeat(396),
        serde_json::json!({}),
    ));
    assert_eq!(
        estimate_input_tokens(&request),
        1_100,
        "the tool definitions count"
    );
}

/// The window is the provider catalog's limit for the exact wire model.
#[test]
fn the_route_window_comes_from_the_catalog_for_the_exact_model() {
    assert_eq!(
        route_window("anthropic", "claude-sonnet-4-6"),
        Some(200_000)
    );
    assert_eq!(
        route_window("anthropic", "claude-sonnet-4"),
        None,
        "no prefix match"
    );
    assert_eq!(route_window("nowhere", "claude-sonnet-4-6"), None);
}

/// A 400 `invalid_request_error` is an overflow only when the estimate reaches
/// a known window; a documented overflow gains the window; nothing else moves.
#[test]
fn a_400_is_an_overflow_only_when_the_estimate_reaches_a_known_window() {
    let overflow = classified(invalid_request(), 200_000, Some(200_000));
    assert!(
        matches!(
            overflow,
            ProviderError::ContextOverflow {
                limit: Some(200_000),
                details: Some(_)
            }
        ),
        "{overflow:?}"
    );
    let under = classified(invalid_request(), 199_999, Some(200_000));
    assert!(
        matches!(under, ProviderError::HttpResponse { .. }),
        "{under:?}"
    );
    let unknown = classified(invalid_request(), u64::MAX, None);
    assert!(
        matches!(unknown, ProviderError::HttpResponse { .. }),
        "{unknown:?}"
    );
    let filled = classified(
        ProviderError::ContextOverflow {
            limit: None,
            details: None,
        },
        0,
        Some(128_000),
    );
    assert!(
        matches!(
            filled,
            ProviderError::ContextOverflow {
                limit: Some(128_000),
                ..
            }
        ),
        "{filled:?}"
    );
    let rate = ProviderError::HttpResponse {
        details: ProviderHttpError::new(429, None, Some("invalid_request_error"), None),
    };
    assert!(
        matches!(
            classified(rate, u64::MAX, Some(1)),
            ProviderError::HttpResponse { .. }
        ),
        "a 429 stays itself"
    );
}

/// The Anthropic case end to end: a prompt the engine estimates at the route's
/// window comes back as a typed overflow naming the window; the same 400 on a
/// small prompt stays the provider's own refusal.
#[tokio::test]
async fn an_anthropic_400_over_the_window_is_a_typed_overflow() {
    let asked = |text: String| {
        let mut request =
            InferRequest::new("claude-sonnet-4-6", vec![Message::text(Role::User, text)]);
        request.max_tokens = Some(64);
        request
    };
    let big = asked("a".repeat(800_000));
    let http = FakeHttp::with_json(400, INVALID);
    let error = (resolved_with(&http, "anthropic/claude-sonnet-4-6", "test-key").infer(big))
        .await
        .expect_err("the provider refused");
    assert!(
        matches!(
            error.unobserved(),
            ProviderError::ContextOverflow {
                limit: Some(200_000),
                details: Some(_)
            }
        ),
        "{error:?}"
    );
    assert!(!error.is_transient());

    let small = asked("a short prompt".to_owned());
    let http = FakeHttp::with_json(400, INVALID);
    let error = (resolved_with(&http, "anthropic/claude-sonnet-4-6", "test-key").infer(small))
        .await
        .expect_err("the provider refused");
    assert!(
        matches!(error.unobserved(), ProviderError::HttpResponse { .. }),
        "{error:?}"
    );
}

/// A success reporting more input than the window holds is marked window-full;
/// one within the window, or with no reported usage, is not.
#[test]
fn a_success_over_the_window_is_marked_window_full() {
    let answer =
        |input: u64| InferResponse::new(Vec::new(), TokenUsage::new(input, 8), StopReason::EndTurn);
    let mut over = answer(200_001);
    mark_silent_overflow(&mut over, Some(200_000));
    assert!(over.window_full);
    assert_eq!(
        over.stop_reason,
        StopReason::EndTurn,
        "the stop stays the provider's"
    );
    let mut within = answer(200_000);
    mark_silent_overflow(&mut within, Some(200_000));
    assert!(!within.window_full);
    let mut unreported = answer(200_001).with_usage_reported(false);
    mark_silent_overflow(&mut unreported, Some(200_000));
    assert!(!unreported.window_full);
    let mut unknown = answer(u64::MAX);
    mark_silent_overflow(&mut unknown, None);
    assert!(!unknown.window_full);
}
