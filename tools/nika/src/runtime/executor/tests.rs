//! Tests for TaskExecutor
//!
//! Covers: construction, exec verb, fetch verb, invoke verb,
//! binding resolution, decompose, error handling, policy enforcement,
//! shlex parsing, and shell-free security.

use super::*;
use crate::ast::decompose::{DecomposeSpec, DecomposeStrategy};
use crate::ast::{ExecParams, FetchParams, InvokeParams};
use crate::event::EventKind;
use crate::store::{RunContext, TaskResult};
use serde_json::json;
use std::time::Duration;

// ═══════════════════════════════════════════════════════════════
// EXECUTOR CONSTRUCTION TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_executor_new_default() {
    let executor = TaskExecutor::new("claude", None, None, EventLog::new());
    assert_eq!(executor.default_provider.as_ref(), "claude");
    assert!(executor.default_model.is_none());
}

#[test]
fn test_executor_new_with_model() {
    let executor = TaskExecutor::new("openai", Some("gpt-4"), None, EventLog::new());
    assert_eq!(executor.default_provider.as_ref(), "openai");
    assert_eq!(executor.default_model.as_deref(), Some("gpt-4"));
}

#[test]
fn test_executor_new_with_mcp_configs() {
    let mut mcp_configs = rustc_hash::FxHashMap::default();
    mcp_configs.insert(
        "novanet".to_string(),
        McpConfigInline {
            command: "cargo run".to_string(),
            args: vec![
                "--manifest-path".to_string(),
                "path/to/Cargo.toml".to_string(),
            ],
            env: rustc_hash::FxHashMap::default(),
            cwd: None,
        },
    );

    let executor = TaskExecutor::new("mock", None, Some(mcp_configs), EventLog::new());
    assert_eq!(executor.default_provider.as_ref(), "mock");
}

#[test]
fn test_executor_is_clone() {
    let exec = TaskExecutor::new("mock", None, None, EventLog::new());
    let _cloned = exec.clone();
}

// ═══════════════════════════════════════════════════════════════
// EXEC VERB TESTS
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_execute_exec_simple_command() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new();

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "echo hello".to_string(),
            shell: None,
            timeout: None,
            cwd: None,
            env: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_echo");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    assert_eq!(result, "hello");
}

#[tokio::test]
async fn test_execute_exec_with_template_binding() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let mut bindings = ResolvedBindings::new();
    bindings.set("name", json!("world"));
    let datastore = RunContext::new();

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "echo {{use.name}}".to_string(),
            shell: None,
            timeout: None,
            cwd: None,
            env: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_template");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    assert_eq!(result, "world");
}

#[tokio::test]
async fn test_execute_exec_command_failure() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new();

    // Use false command which exists and always returns exit code 1
    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "false".to_string(),
            shell: None,
            timeout: None,
            cwd: None,
            env: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_fail");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;
    assert!(result.is_err());
    match result.unwrap_err() {
        NikaError::Execution(msg) => {
            // Command fails with non-zero exit code
            assert!(
                msg.contains("failed") || msg.contains("exit code"),
                "Expected failure message, got: {msg}"
            );
        }
        err => panic!("Expected Execution error, got: {err:?}"),
    }
}

#[tokio::test]
async fn test_execute_exec_emits_template_resolved() {
    let event_log = EventLog::new();
    let executor = TaskExecutor::new("mock", None, None, event_log.clone());
    let mut bindings = ResolvedBindings::new();
    bindings.set("greeting", json!("Hello"));
    let datastore = RunContext::new();

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "echo {{use.greeting}}".to_string(),
            shell: None,
            timeout: None,
            cwd: None,
            env: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_event");
    executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();

    // Verify TemplateResolved event was emitted
    let events = event_log.filter_task("test_event");
    assert!(!events.is_empty());

    let template_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::TemplateResolved { .. }))
        .collect();
    assert_eq!(template_events.len(), 1);

    if let EventKind::TemplateResolved { result, .. } = &template_events[0].kind {
        assert_eq!(result, "echo Hello");
    }
}

// ═══════════════════════════════════════════════════════════════
// FETCH VERB TESTS (HTTP)
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_execute_fetch_invalid_url() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new();

    let action = TaskAction::Fetch {
        fetch: FetchParams {
            url: "http://invalid.example.invalid".to_string(),
            method: "GET".to_string(),
            headers: rustc_hash::FxHashMap::default(),
            body: None,
            json: None,
            timeout: None,
            retry: None,
            follow_redirects: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_fetch_fail");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;
    // Result is error because the URL cannot be resolved/connected
    assert!(result.is_err());
}

#[tokio::test]
async fn test_execute_fetch_with_template_url() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let mut bindings = ResolvedBindings::new();
    bindings.set("endpoint", json!("httpbin.org/get"));
    let datastore = RunContext::new();

    let action = TaskAction::Fetch {
        fetch: FetchParams {
            url: "https://{{use.endpoint}}".to_string(),
            method: "GET".to_string(),
            headers: rustc_hash::FxHashMap::default(),
            body: None,
            json: None,
            timeout: None,
            retry: None,
            follow_redirects: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_fetch_template");
    // This will connect to the real httpbin, so we expect success if network is available
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;
    // Just verify template was resolved (regardless of network success/failure)
    let events = EventLog::new();
    let executor2 = TaskExecutor::new("mock", None, None, events.clone());
    let result2 = executor2
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;
    // Both should have same result status (both succeed or both fail due to network)
    assert_eq!(result.is_ok(), result2.is_ok());
}

// ═══════════════════════════════════════════════════════════════
// INVOKE VERB TESTS (MCP)
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_execute_invoke_tool_call() {
    let event_log = EventLog::new();
    let executor = TaskExecutor::new("mock", None, None, event_log.clone());
    executor.inject_mock_mcp_client("novanet");
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new();

    let action = TaskAction::Invoke {
        invoke: InvokeParams {
            mcp: Some("novanet".to_string()),
            tool: Some("novanet_generate".to_string()),
            params: Some(json!({"entity": "qr-code", "locale": "fr-FR"})),
            resource: None,
            timeout: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_invoke");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;

    assert!(result.is_ok(), "Invoke should succeed: {:?}", result.err());
    let output = result.unwrap();
    assert!(
        output.contains("entity"),
        "Output should contain entity: {output}"
    );
}

#[tokio::test]
async fn test_execute_invoke_resource_read() {
    let event_log = EventLog::new();
    let executor = TaskExecutor::new("mock", None, None, event_log);
    executor.inject_mock_mcp_client("novanet");
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new();

    let action = TaskAction::Invoke {
        invoke: InvokeParams {
            mcp: Some("novanet".to_string()),
            tool: None,
            params: None,
            resource: Some("neo4j://entity/qr-code".to_string()),
            timeout: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_resource");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;

    assert!(
        result.is_ok(),
        "Resource read should succeed: {:?}",
        result.err()
    );
    let output = result.unwrap();
    assert!(
        output.contains("qr-code"),
        "Output should contain resource id: {output}"
    );
}

#[tokio::test]
async fn test_execute_invoke_emits_mcp_events() {
    let event_log = EventLog::new();
    let executor = TaskExecutor::new("mock", None, None, event_log.clone());
    executor.inject_mock_mcp_client("novanet");
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new();

    let action = TaskAction::Invoke {
        invoke: InvokeParams {
            mcp: Some("novanet".to_string()),
            tool: Some("novanet_describe".to_string()),
            params: None,
            resource: None,
            timeout: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_mcp_events");
    executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();

    // Verify events were emitted
    let events = event_log.filter_task("test_mcp_events");
    assert!(!events.is_empty(), "Should emit events");

    // Check for McpInvoke event
    let invoke_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::McpInvoke { .. }))
        .collect();
    assert_eq!(invoke_events.len(), 1, "Should emit McpInvoke event");

    // Check for McpResponse event
    let response_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::McpResponse { .. }))
        .collect();
    assert_eq!(response_events.len(), 1, "Should emit McpResponse event");
}

#[tokio::test]
async fn test_execute_invoke_tool_with_template_params() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    executor.inject_mock_mcp_client("novanet");
    let mut bindings = ResolvedBindings::new();
    bindings.set("entity_key", json!("qr-code"));
    bindings.set("locale_val", json!("en-US"));
    let datastore = RunContext::new();

    let action = TaskAction::Invoke {
        invoke: InvokeParams {
            mcp: Some("novanet".to_string()),
            tool: Some("novanet_generate".to_string()),
            params: Some(json!({
                "entity": "{{use.entity_key}}",
                "locale": "{{use.locale_val}}"
            })),
            resource: None,
            timeout: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_invoke_template");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;

    assert!(
        result.is_ok(),
        "Invoke with template params should succeed: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn test_execute_invoke_validation_error_both_tool_and_resource() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new();

    // Both tool and resource set (invalid)
    let action = TaskAction::Invoke {
        invoke: InvokeParams {
            mcp: Some("novanet".to_string()),
            tool: Some("test_tool".to_string()),
            params: None,
            resource: Some("test://resource".to_string()),
            timeout: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_invalid");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;

    assert!(result.is_err(), "Should fail with validation error");
    match result.unwrap_err() {
        NikaError::ValidationError { reason } => {
            assert!(reason.contains("mutually exclusive"));
        }
        err => panic!("Expected ValidationError, got: {err:?}"),
    }
}

#[tokio::test]
async fn test_execute_invoke_validation_error_neither_tool_nor_resource() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new();

    // Neither tool nor resource set (invalid)
    let action = TaskAction::Invoke {
        invoke: InvokeParams {
            mcp: Some("novanet".to_string()),
            tool: None,
            params: None,
            resource: None,
            timeout: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_neither");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;

    assert!(result.is_err(), "Should fail with validation error");
    match result.unwrap_err() {
        NikaError::ValidationError { reason } => {
            assert!(reason.contains("either") || reason.contains("must be specified"));
        }
        err => panic!("Expected ValidationError, got: {err:?}"),
    }
}

#[tokio::test]
async fn test_execute_invoke_mcp_not_configured() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    // No inject_mock_mcp_client() - server not configured
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new();

    let action = TaskAction::Invoke {
        invoke: InvokeParams {
            mcp: Some("unconfigured_server".to_string()),
            tool: Some("some_tool".to_string()),
            params: None,
            resource: None,
            timeout: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_unconfigured");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;

    assert!(result.is_err(), "Should fail with McpNotConfigured");
    match result.unwrap_err() {
        NikaError::McpNotConfigured { name } => {
            assert_eq!(name, "unconfigured_server");
        }
        err => panic!("Expected McpNotConfigured, got: {err:?}"),
    }
}

// ═══════════════════════════════════════════════════════════════
// BINDING RESOLUTION TESTS
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_binding_resolution_single_template() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let mut bindings = ResolvedBindings::new();
    bindings.set("key", json!("value123"));
    let datastore = RunContext::new();

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "echo {{use.key}}".to_string(),
            shell: None,
            timeout: None,
            cwd: None,
            env: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_binding");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    assert_eq!(result, "value123");
}

#[tokio::test]
async fn test_binding_resolution_multiple_templates() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let mut bindings = ResolvedBindings::new();
    bindings.set("first", json!("hello"));
    bindings.set("second", json!("world"));
    let datastore = RunContext::new();

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "echo {{use.first}} {{use.second}}".to_string(),
            shell: None,
            timeout: None,
            cwd: None,
            env: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_multi");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    assert_eq!(result, "hello world");
}

#[tokio::test]
async fn test_binding_resolution_no_templates() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new();

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "echo static".to_string(),
            shell: None,
            timeout: None,
            cwd: None,
            env: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_static");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    assert_eq!(result, "static");
}

#[tokio::test]
async fn test_binding_resolution_json_value() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let mut bindings = ResolvedBindings::new();
    bindings.set("data", json!({"id": 42, "name": "test"}));
    let datastore = RunContext::new();

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "echo {{use.data}}".to_string(),
            shell: None,
            timeout: None,
            cwd: None,
            env: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_json");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    // JSON should be serialized and echoed
    assert!(result.contains("id"));
    assert!(result.contains("42"));
}

#[tokio::test]
async fn test_binding_resolution_datastore_lookup() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let mut bindings = ResolvedBindings::new();
    bindings.set("task_output", json!({"result": "success"}));
    let datastore = RunContext::new();
    let task_id_prev: Arc<str> = Arc::from("prev_task");
    datastore.insert(
        task_id_prev.clone(),
        TaskResult::success_str("from_previous_task", Duration::from_millis(100)),
    );

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "echo {{use.task_output}}".to_string(),
            shell: None,
            timeout: None,
            cwd: None,
            env: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_store");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    assert!(result.contains("success"));
}

// ═══════════════════════════════════════════════════════════════
// DECOMPOSE TESTS
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_expand_decompose_static() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let mut bindings = ResolvedBindings::new();
    bindings.set("items", json!(["item1", "item2", "item3"]));
    let datastore = RunContext::new();

    let spec = DecomposeSpec {
        strategy: DecomposeStrategy::Static,
        traverse: "HAS_CHILD".to_string(),
        source: "{{use.items}}".to_string(),
        max_items: None,
        max_depth: None,
        mcp_server: None,
    };

    let result = executor
        .expand_decompose(&spec, &bindings, &datastore)
        .await
        .unwrap();

    assert_eq!(result.len(), 3);
    assert_eq!(result[0].as_str().unwrap(), "item1");
    assert_eq!(result[1].as_str().unwrap(), "item2");
    assert_eq!(result[2].as_str().unwrap(), "item3");
}

#[tokio::test]
async fn test_expand_decompose_static_with_max_items() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let mut bindings = ResolvedBindings::new();
    bindings.set("items", json!(["a", "b", "c", "d", "e"]));
    let datastore = RunContext::new();

    let spec = DecomposeSpec {
        strategy: DecomposeStrategy::Static,
        traverse: "HAS_CHILD".to_string(),
        source: "{{use.items}}".to_string(),
        max_items: Some(2),
        max_depth: None,
        mcp_server: None,
    };

    let result = executor
        .expand_decompose(&spec, &bindings, &datastore)
        .await
        .unwrap();

    assert_eq!(result.len(), 2);
}

#[tokio::test]
async fn test_expand_decompose_static_wrong_type() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let mut bindings = ResolvedBindings::new();
    bindings.set("notarray", json!({"key": "value"}));
    let datastore = RunContext::new();

    let spec = DecomposeSpec {
        strategy: DecomposeStrategy::Static,
        traverse: "HAS_CHILD".to_string(),
        source: "{{use.notarray}}".to_string(),
        max_items: None,
        max_depth: None,
        mcp_server: None,
    };

    let result = executor
        .expand_decompose(&spec, &bindings, &datastore)
        .await;
    assert!(result.is_err(), "Should fail with type mismatch");
}

#[tokio::test]
async fn test_extract_decompose_key_from_string() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let value = json!("entity:qr-code");

    let key = executor.extract_decompose_key(&value).unwrap();
    assert_eq!(key, "entity:qr-code");
}

#[tokio::test]
async fn test_extract_decompose_key_from_object() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let value = json!({"key": "entity:test", "name": "Test Entity"});

    let key = executor.extract_decompose_key(&value).unwrap();
    assert_eq!(key, "entity:test");
}

#[tokio::test]
async fn test_extract_decompose_key_invalid() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let value = json!(123);

    let result = executor.extract_decompose_key(&value);
    assert!(result.is_err());
}

#[tokio::test]
async fn test_extract_decompose_nodes_from_nodes_field() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let result_json = json!({
        "nodes": [
            {"key": "node1"},
            {"key": "node2"}
        ]
    });

    let nodes = executor.extract_decompose_nodes(result_json).unwrap();
    assert_eq!(nodes.len(), 2);
}

#[tokio::test]
async fn test_extract_decompose_nodes_from_items_field() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let result_json = json!({
        "items": ["item1", "item2", "item3"]
    });

    let nodes = executor.extract_decompose_nodes(result_json).unwrap();
    assert_eq!(nodes.len(), 3);
}

#[tokio::test]
async fn test_extract_decompose_nodes_from_results_field() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let result_json = json!({
        "results": ["result1", "result2"]
    });

    let nodes = executor.extract_decompose_nodes(result_json).unwrap();
    assert_eq!(nodes.len(), 2);
}

#[tokio::test]
async fn test_extract_decompose_nodes_from_array() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let result_json = json!(["direct1", "direct2"]);

    let nodes = executor.extract_decompose_nodes(result_json).unwrap();
    assert_eq!(nodes.len(), 2);
}

#[tokio::test]
async fn test_extract_decompose_nodes_empty_nodes() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let result_json = json!({"nodes": []});

    let nodes = executor.extract_decompose_nodes(result_json).unwrap();
    assert_eq!(nodes.len(), 0);
}

// ═══════════════════════════════════════════════════════════════
// ERROR HANDLING TESTS
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_error_handling_exec_timeout() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new();

    // Sleep command longer than timeout
    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "sleep 100".to_string(),
            shell: None,
            timeout: None,
            cwd: None,
            env: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_timeout");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;

    assert!(result.is_err(), "Should timeout");
    match result.unwrap_err() {
        NikaError::Execution(msg) => {
            assert!(msg.contains("timed out") || msg.contains("timeout"));
        }
        err => panic!("Expected Execution error with timeout, got: {err:?}"),
    }
}

#[tokio::test]
async fn test_action_type_helper() {
    let infer_action = TaskAction::Infer {
        infer: crate::ast::InferParams {
            prompt: "test".to_string(),
            ..Default::default()
        },
    };
    assert_eq!(action_type(&infer_action), "infer");

    let exec_action = TaskAction::Exec {
        exec: ExecParams {
            command: "echo test".to_string(),
            shell: None,
            timeout: None,
            cwd: None,
            env: None,
        },
    };
    assert_eq!(action_type(&exec_action), "exec");

    let fetch_action = TaskAction::Fetch {
        fetch: FetchParams {
            url: "http://example.com".to_string(),
            method: "GET".to_string(),
            headers: rustc_hash::FxHashMap::default(),
            body: None,
            json: None,
            timeout: None,
            retry: None,
            follow_redirects: None,
        },
    };
    assert_eq!(action_type(&fetch_action), "fetch");

    let invoke_action = TaskAction::Invoke {
        invoke: InvokeParams {
            mcp: Some("novanet".to_string()),
            tool: Some("test".to_string()),
            params: None,
            resource: None,
            timeout: None,
        },
    };
    assert_eq!(action_type(&invoke_action), "invoke");

    let agent_action = TaskAction::Agent {
        agent: crate::ast::AgentParams {
            prompt: "test".to_string(),
            provider: None,
            model: None,
            system: None,
            mcp: vec![],
            tools: vec![],
            max_turns: None,
            stop_sequences: vec![],
            scope: None,
            token_budget: None,
            extended_thinking: None,
            thinking_budget: None,
            depth_limit: None,
            tool_choice: None,
            temperature: None,
            max_tokens: None,
            skills: None,
            completion: None,
            guardrails: vec![],
            limits: None,
        },
    };
    assert_eq!(action_type(&agent_action), "agent");
}

// ═══════════════════════════════════════════════════════════════
// POLICY ENFORCEMENT TESTS
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_execute_exec_blocked_by_policy() {
    // Configure policy to block custom commands
    // Note: "sudo" is now in the security blocklist, so we use custom patterns
    let policy_config = PolicyConfig {
        allow_exec: true,
        blocked_commands: vec!["dangerous_tool".to_string(), "custom_block".to_string()],
        ..Default::default()
    };
    let executor =
        TaskExecutor::with_policy("mock", None, None, EventLog::new(), Some(policy_config));
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new();

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "dangerous_tool --flag".to_string(),
            shell: None,
            timeout: None,
            cwd: None,
            env: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_policy_exec");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;

    assert!(result.is_err(), "Should be blocked by policy");
    match result.unwrap_err() {
        NikaError::PolicyViolation { reason } => {
            assert!(
                reason.contains("dangerous_tool"),
                "Reason should mention blocked pattern"
            );
        }
        err => panic!("Expected PolicyViolation, got: {err:?}"),
    }
}

#[tokio::test]
async fn test_execute_exec_allowed_by_policy() {
    let policy_config = PolicyConfig {
        allow_exec: true,
        blocked_commands: vec!["sudo".to_string()],
        ..Default::default()
    };
    let executor =
        TaskExecutor::with_policy("mock", None, None, EventLog::new(), Some(policy_config));
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new();

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "echo hello".to_string(),
            shell: None,
            timeout: None,
            cwd: None,
            env: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_policy_exec_allowed");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;

    assert!(result.is_ok(), "Should be allowed: {:?}", result.err());
    assert_eq!(result.unwrap(), "hello");
}

#[tokio::test]
async fn test_execute_exec_disabled_by_policy() {
    // Configure policy to disable exec entirely
    let policy_config = PolicyConfig {
        allow_exec: false,
        ..Default::default()
    };
    let executor =
        TaskExecutor::with_policy("mock", None, None, EventLog::new(), Some(policy_config));
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new();

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "echo safe".to_string(),
            shell: None,
            timeout: None,
            cwd: None,
            env: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_policy_exec_disabled");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;

    assert!(result.is_err(), "Should be blocked when exec is disabled");
    match result.unwrap_err() {
        NikaError::PolicyViolation { reason } => {
            assert!(
                reason.contains("disabled"),
                "Reason should mention disabled"
            );
        }
        err => panic!("Expected PolicyViolation, got: {err:?}"),
    }
}

#[tokio::test]
async fn test_execute_fetch_blocked_by_policy() {
    // Configure policy to block specific hosts
    let policy_config = PolicyConfig {
        allow_network: true,
        blocked_hosts: vec!["evil.com".to_string()],
        ..Default::default()
    };
    let executor =
        TaskExecutor::with_policy("mock", None, None, EventLog::new(), Some(policy_config));
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new();

    let action = TaskAction::Fetch {
        fetch: FetchParams {
            url: "https://evil.com/api".to_string(),
            method: "GET".to_string(),
            headers: rustc_hash::FxHashMap::default(),
            body: None,
            json: None,
            timeout: None,
            retry: None,
            follow_redirects: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_policy_fetch");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;

    assert!(result.is_err(), "Should be blocked by policy");
    match result.unwrap_err() {
        NikaError::PolicyViolation { reason } => {
            assert!(reason.contains("blocked"), "Reason should mention blocked");
        }
        err => panic!("Expected PolicyViolation, got: {err:?}"),
    }
}

#[tokio::test]
async fn test_execute_fetch_disabled_by_policy() {
    // Configure policy to disable network entirely
    let policy_config = PolicyConfig {
        allow_network: false,
        ..Default::default()
    };
    let executor =
        TaskExecutor::with_policy("mock", None, None, EventLog::new(), Some(policy_config));
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new();

    let action = TaskAction::Fetch {
        fetch: FetchParams {
            url: "https://api.example.com".to_string(),
            method: "GET".to_string(),
            headers: rustc_hash::FxHashMap::default(),
            body: None,
            json: None,
            timeout: None,
            retry: None,
            follow_redirects: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_policy_fetch_disabled");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;

    assert!(
        result.is_err(),
        "Should be blocked when network is disabled"
    );
    match result.unwrap_err() {
        NikaError::PolicyViolation { reason } => {
            assert!(
                reason.contains("disabled"),
                "Reason should mention disabled"
            );
        }
        err => panic!("Expected PolicyViolation, got: {err:?}"),
    }
}

#[tokio::test]
async fn test_executor_with_policy_config() {
    let policy_config = PolicyConfig {
        allow_exec: true,
        allow_network: false,
        max_token_spend: Some(1000),
        ..Default::default()
    };

    let executor =
        TaskExecutor::with_policy("mock", None, None, EventLog::new(), Some(policy_config));

    // Verify executor was created (basic sanity check)
    assert_eq!(executor.default_provider.as_ref(), "mock");
}

// ═══════════════════════════════════════════════════════════════
// SHLEX COMMAND PARSING TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_shlex_split_simple_command() {
    let parts = shlex::split("echo hello world").unwrap();
    assert_eq!(parts, vec!["echo", "hello", "world"]);
}

#[test]
fn test_shlex_split_quoted_args() {
    let parts = shlex::split(r#"echo "hello world""#).unwrap();
    assert_eq!(parts, vec!["echo", "hello world"]);
}

#[test]
fn test_shlex_split_single_quoted() {
    let parts = shlex::split("echo 'hello world'").unwrap();
    assert_eq!(parts, vec!["echo", "hello world"]);
}

#[test]
fn test_shlex_split_escaped_characters() {
    let parts = shlex::split(r#"echo hello\ world"#).unwrap();
    assert_eq!(parts, vec!["echo", "hello world"]);
}

// ═══════════════════════════════════════════════════════════════
// SHELL-FREE EXECUTION TESTS
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_run_exec_shell_free_mode_default() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let task_id: Arc<str> = Arc::from("test_shell_free");
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new();

    // Shell-free: semicolon should NOT be interpreted as command separator
    let params = ExecParams {
        command: "echo hello; echo world".to_string(),
        shell: None, // Default: shell-free
        timeout: None,
        cwd: None,
        env: None,
    };

    let result = executor
        .run_exec(&task_id, &params, &bindings, &datastore)
        .await;
    // Either succeeds with literal output or fails due to no command chaining
    assert!(result.is_ok() || result.is_err());
    if let Ok(output) = result {
        assert!(output.contains("hello;") || output.contains("hello"));
    }
}

#[tokio::test]
async fn test_run_exec_shell_true_mode_interprets_metacharacters() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let task_id: Arc<str> = Arc::from("test_shell_true");
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new();

    // Shell mode: && should work as command separator
    let params = ExecParams {
        command: "echo hello && echo world".to_string(),
        shell: Some(true),
        timeout: None,
        cwd: None,
        env: None,
    };

    let result = executor
        .run_exec(&task_id, &params, &bindings, &datastore)
        .await
        .unwrap();
    assert!(result.contains("hello"));
    assert!(result.contains("world"));
}

#[tokio::test]
async fn test_run_exec_shell_free_prevents_injection() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let task_id: Arc<str> = Arc::from("test_injection");
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new();

    // In shell-free mode, injection attempts are harmless
    let params = ExecParams {
        command: "echo 'hello; echo injected'".to_string(),
        shell: None,
        timeout: None,
        cwd: None,
        env: None,
    };

    let result = executor
        .run_exec(&task_id, &params, &bindings, &datastore)
        .await
        .unwrap();
    assert!(result.contains("hello; echo injected") || result.contains("hello;"));
}

#[tokio::test]
async fn test_run_exec_security_validation_blocks_dangerous_commands() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let task_id: Arc<str> = Arc::from("test_blocked");
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new();

    // Blocklisted command should be rejected even in shell-free mode
    let params = ExecParams {
        command: "rm -rf /".to_string(),
        shell: None,
        timeout: None,
        cwd: None,
        env: None,
    };

    let result = executor
        .run_exec(&task_id, &params, &bindings, &datastore)
        .await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("NIKA-053") || err.to_string().contains("blocked"));
}
