//! Wiremock-based fetch verb E2E tests
//!
//! Tests HTTP methods, response modes, extraction modes, error handling,
//! and retry behavior using wiremock for deterministic mock servers.

use super::*;
use crate::ast::FetchParams;
use crate::binding::ResolvedBindings;
use crate::event::{EventKind, EventLog};
use crate::store::RunContext;
use std::sync::Arc;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Helper: create executor + bindings + datastore for fetch tests.
/// Uses a policy that explicitly allows localhost (required because SSRF
/// protection blocks 127.0.0.1 by default, which wiremock uses).
fn setup() -> (TaskExecutor, ResolvedBindings, RunContext, EventLog) {
    let event_log = EventLog::new();
    let policy = crate::runtime::boot::PolicyConfig {
        allow_exec: true,
        allow_network: true,
        blocked_commands: vec![],
        max_token_spend: None,
        allowed_hosts: vec!["127.0.0.1".to_string(), "localhost".to_string()],
        blocked_hosts: vec![],
    };
    let executor = TaskExecutor::with_policy(
        "mock",
        None,
        None,
        event_log.clone(),
        Some(policy),
        None,
        None,
    )
    .unwrap();
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new();
    (executor, bindings, datastore, event_log)
}

/// Helper: build a FetchParams pointing at a wiremock server
fn fetch_params(url: &str, method_str: &str) -> FetchParams {
    FetchParams {
        url: url.to_string(),
        method: method_str.to_string(),
        headers: rustc_hash::FxHashMap::default(),
        body: None,
        json: None,
        timeout: None,
        retry: None,
        follow_redirects: None,
        response: None,
        extract: None,
        selector: None,
    }
}

// ═══════════════════════════════════════════════════════════════
// HTTP Method Tests
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn wiremock_fetch_get() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/data"))
        .respond_with(ResponseTemplate::new(200).set_body_string("hello"))
        .mount(&server)
        .await;

    let (executor, bindings, datastore, _) = setup();
    let task_id: Arc<str> = Arc::from("wm_get");
    let params = fetch_params(&format!("{}/data", server.uri()), "GET");
    let action = TaskAction::Fetch { fetch: params };
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    assert_eq!(result, "hello");
}

#[tokio::test]
async fn wiremock_fetch_post() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/submit"))
        .respond_with(ResponseTemplate::new(201).set_body_string(r#"{"id": 1}"#))
        .mount(&server)
        .await;

    let (executor, bindings, datastore, _) = setup();
    let task_id: Arc<str> = Arc::from("wm_post");
    let mut params = fetch_params(&format!("{}/submit", server.uri()), "POST");
    params.body = Some(r#"{"name":"test"}"#.to_string());
    let action = TaskAction::Fetch { fetch: params };
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    assert!(result.contains("\"id\""));
}

#[tokio::test]
async fn wiremock_fetch_put() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/update"))
        .respond_with(ResponseTemplate::new(200).set_body_string("updated"))
        .mount(&server)
        .await;

    let (executor, bindings, datastore, _) = setup();
    let task_id: Arc<str> = Arc::from("wm_put");
    let params = fetch_params(&format!("{}/update", server.uri()), "PUT");
    let action = TaskAction::Fetch { fetch: params };
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    assert_eq!(result, "updated");
}

#[tokio::test]
async fn wiremock_fetch_delete() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/remove"))
        .respond_with(ResponseTemplate::new(204).set_body_string(""))
        .mount(&server)
        .await;

    let (executor, bindings, datastore, _) = setup();
    let task_id: Arc<str> = Arc::from("wm_delete");
    let params = fetch_params(&format!("{}/remove", server.uri()), "DELETE");
    let action = TaskAction::Fetch { fetch: params };
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    assert_eq!(result, "");
}

#[tokio::test]
async fn wiremock_fetch_patch() {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/patch"))
        .respond_with(ResponseTemplate::new(200).set_body_string("patched"))
        .mount(&server)
        .await;

    let (executor, bindings, datastore, _) = setup();
    let task_id: Arc<str> = Arc::from("wm_patch");
    let params = fetch_params(&format!("{}/patch", server.uri()), "PATCH");
    let action = TaskAction::Fetch { fetch: params };
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    assert_eq!(result, "patched");
}

#[tokio::test]
async fn wiremock_fetch_head() {
    let server = MockServer::start().await;
    Mock::given(method("HEAD"))
        .and(path("/check"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let (executor, bindings, datastore, _) = setup();
    let task_id: Arc<str> = Arc::from("wm_head");
    let params = fetch_params(&format!("{}/check", server.uri()), "HEAD");
    let action = TaskAction::Fetch { fetch: params };
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    // HEAD has no body
    assert_eq!(result, "");
}

#[tokio::test]
async fn wiremock_fetch_options() {
    let server = MockServer::start().await;
    Mock::given(method("OPTIONS"))
        .and(path("/cors"))
        .respond_with(ResponseTemplate::new(200).insert_header("Allow", "GET, POST, OPTIONS"))
        .mount(&server)
        .await;

    let (executor, bindings, datastore, _) = setup();
    let task_id: Arc<str> = Arc::from("wm_options");
    let params = fetch_params(&format!("{}/cors", server.uri()), "OPTIONS");
    let action = TaskAction::Fetch { fetch: params };
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;
    assert!(result.is_ok());
}

// ═══════════════════════════════════════════════════════════════
// Response Mode Tests
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn wiremock_fetch_response_full() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/full"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"key":"value"}"#)
                .insert_header("Content-Type", "application/json"),
        )
        .mount(&server)
        .await;

    let (executor, bindings, datastore, _) = setup();
    let task_id: Arc<str> = Arc::from("wm_full");
    let mut params = fetch_params(&format!("{}/full", server.uri()), "GET");
    params.response = Some(nika_core::ast::extract::ResponseMode::Full);
    let action = TaskAction::Fetch { fetch: params };
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["status"], 200);
    assert!(parsed["headers"].is_object());
    assert!(parsed["body"].as_str().unwrap().contains("key"));
    assert!(parsed["url"].as_str().unwrap().contains("/full"));
    assert!(
        parsed["elapsed_ms"].is_u64(),
        "response:full must include elapsed_ms"
    );
}

#[tokio::test]
async fn wiremock_fetch_response_binary_stores_in_cas() {
    let server = MockServer::start().await;
    let binary_data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]; // PNG magic
    Mock::given(method("GET"))
        .and(path("/image.png"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(binary_data.clone())
                .insert_header("Content-Type", "image/png"),
        )
        .mount(&server)
        .await;

    let (executor, bindings, datastore, _) = setup();
    let task_id: Arc<str> = Arc::from("wm_binary");
    let mut params = fetch_params(&format!("{}/image.png", server.uri()), "GET");
    params.response = Some(nika_core::ast::extract::ResponseMode::Binary);
    let action = TaskAction::Fetch { fetch: params };
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert!(parsed["hash"].as_str().unwrap().starts_with("blake3:"));
    assert_eq!(parsed["mime_type"], "image/png");
    assert_eq!(parsed["size_bytes"], binary_data.len());
}

/// Fetch binary must stage a MediaRef in the datastore so that
/// artifact format: binary can find it. Without this, NIKA-281 fires
/// because write_binary_artifact() gets an empty media_refs slice.
#[tokio::test]
async fn wiremock_fetch_response_binary_stages_media_ref() {
    let server = MockServer::start().await;
    let binary_data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]; // PNG magic
    Mock::given(method("GET"))
        .and(path("/media.png"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(binary_data.clone())
                .insert_header("Content-Type", "image/png"),
        )
        .mount(&server)
        .await;

    let (executor, bindings, datastore, _) = setup();
    let task_id: Arc<str> = Arc::from("wm_binary_media");
    let mut params = fetch_params(&format!("{}/media.png", server.uri()), "GET");
    params.response = Some(nika_core::ast::extract::ResponseMode::Binary);
    let action = TaskAction::Fetch { fetch: params };
    let _result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();

    // After execute, the datastore MUST have a staged MediaRef
    let media_refs = datastore.take_media(&task_id);
    assert_eq!(
        media_refs.len(),
        1,
        "fetch binary must stage exactly 1 MediaRef"
    );

    let mr = &media_refs[0];
    assert!(
        mr.hash.starts_with("blake3:"),
        "MediaRef hash must be blake3-prefixed"
    );
    assert_eq!(mr.mime_type, "image/png");
    assert_eq!(mr.size_bytes, binary_data.len() as u64);
    assert_eq!(mr.created_by, "wm_binary_media");
    assert!(mr.path.exists(), "CAS file must exist on disk");
}

// ═══════════════════════════════════════════════════════════════
// Extraction Mode Tests
// ═══════════════════════════════════════════════════════════════

#[cfg(feature = "fetch-markdown")]
#[tokio::test]
async fn wiremock_fetch_extract_markdown() {
    let server = MockServer::start().await;
    let html = "<html><body><h1>Title</h1><p>Hello <strong>world</strong></p></body></html>";
    Mock::given(method("GET"))
        .and(path("/page"))
        .respond_with(ResponseTemplate::new(200).set_body_string(html))
        .mount(&server)
        .await;

    let (executor, bindings, datastore, _) = setup();
    let task_id: Arc<str> = Arc::from("wm_md");
    let mut params = fetch_params(&format!("{}/page", server.uri()), "GET");
    params.extract = Some(nika_core::ast::extract::ExtractMode::Markdown);
    let action = TaskAction::Fetch { fetch: params };
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    assert!(result.contains("# Title"));
    assert!(result.contains("**world**"));
}

#[cfg(feature = "fetch-html")]
#[tokio::test]
async fn wiremock_fetch_extract_text_with_selector() {
    let server = MockServer::start().await;
    let html =
        r#"<html><body><p class="target">Found</p><p class="other">Hidden</p></body></html>"#;
    Mock::given(method("GET"))
        .and(path("/text"))
        .respond_with(ResponseTemplate::new(200).set_body_string(html))
        .mount(&server)
        .await;

    let (executor, bindings, datastore, _) = setup();
    let task_id: Arc<str> = Arc::from("wm_text");
    let mut params = fetch_params(&format!("{}/text", server.uri()), "GET");
    params.extract = Some(nika_core::ast::extract::ExtractMode::Text);
    params.selector = Some("p.target".to_string());
    let action = TaskAction::Fetch { fetch: params };
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    assert!(result.contains("Found"));
    assert!(!result.contains("Hidden"));
}

#[cfg(feature = "fetch-html")]
#[tokio::test]
async fn wiremock_fetch_extract_metadata_with_og_tags() {
    let server = MockServer::start().await;
    let html = r#"<html><head>
        <title>My Page</title>
        <meta name="description" content="Page desc">
        <meta property="og:title" content="OG Title">
        <meta property="og:image" content="https://img.example.com/og.png">
    </head><body></body></html>"#;
    Mock::given(method("GET"))
        .and(path("/meta"))
        .respond_with(ResponseTemplate::new(200).set_body_string(html))
        .mount(&server)
        .await;

    let (executor, bindings, datastore, _) = setup();
    let task_id: Arc<str> = Arc::from("wm_meta");
    let mut params = fetch_params(&format!("{}/meta", server.uri()), "GET");
    params.extract = Some(nika_core::ast::extract::ExtractMode::Metadata);
    let action = TaskAction::Fetch { fetch: params };
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["title"], "My Page");
    assert_eq!(parsed["og"]["title"], "OG Title");
}

#[tokio::test]
async fn wiremock_fetch_extract_metadata_hreflang() {
    let server = MockServer::start().await;
    let html = r#"<html><head>
        <title>Page</title>
        <link rel="alternate" hreflang="en" href="https://example.com/en/page">
        <link rel="alternate" hreflang="fr" href="https://example.com/fr/page">
        <link rel="alternate" hreflang="x-default" href="https://example.com/page">
    </head><body></body></html>"#;
    Mock::given(method("GET"))
        .and(path("/hreflang"))
        .respond_with(ResponseTemplate::new(200).set_body_string(html))
        .mount(&server)
        .await;

    let (executor, bindings, datastore, _) = setup();
    let task_id: Arc<str> = Arc::from("wm_hreflang");
    let mut params = fetch_params(&format!("{}/hreflang", server.uri()), "GET");
    params.extract = Some(nika_core::ast::extract::ExtractMode::Metadata);
    let action = TaskAction::Fetch { fetch: params };
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["title"], "Page");
    let hreflang = parsed["hreflang"].as_array().expect("hreflang should be array");
    assert_eq!(hreflang.len(), 3);
    assert_eq!(hreflang[0]["lang"], "en");
    assert_eq!(hreflang[0]["href"], "https://example.com/en/page");
    assert_eq!(hreflang[1]["lang"], "fr");
    assert_eq!(hreflang[2]["lang"], "x-default");
}

#[tokio::test]
async fn wiremock_fetch_extract_metadata_no_hreflang() {
    let server = MockServer::start().await;
    let html = r#"<html><head><title>Simple</title></head><body></body></html>"#;
    Mock::given(method("GET"))
        .and(path("/simple"))
        .respond_with(ResponseTemplate::new(200).set_body_string(html))
        .mount(&server)
        .await;

    let (executor, bindings, datastore, _) = setup();
    let task_id: Arc<str> = Arc::from("wm_no_hreflang");
    let mut params = fetch_params(&format!("{}/simple", server.uri()), "GET");
    params.extract = Some(nika_core::ast::extract::ExtractMode::Metadata);
    let action = TaskAction::Fetch { fetch: params };
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["title"], "Simple");
    // hreflang should be absent (not empty array)
    assert!(parsed.get("hreflang").is_none());
}

#[tokio::test]
async fn wiremock_fetch_extract_jsonpath() {
    let server = MockServer::start().await;
    let json = r#"{"users": [{"name": "Alice"}, {"name": "Bob"}]}"#;
    Mock::given(method("GET"))
        .and(path("/api"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(json)
                .insert_header("Content-Type", "application/json"),
        )
        .mount(&server)
        .await;

    let (executor, bindings, datastore, _) = setup();
    let task_id: Arc<str> = Arc::from("wm_jsonpath");
    let mut params = fetch_params(&format!("{}/api", server.uri()), "GET");
    params.extract = Some(nika_core::ast::extract::ExtractMode::Jsonpath);
    params.selector = Some("$.users[*].name".to_string());
    let action = TaskAction::Fetch { fetch: params };
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    assert_eq!(result, r#"["Alice","Bob"]"#);
}

// ═══════════════════════════════════════════════════════════════
// Error + Edge Case Tests
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn wiremock_fetch_timeout() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/slow"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("slow")
                .set_delay(std::time::Duration::from_secs(10)),
        )
        .mount(&server)
        .await;

    let (executor, bindings, datastore, _) = setup();
    let task_id: Arc<str> = Arc::from("wm_timeout");
    let mut params = fetch_params(&format!("{}/slow", server.uri()), "GET");
    params.timeout = Some(1); // 1 second timeout
    let action = TaskAction::Fetch { fetch: params };
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("failed") || err.contains("timeout") || err.contains("timed out"),
        "Expected timeout error, got: {}",
        err
    );
}

#[tokio::test]
async fn wiremock_fetch_retry_on_503() {
    let server = MockServer::start().await;

    // First request: 503, second: 200
    Mock::given(method("GET"))
        .and(path("/retry"))
        .respond_with(ResponseTemplate::new(503).set_body_string("Service Unavailable"))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/retry"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let (executor, bindings, datastore, _) = setup();
    let task_id: Arc<str> = Arc::from("wm_retry");
    let mut params = fetch_params(&format!("{}/retry", server.uri()), "GET");
    // Construct RetryConfig via deserialization since the type is in a private module
    let retry_cfg: serde_json::Value = serde_json::json!({
        "max_attempts": 3,
        "backoff_ms": 10,
        "multiplier": 1.0
    });
    params.retry = Some(serde_json::from_value(retry_cfg).unwrap());
    let action = TaskAction::Fetch { fetch: params };
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    assert_eq!(result, "ok");
}

#[tokio::test]
async fn wiremock_fetch_json_body_auto_content_type() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/json"))
        .and(header("Content-Type", "application/json"))
        .respond_with(ResponseTemplate::new(200).set_body_string("accepted"))
        .mount(&server)
        .await;

    let (executor, bindings, datastore, _) = setup();
    let task_id: Arc<str> = Arc::from("wm_json_body");
    let mut params = fetch_params(&format!("{}/json", server.uri()), "POST");
    params.json = Some(serde_json::json!({"key": "value"}));
    let action = TaskAction::Fetch { fetch: params };
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    assert_eq!(result, "accepted");
}

#[tokio::test]
async fn wiremock_fetch_custom_headers() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/auth"))
        .and(header("Authorization", "Bearer test-token"))
        .respond_with(ResponseTemplate::new(200).set_body_string("authorized"))
        .mount(&server)
        .await;

    let (executor, bindings, datastore, _) = setup();
    let task_id: Arc<str> = Arc::from("wm_headers");
    let mut params = fetch_params(&format!("{}/auth", server.uri()), "GET");
    params
        .headers
        .insert("Authorization".to_string(), "Bearer test-token".to_string());
    let action = TaskAction::Fetch { fetch: params };
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    assert_eq!(result, "authorized");
}

// ═══════════════════════════════════════════════════════════════
// HTTP Event Emission Tests
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn wiremock_fetch_emits_http_request_and_response_events() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/events"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("ok")
                .insert_header("Content-Type", "text/plain"),
        )
        .mount(&server)
        .await;

    let (executor, bindings, datastore, event_log) = setup();
    let task_id: Arc<str> = Arc::from("wm_events");
    let params = fetch_params(&format!("{}/events", server.uri()), "GET");
    let action = TaskAction::Fetch { fetch: params };
    executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();

    let events = event_log.filter_task("wm_events");

    // Check HttpRequest event
    let http_req: Vec<_> = events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::HttpRequest { .. }))
        .collect();
    assert_eq!(http_req.len(), 1, "Should emit exactly 1 HttpRequest event");
    if let EventKind::HttpRequest {
        method, has_body, ..
    } = &http_req[0].kind
    {
        assert_eq!(method, "GET");
        assert!(!has_body);
    }

    // Check HttpResponse event
    let http_resp: Vec<_> = events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::HttpResponse { .. }))
        .collect();
    assert_eq!(
        http_resp.len(),
        1,
        "Should emit exactly 1 HttpResponse event"
    );
    if let EventKind::HttpResponse {
        status_code,
        content_type,
        elapsed_ms,
        ..
    } = &http_resp[0].kind
    {
        assert_eq!(*status_code, 200);
        assert_eq!(content_type.as_deref(), Some("text/plain"));
        assert!(*elapsed_ms < 5000); // Sanity: should be fast
    }
}

#[tokio::test]
async fn wiremock_fetch_post_with_body_emits_has_body_true() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/body"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let (executor, bindings, datastore, event_log) = setup();
    let task_id: Arc<str> = Arc::from("wm_body_flag");
    let mut params = fetch_params(&format!("{}/body", server.uri()), "POST");
    params.body = Some("payload".to_string());
    let action = TaskAction::Fetch { fetch: params };
    executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();

    let events = event_log.filter_task("wm_body_flag");
    let http_req: Vec<_> = events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::HttpRequest { .. }))
        .collect();
    if let EventKind::HttpRequest { has_body, .. } = &http_req[0].kind {
        assert!(has_body, "POST with body should have has_body=true");
    }
}

// ═══════════════════════════════════════════════════════════════
// Additional Tests for Coverage
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn wiremock_fetch_404_returns_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/missing"))
        .respond_with(ResponseTemplate::new(404).set_body_string("not found"))
        .mount(&server)
        .await;

    let (executor, bindings, datastore, _) = setup();
    let task_id: Arc<str> = Arc::from("wm_404");
    let params = fetch_params(&format!("{}/missing", server.uri()), "GET");
    let action = TaskAction::Fetch { fetch: params };
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;
    assert!(
        result.is_err(),
        "404 should return error, got: {:?}",
        result.ok()
    );
    let err = result.unwrap_err().to_string();
    assert!(err.contains("404"), "Error should mention 404: {err}");
}

#[tokio::test]
async fn wiremock_fetch_empty_response() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/empty"))
        .respond_with(ResponseTemplate::new(200).set_body_string(""))
        .mount(&server)
        .await;

    let (executor, bindings, datastore, _) = setup();
    let task_id: Arc<str> = Arc::from("wm_empty");
    let params = fetch_params(&format!("{}/empty", server.uri()), "GET");
    let action = TaskAction::Fetch { fetch: params };
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    assert_eq!(result, "");
}

#[tokio::test]
async fn wiremock_fetch_large_json_response() {
    let server = MockServer::start().await;
    let items: Vec<serde_json::Value> = (0..100)
        .map(|i| serde_json::json!({"id": i, "name": format!("item_{}", i)}))
        .collect();
    let body = serde_json::to_string(&items).unwrap();
    Mock::given(method("GET"))
        .and(path("/large"))
        .respond_with(ResponseTemplate::new(200).set_body_string(&body))
        .mount(&server)
        .await;

    let (executor, bindings, datastore, _) = setup();
    let task_id: Arc<str> = Arc::from("wm_large");
    let params = fetch_params(&format!("{}/large", server.uri()), "GET");
    let action = TaskAction::Fetch { fetch: params };
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed.len(), 100);
}

#[tokio::test]
async fn wiremock_fetch_gzip_response() {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

    let server = MockServer::start().await;
    let body = "Hello, compressed world!";
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(body.as_bytes()).unwrap();
    let compressed = encoder.finish().unwrap();

    Mock::given(method("GET"))
        .and(path("/gzip"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(compressed)
                .insert_header("Content-Encoding", "gzip"),
        )
        .mount(&server)
        .await;

    let (executor, bindings, datastore, _) = setup();
    let task_id: Arc<str> = Arc::from("wm_gzip");
    let params = fetch_params(&format!("{}/gzip", server.uri()), "GET");
    let action = TaskAction::Fetch { fetch: params };
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    assert_eq!(result, body);
}

#[tokio::test]
async fn wiremock_fetch_template_resolved_in_url() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/items/42"))
        .respond_with(ResponseTemplate::new(200).set_body_string("found"))
        .mount(&server)
        .await;

    let event_log = EventLog::new();
    let policy = crate::runtime::boot::PolicyConfig {
        allow_exec: true,
        allow_network: true,
        blocked_commands: vec![],
        max_token_spend: None,
        allowed_hosts: vec!["127.0.0.1".to_string(), "localhost".to_string()],
        blocked_hosts: vec![],
    };
    let executor = TaskExecutor::with_policy(
        "mock",
        None,
        None,
        event_log.clone(),
        Some(policy),
        None,
        None,
    )
    .unwrap();
    let mut bindings = ResolvedBindings::new();
    bindings.set("item_id", serde_json::json!("42"));
    let datastore = RunContext::new();

    let task_id: Arc<str> = Arc::from("wm_tpl");
    let params = fetch_params(
        &format!("{}/items/{{{{with.item_id}}}}", server.uri()),
        "GET",
    );
    let action = TaskAction::Fetch { fetch: params };
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    assert_eq!(result, "found");

    // Verify TemplateResolved event was emitted
    let events = event_log.filter_task("wm_tpl");
    let tpl: Vec<_> = events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::TemplateResolved { .. }))
        .collect();
    assert_eq!(tpl.len(), 1);
}

#[tokio::test]
async fn wiremock_fetch_returns_error_on_exhausted_5xx_retries() {
    let server = MockServer::start().await;

    // ALL requests return 500 — no recovery
    Mock::given(method("GET"))
        .and(path("/always-500"))
        .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
        .mount(&server)
        .await;

    let (executor, bindings, datastore, event_log) = setup();
    let task_id: Arc<str> = Arc::from("wm_exhaust_5xx");
    let mut params = fetch_params(&format!("{}/always-500", server.uri()), "GET");
    let retry_cfg: serde_json::Value = serde_json::json!({
        "max_attempts": 2,
        "backoff_ms": 10,
        "multiplier": 1.0
    });
    params.retry = Some(serde_json::from_value(retry_cfg).unwrap());
    let action = TaskAction::Fetch { fetch: params };

    // Must return Err, NOT Ok with empty/error body
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;
    assert!(
        result.is_err(),
        "fetch should fail after exhausting retries on 500, got: {:?}",
        result
    );

    // Verify retry events were emitted
    let events = event_log.filter_task("wm_exhaust_5xx");
    let retries: Vec<_> = events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::FetchRetry { .. }))
        .collect();
    assert!(
        !retries.is_empty(),
        "Should have emitted at least one FetchRetry event"
    );
}
