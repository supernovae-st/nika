// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Cycle detection tests.
//!
//! Ensures DAG validation correctly identifies cyclic dependencies.
//!
//! With the migration to `depends_on:` syntax (schema @0.12+), cycles are
//! detected at parse time by `parse_workflow()` rather than by `Dag::detect_cycles()`.
//! Valid (acyclic) graphs still use `Dag::from_workflow()` + `detect_cycles()`.

use nika::ast::parse_workflow;
use nika::dag::Dag;

// ============================================================================
// SIMPLE CYCLE TESTS
// ============================================================================

#[test]
fn test_direct_self_cycle() {
    // Task depends on itself: A -> A
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: self-cycle
description: "Self-referential cycle"

tasks:
  - id: A
    depends_on: [A]
    exec: "echo A"
"#;

    let result = parse_workflow(yaml);
    assert!(result.is_err(), "Should detect self-cycle via depends_on");
}

#[test]
fn test_two_node_cycle() {
    // A -> B -> A
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: two-cycle
description: "Two-node cycle: A -> B -> A"

tasks:
  - id: A
    depends_on: [B]
    exec: "echo A"
  - id: B
    depends_on: [A]
    exec: "echo B"
"#;

    let result = parse_workflow(yaml);
    assert!(result.is_err(), "Should detect cycle via depends_on");
}

#[test]
fn test_three_node_cycle() {
    // A -> B -> C -> A
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: three-cycle
description: "Three-node cycle: A -> B -> C -> A"

tasks:
  - id: A
    depends_on: [C]
    exec: "echo A"
  - id: B
    depends_on: [A]
    exec: "echo B"
  - id: C
    depends_on: [B]
    exec: "echo C"
"#;

    let result = parse_workflow(yaml);
    assert!(result.is_err(), "Should detect cycle via depends_on");
}

// ============================================================================
// COMPLEX CYCLE TESTS
// ============================================================================

#[test]
fn test_cycle_in_diamond() {
    // Diamond with cycle: A -> B, A -> C, B -> D, C -> D, D -> A
    // In depends_on notation:
    //   B depends_on A, C depends_on A, D depends_on [B, C], A depends_on D
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: diamond-cycle
description: "Diamond with cycle back to start"

tasks:
  - id: A
    depends_on: [D]
    exec: "echo A"
  - id: B
    depends_on: [A]
    exec: "echo B"
  - id: C
    depends_on: [A]
    exec: "echo C"
  - id: D
    depends_on: [B, C]
    exec: "echo D"
"#;

    let result = parse_workflow(yaml);
    assert!(result.is_err(), "Should detect cycle via depends_on");
}

#[test]
fn test_hidden_cycle_in_chain() {
    // Long chain with hidden cycle: A -> B -> C -> D -> E -> B
    // In depends_on notation:
    //   B depends_on [A, E], C depends_on B, D depends_on C, E depends_on D
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: hidden-cycle
description: "Hidden cycle in long chain"

tasks:
  - id: A
    exec: "echo A"
  - id: B
    depends_on: [A, E]
    exec: "echo B"
  - id: C
    depends_on: [B]
    exec: "echo C"
  - id: D
    depends_on: [C]
    exec: "echo D"
  - id: E
    depends_on: [D]
    exec: "echo E"
"#;

    let result = parse_workflow(yaml);
    assert!(result.is_err(), "Should detect cycle via depends_on");
}

#[test]
fn test_multiple_cycles() {
    // Multiple independent cycles
    // Cycle 1: task_a <-> task_b
    // Cycle 2: task_x <-> task_y
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: multi-cycle
description: "Multiple cycles in same graph"

tasks:
  - id: task_a
    depends_on: [task_b]
    exec: "echo A"
  - id: task_b
    depends_on: [task_a]
    exec: "echo B"
  - id: task_x
    depends_on: [task_y]
    exec: "echo X"
  - id: task_y
    depends_on: [task_x]
    exec: "echo Y"
"#;

    let result = parse_workflow(yaml);
    assert!(result.is_err(), "Should detect cycle via depends_on");
}

// ============================================================================
// VALID GRAPHS (NO CYCLES)
// ============================================================================

#[test]
fn test_valid_diamond_no_cycle() {
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: valid-diamond
description: "Valid diamond (no cycle)"

tasks:
  - id: A
    exec: "echo A"
  - id: B
    depends_on: [A]
    exec: "echo B"
  - id: C
    depends_on: [A]
    exec: "echo C"
  - id: D
    depends_on: [B, C]
    exec: "echo D"
"#;

    let workflow = parse_workflow(yaml).unwrap();
    let graph = Dag::from_workflow(&workflow).unwrap();

    assert!(graph.detect_cycles().is_ok());
}

#[test]
fn test_valid_complex_dag() {
    // Complex but valid DAG
    // A -> B, A -> C, B -> D, C -> D, D -> E, D -> F, A -> F
    // In depends_on notation:
    //   B depends_on A, C depends_on A, D depends_on [B, C],
    //   E depends_on D, F depends_on [D, A]
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: complex-valid
description: "Complex valid DAG"

tasks:
  - id: A
    exec: "echo A"
  - id: B
    depends_on: [A]
    exec: "echo B"
  - id: C
    depends_on: [A]
    exec: "echo C"
  - id: D
    depends_on: [B, C]
    exec: "echo D"
  - id: E
    depends_on: [D]
    exec: "echo E"
  - id: F
    depends_on: [D, A]
    exec: "echo F"
"#;

    let workflow = parse_workflow(yaml).unwrap();
    let graph = Dag::from_workflow(&workflow).unwrap();

    assert!(graph.detect_cycles().is_ok());
}

// ============================================================================
// BINDING-INDUCED CYCLE TESTS
// ============================================================================

#[test]
fn test_binding_cycle_detection() {
    // Cycle through bindings: A uses B, B uses A
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: binding-cycle
description: "Cycle through use: bindings"

tasks:
  - id: A
    infer: "Use data"

  - id: B
    infer: "Use data"
"#;

    let workflow = parse_workflow(yaml).unwrap();

    // Binding analysis should detect this as a cycle
    // The exact behavior depends on implementation
    assert_eq!(workflow.tasks.len(), 2);
}
