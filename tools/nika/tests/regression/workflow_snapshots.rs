// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Workflow output snapshot tests.
//!
//! Uses insta for snapshot comparison.

use nika::ast::{parse_workflow, TaskAction};
use serde_json::json;

// ============================================================================
// WORKFLOW PARSING SNAPSHOTS
// ============================================================================

#[test]
fn test_snapshot_simple_workflow_parse() {
    let yaml = r#"
schema: "nika/workflow@0.12"

tasks:
  - id: task1
    exec: "echo hello"
"#;

    let workflow = parse_workflow(yaml).unwrap();

    insta::assert_yaml_snapshot!(
        "simple_workflow_parse",
        json!({
            "schema": workflow.schema,
            "task_count": workflow.tasks.len(),
            "task_ids": workflow.tasks.iter().map(|t| t.id.as_str()).collect::<Vec<_>>()
        })
    );
}

#[test]
fn test_snapshot_complex_workflow_parse() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: fetch_data
    fetch:
      url: "https://api.example.com/data"
      method: GET
      timeout: 5000

  - id: process
    depends_on: [fetch_data]
    exec: "process-data.sh"

  - id: analyze
    depends_on: [fetch_data]
    infer: "Analyze this data: {{with.data}}"
    with:
      data: $fetch_data

  - id: report
    depends_on: [analyze]
    infer: "Generate report from: {{with.analysis}}"
    with:
      analysis: $analyze
"#;

    let workflow = parse_workflow(yaml).unwrap();

    insta::assert_yaml_snapshot!(
        "complex_workflow_parse",
        json!({
            "schema": workflow.schema,
            "provider": workflow.provider,
            "task_count": workflow.tasks.len(),
            "task_ids": workflow.tasks.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            "has_flows": workflow.flow_count() > 0,
            "flow_count": workflow.flow_count(),
            "has_bindings": workflow.tasks.iter().any(|t| t.with_spec.is_some())
        })
    );
}

#[test]
fn test_snapshot_for_each_workflow() {
    let yaml = r#"
schema: "nika/workflow@0.12"

tasks:
  - id: parallel_tasks
    for_each: ["item1", "item2", "item3"]
    as: item
    concurrency: 3
    fail_fast: true
    exec: "process {{with.item}}"
"#;

    let workflow = parse_workflow(yaml).unwrap();
    let task = &workflow.tasks[0];

    insta::assert_yaml_snapshot!(
        "foreach_workflow",
        json!({
            "task_id": task.id,
            "has_for_each": task.for_each.is_some(),
            "as_var": task.for_each_as,
            "concurrency": task.concurrency,
            "fail_fast": task.fail_fast
        })
    );
}

#[test]
fn test_snapshot_mcp_workflow() {
    let yaml = r#"
schema: "nika/workflow@0.12"

mcp:
  servers:
    novanet:
      command: cargo
      args: ["run", "--bin", "novanet-mcp"]

tasks:
  - id: invoke_tool
    invoke:
      mcp: novanet
      tool: "novanet_describe"
      params:
        entity: "test"
"#;

    let workflow = parse_workflow(yaml).unwrap();

    insta::assert_yaml_snapshot!(
        "mcp_workflow",
        json!({
            "has_mcp": workflow.mcp.is_some(),
            "mcp_servers": workflow.mcp.as_ref().map(|m| m.keys().collect::<Vec<_>>()).unwrap_or_default(),
            "task_count": workflow.tasks.len()
        })
    );
}

#[test]
fn test_snapshot_agent_workflow() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: research_agent
    agent:
      prompt: "Research and summarize the topic"
      mcp: ["novanet"]
      max_turns: 5
      depth_limit: 3
"#;

    let workflow = parse_workflow(yaml).unwrap();

    let is_agent = matches!(&workflow.tasks[0].action, TaskAction::Agent { .. });

    insta::assert_yaml_snapshot!(
        "agent_workflow",
        json!({
            "task_id": workflow.tasks[0].id,
            "is_agent": is_agent
        })
    );
}

// ============================================================================
// ERROR MESSAGE SNAPSHOTS
// ============================================================================

#[test]
fn test_snapshot_parse_error_missing_schema() {
    let yaml = r#"
tasks:
  - id: task1
    exec: "echo"
"#;

    let result = parse_workflow(yaml);

    match result {
        Err(e) => {
            let error_msg = e.to_string();
            insta::assert_snapshot!("parse_error_missing_schema", error_msg);
        }
        Ok(_) => {
            // If parsing succeeds (schema optional), that's also valid
            insta::assert_snapshot!(
                "parse_error_missing_schema",
                "Schema optional - parsing succeeded"
            );
        }
    }
}

#[test]
fn test_snapshot_parse_error_invalid_yaml() {
    let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: task1
    exec: "echo"
    invalid_field: [this is not valid yaml
"#;

    let result = parse_workflow(yaml);

    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();

    // Snapshot the general error structure (not exact position)
    assert!(
        error_msg.contains("error")
            || error_msg.contains("invalid")
            || error_msg.contains("expected")
    );
}

// ============================================================================
// TASK ACTION SNAPSHOTS
// ============================================================================

#[test]
fn test_snapshot_all_verb_types() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude

mcp:
  servers:
    test:
      command: "echo"

tasks:
  - id: infer_task
    infer: "Generate text"

  - id: exec_task
    exec: "echo hello"

  - id: fetch_task
    fetch:
      url: "https://example.com"
      method: GET

  - id: invoke_task
    invoke:
      mcp: test
      tool: "test_tool"

  - id: agent_task
    agent:
      prompt: "Do something"
"#;

    let workflow = parse_workflow(yaml).unwrap();

    let verb_types: Vec<String> = workflow
        .tasks
        .iter()
        .map(|t| format!("{}: {}", t.id, t.action.verb_name()))
        .collect();

    insta::assert_yaml_snapshot!("all_verb_types", verb_types);
}

// ============================================================================
// BINDING SNAPSHOTS
// ============================================================================

#[test]
fn test_snapshot_binding_variants() {
    let yaml = r#"
schema: "nika/workflow@0.12"

tasks:
  - id: source
    exec: "echo data"

  - id: simple_binding
    depends_on: [source]
    infer: "Process: {{with.data}}"
    with:
      data: $source
"#;

    let workflow = parse_workflow(yaml).unwrap();

    insta::assert_yaml_snapshot!(
        "binding_variants",
        json!({
            "task_count": workflow.tasks.len(),
            "has_bindings": workflow.tasks.iter().any(|t| t.with_spec.is_some())
        })
    );
}

// ============================================================================
// ANALYZER ERROR MESSAGE SNAPSHOTS (all 8 AnalyzeErrorKind variants)
// ============================================================================

use nika::ast::analyzer::analyze;
use nika::ast::raw::parse;
use nika::source::FileId;

/// Helper: parse YAML → analyze → collect errors as "[CODE] message" strings.
fn analyze_errors(yaml: &str) -> Vec<String> {
    let raw = parse(yaml, FileId(0)).unwrap();
    let result = analyze(raw);
    result
        .errors
        .iter()
        .map(|e| format!("[{}] {}", e.kind.code(), e))
        .collect()
}

/// Helper: parse YAML → analyze → collect warnings as "[CODE] message" strings.
fn analyze_warnings(yaml: &str) -> Vec<String> {
    let raw = parse(yaml, FileId(0)).unwrap();
    let result = analyze(raw);
    result
        .warnings
        .iter()
        .map(|e| format!("[{}] {}", e.kind.code(), e))
        .collect()
}

#[test]
fn snapshot_error_unknown_task() {
    let errors = analyze_errors(
        r#"
schema: nika/workflow@0.12
model: test-model
tasks:
  - id: task1
    depends_on: [nonexistent]
    infer: "hello"
"#,
    );
    assert!(!errors.is_empty(), "expected UnknownTask error");
    insta::assert_yaml_snapshot!("error_unknown_task", errors);
}

#[test]
fn snapshot_error_duplicate_task() {
    let errors = analyze_errors(
        r#"
schema: nika/workflow@0.12
model: test-model
tasks:
  - id: dup
    infer: "hello"
  - id: dup
    infer: "world"
"#,
    );
    assert!(!errors.is_empty(), "expected DuplicateTask error");
    insta::assert_yaml_snapshot!("error_duplicate_task", errors);
}

#[test]
fn snapshot_error_invalid_schema() {
    let errors = analyze_errors(
        r#"
schema: nika/workflow@99.99
model: test-model
tasks:
  - id: task1
    infer: "hello"
"#,
    );
    assert!(!errors.is_empty(), "expected InvalidSchema error");
    insta::assert_yaml_snapshot!("error_invalid_schema", errors);
}

#[test]
fn snapshot_error_cyclic_dependency() {
    let errors = analyze_errors(
        r#"
schema: nika/workflow@0.12
model: test-model
tasks:
  - id: a
    depends_on: [b]
    infer: "hello"
  - id: b
    depends_on: [a]
    infer: "world"
"#,
    );
    assert!(!errors.is_empty(), "expected CyclicDependency error");
    insta::assert_yaml_snapshot!("error_cyclic_dependency", errors);
}

#[test]
fn snapshot_error_invalid_value() {
    // Task ID starting with '$' triggers InvalidValue
    let errors = analyze_errors(
        r#"
schema: nika/workflow@0.12
model: test-model
tasks:
  - id: $bad_id
    infer: "hello"
"#,
    );
    assert!(!errors.is_empty(), "expected InvalidValue error");
    insta::assert_yaml_snapshot!("error_invalid_value", errors);
}

#[test]
fn snapshot_error_missing_field() {
    // MCP stdio server without 'command' triggers MissingField
    let errors = analyze_errors(
        r#"
schema: nika/workflow@0.12
model: test-model
mcp:
  servers:
    broken:
      args: ["--flag"]
tasks:
  - id: task1
    infer: "hello"
"#,
    );
    assert!(!errors.is_empty(), "expected MissingField error");
    insta::assert_yaml_snapshot!("error_missing_field", errors);
}

#[test]
fn snapshot_error_unsupported_feature() {
    // SSE MCP server triggers UnsupportedFeature warning
    let warnings = analyze_warnings(
        r#"
schema: nika/workflow@0.12
model: test-model
mcp:
  servers:
    remote:
      url: "https://example.com/sse"
tasks:
  - id: task1
    infer: "hello"
"#,
    );
    assert!(!warnings.is_empty(), "expected UnsupportedFeature warning");
    insta::assert_yaml_snapshot!("error_unsupported_feature", warnings);
}

#[test]
fn snapshot_error_invalid_binding() {
    let errors = analyze_errors(
        r#"
schema: nika/workflow@0.12
model: test-model
tasks:
  - id: task1
    with:
      x: ""
    infer: "hello"
"#,
    );
    assert!(!errors.is_empty(), "expected InvalidBinding error");
    insta::assert_yaml_snapshot!("error_invalid_binding", errors);
}
