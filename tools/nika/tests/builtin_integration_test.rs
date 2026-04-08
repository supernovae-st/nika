// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Integration tests for builtin nika:* tools (v0.9.3)
//!
//! Tests the full invoke path through TaskExecutor for builtin tools.

use nika::ast::{InvokeParams, TaskAction};
use nika::binding::ResolvedBindings;
use nika::event::EventLog;
use nika::runtime::TaskExecutor;
use nika::store::RunContext;
use serde_json::json;
use std::sync::Arc;

fn create_executor() -> TaskExecutor {
    TaskExecutor::new("mock", None, None, EventLog::new()).unwrap()
}

fn create_invoke_action(tool: &str, params: serde_json::Value) -> TaskAction {
    TaskAction::Invoke {
        invoke: InvokeParams {
            mcp: Some("builtin".to_string()), // For builtin tools, mcp value is ignored
            tool: Some(tool.to_string()),
            params: Some(params),
            resource: None,
            timeout: None,
        },
    }
}

// ═══════════════════════════════════════════
// nika:sleep integration tests
// ═══════════════════════════════════════════

#[tokio::test]
async fn test_executor_invoke_nika_sleep() {
    let executor = create_executor();
    let task_id: Arc<str> = "test-sleep".into();
    let action = create_invoke_action("nika:sleep", json!({"duration": "1ms"}));
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::new(nika::trust::InvocationSource::Test);

    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;

    assert!(result.is_ok(), "Expected Ok, got {:?}", result);
    let output = result.unwrap();
    let value: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(value["slept_for_ms"], 1);
}

// ═══════════════════════════════════════════
// nika:log integration tests
// ═══════════════════════════════════════════

#[tokio::test]
async fn test_executor_invoke_nika_log() {
    let executor = create_executor();
    let task_id: Arc<str> = "test-log".into();
    let action = create_invoke_action(
        "nika:log",
        json!({"level": "info", "message": "Integration test"}),
    );
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::new(nika::trust::InvocationSource::Test);

    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;

    assert!(result.is_ok(), "Expected Ok, got {:?}", result);
    let output = result.unwrap();
    let value: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(value["logged"], true);
    assert_eq!(value["level"], "info");
}

// ═══════════════════════════════════════════
// nika:emit integration tests
// ═══════════════════════════════════════════

#[tokio::test]
async fn test_executor_invoke_nika_emit() {
    let executor = create_executor();
    let task_id: Arc<str> = "test-emit".into();
    let action = create_invoke_action(
        "nika:emit",
        json!({"name": "test_event", "payload": {"key": "value"}}),
    );
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::new(nika::trust::InvocationSource::Test);

    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;

    assert!(result.is_ok(), "Expected Ok, got {:?}", result);
    let output = result.unwrap();
    let value: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(value["emitted"], true);
    assert_eq!(value["name"], "test_event");
}

// ═══════════════════════════════════════════
// nika:assert integration tests
// ═══════════════════════════════════════════

#[tokio::test]
async fn test_executor_invoke_nika_assert_true() {
    let executor = create_executor();
    let task_id: Arc<str> = "test-assert".into();
    let action = create_invoke_action("nika:assert", json!({"condition": true}));
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::new(nika::trust::InvocationSource::Test);

    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;

    assert!(result.is_ok(), "Expected Ok, got {:?}", result);
    let output = result.unwrap();
    let value: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(value["passed"], true);
}

#[tokio::test]
async fn test_executor_invoke_nika_assert_false() {
    let executor = create_executor();
    let task_id: Arc<str> = "test-assert".into();
    let action = create_invoke_action(
        "nika:assert",
        json!({"condition": false, "message": "Expected true"}),
    );
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::new(nika::trust::InvocationSource::Test);

    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("Expected true"));
}

// ═══════════════════════════════════════════
// nika:prompt integration tests
// ═══════════════════════════════════════════

#[tokio::test]
async fn test_executor_invoke_nika_prompt_with_default() {
    let executor = create_executor();
    let task_id: Arc<str> = "test-prompt".into();
    let action = create_invoke_action(
        "nika:prompt",
        json!({"message": "Confirm?", "default": "yes"}),
    );
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::new(nika::trust::InvocationSource::Test);

    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;

    assert!(result.is_ok(), "Expected Ok, got {:?}", result);
    let output = result.unwrap();
    let value: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(value["response"], "yes");
    assert_eq!(value["default_used"], true);
}

#[tokio::test]
async fn test_executor_invoke_nika_prompt_without_default_errors() {
    let executor = create_executor();
    let task_id: Arc<str> = "test-prompt".into();
    let action = create_invoke_action("nika:prompt", json!({"message": "Input needed"}));
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::new(nika::trust::InvocationSource::Test);

    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;

    // In headless mode, prompt without default should error
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("headless mode"));
}

// ═══════════════════════════════════════════
// nika:run integration tests
// ═══════════════════════════════════════════

#[tokio::test]
async fn test_executor_invoke_nika_run_nonexistent_errors() {
    // v0.10.0: nika:run now executes real workflows, so missing files error
    let executor = create_executor();
    let task_id: Arc<str> = "test-run".into();
    let action = create_invoke_action("nika:run", json!({"workflow": "test.nika.yaml"}));
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::new(nika::trust::InvocationSource::Test);

    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;

    assert!(result.is_err(), "Expected Err for non-existent workflow");
    let err = result.unwrap_err();
    // v0.14.0: Path canonicalization gives "resolve workflow path" error for missing files
    assert!(
        err.to_string().contains("resolve workflow path") || err.to_string().contains("not found"),
        "Expected error, got: {}",
        err
    );
}

#[tokio::test]
async fn test_executor_invoke_nika_run_executes_workflow() {
    use std::io::Write;
    use tempfile::NamedTempFile;

    // Create a minimal workflow file
    let mut temp_file = NamedTempFile::with_suffix(".nika.yaml").unwrap();
    writeln!(
        temp_file,
        r#"schema: nika/workflow@0.12
workflow: integration-test
tasks:
  - id: hello
    exec: "echo integration""#
    )
    .unwrap();

    let executor = create_executor();
    let task_id: Arc<str> = "test-run".into();
    let action = create_invoke_action(
        "nika:run",
        json!({"workflow": temp_file.path().to_str().unwrap()}),
    );
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::new(nika::trust::InvocationSource::Test);

    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;

    assert!(result.is_ok(), "Expected Ok, got {:?}", result);
    let output = result.unwrap();
    let value: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(value["executed"], true);
}

#[tokio::test]
async fn test_executor_invoke_nika_run_invalid_extension() {
    let executor = create_executor();
    let task_id: Arc<str> = "test-run".into();
    let action = create_invoke_action("nika:run", json!({"workflow": "test.yaml"}));
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::new(nika::trust::InvocationSource::Test);

    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains(".nika.yaml"));
}

// ═══════════════════════════════════════════
// Template resolution in builtin tools
// ═══════════════════════════════════════════

#[tokio::test]
async fn test_executor_invoke_builtin_with_template_resolution() {
    let executor = create_executor();
    let task_id: Arc<str> = "test-template".into();

    // Create invoke with template in params
    let action = TaskAction::Invoke {
        invoke: InvokeParams {
            mcp: Some("builtin".to_string()),
            tool: Some("nika:log".to_string()),
            params: Some(json!({"level": "info", "message": "Value: {{with.value}}"})),
            resource: None,
            timeout: None,
        },
    };

    // Create bindings with the value
    let mut bindings = ResolvedBindings::default();
    bindings.set("value", json!("resolved_value"));

    let datastore = RunContext::new(nika::trust::InvocationSource::Test);

    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;

    assert!(result.is_ok(), "Expected Ok, got {:?}", result);
    let output = result.unwrap();
    let value: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(value["logged"], true);
    // The message should have been resolved
    assert_eq!(value["message"], "Value: resolved_value");
}

// ═══════════════════════════════════════════
// Event emission verification
// ═══════════════════════════════════════════

#[tokio::test]
async fn test_executor_invoke_builtin_emits_events() {
    let event_log = EventLog::new();
    let executor = TaskExecutor::new("mock", None, None, event_log.clone()).unwrap();

    let task_id: Arc<str> = "test-events".into();
    let action = create_invoke_action("nika:sleep", json!({"duration": "1ms"}));
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::new(nika::trust::InvocationSource::Test);

    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;

    assert!(result.is_ok());

    // Check that McpInvoke and McpResponse events were emitted
    let events = event_log.events();

    // Find McpInvoke event
    let invoke_event = events.iter().find(|e| {
        matches!(&e.kind, nika::event::EventKind::McpInvoke { tool, .. } if tool == &Some("nika:sleep".to_string()))
    });
    assert!(
        invoke_event.is_some(),
        "McpInvoke event should be emitted for builtin tools"
    );

    // Find McpResponse event
    let response_event = events.iter().find(|e| {
        matches!(
            &e.kind,
            nika::event::EventKind::McpResponse { is_error, .. } if !*is_error
        )
    });
    assert!(
        response_event.is_some(),
        "McpResponse event should be emitted for builtin tools"
    );
}

// ═══════════════════════════════════════════
// All 6 builtin tools via router
// ═══════════════════════════════════════════

#[tokio::test]
async fn test_all_builtin_tools_accessible() {
    let router = nika::runtime::BuiltinToolRouter::new();

    // Verify all 6 tools are registered
    assert!(router.has_tool("sleep"), "sleep tool should be registered");
    assert!(router.has_tool("log"), "log tool should be registered");
    assert!(router.has_tool("emit"), "emit tool should be registered");
    assert!(
        router.has_tool("assert"),
        "assert tool should be registered"
    );
    assert!(
        router.has_tool("prompt"),
        "prompt tool should be registered"
    );
    assert!(router.has_tool("run"), "run tool should be registered");

    // Verify is_builtin works for all
    assert!(nika::runtime::BuiltinToolRouter::is_builtin("nika:sleep"));
    assert!(nika::runtime::BuiltinToolRouter::is_builtin("nika:log"));
    assert!(nika::runtime::BuiltinToolRouter::is_builtin("nika:emit"));
    assert!(nika::runtime::BuiltinToolRouter::is_builtin("nika:assert"));
    assert!(nika::runtime::BuiltinToolRouter::is_builtin("nika:prompt"));
    assert!(nika::runtime::BuiltinToolRouter::is_builtin("nika:run"));

    // Verify non-builtin detection
    assert!(!nika::runtime::BuiltinToolRouter::is_builtin(
        "novanet_describe"
    ));
    assert!(!nika::runtime::BuiltinToolRouter::is_builtin("sleep")); // Missing nika: prefix
}
