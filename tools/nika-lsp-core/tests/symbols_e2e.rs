// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! E2E tests for `document_symbols` handler.
//!
//! These tests exercise the public API from the consumer perspective,
//! feeding realistic workflow YAML and asserting on the symbol tree.

use nika_lsp_core::handlers::symbols::{document_symbols, SymbolKind};

// ---------------------------------------------------------------------------
// 1. Full workflow -> schema + tasks container + individual tasks
// ---------------------------------------------------------------------------
#[test]
fn full_workflow_produces_schema_and_tasks() {
    let yaml = "\
schema: nika/workflow@0.12
workflow: media-pipeline
tasks:
  - id: download
    exec: \"curl -O {{with.url}}\"
  - id: caption
    infer: \"describe this image\"
  - id: upload
    exec: \"aws s3 cp out.png s3://bucket\"
";

    let symbols = document_symbols(yaml);

    // Top-level: schema, workflow, tasks
    let schema = symbols
        .iter()
        .find(|s| s.name.starts_with("schema:"))
        .expect("should contain a schema symbol");
    assert_eq!(schema.kind, SymbolKind::Property);

    let workflow = symbols
        .iter()
        .find(|s| s.name.starts_with("workflow:"))
        .expect("should contain a workflow symbol");
    assert_eq!(workflow.kind, SymbolKind::File);

    let tasks = symbols
        .iter()
        .find(|s| s.name.starts_with("tasks"))
        .expect("should contain a tasks symbol");
    assert_eq!(tasks.kind, SymbolKind::Module);

    // Individual tasks are children of the tasks container.
    assert_eq!(tasks.children.len(), 3, "should have 3 task children");

    let names: Vec<&str> = tasks.children.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["download", "caption", "upload"]);

    for child in &tasks.children {
        assert_eq!(child.kind, SymbolKind::Function);
    }
}

// ---------------------------------------------------------------------------
// 2. Task with verb -> verb as child symbol
// ---------------------------------------------------------------------------
#[test]
fn task_verb_appears_as_child_symbol() {
    let yaml = "\
tasks:
  - id: summarise
    infer: \"summarise the document\"
    with:
      doc: input.txt
";

    let symbols = document_symbols(yaml);
    let tasks = &symbols[0];
    let task = &tasks.children[0];

    assert_eq!(task.name, "summarise");

    let verb = task
        .children
        .iter()
        .find(|c| c.name == "infer")
        .expect("infer verb should be a child of the task");
    assert_eq!(verb.kind, SymbolKind::Function);

    let with = task
        .children
        .iter()
        .find(|c| c.name == "with")
        .expect("with section should be a child of the task");
    assert_eq!(with.kind, SymbolKind::Variable);
}

// ---------------------------------------------------------------------------
// 3. MCP section -> server symbols
// ---------------------------------------------------------------------------
#[test]
fn mcp_section_produces_server_symbols() {
    let yaml = "\
schema: nika/workflow@0.12
mcp:
  novanet:
    command: novanet serve
  filesystem:
    command: fs-server
";

    let symbols = document_symbols(yaml);

    let mcp = symbols
        .iter()
        .find(|s| s.name.starts_with("mcp"))
        .expect("should contain an mcp symbol");

    assert_eq!(mcp.kind, SymbolKind::Module);
    assert_eq!(mcp.children.len(), 2, "two MCP servers expected");

    let server_names: Vec<&str> = mcp.children.iter().map(|c| c.name.as_str()).collect();
    assert!(server_names.contains(&"novanet"));
    assert!(server_names.contains(&"filesystem"));

    for srv in &mcp.children {
        assert_eq!(srv.kind, SymbolKind::Module);
    }
}

// ---------------------------------------------------------------------------
// 4. Task count in name
// ---------------------------------------------------------------------------
#[test]
fn tasks_symbol_name_contains_count() {
    let yaml = "\
tasks:
  - id: a
    exec: echo a
  - id: b
    exec: echo b
  - id: c
    exec: echo c
  - id: d
    exec: echo d
";

    let symbols = document_symbols(yaml);
    let tasks = &symbols[0];

    assert!(
        tasks.name.contains("4"),
        "tasks symbol name should include the count: got `{}`",
        tasks.name
    );
    assert_eq!(tasks.children.len(), 4);
}

// ---------------------------------------------------------------------------
// 5. Empty file -> no symbols
// ---------------------------------------------------------------------------
#[test]
fn empty_file_produces_no_symbols() {
    let symbols = document_symbols("");
    assert!(
        symbols.is_empty(),
        "empty file should produce zero symbols, got {}",
        symbols.len()
    );
}
