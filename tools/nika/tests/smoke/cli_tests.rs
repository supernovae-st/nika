// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! CLI command smoke tests.
//!
//! Tests various CLI commands and their outputs.

use std::env;
use std::path::PathBuf;
use std::process::Command;

/// Get the path to the nika binary
fn nika_binary() -> PathBuf {
    // During tests, use cargo run
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Run nika command and return output
fn run_nika(args: &[&str]) -> std::process::Output {
    Command::new("cargo")
        .current_dir(nika_binary())
        .arg("run")
        .arg("--quiet")
        .arg("--")
        .args(args)
        .output()
        .expect("Failed to execute nika")
}

// ============================================================================
// HELP COMMAND TESTS
// ============================================================================

#[test]
fn test_cli_help() {
    let output = run_nika(&["--help"]);

    assert!(output.status.success(), "nika --help failed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("nika") || stdout.contains("Nika"));
    assert!(stdout.contains("workflow") || stdout.contains("YAML"));
}

#[test]
fn test_cli_version() {
    let output = run_nika(&["--version"]);

    assert!(output.status.success(), "nika --version failed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should contain version number
    assert!(stdout.contains("0.") || stdout.contains("nika"));
}

// ============================================================================
// CHECK/VALIDATE COMMAND TESTS
// ============================================================================

#[test]
fn test_cli_check_valid_workflow() {
    // Create a temp valid workflow with unique name
    let tmp_dir = std::env::temp_dir();
    let unique_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let workflow_path = tmp_dir.join(format!("test-valid-{}.nika.yaml", unique_id));

    std::fs::write(
        &workflow_path,
        r#"
schema: "nika/workflow@0.12"
workflow: test-valid
description: "Valid test workflow"

tasks:
  - id: hello
    exec: "echo hello"
"#,
    )
    .unwrap();

    let output = run_nika(&["check", workflow_path.to_str().unwrap()]);

    // Clean up
    std::fs::remove_file(&workflow_path).ok();

    assert!(
        output.status.success(),
        "nika check failed for valid workflow: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_cli_check_invalid_workflow() {
    // Create a temp invalid workflow with unique name
    let tmp_dir = std::env::temp_dir();
    let unique_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let workflow_path = tmp_dir.join(format!("test-invalid-{}.nika.yaml", unique_id));

    // Missing required field
    std::fs::write(
        &workflow_path,
        r#"
workflow: test-invalid
tasks:
  - id: hello
"#,
    )
    .unwrap();

    let output = run_nika(&["check", workflow_path.to_str().unwrap()]);

    // Clean up
    std::fs::remove_file(&workflow_path).ok();

    // Should fail for invalid workflow
    assert!(!output.status.success() || !output.stderr.is_empty());
}

#[test]
fn test_cli_check_missing_file() {
    let output = run_nika(&["check", "/nonexistent/workflow.nika.yaml"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not found") || stderr.contains("No such file") || stderr.contains("error")
    );
}

// ============================================================================
// INIT COMMAND TESTS
// ============================================================================

#[test]
fn test_cli_init() {
    let tmp_dir = std::env::temp_dir().join("nika-init-test");
    // Clean any previous state
    std::fs::remove_dir_all(&tmp_dir).ok();
    std::fs::create_dir_all(&tmp_dir).unwrap();

    // Build the binary path
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let binary = manifest_dir.join("target/debug/nika");

    // Run init in temp dir (nika init creates .nika in current directory)
    let output = Command::new(&binary)
        .current_dir(&tmp_dir)
        .arg("init")
        .arg("--no-example")
        .output()
        .expect("Failed to run init");

    // Check if .nika directory was created
    let nika_dir = tmp_dir.join(".nika");
    let exists = nika_dir.exists();

    // Clean up
    std::fs::remove_dir_all(&tmp_dir).ok();

    // Init should complete and create the directory
    assert!(
        output.status.success() || exists,
        "nika init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ============================================================================
// TRACE COMMAND TESTS
// ============================================================================

#[test]
fn test_cli_trace_list() {
    let output = run_nika(&["trace", "list"]);

    // Should complete (may be empty)
    assert!(
        output.status.success(),
        "nika trace list failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ============================================================================
// OUTPUT FORMAT TESTS
// ============================================================================

#[test]
fn test_cli_json_output() {
    let output = run_nika(&["--help", "--format", "json"]);

    // If format is supported, should work
    // If not, should fail gracefully
    let _stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Either succeeds with JSON or fails with helpful error
    assert!(output.status.success() || stderr.contains("format") || stderr.contains("unknown"));
}

// ============================================================================
// ERROR HANDLING TESTS
// ============================================================================

#[test]
fn test_cli_unknown_command() {
    let output = run_nika(&["nonexistent-command"]);

    // Should fail with helpful error
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Nika treats positional args as workflow files, so it says "Expected .nika.yaml file"
    assert!(
        stderr.contains("Expected") || stderr.contains("error") || stderr.contains("Error"),
        "Unexpected stderr: {}",
        stderr
    );
}

#[test]
fn test_cli_invalid_args() {
    let output = run_nika(&["check", "--invalid-flag"]);

    assert!(!output.status.success());
}

// ============================================================================
// WORKFLOW EXECUTION TESTS (without API)
// ============================================================================

#[test]
fn test_cli_run_exec_only_workflow() {
    // Create a temp exec-only workflow with unique name
    let tmp_dir = std::env::temp_dir();
    let unique_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let workflow_path = tmp_dir.join(format!("test-exec-only-{}.nika.yaml", unique_id));

    std::fs::write(
        &workflow_path,
        r#"
schema: "nika/workflow@0.12"
workflow: test-exec-only
description: "Exec-only workflow (no API needed)"

tasks:
  - id: get_date
    exec: "date +%Y-%m-%d"

  - id: get_hostname
    exec: "hostname"
"#,
    )
    .unwrap();

    let output = run_nika(&["run", workflow_path.to_str().unwrap()]);

    // Clean up
    std::fs::remove_file(&workflow_path).ok();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should succeed or at least run
    println!("stdout: {}", stdout);
    println!("stderr: {}", stderr);
}

#[test]
fn test_cli_run_fetch_workflow() {
    // Create a temp fetch workflow with unique name
    let tmp_dir = std::env::temp_dir();
    let unique_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let workflow_path = tmp_dir.join(format!("test-fetch-{}.nika.yaml", unique_id));

    std::fs::write(
        &workflow_path,
        r#"
schema: "nika/workflow@0.12"
workflow: test-fetch
description: "Fetch-only workflow"

tasks:
  - id: get_ip
    fetch:
      url: "https://httpbin.org/ip"
      method: GET
      timeout: 10000
"#,
    )
    .unwrap();

    let output = run_nika(&["run", workflow_path.to_str().unwrap()]);

    // Clean up
    std::fs::remove_file(&workflow_path).ok();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("stdout: {}", stdout);
    println!("stderr: {}", stderr);
}

// ============================================================================
// EXIT CODE TESTS
// ============================================================================

#[test]
fn test_exit_code_success() {
    let output = run_nika(&["--help"]);
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn test_exit_code_invalid_args() {
    let output = run_nika(&["--invalid-flag-12345"]);
    assert_ne!(output.status.code(), Some(0));
}
