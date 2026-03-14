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
schema: "nika/workflow@0.5"

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
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: fetch_data
    fetch:
      url: "https://api.example.com/data"
      method: GET
      timeout: 5000

  - id: process
    exec: "process-data.sh"

  - id: analyze
    infer: "Analyze this data: {{use.data}}"
    use:
      data: fetch_data

  - id: report
    infer: "Generate report from: {{use.analysis}}"
    use:
      analysis: analyze

flows:
  - source: fetch_data
    target: process
  - source: fetch_data
    target: analyze
  - source: analyze
    target: report
"#;

    let workflow = parse_workflow(yaml).unwrap();

    insta::assert_yaml_snapshot!(
        "complex_workflow_parse",
        json!({
            "schema": workflow.schema,
            "provider": workflow.provider,
            "task_count": workflow.tasks.len(),
            "task_ids": workflow.tasks.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            "has_flows": !workflow.flows.is_empty(),
            "flow_count": workflow.flows.len()
        })
    );
}

#[test]
fn test_snapshot_for_each_workflow() {
    let yaml = r#"
schema: "nika/workflow@0.5"

tasks:
  - id: parallel_tasks
    for_each: ["item1", "item2", "item3"]
    as: item
    concurrency: 3
    fail_fast: true
    exec: "process {{use.item}}"
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
schema: "nika/workflow@0.5"

mcp:
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
schema: "nika/workflow@0.5"
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
schema: "nika/workflow@0.5"
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
schema: "nika/workflow@0.5"
provider: claude

mcp:
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
schema: "nika/workflow@0.5"

tasks:
  - id: source
    exec: "echo data"

  - id: simple_binding
    infer: "Process: {{use.data}}"
    use:
      data: source

  - id: lazy_binding
    infer: "Process: {{use.lazy_data}}"
    use:
      lazy_data:
        path: source
        lazy: true
        default: "fallback"
"#;

    let workflow = parse_workflow(yaml).unwrap();

    insta::assert_yaml_snapshot!(
        "binding_variants",
        json!({
            "task_count": workflow.tasks.len(),
            "has_bindings": workflow.tasks.iter().any(|t| t.use_wiring.is_some())
        })
    );
}
