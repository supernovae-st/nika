//! Integration tests for the Nika CLI
//!
//! These tests run the actual CLI binary and verify output.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Get the binary to test
fn nika_cmd() -> Command {
    Command::cargo_bin("nika").unwrap()
}

#[test]
fn test_no_args_shows_banner() {
    nika_cmd()
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Native Intelligence Kernel for Agents",
        ))
        .stdout(predicate::str::contains("v0.1.0"));
}

#[test]
fn test_help_flag() {
    nika_cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "CLI for Nika workflow orchestration",
        ));
}

#[test]
fn test_validate_help() {
    nika_cmd()
        .args(["validate", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--format"))
        .stdout(predicate::str::contains("--rules"));
}

#[test]
fn test_validate_valid_workflow() {
    // Create temp directory with test files
    let temp_dir = TempDir::new().unwrap();
    let rules_dir = temp_dir.path().join("rules");
    let workflow_dir = temp_dir.path().join("workflows");

    fs::create_dir_all(&rules_dir).unwrap();
    fs::create_dir_all(&workflow_dir).unwrap();

    // Create minimal rule files
    fs::write(
        rules_dir.join("node-types.yaml"),
        r#"
version: "1.0"
description: "Test types"
lookup:
  context: context
  nika/transform: data
"#,
    )
    .unwrap();

    fs::write(
        rules_dir.join("paradigm-matrix.yaml"),
        r#"
version: "1.0"
description: "Test matrix"
paradigms:
  context:
    symbol: "🧠"
    description: "Context"
    color: "violet"
    border: "solid"
    sdk_mapping: "query()"
    token_cost: "500+"
  data:
    symbol: "⚡"
    description: "Data"
    color: "cyan"
    border: "thin"
    sdk_mapping: "@tool"
    token_cost: "0"
connections:
  context:
    context: true
    data: true
  data:
    context: true
    data: true
"#,
    )
    .unwrap();

    // Create valid workflow
    fs::write(
        workflow_dir.join("test.wf.yaml"),
        r#"
mainAgent:
  model: "claude-sonnet-4-5"
  systemPrompt: "Test"
nodes:
  - id: prompt1
    type: context
  - id: transform1
    type: nika/transform
edges:
  - source: prompt1
    target: transform1
"#,
    )
    .unwrap();

    nika_cmd()
        .args([
            "validate",
            workflow_dir.to_str().unwrap(),
            "--rules",
            rules_dir.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("✅"))
        .stdout(predicate::str::contains("2 nodes"));
}

#[test]
fn test_validate_invalid_workflow() {
    let temp_dir = TempDir::new().unwrap();
    let rules_dir = temp_dir.path().join("rules");
    let workflow_dir = temp_dir.path().join("workflows");

    fs::create_dir_all(&rules_dir).unwrap();
    fs::create_dir_all(&workflow_dir).unwrap();

    // Create rule files with isolated paradigm
    fs::write(
        rules_dir.join("node-types.yaml"),
        r#"
version: "1.0"
description: "Test types"
lookup:
  context: context
  isolated: isolated
"#,
    )
    .unwrap();

    fs::write(
        rules_dir.join("paradigm-matrix.yaml"),
        r#"
version: "1.0"
description: "Test matrix"
paradigms:
  context:
    symbol: "🧠"
    description: "Context"
    color: "violet"
    border: "solid"
    sdk_mapping: "query()"
    token_cost: "500+"
  isolated:
    symbol: "🤖"
    description: "Isolated"
    color: "amber"
    border: "dashed"
    sdk_mapping: "agents"
    token_cost: "8000+"
connections:
  context:
    context: true
    isolated: true
  isolated:
    context: false
    isolated: false
"#,
    )
    .unwrap();

    // Create invalid workflow (isolated -> context)
    fs::write(
        workflow_dir.join("invalid.wf.yaml"),
        r#"
mainAgent:
  model: "claude-sonnet-4-5"
  systemPrompt: "Test"
nodes:
  - id: expert1
    type: isolated
  - id: prompt1
    type: context
edges:
  - source: expert1
    target: prompt1
"#,
    )
    .unwrap();

    nika_cmd()
        .args([
            "validate",
            workflow_dir.to_str().unwrap(),
            "--rules",
            rules_dir.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("❌"))
        .stdout(predicate::str::contains("Invalid connection"))
        .stdout(predicate::str::contains("bridge pattern"));
}

#[test]
fn test_validate_json_output() {
    let temp_dir = TempDir::new().unwrap();
    let rules_dir = temp_dir.path().join("rules");
    let workflow_file = temp_dir.path().join("test.wf.yaml");

    fs::create_dir_all(&rules_dir).unwrap();

    fs::write(
        rules_dir.join("node-types.yaml"),
        r#"
version: "1.0"
description: "Test"
lookup:
  context: context
"#,
    )
    .unwrap();

    fs::write(
        rules_dir.join("paradigm-matrix.yaml"),
        r#"
version: "1.0"
description: "Test"
paradigms:
  context:
    symbol: "🧠"
    description: "Context"
    color: "violet"
    border: "solid"
    sdk_mapping: "query()"
    token_cost: "500+"
connections:
  context:
    context: true
"#,
    )
    .unwrap();

    fs::write(
        &workflow_file,
        r#"
mainAgent:
  model: "claude-sonnet-4-5"
  systemPrompt: "Test"
nodes:
  - id: prompt1
    type: context
edges: []
"#,
    )
    .unwrap();

    nika_cmd()
        .args([
            "validate",
            workflow_file.to_str().unwrap(),
            "--rules",
            rules_dir.to_str().unwrap(),
            "--format",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"valid\": true"))
        .stdout(predicate::str::contains("\"node_count\": 1"));
}

#[test]
fn test_validate_missing_rules() {
    nika_cmd()
        .args(["validate", ".", "--rules", "/nonexistent/path"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Failed to load"));
}

#[test]
fn test_validate_invalid_isolated_to_context() {
    nika_cmd()
        .args([
            "validate",
            "../spec/examples/invalid-isolated-to-context.wf.yaml",
            "--rules",
            "../spec/validation",
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("❌"))
        .stdout(predicate::str::contains("Invalid connection"));
}

#[test]
fn test_validate_valid_bridge_pattern() {
    nika_cmd()
        .args([
            "validate",
            "../spec/examples/valid-bridge-pattern.wf.yaml",
            "--rules",
            "../spec/validation",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("✅"))
        .stdout(predicate::str::contains("3 nodes"));
}

#[test]
fn test_validate_all_paradigms() {
    nika_cmd()
        .args([
            "validate",
            "../spec/examples/all-paradigms.wf.yaml",
            "--rules",
            "../spec/validation",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("✅"))
        .stdout(predicate::str::contains("9 nodes"));
}
