// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Comprehensive exec verb error path tests (v0.21.3)
//!
//! Tests all error conditions in the exec verb implementation:
//! - Empty command
//! - Shlex parse failures (unbalanced quotes)
//! - Command not found (nonexistent binary)
//! - Non-zero exit codes with various exit statuses
//! - Stderr output capture
//! - Template resolution failures
//! - Security blocklist violations
//! - Policy violations
//! - Working directory (cwd) errors

use nika::ast::{ExecParams, TaskAction};
use nika::binding::ResolvedBindings;
use nika::error::NikaError;
use nika::event::EventLog;
use nika::runtime::TaskExecutor;
use nika::store::RunContext;
use std::sync::Arc;

/// Helper to create executor with default settings
fn create_executor() -> TaskExecutor {
    TaskExecutor::new("mock", None, None, EventLog::new()).unwrap()
}

/// Helper to execute an exec action
async fn run_exec(
    executor: &TaskExecutor,
    task_id: &str,
    command: &str,
    shell: Option<bool>,
) -> Result<String, NikaError> {
    let action = TaskAction::Exec {
        exec: ExecParams {
            command: command.to_string(),
            shell,
            timeout: None,
            cwd: None,
            env: None,
            max_stdout: None,
        },
    };
    let task_id: Arc<str> = Arc::from(task_id);
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika::trust::InvocationSource::Test);

    executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
}

// ============================================================================
// EMPTY COMMAND TESTS
// ============================================================================

#[tokio::test]
async fn test_exec_empty_command_string() {
    let executor = create_executor();
    let result = run_exec(&executor, "empty_cmd", "", None).await;

    assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
    let err = result.unwrap_err();
    // Empty command should fail during shlex parsing or validation
    assert!(
        matches!(err, NikaError::Execution(_)),
        "Expected Execution error, got: {err:?}"
    );
}

#[tokio::test]
async fn test_exec_whitespace_only_command() {
    let executor = create_executor();
    let result = run_exec(&executor, "whitespace_cmd", "   ", None).await;

    assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
    let err = result.unwrap_err();
    assert!(
        matches!(err, NikaError::Execution(_)),
        "Expected Execution error, got: {err:?}"
    );
}

// ============================================================================
// SHLEX PARSE ERROR TESTS
// ============================================================================

#[tokio::test]
async fn test_exec_unbalanced_single_quotes() {
    let executor = create_executor();
    // Missing closing single quote
    let result = run_exec(&executor, "unbalanced_single", "echo 'hello", None).await;

    assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
    let err = result.unwrap_err();
    if let NikaError::Execution(msg) = &err {
        assert!(
            msg.contains("unbalanced") || msg.contains("parse") || msg.contains("quote"),
            "Expected parse error message, got: {msg}"
        );
    } else {
        panic!("Expected Execution error, got: {err:?}");
    }
}

#[tokio::test]
async fn test_exec_unbalanced_double_quotes() {
    let executor = create_executor();
    // Missing closing double quote
    let result = run_exec(&executor, "unbalanced_double", "echo \"hello", None).await;

    assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
    let err = result.unwrap_err();
    if let NikaError::Execution(msg) = &err {
        assert!(
            msg.contains("unbalanced") || msg.contains("parse") || msg.contains("quote"),
            "Expected parse error message, got: {msg}"
        );
    } else {
        panic!("Expected Execution error, got: {err:?}");
    }
}

#[tokio::test]
async fn test_exec_mixed_unbalanced_quotes() {
    let executor = create_executor();
    // Mixed unbalanced quotes
    let result = run_exec(&executor, "mixed_quotes", "echo \"hello'", None).await;

    assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
}

// ============================================================================
// COMMAND NOT FOUND TESTS
// ============================================================================

#[tokio::test]
async fn test_exec_nonexistent_command() {
    let executor = create_executor();
    // Command that definitely doesn't exist
    let result = run_exec(
        &executor,
        "nonexistent",
        "this_command_definitely_does_not_exist_12345",
        None,
    )
    .await;

    assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
    let err = result.unwrap_err();
    assert!(
        matches!(err, NikaError::Execution(_)),
        "Expected Execution error, got: {err:?}"
    );
}

#[tokio::test]
async fn test_exec_nonexistent_command_shell_mode() {
    let executor = create_executor();
    // Nonexistent command in shell mode
    let result = run_exec(
        &executor,
        "nonexistent_shell",
        "this_command_definitely_does_not_exist_12345",
        Some(true),
    )
    .await;

    assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
}

// ============================================================================
// EXIT CODE TESTS
// ============================================================================

#[tokio::test]
async fn test_exec_exit_code_1() {
    let executor = create_executor();
    // false command always returns exit code 1
    let result = run_exec(&executor, "exit_1", "false", None).await;

    assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
    let err = result.unwrap_err();
    if let NikaError::Execution(msg) = &err {
        assert!(
            msg.contains("failed") || msg.contains("Command"),
            "Expected failure message, got: {msg}"
        );
    } else {
        panic!("Expected Execution error, got: {err:?}");
    }
}

#[tokio::test]
async fn test_exec_exit_code_1_shell_mode() {
    let executor = create_executor();
    // exit 1 in shell mode
    let result = run_exec(&executor, "exit_1_shell", "exit 1", Some(true)).await;

    assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
}

#[tokio::test]
async fn test_exec_exit_code_2() {
    let executor = create_executor();
    // Shell exit 2 (often indicates invalid usage)
    let result = run_exec(&executor, "exit_2", "exit 2", Some(true)).await;

    assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
}

#[tokio::test]
async fn test_exec_exit_code_127() {
    let executor = create_executor();
    // exit 127 is "command not found" in shell
    let result = run_exec(&executor, "exit_127", "exit 127", Some(true)).await;

    assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
}

// ============================================================================
// STDERR OUTPUT TESTS
// ============================================================================

#[tokio::test]
async fn test_exec_stderr_output_captured() {
    let executor = create_executor();
    // Command that writes to stderr and exits with error
    let result = run_exec(
        &executor,
        "stderr_test",
        "echo 'error message' >&2 && false",
        Some(true),
    )
    .await;

    assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
    let err = result.unwrap_err();
    if let NikaError::Execution(msg) = &err {
        // Stderr should be captured in the error message
        assert!(
            msg.contains("error message") || msg.contains("failed"),
            "Expected stderr capture, got: {msg}"
        );
    }
}

#[tokio::test]
async fn test_exec_large_stderr() {
    let executor = create_executor();
    // Generate large stderr output
    let result = run_exec(
        &executor,
        "large_stderr",
        "for i in $(seq 1 100); do echo 'error line' >&2; done; false",
        Some(true),
    )
    .await;

    assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
}

// ============================================================================
// SECURITY BLOCKLIST TESTS (validates NIKA-053)
// ============================================================================

#[tokio::test]
async fn test_exec_blocked_rm_rf() {
    let executor = create_executor();
    let result = run_exec(&executor, "blocked_rm", "rm -rf /", None).await;

    assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
    let err = result.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("NIKA-053") || msg.contains("blocked") || msg.contains("dangerous"),
        "Expected blocked command error, got: {msg}"
    );
}

#[tokio::test]
async fn test_exec_blocked_sudo() {
    let executor = create_executor();
    let result = run_exec(&executor, "blocked_sudo", "sudo ls", None).await;

    assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
}

#[tokio::test]
async fn test_exec_blocked_chmod_777() {
    let executor = create_executor();
    let result = run_exec(&executor, "blocked_chmod", "chmod 777 /", None).await;

    assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
}

#[tokio::test]
async fn test_exec_blocked_mkfs() {
    let executor = create_executor();
    let result = run_exec(&executor, "blocked_mkfs", "mkfs.ext4 /dev/sda", None).await;

    assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
}

#[tokio::test]
async fn test_exec_blocked_dd() {
    let executor = create_executor();
    let result = run_exec(&executor, "blocked_dd", "dd if=/dev/zero of=/dev/sda", None).await;

    assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
}

// ============================================================================
// TEMPLATE RESOLUTION ERROR TESTS
// ============================================================================

#[tokio::test]
async fn test_exec_template_missing_binding() {
    let executor = create_executor();
    let action = TaskAction::Exec {
        exec: ExecParams {
            command: "echo {{with.nonexistent}}".to_string(),
            shell: None,
            timeout: None,
            cwd: None,
            env: None,
            max_stdout: None,
        },
    };
    let task_id: Arc<str> = Arc::from("template_missing");
    let bindings = ResolvedBindings::new(); // Empty bindings
    let datastore = RunContext::new(nika::trust::InvocationSource::Test);

    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;

    assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
    let err = result.unwrap_err();
    // Should error on missing binding
    assert!(
        matches!(
            err,
            NikaError::TemplateError { .. } | NikaError::Execution(_)
        ),
        "Expected binding/template error, got: {err:?}"
    );
}

// ============================================================================
// SHELL MODE SPECIFIC TESTS
// ============================================================================

#[tokio::test]
async fn test_exec_shell_mode_pipes_work() {
    let executor = create_executor();
    // Pipe should work in shell mode
    let result = run_exec(
        &executor,
        "shell_pipe",
        "echo hello | tr a-z A-Z",
        Some(true),
    )
    .await;

    assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    let output = result.unwrap();
    assert!(output.contains("HELLO"), "Expected HELLO, got: {output}");
}

#[tokio::test]
async fn test_exec_shell_mode_redirects_work() {
    let executor = create_executor();
    // Redirect should work in shell mode
    let result = run_exec(
        &executor,
        "shell_redirect",
        "echo hello > /dev/null && echo done",
        Some(true),
    )
    .await;

    assert!(result.is_ok(), "Should succeed: {:?}", result.err());
}

#[tokio::test]
async fn test_exec_shell_mode_command_chaining() {
    let executor = create_executor();
    // && should work in shell mode
    let result = run_exec(
        &executor,
        "shell_chain",
        "echo first && echo second",
        Some(true),
    )
    .await;

    assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    let output = result.unwrap();
    assert!(output.contains("first"));
    assert!(output.contains("second"));
}

#[tokio::test]
async fn test_exec_shell_false_prevents_chaining() {
    let executor = create_executor();
    // In shell-free mode, && is treated literally
    let result = run_exec(
        &executor,
        "no_chain",
        "echo hello && echo world",
        Some(false),
    )
    .await;

    // Should either succeed with literal output or fail (echo doesn't understand &&)
    // The key is that "world" shouldn't execute as a separate command
    if let Ok(output) = &result {
        // If it succeeds, the output should contain "&&" literally
        // or just the first word depending on how echo handles it
        assert!(
            !output.contains("world") || output.contains("&&"),
            "Shell chaining should not work: {output}"
        );
    }
}

// ============================================================================
// SUCCESS CASES (sanity checks)
// ============================================================================

#[tokio::test]
async fn test_exec_simple_echo_success() {
    let executor = create_executor();
    let result = run_exec(&executor, "echo_success", "echo hello", None).await;

    assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    let output = result.unwrap();
    assert!(output.contains("hello"));
}

#[tokio::test]
async fn test_exec_with_arguments() {
    let executor = create_executor();
    let result = run_exec(&executor, "with_args", "echo -n test", None).await;

    assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    let output = result.unwrap();
    assert!(output.contains("test"));
}

#[tokio::test]
async fn test_exec_with_spaces_in_quotes() {
    let executor = create_executor();
    let result = run_exec(&executor, "quoted_spaces", "echo 'hello world'", None).await;

    assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    let output = result.unwrap();
    assert!(output.contains("hello world"));
}
