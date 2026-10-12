// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Hermetic HTTP failure classification and disclosure regressions.

use super::*;
use nika_kernel::prelude::NikaErrorCode;

#[test]
fn quota_exhaustion_is_terminal_even_with_retry_after() {
    for body in [
        r#"{"error":{"code":"credit_balance_exhausted","type":"insufficient_quota"}}"#,
        r#"{"error":{"type":"insufficient_quota"}}"#,
        r#"{"error":{"code":"credit_balance_exhausted"}}"#,
    ] {
        let error = status_error(429, body.as_bytes(), Some("2.5"), None);
        assert!(!error.is_transient());
        assert!(!NikaErrorCode::is_transient(&error));
        assert_eq!(error.nika_code().num, 330);
        let ProviderError::HttpResponse { details } = &error else {
            panic!("{error:?}")
        };
        assert_eq!(details.status(), 429);
        assert!(details.is_quota_exhausted());
        assert_eq!(details.retry_after_ms(), Some(2500));
        assert_eq!(details.retry_after(), Some("2.5"));
        let diagnostic = error.to_string();
        assert!(diagnostic.contains("quota exhausted"));
        assert!(diagnostic.contains("automatic retry disabled"));
        assert!(diagnostic.contains("usage and billing unknown"));
    }
}

#[test]
fn transient_rate_limit_preserves_safe_evidence() {
    let error = status_error(
        429,
        br#"{"error":{"code":"rate_limit_exceeded","type":"tokens","message":"wait"}}"#,
        Some("3"),
        None,
    );
    assert!(error.is_transient());
    assert_eq!(error.nika_code().num, 332);
    let ProviderError::HttpResponse { details } = error else {
        panic!("metadata missing")
    };
    assert_eq!(details.code(), Some("rate_limit_exceeded"));
    assert_eq!(details.error_type(), Some("tokens"));
    assert_eq!(details.retry_after_ms(), Some(3000));
}

#[test]
fn gemini_names_its_delay_in_the_body_and_its_status_as_the_type() {
    // The Gemini API sends no Retry-After header: google.rpc.RetryInfo
    // rides the body. The backoff must read it, bounded like the header.
    let body = br#"{"error":{"code":429,"message":"quota","status":"RESOURCE_EXHAUSTED",
        "details":[{"@type":"type.googleapis.com/google.rpc.ErrorInfo","reason":"RATE_LIMIT_EXCEEDED"},
                   {"@type":"type.googleapis.com/google.rpc.RetryInfo","retryDelay":"39s"}]}}"#;
    let error = status_error(429, body, None, None);
    let ProviderError::HttpResponse { details } = &error else {
        panic!("{error:?}")
    };
    assert_eq!(details.status(), 429);
    assert_eq!(details.retry_after(), Some("39"));
    assert_eq!(details.retry_after_ms(), Some(39_000));
    assert_eq!(details.error_type(), Some("RESOURCE_EXHAUSTED"));
    assert_eq!(details.code(), None, "a numeric code is not an identifier");
    assert!(error.is_transient());
    // A header still wins over the body; a fractional delay parses; a
    // hostile delay is dropped by the same bounded parser.
    let with_header = status_error(429, body, Some("2"), None);
    let ProviderError::HttpResponse { details } = &with_header else {
        panic!()
    };
    assert_eq!(details.retry_after_ms(), Some(2000));
    let fractional = br#"{"error":{"details":[{"retryDelay":"0.750s"}]}}"#;
    let ProviderError::HttpResponse { details } = status_error(429, fractional, None, None) else {
        panic!()
    };
    assert_eq!(details.retry_after_ms(), Some(750));
    let hostile = br#"{"error":{"details":[{"retryDelay":"\r\nAuthorization: x"}]}}"#;
    let ProviderError::HttpResponse { details } = status_error(429, hostile, None, None) else {
        panic!()
    };
    assert_eq!(details.retry_after(), None);
}

/// Hostile bodies never become decisions: the provider's words are relayed, its unknown
/// identifiers dropped, and neither the key the call sent nor a credential shape survives.
#[test]
fn malformed_and_hostile_bodies_never_become_decisions_or_leak_a_key() {
    let key = Secret::new("private-model-key");
    for body in [
        "sk-secret-private-prompt",
        r#"{"error":{"code":"sk-secret-private-prompt","type":"private_intent","message":"sk-secret-private-prompt"}}"#,
        r#"{"error":{"code":{"nested":"sk-secret-private-prompt"},"type":12},"message":"sk-secret-private-prompt"}"#,
        r#"{"error":{"message":"insufficient_quota private-model-key"}}"#,
        "null",
        "[]",
    ] {
        for status in [400, 401, 403, 404, 429, 500] {
            let error = status_error(
                status,
                body.as_bytes(),
                Some("Bearer sk-secret"),
                Some(&key),
            );
            let ProviderError::HttpResponse { details } = &error else {
                panic!("{error:?}")
            };
            assert_eq!(details.status(), status);
            assert_eq!(details.code(), None);
            assert_eq!(details.error_type(), None);
            assert_eq!(details.retry_after(), None);
            assert_eq!(details.retry_after_ms(), None);
            assert!(!details.is_quota_exhausted(), "never classify prose");
            for rendered in [error.to_string(), format!("{error:?}")] {
                for secret in ["sk-secret", "private_intent", "private-model-key"] {
                    assert!(!rendered.contains(secret), "{rendered}");
                }
            }
        }
    }
}

#[test]
fn retry_after_is_bounded_and_date_form_is_retained_without_a_clock() {
    for header in [
        "NaN",
        "inf",
        "-1",
        "1e12",
        "12345678901234567890123456789012345",
        "\r\nAuthorization: secret",
        "Sun, 99 Nov 1994 08:49:37 GMT",
        "Sun, +1 Nov 1994 08:49:37 GMT",
        "Sun, 06 Nov 1994 08:49:37 key",
    ] {
        let details = ProviderHttpError::new(429, None, None, Some(header));
        assert_eq!(details.retry_after(), None, "{header}");
    }
    let date = "Sun, 06 Nov 1994 08:49:37 GMT";
    let details = ProviderHttpError::new(429, None, None, Some(date));
    assert_eq!(details.retry_after(), Some(date));
    assert_eq!(details.retry_after_ms(), None);
    assert!(details.is_transient());
}

#[tokio::test]
async fn oversized_stream_body_is_capped_and_still_reports_status() {
    use std::collections::BTreeMap;
    let body = Bytes::from(vec![b'x'; 65 * 1024]);
    let response = nika_kernel::http::HttpStreamResponse::new(
        429,
        BTreeMap::from([("retry-after".into(), "1".into())]),
        "https://unused.invalid",
        None,
        Box::pin(super::tests::Q([Ok(body)].into())),
    );
    let error = stream_status_error(response, None).await;
    let ProviderError::HttpResponse { details } = error else {
        panic!("metadata missing")
    };
    assert_eq!(details.status(), 429);
    assert_eq!(details.code(), None);
    assert_eq!(details.retry_after_ms(), Some(1000));
}

/// The documented overflow code turns a 400 into a typed context overflow: terminal, with the
/// provider's sanitized answer kept. The same words in prose classify nothing, and 429 · 503 ·
/// 529 stay transient rejections whatever code rides them.
#[test]
fn an_overflow_status_with_its_documented_code_is_a_typed_overflow() {
    let body = br#"{"error":{"message":"This model's maximum context length is 128000 tokens.",
        "type":"invalid_request_error","param":"messages","code":"context_length_exceeded"}}"#;
    let error = status_error(400, body, None, None);
    let ProviderError::ContextOverflow { limit, details } = &error else {
        panic!("{error:?}")
    };
    assert_eq!(*limit, None, "the wire does not know the route's window");
    let details = details.as_ref().expect("the provider's answer is kept");
    assert_eq!(details.code(), Some("context_length_exceeded"));
    assert_eq!(details.error_type(), Some("invalid_request_error"));
    assert!(!error.is_transient());
    assert_eq!(error.nika_code().num, 330);
    assert!(crate::retry::transient_rejection(&error).is_none());
    assert!(
        error
            .to_string()
            .starts_with("the request exceeds the model's context window: "),
        "{error}"
    );

    let prose = status_error(
        400,
        br#"{"error":{"message":"context_length_exceeded: maximum context length reached","type":"invalid_request_error"}}"#,
        None,
        None,
    );
    assert!(
        matches!(prose, ProviderError::HttpResponse { .. }),
        "prose classifies nothing: {prose:?}"
    );
    for status in [429, 503, 529] {
        let transient = status_error(status, body, Some("1"), None);
        assert!(
            crate::retry::transient_rejection(&transient).is_some(),
            "{status} stays a transient rejection: {transient:?}"
        );
    }
}

/// `OpenRouter` documents its overflow class beside a numeric code, in
/// `error.metadata.error_type`: that structured field types the overflow too.
#[test]
fn openrouters_documented_error_type_is_a_typed_overflow() {
    let body = br#"{"error":{"code":400,"message":"This endpoint's maximum context length is 8192 tokens.",
        "metadata":{"error_type":"context_length_exceeded","provider_name":"Example"}}}"#;
    let error = status_error(400, body, None, None);
    assert!(
        matches!(
            error,
            ProviderError::ContextOverflow {
                limit: None,
                details: Some(_)
            }
        ),
        "{error:?}"
    );
    let other = status_error(
        400,
        br#"{"error":{"code":400,"message":"bad","metadata":{"error_type":"unknown_class"}}}"#,
        None,
        None,
    );
    assert!(
        matches!(other, ProviderError::HttpResponse { .. }),
        "{other:?}"
    );
}
