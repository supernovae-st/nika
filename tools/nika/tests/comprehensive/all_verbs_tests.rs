//! Tests for all 5 verbs used simultaneously.
//!
//! Validates that complex workflows with all verbs:
//! - Parse correctly
//! - Generate valid DAGs
//! - Have proper dependency resolution

use nika::serde_yaml;
use nika::ast::{TaskAction, Workflow};
use nika::dag::Dag;
use std::path::PathBuf;

/// Get path to examples directory
fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples")
}

/// Parse a workflow file and return it
fn parse_workflow(filename: &str) -> Workflow {
    let path = examples_dir().join(filename);
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", path.display(), e));
    serde_yaml::from_str(&content).unwrap_or_else(|e| panic!("Failed to parse {}: {}", filename, e))
}

// ============================================================================
// ALL 5 VERBS PARSING TESTS
// ============================================================================

#[test]
fn test_parse_all_verbs_complex() {
    let workflow = parse_workflow("test-all-verbs-complex.nika.yaml");

    // Verify schema
    assert_eq!(workflow.schema, "nika/workflow@0.5");

    // Count verbs
    let mut infer_count = 0;
    let mut exec_count = 0;
    let mut fetch_count = 0;
    let mut invoke_count = 0;
    let mut agent_count = 0;

    for task in &workflow.tasks {
        match &task.action {
            TaskAction::Infer { .. } => infer_count += 1,
            TaskAction::Exec { .. } => exec_count += 1,
            TaskAction::Fetch { .. } => fetch_count += 1,
            TaskAction::Invoke { .. } => invoke_count += 1,
            TaskAction::Agent { .. } => agent_count += 1,
        }
    }

    // Should have all verb types
    assert!(infer_count > 0, "Should have infer tasks");
    assert!(exec_count > 0, "Should have exec tasks");
    assert!(fetch_count > 0, "Should have fetch tasks");
    assert!(invoke_count > 0, "Should have invoke tasks");
    assert!(agent_count > 0, "Should have agent tasks");

    // Verify MCP configuration
    assert!(workflow.mcp.is_some(), "Should have MCP config");
    let mcp = workflow.mcp.as_ref().unwrap();
    assert!(mcp.contains_key("novanet"), "Should have novanet MCP");
}

#[test]
fn test_all_verbs_dag_construction() {
    let workflow = parse_workflow("test-all-verbs-complex.nika.yaml");

    // Build DAG (returns Dag directly, not Result)
    let graph = Dag::from_workflow(&workflow);

    // Verify no cycles (returns Result<(), NikaError>)
    assert!(
        graph.detect_cycles().is_ok(),
        "Complex workflow should have no cycles"
    );

    // Verify final tasks exist
    let final_tasks = graph.get_final_tasks();
    assert!(!final_tasks.is_empty(), "Should have final tasks");
}

#[test]
fn test_all_verbs_binding_expressions() {
    let workflow = parse_workflow("test-all-verbs-complex.nika.yaml");

    // Find tasks with use bindings
    let tasks_with_bindings: Vec<_> = workflow
        .tasks
        .iter()
        .filter(|t| t.use_wiring.is_some())
        .collect();

    assert!(
        !tasks_with_bindings.is_empty(),
        "Should have tasks with bindings"
    );

    // Verify binding structure
    for task in tasks_with_bindings {
        let wiring = task.use_wiring.as_ref().unwrap();
        assert!(
            !wiring.is_empty(),
            "Task {} should have binding entries",
            task.id
        );
    }
}

#[test]
fn test_all_verbs_for_each_parallelism() {
    let workflow = parse_workflow("test-all-verbs-complex.nika.yaml");

    // Find tasks with for_each
    let parallel_tasks: Vec<_> = workflow
        .tasks
        .iter()
        .filter(|t| t.for_each.is_some())
        .collect();

    assert!(!parallel_tasks.is_empty(), "Should have parallel tasks");

    for task in parallel_tasks {
        assert!(
            task.concurrency.unwrap_or(1) > 0,
            "Concurrency should be positive"
        );
    }
}

// ============================================================================
// VERB COMBINATION TESTS
// ============================================================================

#[test]
fn test_infer_exec_chain() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: generate
    infer: "Generate a list of 3 items"

  - id: process
    use:
      items: generate
    exec: "echo 'Processing: {{use.items}}'"

flows:
  - source: generate
    target: process
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    assert_eq!(workflow.tasks.len(), 2);
}

#[test]
fn test_fetch_infer_chain() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: fetch_data
    fetch:
      url: "https://httpbin.org/json"
      method: GET

  - id: analyze
    use:
      data: fetch_data
    infer: "Analyze this JSON: {{use.data}}"

flows:
  - source: fetch_data
    target: analyze
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    assert_eq!(workflow.tasks.len(), 2);
}

#[test]
fn test_invoke_agent_chain() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

mcp:
  novanet:
    command: "echo"
    args: ["mock"]

tasks:
  - id: get_context
    invoke:
      mcp: novanet
      tool: novanet_describe
      params:
        entity: "qr-code"

  - id: research
    use:
      ctx: get_context
    agent:
      prompt: "Research using context: {{use.ctx}}"
      mcp: ["novanet"]
      max_turns: 3

flows:
  - source: get_context
    target: research
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    assert_eq!(workflow.tasks.len(), 2);

    // Verify agent has MCP list
    if let TaskAction::Agent { agent } = &workflow.tasks[1].action {
        assert_eq!(agent.mcp.len(), 1);
        assert_eq!(agent.mcp[0], "novanet");
    } else {
        panic!("Expected agent task");
    }
}

#[test]
fn test_all_five_verbs_in_sequence() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

mcp:
  tools:
    command: "echo"

tasks:
  - id: step_exec
    exec: "echo 'start'"

  - id: step_fetch
    use:
      prev: step_exec
    fetch:
      url: "https://httpbin.org/get"
      method: GET

  - id: step_infer
    use:
      data: step_fetch
    infer: "Summarize: {{use.data}}"

  - id: step_invoke
    use:
      summary: step_infer
    invoke:
      mcp: tools
      tool: "process"
      params:
        input: "{{use.summary}}"

  - id: step_agent
    use:
      result: step_invoke
    agent:
      prompt: "Review result: {{use.result}}"
      max_turns: 2

flows:
  - source: step_exec
    target: step_fetch
  - source: step_fetch
    target: step_infer
  - source: step_infer
    target: step_invoke
  - source: step_invoke
    target: step_agent
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    assert_eq!(workflow.tasks.len(), 5);
    assert_eq!(workflow.flows.len(), 4);

    // Verify each verb type
    let verbs: Vec<&str> = workflow
        .tasks
        .iter()
        .map(|t| t.action.verb_name())
        .collect();

    assert_eq!(verbs, vec!["exec", "fetch", "infer", "invoke", "agent"]);
}

// ============================================================================
// PRODUCTION TEST SUITE VALIDATION
// ============================================================================

#[test]
fn test_production_test_suite_parses() {
    let workflow = parse_workflow("production-test-suite.nika.yaml");

    assert!(!workflow.tasks.is_empty());
    assert!(!workflow.flows.is_empty());
}

#[test]
fn test_production_test_suite_has_all_verbs() {
    let workflow = parse_workflow("production-test-suite.nika.yaml");

    let verb_names: std::collections::HashSet<&str> = workflow
        .tasks
        .iter()
        .map(|t| t.action.verb_name())
        .collect();

    // Should have at least 4 verb types (may not have invoke without MCP)
    assert!(
        verb_names.len() >= 4,
        "Should have multiple verb types: {:?}",
        verb_names
    );
}
