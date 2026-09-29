// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;
use nika_kernel_mock::{MockFs, MockHttp};

fn args(v: serde_json::Value) -> Args {
    match v {
        serde_json::Value::Object(map) => map,
        other => panic!("test arg must be an object, got {other}"),
    }
}

/// Test adapter — shadows `super::fetch` with the no-fs/no-boundary
/// shape most cases need (multipart file-part tests call
/// `super::fetch` with a real `MockFs` + declared boundary).
async fn fetch(http: &MockHttp, args: &Args) -> BuiltinOutcome {
    super::fetch(http, &MockFs::new(), &FsBoundary::unbounded(), args).await
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
async fn non_2xx_failure_message_never_echoes_userinfo() {
    let http = MockHttp::new().enqueue_ok(404, Vec::new());
    let fail = fetch(
        &http,
        &args(serde_json::json!({ "url": "https://user:hunter2@x.test/a" })),
    )
    .await;
    let Err(f) = fail else {
        panic!("404 must fail")
    };
    assert!(
        !f.message.contains("hunter2"),
        "credential echoed: {}",
        f.message
    );
    assert!(f.message.contains("https://x.test/a"), "{}", f.message);
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
async fn fetch_runtime_mirrors_the_pairing_rules() {
    // A templated mode bypasses the STATIC checker — the runtime
    // must reject the same pairings loud, never silently drop args.
    let http = MockHttp::new(); // nothing enqueued — must fail pre-network
    let err = fetch(
        &http,
        &args(serde_json::json!({
            "url": "https://x.test", "mode": "text", "selector": "div"
        })),
    )
    .await
    .expect_err("selector with non-selector mode");
    assert!(err.message.contains("mode: selector"), "{}", err.message);

    let err = fetch(
        &http,
        &args(serde_json::json!({
            "url": "https://x.test", "jq": ".x"
        })),
    )
    .await
    .expect_err("jq with the default markdown mode");
    assert!(err.message.contains("mode: jq"), "{}", err.message);
    assert!(http.sent_requests().is_empty(), "no request spent");
}

#[tokio::test]
async fn fetch_metadata_merges_link_header_alternates() {
    let html = br#"<html lang="en"><head><title>T</title></head><body></body></html>"#.to_vec();
    let http = MockHttp::new().enqueue_ok_with_headers(
        200,
        [(
            "Link",
            r#"<https://x.test/fr/>; rel="alternate"; hreflang="fr", <https://x.test/de/>; rel="alternate"; hreflang="de""#,
        )],
        html,
    );
    let out = fetch(
        &http,
        &args(serde_json::json!({ "url": "https://x.test", "mode": "metadata" })),
    )
    .await
    .expect("ok");
    assert_eq!(out["alternates"][0]["lang"], "fr");
    assert_eq!(out["alternates"][1]["href"], "https://x.test/de/");
    assert_eq!(out["title"], "T", "the HTML head still mined");
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
async fn fetch_bom_overrides_header_charset() {
    // WHATWG: the BOM is more authoritative than the header. A
    // UTF-16LE body with a misleading `charset=utf-8` header must
    // decode via the BOM, not mojibake as UTF-8 (the audit's P3).
    // "Hi" in UTF-16LE with BOM: FF FE 48 00 69 00.
    let body = vec![0xff, 0xfe, b'H', 0x00, b'i', 0x00];
    let http = MockHttp::new().enqueue_ok_with_headers(
        200,
        [("Content-Type", "text/html; charset=utf-8")],
        body,
    );
    let out = fetch(
        &http,
        &args(serde_json::json!({ "url": "https://x.test", "mode": "text" })),
    )
    .await
    .expect("ok");
    assert_eq!(
        out.as_str(),
        Some("Hi"),
        "UTF-16LE BOM beat the lying header"
    );
}

#[tokio::test]
async fn fetch_meta_charset_prescan_when_header_absent() {
    // No charset in the header → the <meta charset> prescan recovers
    // it (the legacy-page gap). 'é' = 0xE9 in ISO-8859-1.
    let mut body = br#"<html><head><meta charset="iso-8859-1"></head><body><p>Caf"#.to_vec();
    body.push(0xe9);
    body.extend_from_slice(b"</p></body></html>");
    let http = MockHttp::new().enqueue_ok_with_headers(
        200,
        [("Content-Type", "text/html")], // NO charset param
        body,
    );
    let out = fetch(
        &http,
        &args(serde_json::json!({ "url": "https://x.test", "mode": "text" })),
    )
    .await
    .expect("ok");
    assert!(
        out.as_str().is_some_and(|s| s.contains("Café")),
        "meta-charset prescan recovered ISO-8859-1: {out:?}"
    );
}

#[test]
fn meta_charset_ignores_a_meta_inside_a_comment() {
    // A `<meta charset>` living in a comment is NOT a declaration (the
    // WHATWG prescan steps over comments) — the real one downstream wins.
    assert_eq!(
        meta_charset(b"<!-- <meta charset=koi8-r> --><meta charset=shift_jis>"),
        Some(encoding_rs::SHIFT_JIS),
        "the commented <meta> must be skipped; the real one wins"
    );
    // Comment-only → no declaration (decode falls back to UTF-8 upstream).
    assert_eq!(
        meta_charset(b"<!-- <meta charset=koi8-r> -->"),
        None,
        "a <meta> seen only inside a comment yields no charset"
    );
}

#[tokio::test]
async fn fetch_header_charset_beats_meta_prescan() {
    // Precedence: a Content-Type charset OUTRANKS a (conflicting)
    // <meta> declaration. Body is ISO-8859-1 (0xE9 = é); meta lies
    // "utf-8", header says "iso-8859-1" → header wins, é recovered.
    let mut body = br#"<html><head><meta charset="utf-8"></head><body><p>Caf"#.to_vec();
    body.push(0xe9);
    body.extend_from_slice(b"</p></body></html>");
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
    assert!(out.as_str().is_some_and(|s| s.contains("Café")), "{out:?}");
}

#[tokio::test]
async fn fetch_charset_matrix_windows1252_shiftjis_quoted() {
    // windows-1252: € = 0x80 (the byte ISO-8859-1 maps to a C1
    // control — the label distinction is real).
    let body = vec![b'<', b'p', b'>', 0x80, b'5', b'<', b'/', b'p', b'>'];
    let http = MockHttp::new().enqueue_ok_with_headers(
        200,
        [("Content-Type", "text/html; charset=windows-1252")],
        body,
    );
    let out = fetch(
        &http,
        &args(serde_json::json!({ "url": "https://x.test", "mode": "text" })),
    )
    .await
    .expect("ok");
    assert_eq!(out.as_str(), Some("€5"), "windows-1252 euro sign");

    // Shift_JIS: 日本 = 93 FA 96 7B.
    let body = vec![
        b'<', b'p', b'>', 0x93, 0xfa, 0x96, 0x7b, b'<', b'/', b'p', b'>',
    ];
    let http = MockHttp::new().enqueue_ok_with_headers(
        200,
        [("Content-Type", "text/html; charset=Shift_JIS")],
        body,
    );
    let out = fetch(
        &http,
        &args(serde_json::json!({ "url": "https://x.test", "mode": "text" })),
    )
    .await
    .expect("ok");
    assert_eq!(out.as_str(), Some("日本"), "Shift_JIS kanji");

    // Quote-aware param split: a `;` inside a QUOTED earlier param
    // must not hide the real charset (review lens 2 · P3-3).
    let body = vec![
        b'<', b'p', b'>', b'C', b'a', b'f', 0xe9, b'<', b'/', b'p', b'>',
    ];
    let http = MockHttp::new().enqueue_ok_with_headers(
        200,
        [(
            "Content-Type",
            r#"text/html; title="a;charset=koi8-r"; charset=iso-8859-1"#,
        )],
        body,
    );
    let out = fetch(
        &http,
        &args(serde_json::json!({ "url": "https://x.test", "mode": "text" })),
    )
    .await
    .expect("ok");
    assert_eq!(
        out.as_str(),
        Some("Café"),
        "quoted ; did not derail the scan"
    );
}

#[tokio::test]
async fn fetch_huge_body_extracts_without_distortion() {
    // ~600 KB of repeated paragraphs: the blocking-pool handoff +
    // markdown pipeline must hold shape (no truncation · no panic).
    let para = "<p>Sixty kilobyte stress paragraph with stable words.</p>";
    let html = format!("<html><body>{}</body></html>", para.repeat(10_000));
    let http = MockHttp::new().enqueue_ok(200, html.into_bytes());
    let out = fetch(&http, &args(serde_json::json!({ "url": "https://x.test" })))
        .await
        .expect("ok");
    let md = out.as_str().expect("string");
    assert_eq!(
        md.matches("Sixty kilobyte stress paragraph").count(),
        10_000,
        "every paragraph survived"
    );
}

#[tokio::test]
async fn fetch_non_string_header_value_is_loud() {
    let http = MockHttp::new();
    let err = fetch(
        &http,
        &args(serde_json::json!({
            "url": "https://x.test", "headers": { "x-count": 1 }
        })),
    )
    .await
    .expect_err("non-string header");
    assert!(err.message.contains("x-count"), "{}", err.message);
    assert!(http.sent_requests().is_empty(), "failed before the wire");
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
async fn fetch_transport_failures_are_transient() {
    // BUG-D: connection + timeout transport errors are the textbook
    // transient case (DNS/connection-refused/reset surface as
    // HttpError::Connection) — they must be retryable so `retry:` works.
    use nika_kernel::io::http::HttpError;
    for err in [
        HttpError::Timeout { duration_ms: 5000 },
        HttpError::Connection {
            reason: "dns resolution failed".to_owned(),
        },
    ] {
        let http = MockHttp::new().enqueue_err(err);
        let fail = fetch(&http, &args(serde_json::json!({ "url": "https://x.test" })))
            .await
            .expect_err("transport failure");
        assert_eq!(fail.code, "NIKA-BUILTIN-FETCH-001");
        assert!(fail.transient, "a transport failure is retryable");
    }
    // An SSRF/scheme rejection (a deterministic refusal) is NOT transient,
    // and speaks the security-plane NIKA-SEC-005 (not the generic
    // FETCH-001) so it derives `security_error` + never reaches an agent.
    let http = MockHttp::new().enqueue_err(HttpError::SsrfBlocked {
        url: "http://127.0.0.1".to_owned(),
    });
    let fail = fetch(
        &http,
        &args(serde_json::json!({ "url": "http://127.0.0.1" })),
    )
    .await
    .expect_err("ssrf blocked");
    assert!(!fail.transient, "an SSRF block is a deterministic refusal");
    assert_eq!(fail.code, "NIKA-SEC-005", "SSRF is the security-plane code");
}

#[tokio::test]
async fn keyless_mutating_fetch_is_never_transient() {
    // #1371 · the effect-safe retry law: a POST/PUT/DELETE/PATCH WITHOUT an
    // `idempotency-key` header types EVERY failure non-transient — the
    // failure may be ambiguous (the server may have committed before the
    // socket dropped or the 500 was emitted) and a blind replay doubles
    // the effect. One attempt; the static NIKA-SEC-016 refusal owns the
    // declared-`retry:` teaching.
    use nika_kernel::io::http::HttpError;
    for method in ["POST", "PUT", "DELETE", "PATCH", "post"] {
        // The issue's core evidence: a post-commit HTTP 500 used to be
        // transient → declared retry triple-charged. Now one attempt.
        let http = MockHttp::new().enqueue_ok(500, Vec::new());
        let fail = fetch(
            &http,
            &args(serde_json::json!({
                "url": "https://api.test/charge", "method": method, "body": { "a": 1 }
            })),
        )
        .await
        .expect_err("non-2xx fails");
        assert!(
            !fail.transient,
            "{method} keyless + 500 is never retry-eligible (the ambiguous commit)"
        );
        // The canonical ambiguous case: a transport failure AFTER the
        // request may have landed — same law, one attempt.
        let http = MockHttp::new().enqueue_err(HttpError::Connection {
            reason: "socket dropped mid-response".to_owned(),
        });
        let fail = fetch(
            &http,
            &args(serde_json::json!({
                "url": "https://api.test/charge", "method": method, "body": { "a": 1 }
            })),
        )
        .await
        .expect_err("transport failure");
        assert!(
            !fail.transient,
            "{method} keyless + transport drop is never retry-eligible"
        );
    }
}

#[tokio::test]
async fn keyed_mutating_fetch_keeps_the_spec_status_table() {
    // #1371 · with an `idempotency-key` header the receiver dedups the
    // replay — retry works: 5xx/408/429 stay transient, other 4xx do not.
    use nika_kernel::io::http::HttpError;
    for (status, expect) in [(500, true), (503, true), (429, true), (404, false)] {
        let http = MockHttp::new().enqueue_ok(status, Vec::new());
        let fail = fetch(
            &http,
            &args(serde_json::json!({
                "url": "https://api.test/charge",
                "method": "POST",
                // Any-case header name discharges the hazard (RFC 9110 §5.1).
                "headers": { "Idempotency-Key": "order-7" },
                "body": { "a": 1 }
            })),
        )
        .await
        .expect_err("non-2xx fails");
        assert_eq!(fail.transient, expect, "keyed POST · HTTP {status}");
    }
    // Keyed + transport: the receiver's dedup makes the replay safe.
    let http = MockHttp::new().enqueue_err(HttpError::Connection {
        reason: "socket dropped mid-response".to_owned(),
    });
    let fail = fetch(
        &http,
        &args(serde_json::json!({
            "url": "https://api.test/charge",
            "method": "POST",
            "headers": { "idempotency-key": "order-7" },
            "body": { "a": 1 }
        })),
    )
    .await
    .expect_err("transport failure");
    assert!(
        fail.transient,
        "keyed POST + transport drop stays retryable"
    );
}

#[tokio::test]
async fn read_only_methods_keep_the_spec_status_table() {
    // #1371 · GET/HEAD replay nothing — retry free, key or no key:
    // the status table is UNCHANGED for read-only methods.
    use nika_kernel::io::http::HttpError;
    for method in ["GET", "HEAD"] {
        let http = MockHttp::new().enqueue_ok(500, Vec::new());
        let fail = fetch(
            &http,
            &args(serde_json::json!({ "url": "https://api.test/x", "method": method })),
        )
        .await
        .expect_err("non-2xx fails");
        assert!(fail.transient, "{method} + 500 stays transient");
        let http = MockHttp::new().enqueue_err(HttpError::Connection {
            reason: "dns resolution failed".to_owned(),
        });
        let fail = fetch(
            &http,
            &args(serde_json::json!({ "url": "https://api.test/x", "method": method })),
        )
        .await
        .expect_err("transport failure");
        assert!(fail.transient, "{method} + transport stays transient");
    }
}

#[tokio::test]
async fn host_not_allowed_surfaces_as_nika_sec_004() {
    // A permits.net.http escape (the kernel HostNotAllowed) is the
    // spec-plane NIKA-SEC-004 capability denial — NOT a transport
    // failure, NEVER retryable, distinct from the SSRF floor's
    // NIKA-SEC-005. This is the user-facing half of the runtime boundary.
    let http = MockHttp::new().enqueue_err(HttpError::HostNotAllowed {
        host: "evil.com".to_owned(),
    });
    let fail = fetch(
        &http,
        &args(serde_json::json!({ "url": "https://evil.com" })),
    )
    .await
    .expect_err("host outside permits.net.http");
    assert_eq!(
        fail.code, "NIKA-SEC-004",
        "a declared-boundary escape is the security code"
    );
    assert!(!fail.transient, "a capability denial is never retryable");
    assert!(fail.message.contains("net.http"), "{}", fail.message);
    // The refused host IS the one written in the file: no hop to name.
    assert!(!fail.message.contains("redirect"), "{}", fail.message);

    // notify's webhook `target:` rides the very same boundary.
    let http = MockHttp::new().enqueue_err(HttpError::HostNotAllowed {
        host: "evil.com".to_owned(),
    });
    let fail = notify(
        &http,
        &args(serde_json::json!({ "target": "https://evil.com", "message": "x" })),
    )
    .await
    .expect_err("notify target outside permits.net.http");
    assert_eq!(fail.code, "NIKA-SEC-004", "notify honors the same boundary");
}

#[tokio::test]
async fn sec_004_on_a_redirect_hop_names_the_host_check_audited() {
    // #1582 · check green ≠ run: `nika check` audits the URL written in
    // the file; a 3xx hop onto another host is judged at run. The refusal
    // bridges the two doors: the audited host and the exact grant.
    let http = MockHttp::new().enqueue_err(HttpError::HostNotAllowed {
        host: "files-03.restcountries.com".to_owned(),
    });
    let fail = fetch(
        &http,
        &args(serde_json::json!({ "url": "https://restcountries.com/v3.1/name/france" })),
    )
    .await
    .expect_err("redirect hop outside permits.net.http");
    assert_eq!(fail.code, "NIKA-SEC-004", "a hop escape is the same code");
    assert!(!fail.transient, "never retryable");
    let msg = &fail.message;
    assert!(msg.contains("redirect from `restcountries.com`"), "{msg}");
    assert!(
        msg.contains("add `files-03.restcountries.com` to permits.net.http"),
        "{msg}"
    );

    // notify's webhook `target:` rides the same door.
    let http = MockHttp::new().enqueue_err(HttpError::HostNotAllowed {
        host: "hooks.example.net".to_owned(),
    });
    let fail = notify(
        &http,
        &args(serde_json::json!({ "target": "https://example.net/hook", "message": "x" })),
    )
    .await
    .expect_err("notify redirect hop outside permits.net.http");
    assert_eq!(fail.code, "NIKA-SEC-004");
    assert!(
        fail.message.contains("redirect from `example.net`"),
        "{}",
        fail.message
    );
}

#[tokio::test]
async fn sec_004_direct_refusal_is_never_called_a_redirect() {
    // Normalization parity with the transport (`nika-http`'s `host_of`):
    // case · FQDN dot · IPv6 brackets · `\@` userinfo — a spelling
    // difference is the SAME host the file names, never a hop.
    for (requested, refused) in [
        ("https://ALLOWED.com./x", "allowed.com"),
        ("http://[::1]:8080/x", "::1"),
        (r"https://evil.com\@allowed.com/x", "evil.com"),
    ] {
        let http = MockHttp::new().enqueue_err(HttpError::HostNotAllowed {
            host: refused.to_owned(),
        });
        let fail = fetch(&http, &args(serde_json::json!({ "url": requested })))
            .await
            .expect_err("host outside permits.net.http");
        assert_eq!(fail.code, "NIKA-SEC-004");
        assert!(
            !fail.message.contains("redirect"),
            "{requested}: {}",
            fail.message
        );
    }
}

#[test]
fn url_host_agrees_with_the_transport_on_the_pinned_vectors() {
    // The hop detector reads the file's host the way the transport does
    // — pinned by the shared vector table so check · run · this bridge
    // can never drift (a drift would call a spelling a redirect).
    for (raw, expected) in nika_types::net::HOST_EXTRACTION_VECTORS {
        let got = url::Url::parse(raw).ok().and_then(|u| url_host(&u));
        assert_eq!(got.as_deref(), *expected, "vector {raw:?}");
    }
}

#[tokio::test]
async fn notify_ssrf_blocked_surfaces_as_nika_sec_005() {
    // The SSRF floor on the webhook `target:` is the security-plane
    // NIKA-SEC-005 (non-transient · never agent-fed) — same as fetch.
    let http = MockHttp::new().enqueue_err(HttpError::SsrfBlocked {
        url: "http://169.254.169.254".to_owned(),
    });
    let fail = notify(
        &http,
        &args(serde_json::json!({ "target": "http://169.254.169.254", "message": "x" })),
    )
    .await
    .expect_err("notify target is an SSRF block");
    assert_eq!(fail.code, "NIKA-SEC-005", "SSRF is the security floor code");
    assert!(!fail.transient, "an SSRF block is a deterministic refusal");
}

#[tokio::test]
async fn fetch_failure_details_carry_the_status_code() {
    // `details.status_code` is normative (stdlib §fetch) — branching
    // on 403 vs 429 must never mean parsing the human message.
    let http = MockHttp::new().enqueue_ok(429, Vec::new());
    let fail = fetch(&http, &args(serde_json::json!({ "url": "https://x.test" })))
        .await
        .expect_err("non-2xx fails");
    assert_eq!(
        fail.details,
        Some(serde_json::json!({ "status_code": 429 })),
        "machine-readable status in details"
    );
    // Transport-plane failures carry no status (no response existed).
    let empty = MockHttp::new();
    let transport = fetch(
        &empty,
        &args(serde_json::json!({ "url": "https://x.test" })),
    )
    .await
    .expect_err("no canned response = transport error");
    assert_eq!(transport.details, None);
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

#[tokio::test]
async fn notify_data_rides_the_payload_and_is_absent_when_not_given() {
    // With data: the payload is { message, severity, data } —
    // receivers branch on machine fields, never parse the message.
    let http = MockHttp::new().enqueue_ok(200, Vec::new());
    notify(
        &http,
        &args(serde_json::json!({
            "target": "https://hooks.x", "message": "done",
            "data": { "run": "r-42", "count": 7 }
        })),
    )
    .await
    .expect("ok");
    let sent = http.sent_requests();
    let body: serde_json::Value =
        serde_json::from_slice(sent[0].body.as_ref().expect("body")).expect("json");
    assert_eq!(
        body,
        serde_json::json!({
            "message": "done", "severity": "info",
            "data": { "run": "r-42", "count": 7 }
        })
    );

    // Without data: the key is ABSENT (not null — spec §notify).
    let http = MockHttp::new().enqueue_ok(200, Vec::new());
    notify(
        &http,
        &args(serde_json::json!({ "target": "https://hooks.x", "message": "m" })),
    )
    .await
    .expect("ok");
    let sent = http.sent_requests();
    let body: serde_json::Value =
        serde_json::from_slice(sent[0].body.as_ref().expect("body")).expect("json");
    assert_eq!(
        body,
        serde_json::json!({ "message": "m", "severity": "info" })
    );
    assert!(body.get("data").is_none(), "absent, never null");
}
