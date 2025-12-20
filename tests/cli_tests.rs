//! Integration tests for the Nika CLI (v3)
//!
//! These tests run the actual CLI binary and verify output.
//! Architecture v3: 2 task types (agent + action) with scope attribute.

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
        .stdout(predicate::str::contains("v0.1.0"))
        .stdout(predicate::str::contains("Architecture v3"));
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
        .stdout(predicate::str::contains("--verbose"));
}

// ============================================================================
// v3 Workflow Validation Tests
// ============================================================================

#[test]
fn test_validate_valid_hello_world() {
    let temp_dir = TempDir::new().unwrap();
    let workflow_file = temp_dir.path().join("hello.nika.yaml");

    // v3 hello world workflow
    fs::write(
        &workflow_file,
        r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "You are a helpful assistant."

tasks:
  - id: greet
    type: agent
    prompt: "Say hello in French."

flows: []
"#,
    )
    .unwrap();

    nika_cmd()
        .args(["validate", workflow_file.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("1 tasks"));
}

#[test]
fn test_validate_agent_main_to_action() {
    let temp_dir = TempDir::new().unwrap();
    let workflow_file = temp_dir.path().join("test.nika.yaml");

    fs::write(
        &workflow_file,
        r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: analyze
    type: agent
    prompt: "Analyze the code."
  - id: save
    type: action
    run: Write
    file: "output.txt"

flows:
  - source: analyze
    target: save
"#,
    )
    .unwrap();

    nika_cmd()
        .args(["validate", workflow_file.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("2 tasks"));
}

#[test]
fn test_validate_bridge_pattern() {
    let temp_dir = TempDir::new().unwrap();
    let workflow_file = temp_dir.path().join("bridge.nika.yaml");

    // v3 bridge pattern: agent(isolated) -> action -> agent(main)
    fs::write(
        &workflow_file,
        r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "Orchestrator"

tasks:
  - id: worker
    type: agent
    scope: isolated
    prompt: "Do deep work."
  - id: bridge
    type: action
    run: aggregate
  - id: router
    type: agent
    prompt: "Route results."

flows:
  - source: worker
    target: bridge
  - source: bridge
    target: router
"#,
    )
    .unwrap();

    nika_cmd()
        .args(["validate", workflow_file.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("3 tasks"));
}

#[test]
fn test_validate_invalid_isolated_to_main() {
    let temp_dir = TempDir::new().unwrap();
    let workflow_file = temp_dir.path().join("invalid.nika.yaml");

    // v3 invalid: agent(isolated) -> agent(main) is BLOCKED
    fs::write(
        &workflow_file,
        r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: worker
    type: agent
    scope: isolated
    prompt: "Work"
  - id: router
    type: agent
    prompt: "Route"

flows:
  - source: worker
    target: router
"#,
    )
    .unwrap();

    nika_cmd()
        .args(["validate", workflow_file.to_str().unwrap()])
        .assert()
        .failure()
        .stdout(predicate::str::contains("Connection blocked"));
}

#[test]
fn test_validate_invalid_isolated_to_isolated() {
    let temp_dir = TempDir::new().unwrap();
    let workflow_file = temp_dir.path().join("invalid.nika.yaml");

    // v3 invalid: agent(isolated) -> agent(isolated) is BLOCKED
    fs::write(
        &workflow_file,
        r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: sub1
    type: agent
    scope: isolated
    prompt: "Sub1"
  - id: sub2
    type: agent
    scope: isolated
    prompt: "Sub2"

flows:
  - source: sub1
    target: sub2
"#,
    )
    .unwrap();

    nika_cmd()
        .args(["validate", workflow_file.to_str().unwrap()])
        .assert()
        .failure()
        .stdout(predicate::str::contains("Connection blocked"));
}

#[test]
fn test_validate_agent_missing_prompt() {
    let temp_dir = TempDir::new().unwrap();
    let workflow_file = temp_dir.path().join("bad.nika.yaml");

    fs::write(
        &workflow_file,
        r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: bad-agent
    type: agent

flows: []
"#,
    )
    .unwrap();

    nika_cmd()
        .args(["validate", workflow_file.to_str().unwrap()])
        .assert()
        .failure()
        .stdout(predicate::str::contains("requires 'prompt'"));
}

#[test]
fn test_validate_action_missing_run() {
    let temp_dir = TempDir::new().unwrap();
    let workflow_file = temp_dir.path().join("bad.nika.yaml");

    fs::write(
        &workflow_file,
        r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: bad-action
    type: action

flows: []
"#,
    )
    .unwrap();

    nika_cmd()
        .args(["validate", workflow_file.to_str().unwrap()])
        .assert()
        .failure()
        .stdout(predicate::str::contains("requires 'run'"));
}

#[test]
fn test_validate_json_output() {
    let temp_dir = TempDir::new().unwrap();
    let workflow_file = temp_dir.path().join("test.nika.yaml");

    fs::write(
        &workflow_file,
        r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: greet
    type: agent
    prompt: "Hello"

flows: []
"#,
    )
    .unwrap();

    nika_cmd()
        .args([
            "validate",
            workflow_file.to_str().unwrap(),
            "--format",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"valid\": true"))
        .stdout(predicate::str::contains("\"task_count\": 1"));
}

#[test]
fn test_validate_compact_output() {
    let temp_dir = TempDir::new().unwrap();
    let workflow_file = temp_dir.path().join("test.nika.yaml");

    fs::write(
        &workflow_file,
        r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: a
    type: agent
    prompt: "A"
  - id: b
    type: action
    run: Read

flows:
  - source: a
    target: b
"#,
    )
    .unwrap();

    nika_cmd()
        .args([
            "validate",
            workflow_file.to_str().unwrap(),
            "--format",
            "compact",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("2t"))
        .stdout(predicate::str::contains("1f"));
}

#[test]
fn test_validate_verbose() {
    let temp_dir = TempDir::new().unwrap();
    let workflow_file = temp_dir.path().join("test.nika.yaml");

    fs::write(
        &workflow_file,
        r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: greet
    type: agent
    prompt: "Hello"

flows: []
"#,
    )
    .unwrap();

    nika_cmd()
        .args([
            "validate",
            workflow_file.to_str().unwrap(),
            "--verbose",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Architecture v3"));
}

#[test]
fn test_validate_no_workflows_found() {
    let temp_dir = TempDir::new().unwrap();

    nika_cmd()
        .args(["validate", temp_dir.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("No workflow files found"));
}

#[test]
fn test_validate_directory_recursive() {
    let temp_dir = TempDir::new().unwrap();
    let sub_dir = temp_dir.path().join("workflows");
    fs::create_dir_all(&sub_dir).unwrap();

    // Create two workflows
    fs::write(
        sub_dir.join("a.nika.yaml"),
        r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "A"
tasks:
  - id: a
    type: agent
    prompt: "A"
flows: []
"#,
    )
    .unwrap();

    fs::write(
        sub_dir.join("b.nika.yaml"),
        r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "B"
tasks:
  - id: b
    type: agent
    prompt: "B"
flows: []
"#,
    )
    .unwrap();

    nika_cmd()
        .args(["validate", temp_dir.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("2 workflow file(s)"));
}

#[test]
fn test_run_workflow() {
    nika_cmd()
        .args(["run", "spec/examples/hello-world.nika.yaml"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Nika Runner"))
        .stdout(predicate::str::contains("Completed"));
}

#[test]
fn test_init_project() {
    let temp_dir = tempfile::tempdir().unwrap();
    nika_cmd()
        .current_dir(temp_dir.path())
        .args(["init", "test-project"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Project initialized"));

    // Verify files were created
    assert!(temp_dir.path().join("test-project/.nika").exists());
    assert!(temp_dir.path().join("test-project/main.nika.yaml").exists());
    assert!(temp_dir.path().join("test-project/nika.yaml").exists());
}
