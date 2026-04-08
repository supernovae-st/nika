// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Output capture tests for regression detection.
//!
//! These tests capture and compare actual outputs to detect regressions.
//! Uses direct command execution for shell tests and workflow parsing for format validation.

use nika::ast::parse_workflow;
use std::process::Command;

// ============================================================================
// SHELL OUTPUT CAPTURE (via std::process::Command)
// ============================================================================

#[test]
fn test_capture_shell_echo() {
    let output = Command::new("sh")
        .arg("-c")
        .arg("echo 'test output'")
        .output()
        .expect("Failed to execute command");

    let result = String::from_utf8_lossy(&output.stdout);

    // Output should be consistent
    assert!(result.contains("test output"));
    assert_eq!(result.trim(), "test output");
}

#[test]
fn test_capture_shell_multiline() {
    let output = Command::new("sh")
        .arg("-c")
        .arg("printf 'line1\\nline2\\nline3'")
        .output()
        .expect("Failed to execute command");

    let result = String::from_utf8_lossy(&output.stdout);

    // Count lines
    let lines: Vec<&str> = result.lines().collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0], "line1");
    assert_eq!(lines[1], "line2");
    assert_eq!(lines[2], "line3");
}

#[test]
fn test_capture_shell_json_output() {
    let output = Command::new("sh")
        .arg("-c")
        .arg(r#"echo '{"key": "value", "number": 42}'"#)
        .output()
        .expect("Failed to execute command");

    let result = String::from_utf8_lossy(&output.stdout);

    // Should be parseable JSON
    let parsed: serde_json::Value = serde_json::from_str(result.trim()).unwrap();
    assert_eq!(parsed["key"], "value");
    assert_eq!(parsed["number"], 42);
}

#[test]
fn test_capture_shell_success() {
    // Test successful command
    let output = Command::new("true")
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

#[test]
fn test_capture_shell_failure() {
    // Test failed command
    let output = Command::new("false")
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success());
}

// ============================================================================
// DETERMINISTIC OUTPUT TESTS
// ============================================================================

#[test]
fn test_deterministic_shell() {
    // Run 5 times, should get same result
    let mut results = Vec::new();
    for _ in 0..5 {
        let output = Command::new("sh")
            .arg("-c")
            .arg("echo 'deterministic'")
            .output()
            .expect("Failed to execute command");
        let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
        results.push(result);
    }

    // All results should be identical
    let first = &results[0];
    for (i, result) in results.iter().enumerate() {
        assert_eq!(result, first, "Run {} produced different output", i);
    }
}

// ============================================================================
// OUTPUT FORMAT VALIDATION
// ============================================================================

#[test]
fn test_output_trimming() {
    let output = Command::new("sh")
        .arg("-c")
        .arg("echo '  padded  '")
        .output()
        .expect("Failed to execute command");

    let result = String::from_utf8_lossy(&output.stdout);

    // Raw output may have trailing newline
    assert!(result.ends_with('\n') || result.ends_with("  "));
}

#[test]
fn test_output_unicode() {
    let output = Command::new("sh")
        .arg("-c")
        .arg("echo '日本語 emoji 🚀'")
        .output()
        .expect("Failed to execute command");

    let result = String::from_utf8_lossy(&output.stdout);

    assert!(result.contains("日本語"));
    assert!(result.contains("🚀"));
}

#[test]
fn test_output_special_chars() {
    let output = Command::new("sh")
        .arg("-c")
        .arg(r#"echo 'quotes "and" $vars'"#)
        .output()
        .expect("Failed to execute command");

    let result = String::from_utf8_lossy(&output.stdout);

    assert!(result.contains("quotes"));
    assert!(result.contains("and"));
}

// ============================================================================
// WORKFLOW PARSING REGRESSION TESTS
// ============================================================================

#[test]
fn test_regression_exec_workflow_parsing() {
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: exec-regression-test
description: "Test exec parsing stability"

tasks:
  - id: echo_test
    exec: "echo 'test output'"

  - id: multiline_test
    exec:
      command: "printf 'line1\\nline2'"
      timeout: 5000
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse");

    assert_eq!(workflow.tasks.len(), 2);
    assert!(workflow.schema.starts_with("nika/workflow@"));
}

#[test]
fn test_regression_fetch_workflow_parsing() {
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: fetch-regression-test
description: "Test fetch parsing stability"

tasks:
  - id: get_json
    fetch:
      url: "https://httpbin.org/json"
      method: GET

  - id: post_data
    fetch:
      url: "https://httpbin.org/post"
      method: POST
      body: '{"key": "value"}'
      headers:
        Content-Type: "application/json"
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse");

    assert_eq!(workflow.tasks.len(), 2);
}

#[test]
fn test_regression_infer_workflow_parsing() {
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: infer-regression-test
provider: claude

tasks:
  - id: simple_infer
    infer: "Generate a greeting"

  - id: full_infer
    infer:
      prompt: "Generate a detailed response"
      model: claude-sonnet-4-20250514
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse");

    assert_eq!(workflow.tasks.len(), 2);
}

#[test]
fn test_regression_all_verbs_workflow() {
    // Comprehensive workflow with all 5 verbs
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: all-verbs-regression
description: "Test all verb parsing stability"
provider: claude

mcp:
  servers:
    tools:
      command: "node"
      args: ["./tools.js"]

tasks:
  - id: exec_task
    exec: "echo 'exec verb'"

  - id: fetch_task
    depends_on: [exec_task]
    fetch:
      url: "https://example.com"
      method: GET

  - id: infer_task
    depends_on: [fetch_task]
    infer: "Generate text"

  - id: invoke_task
    depends_on: [infer_task]
    invoke:
      mcp: tools
      tool: "some_tool"
      params:
        key: value

  - id: agent_task
    depends_on: [invoke_task]
    agent:
      prompt: "Do something"
      max_turns: 3
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse");

    // Verify structure
    assert_eq!(workflow.tasks.len(), 5);
    assert_eq!(workflow.flow_count(), 4);
    assert!(workflow.mcp.is_some());
}

// ============================================================================
// REGRESSION MARKERS
// ============================================================================

#[test]
fn test_regression_marker_shell_basic() {
    // This test documents expected behavior for regression detection
    let output = Command::new("sh")
        .arg("-c")
        .arg("echo MARKER_V1")
        .output()
        .expect("Failed to execute command");

    let result = String::from_utf8_lossy(&output.stdout);

    // If this assertion fails, behavior has regressed
    assert!(
        result.contains("MARKER_V1"),
        "Regression: shell output format changed"
    );
}

#[test]
fn test_regression_marker_workflow_schema() {
    // Test that workflow schema version is correctly parsed
    let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: test
    exec: "echo test"
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse");

    // Schema should be exactly as specified
    assert_eq!(
        workflow.schema, "nika/workflow@0.12",
        "Regression: schema parsing changed"
    );
}

// ============================================================================
// BINDING SYNTAX REGRESSION
// ============================================================================

#[test]
fn test_regression_binding_syntax() {
    let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: source
    exec: "echo 'source data'"

  - id: consumer
    depends_on: [source]
    infer: "Process: {{with.data}}"
    with:
      data: $source
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse");

    assert_eq!(workflow.tasks.len(), 2);
    // depends_on creates 1 flow, with: binding creates another implicit flow
    assert!(workflow.flow_count() >= 1);
}

#[test]
fn test_regression_lazy_binding_syntax() {
    // Note: lazy bindings with complex map values (path/lazy/default) are not
    // supported in the raw parser's with: field — values must be simple strings.
    // This test verifies basic with: binding works correctly.
    let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: producer
    exec: "echo 'lazy data'"

  - id: consumer
    depends_on: [producer]
    infer: "Process: {{with.ctx}}"
    with:
      ctx: $producer
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse");

    assert_eq!(workflow.tasks.len(), 2);
}

// ============================================================================
// FOR_EACH SYNTAX REGRESSION
// ============================================================================

#[test]
fn test_regression_for_each_static() {
    let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: parallel_task
    for_each: ["a", "b", "c"]
    as: item
    concurrency: 3
    exec: "echo {{with.item}}"
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse");

    let task = &workflow.tasks[0];
    assert!(task.for_each.is_some());
}

#[test]
fn test_regression_for_each_binding() {
    let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: source
    exec: "echo '[1, 2, 3]'"

  - id: iterate
    depends_on: [source]
    for_each: "{{with.items}}"
    as: num
    infer: "Process {{with.num}}"
    with:
      items: $source
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse");

    assert_eq!(workflow.tasks.len(), 2);
}
