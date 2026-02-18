//! Integration tests for Workflow MCP configuration
//!
//! Tests YAML deserialization of the `mcp` field in Workflow struct.
//! This field was added in v0.2 to support inline MCP server configuration.

use nika::Workflow;

// ===================================================================
// V0.2 Workflow with MCP Config Tests
// ===================================================================

#[test]
fn test_workflow_with_mcp_config() {
    // Full v0.2 workflow with mcp block
    let yaml = r#"
schema: "nika/workflow@0.2"
provider: claude

mcp:
  novanet:
    command: cargo
    args:
      - run
      - -p
      - novanet-mcp
    env:
      NEO4J_URI: bolt://localhost:7687

tasks:
  - id: get_context
    invoke:
      mcp: novanet
      tool: novanet_generate
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse workflow");

    // Verify basic workflow fields
    assert_eq!(workflow.schema, "nika/workflow@0.2");
    assert_eq!(workflow.provider, "claude");
    assert_eq!(workflow.tasks.len(), 1);

    // Verify mcp config exists
    let mcp = workflow.mcp.expect("mcp field should be present");
    assert_eq!(mcp.len(), 1);

    // Verify novanet config
    let novanet = mcp.get("novanet").expect("novanet config should exist");
    assert_eq!(novanet.command, "cargo");
    assert_eq!(novanet.args, vec!["run", "-p", "novanet-mcp"]);
    assert_eq!(
        novanet.env.get("NEO4J_URI"),
        Some(&"bolt://localhost:7687".to_string())
    );
    assert!(novanet.cwd.is_none());
}

#[test]
fn test_workflow_without_mcp_config_v01() {
    // V0.1 workflow without mcp (backward compatibility)
    let yaml = r#"
schema: "nika/workflow@0.1"
provider: claude

tasks:
  - id: hello
    infer:
      prompt: "Say hello"
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse workflow");

    // Verify basic fields
    assert_eq!(workflow.schema, "nika/workflow@0.1");
    assert_eq!(workflow.provider, "claude");
    assert_eq!(workflow.tasks.len(), 1);

    // mcp should be None for v0.1 workflows
    assert!(
        workflow.mcp.is_none(),
        "mcp should be None for v0.1 workflows"
    );
}

#[test]
fn test_workflow_mcp_config_minimal() {
    // Minimal mcp config (command only, no args/env/cwd)
    let yaml = r#"
schema: "nika/workflow@0.2"
provider: claude

mcp:
  simple_server:
    command: ./my-mcp-server

tasks:
  - id: test
    invoke:
      mcp: simple_server
      tool: ping
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse workflow");

    let mcp = workflow.mcp.expect("mcp field should be present");
    let server = mcp
        .get("simple_server")
        .expect("simple_server config should exist");

    assert_eq!(server.command, "./my-mcp-server");
    assert!(server.args.is_empty(), "args should default to empty vec");
    assert!(server.env.is_empty(), "env should default to empty map");
    assert!(server.cwd.is_none(), "cwd should be None");
}

// ===================================================================
// Multiple MCP Servers
// ===================================================================

#[test]
fn test_workflow_multiple_mcp_servers() {
    let yaml = r#"
schema: "nika/workflow@0.2"
provider: claude

mcp:
  novanet:
    command: cargo
    args: ["run", "-p", "novanet-mcp"]
  filesystem:
    command: npx
    args: ["-y", "@anthropic/filesystem-mcp"]
    cwd: /tmp/workspace

tasks:
  - id: test
    invoke:
      mcp: novanet
      tool: test
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse workflow");

    let mcp = workflow.mcp.expect("mcp field should be present");
    assert_eq!(mcp.len(), 2);

    // Verify novanet
    let novanet = mcp.get("novanet").expect("novanet should exist");
    assert_eq!(novanet.command, "cargo");
    assert_eq!(novanet.args, vec!["run", "-p", "novanet-mcp"]);

    // Verify filesystem
    let fs = mcp.get("filesystem").expect("filesystem should exist");
    assert_eq!(fs.command, "npx");
    assert_eq!(fs.args, vec!["-y", "@anthropic/filesystem-mcp"]);
    assert_eq!(fs.cwd, Some("/tmp/workspace".to_string()));
}

// ===================================================================
// Edge Cases
// ===================================================================

#[test]
fn test_workflow_mcp_with_complex_env() {
    let yaml = r#"
schema: "nika/workflow@0.2"
provider: claude

mcp:
  database:
    command: ./db-mcp
    env:
      DATABASE_URL: postgres://user:pass@localhost:5432/db
      LOG_LEVEL: debug
      ENABLE_CACHE: "true"

tasks:
  - id: query
    invoke:
      mcp: database
      tool: query
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse workflow");

    let mcp = workflow.mcp.expect("mcp should be present");
    let db = mcp.get("database").expect("database should exist");

    assert_eq!(db.env.len(), 3);
    assert_eq!(
        db.env.get("DATABASE_URL"),
        Some(&"postgres://user:pass@localhost:5432/db".to_string())
    );
    assert_eq!(db.env.get("LOG_LEVEL"), Some(&"debug".to_string()));
    assert_eq!(db.env.get("ENABLE_CACHE"), Some(&"true".to_string()));
}

#[test]
fn test_workflow_empty_mcp_block() {
    // Empty mcp block (valid but unusual)
    let yaml = r#"
schema: "nika/workflow@0.2"
provider: claude

mcp: {}

tasks:
  - id: hello
    infer:
      prompt: "Say hello"
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse workflow");

    let mcp = workflow.mcp.expect("mcp field should be Some even if empty");
    assert!(mcp.is_empty(), "mcp should be an empty map");
}
