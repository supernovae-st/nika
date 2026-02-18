//! Integration test for invoke workflow
//!
//! Tests the complete workflow parsing for invoke-novanet.yaml example,
//! verifying schema, task count, MCP config, and task action types.

use nika::ast::TaskAction;
use nika::Workflow;

// ═══════════════════════════════════════════════════════════════
// Parse Tests
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_invoke_workflow_parses() {
    let yaml = std::fs::read_to_string("examples/invoke-novanet.yaml")
        .expect("Example workflow should exist");

    let workflow: Workflow = serde_yaml::from_str(&yaml).expect("Workflow should parse");

    assert_eq!(workflow.schema, "nika/workflow@0.2");
    assert_eq!(workflow.tasks.len(), 3);

    // Verify MCP config
    let mcp = workflow.mcp.as_ref().expect("Should have MCP config");
    assert!(mcp.contains_key("novanet"));

    // Verify novanet MCP server config
    let novanet = mcp.get("novanet").expect("Should have novanet config");
    assert_eq!(novanet.command, "cargo");
    assert!(!novanet.args.is_empty());

    // Verify task types
    assert!(
        matches!(&workflow.tasks[0].action, TaskAction::Invoke { .. }),
        "First task should be Invoke"
    );
    assert!(
        matches!(&workflow.tasks[1].action, TaskAction::Invoke { .. }),
        "Second task should be Invoke"
    );
    assert!(
        matches!(&workflow.tasks[2].action, TaskAction::Infer { .. }),
        "Third task should be Infer"
    );
}

#[test]
fn test_invoke_workflow_task_ids() {
    let yaml = std::fs::read_to_string("examples/invoke-novanet.yaml")
        .expect("Example workflow should exist");

    let workflow: Workflow = serde_yaml::from_str(&yaml).expect("Workflow should parse");

    let task_ids: Vec<&str> = workflow.tasks.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(task_ids, vec!["discover", "hero_context", "generate_hero"]);
}

#[test]
fn test_invoke_workflow_flows() {
    let yaml = std::fs::read_to_string("examples/invoke-novanet.yaml")
        .expect("Example workflow should exist");

    let workflow: Workflow = serde_yaml::from_str(&yaml).expect("Workflow should parse");

    assert_eq!(workflow.flows.len(), 2);

    // Verify flow edges
    let flow_edges: Vec<(Vec<&str>, Vec<&str>)> = workflow
        .flows
        .iter()
        .map(|f| (f.source.as_vec(), f.target.as_vec()))
        .collect();

    assert_eq!(
        flow_edges,
        vec![
            (vec!["discover"], vec!["hero_context"]),
            (vec!["hero_context"], vec!["generate_hero"]),
        ]
    );
}

#[test]
fn test_invoke_workflow_mcp_env() {
    let yaml = std::fs::read_to_string("examples/invoke-novanet.yaml")
        .expect("Example workflow should exist");

    let workflow: Workflow = serde_yaml::from_str(&yaml).expect("Workflow should parse");

    let mcp = workflow.mcp.as_ref().expect("Should have MCP config");
    let novanet = mcp.get("novanet").expect("Should have novanet config");

    // Verify env vars
    assert!(novanet.env.contains_key("RUST_LOG"));
    assert_eq!(novanet.env.get("RUST_LOG"), Some(&"info".to_string()));
}

// ═══════════════════════════════════════════════════════════════
// Execution Tests (require running infrastructure)
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
#[ignore] // Requires running NovaNet MCP server
async fn test_invoke_workflow_executes() {
    // This test requires:
    // 1. NovaNet MCP server running
    // 2. Neo4j with test data
    // For now, just verify the workflow can be loaded
    let _yaml = std::fs::read_to_string("examples/invoke-novanet.yaml")
        .expect("Example workflow should exist");
}
