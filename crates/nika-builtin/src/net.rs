// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Network builtins (2) — fetch · notify (stdlib §Network).
//!
//! Both compose the injected kernel http seam — SSRF defense lives in the
//! L1 http effect (3-layer · s5), this layer never re-implements it.

use nika_kernel::io::http::{HttpGetDyn, HttpMethod, HttpPostDyn, HttpRequest};

use crate::{Args, BuiltinFailure, BuiltinOutcome, opt_str, req_str};

/// `nika:fetch` — HTTP request + content extraction. Non-2xx is failure
/// (stdlib §fetch · `transient` per status). Extraction modes beyond the
/// raw body are the `nika-extract` job (R2 · honest gap below).
pub(crate) async fn fetch<H: HttpGetDyn + HttpPostDyn>(http: &H, args: &Args) -> BuiltinOutcome {
    const C: &str = "NIKA-BUILTIN-FETCH-001";
    let url = req_str(args, "url", C)?;
    let method = opt_str(args, "method", C)?.unwrap_or("GET").to_uppercase();

    let request = build_request(&method, url, args).map_err(|m| BuiltinFailure::new(C, m))?;
    // MUTATION (equivalent under the mock): GET/HEAD route to .get(), all
    // else to .post() — but a test double serves both identically and the
    // recorded request carries its own method, so deleting this arm is
    // behaviorally invisible in tests. Real transports differ (GET has no
    // body); the per-method `request.method` mapping IS pinned below.
    let response = match method.as_str() {
        "GET" | "HEAD" => http.get(request).await,
        _ => http.post(request).await,
    }
    .map_err(|e| BuiltinFailure::new(C, format!("request failed: {e}")))?;

    if !(200..300).contains(&response.status) {
        return Err(BuiltinFailure {
            code: C,
            message: format!("HTTP {} from {url}", response.status),
        });
    }
    // R1 scope: the raw body as a string. The `mode:` extraction surface
    // (markdown · article · jq · …) lands with `nika-extract` (the spec's
    // extract-modes-v0.1.md) — advertised honestly in the tool def, not
    // faked here.
    let body = String::from_utf8_lossy(&response.body).into_owned();
    Ok(serde_json::Value::String(body))
}

fn build_request(method: &str, url: &str, args: &Args) -> Result<HttpRequest, String> {
    let mut request = match method {
        "GET" | "HEAD" => HttpRequest::get(url),
        "POST" | "PUT" | "DELETE" | "PATCH" => HttpRequest::post(url),
        other => return Err(format!("unsupported method `{other}`")),
    };
    request.method = parse_method(method);
    if let Some(headers) = args.get("headers").and_then(serde_json::Value::as_object) {
        for (key, value) in headers {
            if let Some(text) = value.as_str() {
                request.headers.insert(key.clone(), text.to_owned());
            }
        }
    }
    if let Some(body) = args.get("body") {
        let bytes = match body {
            serde_json::Value::String(s) => s.clone().into_bytes(),
            other => serde_json::to_vec(other).map_err(|e| e.to_string())?,
        };
        request.body = Some(bytes.into());
    }
    Ok(request)
}

fn parse_method(method: &str) -> HttpMethod {
    match method {
        "POST" => HttpMethod::Post,
        "PUT" => HttpMethod::Put,
        "DELETE" => HttpMethod::Delete,
        "PATCH" => HttpMethod::Patch,
        "HEAD" => HttpMethod::Head,
        _ => HttpMethod::Get,
    }
}

/// `nika:notify` — send an alert. `webhook` MUST work (POST the message);
/// other channels are feature-gated → `NIKA-BUILTIN-NOTIFY-001` when
/// unconfigured (stdlib §notify).
pub(crate) async fn notify<H: HttpPostDyn>(http: &H, args: &Args) -> BuiltinOutcome {
    const C1: &str = "NIKA-BUILTIN-NOTIFY-001";
    const C2: &str = "NIKA-BUILTIN-NOTIFY-002";
    let channel = opt_str(args, "channel", C1)?.unwrap_or("webhook");
    if channel != "webhook" {
        return Err(BuiltinFailure::new(
            C1,
            format!("channel `{channel}` is not configured (v0.1 engines MUST support `webhook`)"),
        ));
    }
    let target = req_str(args, "target", C1)?;
    let message = req_str(args, "message", C1)?;
    let severity = opt_str(args, "severity", C1)?.unwrap_or("info");

    let mut request = HttpRequest::post(target);
    request
        .headers
        .insert("content-type".to_owned(), "application/json".to_owned());
    let payload = serde_json::json!({ "message": message, "severity": severity });
    request.body = Some(serde_json::to_vec(&payload).unwrap_or_default().into());

    let response = http
        .post(request)
        .await
        .map_err(|e| BuiltinFailure::new(C2, format!("delivery failed: {e}")))?;
    if (200..300).contains(&response.status) {
        Ok(serde_json::Value::Null)
    } else {
        Err(BuiltinFailure {
            code: C2,
            message: format!("webhook returned HTTP {}", response.status),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_kernel_mock::MockHttp;

    fn args(v: serde_json::Value) -> Args {
        match v {
            serde_json::Value::Object(map) => map,
            other => panic!("test arg must be an object, got {other}"),
        }
    }

    #[tokio::test]
    async fn fetch_returns_body_on_2xx_fails_on_4xx() {
        let http = MockHttp::new().enqueue_ok(200, "hello world".as_bytes().to_vec());
        let out = fetch(&http, &args(serde_json::json!({ "url": "https://x.test" })))
            .await
            .expect("ok");
        assert_eq!(out, serde_json::Value::String("hello world".to_owned()));

        let http = MockHttp::new().enqueue_ok(404, Vec::new());
        let fail = fetch(&http, &args(serde_json::json!({ "url": "https://x.test" }))).await;
        assert!(
            matches!(fail, Err(f) if f.code == "NIKA-BUILTIN-FETCH-001" && f.message.contains("404"))
        );
    }

    #[tokio::test]
    async fn fetch_post_carries_the_body() {
        let http = MockHttp::new().enqueue_ok(200, b"ok".to_vec());
        fetch(
            &http,
            &args(serde_json::json!({
                "url": "https://x.test", "method": "POST", "body": {"a": 1}
            })),
        )
        .await
        .expect("ok");
        let sent = http.sent_requests();
        assert_eq!(sent.len(), 1);
        assert!(matches!(sent[0].method, HttpMethod::Post));
        assert!(sent[0].body.is_some());
    }

    #[tokio::test]
    async fn fetch_maps_every_method_to_the_request() {
        for (name, expected) in [
            ("GET", HttpMethod::Get),
            ("POST", HttpMethod::Post),
            ("PUT", HttpMethod::Put),
            ("DELETE", HttpMethod::Delete),
            ("PATCH", HttpMethod::Patch),
            ("HEAD", HttpMethod::Head),
        ] {
            let http = MockHttp::new().enqueue_ok(200, b"ok".to_vec());
            fetch(
                &http,
                &args(serde_json::json!({ "url": "https://x.test", "method": name })),
            )
            .await
            .expect("ok");
            let sent = http.sent_requests();
            assert_eq!(sent.len(), 1, "{name} sent one request");
            assert_eq!(
                std::mem::discriminant(&sent[0].method),
                std::mem::discriminant(&expected),
                "{name} maps to its HttpMethod"
            );
        }
        // An unsupported method is a build-request failure.
        let http = MockHttp::new();
        let bad = fetch(
            &http,
            &args(serde_json::json!({ "url": "https://x.test", "method": "TRACE" })),
        )
        .await;
        assert!(matches!(bad, Err(f) if f.code == "NIKA-BUILTIN-FETCH-001"));
        assert!(
            http.sent_requests().is_empty(),
            "TRACE never reached the wire"
        );
    }

    #[tokio::test]
    async fn notify_webhook_only_at_v0_1() {
        let http = MockHttp::new().enqueue_ok(200, Vec::new());
        let out = notify(
            &http,
            &args(serde_json::json!({ "target": "https://hooks.x", "message": "done" })),
        )
        .await
        .expect("ok");
        assert_eq!(out, serde_json::Value::Null);

        let http = MockHttp::new();
        let unconfigured = notify(
            &http,
            &args(serde_json::json!({ "channel": "slack", "target": "x", "message": "y" })),
        )
        .await;
        assert!(matches!(unconfigured, Err(f) if f.code == "NIKA-BUILTIN-NOTIFY-001"));
    }
}
