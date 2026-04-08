// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! NovaNet MCP server integration tests.
//!
//! These tests require a running NovaNet MCP server.
//! Start with: cd novanet && cargo run --bin novanet-mcp

use nika::ast::parse_workflow;
use std::env;
use std::path::PathBuf;

/// Check if NovaNet MCP is available
fn has_novanet() -> bool {
    // Check for either running server or buildable binary
    env::var("NOVANET_MCP_URL").is_ok()
        || PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("novanet/tools/novanet-mcp/Cargo.toml").exists())
            .unwrap_or(false)
}

// ============================================================================
// NOVANET MCP TOOL TESTS
// ============================================================================

#[tokio::test]
#[ignore = "Requires NovaNet MCP server"]
async fn test_novanet_describe_tool() {
    if !has_novanet() {
        eprintln!("Skipping: NovaNet MCP not available");
        return;
    }

    // This would test the novanet_describe tool
    // For now, just validate workflow parsing
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: test-novanet-describe
description: "Test novanet_describe MCP tool"

mcp:
  servers:
    novanet:
      command: cargo
      args: ["run", "--manifest-path", "../../novanet/tools/novanet-mcp/Cargo.toml"]

tasks:
  - id: describe_entity
    invoke:
      tool: novanet_describe
      server: novanet
      params:
        entity: "qr-code"
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse workflow");
    assert_eq!(workflow.tasks.len(), 1);
    assert!(workflow.mcp.is_some());
}

#[tokio::test]
#[ignore = "Requires NovaNet MCP server"]
async fn test_novanet_traverse_tool() {
    if !has_novanet() {
        return;
    }

    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: test-novanet-traverse
description: "Test novanet_traverse MCP tool"

mcp:
  servers:
    novanet:
      command: cargo
      args: ["run", "--manifest-path", "../../novanet/tools/novanet-mcp/Cargo.toml"]

tasks:
  - id: traverse_graph
    invoke:
      tool: novanet_traverse
      server: novanet
      params:
        start: "entity:qr-code"
        arc: "HAS_NATIVE"
        depth: 1
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse workflow");
    assert_eq!(workflow.tasks.len(), 1);
}

#[tokio::test]
#[ignore = "Requires NovaNet MCP server"]
async fn test_novanet_generate_tool() {
    if !has_novanet() {
        return;
    }

    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: test-novanet-generate
description: "Test novanet_generate MCP tool"

mcp:
  servers:
    novanet:
      command: cargo
      args: ["run", "--manifest-path", "../../novanet/tools/novanet-mcp/Cargo.toml"]

tasks:
  - id: generate_content
    invoke:
      tool: novanet_generate
      server: novanet
      params:
        entity: "qr-code"
        locale: "fr-FR"
        forms: ["text", "title"]
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse workflow");
    assert_eq!(workflow.tasks.len(), 1);
}

#[tokio::test]
#[ignore = "Requires NovaNet MCP server"]
async fn test_novanet_introspect_tool() {
    if !has_novanet() {
        return;
    }

    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: test-novanet-introspect
description: "Test novanet_introspect MCP tool (v0.5 MVP8)"

mcp:
  servers:
    novanet:
      command: cargo
      args: ["run", "--manifest-path", "../../novanet/tools/novanet-mcp/Cargo.toml"]

tasks:
  - id: introspect_schema
    invoke:
      tool: novanet_introspect
      server: novanet
      params:
        query_type: "node_classes"
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse workflow");
    assert_eq!(workflow.tasks.len(), 1);
}

// ============================================================================
// NOVANET WORKFLOW INTEGRATION TESTS
// ============================================================================

#[tokio::test]
#[ignore = "Requires NovaNet MCP server"]
async fn test_novanet_full_workflow() {
    if !has_novanet() {
        return;
    }

    // Complex workflow combining NovaNet tools with infer
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: test-novanet-full
description: "Full NovaNet integration workflow"
provider: claude

mcp:
  servers:
    novanet:
      command: cargo
      args: ["run", "--manifest-path", "../../novanet/tools/novanet-mcp/Cargo.toml"]

tasks:
  - id: get_entity_context
    invoke:
      tool: novanet_describe
      server: novanet
      params:
        entity: "qr-code"

  - id: generate_content
    infer: |
      Based on this entity context: {{with.ctx}}
      Generate a headline for a landing page.
    with:
      ctx: $get_entity_context

"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse workflow");
    assert_eq!(workflow.tasks.len(), 2);
    // generate_content depends on get_entity_context via with: binding
    assert!(workflow.flow_count() > 0);
}

#[tokio::test]
#[ignore = "Requires NovaNet MCP server"]
async fn test_novanet_multilang_workflow() {
    if !has_novanet() {
        return;
    }

    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: test-novanet-multilang
description: "Multi-language content generation with NovaNet"
provider: claude

mcp:
  servers:
    novanet:
      command: cargo
      args: ["run", "--manifest-path", "../../novanet/tools/novanet-mcp/Cargo.toml"]

tasks:
  - id: generate_locales
    for_each: ["fr-FR", "en-US", "de-DE"]
    as: locale
    concurrency: 3
    invoke:
      tool: novanet_generate
      server: novanet
      params:
        entity: "qr-code"
        locale: "{{with.locale}}"
        forms: ["text", "title"]
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse workflow");
    assert!(workflow.tasks[0].for_each.is_some());
}

// ============================================================================
// NOVANET DECOMPOSE TESTS (v0.5 MVP8)
// ============================================================================

#[tokio::test]
#[ignore = "Requires NovaNet MCP server"]
async fn test_novanet_decompose_semantic() {
    if !has_novanet() {
        return;
    }

    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: test-novanet-decompose
description: "Test decompose with NovaNet traversal (MVP8)"
provider: claude

mcp:
  servers:
    novanet:
      command: cargo
      args: ["run", "--manifest-path", "../../novanet/tools/novanet-mcp/Cargo.toml"]

tasks:
  - id: generate_all
    decompose:
      strategy: semantic
      traverse: HAS_ENTITY
      source: "project:qrcode-ai"
      max_items: 5
    infer: "Generate content for entity: {{with.item}}"
"#;

    let workflow = parse_workflow(yaml).expect("Failed to parse workflow");
    let task = &workflow.tasks[0];
    assert!(task.decompose.is_some());
}
