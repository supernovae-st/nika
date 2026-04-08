// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! DAG Integration Tests
//!
//! Tests for DAG validation including cycle detection and path validation.
//!
//! Note: parse_workflow() does NOT parse `flows:` YAML sections.
//! All dependency edges must be expressed via `depends_on:` (requires @0.12+).

use nika::ast::parse_workflow;
use nika::dag::Dag;

// ═══════════════════════════════════════════════════════════════
// INTEGRATION TESTS: DAG Structure Validation
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_dag_diamond_no_cycle() {
    // Diamond: A → B, A → C, B → D, C → D (valid DAG)
    let yaml = r#"
schema: nika/workflow@0.12
id: diamond
tasks:
  - id: a
    infer:
      prompt: "A"
  - id: b
    depends_on: [a]
    infer:
      prompt: "B"
  - id: c
    depends_on: [a]
    infer:
      prompt: "C"
  - id: d
    depends_on: [b, c]
    infer:
      prompt: "D"
"#;
    let workflow = parse_workflow(yaml).unwrap();
    let graph = Dag::from_workflow(&workflow).unwrap();

    assert!(graph.detect_cycles().is_ok());
    assert_eq!(graph.get_final_tasks().len(), 1);
    assert!(graph.has_path("a", "d"));
    assert!(graph.has_path("b", "d"));
    assert!(graph.has_path("c", "d"));
}

#[test]
fn test_dag_self_loop() {
    // A → A (self-loop = cycle)
    // parse_workflow() detects self-referential cycles via depends_on
    let yaml = r#"
schema: nika/workflow@0.12
id: self_loop
tasks:
  - id: a
    depends_on: [a]
    infer:
      prompt: "A"
"#;
    // parse_workflow() catches cycles at analysis time
    let result = parse_workflow(yaml);
    assert!(result.is_err(), "Should detect self-referential cycle");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("cycle") || err_msg.contains("NIKA-"),
        "Error should mention cycle: {err_msg}"
    );
}

#[test]
fn test_dag_disconnected_valid() {
    // A → B, C → D (two disconnected chains, no cycle)
    let yaml = r#"
schema: nika/workflow@0.12
id: disconnected
tasks:
  - id: a
    infer:
      prompt: "A"
  - id: b
    depends_on: [a]
    infer:
      prompt: "B"
  - id: c
    infer:
      prompt: "C"
  - id: d
    depends_on: [c]
    infer:
      prompt: "D"
"#;
    let workflow = parse_workflow(yaml).unwrap();
    let graph = Dag::from_workflow(&workflow).unwrap();

    assert!(graph.detect_cycles().is_ok());
    assert_eq!(graph.get_final_tasks().len(), 2);
    assert!(!graph.has_path("a", "c"));
    assert!(!graph.has_path("c", "a"));
}

#[test]
fn test_dag_complex_cycle() {
    // Complex cycle: A → B → C → D → B (cycle in the middle)
    // parse_workflow() detects this via depends_on analysis
    let yaml = r#"
schema: nika/workflow@0.12
id: complex_cycle
tasks:
  - id: a
    infer:
      prompt: "A"
  - id: b
    depends_on: [a, d]
    infer:
      prompt: "B"
  - id: c
    depends_on: [b]
    infer:
      prompt: "C"
  - id: d
    depends_on: [c]
    infer:
      prompt: "D"
"#;
    // parse_workflow() catches cycles at analysis time
    let result = parse_workflow(yaml);
    assert!(result.is_err(), "Should detect cycle: B → C → D → B");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("cycle") || err_msg.contains("NIKA-"),
        "Error should mention cycle: {err_msg}"
    );
}

#[test]
fn test_dag_linear_chain() {
    // Simple linear chain: A → B → C → D → E
    let yaml = r#"
schema: nika/workflow@0.12
id: linear
tasks:
  - id: a
    infer:
      prompt: "A"
  - id: b
    depends_on: [a]
    infer:
      prompt: "B"
  - id: c
    depends_on: [b]
    infer:
      prompt: "C"
  - id: d
    depends_on: [c]
    infer:
      prompt: "D"
  - id: e
    depends_on: [d]
    infer:
      prompt: "E"
"#;
    let workflow = parse_workflow(yaml).unwrap();
    let graph = Dag::from_workflow(&workflow).unwrap();

    assert!(graph.detect_cycles().is_ok());
    assert_eq!(graph.get_final_tasks().len(), 1);
    assert!(graph.has_path("a", "e"));
    assert!(!graph.has_path("e", "a"));
}

#[test]
fn test_dag_parallel_merge() {
    // Parallel merge: A → [B, C, D] → E (fan-out, fan-in)
    let yaml = r#"
schema: nika/workflow@0.12
id: parallel_merge
tasks:
  - id: a
    infer:
      prompt: "A"
  - id: b
    depends_on: [a]
    infer:
      prompt: "B"
  - id: c
    depends_on: [a]
    infer:
      prompt: "C"
  - id: d
    depends_on: [a]
    infer:
      prompt: "D"
  - id: e
    depends_on: [b, c, d]
    infer:
      prompt: "E"
"#;
    let workflow = parse_workflow(yaml).unwrap();
    let graph = Dag::from_workflow(&workflow).unwrap();

    assert!(graph.detect_cycles().is_ok());
    assert_eq!(graph.get_final_tasks().len(), 1);

    // All paths from a to e
    assert!(graph.has_path("a", "e"));
    assert!(graph.has_path("b", "e"));
    assert!(graph.has_path("c", "e"));
    assert!(graph.has_path("d", "e"));
}

#[test]
fn test_dag_no_flows_valid() {
    // No dependencies = each task is independent (valid DAG)
    let yaml = r#"
schema: nika/workflow@0.12
id: no_flows
tasks:
  - id: a
    infer:
      prompt: "A"
  - id: b
    infer:
      prompt: "B"
"#;
    let workflow = parse_workflow(yaml).unwrap();
    let graph = Dag::from_workflow(&workflow).unwrap();

    assert!(graph.detect_cycles().is_ok());
    assert_eq!(graph.get_final_tasks().len(), 2); // Both are final
}
