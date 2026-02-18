//! DAG Integration Tests
//!
//! Tests for DAG validation including cycle detection and path validation.

use nika::ast::Workflow;
use nika::dag::FlowGraph;

// ═══════════════════════════════════════════════════════════════
// INTEGRATION TESTS: DAG Structure Validation
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_dag_diamond_no_cycle() {
    // Diamond: A → B, A → C, B → D, C → D (valid DAG)
    let yaml = r#"
schema: nika/workflow@0.1
id: diamond
tasks:
  - id: a
    infer:
      prompt: "A"
  - id: b
    infer:
      prompt: "B"
  - id: c
    infer:
      prompt: "C"
  - id: d
    infer:
      prompt: "D"
flows:
  - source: a
    target: [b, c]
  - source: [b, c]
    target: d
"#;
    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let graph = FlowGraph::from_workflow(&workflow);

    assert!(graph.detect_cycles().is_ok());
    assert_eq!(graph.get_final_tasks().len(), 1);
    assert!(graph.has_path("a", "d"));
    assert!(graph.has_path("b", "d"));
    assert!(graph.has_path("c", "d"));
}

#[test]
fn test_dag_self_loop() {
    // A → A (self-loop = cycle)
    let yaml = r#"
schema: nika/workflow@0.1
id: self_loop
tasks:
  - id: a
    infer:
      prompt: "A"
flows:
  - source: a
    target: a
"#;
    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let graph = FlowGraph::from_workflow(&workflow);

    let result = graph.detect_cycles();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("NIKA-020"));
}

#[test]
fn test_dag_disconnected_valid() {
    // A → B, C → D (two disconnected chains, no cycle)
    let yaml = r#"
schema: nika/workflow@0.1
id: disconnected
tasks:
  - id: a
    infer:
      prompt: "A"
  - id: b
    infer:
      prompt: "B"
  - id: c
    infer:
      prompt: "C"
  - id: d
    infer:
      prompt: "D"
flows:
  - source: a
    target: b
  - source: c
    target: d
"#;
    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let graph = FlowGraph::from_workflow(&workflow);

    assert!(graph.detect_cycles().is_ok());
    assert_eq!(graph.get_final_tasks().len(), 2);
    assert!(!graph.has_path("a", "c"));
    assert!(!graph.has_path("c", "a"));
}

#[test]
fn test_dag_complex_cycle() {
    // Complex cycle: A → B → C → D → B (cycle in the middle)
    let yaml = r#"
schema: nika/workflow@0.1
id: complex_cycle
tasks:
  - id: a
    infer:
      prompt: "A"
  - id: b
    infer:
      prompt: "B"
  - id: c
    infer:
      prompt: "C"
  - id: d
    infer:
      prompt: "D"
flows:
  - source: a
    target: b
  - source: b
    target: c
  - source: c
    target: d
  - source: d
    target: b
"#;
    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let graph = FlowGraph::from_workflow(&workflow);

    let result = graph.detect_cycles();
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("NIKA-020"));
    assert!(err_msg.contains("→")); // Contains cycle path
}

#[test]
fn test_dag_linear_chain() {
    // Simple linear chain: A → B → C → D → E
    let yaml = r#"
schema: nika/workflow@0.1
id: linear
tasks:
  - id: a
    infer:
      prompt: "A"
  - id: b
    infer:
      prompt: "B"
  - id: c
    infer:
      prompt: "C"
  - id: d
    infer:
      prompt: "D"
  - id: e
    infer:
      prompt: "E"
flows:
  - source: a
    target: b
  - source: b
    target: c
  - source: c
    target: d
  - source: d
    target: e
"#;
    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let graph = FlowGraph::from_workflow(&workflow);

    assert!(graph.detect_cycles().is_ok());
    assert_eq!(graph.get_final_tasks().len(), 1);
    assert!(graph.has_path("a", "e"));
    assert!(!graph.has_path("e", "a"));
}

#[test]
fn test_dag_parallel_merge() {
    // Parallel merge: A → [B, C, D] → E (fan-out, fan-in)
    let yaml = r#"
schema: nika/workflow@0.1
id: parallel_merge
tasks:
  - id: a
    infer:
      prompt: "A"
  - id: b
    infer:
      prompt: "B"
  - id: c
    infer:
      prompt: "C"
  - id: d
    infer:
      prompt: "D"
  - id: e
    infer:
      prompt: "E"
flows:
  - source: a
    target: [b, c, d]
  - source: [b, c, d]
    target: e
"#;
    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let graph = FlowGraph::from_workflow(&workflow);

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
    // No flows = each task is independent (valid DAG)
    let yaml = r#"
schema: nika/workflow@0.1
id: no_flows
tasks:
  - id: a
    infer:
      prompt: "A"
  - id: b
    infer:
      prompt: "B"
"#;
    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let graph = FlowGraph::from_workflow(&workflow);

    assert!(graph.detect_cycles().is_ok());
    assert_eq!(graph.get_final_tasks().len(), 2); // Both are final
}
