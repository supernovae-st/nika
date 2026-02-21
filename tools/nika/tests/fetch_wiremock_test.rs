//! HTTP fetch tests using wiremock for isolated mocking
//!
//! This module demonstrates cleaner HTTP mocking compared to manual TcpListener.
//!
//! ## Benefits vs TcpListener mocks
//!
//! | Aspect | TcpListener (old) | wiremock (new) |
//! |--------|-------------------|----------------|
//! | Setup | ~15 lines per server | ~5 lines |
//! | HTTP compliance | Manual response strings | Automatic |
//! | Request matching | Not available | Built-in matchers |
//! | Verification | Manual | `expect(n)` assertions |
//! | JSON responses | Manual serialization | `.set_body_json()` |
//! | Error simulation | Manual HTTP strings | ResponseTemplate |
//! | Delays | `tokio::time::sleep` | `.set_delay()` |
//! | Multiple requests | Complex state | `.mount()` + `expect()` |

use std::sync::Arc;

use nika::ast::{FetchParams, TaskAction};
use nika::binding::ResolvedBindings;
use nika::event::EventLog;
use nika::runtime::TaskExecutor;
use nika::store::DataStore;
use rustc_hash::FxHashMap;
use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// =============================================================================
// HELPERS
// =============================================================================

/// Create a test executor with default settings
fn create_test_executor() -> TaskExecutor {
    TaskExecutor::new("mock", None, None, EventLog::new())
}

/// Create a FetchParams with specified method and optional headers/body
fn fetch_params(url: &str, http_method: &str, body: Option<String>) -> FetchParams {
    FetchParams {
        url: url.to_string(),
        method: http_method.to_string(),
        headers: FxHashMap::default(),
        body,
    }
}

/// Create a FetchParams with custom headers
fn fetch_params_with_headers(
    url: &str,
    http_method: &str,
    headers: Vec<(&str, &str)>,
) -> FetchParams {
    let mut h = FxHashMap::default();
    for (k, v) in headers {
        h.insert(k.to_string(), v.to_string());
    }
    FetchParams {
        url: url.to_string(),
        method: http_method.to_string(),
        headers: h,
        body: None,
    }
}

/// Create empty bindings and datastore for tests
fn empty_context() -> (ResolvedBindings, DataStore) {
    (ResolvedBindings::new(), DataStore::new())
}

// =============================================================================
// BASIC GET TESTS
// =============================================================================

#[tokio::test]
async fn test_fetch_get_with_wiremock() {
    // Arrange
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/data"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"status": "ok", "count": 42})),
        )
        .mount(&mock_server)
        .await;

    let executor = create_test_executor();
    let task_id: Arc<str> = Arc::from("fetch_get");
    let (bindings, datastore) = empty_context();

    let url = format!("{}/api/data", mock_server.uri());
    let action = TaskAction::Fetch {
        fetch: fetch_params(&url, "GET", None),
    };

    // Act
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore)
        .await;

    // Assert
    assert!(result.is_ok(), "GET should succeed: {:?}", result.err());
    let body = result.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(parsed["status"], "ok");
    assert_eq!(parsed["count"], 42);
}

#[tokio::test]
async fn test_fetch_get_plain_text() {
    // Arrange
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/hello"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Hello, World!"))
        .mount(&mock_server)
        .await;

    let executor = create_test_executor();
    let task_id: Arc<str> = Arc::from("fetch_text");
    let (bindings, datastore) = empty_context();

    let url = format!("{}/hello", mock_server.uri());
    let action = TaskAction::Fetch {
        fetch: fetch_params(&url, "GET", None),
    };

    // Act
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore)
        .await;

    // Assert
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Hello, World!");
}

// =============================================================================
// POST TESTS
// =============================================================================

#[tokio::test]
async fn test_fetch_post_with_body() {
    // Arrange
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/submit"))
        .and(body_json(json!({"name": "test", "value": 123})))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"id": "abc-123"})))
        .expect(1) // Verify called exactly once
        .mount(&mock_server)
        .await;

    let executor = create_test_executor();
    let task_id: Arc<str> = Arc::from("fetch_post");
    let (bindings, datastore) = empty_context();

    let url = format!("{}/api/submit", mock_server.uri());
    let body = serde_json::to_string(&json!({"name": "test", "value": 123})).unwrap();
    let action = TaskAction::Fetch {
        fetch: fetch_params(&url, "POST", Some(body)),
    };

    // Act
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore)
        .await;

    // Assert
    assert!(result.is_ok(), "POST should succeed: {:?}", result.err());
    let response_body = result.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&response_body).unwrap();
    assert_eq!(parsed["id"], "abc-123");
}

#[tokio::test]
async fn test_fetch_post_empty_body() {
    // Arrange
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/ping"))
        .respond_with(ResponseTemplate::new(204)) // No content
        .mount(&mock_server)
        .await;

    let executor = create_test_executor();
    let task_id: Arc<str> = Arc::from("fetch_post_empty");
    let (bindings, datastore) = empty_context();

    let url = format!("{}/api/ping", mock_server.uri());
    let action = TaskAction::Fetch {
        fetch: fetch_params(&url, "POST", None),
    };

    // Act
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore)
        .await;

    // Assert
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), ""); // 204 returns empty body
}

// =============================================================================
// ERROR RESPONSE TESTS
// =============================================================================

#[tokio::test]
async fn test_fetch_500_error_returns_body() {
    // Arrange
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/error"))
        .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
        .mount(&mock_server)
        .await;

    let executor = create_test_executor();
    let task_id: Arc<str> = Arc::from("fetch_500");
    let (bindings, datastore) = empty_context();

    let url = format!("{}/api/error", mock_server.uri());
    let action = TaskAction::Fetch {
        fetch: fetch_params(&url, "GET", None),
    };

    // Act
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore)
        .await;

    // Assert - current behavior: returns body regardless of status code
    // This documents current behavior; a future change might return an error for non-2xx
    assert!(result.is_ok(), "500 returns body: {:?}", result.err());
    assert_eq!(result.unwrap(), "Internal Server Error");
}

#[tokio::test]
async fn test_fetch_404_error_returns_body() {
    // Arrange
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/missing"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(json!({"error": "Not Found", "code": 404})),
        )
        .mount(&mock_server)
        .await;

    let executor = create_test_executor();
    let task_id: Arc<str> = Arc::from("fetch_404");
    let (bindings, datastore) = empty_context();

    let url = format!("{}/api/missing", mock_server.uri());
    let action = TaskAction::Fetch {
        fetch: fetch_params(&url, "GET", None),
    };

    // Act
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore)
        .await;

    // Assert
    assert!(result.is_ok());
    let body = result.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(parsed["code"], 404);
}

#[tokio::test]
async fn test_fetch_401_unauthorized() {
    // Arrange
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/protected"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_json(json!({"error": "Unauthorized", "message": "Invalid token"})),
        )
        .mount(&mock_server)
        .await;

    let executor = create_test_executor();
    let task_id: Arc<str> = Arc::from("fetch_401");
    let (bindings, datastore) = empty_context();

    let url = format!("{}/api/protected", mock_server.uri());
    let action = TaskAction::Fetch {
        fetch: fetch_params(&url, "GET", None),
    };

    // Act
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore)
        .await;

    // Assert
    assert!(result.is_ok());
    let body = result.unwrap();
    assert!(body.contains("Unauthorized"));
}

// =============================================================================
// HEADER TESTS
// =============================================================================

#[tokio::test]
async fn test_fetch_with_authorization_header() {
    // Arrange
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/secure"))
        .and(header("Authorization", "Bearer test-token-123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"user": "authenticated"})))
        .mount(&mock_server)
        .await;

    let executor = create_test_executor();
    let task_id: Arc<str> = Arc::from("fetch_auth");
    let (bindings, datastore) = empty_context();

    let url = format!("{}/api/secure", mock_server.uri());
    let action = TaskAction::Fetch {
        fetch: fetch_params_with_headers(
            &url,
            "GET",
            vec![("Authorization", "Bearer test-token-123")],
        ),
    };

    // Act
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore)
        .await;

    // Assert
    assert!(
        result.is_ok(),
        "Auth header should work: {:?}",
        result.err()
    );
    let body = result.unwrap();
    assert!(body.contains("authenticated"));
}

#[tokio::test]
async fn test_fetch_with_content_type_header() {
    // Arrange
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/json"))
        .and(header("Content-Type", "application/json"))
        .respond_with(ResponseTemplate::new(200).set_body_string("OK"))
        .mount(&mock_server)
        .await;

    let executor = create_test_executor();
    let task_id: Arc<str> = Arc::from("fetch_content_type");
    let (bindings, datastore) = empty_context();

    let url = format!("{}/api/json", mock_server.uri());
    let action = TaskAction::Fetch {
        fetch: fetch_params_with_headers(&url, "POST", vec![("Content-Type", "application/json")]),
    };

    // Act
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore)
        .await;

    // Assert
    assert!(result.is_ok());
}

// =============================================================================
// HTTP METHOD TESTS
// =============================================================================

#[tokio::test]
async fn test_fetch_put_method() {
    // Arrange
    let mock_server = MockServer::start().await;

    Mock::given(method("PUT"))
        .and(path("/api/resource/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"updated": true})))
        .mount(&mock_server)
        .await;

    let executor = create_test_executor();
    let task_id: Arc<str> = Arc::from("fetch_put");
    let (bindings, datastore) = empty_context();

    let url = format!("{}/api/resource/1", mock_server.uri());
    let body = serde_json::to_string(&json!({"name": "updated"})).unwrap();
    let action = TaskAction::Fetch {
        fetch: fetch_params(&url, "PUT", Some(body)),
    };

    // Act
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore)
        .await;

    // Assert
    assert!(result.is_ok());
    let response_body = result.unwrap();
    assert!(response_body.contains("updated"));
}

#[tokio::test]
async fn test_fetch_delete_method() {
    // Arrange
    let mock_server = MockServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/api/resource/1"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&mock_server)
        .await;

    let executor = create_test_executor();
    let task_id: Arc<str> = Arc::from("fetch_delete");
    let (bindings, datastore) = empty_context();

    let url = format!("{}/api/resource/1", mock_server.uri());
    let action = TaskAction::Fetch {
        fetch: fetch_params(&url, "DELETE", None),
    };

    // Act
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore)
        .await;

    // Assert
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_fetch_patch_method() {
    // Arrange
    let mock_server = MockServer::start().await;

    Mock::given(method("PATCH"))
        .and(path("/api/resource/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"patched": true})))
        .mount(&mock_server)
        .await;

    let executor = create_test_executor();
    let task_id: Arc<str> = Arc::from("fetch_patch");
    let (bindings, datastore) = empty_context();

    let url = format!("{}/api/resource/1", mock_server.uri());
    let body = serde_json::to_string(&json!({"field": "value"})).unwrap();
    let action = TaskAction::Fetch {
        fetch: fetch_params(&url, "PATCH", Some(body)),
    };

    // Act
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore)
        .await;

    // Assert
    assert!(result.is_ok());
}

// =============================================================================
// VERIFICATION TESTS
// =============================================================================

#[tokio::test]
async fn test_fetch_verifies_call_count() {
    // Arrange
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/counted"))
        .respond_with(ResponseTemplate::new(200).set_body_string("counted"))
        .expect(3) // Expect exactly 3 calls
        .mount(&mock_server)
        .await;

    let executor = create_test_executor();
    let (bindings, datastore) = empty_context();
    let url = format!("{}/api/counted", mock_server.uri());

    // Act - make exactly 3 requests
    for i in 0..3 {
        let task_id: Arc<str> = Arc::from(format!("fetch_{}", i));
        let action = TaskAction::Fetch {
            fetch: fetch_params(&url, "GET", None),
        };
        let result = executor
            .execute(&task_id, &action, &bindings, &datastore)
            .await;
        assert!(result.is_ok());
    }

    // Assert - verification happens automatically when MockServer drops
    // If we didn't make exactly 3 calls, the test would fail on drop
}

#[tokio::test]
async fn test_fetch_multiple_endpoints() {
    // Arrange
    let mock_server = MockServer::start().await;

    // Mount multiple mocks for different endpoints
    Mock::given(method("GET"))
        .and(path("/api/users"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"users": ["alice", "bob"]})))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/products"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"products": ["widget", "gadget"]})),
        )
        .mount(&mock_server)
        .await;

    let executor = create_test_executor();
    let (bindings, datastore) = empty_context();

    // Act & Assert - users endpoint
    let task_id1: Arc<str> = Arc::from("fetch_users");
    let url1 = format!("{}/api/users", mock_server.uri());
    let action1 = TaskAction::Fetch {
        fetch: fetch_params(&url1, "GET", None),
    };
    let result1 = executor
        .execute(&task_id1, &action1, &bindings, &datastore)
        .await;
    assert!(result1.is_ok());
    assert!(result1.unwrap().contains("alice"));

    // Act & Assert - products endpoint
    let task_id2: Arc<str> = Arc::from("fetch_products");
    let url2 = format!("{}/api/products", mock_server.uri());
    let action2 = TaskAction::Fetch {
        fetch: fetch_params(&url2, "GET", None),
    };
    let result2 = executor
        .execute(&task_id2, &action2, &bindings, &datastore)
        .await;
    assert!(result2.is_ok());
    assert!(result2.unwrap().contains("widget"));
}

// =============================================================================
// LARGE RESPONSE TESTS
// =============================================================================

#[tokio::test]
async fn test_fetch_large_json_response() {
    // Arrange
    let mock_server = MockServer::start().await;

    // Generate a large JSON array
    let items: Vec<serde_json::Value> = (0..100)
        .map(|i| {
            json!({
                "id": i,
                "name": format!("item-{}", i),
                "description": "A somewhat long description for testing purposes"
            })
        })
        .collect();

    Mock::given(method("GET"))
        .and(path("/api/large"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items": items})))
        .mount(&mock_server)
        .await;

    let executor = create_test_executor();
    let task_id: Arc<str> = Arc::from("fetch_large");
    let (bindings, datastore) = empty_context();

    let url = format!("{}/api/large", mock_server.uri());
    let action = TaskAction::Fetch {
        fetch: fetch_params(&url, "GET", None),
    };

    // Act
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore)
        .await;

    // Assert
    assert!(result.is_ok());
    let body = result.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(parsed["items"].as_array().unwrap().len(), 100);
}

// =============================================================================
// DELAYED RESPONSE TESTS (using wiremock delay)
// =============================================================================

#[tokio::test]
async fn test_fetch_with_delay_succeeds() {
    use std::time::Duration;

    // Arrange
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/slow"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("slow response")
                .set_delay(Duration::from_millis(100)), // 100ms delay
        )
        .mount(&mock_server)
        .await;

    let executor = create_test_executor();
    let task_id: Arc<str> = Arc::from("fetch_slow");
    let (bindings, datastore) = empty_context();

    let url = format!("{}/api/slow", mock_server.uri());
    let action = TaskAction::Fetch {
        fetch: fetch_params(&url, "GET", None),
    };

    // Act
    let start = std::time::Instant::now();
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore)
        .await;
    let elapsed = start.elapsed();

    // Assert
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "slow response");
    assert!(
        elapsed >= Duration::from_millis(100),
        "Request should take at least 100ms"
    );
}

// =============================================================================
// UNMATCHED REQUEST HANDLING
// =============================================================================

#[tokio::test]
async fn test_fetch_unmatched_path_returns_error() {
    // Arrange
    let mock_server = MockServer::start().await;

    // No mocks mounted - any request should fail
    let executor = create_test_executor();
    let task_id: Arc<str> = Arc::from("fetch_unmatched");
    let (bindings, datastore) = empty_context();

    let url = format!("{}/api/nonexistent", mock_server.uri());
    let action = TaskAction::Fetch {
        fetch: fetch_params(&url, "GET", None),
    };

    // Act
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore)
        .await;

    // Assert - wiremock returns 404 for unmatched requests by default
    assert!(result.is_ok()); // Our executor returns body even for non-2xx
    let body = result.unwrap();
    assert!(body.is_empty() || body.contains("404"));
}
