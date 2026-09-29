// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;
use nika_kernel_mock::{MockFs, MockHttp};
use serde_json::{Value, json};

async fn observe(http: &MockHttp, extra: Value) -> BuiltinOutcome {
    let mut args = serde_json::Map::from_iter([
        ("url".to_owned(), json!("https://service.test/probe")),
        ("mode".to_owned(), json!("raw")),
        ("response".to_owned(), json!({"accept": [200, 404]})),
    ]);
    args.extend(extra.as_object().expect("test args").clone());
    super::fetch(http, &MockFs::new(), &FsBoundary::unbounded(), &args).await
}

#[tokio::test]
async fn accepted_status_always_returns_observation_without_response_credentials() {
    for status in [200, 404, 503] {
        let http = MockHttp::new().enqueue_ok_with_headers(
            status,
            [
                ("set-cookie", "SESSION_SECRET"),
                ("www-authenticate", "AUTH_SECRET"),
            ],
            "business body",
        );
        let result = observe(&http, json!({"response": {"accept": [status]}}))
            .await
            .expect("accepted observation");
        assert_eq!(
            result,
            json!({"status_code": status, "url": null, "body": "business body"})
        );
        assert_eq!(http.sent_requests().len(), 1);
        assert!(!result.to_string().contains("SECRET"));
    }
}

#[tokio::test]
async fn exact_set_has_no_implicit_two_hundred_or_status_coercion() {
    for status in [200, 500] {
        let http = MockHttp::new().enqueue_ok(status, "response");
        let fail = observe(&http, json!({"response": {"accept": [404]}}))
            .await
            .expect_err("unlisted status");
        assert_eq!(fail.code, "NIKA-BUILTIN-FETCH-001");
        assert_eq!(fail.transient, status == 500);
        assert_eq!(
            fail.details,
            Some(json!({"status_code": status, "accepted": [404]}))
        );
        assert_eq!(http.sent_requests().len(), 1);
    }
}

#[tokio::test]
async fn malformed_resolved_or_agent_policy_refuses_before_any_request() {
    for response in [
        Value::Null,
        json!(true),
        json!([]),
        json!({}),
        json!({"accept": []}),
        json!({"accept": [404, 404]}),
        json!({"accept": [404.0]}),
        json!({"accept": [true]}),
        json!({"accept": ["404"]}),
        json!({"accept": [199]}),
        json!({"accept": [600]}),
        json!({"accept": [404], "follow": false}),
        json!({"accept": 404}),
        json!({"accept": ["${{ inputs.code }}"]}),
        json!({"accept": (200..217).collect::<Vec<_>>()}),
    ] {
        let http = MockHttp::new();
        let fail = observe(&http, json!({"response": response}))
            .await
            .expect_err("shape");
        assert_eq!(fail.code, "NIKA-BUILTIN-FETCH-001");
        assert!(!fail.transient);
        assert!(http.sent_requests().is_empty());
    }
}

#[tokio::test]
async fn extraction_failure_keeps_known_status_without_fabricating_observation() {
    for (status, body) in [(404, "<html>missing</html>"), (204, "")] {
        let http = MockHttp::new().enqueue_ok(status, body);
        let fail = observe(
            &http,
            json!({"response": {"accept": [status]}, "mode": "jq", "jq": "."}),
        )
        .await
        .expect_err("invalid JSON body");
        assert!(!fail.transient);
        assert_eq!(
            fail.details,
            Some(json!({"status_code": status, "accepted": [status]}))
        );
    }
    let http = MockHttp::new().enqueue_ok(204, "");
    let result = observe(&http, json!({"response": {"accept": [204]}}))
        .await
        .expect("empty raw");
    assert_eq!(result["body"], "");
}

#[tokio::test]
async fn structured_body_and_final_route_are_preserved_without_url_credentials() {
    let http = MockHttp::new().enqueue_ok_final_url(
        200,
        "{\"ok\":true}",
        "https://user:pass@service.test/login?key=hidden#secret",
    );
    let result = observe(&http, json!({"mode": "jq", "jq": "."}))
        .await
        .expect("JSON observation");
    assert_eq!(
        result,
        json!({"status_code": 200, "url": "https://service.test/login", "body": {"ok": true}})
    );
    assert_eq!(http.sent_requests()[0].url, "https://service.test/probe");
}

#[tokio::test]
async fn keyless_effect_failures_keep_effect_safe_retry_law() {
    for method in ["POST", "PUT", "PATCH", "DELETE"] {
        let http = MockHttp::new().enqueue_ok(503, "may already have committed");
        let fail = observe(
            &http,
            json!({"method": method, "response": {"accept": [201, 409]}}),
        )
        .await
        .expect_err("unaccepted ambiguous status");
        assert!(!fail.transient, "{method}");
        assert_eq!(http.sent_requests().len(), 1);
        let http = MockHttp::new().enqueue_ok(409, "conflict");
        let result = observe(
            &http,
            json!({"method": method, "response": {"accept": [201, 409]}}),
        )
        .await
        .expect("named conflict is data");
        assert_eq!(result["status_code"], 409);
        assert_eq!(http.sent_requests().len(), 1);
    }
}

#[tokio::test]
async fn named_status_does_not_swallow_transport_or_security_failures() {
    for (error, code, transient) in [
        (
            HttpError::Timeout { duration_ms: 1 },
            "NIKA-BUILTIN-FETCH-001",
            true,
        ),
        (
            HttpError::Connection {
                reason: "connection refused".into(),
            },
            "NIKA-BUILTIN-FETCH-001",
            true,
        ),
        (
            HttpError::SsrfBlocked {
                url: "http://169.254.169.254/".into(),
            },
            "NIKA-SEC-005",
            false,
        ),
        (
            HttpError::HostNotAllowed {
                host: "outside.test".into(),
            },
            "NIKA-SEC-004",
            false,
        ),
        (
            HttpError::TooLarge {
                size: 67_108_865,
                max: 67_108_864,
            },
            "NIKA-BUILTIN-FETCH-001",
            false,
        ),
        (
            HttpError::Other {
                reason: "too many redirects".into(),
            },
            "NIKA-BUILTIN-FETCH-001",
            false,
        ),
    ] {
        let http = MockHttp::new().enqueue_err(error);
        let fail = observe(&http, json!({})).await.expect_err("not a response");
        assert_eq!(fail.code, code);
        assert_eq!(fail.transient, transient);
        assert!(fail.details.is_none(), "no status fabricated");
        assert_eq!(http.sent_requests().len(), 1);
    }
}

#[tokio::test]
async fn traverse_excludes_observation_and_headers_before_http() {
    for extra in [
        json!({"response": {"accept": [404]}}),
        json!({"headers": {"x-test": "v"}}),
    ] {
        let http = MockHttp::new();
        let mut args = json!({"url": "https://service.test", "traverse": {"max_pages": 1}})
            .as_object()
            .expect("args")
            .clone();
        args.extend(extra.as_object().expect("extra").clone());
        let fail = super::fetch(&http, &MockFs::new(), &FsBoundary::unbounded(), &args)
            .await
            .expect_err("conflicting family");
        assert!(fail.message.contains("excludes"));
        assert!(http.sent_requests().is_empty());
    }
}

#[tokio::test]
async fn head_observation_preserves_empty_body_and_one_head_request() {
    let http = MockHttp::new().enqueue_ok(404, "");
    let result = observe(&http, json!({"method": "HEAD"}))
        .await
        .expect("HEAD observation");
    assert_eq!(result["body"], "");
    assert_eq!(result["status_code"], 404);
    let sent = http.sent_requests();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].method, HttpMethod::Head);
}
