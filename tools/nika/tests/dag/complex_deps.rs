//! Complex dependency pattern tests.
//!
//! Tests various DAG patterns: diamond, fan-in, fan-out, chain, etc.

use nika::ast::Workflow;
use nika::dag::FlowGraph;

// ============================================================================
// DIAMOND PATTERN TESTS
// ============================================================================

#[test]
fn test_diamond_pattern_basic() {
    // Diamond: A -> B, A -> C, B -> D, C -> D
    let yaml = r#"
schema: "nika/workflow@0.5"
workflow: diamond-basic
description: "Diamond dependency pattern"

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
    let graph = FlowGraph::from_workflow(&workflow);

    // Verify structure
    assert_eq!(workflow.tasks.len(), 4);
    assert!(graph.detect_cycles().is_ok());

    // A should have no dependencies
    assert_eq!(graph.get_dependencies("A").len(), 0);

    // D depends on both B and C
    let d_deps = graph.get_dependencies("D");
    assert_eq!(d_deps.len(), 2);
}

#[test]
fn test_double_diamond() {
    // Two diamonds in sequence
    let yaml = r#"
schema: "nika/workflow@0.5"
workflow: double-diamond
description: "Two diamond patterns in sequence"

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
  - id: G
    exec: "echo G"

flows:
  # First diamond: A -> B,C -> D
  - source: A
    target: B
  - source: A
    target: C
  - source: B
    target: D
  - source: C
    target: D
  # Second diamond: D -> E,F -> G
  - source: D
    target: E
  - source: D
    target: F
  - source: E
    target: G
  - source: F
    target: G
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let graph = FlowGraph::from_workflow(&workflow);

    assert_eq!(workflow.tasks.len(), 7);
    assert!(graph.detect_cycles().is_ok());
}

// ============================================================================
// FAN-OUT / FAN-IN PATTERNS
// ============================================================================

#[test]
fn test_fan_out_pattern() {
    // One task fans out to many
    let yaml = r#"
schema: "nika/workflow@0.5"
workflow: fan-out
description: "Fan-out pattern: 1 -> N"

tasks:
  - id: source
    exec: "echo source"
  - id: target1
    exec: "echo target1"
  - id: target2
    exec: "echo target2"
  - id: target3
    exec: "echo target3"
  - id: target4
    exec: "echo target4"
  - id: target5
    exec: "echo target5"

flows:
  - source: source
    target: target1
  - source: source
    target: target2
  - source: source
    target: target3
  - source: source
    target: target4
  - source: source
    target: target5
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let graph = FlowGraph::from_workflow(&workflow);

    assert_eq!(workflow.tasks.len(), 6);
    assert!(graph.detect_cycles().is_ok());

    // Each target should depend on source
    for i in 1..=5 {
        let deps = graph.get_dependencies(&format!("target{}", i));
        assert_eq!(deps.len(), 1);
    }
}

#[test]
fn test_fan_in_pattern() {
    // Many tasks fan in to one
    let yaml = r#"
schema: "nika/workflow@0.5"
workflow: fan-in
description: "Fan-in pattern: N -> 1"

tasks:
  - id: source1
    exec: "echo source1"
  - id: source2
    exec: "echo source2"
  - id: source3
    exec: "echo source3"
  - id: source4
    exec: "echo source4"
  - id: source5
    exec: "echo source5"
  - id: collector
    exec: "echo collector"

flows:
  - source: source1
    target: collector
  - source: source2
    target: collector
  - source: source3
    target: collector
  - source: source4
    target: collector
  - source: source5
    target: collector
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let graph = FlowGraph::from_workflow(&workflow);

    assert_eq!(workflow.tasks.len(), 6);
    assert!(graph.detect_cycles().is_ok());

    // Collector should depend on all sources
    let deps = graph.get_dependencies("collector");
    assert_eq!(deps.len(), 5);
}

// ============================================================================
// DEEP CHAIN PATTERNS
// ============================================================================

#[test]
fn test_deep_chain_10_levels() {
    // Linear chain of 10 tasks
    let yaml = r#"
schema: "nika/workflow@0.5"
workflow: deep-chain-10
description: "Linear chain: t0 -> t1 -> ... -> t9"

tasks:
  - id: t0
    exec: "echo t0"
  - id: t1
    exec: "echo t1"
  - id: t2
    exec: "echo t2"
  - id: t3
    exec: "echo t3"
  - id: t4
    exec: "echo t4"
  - id: t5
    exec: "echo t5"
  - id: t6
    exec: "echo t6"
  - id: t7
    exec: "echo t7"
  - id: t8
    exec: "echo t8"
  - id: t9
    exec: "echo t9"

flows:
  - source: t0
    target: t1
  - source: t1
    target: t2
  - source: t2
    target: t3
  - source: t3
    target: t4
  - source: t4
    target: t5
  - source: t5
    target: t6
  - source: t6
    target: t7
  - source: t7
    target: t8
  - source: t8
    target: t9
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let graph = FlowGraph::from_workflow(&workflow);

    assert_eq!(workflow.tasks.len(), 10);
    assert!(graph.detect_cycles().is_ok());

    // Verify chain dependencies
    for i in 1..10 {
        let deps = graph.get_dependencies(&format!("t{}", i));
        assert_eq!(deps.len(), 1, "t{} should have 1 dependency", i);
    }
}

// ============================================================================
// COMPLEX REAL-WORLD PATTERNS
// ============================================================================

#[test]
fn test_ml_pipeline_pattern() {
    // Typical ML pipeline: data -> preprocess -> train/validate -> evaluate -> deploy
    let yaml = r#"
schema: "nika/workflow@0.5"
workflow: ml-pipeline
description: "ML pipeline pattern"

tasks:
  - id: fetch_data
    fetch:
      url: "https://data.example.com/dataset"
      method: GET

  - id: preprocess
    exec: "python preprocess.py"

  - id: train_model
    exec: "python train.py"

  - id: validate_model
    exec: "python validate.py"

  - id: evaluate
    infer: "Analyze model metrics"

  - id: deploy
    exec: "python deploy.py"

flows:
  - source: fetch_data
    target: preprocess
  - source: preprocess
    target: train_model
  - source: preprocess
    target: validate_model
  - source: train_model
    target: evaluate
  - source: validate_model
    target: evaluate
  - source: evaluate
    target: deploy
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let graph = FlowGraph::from_workflow(&workflow);

    assert_eq!(workflow.tasks.len(), 6);
    assert!(graph.detect_cycles().is_ok());
}

#[test]
fn test_microservices_pattern() {
    // Multiple independent services with final aggregation
    let yaml = r#"
schema: "nika/workflow@0.5"
workflow: microservices
description: "Microservices aggregation pattern"

tasks:
  - id: user_service
    fetch:
      url: "https://api/users/1"
      method: GET

  - id: order_service
    fetch:
      url: "https://api/orders/user/1"
      method: GET

  - id: payment_service
    fetch:
      url: "https://api/payments/user/1"
      method: GET

  - id: notification_service
    fetch:
      url: "https://api/notifications/user/1"
      method: GET

  - id: aggregate
    infer: "Aggregate user data"

flows:
  - source: user_service
    target: aggregate
  - source: order_service
    target: aggregate
  - source: payment_service
    target: aggregate
  - source: notification_service
    target: aggregate
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let graph = FlowGraph::from_workflow(&workflow);

    assert_eq!(workflow.tasks.len(), 5);
    assert!(graph.detect_cycles().is_ok());

    // First 4 tasks should have no dependencies (parallel)
    assert_eq!(graph.get_dependencies("user_service").len(), 0);
    assert_eq!(graph.get_dependencies("order_service").len(), 0);
    assert_eq!(graph.get_dependencies("payment_service").len(), 0);
    assert_eq!(graph.get_dependencies("notification_service").len(), 0);

    // Aggregate depends on all 4
    assert_eq!(graph.get_dependencies("aggregate").len(), 4);
}

// ============================================================================
// EDGE CASES
// ============================================================================

#[test]
fn test_single_task_no_deps() {
    let yaml = r#"
schema: "nika/workflow@0.5"
workflow: single-task
description: "Single task, no dependencies"

tasks:
  - id: only_task
    exec: "echo hello"
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let graph = FlowGraph::from_workflow(&workflow);

    assert_eq!(workflow.tasks.len(), 1);
    assert!(graph.detect_cycles().is_ok());
}

#[test]
fn test_parallel_independent_tasks() {
    // All tasks can run in parallel
    let yaml = r#"
schema: "nika/workflow@0.5"
workflow: all-parallel
description: "All tasks independent"

tasks:
  - id: task1
    exec: "echo 1"
  - id: task2
    exec: "echo 2"
  - id: task3
    exec: "echo 3"
  - id: task4
    exec: "echo 4"
  - id: task5
    exec: "echo 5"
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let graph = FlowGraph::from_workflow(&workflow);

    assert_eq!(workflow.tasks.len(), 5);
    assert!(graph.detect_cycles().is_ok());

    // All tasks should have no dependencies
    for i in 1..=5 {
        assert_eq!(graph.get_dependencies(&format!("task{}", i)).len(), 0);
    }
}

#[test]
fn test_self_contained_bindings() {
    // Task uses binding but no explicit flow
    let yaml = r#"
schema: "nika/workflow@0.5"
workflow: self-contained
description: "Bindings without explicit flows"

tasks:
  - id: producer
    exec: "echo data"

  - id: consumer
    infer: "Process data"
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();

    // Should parse successfully
    assert_eq!(workflow.tasks.len(), 2);
}
