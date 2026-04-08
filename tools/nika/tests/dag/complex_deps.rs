// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Complex dependency pattern tests.
//!
//! Tests various DAG patterns: diamond, fan-in, fan-out, chain, etc.

use nika::ast::parse_workflow;
use nika::dag::Dag;

// ============================================================================
// DIAMOND PATTERN TESTS
// ============================================================================

#[test]
fn test_diamond_pattern_basic() {
    // Diamond: A -> B, A -> C, B -> D, C -> D
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: diamond-basic
description: "Diamond dependency pattern"

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
schema: "nika/workflow@0.12"
workflow: double-diamond
description: "Two diamond patterns in sequence"

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
    depends_on: [D]
    exec: "echo F"
  - id: G
    depends_on: [E, F]
    exec: "echo G"
"#;

    let workflow = parse_workflow(yaml).unwrap();
    let graph = Dag::from_workflow(&workflow).unwrap();

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
schema: "nika/workflow@0.12"
workflow: fan-out
description: "Fan-out pattern: 1 -> N"

tasks:
  - id: source
    exec: "echo source"
  - id: target1
    depends_on: [source]
    exec: "echo target1"
  - id: target2
    depends_on: [source]
    exec: "echo target2"
  - id: target3
    depends_on: [source]
    exec: "echo target3"
  - id: target4
    depends_on: [source]
    exec: "echo target4"
  - id: target5
    depends_on: [source]
    exec: "echo target5"
"#;

    let workflow = parse_workflow(yaml).unwrap();
    let graph = Dag::from_workflow(&workflow).unwrap();

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
schema: "nika/workflow@0.12"
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
    depends_on: [source1, source2, source3, source4, source5]
    exec: "echo collector"
"#;

    let workflow = parse_workflow(yaml).unwrap();
    let graph = Dag::from_workflow(&workflow).unwrap();

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
schema: "nika/workflow@0.12"
workflow: deep-chain-10
description: "Linear chain: t0 -> t1 -> ... -> t9"

tasks:
  - id: t0
    exec: "echo t0"
  - id: t1
    depends_on: [t0]
    exec: "echo t1"
  - id: t2
    depends_on: [t1]
    exec: "echo t2"
  - id: t3
    depends_on: [t2]
    exec: "echo t3"
  - id: t4
    depends_on: [t3]
    exec: "echo t4"
  - id: t5
    depends_on: [t4]
    exec: "echo t5"
  - id: t6
    depends_on: [t5]
    exec: "echo t6"
  - id: t7
    depends_on: [t6]
    exec: "echo t7"
  - id: t8
    depends_on: [t7]
    exec: "echo t8"
  - id: t9
    depends_on: [t8]
    exec: "echo t9"
"#;

    let workflow = parse_workflow(yaml).unwrap();
    let graph = Dag::from_workflow(&workflow).unwrap();

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
schema: "nika/workflow@0.12"
workflow: ml-pipeline
description: "ML pipeline pattern"

tasks:
  - id: fetch_data
    fetch:
      url: "https://data.example.com/dataset"
      method: GET

  - id: preprocess
    depends_on: [fetch_data]
    exec: "python preprocess.py"

  - id: train_model
    depends_on: [preprocess]
    exec: "python train.py"

  - id: validate_model
    depends_on: [preprocess]
    exec: "python validate.py"

  - id: evaluate
    depends_on: [train_model, validate_model]
    infer: "Analyze model metrics"

  - id: deploy
    depends_on: [evaluate]
    exec: "python deploy.py"
"#;

    let workflow = parse_workflow(yaml).unwrap();
    let graph = Dag::from_workflow(&workflow).unwrap();

    assert_eq!(workflow.tasks.len(), 6);
    assert!(graph.detect_cycles().is_ok());
}

#[test]
fn test_microservices_pattern() {
    // Multiple independent services with final aggregation
    let yaml = r#"
schema: "nika/workflow@0.12"
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
    depends_on: [user_service, order_service, payment_service, notification_service]
    infer: "Aggregate user data"
"#;

    let workflow = parse_workflow(yaml).unwrap();
    let graph = Dag::from_workflow(&workflow).unwrap();

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
schema: "nika/workflow@0.12"
workflow: single-task
description: "Single task, no dependencies"

tasks:
  - id: only_task
    exec: "echo hello"
"#;

    let workflow = parse_workflow(yaml).unwrap();
    let graph = Dag::from_workflow(&workflow).unwrap();

    assert_eq!(workflow.tasks.len(), 1);
    assert!(graph.detect_cycles().is_ok());
}

#[test]
fn test_parallel_independent_tasks() {
    // All tasks can run in parallel
    let yaml = r#"
schema: "nika/workflow@0.12"
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

    let workflow = parse_workflow(yaml).unwrap();
    let graph = Dag::from_workflow(&workflow).unwrap();

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
schema: "nika/workflow@0.12"
workflow: self-contained
description: "Bindings without explicit flows"

tasks:
  - id: producer
    exec: "echo data"

  - id: consumer
    infer: "Process data"
"#;

    let workflow = parse_workflow(yaml).unwrap();

    // Should parse successfully
    assert_eq!(workflow.tasks.len(), 2);
}
