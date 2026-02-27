//! Execution order tests.
//!
//! Verifies that DAG structure supports correct execution ordering.

use nika::ast::Workflow;
use nika::dag::Dag;
use nika::serde_yaml;

// ============================================================================
// DEPENDENCY STRUCTURE TESTS
// ============================================================================

#[test]
fn test_linear_dependencies() {
    let yaml = r#"
schema: "nika/workflow@0.5"
workflow: linear
description: "Linear: A -> B -> C"

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
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let graph = Dag::from_workflow(&workflow);

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
schema: "nika/workflow@0.5"
workflow: diamond
description: "Diamond: A -> B,C -> D"

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
schema: "nika/workflow@0.5"
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
    exec: "echo sink"

flows:
  - source: root1
    target: sink
  - source: root2
    target: sink
  - source: root3
    target: sink
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let graph = Dag::from_workflow(&workflow);

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
schema: "nika/workflow@0.5"
workflow: successor-test
description: "Test successor tracking"

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
  - source: A
    target: C
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let graph = Dag::from_workflow(&workflow);

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
schema: "nika/workflow@0.5"
workflow: deep-wide
description: "Deep and wide DAG"

tasks:
  - id: L0_A
    exec: "echo L0_A"
  - id: L0_B
    exec: "echo L0_B"
  - id: L1_A
    exec: "echo L1_A"
  - id: L1_B
    exec: "echo L1_B"
  - id: L1_C
    exec: "echo L1_C"
  - id: L2_A
    exec: "echo L2_A"
  - id: L2_B
    exec: "echo L2_B"
  - id: L3_SINK
    exec: "echo L3_SINK"

flows:
  # Level 0 -> Level 1
  - source: L0_A
    target: L1_A
  - source: L0_A
    target: L1_B
  - source: L0_B
    target: L1_B
  - source: L0_B
    target: L1_C
  # Level 1 -> Level 2
  - source: L1_A
    target: L2_A
  - source: L1_B
    target: L2_A
  - source: L1_B
    target: L2_B
  - source: L1_C
    target: L2_B
  # Level 2 -> Level 3
  - source: L2_A
    target: L3_SINK
  - source: L2_B
    target: L3_SINK
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let graph = Dag::from_workflow(&workflow);

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
schema: "nika/workflow@0.5"
workflow: large-dag-50
description: "50 task DAG stress test"

tasks:
"#,
    );

    // Create 50 tasks
    for i in 0..50 {
        yaml.push_str(&format!(
            "  - id: task_{}\n    exec: \"echo task_{}\"\n",
            i, i
        ));
    }

    yaml.push_str("\nflows:\n");

    // Create linear chain
    for i in 1..50 {
        yaml.push_str(&format!(
            "  - source: task_{}\n    target: task_{}\n",
            i - 1,
            i
        ));
    }

    let workflow: Workflow = serde_yaml::from_str(&yaml).unwrap();
    let graph = Dag::from_workflow(&workflow);

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
schema: "nika/workflow@0.5"
workflow: wide-dag-20
description: "20 parallel tasks"

tasks:
"#,
    );

    // Create 20 parallel tasks
    for i in 0..20 {
        yaml.push_str(&format!(
            "  - id: parallel_{}\n    exec: \"echo parallel_{}\"\n",
            i, i
        ));
    }
    yaml.push_str("  - id: sink\n    exec: \"echo sink\"\n");

    yaml.push_str("\nflows:\n");
    for i in 0..20 {
        yaml.push_str(&format!("  - source: parallel_{}\n    target: sink\n", i));
    }

    let workflow: Workflow = serde_yaml::from_str(&yaml).unwrap();
    let graph = Dag::from_workflow(&workflow);

    assert_eq!(workflow.tasks.len(), 21);
    assert!(graph.detect_cycles().is_ok());

    // Sink depends on all parallel tasks
    assert_eq!(graph.get_dependencies("sink").len(), 20);

    // All parallel tasks are independent (can run in parallel)
    for i in 0..20 {
        assert_eq!(graph.get_dependencies(&format!("parallel_{}", i)).len(), 0);
    }
}
