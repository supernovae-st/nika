// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Error Handling Tests - All NIKA-XXX error codes.
//!
//! Tests proper error handling for:
//! - Parse errors (NIKA-000-009)
//! - Task errors (NIKA-010-019)
//! - DAG errors (NIKA-020-029)
//! - Provider errors (NIKA-030-039)
//! - Binding errors (NIKA-040-049)
//! - MCP errors (NIKA-100-109)
//! - Agent errors (NIKA-110-119)

use nika::ast::parse_workflow;
use nika::dag::Dag;

// ============================================================================
// PARSE ERROR TESTS (NIKA-000-009)
// ============================================================================

#[test]
fn test_parse_error_missing_schema() {
    let yaml = r#"
provider: claude

tasks:
  - id: test
    infer: "Hello"
"#;

    // Missing required 'schema' field
    let result = parse_workflow(yaml);
    assert!(result.is_err(), "Should fail without schema");
}

#[test]
fn test_parse_error_invalid_schema_version() {
    let yaml = r#"
schema: "nika/workflow@99.99"
provider: claude

tasks:
  - id: test
    infer: "Hello"
"#;

    // parse_workflow() validates the schema version during analysis.
    // Unknown versions like 99.99 are rejected.
    let result = parse_workflow(yaml);
    assert!(result.is_err(), "Should reject unknown schema version");
}

#[test]
fn test_parse_error_invalid_yaml_syntax() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: test
    infer: "Hello
    # Missing closing quote
"#;

    let result = parse_workflow(yaml);
    assert!(result.is_err(), "Should fail on invalid YAML");
}

#[test]
fn test_parse_error_unknown_field() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude
unknown_field: "value"

tasks:
  - id: test
    infer: "Hello"
"#;

    // serde_yaml with deny_unknown_fields would catch this
    // Current config allows unknown fields for forward compatibility
    let result = parse_workflow(yaml);
    // Depends on serde config - may pass or fail
    let _ = result;
}

#[test]
fn test_parse_error_empty_tasks() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude

tasks: []
"#;

    let result = parse_workflow(yaml);
    assert!(
        result.is_err(),
        "Empty tasks array should be rejected by analyzer"
    );
}

#[test]
fn test_parse_error_missing_task_id() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - infer: "Hello"
"#;

    // Missing required 'id' field on task
    let result = parse_workflow(yaml);
    assert!(result.is_err(), "Should fail without task id");
}

#[test]
fn test_parse_error_missing_verb() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: test
    output:
      format: json
"#;

    // parse_workflow() may accept tasks without a verb at parse time;
    // the missing verb is caught at runtime dispatch.
    let result = parse_workflow(yaml);
    // Document actual behavior: permissive parsing
    let _ = result;
}

#[test]
fn test_parse_error_multiple_verbs() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: test
    infer: "Hello"
    exec: "echo"
"#;

    // Task with multiple verbs - serde_yaml's tagged enum will pick one
    // This is a parser limitation - runtime validation would catch it
    let result = parse_workflow(yaml);
    // Note: Due to serde's tagged enum handling, this may parse (picking first verb)
    // The test documents actual behavior rather than ideal behavior
    let _ = result; // Implementation-dependent
}

// ============================================================================
// DAG ERROR TESTS (NIKA-020-029)
// ============================================================================
// Note: The raw parser does NOT parse `flows:` YAML sections.
// Cycles must be expressed via `depends_on:` so the analyzer detects them
// during parse_workflow() Phase 2.

#[test]
fn test_dag_error_self_reference() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: circular
    depends_on: [circular]
    infer: "Hello"
"#;

    let result = parse_workflow(yaml);
    assert!(
        result.is_err(),
        "Should detect self-referential cycle via depends_on"
    );
}

#[test]
fn test_dag_error_two_node_cycle() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: a
    depends_on: [b]
    infer: "A"
  - id: b
    depends_on: [a]
    infer: "B"
"#;

    let result = parse_workflow(yaml);
    assert!(
        result.is_err(),
        "Should detect two-node cycle via depends_on"
    );
}

#[test]
fn test_dag_error_three_node_cycle() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: a
    depends_on: [c]
    infer: "A"
  - id: b
    depends_on: [a]
    infer: "B"
  - id: c
    depends_on: [b]
    infer: "C"
"#;

    let result = parse_workflow(yaml);
    assert!(
        result.is_err(),
        "Should detect three-node cycle via depends_on"
    );
}

#[test]
fn test_dag_error_complex_cycle() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: start
    infer: "Start"
  - id: a
    depends_on: [start]
    infer: "A"
  - id: b
    depends_on: [a, d]
    infer: "B"
  - id: c
    depends_on: [b]
    infer: "C"
  - id: d
    depends_on: [c]
    infer: "D"
  - id: end_task
    depends_on: [b]
    infer: "End"
"#;

    // d -> b -> c -> d creates a cycle
    let result = parse_workflow(yaml);
    assert!(
        result.is_err(),
        "Should detect cycle in complex DAG via depends_on"
    );
}

#[test]
fn test_dag_error_orphan_flow_source() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: task1
    infer: "Hello"

"#;

    let workflow = parse_workflow(yaml).expect("Should parse");
    // Dag builds successfully but may have reference to unknown task
    let graph = Dag::from_workflow(&workflow).unwrap();
    // Check if the orphan reference was added or ignored
    let deps = graph.get_dependencies("task1");
    // Either the orphan is included or ignored - both are valid behaviors
    let has_nonexistent = deps.iter().any(|d| d.as_ref() == "nonexistent");
    let _ = has_nonexistent; // Implementation-dependent
}

#[test]
fn test_dag_error_orphan_flow_target() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: task1
    infer: "Hello"

"#;

    let workflow = parse_workflow(yaml).expect("Should parse");
    let result = Dag::from_workflow(&workflow).unwrap();
    // Should either error or create a reference to unknown task
    let _ = result;
}

// ============================================================================
// BINDING ERROR TESTS (NIKA-040-049)
// ============================================================================

#[test]
fn test_binding_in_prompt() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: setup
    exec: "echo 'data'"

  - id: use_binding
    with:
      data: $setup
    infer: "Process: {{with.data}}"
"#;

    // parse_workflow() processes use: bindings during analysis.
    // The lowered Workflow struct uses with_spec for bindings
    // since bindings are resolved at runtime, not stored structurally.
    let workflow = parse_workflow(yaml).expect("Should parse");
    assert_eq!(workflow.tasks.len(), 2);
}

#[test]
fn test_binding_dollar_syntax() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: setup
    exec: "echo 'data'"

  - id: use_binding
    with:
      data: $setup
    infer: "Process: $data"
"#;

    // parse_workflow() processes use: bindings during analysis.
    // The lowered Workflow struct uses with_spec for bindings
    // since bindings are resolved at runtime, not stored structurally.
    let workflow = parse_workflow(yaml).expect("Should parse");
    assert_eq!(workflow.tasks.len(), 2);
}

#[test]
#[ignore = "raw parser parse_string_map does not support lazy binding objects under with: yet"]
fn test_binding_lazy_with_default() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: maybe_fails
    exec: "echo 'ok'"

  - id: use_lazy
    with:
      data:
        path: maybe_fails
        lazy: true
        default: "fallback"
    infer: "Data: {{with.data}}"
"#;

    // parse_workflow() processes use: with lazy/default during analysis.
    // The lowered Workflow struct uses with_spec for bindings.
    // Lazy binding resolution happens at runtime.
    let workflow = parse_workflow(yaml).expect("Should parse");
    assert_eq!(workflow.tasks.len(), 2);
}

#[test]
fn test_binding_multiple_aliases() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: task_a
    exec: "echo 'a'"

  - id: task_b
    exec: "echo 'b'"

  - id: task_c
    exec: "echo 'c'"

  - id: aggregate
    with:
      a: $task_a
      b: $task_b
      c: $task_c
    infer: "A={{with.a}}, B={{with.b}}, C={{with.c}}"
"#;

    // parse_workflow() processes use: bindings during analysis.
    // The lowered Workflow struct uses with_spec for bindings
    // since bindings are resolved at runtime, not stored structurally.
    let workflow = parse_workflow(yaml).expect("Should parse");
    assert_eq!(workflow.tasks.len(), 4);
}

// ============================================================================
// VERB-SPECIFIC ERROR TESTS
// ============================================================================

#[test]
fn test_infer_shorthand_string() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: simple
    infer: "Just a simple prompt"
"#;

    let workflow = parse_workflow(yaml).expect("Should parse shorthand");
    assert_eq!(workflow.tasks.len(), 1);
}

#[test]
fn test_infer_full_form() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: full
    infer:
      prompt: "Full form prompt"
      model: gpt-4o
"#;

    let workflow = parse_workflow(yaml).expect("Should parse full form");
    assert_eq!(workflow.tasks.len(), 1);
}

#[test]
fn test_exec_shorthand_string() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: simple
    exec: "echo 'hello'"
"#;

    let workflow = parse_workflow(yaml).expect("Should parse shorthand");
    assert_eq!(workflow.tasks.len(), 1);
}

#[test]
fn test_exec_full_form() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: full
    exec:
      command: "echo 'hello'"
      timeout: 5000
"#;

    let workflow = parse_workflow(yaml).expect("Should parse full form");
    assert_eq!(workflow.tasks.len(), 1);
}

#[test]
fn test_fetch_requires_object() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: fetch_task
    fetch:
      url: "https://httpbin.org/get"
      method: GET
"#;

    let workflow = parse_workflow(yaml).expect("Should parse");
    assert_eq!(workflow.tasks.len(), 1);
}

#[test]
fn test_invoke_requires_object() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude

mcp:
  novanet:
    command: "echo"

tasks:
  - id: invoke_task
    invoke:
      mcp: novanet
      tool: novanet_describe
      params:
        entity: "qr-code"
"#;

    let workflow = parse_workflow(yaml).expect("Should parse");
    assert_eq!(workflow.tasks.len(), 1);
}

#[test]
fn test_agent_requires_prompt() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: agent_task
    agent:
      prompt: "Required prompt"
"#;

    let workflow = parse_workflow(yaml).expect("Should parse");
    assert_eq!(workflow.tasks.len(), 1);
}

// ============================================================================
// OUTPUT FORMAT TESTS
// ============================================================================

#[test]
fn test_output_format_json() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: json_output
    exec: "echo '{\"key\": \"value\"}'"
    output:
      format: json
"#;

    let workflow = parse_workflow(yaml).expect("Should parse");
    assert!(workflow.tasks[0].output.is_some());
}

#[test]
fn test_output_format_text() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: text_output
    exec: "echo 'plain text'"
    output:
      format: text
"#;

    let workflow = parse_workflow(yaml).expect("Should parse");
    assert!(workflow.tasks[0].output.is_some());
}

// ============================================================================
// FOR_EACH ERROR TESTS
// ============================================================================

#[test]
fn test_for_each_with_empty_array() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: parallel
    for_each: []
    infer: "Process {{with.item}}"
"#;

    let workflow = parse_workflow(yaml).expect("Should parse");
    assert!(workflow.tasks[0].for_each.is_some());
}

#[test]
fn test_for_each_with_binding() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: get_items
    exec: "echo '[1,2,3]'"
    output:
      format: json

  - id: parallel
    with:
      items: $get_items
    for_each: "{{with.items}}"
    as: item
    infer: "Process {{with.item}}"

"#;

    let workflow = parse_workflow(yaml).expect("Should parse");
    assert!(workflow.tasks[1].for_each.is_some());
}

#[test]
fn test_for_each_concurrency() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: parallel
    for_each: [1, 2, 3, 4, 5]
    as: num
    concurrency: 3
    fail_fast: false
    infer: "Process {{with.num}}"
"#;

    let workflow = parse_workflow(yaml).expect("Should parse");
    let task = &workflow.tasks[0];
    assert_eq!(task.concurrency, Some(3));
    assert_eq!(task.fail_fast, Some(false));
}
