// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Execution order tests.
//!
//! Verifies that DAG structure supports correct execution ordering.
//! All tests use `depends_on:` syntax (schema @0.12+) instead of legacy `flows:`.

use nika::ast::parse_workflow;
use nika::dag::Dag;

// ============================================================================
// DEPENDENCY STRUCTURE TESTS
// ============================================================================

#[test]
fn test_linear_dependencies() {
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: linear
description: "Linear: A -> B -> C"

tasks:
  - id: A
    exec: "echo A"
  - id: B
    depends_on: [A]
    exec: "echo B"
  - id: C
    depends_on: [B]
    exec: "echo C"
"#;

    let workflow = parse_workflow(yaml).unwrap();
    let graph = Dag::from_workflow(&workflow).unwrap();

    assert!(graph.detect_cycles().is_ok());

    // Verify dependencies
    assert_eq!(graph.get_dependencies("A").len(), 0);
    assert_eq!(graph.get_dependencies("B").len(), 1);
    assert_eq!(graph.get_dependencies("C").len(), 1);

    // Verify paths
    assert!(graph.has_path("A", "B"));
    assert!(graph.has_path("B", "C"));
    assert!(graph.has_path("A", "C"));
}

#[test]
fn test_diamond_dependencies() {
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: diamond
description: "Diamond: A -> B,C -> D"

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

    // A is root (no dependencies)
    assert_eq!(graph.get_dependencies("A").len(), 0);

    // B and C depend on A
    assert_eq!(graph.get_dependencies("B").len(), 1);
    assert_eq!(graph.get_dependencies("C").len(), 1);

    // D depends on both B and C
    assert_eq!(graph.get_dependencies("D").len(), 2);

    // Verify paths
    assert!(graph.has_path("A", "D"));
    assert!(graph.has_path("B", "D"));
    assert!(graph.has_path("C", "D"));
}

#[test]
fn test_parallel_roots() {
    // Multiple root nodes (no dependencies)
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: parallel-roots
description: "Multiple roots -> single sink"

tasks:
  - id: root1
    exec: "echo root1"
  - id: root2
    exec: "echo root2"
  - id: root3
    exec: "echo root3"
  - id: sink
    depends_on: [root1, root2, root3]
    exec: "echo sink"
"#;

    let workflow = parse_workflow(yaml).unwrap();
    let graph = Dag::from_workflow(&workflow).unwrap();

    assert!(graph.detect_cycles().is_ok());

    // All roots have no dependencies (can run in parallel)
    assert_eq!(graph.get_dependencies("root1").len(), 0);
    assert_eq!(graph.get_dependencies("root2").len(), 0);
    assert_eq!(graph.get_dependencies("root3").len(), 0);

    // Sink depends on all roots
    assert_eq!(graph.get_dependencies("sink").len(), 3);

    // Verify final tasks
    let finals = graph.get_final_tasks();
    assert_eq!(finals.len(), 1);
}

// ============================================================================
// SUCCESSOR TESTS
// ============================================================================

#[test]
fn test_successor_tracking() {
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: successor-test
description: "Test successor tracking"

tasks:
  - id: A
    exec: "echo A"
  - id: B
    depends_on: [A]
    exec: "echo B"
  - id: C
    depends_on: [A]
    exec: "echo C"
"#;

    let workflow = parse_workflow(yaml).unwrap();
    let graph = Dag::from_workflow(&workflow).unwrap();

    assert!(graph.detect_cycles().is_ok());

    // A has two successors
    let a_successors = graph.get_successors("A");
    assert_eq!(a_successors.len(), 2);

    // B and C have no successors (final tasks)
    assert_eq!(graph.get_successors("B").len(), 0);
    assert_eq!(graph.get_successors("C").len(), 0);
}

// ============================================================================
// COMPLEX ORDERING SCENARIOS
// ============================================================================

#[test]
fn test_deep_wide_dag() {
    // Wide and deep DAG
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: deep-wide
description: "Deep and wide DAG"

tasks:
  - id: L0_A
    exec: "echo L0_A"
  - id: L0_B
    exec: "echo L0_B"
  - id: L1_A
    depends_on: [L0_A]
    exec: "echo L1_A"
  - id: L1_B
    depends_on: [L0_A, L0_B]
    exec: "echo L1_B"
  - id: L1_C
    depends_on: [L0_B]
    exec: "echo L1_C"
  - id: L2_A
    depends_on: [L1_A, L1_B]
    exec: "echo L2_A"
  - id: L2_B
    depends_on: [L1_B, L1_C]
    exec: "echo L2_B"
  - id: L3_SINK
    depends_on: [L2_A, L2_B]
    exec: "echo L3_SINK"
"#;

    let workflow = parse_workflow(yaml).unwrap();
    let graph = Dag::from_workflow(&workflow).unwrap();

    assert!(graph.detect_cycles().is_ok());
    assert_eq!(workflow.tasks.len(), 8);

    // Verify paths exist
    assert!(graph.has_path("L0_A", "L3_SINK"));
    assert!(graph.has_path("L0_B", "L3_SINK"));
    assert!(graph.has_path("L1_B", "L3_SINK"));

    // Verify final tasks
    let finals = graph.get_final_tasks();
    assert_eq!(finals.len(), 1);
}

// ============================================================================
// STRESS TESTS
// ============================================================================

#[test]
fn test_large_dag_50_tasks() {
    // Generate a large DAG with 50 tasks
    let mut yaml = String::from(
        r#"
schema: "nika/workflow@0.12"
workflow: large-dag-50
description: "50 task DAG stress test"

tasks:
"#,
    );

    // Create 50 tasks with linear dependencies
    for i in 0..50 {
        if i == 0 {
            yaml.push_str(&format!(
                "  - id: task_{}\n    exec: \"echo task_{}\"\n",
                i, i
            ));
        } else {
            yaml.push_str(&format!(
                "  - id: task_{}\n    depends_on: [task_{}]\n    exec: \"echo task_{}\"\n",
                i,
                i - 1,
                i
            ));
        }
    }

    let workflow = parse_workflow(&yaml).unwrap();
    let graph = Dag::from_workflow(&workflow).unwrap();

    assert_eq!(workflow.tasks.len(), 50);
    assert!(graph.detect_cycles().is_ok());

    // Verify chain
    assert!(graph.has_path("task_0", "task_49"));
    assert_eq!(graph.get_final_tasks().len(), 1);
}

#[test]
fn test_wide_dag_20_parallel() {
    // 20 parallel tasks feeding into one sink
    let mut yaml = String::from(
        r#"
schema: "nika/workflow@0.12"
workflow: wide-dag-20
description: "20 parallel tasks"

tasks:
"#,
    );

    // Create 20 parallel tasks (no depends_on)
    for i in 0..20 {
        yaml.push_str(&format!(
            "  - id: parallel_{}\n    exec: \"echo parallel_{}\"\n",
            i, i
        ));
    }

    // Sink task depends on all parallel tasks
    let deps: Vec<String> = (0..20).map(|i| format!("parallel_{}", i)).collect();
    yaml.push_str(&format!(
        "  - id: sink\n    depends_on: [{}]\n    exec: \"echo sink\"\n",
        deps.join(", ")
    ));

    let workflow = parse_workflow(&yaml).unwrap();
    let graph = Dag::from_workflow(&workflow).unwrap();

    assert_eq!(workflow.tasks.len(), 21);
    assert!(graph.detect_cycles().is_ok());

    // Sink depends on all parallel tasks
    assert_eq!(graph.get_dependencies("sink").len(), 20);

    // All parallel tasks are independent (can run in parallel)
    for i in 0..20 {
        assert_eq!(graph.get_dependencies(&format!("parallel_{}", i)).len(), 0);
    }
}
