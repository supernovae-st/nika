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
        let error = status_error(429, body.as_bytes(), Some("2.5"), "m");
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
        "m",
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
fn malformed_and_hostile_bodies_never_become_diagnostics() {
    for body in [
        "sk-secret-private-prompt",
        r#"{"error":{"code":"sk-secret-private-prompt","type":"private_intent","message":"sk-secret-private-prompt"}}"#,
        r#"{"error":{"code":{"nested":"sk-secret-private-prompt"},"type":12},"message":"sk-secret-private-prompt"}"#,
        r#"{"error":{"message":"insufficient_quota"}}"#,
        "null",
        "[]",
    ] {
        for status in [400, 401, 403, 404, 429, 500] {
            let error = status_error(
                status,
                body.as_bytes(),
                Some("Bearer sk-secret"),
                "private-model",
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
                for secret in [
                    "sk-secret",
                    "private_intent",
                    "private-model",
                    "insufficient_quota",
                ] {
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
    let error = stream_status_error(response, "m").await;
    let ProviderError::HttpResponse { details } = error else {
        panic!("metadata missing")
    };
    assert_eq!(details.status(), 429);
    assert_eq!(details.code(), None);
    assert_eq!(details.retry_after_ms(), Some(1000));
}
