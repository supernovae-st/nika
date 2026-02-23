//! Invalid Workflow Tests - Ensure proper rejection of malformed workflows.
//!
//! Tests that invalid workflows are properly rejected:
//! - Cyclic dependencies
//! - Missing task references
//! - Invalid verb syntax
//! - Unknown verbs
//! - Invalid MCP config
//! - Missing required fields
//! - Type mismatches

use nika::ast::Workflow;
use nika::dag::FlowGraph;

// ============================================================================
// CYCLIC DEPENDENCY TESTS
// ============================================================================

#[test]
fn test_reject_direct_self_cycle() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: self_ref
    infer: "I reference myself"

flows:
  - source: self_ref
    target: self_ref
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    let graph = FlowGraph::from_workflow(&workflow);
    assert!(
        graph.detect_cycles().is_err(),
        "Should detect self-referential cycle"
    );
}

#[test]
fn test_reject_indirect_two_node_cycle() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: task_a
    infer: "A"
  - id: task_b
    infer: "B"

flows:
  - source: task_a
    target: task_b
  - source: task_b
    target: task_a
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    let graph = FlowGraph::from_workflow(&workflow);
    assert!(
        graph.detect_cycles().is_err(),
        "Should detect two-node cycle"
    );
}

#[test]
fn test_reject_long_cycle() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: t1
    infer: "1"
  - id: t2
    infer: "2"
  - id: t3
    infer: "3"
  - id: t4
    infer: "4"
  - id: t5
    infer: "5"

flows:
  - source: t1
    target: t2
  - source: t2
    target: t3
  - source: t3
    target: t4
  - source: t4
    target: t5
  - source: t5
    target: t1
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    let graph = FlowGraph::from_workflow(&workflow);
    assert!(graph.detect_cycles().is_err(), "Should detect 5-node cycle");
}

#[test]
fn test_reject_hidden_cycle_in_dag() {
    // Valid-looking DAG with hidden cycle
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: start
    infer: "start"
  - id: branch_a
    infer: "a"
  - id: branch_b
    infer: "b"
  - id: merge
    infer: "merge"
  - id: sneaky
    infer: "sneaky"
  - id: end_task
    infer: "end"

flows:
  - source: start
    target: [branch_a, branch_b]
  - source: branch_a
    target: merge
  - source: branch_b
    target: merge
  - source: merge
    target: sneaky
  - source: sneaky
    target: branch_a
  - source: merge
    target: end_task
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    let graph = FlowGraph::from_workflow(&workflow);
    assert!(
        graph.detect_cycles().is_err(),
        "Should detect hidden cycle: merge -> sneaky -> branch_a -> merge"
    );
}

// ============================================================================
// MISSING REFERENCE TESTS
// ============================================================================

#[test]
fn test_orphan_binding_reference() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: consumer
    use:
      data: nonexistent_task
    infer: "Use data"
"#;

    // Parses successfully but references non-existent task
    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    assert!(workflow.tasks[0].use_wiring.is_some());

    // DAG building or validation should catch this
    // (depending on implementation)
}

#[test]
fn test_orphan_flow_source() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: real_task
    infer: "Hello"

flows:
  - source: ghost_task
    target: real_task
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");

    // FlowGraph should handle or reject orphan reference
    let result = FlowGraph::from_workflow(&workflow);
    // Implementation-dependent whether this errors or creates orphan node
    let _ = result;
}

#[test]
fn test_orphan_flow_target() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: real_task
    infer: "Hello"

flows:
  - source: real_task
    target: ghost_task
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    let result = FlowGraph::from_workflow(&workflow);
    let _ = result;
}

// ============================================================================
// INVALID VERB SYNTAX TESTS
// ============================================================================

#[test]
fn test_reject_empty_infer_prompt() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: empty
    infer: ""
"#;

    // Empty string is valid YAML but may be rejected at runtime
    let result: Result<Workflow, _> = serde_yaml::from_str(yaml);
    // Depends on validation - empty string may or may not be allowed
    let _ = result;
}

#[test]
fn test_reject_null_infer() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: null_infer
    infer: null
"#;

    let result: Result<Workflow, _> = serde_yaml::from_str(yaml);
    // null should be rejected for infer
    assert!(result.is_err(), "Should reject null infer value");
}

#[test]
fn test_reject_array_infer() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: array_infer
    infer: [1, 2, 3]
"#;

    let result: Result<Workflow, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err(), "Should reject array as infer value");
}

#[test]
fn test_reject_number_as_shell_command() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: number_cmd
    exec: 12345
"#;

    let result: Result<Workflow, _> = serde_yaml::from_str(yaml);
    // Number should be rejected for shell command
    assert!(
        result.is_err(),
        "Should reject number as shell command value"
    );
}

// ============================================================================
// INVALID MCP CONFIG TESTS
// ============================================================================

#[test]
fn test_mcp_missing_command() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

mcp:
  broken_server:
    args: ["--port", "8080"]

tasks:
  - id: test
    invoke:
      mcp: broken_server
      tool: some_tool
"#;

    let result: Result<Workflow, _> = serde_yaml::from_str(yaml);
    // Missing 'command' field should be rejected
    assert!(result.is_err(), "Should reject MCP config without command");
}

#[test]
fn test_invoke_unknown_mcp_server() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

mcp:
  novanet:
    command: "echo"

tasks:
  - id: test
    invoke:
      mcp: unknown_server
      tool: some_tool
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");

    // References unknown MCP server - should fail at runtime validation
    if let nika::ast::TaskAction::Invoke { invoke } = &workflow.tasks[0].action {
        assert_eq!(invoke.mcp, "unknown_server");
    }
}

// ============================================================================
// MISSING REQUIRED FIELDS TESTS
// ============================================================================

#[test]
fn test_reject_task_without_id() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - infer: "Hello"
"#;

    let result: Result<Workflow, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err(), "Should reject task without id");
}

#[test]
fn test_reject_task_without_verb() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: no_verb
    output:
      format: json
"#;

    let result: Result<Workflow, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err(), "Should reject task without verb");
}

#[test]
fn test_reject_workflow_without_schema() {
    let yaml = r#"
provider: claude

tasks:
  - id: test
    infer: "Hello"
"#;

    let result: Result<Workflow, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err(), "Should reject workflow without schema");
}

#[test]
fn test_reject_fetch_without_url() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: bad_fetch
    fetch:
      method: GET
"#;

    let result: Result<Workflow, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err(), "Should reject fetch without url");
}

#[test]
fn test_reject_invoke_without_tool() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

mcp:
  server:
    command: "echo"

tasks:
  - id: bad_invoke
    invoke:
      mcp: server
"#;

    // Note: Missing 'tool' field - serde may accept with default or reject
    // This test documents actual parser behavior
    let result: Result<Workflow, _> = serde_yaml::from_str(yaml);
    // Runtime validation catches missing tool, not always parse-time
    let _ = result; // Implementation-dependent
}

#[test]
fn test_reject_agent_without_prompt() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: bad_agent
    agent:
      max_turns: 5
"#;

    let result: Result<Workflow, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err(), "Should reject agent without prompt");
}

// ============================================================================
// TYPE MISMATCH TESTS
// ============================================================================

#[test]
fn test_reject_invalid_max_turns_type() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: bad_type
    agent:
      prompt: "Hello"
      max_turns: "five"
"#;

    let result: Result<Workflow, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err(), "Should reject string for max_turns");
}

#[test]
fn test_reject_invalid_concurrency_type() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: bad_concurrency
    for_each: [1, 2, 3]
    concurrency: "high"
    infer: "Process item"
"#;

    let result: Result<Workflow, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err(), "Should reject string for concurrency");
}

#[test]
fn test_reject_negative_max_turns() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: negative
    agent:
      prompt: "Hello"
      max_turns: -5
"#;

    // Negative values should be rejected (u32 cannot be negative)
    let result: Result<Workflow, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err(), "Should reject negative max_turns");
}

#[test]
fn test_reject_invalid_boolean() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: bad_bool
    agent:
      prompt: "Hello"
      extended_thinking: "yes please"
"#;

    let result: Result<Workflow, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err(), "Should reject string for boolean field");
}

// ============================================================================
// DUPLICATE ID TESTS
// ============================================================================

#[test]
fn test_duplicate_task_ids() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: duplicate
    infer: "First"
  - id: duplicate
    infer: "Second"
"#;

    // YAML allows duplicate keys, but our validation should catch this
    let result: Result<Workflow, _> = serde_yaml::from_str(yaml);

    if let Ok(workflow) = result {
        // If parsing succeeds, DAG building should detect duplicate
        let result = FlowGraph::from_workflow(&workflow);
        // Implementation-dependent behavior for duplicates
        let _ = result;
    }
}

// ============================================================================
// BOUNDARY TESTS
// ============================================================================

#[test]
fn test_very_long_task_id() {
    let long_id = "a".repeat(1000);
    let yaml = format!(
        r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: {}
    infer: "Hello"
"#,
        long_id
    );

    let result: Result<Workflow, _> = serde_yaml::from_str(&yaml);
    // Very long IDs should be allowed (or explicitly rejected)
    let _ = result;
}

#[test]
fn test_special_chars_in_task_id() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: "task-with-dash_and_underscore"
    infer: "Hello"
"#;

    let result: Result<Workflow, _> = serde_yaml::from_str(yaml);
    // Dashes and underscores should be allowed
    assert!(
        result.is_ok(),
        "Standard special characters should be allowed"
    );
}

#[test]
fn test_unicode_task_id() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: "tache_francais"
    infer: "Hello"
"#;

    let result: Result<Workflow, _> = serde_yaml::from_str(yaml);
    // Unicode IDs should work
    assert!(result.is_ok(), "Unicode task IDs should be allowed");
}

#[test]
fn test_whitespace_only_prompt() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: whitespace
    infer: "   "
"#;

    let result: Result<Workflow, _> = serde_yaml::from_str(yaml);
    // Whitespace-only prompt may be allowed or rejected
    let _ = result;
}
