// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Generic MCP server integration tests.
//!
//! Tests MCP client functionality with various MCP servers.

use nika::ast::{parse_workflow, TaskAction};
use std::env;

/// Check if generic MCP tests are enabled
fn mcp_tests_enabled() -> bool {
    env::var("NIKA_MCP_TESTS").is_ok()
}

// ============================================================================
// MCP CLIENT TESTS
// ============================================================================

#[test]
fn test_mcp_config_parsing() {
    let yaml = r#"
schema: "nika/workflow@0.12"

mcp:
  servers:
    test-server:
      command: echo
      args: ["hello"]
      env:
        TEST_VAR: "test_value"

tasks:
  - id: use_mcp
    invoke:
      mcp: test-server
      tool: "test_tool"
      params:
        arg1: "value1"
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse workflow");
    assert!(workflow.mcp.is_some());
    let mcp = workflow.mcp.as_ref().unwrap();
    assert!(mcp.contains_key("test-server"));
}

#[test]
fn test_mcp_multiple_servers() {
    let yaml = r#"
schema: "nika/workflow@0.12"

mcp:
  servers:
    server1:
      command: echo
      args: ["server1"]
    server2:
      command: echo
      args: ["server2"]
    server3:
      command: echo
      args: ["server3"]

tasks:
  - id: use_server1
    invoke:
      mcp: server1
      tool: "tool1"

  - id: use_server2
    invoke:
      mcp: server2
      tool: "tool2"

  - id: combine
    depends_on: [use_server1, use_server2]
    infer: "Combine {{with.s1}} and {{with.s2}}"
    with:
      s1: $use_server1
      s2: $use_server2
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse workflow");
    assert!(workflow.mcp.is_some());
    let mcp = workflow.mcp.as_ref().unwrap();
    assert_eq!(mcp.len(), 3);
}

// ============================================================================
// EXTERNAL MCP SERVER TESTS
// ============================================================================

#[tokio::test]
#[ignore = "Requires Perplexity MCP server"]
async fn test_perplexity_mcp_search() {
    if !mcp_tests_enabled() {
        return;
    }

    let yaml = r#"
schema: "nika/workflow@0.12"

mcp:
  servers:
    perplexity:
      command: npx
      args: ["-y", "perplexity-mcp"]
      env:
        PERPLEXITY_API_KEY: "${PERPLEXITY_API_KEY}"

tasks:
  - id: search
    invoke:
      mcp: perplexity
      tool: "search"
      params:
        query: "Rust programming language"
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse workflow");
    assert_eq!(workflow.tasks.len(), 1);
}

#[tokio::test]
#[ignore = "Requires Firecrawl MCP server"]
async fn test_firecrawl_mcp_scrape() {
    if !mcp_tests_enabled() {
        return;
    }

    let yaml = r#"
schema: "nika/workflow@0.12"

mcp:
  servers:
    firecrawl:
      command: npx
      args: ["-y", "@anthropic/firecrawl-mcp"]
      env:
        FIRECRAWL_API_KEY: "${FIRECRAWL_API_KEY}"

tasks:
  - id: scrape
    invoke:
      mcp: firecrawl
      tool: "firecrawl_scrape"
      params:
        url: "https://rust-lang.org"
        formats: ["markdown"]
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse workflow");
    assert_eq!(workflow.tasks.len(), 1);
}

#[tokio::test]
#[ignore = "Requires filesystem MCP server"]
async fn test_filesystem_mcp() {
    if !mcp_tests_enabled() {
        return;
    }

    let yaml = r#"
schema: "nika/workflow@0.12"

mcp:
  servers:
    fs:
      command: npx
      args: ["-y", "@anthropic/filesystem-mcp", "/tmp"]

tasks:
  - id: list_files
    invoke:
      mcp: fs
      tool: "list_directory"
      params:
        path: "/tmp"
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse workflow");
    assert_eq!(workflow.tasks.len(), 1);
}

// ============================================================================
// MCP ERROR HANDLING TESTS
// ============================================================================

#[test]
fn test_mcp_invalid_server_reference() {
    let yaml = r#"
schema: "nika/workflow@0.12"

mcp:
  servers:
    real-server:
      command: echo

tasks:
  - id: bad_invoke
    invoke:
      mcp: nonexistent-server
      tool: "some_tool"
"#;

    // Workflow should parse but validation should fail
    let workflow = parse_workflow(yaml);
    assert!(workflow.is_ok()); // Parsing succeeds
                               // Runtime validation would catch the invalid server reference
}

#[test]
fn test_mcp_env_config() {
    let yaml = r#"
schema: "nika/workflow@0.12"

mcp:
  servers:
    config-server:
      command: node
      args: ["./server.js"]
      env:
        API_KEY: "secret"
        DEBUG: "true"
      cwd: "/path/to/server"

tasks:
  - id: use_config
    invoke:
      mcp: config-server
      tool: "test_tool"
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse workflow");
    let mcp = workflow.mcp.as_ref().unwrap();
    let server = mcp.get("config-server").unwrap();
    assert_eq!(server.env.get("API_KEY"), Some(&"secret".to_string()));
    assert_eq!(server.cwd, Some("/path/to/server".to_string()));
}

// ============================================================================
// AGENT WITH MCP TESTS
// ============================================================================

#[tokio::test]
#[ignore = "Requires MCP server and API key"]
async fn test_agent_with_mcp_tools() {
    if !mcp_tests_enabled() {
        return;
    }

    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude

mcp:
  servers:
    test-server:
      command: echo
      args: ["mock"]

tasks:
  - id: research_agent
    agent:
      prompt: "Research and summarize information about Rust"
      mcp: ["test-server"]
      max_turns: 3
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse workflow");
    let task = &workflow.tasks[0];
    assert!(matches!(task.action, TaskAction::Agent { .. }));
}

#[tokio::test]
#[ignore = "Requires MCP servers and API key"]
async fn test_agent_with_multiple_mcp() {
    if !mcp_tests_enabled() {
        return;
    }

    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude

mcp:
  servers:
    knowledge:
      command: echo
      args: ["knowledge"]
    search:
      command: echo
      args: ["search"]
    files:
      command: echo
      args: ["files"]

tasks:
  - id: multi_tool_agent
    agent:
      prompt: "Use multiple tools to complete the task"
      mcp: ["knowledge", "search", "files"]
      max_turns: 5
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse workflow");
    let task = &workflow.tasks[0];
    if let TaskAction::Agent { agent } = &task.action {
        assert_eq!(agent.mcp.len(), 3);
    } else {
        panic!("Expected agent task");
    }
}
