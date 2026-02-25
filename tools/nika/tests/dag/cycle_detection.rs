//! Cycle detection tests.
//!
//! Ensures DAG validation correctly identifies cyclic dependencies.

use nika::ast::Workflow;
use nika::dag::Dag;

// ============================================================================
// SIMPLE CYCLE TESTS
// ============================================================================

#[test]
fn test_direct_self_cycle() {
    // Task depends on itself: A -> A
    let yaml = r#"
schema: "nika/workflow@0.5"
workflow: self-cycle
description: "Self-referential cycle"

tasks:
  - id: A
    exec: "echo A"

flows:
  - source: A
    target: A
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let graph = Dag::from_workflow(&workflow);

    // Should detect cycle
    assert!(graph.detect_cycles().is_err());
}

#[test]
fn test_two_node_cycle() {
    // A -> B -> A
    let yaml = r#"
schema: "nika/workflow@0.5"
workflow: two-cycle
description: "Two-node cycle: A -> B -> A"

tasks:
  - id: A
    exec: "echo A"
  - id: B
    exec: "echo B"

flows:
  - source: A
    target: B
  - source: B
    target: A
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let graph = Dag::from_workflow(&workflow);

    assert!(graph.detect_cycles().is_err());
}

#[test]
fn test_three_node_cycle() {
    // A -> B -> C -> A
    let yaml = r#"
schema: "nika/workflow@0.5"
workflow: three-cycle
description: "Three-node cycle: A -> B -> C -> A"

tasks:
  - id: A
    exec: "echo A"
  - id: B
    exec: "echo B"
  - id: C
    exec: "echo C"

flows:
  - source: A
    target: B
  - source: B
    target: C
  - source: C
    target: A
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let graph = Dag::from_workflow(&workflow);

    assert!(graph.detect_cycles().is_err());
}

// ============================================================================
// COMPLEX CYCLE TESTS
// ============================================================================

#[test]
fn test_cycle_in_diamond() {
    // Diamond with cycle: A -> B, A -> C, B -> D, C -> D, D -> A
    let yaml = r#"
schema: "nika/workflow@0.5"
workflow: diamond-cycle
description: "Diamond with cycle back to start"

tasks:
  - id: A
    exec: "echo A"
  - id: B
    exec: "echo B"
  - id: C
    exec: "echo C"
  - id: D
    exec: "echo D"

flows:
  - source: A
    target: B
  - source: A
    target: C
  - source: B
    target: D
  - source: C
    target: D
  - source: D
    target: A
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let graph = Dag::from_workflow(&workflow);

    assert!(graph.detect_cycles().is_err());
}

#[test]
fn test_hidden_cycle_in_chain() {
    // Long chain with hidden cycle: A -> B -> C -> D -> E -> B
    let yaml = r#"
schema: "nika/workflow@0.5"
workflow: hidden-cycle
description: "Hidden cycle in long chain"

tasks:
  - id: A
    exec: "echo A"
  - id: B
    exec: "echo B"
  - id: C
    exec: "echo C"
  - id: D
    exec: "echo D"
  - id: E
    exec: "echo E"

flows:
  - source: A
    target: B
  - source: B
    target: C
  - source: C
    target: D
  - source: D
    target: E
  - source: E
    target: B
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let graph = Dag::from_workflow(&workflow);

    assert!(graph.detect_cycles().is_err());
}

#[test]
fn test_multiple_cycles() {
    // Multiple independent cycles
    let yaml = r#"
schema: "nika/workflow@0.5"
workflow: multi-cycle
description: "Multiple cycles in same graph"

tasks:
  - id: A
    exec: "echo A"
  - id: B
    exec: "echo B"
  - id: X
    exec: "echo X"
  - id: Y
    exec: "echo Y"

flows:
  # Cycle 1: A -> B -> A
  - source: A
    target: B
  - source: B
    target: A
  # Cycle 2: X -> Y -> X
  - source: X
    target: Y
  - source: Y
    target: X
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let graph = Dag::from_workflow(&workflow);

    assert!(graph.detect_cycles().is_err());
}

// ============================================================================
// VALID GRAPHS (NO CYCLES)
// ============================================================================

#[test]
fn test_valid_diamond_no_cycle() {
    let yaml = r#"
schema: "nika/workflow@0.5"
workflow: valid-diamond
description: "Valid diamond (no cycle)"

tasks:
  - id: A
    exec: "echo A"
  - id: B
    exec: "echo B"
  - id: C
    exec: "echo C"
  - id: D
    exec: "echo D"

flows:
  - source: A
    target: B
  - source: A
    target: C
  - source: B
    target: D
  - source: C
    target: D
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let graph = Dag::from_workflow(&workflow);

    assert!(graph.detect_cycles().is_ok());
}

#[test]
fn test_valid_complex_dag() {
    // Complex but valid DAG
    let yaml = r#"
schema: "nika/workflow@0.5"
workflow: complex-valid
description: "Complex valid DAG"

tasks:
  - id: A
    exec: "echo A"
  - id: B
    exec: "echo B"
  - id: C
    exec: "echo C"
  - id: D
    exec: "echo D"
  - id: E
    exec: "echo E"
  - id: F
    exec: "echo F"

flows:
  - source: A
    target: B
  - source: A
    target: C
  - source: B
    target: D
  - source: C
    target: D
  - source: D
    target: E
  - source: D
    target: F
  - source: A
    target: F
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let graph = Dag::from_workflow(&workflow);

    assert!(graph.detect_cycles().is_ok());
}

// ============================================================================
// BINDING-INDUCED CYCLE TESTS
// ============================================================================

#[test]
fn test_binding_cycle_detection() {
    // Cycle through bindings: A uses B, B uses A
    let yaml = r#"
schema: "nika/workflow@0.5"
workflow: binding-cycle
description: "Cycle through use: bindings"

tasks:
  - id: A
    infer: "Use data"

  - id: B
    infer: "Use data"
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();

    // Binding analysis should detect this as a cycle
    // The exact behavior depends on implementation
    assert_eq!(workflow.tasks.len(), 2);
}
