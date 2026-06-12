// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Network builtins (2) — fetch · notify (stdlib §Network).
//!
//! Both compose the injected kernel http seam — SSRF defense lives in the
//! L1 http effect (3-layer · s5), this layer never re-implements it.
//!
//! `nika:fetch` is web-CONTENT acquisition: the kernel http GET/POST,
//! then the `mode:` extraction (`nika-extract` · 8 modes) — `mode: jq`
//! composes THIS crate's one jq engine (`data::jq`), never a second one.

use bytes::Bytes;
use nika_extract::{ExtractMode, ExtractOptions};
use nika_kernel::io::http::{HttpError, HttpGetDyn, HttpMethod, HttpPostDyn, HttpRequest};

use crate::{Args, BuiltinFailure, BuiltinOutcome, opt_str, req_str};

/// `nika:fetch` — HTTP request + content extraction (stdlib §fetch).
/// Non-2xx is failure (`transient: true` for 5xx/408/429, `false` for
/// other 4xx — normative). The body is decoded then run through the
/// `mode:` extraction (default `markdown` · extract-modes-v0.1.md).
pub(crate) async fn fetch<H: HttpGetDyn + HttpPostDyn>(http: &H, args: &Args) -> BuiltinOutcome {
    const C: &str = "NIKA-BUILTIN-FETCH-001";
    let url = req_str(args, "url", C)?;
    let method = opt_str(args, "method", C)?.unwrap_or("GET").to_uppercase();
    // Vet the mode BEFORE the network call — a bad `mode:` should fail
    // without spending a request (the static checker catches it earlier;
    // this is the runtime defense for a hand-built ToolCall).
    let mode = parse_mode(args, C)?;

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
    .map_err(|e| {
        // Transport-plane retryability: timeouts + connection failures are
        // the canonical transient conditions; SSRF/size/scheme rejections
        // are deterministic.
        let transient = matches!(e, HttpError::Timeout { .. } | HttpError::Connection { .. });
        BuiltinFailure::new(C, format!("request failed: {e}")).with_transient(transient)
    })?;

    if !(200..300).contains(&response.status) {
        return Err(
            BuiltinFailure::new(C, format!("HTTP {} from {url}", response.status))
                .with_transient(is_transient_status(response.status)),
        );
    }

    // Gather the extraction inputs (owned) BEFORE handing off, so the
    // CPU-heavy parse runs on the blocking pool — a 64 MiB HTML parse or
    // a heavy jq must not starve the async executor (the data::jq /
    // nika-ocr precedent). `Bytes` is Arc-backed: the clone is cheap.
    let plan = ExtractPlan {
        mode,
        body: response.body.clone(),
        content_type: response.headers.get("content-type").cloned(),
        base_url: if response.final_url.is_empty() {
            url.to_owned()
        } else {
            response.final_url.clone()
        },
        selector: opt_str(args, "selector", C)?.map(str::to_owned),
        jq: if mode == ExtractMode::Jq {
            Some(req_str(args, "jq", C)?.to_owned())
        } else {
            None
        },
    };
    tokio::task::spawn_blocking(move || plan.run(C))
        .await
        .map_err(|e| BuiltinFailure::new(C, format!("extraction task failed: {e}")))?
}

/// Resolve the `mode:` argument against the closed extract-mode set
/// (default `markdown` · extract-modes-v0.1.md). A templated value is
/// already CEL-resolved by the time the builtin runs — a non-canon
/// string here is a genuine error.
fn parse_mode(args: &Args, code: &'static str) -> Result<ExtractMode, BuiltinFailure> {
    match opt_str(args, "mode", code)? {
        None => Ok(ExtractMode::Markdown),
        Some(raw) => raw
            .parse::<ExtractMode>()
            .map_err(|e| BuiltinFailure::new(code, e.to_string())),
    }
}

/// The owned extraction inputs — runs on the blocking pool.
struct ExtractPlan {
    mode: ExtractMode,
    body: Bytes,
    content_type: Option<String>,
    base_url: String,
    selector: Option<String>,
    jq: Option<String>,
}

impl ExtractPlan {
    /// Decode per `mode` and run the extraction. `raw`/`jq` demand strict
    /// UTF-8 (raw is the spec's UTF-8 contract; jq input is JSON = UTF-8
    /// by RFC 8259); the HTML/feed/sitemap modes decode charset-aware
    /// from `Content-Type` (the web is not all UTF-8).
    fn run(self, code: &'static str) -> BuiltinOutcome {
        match self.mode {
            ExtractMode::Raw => {
                Ok(serde_json::Value::String(decode_utf8_strict(&self.body, code)?))
            }
            ExtractMode::Jq => {
                // `jq:` presence is guaranteed by the caller (mode == Jq).
                let expression = self.jq.unwrap_or_default();
                let text = decode_utf8_strict(&self.body, code)?;
                let input: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
                    BuiltinFailure::new(code, format!("response is not JSON (mode: jq): {e}"))
                })?;
                // Compose THE jq engine (data::jq · one data language · the
                // exactly-one-output law + ceiling live there, not here).
                let mut jq_args = serde_json::Map::new();
                jq_args.insert("expression".to_owned(), serde_json::Value::String(expression));
                jq_args.insert("input".to_owned(), input);
                crate::data::jq(&jq_args)
            }
            other => {
                let mut opts = ExtractOptions::new();
                opts.base_url = Some(&self.base_url);
                opts.selector = self.selector.as_deref();
                let text = decode_charset(&self.body, self.content_type.as_deref());
                // Extraction is deterministic (parse failures don't get
                // better on retry).
                nika_extract::extract(&text, other, &opts)
                    .map_err(|e| BuiltinFailure::new(code, e.to_string()))
            }
        }
    }
}

/// Strict UTF-8 decode (`raw`/`jq`): a non-UTF-8 body is
/// `NIKA-BUILTIN-FETCH-001` (stdlib §fetch raw contract · binary is
/// file-mediated, not fetch's job).
fn decode_utf8_strict(body: &[u8], code: &'static str) -> Result<String, BuiltinFailure> {
    std::str::from_utf8(body).map(str::to_owned).map_err(|e| {
        BuiltinFailure::new(
            code,
            format!(
                "response body is not valid UTF-8 ({e}) — `mode: raw`/`jq` need text; \
                 binary payloads are not a fetch concern"
            ),
        )
    })
}

/// Charset-aware decode for the extraction modes: honor the
/// `Content-Type` charset (default UTF-8), lossily replacing
/// undecodable bytes — extraction is best-effort cleanup, a stray byte
/// must not sink the whole page (the strict path is `raw`/`jq` above).
fn decode_charset(body: &[u8], content_type: Option<&str>) -> String {
    let encoding = content_type
        .and_then(charset_label)
        .and_then(|label| encoding_rs::Encoding::for_label(label.as_bytes()))
        .unwrap_or(encoding_rs::UTF_8);
    encoding.decode(body).0.into_owned()
}

/// Pull the `charset=` parameter out of a `Content-Type` (case-
/// insensitive key · quotes trimmed). `None` when absent → caller
/// defaults to UTF-8.
fn charset_label(content_type: &str) -> Option<&str> {
    content_type.split(';').skip(1).find_map(|param| {
        let (key, value) = param.split_once('=')?;
        key.trim()
            .eq_ignore_ascii_case("charset")
            .then(|| value.trim().trim_matches('"'))
    })
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

/// The spec's status→retryability table (stdlib §fetch · normative):
/// 5xx, 408 (request timeout) and 429 (rate limit) are transient.
fn is_transient_status(status: u16) -> bool {
    matches!(status, 500..=599 | 408 | 429)
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
    let body = serde_json::to_vec(&payload)
        .map_err(|e| BuiltinFailure::new(C1, format!("payload serialization failed: {e}")))?;
    request.body = Some(body.into());

    let response = http
        .post(request)
        .await
        .map_err(|e| BuiltinFailure::new(C2, format!("delivery failed: {e}")))?;
    if (200..300).contains(&response.status) {
        Ok(serde_json::Value::Null)
    } else {
        Err(
            BuiltinFailure::new(C2, format!("webhook returned HTTP {}", response.status))
                .with_transient(is_transient_status(response.status)),
        )
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
    async fn fetch_raw_returns_body_verbatim_fails_on_4xx() {
        // mode: raw is the transport-passthrough (no extraction).
        let http = MockHttp::new().enqueue_ok(200, "hello world".as_bytes().to_vec());
        let out = fetch(
            &http,
            &args(serde_json::json!({ "url": "https://x.test", "mode": "raw" })),
        )
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
    async fn fetch_default_mode_is_markdown_extraction() {
        // No mode: → markdown (the spec default · extract-modes-v0.1.md).
        let html = b"<html><body><h1>Title</h1><p>Body text.</p>\
                     <script>evil()</script></body></html>"
            .to_vec();
        let http = MockHttp::new().enqueue_ok(200, html);
        let out = fetch(&http, &args(serde_json::json!({ "url": "https://x.test" })))
            .await
            .expect("ok");
        let md = out.as_str().expect("string");
        assert!(md.contains("# Title"), "heading→markdown: {md}");
        assert!(md.contains("Body text."), "prose survives");
        assert!(!md.contains("evil()"), "script stripped: {md}");
    }

    #[tokio::test]
    async fn fetch_mode_jq_composes_the_one_jq_engine() {
        let json = br#"{"items":[{"name":"a"},{"name":"b"}]}"#.to_vec();
        let http = MockHttp::new().enqueue_ok(200, json);
        let out = fetch(
            &http,
            &args(serde_json::json!({
                "url": "https://api.test", "mode": "jq", "jq": "[.items[].name]"
            })),
        )
        .await
        .expect("ok");
        assert_eq!(out, serde_json::json!(["a", "b"]));

        // The one-output law is the jq engine's (a bare stream is rejected
        // with the [ … ]-collect advice · NOT re-implemented here).
        let json = br#"{"items":[{"name":"a"},{"name":"b"}]}"#.to_vec();
        let http = MockHttp::new().enqueue_ok(200, json);
        let stream = fetch(
            &http,
            &args(serde_json::json!({
                "url": "https://api.test", "mode": "jq", "jq": ".items[].name"
            })),
        )
        .await
        .expect_err("stream is a single-output violation");
        assert!(stream.message.contains("[ … ]"), "{}", stream.message);
    }

    #[tokio::test]
    async fn fetch_mode_jq_on_non_json_is_a_clear_error() {
        let http = MockHttp::new().enqueue_ok(200, b"<html>not json</html>".to_vec());
        let err = fetch(
            &http,
            &args(serde_json::json!({
                "url": "https://x.test", "mode": "jq", "jq": "."
            })),
        )
        .await
        .expect_err("html is not json");
        assert!(err.code == "NIKA-BUILTIN-FETCH-001" && err.message.contains("not JSON"));
    }

    #[tokio::test]
    async fn fetch_mode_selector_extracts_matches() {
        let html = b"<div class=\"x\"><p>one</p></div><div class=\"x\"><p>two</p></div>".to_vec();
        let http = MockHttp::new().enqueue_ok(200, html);
        let out = fetch(
            &http,
            &args(serde_json::json!({
                "url": "https://x.test", "mode": "selector", "selector": "div.x"
            })),
        )
        .await
        .expect("ok");
        let s = out.as_str().expect("string");
        assert!(s.contains("<p>one</p>") && s.contains("<p>two</p>"), "{s}");
    }

    #[tokio::test]
    async fn fetch_unknown_mode_fails_before_the_network() {
        let http = MockHttp::new(); // no response enqueued
        let err = fetch(
            &http,
            &args(serde_json::json!({ "url": "https://x.test", "mode": "html" })),
        )
        .await
        .expect_err("html is not a mode");
        assert_eq!(err.code, "NIKA-BUILTIN-FETCH-001");
        assert!(err.message.contains("closed"), "{}", err.message);
        assert!(http.sent_requests().is_empty(), "no request was spent");
    }

    #[tokio::test]
    async fn fetch_raw_rejects_non_utf8_body() {
        // 0xFF is never valid UTF-8 — raw is the spec's text contract.
        let http = MockHttp::new().enqueue_ok(200, vec![0xff, 0xfe, 0x00]);
        let err = fetch(
            &http,
            &args(serde_json::json!({ "url": "https://x.test", "mode": "raw" })),
        )
        .await
        .expect_err("non-utf8 raw");
        assert!(err.message.contains("not valid UTF-8"), "{}", err.message);
    }

    #[tokio::test]
    async fn fetch_decodes_declared_charset_for_extraction() {
        // "Café" in ISO-8859-1: 'é' = 0xE9. UTF-8-lossy would corrupt it;
        // charset-aware decode recovers it.
        let body = vec![
            b'<', b'p', b'>', b'C', b'a', b'f', 0xe9, b'<', b'/', b'p', b'>',
        ];
        let http = MockHttp::new().enqueue_ok_with_headers(
            200,
            [("Content-Type", "text/html; charset=iso-8859-1")],
            body,
        );
        let out = fetch(
            &http,
            &args(serde_json::json!({ "url": "https://x.test", "mode": "text" })),
        )
        .await
        .expect("ok");
        assert_eq!(out.as_str(), Some("Café"), "ISO-8859-1 é recovered");
    }

    #[tokio::test]
    async fn fetch_transient_follows_the_spec_status_table() {
        // 5xx · 408 · 429 are transient; other 4xx are not (stdlib §fetch).
        for (status, expect) in [
            (503, true),
            (500, true),
            (408, true),
            (429, true),
            (404, false),
        ] {
            let http = MockHttp::new().enqueue_ok(status, Vec::new());
            let fail = fetch(&http, &args(serde_json::json!({ "url": "https://x.test" })))
                .await
                .expect_err("non-2xx fails");
            assert_eq!(fail.transient, expect, "HTTP {status}");
        }
        // The boundary neighbours of the 5xx range stay non-transient.
        assert!(!is_transient_status(499));
        assert!(is_transient_status(599));
        assert!(!is_transient_status(600));
        assert!(!is_transient_status(200));
    }

    #[tokio::test]
    async fn fetch_head_succeeds_with_an_empty_body() {
        // HEAD carries no body per HTTP — a 200 HEAD is an empty-string
        // success, not an error.
        let http = MockHttp::new().enqueue_ok(200, Vec::new());
        let out = fetch(
            &http,
            &args(serde_json::json!({ "url": "https://x.test", "method": "HEAD" })),
        )
        .await
        .expect("ok");
        assert_eq!(out, serde_json::Value::String(String::new()));
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

        // A 5xx webhook delivery is transient (same status table as fetch).
        let http = MockHttp::new().enqueue_ok(503, Vec::new());
        let retry = notify(
            &http,
            &args(serde_json::json!({ "target": "https://hooks.x", "message": "m" })),
        )
        .await
        .expect_err("5xx fails");
        assert!(retry.code == "NIKA-BUILTIN-NOTIFY-002" && retry.transient);
    }
}
