//! FETCH Verb Error Path Tests
//!
//! Tests error handling for the `fetch:` verb in TaskExecutor.
//! Uses a minimal mock HTTP server to simulate various failure scenarios.
//!
//! Coverage: Gap 2 from test-coverage-gaps.md
//! - HTTP timeout (HIGH)
//! - Invalid URL format (HIGH)
//! - Non-2xx HTTP status (MEDIUM)
//! - Connection refused (LOW)

use std::sync::Arc;
use std::time::Duration;

use nika::ast::{FetchParams, TaskAction};
use nika::binding::ResolvedBindings;
use nika::error::NikaError;
use nika::event::EventLog;
use nika::runtime::TaskExecutor;
use nika::store::DataStore;
use rustc_hash::FxHashMap;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

// ═══════════════════════════════════════════════════════════════════════════
// HELPERS
// ═══════════════════════════════════════════════════════════════════════════

/// Create a test executor with default settings
fn create_test_executor() -> TaskExecutor {
    TaskExecutor::new("mock", None, None, EventLog::new())
}

/// Create a FetchParams with GET method and empty headers
fn fetch_params(url: &str) -> FetchParams {
    FetchParams {
        url: url.to_string(),
        method: "GET".to_string(),
        headers: FxHashMap::default(),
        body: None,
    }
}

/// Create empty bindings and datastore for tests
fn empty_context() -> (ResolvedBindings, DataStore) {
    (ResolvedBindings::new(), DataStore::new())
}

/// Start a mock server that delays before responding
async fn start_delayed_server(delay: Duration) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{}", addr);

    tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            // Wait before sending response
            tokio::time::sleep(delay).await;
            let response = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK";
            let _ = socket.write_all(response.as_bytes()).await;
        }
    });

    url
}

/// Start a mock server that returns a specific status code
async fn start_status_server(status: u16, body: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{}", addr);

    let response = format!(
        "HTTP/1.1 {} Status\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
        status,
        body.len(),
        body
    );

    tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            // Read the request first (important for HTTP compliance)
            let mut buf = [0u8; 1024];
            let _ = tokio::io::AsyncReadExt::read(&mut socket, &mut buf).await;
            let _ = socket.write_all(response.as_bytes()).await;
        }
    });

    // Give the server a moment to start
    tokio::time::sleep(Duration::from_millis(10)).await;
    url
}

/// Start a mock server that returns malformed HTTP
async fn start_malformed_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{}", addr);

    tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            // Read the request
            let mut buf = [0u8; 1024];
            let _ = tokio::io::AsyncReadExt::read(&mut socket, &mut buf).await;
            // Send malformed response (not valid HTTP)
            let _ = socket.write_all(b"NOT HTTP AT ALL\r\n").await;
        }
    });

    tokio::time::sleep(Duration::from_millis(10)).await;
    url
}

// ═══════════════════════════════════════════════════════════════════════════
// HIGH PRIORITY TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_fetch_invalid_url_returns_execution_error() {
    // Arrange
    let executor = create_test_executor();
    let task_id: Arc<str> = Arc::from("fetch_invalid");
    let (bindings, datastore) = empty_context();

    // Use a completely invalid URL
    let action = TaskAction::Fetch {
        fetch: fetch_params("not-a-url"),
    };

    // Act
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore)
        .await;

    // Assert
    assert!(result.is_err(), "Invalid URL should fail");
    match result.unwrap_err() {
        NikaError::Execution(msg) => {
            assert!(
                msg.contains("HTTP request failed"),
                "Error should mention HTTP failure: {msg}"
            );
        }
        err => panic!("Expected Execution error, got: {err:?}"),
    }
}

#[tokio::test]
async fn test_fetch_invalid_scheme_returns_error() {
    // Arrange
    let executor = create_test_executor();
    let task_id: Arc<str> = Arc::from("fetch_bad_scheme");
    let (bindings, datastore) = empty_context();

    // Use an unsupported scheme
    let action = TaskAction::Fetch {
        fetch: fetch_params("ftp://example.com/file"),
    };

    // Act
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore)
        .await;

    // Assert
    assert!(result.is_err(), "Unsupported scheme should fail");
    match result.unwrap_err() {
        NikaError::Execution(msg) => {
            assert!(
                msg.contains("HTTP request failed"),
                "Error should mention HTTP failure: {msg}"
            );
        }
        err => panic!("Expected Execution error, got: {err:?}"),
    }
}

#[tokio::test]
async fn test_fetch_non_2xx_status_returns_body_not_error() {
    // NOTE: Current implementation returns body regardless of status code
    // This test documents the current behavior

    // Arrange
    let executor = create_test_executor();
    let task_id: Arc<str> = Arc::from("fetch_500");
    let (bindings, datastore) = empty_context();

    let url = start_status_server(500, "Internal Server Error").await;
    let action = TaskAction::Fetch {
        fetch: fetch_params(&url),
    };

    // Act
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore)
        .await;

    // Assert - current behavior: returns body text regardless of status
    // This documents current behavior; a future change might return an error for non-2xx
    assert!(
        result.is_ok(),
        "Current impl returns body for any HTTP response: {:?}",
        result.err()
    );
    let body = result.unwrap();
    assert!(
        body.contains("Internal Server Error"),
        "Body should contain server response: {body}"
    );
}

#[tokio::test]
async fn test_fetch_404_returns_body() {
    // Arrange
    let executor = create_test_executor();
    let task_id: Arc<str> = Arc::from("fetch_404");
    let (bindings, datastore) = empty_context();

    let url = start_status_server(404, "Not Found").await;
    let action = TaskAction::Fetch {
        fetch: fetch_params(&url),
    };

    // Act
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore)
        .await;

    // Assert
    assert!(result.is_ok(), "404 returns body: {:?}", result.err());
    assert_eq!(result.unwrap(), "Not Found");
}

#[tokio::test]
async fn test_fetch_connection_refused_returns_error() {
    // Arrange
    let executor = create_test_executor();
    let task_id: Arc<str> = Arc::from("fetch_refused");
    let (bindings, datastore) = empty_context();

    // Use a port that's definitely not listening
    // Port 1 requires root on Unix and is typically not available
    let action = TaskAction::Fetch {
        fetch: fetch_params("http://127.0.0.1:1/"),
    };

    // Act
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore)
        .await;

    // Assert
    assert!(result.is_err(), "Connection refused should fail");
    match result.unwrap_err() {
        NikaError::Execution(msg) => {
            assert!(
                msg.contains("HTTP request failed"),
                "Error should mention HTTP failure: {msg}"
            );
        }
        err => panic!("Expected Execution error, got: {err:?}"),
    }
}

#[tokio::test]
async fn test_fetch_malformed_response_returns_error() {
    // Arrange
    let executor = create_test_executor();
    let task_id: Arc<str> = Arc::from("fetch_malformed");
    let (bindings, datastore) = empty_context();

    let url = start_malformed_server().await;
    let action = TaskAction::Fetch {
        fetch: fetch_params(&url),
    };

    // Act
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore)
        .await;

    // Assert - malformed HTTP response should cause a parse error
    assert!(result.is_err(), "Malformed HTTP should fail");
    match result.unwrap_err() {
        NikaError::Execution(msg) => {
            // reqwest will fail to parse the response
            assert!(
                msg.contains("HTTP request failed") || msg.contains("Failed to read"),
                "Error should mention HTTP or read failure: {msg}"
            );
        }
        err => panic!("Expected Execution error, got: {err:?}"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TIMEOUT TESTS
// ═══════════════════════════════════════════════════════════════════════════

// NOTE: Timeout tests are tricky because the executor uses FETCH_TIMEOUT (30s)
// and CONNECT_TIMEOUT (10s) which are too long for unit tests.
// These tests document the expected behavior but may need to be run with
// a custom executor that has shorter timeouts for CI.

#[tokio::test]
#[ignore = "Requires custom executor with short timeout - takes too long for CI"]
async fn test_fetch_timeout_with_delayed_server() {
    // This test would need a way to inject a custom reqwest::Client
    // with a short timeout, which isn't currently supported.

    // Arrange
    let executor = create_test_executor();
    let task_id: Arc<str> = Arc::from("fetch_timeout");
    let (bindings, datastore) = empty_context();

    // Server delays 35 seconds (longer than FETCH_TIMEOUT of 30s)
    let url = start_delayed_server(Duration::from_secs(35)).await;
    let action = TaskAction::Fetch {
        fetch: fetch_params(&url),
    };

    // Act
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore)
        .await;

    // Assert
    assert!(result.is_err(), "Timeout should fail");
    match result.unwrap_err() {
        NikaError::Execution(msg) => {
            assert!(
                msg.contains("timeout") || msg.contains("timed out"),
                "Error should mention timeout: {msg}"
            );
        }
        err => panic!("Expected Execution error with timeout, got: {err:?}"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SUCCESSFUL FETCH TESTS (for comparison)
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_fetch_success_returns_body() {
    // Arrange
    let executor = create_test_executor();
    let task_id: Arc<str> = Arc::from("fetch_success");
    let (bindings, datastore) = empty_context();

    let url = start_status_server(200, "Hello, World!").await;
    let action = TaskAction::Fetch {
        fetch: fetch_params(&url),
    };

    // Act
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore)
        .await;

    // Assert
    assert!(result.is_ok(), "200 OK should succeed: {:?}", result.err());
    assert_eq!(result.unwrap(), "Hello, World!");
}

#[tokio::test]
async fn test_fetch_with_json_body() {
    // Arrange
    let executor = create_test_executor();
    let task_id: Arc<str> = Arc::from("fetch_json");
    let (bindings, datastore) = empty_context();

    let json_body = r#"{"status":"ok","count":42}"#;
    let url = start_status_server(200, json_body).await;
    let action = TaskAction::Fetch {
        fetch: fetch_params(&url),
    };

    // Act
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore)
        .await;

    // Assert
    assert!(result.is_ok(), "JSON response should succeed");
    let body = result.unwrap();
    assert_eq!(body, json_body);
    // Verify it's valid JSON
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(parsed["count"], 42);
}

// ═══════════════════════════════════════════════════════════════════════════
// EDGE CASES
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_fetch_empty_url_fails() {
    // Arrange
    let executor = create_test_executor();
    let task_id: Arc<str> = Arc::from("fetch_empty");
    let (bindings, datastore) = empty_context();

    let action = TaskAction::Fetch {
        fetch: fetch_params(""),
    };

    // Act
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore)
        .await;

    // Assert
    assert!(result.is_err(), "Empty URL should fail");
    match result.unwrap_err() {
        NikaError::Execution(msg) => {
            assert!(
                msg.contains("HTTP request failed"),
                "Error should mention HTTP failure: {msg}"
            );
        }
        err => panic!("Expected Execution error, got: {err:?}"),
    }
}

#[tokio::test]
async fn test_fetch_url_with_invalid_characters() {
    // Arrange
    let executor = create_test_executor();
    let task_id: Arc<str> = Arc::from("fetch_invalid_chars");
    let (bindings, datastore) = empty_context();

    // Use a URL with invalid characters that reqwest cannot handle
    // Note: reqwest is lenient with spaces (may URL-encode them)
    // but fails on truly malformed URLs
    let action = TaskAction::Fetch {
        fetch: fetch_params("http://[invalid-ipv6/path"),
    };

    // Act
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore)
        .await;

    // Assert - Malformed URLs should fail
    assert!(result.is_err(), "Malformed URL should fail");
    match result.unwrap_err() {
        NikaError::Execution(msg) => {
            assert!(
                msg.contains("HTTP request failed"),
                "Error should mention HTTP failure: {msg}"
            );
        }
        err => panic!("Expected Execution error, got: {err:?}"),
    }
}

#[tokio::test]
async fn test_fetch_localhost_unreachable_fails() {
    // Arrange
    let executor = create_test_executor();
    let task_id: Arc<str> = Arc::from("fetch_unreachable");
    let (bindings, datastore) = empty_context();

    // Use a high port that's almost certainly not in use
    let action = TaskAction::Fetch {
        fetch: fetch_params("http://127.0.0.1:59999/nonexistent"),
    };

    // Act
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore)
        .await;

    // Assert
    assert!(result.is_err(), "Unreachable host should fail");
}

#[tokio::test]
async fn test_fetch_dns_resolution_failure() {
    // Arrange
    let executor = create_test_executor();
    let task_id: Arc<str> = Arc::from("fetch_dns_fail");
    let (bindings, datastore) = empty_context();

    // Use a domain that definitely doesn't exist
    let action = TaskAction::Fetch {
        fetch: fetch_params("http://this-domain-definitely-does-not-exist-12345.invalid/"),
    };

    // Act
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore)
        .await;

    // Assert
    assert!(result.is_err(), "DNS failure should fail");
    match result.unwrap_err() {
        NikaError::Execution(msg) => {
            // DNS failures show up as connection errors
            assert!(
                msg.contains("HTTP request failed"),
                "Error should mention HTTP failure: {msg}"
            );
        }
        err => panic!("Expected Execution error, got: {err:?}"),
    }
}
