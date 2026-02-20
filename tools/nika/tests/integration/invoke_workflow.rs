//! Integration test for invoke workflow
//!
//! Tests the complete workflow parsing for invoke-novanet.nika.yaml example,
//! verifying schema, task count, MCP config, and task action types.

use nika::ast::TaskAction;
use nika::Workflow;

// ═══════════════════════════════════════════════════════════════
// Parse Tests
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_invoke_workflow_parses() {
    let yaml = std::fs::read_to_string("examples/invoke-novanet.nika.yaml")
        .expect("Example workflow should exist");

    let workflow: Workflow = serde_yaml::from_str(&yaml).expect("Workflow should parse");

    assert_eq!(workflow.schema, "nika/workflow@0.2");
    assert_eq!(workflow.tasks.len(), 4);

    // Verify MCP config
    let mcp = workflow.mcp.as_ref().expect("Should have MCP config");
    assert!(mcp.contains_key("novanet"));

    // Verify novanet MCP server config
    let novanet = mcp.get("novanet").expect("Should have novanet config");
    assert_eq!(novanet.command, "cargo");
    assert!(!novanet.args.is_empty());

    // Verify task types: Invoke, Invoke, Invoke, Infer
    assert!(
        matches!(&workflow.tasks[0].action, TaskAction::Invoke { .. }),
        "First task should be Invoke (schema)"
    );
    assert!(
        matches!(&workflow.tasks[1].action, TaskAction::Invoke { .. }),
        "Second task should be Invoke (entity_context)"
    );
    assert!(
        matches!(&workflow.tasks[2].action, TaskAction::Invoke { .. }),
        "Third task should be Invoke (traverse_native)"
    );
    assert!(
        matches!(&workflow.tasks[3].action, TaskAction::Infer { .. }),
        "Fourth task should be Infer (generate_content)"
    );
}

#[test]
fn test_invoke_workflow_task_ids() {
    let yaml = std::fs::read_to_string("examples/invoke-novanet.nika.yaml")
        .expect("Example workflow should exist");

    let workflow: Workflow = serde_yaml::from_str(&yaml).expect("Workflow should parse");

    let task_ids: Vec<&str> = workflow.tasks.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(
        task_ids,
        vec![
            "schema",
            "entity_context",
            "traverse_native",
            "generate_content"
        ]
    );
}

#[test]
fn test_invoke_workflow_flows() {
    let yaml = std::fs::read_to_string("examples/invoke-novanet.nika.yaml")
        .expect("Example workflow should exist");

    let workflow: Workflow = serde_yaml::from_str(&yaml).expect("Workflow should parse");

    assert_eq!(workflow.flows.len(), 3);

    // Verify flow edges
    let flow_edges: Vec<(Vec<&str>, Vec<&str>)> = workflow
        .flows
        .iter()
        .map(|f| (f.source.as_vec(), f.target.as_vec()))
        .collect();

    assert_eq!(
        flow_edges,
        vec![
            (vec!["schema"], vec!["entity_context"]),
            (vec!["entity_context"], vec!["traverse_native"]),
            (vec!["traverse_native"], vec!["generate_content"]),
        ]
    );
}

#[test]
fn test_invoke_workflow_mcp_env() {
    let yaml = std::fs::read_to_string("examples/invoke-novanet.nika.yaml")
        .expect("Example workflow should exist");

    let workflow: Workflow = serde_yaml::from_str(&yaml).expect("Workflow should parse");

    let mcp = workflow.mcp.as_ref().expect("Should have MCP config");
    let novanet = mcp.get("novanet").expect("Should have novanet config");

    // Verify essential Neo4j env vars are present
    // Note: RUST_LOG is intentionally NOT set in workflow config to avoid TUI pollution
    // The MCP child process gets RUST_LOG=off from rmcp_adapter.rs
    assert!(novanet.env.contains_key("NOVANET_MCP_NEO4J_URI"));
    assert!(novanet.env.contains_key("NOVANET_MCP_NEO4J_USER"));
    assert!(novanet.env.contains_key("NOVANET_MCP_NEO4J_PASSWORD"));
}
