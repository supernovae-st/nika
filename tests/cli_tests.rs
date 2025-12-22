//! Integration tests for the Nika CLI (v4.7.1)
//!
//! These tests run the actual CLI binary and verify output.
//! Architecture v4.7.1: 7 keywords with type inference (agent, subagent, shell, http, mcp, function, llm).

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Get the binary to test
#[allow(deprecated)] // cargo_bin works fine, deprecation is for edge case with custom build-dir
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
        .stdout(predicate::str::contains("Architecture v4.7.1"));
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
// v4.7.1 Workflow Validation Tests
// ============================================================================

#[test]
fn test_validate_valid_hello_world() {
    let temp_dir = TempDir::new().unwrap();
    let workflow_file = temp_dir.path().join("hello.nika.yaml");

    // v4.7.1 hello world workflow
    fs::write(
        &workflow_file,
        r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "You are a helpful assistant."

tasks:
  - id: greet
    agent:
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
fn test_validate_agent_to_tool() {
    let temp_dir = TempDir::new().unwrap();
    let workflow_file = temp_dir.path().join("test.nika.yaml");

    // v4.7.1: agent: -> shell: is valid
    fs::write(
        &workflow_file,
        r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: analyze
    agent:
      prompt: "Analyze the code."
  - id: save
    shell:
      command: "echo done > output.txt"

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

    // v4.7.1 bridge pattern: subagent: -> function: -> agent:
    fs::write(
        &workflow_file,
        r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Orchestrator"

tasks:
  - id: worker
    subagent:
      prompt: "Do deep work."
  - id: bridge
    function:

      reference: "aggregate::collect"
  - id: router
    agent:
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
fn test_validate_subagent_to_agent_allowed_v471() {
    let temp_dir = TempDir::new().unwrap();
    let workflow_file = temp_dir.path().join("valid.nika.yaml");

    // v4.7.1: subagent: -> agent: is NOW ALLOWED (WorkflowRunner auto-writes)
    fs::write(
        &workflow_file,
        r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: worker
    subagent:
      prompt: "Work"
  - id: router
    agent:
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
        .success()
        .stdout(predicate::str::contains("2 tasks"));
}

#[test]
fn test_validate_invalid_subagent_to_subagent() {
    let temp_dir = TempDir::new().unwrap();
    let workflow_file = temp_dir.path().join("invalid.nika.yaml");

    // v4.7.1 invalid: subagent: -> subagent: is BLOCKED
    fs::write(
        &workflow_file,
        r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: sub1
    subagent:
      prompt: "Sub1"
  - id: sub2
    subagent:
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
fn test_validate_missing_keyword() {
    let temp_dir = TempDir::new().unwrap();
    let workflow_file = temp_dir.path().join("bad.nika.yaml");

    // v4.7.1: task must have exactly one keyword
    fs::write(
        &workflow_file,
        r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: bad-task

flows: []
"#,
    )
    .unwrap();

    // v4.7.1: Missing keyword is a parse error, not validation error
    // CLI continues batch processing (returns 0) but prints error to stderr
    nika_cmd()
        .args(["validate", workflow_file.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("Failed to parse"));
}

#[test]
fn test_validate_mcp_missing_separator() {
    let temp_dir = TempDir::new().unwrap();
    let workflow_file = temp_dir.path().join("bad.nika.yaml");

    // v4.7.1: mcp must use :: separator
    fs::write(
        &workflow_file,
        r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: bad-mcp
    mcp:
      reference: "filesystem_read"

flows: []
"#,
    )
    .unwrap();

    nika_cmd()
        .args(["validate", workflow_file.to_str().unwrap()])
        .assert()
        .failure()
        .stdout(predicate::str::contains("'::'"));
}

#[test]
fn test_validate_json_output() {
    let temp_dir = TempDir::new().unwrap();
    let workflow_file = temp_dir.path().join("test.nika.yaml");

    fs::write(
        &workflow_file,
        r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: greet
    agent:
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
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: a
    agent:
      prompt: "A"
  - id: b
    mcp:
      reference: "filesystem::read"

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
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: greet
    agent:
      prompt: "Hello"

flows: []
"#,
    )
    .unwrap();

    nika_cmd()
        .args(["validate", workflow_file.to_str().unwrap(), "--verbose"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Architecture v4.7.1"));
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

    // Create two v4.7.1 workflows
    fs::write(
        sub_dir.join("a.nika.yaml"),
        r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "A"
tasks:
  - id: a
    agent:
      prompt: "A"
flows: []
"#,
    )
    .unwrap();

    fs::write(
        sub_dir.join("b.nika.yaml"),
        r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "B"
tasks:
  - id: b
    agent:
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

#[test]
fn test_all_7_keywords_valid() {
    let temp_dir = TempDir::new().unwrap();
    let workflow_file = temp_dir.path().join("all-keywords.nika.yaml");

    // v4.7.1: all 7 keywords in one workflow
    fs::write(
        &workflow_file,
        r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test all keywords"

tasks:
  - id: t1
    agent:
      prompt: "Main agent task"
  - id: t2
    subagent:
      prompt: "Subagent task"
  - id: t3
    shell:
      command: "npm test"
  - id: t4
    http:
      url: "https://api.example.com"
  - id: t5
    mcp:
      reference: "filesystem::read_file"
  - id: t6
    function:

      reference: "tools::transform"
  - id: t7
    llm:
      prompt: "Classify this text"

flows: []
"#,
    )
    .unwrap();

    nika_cmd()
        .args(["validate", workflow_file.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("7 tasks"));
}
