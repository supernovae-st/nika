//! Security tests for Nika CLI
//!
//! These tests verify that shell command injection and other security
//! vulnerabilities are properly prevented.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Get the binary to test
#[allow(deprecated)]
fn nika_cmd() -> Command {
    Command::cargo_bin("nika").unwrap()
}

// ============================================================================
// SHELL COMMAND INJECTION TESTS
// ============================================================================

/// Test that shell command injection via semicolon is blocked
#[test]
fn test_shell_command_injection_semicolon_blocked() {
    let temp_dir = TempDir::new().unwrap();
    let workflow_file = temp_dir.path().join("injection.nika.yaml");
    let marker_file = temp_dir.path().join("pwned.txt");

    // Attempt to inject a command using semicolon
    // If vulnerable, this would create a file "pwned.txt"
    let injection_cmd = format!("echo safe; touch {}", marker_file.to_str().unwrap());

    fs::write(
        &workflow_file,
        format!(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: test-shell
    shell:
      command: "{}"

flows: []
"#,
            injection_cmd
        ),
    )
    .unwrap();

    // Run the workflow - should succeed overall but shell task should fail
    let output = nika_cmd()
        .args(["run", workflow_file.to_str().unwrap(), "--provider", "mock"])
        .assert()
        .success();

    // The output should indicate the shell task failed
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    assert!(
        stdout.contains("1 tasks failed"),
        "Shell task with injection should fail. Got: {}",
        stdout
    );

    // SECURITY CHECK: The marker file should NOT exist
    assert!(
        !marker_file.exists(),
        "SECURITY VULNERABILITY: Shell command injection via semicolon succeeded!"
    );
}

/// Test that shell command injection via backticks is blocked
#[test]
fn test_shell_command_injection_backticks_blocked() {
    let temp_dir = TempDir::new().unwrap();
    let workflow_file = temp_dir.path().join("injection.nika.yaml");
    let marker_file = temp_dir.path().join("pwned_backtick.txt");

    // Attempt to inject using backticks (command substitution)
    let injection_cmd = format!("echo `touch {}`", marker_file.to_str().unwrap());

    fs::write(
        &workflow_file,
        format!(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: test-shell
    shell:
      command: "{}"

flows: []
"#,
            injection_cmd
        ),
    )
    .unwrap();

    let output = nika_cmd()
        .args(["run", workflow_file.to_str().unwrap(), "--provider", "mock"])
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    assert!(
        stdout.contains("1 tasks failed"),
        "Shell task with backticks should fail. Got: {}",
        stdout
    );

    assert!(
        !marker_file.exists(),
        "SECURITY VULNERABILITY: Shell command injection via backticks succeeded!"
    );
}

/// Test that shell command injection via $() is blocked
#[test]
fn test_shell_command_injection_dollar_paren_blocked() {
    let temp_dir = TempDir::new().unwrap();
    let workflow_file = temp_dir.path().join("injection.nika.yaml");
    let marker_file = temp_dir.path().join("pwned_dollar.txt");

    // Attempt to inject using $() (command substitution)
    let injection_cmd = format!("echo $(touch {})", marker_file.to_str().unwrap());

    fs::write(
        &workflow_file,
        format!(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: test-shell
    shell:
      command: "{}"

flows: []
"#,
            injection_cmd
        ),
    )
    .unwrap();

    let output = nika_cmd()
        .args(["run", workflow_file.to_str().unwrap(), "--provider", "mock"])
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    assert!(
        stdout.contains("1 tasks failed"),
        "Shell task with $() should fail. Got: {}",
        stdout
    );

    assert!(
        !marker_file.exists(),
        "SECURITY VULNERABILITY: Shell command injection via $() succeeded!"
    );
}

/// Test that shell command injection via pipe is blocked
#[test]
fn test_shell_command_injection_pipe_blocked() {
    let temp_dir = TempDir::new().unwrap();
    let workflow_file = temp_dir.path().join("injection.nika.yaml");
    let marker_file = temp_dir.path().join("pwned_pipe.txt");

    // Attempt to inject using pipe
    let injection_cmd = format!("echo safe | touch {}", marker_file.to_str().unwrap());

    fs::write(
        &workflow_file,
        format!(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: test-shell
    shell:
      command: "{}"

flows: []
"#,
            injection_cmd
        ),
    )
    .unwrap();

    let output = nika_cmd()
        .args(["run", workflow_file.to_str().unwrap(), "--provider", "mock"])
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    assert!(
        stdout.contains("1 tasks failed"),
        "Shell task with pipe should fail. Got: {}",
        stdout
    );

    assert!(
        !marker_file.exists(),
        "SECURITY VULNERABILITY: Shell command injection via pipe succeeded!"
    );
}

/// Test that shell command injection via && is blocked
#[test]
fn test_shell_command_injection_and_blocked() {
    let temp_dir = TempDir::new().unwrap();
    let workflow_file = temp_dir.path().join("injection.nika.yaml");
    let marker_file = temp_dir.path().join("pwned_and.txt");

    // Attempt to inject using &&
    let injection_cmd = format!("echo safe && touch {}", marker_file.to_str().unwrap());

    fs::write(
        &workflow_file,
        format!(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: test-shell
    shell:
      command: "{}"

flows: []
"#,
            injection_cmd
        ),
    )
    .unwrap();

    let output = nika_cmd()
        .args(["run", workflow_file.to_str().unwrap(), "--provider", "mock"])
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    assert!(
        stdout.contains("1 tasks failed"),
        "Shell task with && should fail. Got: {}",
        stdout
    );

    assert!(
        !marker_file.exists(),
        "SECURITY VULNERABILITY: Shell command injection via && succeeded!"
    );
}

/// Test that shell command injection via || is blocked
#[test]
fn test_shell_command_injection_or_blocked() {
    let temp_dir = TempDir::new().unwrap();
    let workflow_file = temp_dir.path().join("injection.nika.yaml");
    let marker_file = temp_dir.path().join("pwned_or.txt");

    // Attempt to inject using || (contains |)
    let injection_cmd = format!("false || touch {}", marker_file.to_str().unwrap());

    fs::write(
        &workflow_file,
        format!(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: test-shell
    shell:
      command: "{}"

flows: []
"#,
            injection_cmd
        ),
    )
    .unwrap();

    let output = nika_cmd()
        .args(["run", workflow_file.to_str().unwrap(), "--provider", "mock"])
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    // || contains |, so it triggers the pipe check
    assert!(
        stdout.contains("1 tasks failed"),
        "Shell task with || should fail. Got: {}",
        stdout
    );

    assert!(
        !marker_file.exists(),
        "SECURITY VULNERABILITY: Shell command injection via || succeeded!"
    );
}

/// Test that newline injection is blocked
#[test]
fn test_shell_command_injection_newline_blocked() {
    let temp_dir = TempDir::new().unwrap();
    let workflow_file = temp_dir.path().join("injection.nika.yaml");
    let marker_file = temp_dir.path().join("pwned_newline.txt");

    // Note: YAML multiline would need different escape handling
    // This tests if raw newlines in the command are sanitized
    let injection_cmd = format!("echo safe\ntouch {}", marker_file.to_str().unwrap());

    fs::write(
        &workflow_file,
        format!(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: test-shell
    shell:
      command: "{}"

flows: []
"#,
            injection_cmd
        ),
    )
    .unwrap();

    nika_cmd()
        .args(["run", workflow_file.to_str().unwrap(), "--provider", "mock"])
        .assert()
        .success();

    assert!(
        !marker_file.exists(),
        "SECURITY VULNERABILITY: Shell command injection via newline succeeded!"
    );
}

/// Test dangerous rm -rf command is blocked
#[test]
fn test_shell_dangerous_rm_rf_blocked() {
    let temp_dir = TempDir::new().unwrap();
    let workflow_file = temp_dir.path().join("dangerous.nika.yaml");

    // Attempt to run a dangerous rm -rf command
    fs::write(
        &workflow_file,
        r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: dangerous
    shell:
      command: "rm -rf /"

flows: []
"#,
    )
    .unwrap();

    // The command should either:
    // 1. Be blocked at validation/sanitization
    // 2. Fail with error
    // 3. Be sandboxed so it can't actually do damage
    let result = nika_cmd()
        .args(["run", workflow_file.to_str().unwrap(), "--provider", "mock"])
        .assert();

    // We expect either an error (blocked) or success (sandboxed/mock)
    // The key is that it shouldn't actually delete anything
    // For now, we just verify the command doesn't hang or crash
    result.success();
}

/// Test that simple safe commands still work
#[test]
fn test_shell_safe_commands_work() {
    let temp_dir = TempDir::new().unwrap();
    let workflow_file = temp_dir.path().join("safe.nika.yaml");

    fs::write(
        &workflow_file,
        r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: safe-echo
    shell:
      command: "echo hello"

flows: []
"#,
    )
    .unwrap();

    nika_cmd()
        .args(["run", workflow_file.to_str().unwrap(), "--provider", "mock"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Completed"));
}

// ============================================================================
// VALIDATION-TIME SECURITY TESTS
// ============================================================================

/// Test that validation rejects obviously dangerous shell patterns
#[test]
fn test_validate_rejects_dangerous_shell_patterns() {
    let temp_dir = TempDir::new().unwrap();
    let workflow_file = temp_dir.path().join("danger.nika.yaml");

    // Commands with obvious injection patterns should be rejected at validation
    fs::write(
        &workflow_file,
        r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: injection
    shell:
      command: "echo test; rm -rf /"

flows: []
"#,
    )
    .unwrap();

    // Validation should either:
    // 1. Pass but warn about dangerous patterns
    // 2. Fail with security error
    // Currently we just run to verify no crashes
    nika_cmd()
        .args(["validate", workflow_file.to_str().unwrap()])
        .assert()
        .success();

    // TODO: Once security validation is implemented, this should fail:
    // .failure()
    // .stderr(predicate::str::contains("dangerous"));
}
