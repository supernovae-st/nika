//! Test utilities for Nika (only available in test builds)
//!
//! Provides builders, fixtures, and assertions for testing.
//! Centralizes common test patterns to reduce duplication.
//!
//! # Usage
//!
//! ```rust,ignore
//! use nika::test_utils::builders::*;
//! use nika::test_utils::fixtures::*;
//!
//! // Create a mock executor
//! let executor = mock_executor();
//!
//! // Create executor with MCP clients
//! let executor = mock_executor_with_mcp(&["novanet"]);
//!
//! // Create test bindings and datastore
//! let (bindings, store) = test_context();
//!
//! // Get test fixtures
//! let data = weather_data();
//! ```

use crate::binding::ResolvedBindings;
use crate::event::EventLog;
use crate::runtime::TaskExecutor;
use crate::store::DataStore;
use serde_json::{json, Value};

// ═══════════════════════════════════════════════════════════════════════════
// BUILDERS - Factory functions for test objects
// ═══════════════════════════════════════════════════════════════════════════

/// Create a mock executor for testing.
///
/// Uses "mock" provider with no model or MCP configs.
/// The event log is created fresh for isolation.
///
/// # Example
///
/// ```rust,ignore
/// let executor = mock_executor();
/// let result = executor.execute(&task_id, &action, &bindings, &store).await;
/// ```
pub fn mock_executor() -> TaskExecutor {
    let event_log = EventLog::new();
    TaskExecutor::new("mock", None, None, event_log)
}

/// Create a mock executor with a shared event log for event inspection.
///
/// Returns both the executor and the event log so tests can inspect
/// emitted events after execution.
///
/// # Example
///
/// ```rust,ignore
/// let (executor, event_log) = mock_executor_with_events();
/// executor.execute(&task_id, &action, &bindings, &store).await?;
/// let events = event_log.filter_task("my-task");
/// assert!(!events.is_empty());
/// ```
pub fn mock_executor_with_events() -> (TaskExecutor, EventLog) {
    let event_log = EventLog::new();
    let executor = TaskExecutor::new("mock", None, None, event_log.clone());
    (executor, event_log)
}

/// Create a mock executor with MCP clients injected.
///
/// Each server name in the slice gets a mock MCP client.
/// Useful for testing `invoke:` actions without real MCP servers.
///
/// # Example
///
/// ```rust,ignore
/// let executor = mock_executor_with_mcp(&["novanet", "perplexity"]);
/// // Now invoke actions for "novanet" and "perplexity" will use mock clients
/// ```
#[cfg(test)]
pub fn mock_executor_with_mcp(servers: &[&str]) -> TaskExecutor {
    let executor = mock_executor();
    for server in servers {
        executor.inject_mock_mcp_client(server);
    }
    executor
}

/// Create a mock executor with MCP clients and shared event log.
///
/// Combines `mock_executor_with_mcp` and `mock_executor_with_events`.
///
/// # Example
///
/// ```rust,ignore
/// let (executor, event_log) = mock_executor_with_mcp_and_events(&["novanet"]);
/// executor.execute(&task_id, &action, &bindings, &store).await?;
/// let mcp_events = event_log.filter_task("invoke-task");
/// ```
#[cfg(test)]
pub fn mock_executor_with_mcp_and_events(servers: &[&str]) -> (TaskExecutor, EventLog) {
    let event_log = EventLog::new();
    let executor = TaskExecutor::new("mock", None, None, event_log.clone());
    for server in servers {
        executor.inject_mock_mcp_client(server);
    }
    (executor, event_log)
}

/// Create test bindings and datastore pair.
///
/// Returns empty `ResolvedBindings` and empty `DataStore`.
/// This is the most common setup for executor tests.
///
/// # Example
///
/// ```rust,ignore
/// let (bindings, store) = test_context();
/// let result = executor.execute(&task_id, &action, &bindings, &store).await;
/// ```
pub fn test_context() -> (ResolvedBindings, DataStore) {
    (ResolvedBindings::new(), DataStore::new())
}

/// Create test bindings with pre-populated values.
///
/// Accepts a list of (key, value) pairs to populate the bindings.
///
/// # Example
///
/// ```rust,ignore
/// let bindings = bindings_with(&[
///     ("name", json!("Alice")),
///     ("count", json!(42)),
/// ]);
/// ```
pub fn bindings_with(entries: &[(&str, Value)]) -> ResolvedBindings {
    let mut bindings = ResolvedBindings::new();
    for &(key, ref value) in entries {
        bindings.set(key, value.clone());
    }
    bindings
}

/// Create test context with pre-populated bindings.
///
/// Combines `bindings_with` with an empty DataStore.
///
/// # Example
///
/// ```rust,ignore
/// let (bindings, store) = test_context_with(&[
///     ("entity", json!("qr-code")),
///     ("locale", json!("fr-FR")),
/// ]);
/// ```
pub fn test_context_with(entries: &[(&str, Value)]) -> (ResolvedBindings, DataStore) {
    (bindings_with(entries), DataStore::new())
}

// ═══════════════════════════════════════════════════════════════════════════
// FIXTURES - Common test data
// ═══════════════════════════════════════════════════════════════════════════

/// Weather data fixture for tests.
///
/// Returns a JSON object with temp, humidity, and summary.
pub fn weather_data() -> Value {
    json!({
        "temp": 25,
        "humidity": 60,
        "summary": "Sunny"
    })
}

/// Entity context fixture for tests.
///
/// Returns a JSON object with entity, locale, and title.
pub fn entity_context() -> Value {
    json!({
        "entity": "qr-code",
        "locale": "fr-FR",
        "title": "QR Code"
    })
}

/// User profile fixture for tests.
///
/// Returns a JSON object with user info.
pub fn user_profile() -> Value {
    json!({
        "id": "user-123",
        "name": "Test User",
        "email": "test@example.com",
        "role": "admin"
    })
}

/// API response fixture for tests.
///
/// Returns a JSON object simulating an API response.
pub fn api_response() -> Value {
    json!({
        "status": "ok",
        "data": {
            "items": [
                { "id": 1, "name": "Item 1" },
                { "id": 2, "name": "Item 2" }
            ],
            "total": 2
        },
        "timestamp": "2026-02-21T10:00:00Z"
    })
}

/// MCP tool result fixture for tests.
///
/// Returns a JSON object simulating an MCP tool response.
pub fn mcp_tool_result() -> Value {
    json!({
        "entity": "qr-code",
        "locale": "fr-FR",
        "content": {
            "title": "Code QR",
            "description": "Un code-barres bidimensionnel"
        }
    })
}

/// Empty result fixture.
///
/// Returns an empty JSON object.
pub fn empty_result() -> Value {
    json!({})
}

/// Nested data fixture for JSONPath tests.
///
/// Returns deeply nested JSON for testing path extraction.
pub fn nested_data() -> Value {
    json!({
        "level1": {
            "level2": {
                "level3": {
                    "value": "deep-value"
                }
            },
            "array": [
                { "id": "a" },
                { "id": "b" },
                { "id": "c" }
            ]
        }
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ───────────────────────────────────────────────────────────────
    // BUILDER TESTS
    // ───────────────────────────────────────────────────────────────

    #[test]
    fn test_mock_executor_creates_valid_executor() {
        let executor = mock_executor();
        // Executor should be cloneable (Clone impl requirement)
        let _cloned = executor.clone();
    }

    #[test]
    fn test_mock_executor_with_events_returns_shared_log() {
        let (executor, event_log) = mock_executor_with_events();
        // Should be able to clone both
        let _exec_clone = executor.clone();
        let _log_clone = event_log.clone();
        // Event log should start empty
        assert!(event_log.events().is_empty());
    }

    #[test]
    fn test_mock_executor_with_mcp_injects_clients() {
        let executor = mock_executor_with_mcp(&["novanet", "perplexity"]);
        // Executor should be usable (we can't easily verify MCP clients without executing)
        let _cloned = executor.clone();
    }

    #[test]
    fn test_mock_executor_with_mcp_and_events() {
        let (executor, event_log) = mock_executor_with_mcp_and_events(&["novanet"]);
        let _exec_clone = executor.clone();
        assert!(event_log.events().is_empty());
    }

    #[test]
    fn test_test_context_returns_empty_bindings_and_store() {
        let (bindings, store) = test_context();
        // Bindings should be empty
        assert!(bindings.get("nonexistent").is_none());
        // Store should be empty
        assert!(store.get("nonexistent").is_none());
    }

    #[test]
    fn test_bindings_with_populates_entries() {
        let bindings = bindings_with(&[("name", json!("Alice")), ("age", json!(30))]);
        assert_eq!(bindings.get("name").unwrap(), &json!("Alice"));
        assert_eq!(bindings.get("age").unwrap(), &json!(30));
    }

    #[test]
    fn test_bindings_with_empty_returns_empty() {
        let bindings = bindings_with(&[]);
        assert!(bindings.get("anything").is_none());
    }

    #[test]
    fn test_test_context_with_populates_bindings() {
        let (bindings, store) = test_context_with(&[("key", json!("value"))]);
        assert_eq!(bindings.get("key").unwrap(), &json!("value"));
        assert!(store.get("nonexistent").is_none());
    }

    // ───────────────────────────────────────────────────────────────
    // FIXTURE TESTS
    // ───────────────────────────────────────────────────────────────

    #[test]
    fn test_weather_data_has_expected_fields() {
        let data = weather_data();
        assert_eq!(data["temp"], 25);
        assert_eq!(data["humidity"], 60);
        assert_eq!(data["summary"], "Sunny");
    }

    #[test]
    fn test_entity_context_has_expected_fields() {
        let ctx = entity_context();
        assert_eq!(ctx["entity"], "qr-code");
        assert_eq!(ctx["locale"], "fr-FR");
        assert_eq!(ctx["title"], "QR Code");
    }

    #[test]
    fn test_user_profile_has_expected_fields() {
        let user = user_profile();
        assert_eq!(user["id"], "user-123");
        assert_eq!(user["name"], "Test User");
        assert_eq!(user["email"], "test@example.com");
        assert_eq!(user["role"], "admin");
    }

    #[test]
    fn test_api_response_has_expected_structure() {
        let resp = api_response();
        assert_eq!(resp["status"], "ok");
        assert!(resp["data"]["items"].is_array());
        assert_eq!(resp["data"]["total"], 2);
    }

    #[test]
    fn test_mcp_tool_result_has_expected_fields() {
        let result = mcp_tool_result();
        assert_eq!(result["entity"], "qr-code");
        assert_eq!(result["locale"], "fr-FR");
        assert!(result["content"].is_object());
    }

    #[test]
    fn test_empty_result_is_empty_object() {
        let result = empty_result();
        assert!(result.is_object());
        assert!(result.as_object().unwrap().is_empty());
    }

    #[test]
    fn test_nested_data_supports_deep_access() {
        let data = nested_data();
        assert_eq!(data["level1"]["level2"]["level3"]["value"], "deep-value");
        assert!(data["level1"]["array"].is_array());
        assert_eq!(data["level1"]["array"][0]["id"], "a");
    }
}
