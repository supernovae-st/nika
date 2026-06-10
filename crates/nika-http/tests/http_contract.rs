// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// `net_guard()` deliberately holds a std Mutex across the test's awaits:
// that IS the serialization (one network-fixture section runs at a time ·
// guards port-space, not data · each test acquires once, no re-entrancy ·
// per-test current-thread runtimes → no executor starvation).
#![allow(clippy::await_holding_lock)]

//! Contract tests for `nika-http` — the production `ReqwestHttp`.
//!
//! SSRF enforcement is exercised end-to-end with ZERO external network:
//! blocked targets fail BEFORE any connection. The transport mechanics
//! (redirect loop · size caps · streaming · timeout mapping) run against
//! handcrafted local HTTP/1.1 servers with `SsrfMode::Disabled`
//! (loopback is — correctly — blocked under Enforce).

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_core::Stream;
use nika_http::{HttpConfig, ReqwestHttp, SsrfMode};
use nika_kernel::http::{HttpGetDyn, HttpPostDyn};
use nika_kernel::{HttpClient, HttpError, HttpRequest};
use std::pin::pin;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

// ─── network-fixture serialization ───────────────────────────────────
//
// Every fixture test binds `127.0.0.1:0`. Under parallel execution the
// OS recycles ports across per-test tokio runtimes faster than the
// detached accept loops wind down, so a client can connect to a port a
// DIFFERENT test's fixture just grabbed (cross-test bleed · observed
// 2026-06-10 as spurious TooLarge / missing-TooLarge). A process-wide
// lock serializes the fixture-bound tests; the SSRF-blocked and
// type-level tests need no fixture and stay fully parallel.
static NET_LOCK: Mutex<()> = Mutex::new(());

/// Acquire the network-fixture lock for the duration of a test. Poisoning
/// is irrelevant (the guard protects port-space, not data), so recover.
fn net_guard() -> std::sync::MutexGuard<'static, ()> {
    NET_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

// ─── local fixture server ────────────────────────────────────────────

/// Serve canned HTTP/1.1 responses on a loopback socket. Each accepted
/// connection drains the request head then writes the next response in
/// the list (the last response repeats forever — redirect-loop tests
/// rely on that).
async fn serve(responses: Vec<String>) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let mut i = 0usize;
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                break;
            };
            let resp = responses[i.min(responses.len() - 1)].clone();
            i += 1;
            tokio::spawn(async move {
                drain_head(&mut sock).await;
                let _ = sock.write_all(resp.as_bytes()).await;
                let _ = sock.shutdown().await;
            });
        }
    });
    addr
}

/// Read until the end of the HTTP request head (\r\n\r\n) or EOF.
/// TCP gives no single-read guarantee, so loop instead of one `read`.
async fn read_head(sock: &mut tokio::net::TcpStream) -> String {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 256];
    loop {
        match sock.read(&mut tmp).await {
            Ok(n) if n > 0 => {
                buf.extend_from_slice(&tmp[..n]);
                if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            // EOF (Ok(0)) or read error — stop draining either way.
            _ => break,
        }
    }
    String::from_utf8_lossy(&buf).to_string()
}

async fn drain_head(sock: &mut tokio::net::TcpStream) {
    let _ = read_head(sock).await;
}

fn ok_response(body: &str, extra_headers: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n{}Connection: close\r\n\r\n{}",
        body.len(),
        extra_headers,
        body
    )
}

fn redirect_response(location: &str) -> String {
    format!(
        "HTTP/1.1 301 Moved Permanently\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )
}

fn mechanics_client() -> ReqwestHttp {
    let mut config = HttpConfig::new();
    config.ssrf = SsrfMode::Disabled;
    ReqwestHttp::with_config(config).expect("client builds")
}

fn mechanics_client_with(f: impl FnOnce(&mut HttpConfig)) -> ReqwestHttp {
    let mut config = HttpConfig::new();
    config.ssrf = SsrfMode::Disabled;
    f(&mut config);
    ReqwestHttp::with_config(config).expect("client builds")
}

// ─── type-level guarantees ───────────────────────────────────────────

#[test]
fn reqwesthttp_satisfies_blanket_client_and_dyn_variants() {
    fn accepts_client<T: HttpClient>(_: &T) {}
    fn accepts_dyn<T: HttpGetDyn + HttpPostDyn>(_: &T) {}
    let client = ReqwestHttp::new().expect("client builds");
    accepts_client(&client);
    accepts_dyn(&client);
}

#[test]
fn config_defaults_are_safe() {
    let config = HttpConfig::new();
    assert_eq!(config.ssrf, SsrfMode::Enforce, "SSRF is on by default");
    assert_eq!(config.timeout, Duration::from_secs(30));
    assert_eq!(config.max_redirects, 5);
    assert_eq!(config.max_response_bytes, 64 * 1024 * 1024);
}

// ─── SSRF enforcement (end-to-end · zero network) ────────────────────

#[tokio::test]
async fn enforce_blocks_loopback_literal() {
    let client = ReqwestHttp::new().expect("client builds");
    let err = client
        .get(HttpRequest::get("http://127.0.0.1:1/x"))
        .await
        .unwrap_err();
    assert!(matches!(err, HttpError::SsrfBlocked { .. }), "got {err:?}");
}

#[tokio::test]
async fn enforce_blocks_private_metadata_and_schemes() {
    let client = ReqwestHttp::new().expect("client builds");
    for url in [
        "http://10.0.0.8/x",
        "http://192.168.1.1/x",
        "http://169.254.169.254/latest/meta-data/",
        "http://metadata.google.internal/computeMetadata/v1/",
        "http://[::1]/x",
        "http://[::ffff:127.0.0.1]/x",
        "file:///etc/passwd",
    ] {
        let err = client.get(HttpRequest::get(url)).await.unwrap_err();
        assert!(
            matches!(err, HttpError::SsrfBlocked { .. }),
            "{url} must be SsrfBlocked, got {err:?}"
        );
    }
}

#[tokio::test]
async fn enforce_blocks_decimal_ip_via_resolve() {
    // http://2130706433/ is 127.0.0.1 in decimal — invisible to string
    // checks, caught by the DNS-resolve layer (resolves locally).
    let client = ReqwestHttp::new().expect("client builds");
    let err = client
        .get(HttpRequest::get("http://2130706433/x"))
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            HttpError::SsrfBlocked { .. } | HttpError::Connection { .. }
        ),
        "decimal-IP must not reach a private target, got {err:?}"
    );
}

#[tokio::test]
async fn enforce_blocks_trailing_dot_localhost() {
    // "localhost." (absolute FQDN) — blocked in the STATIC layer since
    // the trailing-dot normalization (review swarm P2).
    let client = ReqwestHttp::new().expect("client builds");
    let err = client
        .get(HttpRequest::get("http://localhost./x"))
        .await
        .unwrap_err();
    assert!(matches!(err, HttpError::SsrfBlocked { .. }), "got {err:?}");
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn enforce_blocks_hosts_file_alias_via_resolve() {
    // "broadcasthost" ships in the macOS /etc/hosts resolving to
    // 255.255.255.255 — not in the static blocklist, not a literal:
    // ONLY the DNS-resolve layer blocks it (zero network · local
    // resolver). STRICT SsrfBlocked: with `resolve_guard` neutered this
    // would surface as Connection — the mutant dies here.
    let client = ReqwestHttp::new().expect("client builds");
    let err = client
        .get(HttpRequest::get("http://broadcasthost/x"))
        .await
        .unwrap_err();
    assert!(matches!(err, HttpError::SsrfBlocked { .. }), "got {err:?}");
}

#[tokio::test]
async fn disabled_mode_reaches_loopback() {
    let _net = net_guard();
    let addr = serve(vec![ok_response("hello", "")]).await;
    let client = mechanics_client();
    let resp = client
        .get(HttpRequest::get(format!("http://{addr}/x")))
        .await
        .expect("disabled mode may hit loopback");
    assert_eq!(resp.status, 200);
    assert_eq!(&resp.body[..], b"hello");
}

// ─── transport mechanics (local server · Disabled mode) ─────────────

#[tokio::test]
async fn get_returns_status_headers_body_and_final_url() {
    let _net = net_guard();
    let addr = serve(vec![ok_response("payload", "X-Test: marker\r\n")]).await;
    let url = format!("http://{addr}/path");
    let client = mechanics_client();

    let resp = client.get(HttpRequest::get(&url)).await.expect("get ok");
    assert_eq!(resp.status, 200);
    assert_eq!(&resp.body[..], b"payload");
    assert_eq!(resp.final_url, url);
    assert_eq!(
        resp.headers.get("x-test").map(String::as_str),
        Some("marker")
    );
}

#[tokio::test]
async fn redirect_followed_to_second_host() {
    let _net = net_guard();
    let target = serve(vec![ok_response("landed", "")]).await;
    let hop1 = serve(vec![redirect_response(&format!("http://{target}/final"))]).await;
    let client = mechanics_client();

    let resp = client
        .get(HttpRequest::get(format!("http://{hop1}/start")))
        .await
        .expect("redirect followed");
    assert_eq!(resp.status, 200);
    assert_eq!(&resp.body[..], b"landed");
    assert_eq!(resp.final_url, format!("http://{target}/final"));
}

#[tokio::test]
async fn relative_location_resolved_against_current_url() {
    let _net = net_guard();
    let addr = serve(vec![
        redirect_response("/next"),
        ok_response("relative-ok", ""),
    ])
    .await;
    let client = mechanics_client();

    let resp = client
        .get(HttpRequest::get(format!("http://{addr}/start")))
        .await
        .expect("relative redirect followed");
    assert_eq!(&resp.body[..], b"relative-ok");
    assert_eq!(resp.final_url, format!("http://{addr}/next"));
}

#[tokio::test]
async fn follow_redirects_false_returns_the_3xx() {
    let _net = net_guard();
    let addr = serve(vec![redirect_response("http://example.invalid/never")]).await;
    let client = mechanics_client();

    let mut req = HttpRequest::get(format!("http://{addr}/start"));
    req.follow_redirects = false;
    let resp = client.get(req).await.expect("3xx returned as-is");
    assert_eq!(resp.status, 301);
    assert_eq!(
        resp.headers.get("location").map(String::as_str),
        Some("http://example.invalid/never")
    );
}

#[tokio::test]
async fn redirect_loop_exceeds_max_redirects() {
    let _net = net_guard();
    // The fixture repeats its last response forever: a self-redirect.
    let addr_placeholder = serve(vec![redirect_response("/again")]).await;
    let client = mechanics_client_with(|c| c.max_redirects = 3);

    let err = client
        .get(HttpRequest::get(format!("http://{addr_placeholder}/loop")))
        .await
        .unwrap_err();
    assert!(
        matches!(err, HttpError::Other { ref reason } if reason.contains("redirect")),
        "got {err:?}"
    );
}

#[tokio::test]
async fn missing_location_on_redirect_is_an_error() {
    let _net = net_guard();
    let addr = serve(vec![
        "HTTP/1.1 301 Moved Permanently\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            .to_string(),
    ])
    .await;
    let client = mechanics_client();

    let err = client
        .get(HttpRequest::get(format!("http://{addr}/x")))
        .await
        .unwrap_err();
    assert!(matches!(err, HttpError::Other { .. }), "got {err:?}");
}

// ─── response size cap (HttpError::TooLarge) ─────────────────────────

#[tokio::test]
async fn content_length_above_cap_fails_fast() {
    let _net = net_guard();
    let addr = serve(vec![
        "HTTP/1.1 200 OK\r\nContent-Length: 4096\r\nConnection: close\r\n\r\n".to_string(),
    ])
    .await;
    let client = mechanics_client_with(|c| c.max_response_bytes = 1024);

    let err = client
        .get(HttpRequest::get(format!("http://{addr}/big")))
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            HttpError::TooLarge {
                size: 4096,
                max: 1024
            }
        ),
        "got {err:?}"
    );
}

#[tokio::test]
async fn body_above_cap_without_content_length_is_capped_mid_read() {
    let _net = net_guard();
    let big = "x".repeat(4096);
    // No Content-Length: reqwest reads until EOF.
    let raw = format!("HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{big}");
    let addr = serve(vec![raw]).await;
    let client = mechanics_client_with(|c| c.max_response_bytes = 1024);

    let err = client
        .get(HttpRequest::get(format!("http://{addr}/big")))
        .await
        .unwrap_err();
    assert!(
        matches!(err, HttpError::TooLarge { max: 1024, .. }),
        "got {err:?}"
    );
}

// ─── streaming ───────────────────────────────────────────────────────

#[tokio::test]
async fn send_streaming_yields_the_body_chunks() {
    let _net = net_guard();
    let addr = serve(vec![ok_response("stream-me", "")]).await;
    let client = mechanics_client();

    let resp = client
        .send_streaming(HttpRequest::get(format!("http://{addr}/s")))
        .await
        .expect("stream opens");
    assert_eq!(resp.status, 200);
    assert_eq!(resp.content_length, Some(9));

    let mut collected = Vec::new();
    let mut body = pin!(resp.body);
    loop {
        let next = std::future::poll_fn(|cx| body.as_mut().poll_next(cx)).await;
        match next {
            Some(Ok(chunk)) => collected.extend_from_slice(&chunk),
            Some(Err(e)) => panic!("stream error: {e:?}"),
            None => break,
        }
    }
    assert_eq!(&collected[..], b"stream-me");
}

#[tokio::test]
async fn streaming_above_cap_yields_too_large_mid_stream() {
    let _net = net_guard();
    let big = "y".repeat(4096);
    let raw = format!("HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{big}");
    let addr = serve(vec![raw]).await;
    let client = mechanics_client_with(|c| c.max_response_bytes = 512);

    let resp = client
        .send_streaming(HttpRequest::get(format!("http://{addr}/s")))
        .await
        .expect("stream opens (cap applies to the body)");

    let mut saw_too_large = false;
    let mut body = pin!(resp.body);
    loop {
        let next = std::future::poll_fn(|cx| body.as_mut().poll_next(cx)).await;
        match next {
            Some(Ok(_)) => {}
            Some(Err(HttpError::TooLarge { max: 512, .. })) => {
                saw_too_large = true;
                break;
            }
            Some(Err(e)) => panic!("unexpected stream error: {e:?}"),
            None => break,
        }
    }
    assert!(saw_too_large, "cap must surface as TooLarge mid-stream");
}

#[tokio::test]
async fn streaming_body_exactly_at_cap_passes() {
    let _net = net_guard();
    // The cap is inclusive: seen == max is allowed, only seen > max
    // errors (pins the `>` vs `>=` boundary in CappedStream).
    let body = "z".repeat(512);
    let addr = serve(vec![ok_response(&body, "")]).await;
    let client = mechanics_client_with(|c| c.max_response_bytes = 512);

    let resp = client
        .send_streaming(HttpRequest::get(format!("http://{addr}/s")))
        .await
        .expect("stream opens");
    let mut collected = 0usize;
    let mut stream = pin!(resp.body);
    loop {
        let next = std::future::poll_fn(|cx| stream.as_mut().poll_next(cx)).await;
        match next {
            Some(Ok(chunk)) => collected += chunk.len(),
            Some(Err(e)) => panic!("exact-cap body must stream fully, got {e:?}"),
            None => break,
        }
    }
    assert_eq!(collected, 512);
}

#[tokio::test]
async fn body_exactly_at_cap_passes_non_streaming() {
    let _net = net_guard();
    let body = "w".repeat(256);
    let addr = serve(vec![ok_response(&body, "")]).await;
    let client = mechanics_client_with(|c| c.max_response_bytes = 256);

    let resp = client
        .get(HttpRequest::get(format!("http://{addr}/exact")))
        .await
        .expect("exact-cap body is allowed (cap is inclusive)");
    assert_eq!(resp.body.len(), 256);
}

#[tokio::test]
async fn streaming_content_length_above_cap_fails_fast() {
    let _net = net_guard();
    let addr = serve(vec![
        "HTTP/1.1 200 OK\r\nContent-Length: 9999\r\nConnection: close\r\n\r\n".to_string(),
    ])
    .await;
    let client = mechanics_client_with(|c| c.max_response_bytes = 100);

    let err = client
        .send_streaming(HttpRequest::get(format!("http://{addr}/s")))
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            HttpError::TooLarge {
                size: 9999,
                max: 100
            }
        ),
        "got {err:?}"
    );
}

// ─── POST ────────────────────────────────────────────────────────────

#[tokio::test]
async fn post_sends_body_and_returns_response() {
    let _net = net_guard();
    let addr = serve(vec![ok_response("posted", "")]).await;
    let client = mechanics_client();

    let mut req = HttpRequest::post(format!("http://{addr}/submit"));
    req.body = Some(bytes::Bytes::from_static(b"hello-body"));
    req.headers
        .insert("content-type".into(), "text/plain".into());

    let resp = client.post(req).await.expect("post ok");
    assert_eq!(resp.status, 200);
    assert_eq!(&resp.body[..], b"posted");
}

// ─── method-recording fixture (redirect demotion) ───────────────────

/// Like `serve`, but records the request-line METHOD of every accepted
/// connection into the returned handle — so a test can assert what the
/// client actually sent at each hop (303→GET demotion · 307 preserve).
async fn serve_recording(responses: Vec<String>) -> (SocketAddr, Arc<Mutex<Vec<String>>>) {
    let (addr, _heads, methods) = serve_recording_full(responses).await;
    (addr, methods)
}

/// Like `serve_recording` but also captures each connection's FULL
/// request head (for header assertions across redirect hops).
#[allow(clippy::type_complexity)]
async fn serve_recording_full(
    responses: Vec<String>,
) -> (SocketAddr, Arc<Mutex<Vec<String>>>, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let methods: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let heads: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let m_sink = Arc::clone(&methods);
    let h_sink = Arc::clone(&heads);
    tokio::spawn(async move {
        let mut i = 0usize;
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                break;
            };
            let resp = responses[i.min(responses.len() - 1)].clone();
            i += 1;
            let m_sink = Arc::clone(&m_sink);
            let h_sink = Arc::clone(&h_sink);
            tokio::spawn(async move {
                let head = read_head(&mut sock).await;
                if let Some(method) = head.split_whitespace().next() {
                    m_sink.lock().unwrap().push(method.to_string());
                }
                h_sink.lock().unwrap().push(head);
                let _ = sock.write_all(resp.as_bytes()).await;
                let _ = sock.shutdown().await;
            });
        }
    });
    (addr, heads, methods)
}

fn status_redirect(code: u16, reason: &str, location: &str) -> String {
    format!(
        "HTTP/1.1 {code} {reason}\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )
}

#[tokio::test]
async fn post_303_demotes_to_get_at_final_hop() {
    let _net = net_guard();
    let (addr, methods) = serve_recording(vec![
        status_redirect(303, "See Other", "/result"),
        ok_response("done", ""),
    ])
    .await;
    let client = mechanics_client();

    let mut req = HttpRequest::post(format!("http://{addr}/submit"));
    req.body = Some(bytes::Bytes::from_static(b"payload"));
    let resp = client.post(req).await.expect("303 followed");
    assert_eq!(&resp.body[..], b"done");

    let seen = methods.lock().unwrap().clone();
    assert_eq!(seen, vec!["POST", "GET"], "303 must demote POST→GET");
}

#[tokio::test]
async fn post_301_demotes_to_get() {
    let _net = net_guard();
    let (addr, methods) = serve_recording(vec![
        status_redirect(301, "Moved Permanently", "/moved"),
        ok_response("done", ""),
    ])
    .await;
    let client = mechanics_client();

    let mut req = HttpRequest::post(format!("http://{addr}/submit"));
    req.body = Some(bytes::Bytes::from_static(b"payload"));
    client.post(req).await.expect("301 followed");

    let seen = methods.lock().unwrap().clone();
    assert_eq!(seen, vec!["POST", "GET"], "301 demotes POST→GET (de-facto)");
}

#[tokio::test]
async fn post_307_preserves_method_and_body() {
    let _net = net_guard();
    let (addr, methods) = serve_recording(vec![
        status_redirect(307, "Temporary Redirect", "/keep"),
        ok_response("done", ""),
    ])
    .await;
    let client = mechanics_client();

    let mut req = HttpRequest::post(format!("http://{addr}/submit"));
    req.body = Some(bytes::Bytes::from_static(b"payload"));
    client.post(req).await.expect("307 followed");

    let seen = methods.lock().unwrap().clone();
    assert_eq!(seen, vec!["POST", "POST"], "307 must preserve the method");
}

#[tokio::test]
async fn post_308_preserves_method() {
    let _net = net_guard();
    let (addr, methods) = serve_recording(vec![
        status_redirect(308, "Permanent Redirect", "/keep"),
        ok_response("done", ""),
    ])
    .await;
    let client = mechanics_client();

    let mut req = HttpRequest::post(format!("http://{addr}/submit"));
    req.body = Some(bytes::Bytes::from_static(b"payload"));
    client.post(req).await.expect("308 followed");

    let seen = methods.lock().unwrap().clone();
    assert_eq!(seen, vec!["POST", "POST"], "308 must preserve the method");
}

// ─── non-followable 3xx returned verbatim ────────────────────────────

#[tokio::test]
async fn not_modified_304_returned_verbatim_not_error() {
    let _net = net_guard();
    // 304 carries no Location — it must reach the caller, not become a
    // "redirect without Location" Other error (conditional-GET answer).
    let addr = serve(vec![
        "HTTP/1.1 304 Not Modified\r\nETag: \"abc\"\r\nConnection: close\r\n\r\n".to_string(),
    ])
    .await;
    let client = mechanics_client();
    let resp = client
        .get(HttpRequest::get(format!("http://{addr}/cached")))
        .await
        .expect("304 is a normal response");
    assert_eq!(resp.status, 304);
    assert_eq!(
        resp.headers.get("etag").map(String::as_str),
        Some("\"abc\"")
    );
}

#[tokio::test]
async fn multiple_choices_300_returned_verbatim() {
    let _net = net_guard();
    let addr = serve(vec![
        "HTTP/1.1 300 Multiple Choices\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            .to_string(),
    ])
    .await;
    let client = mechanics_client();
    let resp = client
        .get(HttpRequest::get(format!("http://{addr}/choices")))
        .await
        .expect("300 returned as-is (no Location follow)");
    assert_eq!(resp.status, 300);
}

#[tokio::test]
async fn head_stays_head_on_303() {
    let _net = net_guard();
    // RFC 9110 §15.4.4: 303 changes other methods to GET but keeps HEAD.
    let (addr, methods) = serve_recording(vec![
        status_redirect(303, "See Other", "/result"),
        ok_response("", ""),
    ])
    .await;
    let client = mechanics_client();

    let mut req = HttpRequest::get(format!("http://{addr}/probe"));
    req.method = nika_kernel::HttpMethod::Head;
    client.get(req).await.expect("303 followed");

    let seen = methods.lock().unwrap().clone();
    assert_eq!(seen, vec!["HEAD", "HEAD"], "303 must keep HEAD as HEAD");
}

#[tokio::test]
async fn content_type_stripped_on_post_get_demotion() {
    let _net = net_guard();
    let (addr, heads, _m) = serve_recording_full(vec![
        status_redirect(303, "See Other", "/done"),
        ok_response("ok", ""),
    ])
    .await;
    let client = mechanics_client();

    let mut req = HttpRequest::post(format!("http://{addr}/submit"));
    req.body = Some(bytes::Bytes::from_static(b"{}"));
    req.headers
        .insert("content-type".into(), "application/json".into());
    client.post(req).await.expect("303 followed");

    let captured = heads.lock().unwrap().clone();
    let hop2 = &captured[1].to_ascii_lowercase();
    assert!(
        !hop2.contains("content-type:"),
        "Content-Type must not ride the bodyless GET, got: {hop2}"
    );
}

// ─── credential stripping on cross-origin redirect ──────────────────

fn has_auth_header(head: &str) -> bool {
    head.lines()
        .any(|l| l.to_ascii_lowercase().starts_with("authorization:"))
}

#[tokio::test]
async fn authorization_stripped_on_cross_origin_redirect() {
    let _net = net_guard();
    // hop1 redirects to a DIFFERENT host (target) — Authorization must
    // NOT reach target.
    let (target, target_heads, _tm) = serve_recording_full(vec![ok_response("landed", "")]).await;
    let (hop1, hop1_heads, _hm) =
        serve_recording_full(vec![redirect_response(&format!("http://{target}/final"))]).await;
    let client = mechanics_client();

    let mut req = HttpRequest::get(format!("http://{hop1}/start"));
    req.headers
        .insert("authorization".into(), "Bearer secret-token".into());
    let resp = client.get(req).await.expect("redirect followed");
    assert_eq!(&resp.body[..], b"landed");

    assert!(
        has_auth_header(&hop1_heads.lock().unwrap()[0]),
        "first hop (same origin as request) keeps Authorization"
    );
    assert!(
        !has_auth_header(&target_heads.lock().unwrap()[0]),
        "cross-origin hop must NOT receive Authorization (credential leak)"
    );
}

#[tokio::test]
async fn authorization_kept_on_same_origin_redirect() {
    let _net = net_guard();
    // Same host, relative Location → same origin → keep Authorization.
    let (addr, heads, _m) =
        serve_recording_full(vec![redirect_response("/next"), ok_response("ok", "")]).await;
    let client = mechanics_client();

    let mut req = HttpRequest::get(format!("http://{addr}/start"));
    req.headers
        .insert("authorization".into(), "Bearer keep-me".into());
    client
        .get(req)
        .await
        .expect("same-origin redirect followed");

    let captured = heads.lock().unwrap().clone();
    assert_eq!(captured.len(), 2, "two hops on the same server");
    assert!(has_auth_header(&captured[0]), "hop 1 has auth");
    assert!(
        has_auth_header(&captured[1]),
        "same-origin hop 2 must KEEP Authorization"
    );
}

// ─── TLS backend (https path · no fixture server) ───────────────────

#[tokio::test]
async fn tls_backend_initializes_and_https_reaches_transport() {
    let _net = net_guard();
    // No network assertion: 192.0.2.1 is RFC 5737 TEST-NET-1 (public ·
    // SSRF allows it · never routes). The point is to prove the rustls
    // backend BUILT and an https:// URL reaches the connect path — the
    // error must be a transport error (Timeout/Connection), NEVER the
    // `Other{failed to build HTTP client}` that a broken TLS init gives.
    let client = ReqwestHttp::new().expect("rustls backend initializes");
    let mut req = HttpRequest::get("https://192.0.2.1/x");
    req.timeout = Some(Duration::from_millis(200));
    let err = client.get(req).await.unwrap_err();
    assert!(
        matches!(
            err,
            HttpError::Timeout { .. } | HttpError::Connection { .. }
        ),
        "https must reach the transport (TLS init OK), got {err:?}"
    );
}

// ─── error mapping ───────────────────────────────────────────────────

#[tokio::test]
async fn timeout_maps_to_timeout_error() {
    let _net = net_guard();
    // Bind a listener that accepts but never responds.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _hold = listener.accept().await; // accept then hang
        tokio::time::sleep(Duration::from_secs(60)).await;
    });

    let client = mechanics_client();
    let mut req = HttpRequest::get(format!("http://{addr}/slow"));
    req.timeout = Some(Duration::from_millis(150));

    let err = client.get(req).await.unwrap_err();
    assert!(
        matches!(err, HttpError::Timeout { duration_ms: 150 }),
        "got {err:?}"
    );
}

#[tokio::test]
async fn connection_refused_maps_to_connection_error() {
    let _net = net_guard();
    // Reserve a port then free it: connect must be refused.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let client = mechanics_client();
    let err = client
        .get(HttpRequest::get(format!("http://{addr}/x")))
        .await
        .unwrap_err();
    assert!(matches!(err, HttpError::Connection { .. }), "got {err:?}");
}

#[tokio::test]
async fn unparseable_url_maps_to_other() {
    let client = ReqwestHttp::new().expect("client builds");
    let err = client
        .get(HttpRequest::get("not a url at all"))
        .await
        .unwrap_err();
    assert!(matches!(err, HttpError::Other { .. }), "got {err:?}");
}
