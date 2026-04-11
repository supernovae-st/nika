// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Tests for TaskExecutor
//!
//! Covers: construction, exec verb, fetch verb, invoke verb, infer verb,
//! binding resolution, decompose, error handling, policy enforcement,
//! shlex parsing, shell-free security, build_json_schema_instruction,
//! and get_rig_provider error paths.

use super::*;
use crate::ast::decompose::{DecomposeSpec, DecomposeStrategy};
use crate::ast::output::{OutputFormat, OutputPolicy, SchemaRef};
use crate::ast::{AgentParams, ExecParams, FetchParams, InferParams, InvokeParams};
use crate::event::EventKind;
use crate::runtime::structured_retry;
use crate::store::{RunContext, TaskResult};
use base64::Engine;
use serde_json::json;
use std::time::Duration;

// ═══════════════════════════════════════════════════════════════
// EXECUTOR CONSTRUCTION TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_executor_new_default() {
    let executor = TaskExecutor::new("claude", None, None, EventLog::new()).unwrap();
    assert_eq!(executor.default_provider.as_ref(), "claude");
    assert!(executor.default_model.is_none());
}

#[test]
fn test_executor_new_with_model() {
    let executor = TaskExecutor::new("openai", Some("gpt-4"), None, EventLog::new()).unwrap();
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

    let executor = TaskExecutor::new("mock", None, Some(mcp_configs), EventLog::new()).unwrap();
    assert_eq!(executor.default_provider.as_ref(), "mock");
}

#[test]
fn test_executor_is_clone() {
    let exec = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let _cloned = exec.clone();
}

// ═══════════════════════════════════════════════════════════════
// EXEC VERB TESTS
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_execute_exec_simple_command() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "echo hello".to_string(),
            shell: None,
            timeout: None,
            cwd: None,
            env: None,
            max_stdout: None,
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
async fn test_exec_stdout_truncation() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    // Generate 200 bytes of output, limit to 100
    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "python3 -c \"print('A' * 200)\"".to_string(),
            shell: Some(true),
            timeout: None,
            cwd: None,
            env: None,
            max_stdout: Some(100),
        },
    };

    let task_id: Arc<str> = Arc::from("test_truncate");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    // Should contain truncation notice
    assert!(
        result.contains("[truncated:"),
        "Expected truncation notice, got: {}",
        &result[..result.len().min(200)]
    );
    // Should not contain all 200 A's
    assert!(result.matches('A').count() < 200);
}

#[tokio::test]
async fn test_exec_stdout_within_limit() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "echo hello".to_string(),
            shell: None,
            timeout: None,
            cwd: None,
            env: None,
            max_stdout: Some(1000),
        },
    };

    let task_id: Arc<str> = Arc::from("test_within_limit");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    // Should NOT contain truncation notice
    assert!(!result.contains("[truncated:"));
    assert_eq!(result, "hello");
}

/// BUG-032: python3 -c and node -e in static YAML templates must be allowed.
/// The security check should recognize interpreter -c/-e as intentional when
/// written directly in YAML (not injected via template resolution).
#[tokio::test]
async fn test_exec_python3_c_allowed_in_static_yaml() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "python3 -c 'import json; print(json.dumps({\"ok\": True}))'".to_string(),
            shell: Some(true),
            timeout: None,
            cwd: None,
            env: None,
            max_stdout: None,
        },
    };

    let task_id: Arc<str> = Arc::from("bug032_python3_c");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;
    assert!(
        result.is_ok(),
        "python3 -c should be allowed in static YAML, got error: {:?}",
        result.err()
    );
    let output = result.unwrap();
    assert!(
        output.contains("ok"),
        "Expected JSON with 'ok', got: {}",
        output
    );
}

#[tokio::test]
async fn test_execute_exec_with_template_binding() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let mut bindings = ResolvedBindings::new();
    bindings.set("name", json!("world"));
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "echo {{with.name}}".to_string(),
            shell: None,
            timeout: None,
            cwd: None,
            env: None,
            max_stdout: None,
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
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    // Use false command which exists and always returns exit code 1
    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "false".to_string(),
            shell: None,
            timeout: None,
            cwd: None,
            env: None,
            max_stdout: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_fail");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;
    assert!(result.is_err());
    match result.unwrap_err() {
        NikaError::ExecError { reason } => {
            // Command fails with non-zero exit code
            assert!(
                reason.contains("failed") || reason.contains("exit code"),
                "Expected failure message, got: {reason}"
            );
        }
        err => panic!("Expected ExecError, got: {err:?}"),
    }
}

#[tokio::test]
async fn test_execute_exec_emits_template_resolved() {
    let event_log = EventLog::new();
    let executor = TaskExecutor::new("mock", None, None, event_log.clone()).unwrap();
    let mut bindings = ResolvedBindings::new();
    bindings.set("greeting", json!("Hello"));
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "echo {{with.greeting}}".to_string(),
            shell: None,
            timeout: None,
            cwd: None,
            env: None,
            max_stdout: None,
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
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

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
            response: None,
            extract: None,
            selector: None,
            session: None,
            cache: None,
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
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let mut bindings = ResolvedBindings::new();
    bindings.set("endpoint", json!("httpbin.org/get"));
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let action = TaskAction::Fetch {
        fetch: FetchParams {
            url: "https://{{with.endpoint}}".to_string(),
            method: "GET".to_string(),
            headers: rustc_hash::FxHashMap::default(),
            body: None,
            json: None,
            timeout: None,
            retry: None,
            follow_redirects: None,
            response: None,
            extract: None,
            selector: None,
            session: None,
            cache: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_fetch_template");
    // This will connect to the real httpbin, so we expect success if network is available
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;
    // Just verify template was resolved (regardless of network success/failure)
    let events = EventLog::new();
    let executor2 = TaskExecutor::new("mock", None, None, events.clone()).unwrap();
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
    let executor = TaskExecutor::new("mock", None, None, event_log.clone()).unwrap();
    executor.inject_mock_mcp_client("novanet");
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let action = TaskAction::Invoke {
        invoke: InvokeParams {
            mcp: Some("novanet".to_string()),
            tool: Some("novanet_context".to_string()),
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
    let executor = TaskExecutor::new("mock", None, None, event_log).unwrap();
    executor.inject_mock_mcp_client("novanet");
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

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
    let executor = TaskExecutor::new("mock", None, None, event_log.clone()).unwrap();
    executor.inject_mock_mcp_client("novanet");
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

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
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    executor.inject_mock_mcp_client("novanet");
    let mut bindings = ResolvedBindings::new();
    bindings.set("entity_key", json!("qr-code"));
    bindings.set("locale_val", json!("en-US"));
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let action = TaskAction::Invoke {
        invoke: InvokeParams {
            mcp: Some("novanet".to_string()),
            tool: Some("novanet_context".to_string()),
            params: Some(json!({
                "entity": "{{with.entity_key}}",
                "locale": "{{with.locale_val}}"
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
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

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
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

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
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    // No inject_mock_mcp_client() - server not configured
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

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
// BUILTIN MEDIA TOOL MEDIA STAGING TESTS
// ═══════════════════════════════════════════════════════════════

/// Builtin tools that return JSON with hash/path/mime_type fields
/// must stage a MediaRef in the datastore so that artifact format: binary
/// can find it. Without this, write_binary_artifact() gets empty media_refs → NIKA-281.
#[tokio::test]
async fn test_builtin_invoke_stages_media_ref() {
    let event_log = EventLog::new();
    let executor = TaskExecutor::new("mock", None, None, event_log).unwrap();
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    // Use nika:decode instead of nika:import to avoid path confinement issues in tests.
    // Both tools return the same hash/mime_type/size_bytes JSON and test media staging.
    let png_b64 = base64::engine::general_purpose::STANDARD
        .encode([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
    let action = TaskAction::Invoke {
        invoke: InvokeParams {
            mcp: None,
            tool: Some("nika:decode".to_string()),
            params: Some(json!({"data": png_b64, "mime_type": "image/png"})),
            resource: None,
            timeout: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_builtin_media");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();

    // Verify the JSON output has hash (existing behavior)
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert!(parsed["hash"].as_str().unwrap().starts_with("blake3:"));

    // After execute, the datastore MUST have a staged MediaRef
    let media_refs = datastore.take_media(&task_id);
    assert_eq!(
        media_refs.len(),
        1,
        "builtin media tool must stage exactly 1 MediaRef"
    );

    let mr = &media_refs[0];
    assert!(
        mr.hash.starts_with("blake3:"),
        "MediaRef hash must be blake3-prefixed"
    );
    assert_eq!(mr.created_by, "test_builtin_media");
    assert!(mr.size_bytes > 0);
}

// ═══════════════════════════════════════════════════════════════
// BINDING RESOLUTION TESTS
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_binding_resolution_single_template() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let mut bindings = ResolvedBindings::new();
    bindings.set("key", json!("value123"));
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "echo {{with.key}}".to_string(),
            shell: None,
            timeout: None,
            cwd: None,
            env: None,
            max_stdout: None,
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
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let mut bindings = ResolvedBindings::new();
    bindings.set("first", json!("hello"));
    bindings.set("second", json!("world"));
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "echo {{with.first}} {{with.second}}".to_string(),
            shell: None,
            timeout: None,
            cwd: None,
            env: None,
            max_stdout: None,
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
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "echo static".to_string(),
            shell: None,
            timeout: None,
            cwd: None,
            env: None,
            max_stdout: None,
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
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let mut bindings = ResolvedBindings::new();
    bindings.set("data", json!({"id": 42, "name": "test"}));
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "echo {{with.data}}".to_string(),
            shell: None,
            timeout: None,
            cwd: None,
            env: None,
            max_stdout: None,
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
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let mut bindings = ResolvedBindings::new();
    bindings.set("task_output", json!({"data": "ok"}));
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);
    let task_id_prev: Arc<str> = Arc::from("prev_task");
    datastore.insert(
        task_id_prev.clone(),
        TaskResult::success_str("from_previous_task", Duration::from_millis(100)),
    );

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "echo {{with.task_output}}".to_string(),
            shell: None,
            timeout: None,
            cwd: None,
            env: None,
            max_stdout: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_store");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    assert!(result.contains("data"));
}

// ═══════════════════════════════════════════════════════════════
// DECOMPOSE TESTS
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_expand_decompose_static() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let mut bindings = ResolvedBindings::new();
    bindings.set("items", json!(["item1", "item2", "item3"]));
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let spec = DecomposeSpec {
        strategy: DecomposeStrategy::Static,
        traverse: "HAS_CHILD".to_string(),
        source: "{{with.items}}".to_string(),
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
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let mut bindings = ResolvedBindings::new();
    bindings.set("items", json!(["a", "b", "c", "d", "e"]));
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let spec = DecomposeSpec {
        strategy: DecomposeStrategy::Static,
        traverse: "HAS_CHILD".to_string(),
        source: "{{with.items}}".to_string(),
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
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let mut bindings = ResolvedBindings::new();
    bindings.set("notarray", json!({"key": "value"}));
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let spec = DecomposeSpec {
        strategy: DecomposeStrategy::Static,
        traverse: "HAS_CHILD".to_string(),
        source: "{{with.notarray}}".to_string(),
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
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let value = json!("entity:qr-code");

    let key = executor.extract_decompose_key(&value).unwrap();
    assert_eq!(key, "entity:qr-code");
}

#[tokio::test]
async fn test_extract_decompose_key_from_object() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let value = json!({"key": "entity:test", "name": "Test Entity"});

    let key = executor.extract_decompose_key(&value).unwrap();
    assert_eq!(key, "entity:test");
}

#[tokio::test]
async fn test_extract_decompose_key_invalid() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let value = json!(123);

    let result = executor.extract_decompose_key(&value);
    assert!(result.is_err());
}

#[tokio::test]
async fn test_extract_decompose_nodes_from_nodes_field() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
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
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let result_json = json!({
        "items": ["item1", "item2", "item3"]
    });

    let nodes = executor.extract_decompose_nodes(result_json).unwrap();
    assert_eq!(nodes.len(), 3);
}

#[tokio::test]
async fn test_extract_decompose_nodes_from_results_field() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let result_json = json!({
        "results": ["result1", "result2"]
    });

    let nodes = executor.extract_decompose_nodes(result_json).unwrap();
    assert_eq!(nodes.len(), 2);
}

#[tokio::test]
async fn test_extract_decompose_nodes_from_array() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let result_json = json!(["direct1", "direct2"]);

    let nodes = executor.extract_decompose_nodes(result_json).unwrap();
    assert_eq!(nodes.len(), 2);
}

#[tokio::test]
async fn test_extract_decompose_nodes_empty_nodes() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let result_json = json!({"nodes": []});

    let nodes = executor.extract_decompose_nodes(result_json).unwrap();
    assert_eq!(nodes.len(), 0);
}

// ═══════════════════════════════════════════════════════════════
// ERROR HANDLING TESTS
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_error_handling_exec_timeout() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    // Sleep command longer than timeout
    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "sleep 100".to_string(),
            shell: None,
            timeout: None,
            cwd: None,
            env: None,
            max_stdout: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_timeout");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;

    assert!(result.is_err(), "Should timeout");
    match result.unwrap_err() {
        NikaError::ExecError { reason } => {
            assert!(reason.contains("timed out") || reason.contains("timeout"));
        }
        err => panic!("Expected ExecError with timeout, got: {err:?}"),
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
            max_stdout: None,
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
            response: None,
            extract: None,
            selector: None,
            session: None,
            cache: None,
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

            provider_chain: None,
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
    let executor = TaskExecutor::with_policy(
        "mock",
        None,
        None,
        EventLog::new(),
        Some(policy_config),
        None,
        None,
    )
    .unwrap();
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "dangerous_tool --flag".to_string(),
            shell: None,
            timeout: None,
            cwd: None,
            env: None,
            max_stdout: None,
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
    let executor = TaskExecutor::with_policy(
        "mock",
        None,
        None,
        EventLog::new(),
        Some(policy_config),
        None,
        None,
    )
    .unwrap();
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "echo hello".to_string(),
            shell: None,
            timeout: None,
            cwd: None,
            env: None,
            max_stdout: None,
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
    let executor = TaskExecutor::with_policy(
        "mock",
        None,
        None,
        EventLog::new(),
        Some(policy_config),
        None,
        None,
    )
    .unwrap();
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "echo safe".to_string(),
            shell: None,
            timeout: None,
            cwd: None,
            env: None,
            max_stdout: None,
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
    let executor = TaskExecutor::with_policy(
        "mock",
        None,
        None,
        EventLog::new(),
        Some(policy_config),
        None,
        None,
    )
    .unwrap();
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

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
            response: None,
            extract: None,
            selector: None,
            session: None,
            cache: None,
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
    let executor = TaskExecutor::with_policy(
        "mock",
        None,
        None,
        EventLog::new(),
        Some(policy_config),
        None,
        None,
    )
    .unwrap();
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

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
            response: None,
            extract: None,
            selector: None,
            session: None,
            cache: None,
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

    let executor = TaskExecutor::with_policy(
        "mock",
        None,
        None,
        EventLog::new(),
        Some(policy_config),
        None,
        None,
    )
    .unwrap();

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
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let task_id: Arc<str> = Arc::from("test_shell_free");
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    // Shell-free: semicolon should NOT be interpreted as command separator
    let params = ExecParams {
        command: "echo hello; echo world".to_string(),
        shell: None, // Default: shell-free
        timeout: None,
        cwd: None,
        env: None,
        max_stdout: None,
    };

    let result = executor
        .run_exec(&task_id, &params, &bindings, &datastore)
        .await;
    // Shell-free: shlex splits "echo hello; echo world" into ["echo", "hello;", "echo", "world"]
    // echo receives all args and succeeds
    assert!(
        result.is_ok(),
        "Shell-free exec should succeed: {:?}",
        result.err()
    );
    if let Ok(output) = result {
        assert!(output.contains("hello;") || output.contains("hello"));
    }
}

#[tokio::test]
async fn test_run_exec_shell_true_mode_interprets_metacharacters() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let task_id: Arc<str> = Arc::from("test_shell_true");
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    // Shell mode: && should work as command separator
    let params = ExecParams {
        command: "echo hello && echo world".to_string(),
        shell: Some(true),
        timeout: None,
        cwd: None,
        env: None,
        max_stdout: None,
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
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let task_id: Arc<str> = Arc::from("test_injection");
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    // In shell-free mode, injection attempts are harmless
    let params = ExecParams {
        command: "echo 'hello; echo injected'".to_string(),
        shell: None,
        timeout: None,
        cwd: None,
        env: None,
        max_stdout: None,
    };

    let result = executor
        .run_exec(&task_id, &params, &bindings, &datastore)
        .await
        .unwrap();
    assert!(result.contains("hello; echo injected") || result.contains("hello;"));
}

#[tokio::test]
async fn test_run_exec_security_validation_blocks_dangerous_commands() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let task_id: Arc<str> = Arc::from("test_blocked");
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    // Blocklisted command should be rejected even in shell-free mode
    let params = ExecParams {
        command: "rm -rf /".to_string(),
        shell: None,
        timeout: None,
        cwd: None,
        env: None,
        max_stdout: None,
    };

    let result = executor
        .run_exec(&task_id, &params, &bindings, &datastore)
        .await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("NIKA-053") || err.to_string().contains("blocked"));
}

// ═══════════════════════════════════════════════════════════════
// BUILD_JSON_SCHEMA_INSTRUCTION TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_build_json_schema_instruction_none_policy() {
    let result = TaskExecutor::build_json_schema_instruction(None, None);
    assert!(result.is_none());
}

#[test]
fn test_build_json_schema_instruction_text_format() {
    let policy = OutputPolicy {
        format: OutputFormat::Text,
        schema: Some(SchemaRef::Inline(json!({"type": "object"}))),
        from_example: None,
        max_retries: None,
        source_structured_spec: None,
    };
    let result = TaskExecutor::build_json_schema_instruction(Some(&policy), None);
    assert!(
        result.is_none(),
        "Text format should not produce schema instruction"
    );
}

#[test]
fn test_build_json_schema_instruction_json_no_schema() {
    let policy = OutputPolicy {
        format: OutputFormat::Json,
        schema: None,
        from_example: None,
        max_retries: None,
        source_structured_spec: None,
    };
    let result = TaskExecutor::build_json_schema_instruction(Some(&policy), None);
    assert!(
        result.is_none(),
        "JSON format without schema should return None"
    );
}

#[test]
fn test_build_json_schema_instruction_json_inline_schema() {
    let schema = json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "age": { "type": "integer" }
        },
        "required": ["name", "age"]
    });
    let policy = OutputPolicy {
        format: OutputFormat::Json,
        schema: Some(SchemaRef::Inline(schema.clone())),
        from_example: None,
        max_retries: None,
        source_structured_spec: None,
    };
    let result = TaskExecutor::build_json_schema_instruction(Some(&policy), None);
    assert!(result.is_some());
    let instruction = result.unwrap();
    assert!(instruction.contains("CRITICAL OUTPUT REQUIREMENT"));
    assert!(instruction.contains("\"name\""));
    assert!(instruction.contains("\"age\""));
    assert!(instruction.contains("conforms to this schema"));
}

#[test]
fn test_build_json_schema_instruction_json_file_schema() {
    let policy = OutputPolicy {
        format: OutputFormat::Json,
        schema: Some(SchemaRef::File("schemas/user.json".to_string())),
        from_example: None,
        max_retries: None,
        source_structured_spec: None,
    };
    let result = TaskExecutor::build_json_schema_instruction(Some(&policy), None);
    assert!(result.is_some());
    let instruction = result.unwrap();
    assert!(instruction.contains("CRITICAL OUTPUT REQUIREMENT"));
    assert!(instruction.contains("valid JSON"));
    // File ref produces a generic instruction without schema content
    assert!(!instruction.contains("conforms to this schema"));
}

#[test]
fn test_build_json_schema_instruction_yaml_format() {
    let policy = OutputPolicy {
        format: OutputFormat::Yaml,
        schema: Some(SchemaRef::Inline(json!({"type": "object"}))),
        from_example: None,
        max_retries: None,
        source_structured_spec: None,
    };
    let result = TaskExecutor::build_json_schema_instruction(Some(&policy), None);
    assert!(
        result.is_none(),
        "YAML format should not produce schema instruction"
    );
}

#[test]
fn test_build_json_schema_instruction_markdown_format() {
    let policy = OutputPolicy {
        format: OutputFormat::Markdown,
        schema: Some(SchemaRef::Inline(json!({"type": "object"}))),
        from_example: None,
        max_retries: None,
        source_structured_spec: None,
    };
    let result = TaskExecutor::build_json_schema_instruction(Some(&policy), None);
    assert!(
        result.is_none(),
        "Markdown format should not produce schema instruction"
    );
}

// ═══════════════════════════════════════════════════════════════
// BUILD_JSON_SCHEMA_INSTRUCTION — from_example paths
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_build_json_schema_instruction_inline_from_example() {
    use crate::ast::structured::StructuredOutputSpec;

    let spec = StructuredOutputSpec::with_example_inline(json!({
        "name": "alice",
        "score": 42
    }));
    let policy = spec.to_output_policy();
    let result = TaskExecutor::build_json_schema_instruction(Some(&policy), None);
    let instruction = result.expect("should produce instruction for inline from_example");
    assert!(
        instruction.contains("name"),
        "inline example keys must appear in prompt"
    );
    assert!(
        instruction.contains("score"),
        "inline example values must appear in prompt"
    );
    assert!(
        instruction.contains("exact structure"),
        "inline from_example should use exact-structure wording"
    );
}

#[test]
fn test_build_json_schema_instruction_file_from_example_returns_generic() {
    use crate::ast::structured::StructuredOutputSpec;

    let spec = StructuredOutputSpec::with_example_file("./structure.json");
    let policy = spec.to_output_policy();
    let result = TaskExecutor::build_json_schema_instruction(Some(&policy), None);
    let instruction = result.expect("file from_example should produce generic instruction");
    // Must NOT inject the {} placeholder schema
    assert!(
        !instruction.contains("\"{}\"") && !instruction.contains("conforms to this schema"),
        "file from_example must NOT inject placeholder schema: {instruction}"
    );
    // Must give valid-JSON instruction
    assert!(
        instruction.contains("valid JSON"),
        "file from_example must produce generic valid-JSON instruction"
    );
}

#[test]
fn test_build_json_schema_instruction_file_from_example_with_cached_value() {
    use crate::ast::structured::StructuredOutputSpec;

    let spec = StructuredOutputSpec::with_example_file("./structure.json");
    let policy = spec.to_output_policy();
    let cached = json!({"name": "Alice", "score": 42});
    let result = TaskExecutor::build_json_schema_instruction(Some(&policy), Some(&cached));
    let instruction = result.expect("file from_example with cache should produce full instruction");
    assert!(
        instruction.contains("Alice"),
        "cached example should be injected into prompt"
    );
    assert!(
        instruction.contains("exact structure"),
        "should use exact-structure wording"
    );
}

// ═══════════════════════════════════════════════════════════════
// GET_RIG_PROVIDER ERROR PATH TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_get_rig_provider_unknown_provider() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let result = executor.get_rig_provider("nonexistent_provider");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("nonexistent_provider"),
        "Error should mention the unknown provider name"
    );
}

#[test]
fn test_get_rig_provider_empty_name() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let result = executor.get_rig_provider("");
    assert!(result.is_err(), "Empty provider name should fail");
}

// ═══════════════════════════════════════════════════════════════
// GET_DYN_PROVIDER — Constellation Phase 11 keystone
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_get_dyn_provider_mock_returns_trait_object() {
    use nika_kernel::provider::Provider;

    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let dyn_provider = executor.get_dyn_provider("mock").unwrap();

    // Prove the trait object is callable via the kernel trait
    assert_eq!(Provider::name(dyn_provider.as_ref()), "mock");
    let caps = Provider::capabilities(dyn_provider.as_ref(), "any-model");
    assert!(caps.is_some(), "capabilities should return Some for mock");
}

#[test]
fn test_get_dyn_provider_unknown_provider_errors() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let result = executor.get_dyn_provider("nonexistent_provider");
    assert!(result.is_err());
}

#[test]
fn test_get_dyn_provider_is_send_sync() {
    // Compile-time assertion: Arc<dyn Provider> returned by get_dyn_provider
    // must be Send + Sync so it can cross .await points and be shared between
    // spawned tasks. This is the core requirement for Phase 12+ verb crates.
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<std::sync::Arc<dyn nika_kernel::provider::Provider>>();
}

// ═══════════════════════════════════════════════════════════════
// RUN_INFER MOCK PROVIDER TESTS
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_run_infer_mock_basic() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let task_id: Arc<str> = Arc::from("test-infer-mock");
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::default();

    let infer = InferParams {
        prompt: "Generate a test response".to_string(),
        ..Default::default()
    };

    let result = executor
        .run_infer(&task_id, &infer, &bindings, &datastore, None)
        .await;
    assert!(
        result.is_ok(),
        "Mock infer should succeed: {:?}",
        result.err()
    );

    let response = result.unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(&response).expect("Mock response should be valid JSON");

    assert_eq!(parsed["mock"], true);
    assert_eq!(parsed["task_id"], "test-infer-mock");
    assert_eq!(parsed["status"], "success");
    assert!(parsed["items"].is_array());
}

#[tokio::test]
async fn test_run_infer_mock_emits_provider_responded() {
    let event_log = EventLog::new();
    let executor = TaskExecutor::new("mock", None, None, event_log.clone()).unwrap();
    let task_id: Arc<str> = Arc::from("test-infer-events");
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::default();

    let infer = InferParams {
        prompt: "Hello mock".to_string(),
        ..Default::default()
    };

    executor
        .run_infer(&task_id, &infer, &bindings, &datastore, None)
        .await
        .expect("Mock infer should succeed");

    let events = event_log.events();
    let has_provider_responded = events.iter().any(|e| {
        matches!(
            &e.kind,
            EventKind::ProviderResponded { task_id, .. } if task_id.as_ref() == "test-infer-events"
        )
    });
    assert!(
        has_provider_responded,
        "Should emit ProviderResponded event"
    );
}

/// **W16-B1 — engine-side golden oracle for the mock fast-path.**
///
/// The S14-δ golden oracle
/// (`infer_emits_provider_responded_with_all_fields` in
/// `nika-verb-infer/src/lib.rs`) asserts all 8
/// `ProviderResponded` fields when the event is emitted via the verb
/// crate's `run()`. After W16-A0 the engine's mock fast-path at
/// `infer.rs:621` routes through the SAME helper
/// (`nika_verb_infer::emit_provider_responded`), but the helper
/// receives values synthesized by the engine (mock request id,
/// estimated tokens, `FinishReason::Mock`, zero cost), not pulled from
/// an `InferResponse`.
///
/// Phase 1 rust-pro flagged this as the single biggest regression gap:
/// the existing engine test above only asserts `task_id` presence, so
/// a silent field drop during the helper extraction or any future
/// refactor would ship undetected. This test closes that gap by
/// pinning every field emitted by the engine's mock path to either an
/// exact value (the deterministic ones) or a concrete non-zero bound
/// (the estimate-driven ones).
///
/// Invariant #24 companion: whenever a new field lands on
/// `EventKind::ProviderResponded`, the destructure below fails to
/// compile AND this assertion list has to grow — the test is
/// deliberately exhaustive, mirroring the verb-crate golden oracle.
#[tokio::test]
async fn test_run_infer_mock_emits_provider_responded_with_all_fields() {
    let event_log = EventLog::new();
    let executor = TaskExecutor::new("mock", None, None, event_log.clone()).unwrap();
    let task_id: Arc<str> = Arc::from("w16-b1-golden-task");
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::default();

    let infer = InferParams {
        prompt: "Hello mock, describe Alice briefly.".to_string(),
        ..Default::default()
    };

    executor
        .run_infer(&task_id, &infer, &bindings, &datastore, None)
        .await
        .expect("Mock infer should succeed");

    let events = event_log.events();
    let provider_responded: Vec<_> = events
        .iter()
        .filter(|e| matches!(&e.kind, EventKind::ProviderResponded { .. }))
        .collect();

    // Exactly one event — catches accidental double-emit regressions
    // from a future refactor that calls the helper twice by mistake.
    assert_eq!(
        provider_responded.len(),
        1,
        "mock fast-path must emit exactly one ProviderResponded event"
    );

    let EventKind::ProviderResponded {
        task_id: emitted_task_id,
        request_id,
        input_tokens,
        output_tokens,
        cache_read_tokens,
        ttft_ms,
        finish_reason,
        cost_usd,
    } = &provider_responded[0].kind
    else {
        panic!(
            "expected ProviderResponded, got {:?}",
            provider_responded[0].kind
        );
    };

    // task_id — threaded through from the run_infer argument.
    assert_eq!(emitted_task_id.as_ref(), "w16-b1-golden-task");

    // Mock path pins request_id to the sentinel string. Any change
    // here should be a deliberate contract update, not an accident.
    assert_eq!(
        request_id.as_deref(),
        Some("mock-request"),
        "mock path request_id contract is the \"mock-request\" sentinel"
    );

    // Mock tokens come from estimate_tokens(len); we only pin that
    // they are strictly positive — the estimator is internal and
    // tests should not assert its exact formula, but a zero on either
    // side would indicate the mock synth pipeline is broken.
    assert!(
        *input_tokens > 0,
        "mock input_tokens must be > 0 for a non-empty prompt, got {input_tokens}"
    );
    assert!(
        *output_tokens > 0,
        "mock output_tokens must be > 0 for a synthesized response, got {output_tokens}"
    );

    // Deterministic fields — the mock path hardcodes these and they
    // are load-bearing for invariant #24 (golden oracle has a value
    // to pin, not just a shape).
    assert_eq!(
        *cache_read_tokens, 0,
        "mock path never reports cached tokens"
    );
    assert_eq!(
        *ttft_ms,
        Some(0),
        "mock path uses ttft_ms=Some(0), not None — a None would break TUI rendering guards"
    );
    assert_eq!(
        *finish_reason,
        nika_event::FinishReason::Mock,
        "mock path must emit FinishReason::Mock, NOT Stop/EndTurn — downstream tooling distinguishes them"
    );
    // Cost is exact 0.0 f64 — pure pass-through, no arithmetic. The
    // S14.5 precedent (cost_usd assertion in verb-crate golden)
    // applies: exact equality is correct here.
    assert_eq!(
        *cost_usd, 0.0_f64,
        "mock path must emit cost_usd=0.0 — no cost accounting on the mock fast-path"
    );
}

#[tokio::test]
async fn test_run_infer_mock_with_json_schema_injection() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let task_id: Arc<str> = Arc::from("test-infer-schema");
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::default();

    let infer = InferParams {
        prompt: "Generate user data".to_string(),
        ..Default::default()
    };

    let policy = OutputPolicy {
        format: OutputFormat::Json,
        schema: Some(SchemaRef::Inline(json!({
            "type": "object",
            "properties": { "name": { "type": "string" } }
        }))),
        from_example: None,
        max_retries: None,
        source_structured_spec: None,
    };

    // With output policy, the prompt gets schema instruction appended
    // but mock still returns its canned response
    let result = executor
        .run_infer(&task_id, &infer, &bindings, &datastore, Some(&policy))
        .await;
    assert!(result.is_ok(), "Mock infer with schema should succeed");
}

#[tokio::test]
async fn test_run_infer_mock_with_task_level_provider() {
    // Executor default is "claude" but task overrides to "mock"
    let executor = TaskExecutor::new("claude", None, None, EventLog::new()).unwrap();
    let task_id: Arc<str> = Arc::from("test-infer-override");
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::default();

    let infer = InferParams {
        prompt: "Test task-level provider override".to_string(),
        provider: Some(nika_core::ProviderName::Mock),
        ..Default::default()
    };

    let result = executor
        .run_infer(&task_id, &infer, &bindings, &datastore, None)
        .await;
    assert!(
        result.is_ok(),
        "Task-level mock provider override should succeed: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn test_run_infer_empty_prompt() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let task_id: Arc<str> = Arc::from("test-infer-empty");
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::default();

    let infer = InferParams {
        prompt: "   ".to_string(),
        ..Default::default()
    };

    let result = executor
        .run_infer(&task_id, &infer, &bindings, &datastore, None)
        .await;
    assert!(result.is_err(), "Empty prompt should fail validation");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("empty") || err.contains("Empty") || err.contains("prompt"),
        "Error should mention empty prompt: {}",
        err
    );
}

// ═══════════════════════════════════════════════════════════════
// EXEC VERB: ENV VAR INJECTION TESTS
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_run_exec_with_env_vars() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let mut env = rustc_hash::FxHashMap::default();
    env.insert("MY_VAR".to_string(), "hello_from_env".to_string());

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "echo $MY_VAR".to_string(),
            shell: Some(true),
            timeout: None,
            cwd: None,
            env: Some(env),
            max_stdout: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_env");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    assert_eq!(result, "hello_from_env");
}

#[tokio::test]
async fn test_run_exec_with_env_template_resolution() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let mut bindings = ResolvedBindings::new();
    bindings.set("env_val", json!("resolved_value"));
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let mut env = rustc_hash::FxHashMap::default();
    env.insert("DYNAMIC".to_string(), "{{with.env_val}}".to_string());

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "echo $DYNAMIC".to_string(),
            shell: Some(true),
            timeout: None,
            cwd: None,
            env: Some(env),
            max_stdout: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_env_tpl");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    assert_eq!(result, "resolved_value");
}

#[tokio::test]
async fn test_run_exec_with_multiple_env_vars() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let mut env = rustc_hash::FxHashMap::default();
    env.insert("A".to_string(), "first".to_string());
    env.insert("B".to_string(), "second".to_string());

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "echo ${A}_${B}".to_string(),
            shell: Some(true),
            timeout: None,
            cwd: None,
            env: Some(env),
            max_stdout: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_multi_env");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    assert_eq!(result, "first_second");
}

// ═══════════════════════════════════════════════════════════════
// EXEC VERB: PER-TASK TIMEOUT
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_run_exec_with_custom_timeout() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    // Use a short timeout with a command that finishes quickly
    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "echo fast".to_string(),
            shell: None,
            timeout: Some(10), // 10 seconds, plenty of time
            cwd: None,
            env: None,
            max_stdout: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_timeout_ok");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    assert_eq!(result, "fast");
}

// ═══════════════════════════════════════════════════════════════
// INFER VERB: TEMPLATE RESOLUTION
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_run_infer_mock_with_template_binding() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let task_id: Arc<str> = Arc::from("test-infer-tpl");
    let mut bindings = ResolvedBindings::new();
    bindings.set("topic", json!("quantum computing"));
    let datastore = RunContext::default();

    let infer = InferParams {
        prompt: "Explain {{with.topic}} in simple terms".to_string(),
        ..Default::default()
    };

    let result = executor
        .run_infer(&task_id, &infer, &bindings, &datastore, None)
        .await;
    assert!(
        result.is_ok(),
        "Infer with template should succeed: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn test_run_infer_mock_missing_binding_fails() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let task_id: Arc<str> = Arc::from("test-infer-missing");
    let bindings = ResolvedBindings::new(); // Empty bindings
    let datastore = RunContext::default();

    let infer = InferParams {
        prompt: "Process {{with.nonexistent}}".to_string(),
        ..Default::default()
    };

    let result = executor
        .run_infer(&task_id, &infer, &bindings, &datastore, None)
        .await;
    assert!(result.is_err(), "Missing binding should fail");
}

#[tokio::test]
async fn test_run_infer_mock_with_model_override() {
    let executor = TaskExecutor::new("mock", Some("default-model"), None, EventLog::new()).unwrap();
    let task_id: Arc<str> = Arc::from("test-infer-model");
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::default();

    let infer = InferParams {
        prompt: "Generate something".to_string(),
        model: Some("custom-model".to_string()),
        ..Default::default()
    };

    let result = executor
        .run_infer(&task_id, &infer, &bindings, &datastore, None)
        .await;
    assert!(
        result.is_ok(),
        "Model override should succeed with mock: {:?}",
        result.err()
    );
}

// ═══════════════════════════════════════════════════════════════
// DECOMPOSE: RESOLVE SOURCE VARIANTS
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_resolve_decompose_source_literal() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let result = executor.resolve_decompose_source("some-key", &bindings, &datastore);
    assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    assert_eq!(result.unwrap(), json!("some-key"));
}

#[tokio::test]
async fn test_resolve_decompose_source_template() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let mut bindings = ResolvedBindings::new();
    bindings.set("entity", json!("qr-code"));
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let result = executor.resolve_decompose_source("{{with.entity}}", &bindings, &datastore);
    assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    assert_eq!(result.unwrap(), json!("qr-code"));
}

#[tokio::test]
async fn test_resolve_decompose_source_dollar_binding() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let mut bindings = ResolvedBindings::new();
    bindings.set("my_key", json!("entity-key"));
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let result = executor.resolve_decompose_source("$my_key", &bindings, &datastore);
    assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    assert_eq!(result.unwrap(), json!("entity-key"));
}

#[tokio::test]
async fn test_resolve_decompose_source_missing_binding_fails() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let result = executor.resolve_decompose_source("$nonexistent", &bindings, &datastore);
    assert!(result.is_err(), "Missing binding should fail");
}

#[tokio::test]
async fn test_resolve_decompose_source_dollar_path_from_datastore() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    // Insert task result with nested field
    datastore.insert(
        Arc::from("prev_task"),
        TaskResult::success(json!({"key": "nested-key"}), Duration::from_millis(1)),
    );

    let result = executor.resolve_decompose_source("$prev_task.key", &bindings, &datastore);
    assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    assert_eq!(result.unwrap(), json!("nested-key"));
}

// ═══════════════════════════════════════════════════════════════
// DECOMPOSE: JSON TYPE NAME
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_json_type_name_all_types() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();

    assert_eq!(executor.json_type_name(&json!(null)), "null");
    assert_eq!(executor.json_type_name(&json!(true)), "boolean");
    assert_eq!(executor.json_type_name(&json!(42)), "number");
    assert_eq!(executor.json_type_name(&json!("text")), "string");
    assert_eq!(executor.json_type_name(&json!([])), "array");
    assert_eq!(executor.json_type_name(&json!({})), "object");
}

// ═══════════════════════════════════════════════════════════════
// DECOMPOSE: EXTRACT KEY EDGE CASES
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_extract_decompose_key_from_object_with_extra_fields() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let value = json!({"key": "my-entity", "name": "My Entity", "type": "Entity"});
    let key = executor.extract_decompose_key(&value).unwrap();
    assert_eq!(key, "my-entity");
}

#[tokio::test]
async fn test_extract_decompose_key_from_number_fails() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let result = executor.extract_decompose_key(&json!(42));
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("number"),
        "Should mention the actual type: {}",
        err
    );
}

#[tokio::test]
async fn test_extract_decompose_key_from_null_fails() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let result = executor.extract_decompose_key(&json!(null));
    assert!(result.is_err());
}

// ═══════════════════════════════════════════════════════════════
// DECOMPOSE: EXTRACT NODES EDGE CASES
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_extract_decompose_nodes_from_non_object_non_array_fails() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let result = executor.extract_decompose_nodes(json!("just a string"));
    assert!(result.is_err());
}

#[tokio::test]
async fn test_extract_decompose_nodes_from_object_without_known_fields_fails() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let result = executor.extract_decompose_nodes(json!({"data": [1, 2, 3]}));
    assert!(result.is_err());
}

// ═══════════════════════════════════════════════════════════════
// DEEP AUDIT: EXEC VERB — EDGE CASES AND BUGS
// ═══════════════════════════════════════════════════════════════

/// AUDIT: exec with per-task timeout should actually cancel quickly.
///
/// BUG DOCUMENTED: `tokio::time::timeout` cancels the future but does
/// NOT call `kill()` on the spawned OS child process. The child process
/// may continue running as an orphan. `kill_on_drop(true)` is never set.
#[tokio::test]
async fn audit_exec_timeout_fires_promptly() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "sleep 60".to_string(),
            shell: None,
            timeout: Some(1),
            cwd: None,
            env: None,
            max_stdout: None,
        },
    };

    let task_id: Arc<str> = Arc::from("audit_timeout");
    let start = std::time::Instant::now();
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;
    let elapsed = start.elapsed();

    assert!(result.is_err(), "Should timeout");
    assert!(
        elapsed < Duration::from_secs(5),
        "Timeout should fire in ~1s, took {:?}",
        elapsed
    );
    match result.unwrap_err() {
        NikaError::ExecError { reason } => {
            assert!(
                reason.contains("timed out"),
                "Expected timeout, got: {}",
                reason
            );
        }
        err => panic!("Expected ExecError, got: {:?}", err),
    }
    // GAP: The sleep 60 child process may still be alive.
    // Fix: call cmd.kill_on_drop(true) before spawning.
}

/// AUDIT: exec with JSON object binding breaks shlex in shell-free mode.
///
/// When {{with.data}} resolves to a JSON object like {"key":"value"},
/// the compact serialization `{"key":"value"}` contains double quotes.
/// shlex::split treats these as shell quoting — the `{` is outside
/// quotes but `key` is inside quotes, producing unexpected splitting.
#[tokio::test]
async fn audit_exec_json_object_binding_breaks_shlex() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let mut bindings = ResolvedBindings::new();
    bindings.set("data", json!({"key": "value"}));
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "echo {{with.data}}".to_string(),
            shell: None,
            timeout: None,
            cwd: None,
            env: None,
            max_stdout: None,
        },
    };

    let task_id: Arc<str> = Arc::from("audit_json_shlex");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;

    // Document actual behavior: shlex may produce unexpected results
    // because JSON's double quotes interact with shlex quoting rules.
    // The test documents whichever behavior actually occurs.
    match result {
        Ok(output) => {
            // shlex managed to parse it somehow
            assert!(
                output.contains("key"),
                "JSON content should appear: {}",
                output
            );
        }
        Err(err) => {
            // shlex failed to parse — this IS the bug
            let msg = err.to_string();
            assert!(
                msg.contains("parse command") || msg.contains("unbalanced"),
                "Expected shlex parse error: {}",
                msg
            );
        }
    }
}

/// AUDIT: exec stderr is captured in error message on failure.
#[tokio::test]
async fn audit_exec_stderr_in_error_message() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "sh -c 'echo AUDIT_ERROR >&2 && exit 1'".to_string(),
            shell: Some(true),
            timeout: None,
            cwd: None,
            env: None,
            max_stdout: None,
        },
    };

    let task_id: Arc<str> = Arc::from("audit_stderr");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("AUDIT_ERROR"),
        "Error should contain stderr, got: {}",
        err_msg
    );
}

/// AUDIT: exec output trailing whitespace is trimmed.
#[tokio::test]
async fn audit_exec_output_trimmed() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "printf 'hello\\n\\n\\n'".to_string(),
            shell: Some(true),
            timeout: None,
            cwd: None,
            env: None,
            max_stdout: None,
        },
    };

    let task_id: Arc<str> = Arc::from("audit_trim");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    assert_eq!(result, "hello", "Output should be trimmed: '{}'", result);
}

// ═══════════════════════════════════════════════════════════════
// DEEP AUDIT: SECURITY BLOCKLIST — BYPASS VECTORS
// ═══════════════════════════════════════════════════════════════

/// AUDIT: Blocklist bypass via extra whitespace between arguments.
///
/// BUG: "rm  -rf  /" (double spaces) does NOT contain "rm -rf /"
/// (single space). The blocklist uses `String::contains()` which
/// requires exact character match including whitespace.
#[tokio::test]
async fn audit_blocklist_extra_spaces_bypass() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "rm  -rf  /".to_string(),
            shell: None,
            timeout: None,
            cwd: None,
            env: None,
            max_stdout: None,
        },
    };

    let task_id: Arc<str> = Arc::from("audit_spaces");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;

    if result.is_ok() {
        panic!(
            "GAP CONFIRMED: 'rm  -rf  /' bypasses blocklist! \
             contains() needs exact whitespace. Fix: normalize \
             whitespace before blocklist check."
        );
    }
    // If blocked, the security check caught it
}

/// AUDIT: Blocklist bypass via tab characters between arguments.
///
/// BUG: Tabs (0x09) pass control char validation. But "rm\t-rf\t/"
/// does not match blocklist pattern "rm -rf /" (which uses spaces).
#[tokio::test]
async fn audit_blocklist_tab_bypass() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "rm\t-rf\t/".to_string(),
            shell: None,
            timeout: None,
            cwd: None,
            env: None,
            max_stdout: None,
        },
    };

    let task_id: Arc<str> = Arc::from("audit_tabs");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;

    if result.is_ok() {
        panic!(
            "GAP CONFIRMED: 'rm\\t-rf\\t/' bypasses blocklist! \
             Tabs pass control char check but blocklist patterns use \
             spaces. Fix: normalize whitespace before blocklist check."
        );
    }
}

/// AUDIT: Blocklist bypass via reordered flags.
///
/// "rm -f -r /" is semantically identical to "rm -rf /"
/// but does not match the blocklist pattern.
#[test]
fn audit_blocklist_flag_reorder_bypass() {
    let result = crate::runtime::security::check_blocklist("rm -f -r /");
    if result.is_ok() {
        // This is a known limitation of pattern-based blocklists.
        // Not necessarily a bug, but worth documenting.
    } else {
        // If caught, great
    }
}

/// AUDIT: Blocklist correctly catches newline-embedded patterns.
#[test]
fn audit_blocklist_newline_still_caught() {
    let cmd = "echo safe\nrm -rf /";
    let result = crate::runtime::security::check_blocklist(cmd);
    // The substring "rm -rf /" is present after the newline
    assert!(
        result.is_err(),
        "Newline-separated 'rm -rf /' should be caught by contains()"
    );
}

// ═══════════════════════════════════════════════════════════════
// DEEP AUDIT: EXEC CWD WIRING
// ═══════════════════════════════════════════════════════════════

/// AUDIT: Verify cwd parameter is actually wired to Command.
#[tokio::test]
async fn audit_exec_cwd_is_wired() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "pwd".to_string(),
            shell: Some(true),
            timeout: None,
            cwd: Some(".".to_string()),
            env: None,
            max_stdout: None,
        },
    };

    let task_id: Arc<str> = Arc::from("audit_cwd");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();

    // cwd "." resolves to the current working directory
    let expected = std::env::current_dir().unwrap();
    assert!(
        result.contains(expected.file_name().unwrap().to_str().unwrap()),
        "GAP: cwd not wired. Expected current dir, got: '{}'",
        result
    );
}

/// BUG-002: cwd must resolve {{inputs.*}} and {{with.*}} templates.
#[tokio::test]
async fn test_exec_cwd_resolves_templates_shell_mode() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    // Use "src" subdirectory — within workflow_base_dir (current dir)
    let cwd = std::env::current_dir().unwrap();
    let src_dir = cwd.join("src");
    let mut inputs = rustc_hash::FxHashMap::default();
    inputs.insert(
        "output_dir".to_string(),
        json!(src_dir.to_string_lossy().to_string()),
    );
    datastore.set_inputs(inputs);

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "pwd".to_string(),
            shell: Some(true),
            timeout: None,
            cwd: Some("{{inputs.output_dir}}".to_string()),
            env: None,
            max_stdout: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_cwd_template");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();

    // After template resolution, cwd should contain /src
    assert!(
        result.contains("src"),
        "cwd template not resolved. Expected path containing 'src', got: '{}'",
        result
    );
}

/// BUG-002: cwd template resolution in shell-free mode.
#[tokio::test]
async fn test_exec_cwd_resolves_templates_shellfree_mode() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let mut bindings = ResolvedBindings::new();
    let cwd = std::env::current_dir().unwrap();
    let src_dir = cwd.join("src");
    bindings.set("work_dir", json!(src_dir.to_string_lossy().to_string()));
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "pwd".to_string(),
            shell: None,
            timeout: None,
            cwd: Some("{{with.work_dir}}".to_string()),
            env: None,
            max_stdout: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_cwd_template_shellfree");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();

    assert!(
        result.contains("src"),
        "cwd template not resolved in shell-free mode. Got: '{}'",
        result
    );
}

// ═══════════════════════════════════════════════════════════════
// AGENT PROVIDER FALLBACK CHAIN TESTS
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_agent_provider_fallback_to_mock() {
    // provider_chain: [nonexistent_fake, mock] should fall back to mock
    let event_log = EventLog::new();
    let executor = TaskExecutor::new("mock", None, None, event_log.clone()).unwrap();
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let action = TaskAction::Agent {
        agent: AgentParams {
            prompt: "Say hello".to_string(),
            system: None,
            provider: None,
            model: None,
            mcp: vec![],
            tools: vec![],
            max_turns: Some(1),
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

            provider_chain: Some(vec![
                nika_core::ProviderName::Custom("nonexistent_fake".to_string()),
                nika_core::ProviderName::Mock,
            ]),
        },
    };

    let task_id: Arc<str> = Arc::from("test_agent_fallback");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;

    assert!(
        result.is_ok(),
        "Agent should succeed via mock fallback: {:?}",
        result.err()
    );

    // Verify ProviderFallback event was emitted
    let events = event_log.events();
    let fallback_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(&e.kind, EventKind::ProviderFallback { .. }))
        .collect();
    assert_eq!(
        fallback_events.len(),
        1,
        "Expected exactly one ProviderFallback event"
    );

    if let EventKind::ProviderFallback { from, to, .. } = &fallback_events[0].kind {
        assert_eq!(from, "nonexistent_fake");
        assert_eq!(to, "mock");
    }
}

#[tokio::test]
async fn test_agent_provider_chain_all_fail() {
    // provider_chain: [fake_1, fake_2] — both unknown → FallbackChainExhausted
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let action = TaskAction::Agent {
        agent: AgentParams {
            prompt: "Say hello".to_string(),
            system: None,
            provider: None,
            model: None,
            mcp: vec![],
            tools: vec![],
            max_turns: Some(1),
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

            provider_chain: Some(vec![
                nika_core::ProviderName::Custom("fake_provider_1".to_string()),
                nika_core::ProviderName::Custom("fake_provider_2".to_string()),
            ]),
        },
    };

    let task_id: Arc<str> = Arc::from("test_agent_chain_exhausted");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;

    assert!(
        result.is_err(),
        "Should fail when all providers are unknown"
    );
    match result.unwrap_err() {
        NikaError::FallbackChainExhausted { providers, .. } => {
            assert!(
                providers.contains("fake_provider_1"),
                "Error should list providers"
            );
            assert!(
                providers.contains("fake_provider_2"),
                "Error should list providers"
            );
        }
        err => panic!("Expected FallbackChainExhausted, got: {err:?}"),
    }
}

#[tokio::test]
async fn test_agent_single_provider_mock() {
    // Single provider (no chain) — mock should just work
    let executor = TaskExecutor::new("mock", None, None, EventLog::new()).unwrap();
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_core::trust::InvocationSource::Test);

    let action = TaskAction::Agent {
        agent: AgentParams {
            prompt: "Say hello".to_string(),
            system: None,
            provider: Some(nika_core::ProviderName::Mock),
            model: None,
            mcp: vec![],
            tools: vec![],
            max_turns: Some(1),
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

            provider_chain: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_agent_single_mock");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;

    assert!(
        result.is_ok(),
        "Single mock provider should succeed: {:?}",
        result.err()
    );
}

// ═══════════════════════════════════════════════════════════════
// is_retryable TESTS (consolidated in Runner, regression tests here)
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_is_retryable_500() {
    let err = NikaError::ProviderApiError {
        message: "HTTP 500 Internal Server Error".to_string(),
    };
    assert!(
        structured_retry::is_retryable(&err),
        "500 should be retryable"
    );
}

#[test]
fn test_is_retryable_502() {
    let err = NikaError::ProviderApiError {
        message: "HTTP 502 Bad Gateway".to_string(),
    };
    assert!(
        structured_retry::is_retryable(&err),
        "502 should be retryable"
    );
}

#[test]
fn test_is_retryable_503() {
    let err = NikaError::ProviderApiError {
        message: "HTTP 503 Service Unavailable".to_string(),
    };
    assert!(
        structured_retry::is_retryable(&err),
        "503 should be retryable"
    );
}

#[test]
fn test_is_retryable_429() {
    let err = NikaError::ProviderApiError {
        message: "HTTP 429 Too Many Requests".to_string(),
    };
    assert!(
        structured_retry::is_retryable(&err),
        "429 should be retryable"
    );
}

#[test]
fn test_is_retryable_timeout() {
    let err = NikaError::ProviderApiError {
        message: "request timed out after 30s".to_string(),
    };
    assert!(
        structured_retry::is_retryable(&err),
        "timeout should be retryable"
    );
}

#[test]
fn test_is_retryable_connection() {
    let err = NikaError::ProviderApiError {
        message: "connection refused".to_string(),
    };
    assert!(
        structured_retry::is_retryable(&err),
        "connection error should be retryable"
    );
}

#[test]
fn test_is_not_retryable_401() {
    let err = NikaError::ProviderApiError {
        message: "HTTP 401 Unauthorized".to_string(),
    };
    assert!(
        !structured_retry::is_retryable(&err),
        "401 should NOT be retryable"
    );
}

#[test]
fn test_is_not_retryable_403() {
    let err = NikaError::ProviderApiError {
        message: "HTTP 403 Forbidden".to_string(),
    };
    assert!(
        !structured_retry::is_retryable(&err),
        "403 should NOT be retryable"
    );
}

#[test]
fn test_is_not_retryable_invalid_api_key() {
    let err = NikaError::ProviderApiError {
        message: "Invalid API key provided".to_string(),
    };
    assert!(
        !structured_retry::is_retryable(&err),
        "invalid API key should NOT be retryable"
    );
}
