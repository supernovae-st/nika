//! WIRING-3: BuiltinRouter <-> Executor
//!
//! Verifies: `nika:*` prefix routes to builtin tools, others to MCP.
//! Run after: v0.9.3 (Builtin Tools)

use nika::runtime::builtin::BuiltinToolRouter;

/// Test that nika: prefix is correctly recognized.
#[test]
fn wiring_checkpoint_3_is_builtin() {
    // Builtin tools have nika: prefix
    assert!(BuiltinToolRouter::is_builtin("nika:sleep"));
    assert!(BuiltinToolRouter::is_builtin("nika:log"));
    assert!(BuiltinToolRouter::is_builtin("nika:emit"));
    assert!(BuiltinToolRouter::is_builtin("nika:assert"));
    assert!(BuiltinToolRouter::is_builtin("nika:prompt"));
    assert!(BuiltinToolRouter::is_builtin("nika:run"));

    // Non-nika tools are NOT builtin
    assert!(!BuiltinToolRouter::is_builtin("novanet:describe"));
    assert!(!BuiltinToolRouter::is_builtin("perplexity:search"));
    assert!(!BuiltinToolRouter::is_builtin("sleep")); // No prefix
    assert!(!BuiltinToolRouter::is_builtin(""));
}

/// Test that all 6 builtin tools are registered.
#[test]
fn wiring_checkpoint_3_all_tools_registered() {
    let router = BuiltinToolRouter::new();

    // All 6 tools should be registered
    assert!(router.has_tool("sleep"), "sleep tool should be registered");
    assert!(router.has_tool("log"), "log tool should be registered");
    assert!(router.has_tool("emit"), "emit tool should be registered");
    assert!(router.has_tool("assert"), "assert tool should be registered");
    assert!(router.has_tool("prompt"), "prompt tool should be registered");
    assert!(router.has_tool("run"), "run tool should be registered");

    // Verify count
    let tool_names = router.tool_names();
    assert_eq!(tool_names.len(), 6, "Should have exactly 6 builtin tools");
}

/// Test that extract_name correctly strips nika: prefix.
#[test]
fn wiring_checkpoint_3_extract_name() {
    assert_eq!(BuiltinToolRouter::extract_name("nika:sleep"), Some("sleep"));
    assert_eq!(BuiltinToolRouter::extract_name("nika:log"), Some("log"));
    assert_eq!(BuiltinToolRouter::extract_name("nika:emit"), Some("emit"));
    assert_eq!(
        BuiltinToolRouter::extract_name("nika:assert"),
        Some("assert")
    );
    assert_eq!(
        BuiltinToolRouter::extract_name("nika:prompt"),
        Some("prompt")
    );
    assert_eq!(BuiltinToolRouter::extract_name("nika:run"), Some("run"));

    // Non-nika tools return None
    assert_eq!(BuiltinToolRouter::extract_name("novanet:describe"), None);
    assert_eq!(BuiltinToolRouter::extract_name("sleep"), None);
    assert_eq!(BuiltinToolRouter::extract_name(""), None);
}

/// Test that dispatch works for nika:sleep.
#[tokio::test]
async fn wiring_checkpoint_3_dispatch_sleep() {
    let router = BuiltinToolRouter::new();

    // Dispatch nika:sleep with 1ms duration
    let result = router
        .dispatch("nika:sleep", r#"{"duration": "1ms"}"#.to_string())
        .await;

    assert!(result.is_ok(), "nika:sleep dispatch should succeed");

    let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
    // Response is {"slept_for_ms": N}
    assert!(response["slept_for_ms"].is_number(), "Should have slept_for_ms");
}

/// Test that dispatch works for nika:log.
#[tokio::test]
async fn wiring_checkpoint_3_dispatch_log() {
    let router = BuiltinToolRouter::new();

    // Dispatch nika:log at info level
    let result = router
        .dispatch(
            "nika:log",
            r#"{"level": "info", "message": "WIRING test"}"#.to_string(),
        )
        .await;

    assert!(result.is_ok(), "nika:log dispatch should succeed");

    let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
    assert_eq!(response["logged"], true);
    assert_eq!(response["level"], "info");
    assert_eq!(response["message"], "WIRING test");
}

/// Test that dispatch works for nika:emit.
#[tokio::test]
async fn wiring_checkpoint_3_dispatch_emit() {
    let router = BuiltinToolRouter::new();

    // Dispatch nika:emit - uses "name" not "event", "payload" not "data"
    let result = router
        .dispatch(
            "nika:emit",
            r#"{"name": "test_event", "payload": {"key": "value"}}"#.to_string(),
        )
        .await;

    assert!(result.is_ok(), "nika:emit dispatch should succeed");

    let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
    assert_eq!(response["emitted"], true);
    assert_eq!(response["name"], "test_event");
}

/// Test that dispatch works for nika:assert (passing condition).
#[tokio::test]
async fn wiring_checkpoint_3_dispatch_assert_pass() {
    let router = BuiltinToolRouter::new();

    // Dispatch nika:assert with passing condition - condition is boolean, not string
    let result = router
        .dispatch(
            "nika:assert",
            r#"{"condition": true, "message": "Should pass"}"#.to_string(),
        )
        .await;

    assert!(result.is_ok(), "nika:assert with true should succeed");

    let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
    assert_eq!(response["passed"], true);
}

/// Test that dispatch works for nika:assert (failing condition).
#[tokio::test]
async fn wiring_checkpoint_3_dispatch_assert_fail() {
    let router = BuiltinToolRouter::new();

    // Dispatch nika:assert with failing condition - condition is boolean, not string
    let result = router
        .dispatch(
            "nika:assert",
            r#"{"condition": false, "message": "Should fail"}"#.to_string(),
        )
        .await;

    assert!(result.is_err(), "nika:assert with false should fail");
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("Should fail"),
        "Error should contain the assert message"
    );
}

/// Test that dispatch fails for non-nika tools.
#[tokio::test]
async fn wiring_checkpoint_3_dispatch_non_builtin_fails() {
    let router = BuiltinToolRouter::new();

    // Dispatch a non-nika tool should fail
    let result = router.dispatch("novanet:describe", r#"{}"#.to_string()).await;

    assert!(result.is_err(), "Non-nika tool dispatch should fail");
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("Not a builtin tool"),
        "Error should indicate not a builtin tool"
    );
}

/// Test that dispatch fails for unknown nika tool.
#[tokio::test]
async fn wiring_checkpoint_3_dispatch_unknown_builtin_fails() {
    let router = BuiltinToolRouter::new();

    // Dispatch an unknown nika: tool should fail
    let result = router.dispatch("nika:unknown", r#"{}"#.to_string()).await;

    assert!(result.is_err(), "Unknown nika tool dispatch should fail");
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("Unknown builtin tool"),
        "Error should indicate unknown builtin tool"
    );
}

/// Test nika:run validation (workflow path).
#[tokio::test]
async fn wiring_checkpoint_3_dispatch_run_validation() {
    let router = BuiltinToolRouter::new();

    // Valid workflow path
    let result = router
        .dispatch(
            "nika:run",
            r#"{"workflow": "path/to/test.nika.yaml"}"#.to_string(),
        )
        .await;
    assert!(result.is_ok(), "nika:run with valid path should succeed");

    // Invalid extension should fail
    let result = router
        .dispatch(
            "nika:run",
            r#"{"workflow": "test.yaml"}"#.to_string(),
        )
        .await;
    assert!(
        result.is_err(),
        "nika:run with invalid extension should fail"
    );

    // Empty path should fail
    let result = router
        .dispatch("nika:run", r#"{"workflow": ""}"#.to_string())
        .await;
    assert!(result.is_err(), "nika:run with empty path should fail");
}

/// Test that nika:prompt validates required fields.
#[tokio::test]
async fn wiring_checkpoint_3_dispatch_prompt_validation() {
    let router = BuiltinToolRouter::new();

    // Missing message should fail
    let result = router.dispatch("nika:prompt", r#"{}"#.to_string()).await;
    assert!(result.is_err(), "nika:prompt without message should fail");

    // Note: Full prompt flow requires HITL integration, which is tested elsewhere
}

/// Test router Default implementation.
#[test]
fn wiring_checkpoint_3_router_default() {
    let router = BuiltinToolRouter::default();

    // Default should have all tools
    assert_eq!(router.tool_names().len(), 6);
    assert!(router.has_tool("sleep"));
    assert!(router.has_tool("log"));
}
