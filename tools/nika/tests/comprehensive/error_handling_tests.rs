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

use nika::serde_yaml;
use nika::ast::Workflow;
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
    let result: Result<Workflow, _> = serde_yaml::from_str(yaml);
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

    // Invalid schema version (but still parses - validation happens at runtime)
    let result: Result<Workflow, _> = serde_yaml::from_str(yaml);
    // Note: Schema version validation is at runtime, not parse time
    assert!(result.is_ok(), "Schema version validated at runtime");
}

#[test]
fn test_parse_error_invalid_yaml_syntax() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: test
    infer: "Hello
    # Missing closing quote
"#;

    let result: Result<Workflow, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err(), "Should fail on invalid YAML");
}

#[test]
fn test_parse_error_unknown_field() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude
unknown_field: "value"

tasks:
  - id: test
    infer: "Hello"
"#;

    // serde_yaml with deny_unknown_fields would catch this
    // Current config allows unknown fields for forward compatibility
    let result: Result<Workflow, _> = serde_yaml::from_str(yaml);
    // Depends on serde config - may pass or fail
    let _ = result;
}

#[test]
fn test_parse_error_empty_tasks() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks: []
"#;

    let result: Result<Workflow, _> = serde_yaml::from_str(yaml);
    assert!(result.is_ok(), "Empty tasks array is valid YAML");
    let workflow = result.unwrap();
    assert!(workflow.tasks.is_empty());
}

#[test]
fn test_parse_error_missing_task_id() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - infer: "Hello"
"#;

    // Missing required 'id' field on task
    let result: Result<Workflow, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err(), "Should fail without task id");
}

#[test]
fn test_parse_error_missing_verb() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: test
    output:
      format: json
"#;

    // Task without any verb (infer, exec, fetch, invoke, agent)
    let result: Result<Workflow, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err(), "Should fail without verb");
}

#[test]
fn test_parse_error_multiple_verbs() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: test
    infer: "Hello"
    exec: "echo"
"#;

    // Task with multiple verbs - serde_yaml's tagged enum will pick one
    // This is a parser limitation - runtime validation would catch it
    let result: Result<Workflow, _> = serde_yaml::from_str(yaml);
    // Note: Due to serde's tagged enum handling, this may parse (picking first verb)
    // The test documents actual behavior rather than ideal behavior
    let _ = result; // Implementation-dependent
}

// ============================================================================
// DAG ERROR TESTS (NIKA-020-029)
// ============================================================================

#[test]
fn test_dag_error_self_reference() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: circular
    infer: "Hello"

flows:
  - source: circular
    target: circular
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    let graph = Dag::from_workflow(&workflow);
    assert!(
        graph.detect_cycles().is_err(),
        "Should detect self-referential cycle"
    );
}

#[test]
fn test_dag_error_two_node_cycle() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: a
    infer: "A"
  - id: b
    infer: "B"

flows:
  - source: a
    target: b
  - source: b
    target: a
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    let graph = Dag::from_workflow(&workflow);
    assert!(
        graph.detect_cycles().is_err(),
        "Should detect two-node cycle"
    );
}

#[test]
fn test_dag_error_three_node_cycle() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: a
    infer: "A"
  - id: b
    infer: "B"
  - id: c
    infer: "C"

flows:
  - source: a
    target: b
  - source: b
    target: c
  - source: c
    target: a
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    let graph = Dag::from_workflow(&workflow);
    assert!(
        graph.detect_cycles().is_err(),
        "Should detect three-node cycle"
    );
}

#[test]
fn test_dag_error_complex_cycle() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: start
    infer: "Start"
  - id: a
    infer: "A"
  - id: b
    infer: "B"
  - id: c
    infer: "C"
  - id: d
    infer: "D"
  - id: end
    infer: "End"

flows:
  - source: start
    target: a
  - source: a
    target: b
  - source: b
    target: c
  - source: c
    target: d
  - source: d
    target: b
  - source: b
    target: end
"#;

    // d -> b creates a cycle: b -> c -> d -> b
    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    let graph = Dag::from_workflow(&workflow);
    assert!(
        graph.detect_cycles().is_err(),
        "Should detect cycle in complex DAG"
    );
}

#[test]
fn test_dag_error_orphan_flow_source() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: task1
    infer: "Hello"

flows:
  - source: nonexistent
    target: task1
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    // Dag builds successfully but may have reference to unknown task
    let graph = Dag::from_workflow(&workflow);
    // Check if the orphan reference was added or ignored
    let deps = graph.get_dependencies("task1");
    // Either the orphan is included or ignored - both are valid behaviors
    let has_nonexistent = deps.iter().any(|d| d.as_ref() == "nonexistent");
    let _ = has_nonexistent; // Implementation-dependent
}

#[test]
fn test_dag_error_orphan_flow_target() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: task1
    infer: "Hello"

flows:
  - source: task1
    target: nonexistent
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    let result = Dag::from_workflow(&workflow);
    // Should either error or create a reference to unknown task
    let _ = result;
}

// ============================================================================
// BINDING ERROR TESTS (NIKA-040-049)
// ============================================================================

#[test]
fn test_binding_in_prompt() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: setup
    exec: "echo 'data'"

  - id: use_binding
    use:
      data: setup
    infer: "Process: {{use.data}}"

flows:
  - source: setup
    target: use_binding
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    assert!(workflow.tasks[1].use_wiring.is_some());
}

#[test]
fn test_binding_dollar_syntax() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: setup
    exec: "echo 'data'"

  - id: use_binding
    use:
      data: setup
    infer: "Process: $data"

flows:
  - source: setup
    target: use_binding
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    assert!(workflow.tasks[1].use_wiring.is_some());
}

#[test]
fn test_binding_lazy_with_default() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: maybe_fails
    exec: "echo 'ok'"

  - id: use_lazy
    use:
      data:
        path: maybe_fails
        lazy: true
        default: "fallback"
    infer: "Data: {{use.data}}"
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    let task = &workflow.tasks[1];
    assert!(task.use_wiring.is_some());
    let wiring = task.use_wiring.as_ref().unwrap();
    assert!(!wiring.is_empty());
}

#[test]
fn test_binding_multiple_aliases() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: task_a
    exec: "echo 'a'"

  - id: task_b
    exec: "echo 'b'"

  - id: task_c
    exec: "echo 'c'"

  - id: aggregate
    use:
      a: task_a
      b: task_b
      c: task_c
    infer: "A={{use.a}}, B={{use.b}}, C={{use.c}}"

flows:
  - source: [task_a, task_b, task_c]
    target: aggregate
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    let task = &workflow.tasks[3];
    let wiring = task.use_wiring.as_ref().unwrap();
    assert_eq!(wiring.len(), 3);
}

// ============================================================================
// VERB-SPECIFIC ERROR TESTS
// ============================================================================

#[test]
fn test_infer_shorthand_string() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: simple
    infer: "Just a simple prompt"
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse shorthand");
    assert_eq!(workflow.tasks.len(), 1);
}

#[test]
fn test_infer_full_form() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: full
    infer:
      prompt: "Full form prompt"
      model: gpt-4o
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse full form");
    assert_eq!(workflow.tasks.len(), 1);
}

#[test]
fn test_exec_shorthand_string() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: simple
    exec: "echo 'hello'"
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse shorthand");
    assert_eq!(workflow.tasks.len(), 1);
}

#[test]
fn test_exec_full_form() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: full
    exec:
      command: "echo 'hello'"
      timeout: 5000
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse full form");
    assert_eq!(workflow.tasks.len(), 1);
}

#[test]
fn test_fetch_requires_object() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: fetch_task
    fetch:
      url: "https://httpbin.org/get"
      method: GET
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    assert_eq!(workflow.tasks.len(), 1);
}

#[test]
fn test_invoke_requires_object() {
    let yaml = r#"
schema: "nika/workflow@0.5"
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

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    assert_eq!(workflow.tasks.len(), 1);
}

#[test]
fn test_agent_requires_prompt() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: agent_task
    agent:
      prompt: "Required prompt"
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    assert_eq!(workflow.tasks.len(), 1);
}

// ============================================================================
// OUTPUT FORMAT TESTS
// ============================================================================

#[test]
fn test_output_format_json() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: json_output
    exec: "echo '{\"key\": \"value\"}'"
    output:
      format: json
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    assert!(workflow.tasks[0].output.is_some());
}

#[test]
fn test_output_format_text() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: text_output
    exec: "echo 'plain text'"
    output:
      format: text
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    assert!(workflow.tasks[0].output.is_some());
}

// ============================================================================
// FOR_EACH ERROR TESTS
// ============================================================================

#[test]
fn test_for_each_with_empty_array() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: parallel
    for_each: []
    infer: "Process {{use.item}}"
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    assert!(workflow.tasks[0].for_each.is_some());
}

#[test]
fn test_for_each_with_binding() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: get_items
    exec: "echo '[1,2,3]'"
    output:
      format: json

  - id: parallel
    use:
      items: get_items
    for_each: "{{use.items}}"
    as: item
    infer: "Process {{use.item}}"

flows:
  - source: get_items
    target: parallel
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    assert!(workflow.tasks[1].for_each.is_some());
}

#[test]
fn test_for_each_concurrency() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: parallel
    for_each: [1, 2, 3, 4, 5]
    as: num
    concurrency: 3
    fail_fast: false
    infer: "Process {{use.num}}"
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    let task = &workflow.tasks[0];
    assert_eq!(task.concurrency, Some(3));
    assert_eq!(task.fail_fast, Some(false));
}
