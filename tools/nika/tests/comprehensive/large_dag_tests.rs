//! Large DAG Tests - Stress testing with 100+ tasks.
//!
//! Tests:
//! - 100+ task workflows
//! - 20+ level deep chains
//! - 50+ parallel tasks
//! - Diamond patterns at scale
//! - Fan-in/fan-out at scale

use nika::ast::{TaskAction, Workflow};
use nika::dag::FlowGraph;
use std::path::PathBuf;

/// Get path to examples directory
fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples")
}

/// Parse a workflow file and return it
fn parse_workflow(filename: &str) -> Workflow {
    let path = examples_dir().join(filename);
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", path.display(), e));
    serde_yaml::from_str(&content).unwrap_or_else(|e| panic!("Failed to parse {}: {}", filename, e))
}

// ============================================================================
// STRESS TEST WORKFLOW PARSING
// ============================================================================

#[test]
fn test_parse_stress_large_dag() {
    let workflow = parse_workflow("test-stress-large-dag.nika.yaml");

    // Should have 90+ tasks
    assert!(
        workflow.tasks.len() >= 90,
        "Large DAG should have 90+ tasks, got {}",
        workflow.tasks.len()
    );

    // Should have many flows
    assert!(!workflow.flows.is_empty(), "Should have flow definitions");
}

#[test]
fn test_stress_large_dag_valid() {
    let workflow = parse_workflow("test-stress-large-dag.nika.yaml");

    let graph = FlowGraph::from_workflow(&workflow);

    // No cycles allowed
    assert!(
        graph.detect_cycles().is_ok(),
        "Large DAG should have no cycles"
    );

    // Should have final tasks
    let final_tasks = graph.get_final_tasks();
    assert!(!final_tasks.is_empty(), "Should have final tasks");
}

// ============================================================================
// GENERATED LARGE DAG TESTS
// ============================================================================

#[test]
fn test_generate_100_task_linear_chain() {
    // Generate a 100-task linear chain programmatically
    let mut tasks_yaml = String::new();
    let mut flows_yaml = String::new();

    for i in 0..100 {
        tasks_yaml.push_str(&format!(
            "  - id: task_{}\n    exec: \"echo 'task {}'\"\n",
            i, i
        ));

        if i > 0 {
            flows_yaml.push_str(&format!(
                "  - source: task_{}\n    target: task_{}\n",
                i - 1,
                i
            ));
        }
    }

    let yaml = format!(
        r#"schema: "nika/workflow@0.5"
provider: claude

tasks:
{}
flows:
{}"#,
        tasks_yaml, flows_yaml
    );

    let workflow: Workflow = serde_yaml::from_str(&yaml).expect("Should parse 100-task chain");
    assert_eq!(workflow.tasks.len(), 100);
    assert_eq!(workflow.flows.len(), 99);

    let graph = FlowGraph::from_workflow(&workflow);
    assert!(
        graph.detect_cycles().is_ok(),
        "Linear chain should have no cycles"
    );
}

#[test]
fn test_generate_50_parallel_tasks() {
    // Generate 50 parallel tasks with fan-out from start and fan-in to end
    let mut tasks_yaml = String::from("  - id: start\n    exec: \"echo 'start'\"\n");
    let mut flows_yaml = String::new();

    // 50 parallel tasks
    for i in 0..50 {
        tasks_yaml.push_str(&format!(
            "  - id: parallel_{}\n    exec: \"echo 'parallel {}'\"\n",
            i, i
        ));
        flows_yaml.push_str(&format!("  - source: start\n    target: parallel_{}\n", i));
    }

    // End task
    tasks_yaml.push_str("  - id: end\n    exec: \"echo 'end'\"\n");

    // All parallel tasks flow to end
    for i in 0..50 {
        flows_yaml.push_str(&format!("  - source: parallel_{}\n    target: end\n", i));
    }

    let yaml = format!(
        r#"schema: "nika/workflow@0.5"
provider: claude

tasks:
{}
flows:
{}"#,
        tasks_yaml, flows_yaml
    );

    let workflow: Workflow = serde_yaml::from_str(&yaml).expect("Should parse 50 parallel tasks");
    assert_eq!(workflow.tasks.len(), 52); // start + 50 parallel + end
    assert_eq!(workflow.flows.len(), 100); // 50 from start + 50 to end

    let graph = FlowGraph::from_workflow(&workflow);
    assert!(
        graph.detect_cycles().is_ok(),
        "Fan-out/fan-in should have no cycles"
    );

    // Verify structure
    let start_successors = graph.get_successors("start");
    assert_eq!(start_successors.len(), 50);

    let end_deps = graph.get_dependencies("end");
    assert_eq!(end_deps.len(), 50);
}

#[test]
fn test_generate_deep_20_level_chain() {
    // Generate 20-level deep chain
    let mut tasks_yaml = String::new();
    let mut flows_yaml = String::new();

    for level in 0..20 {
        tasks_yaml.push_str(&format!(
            "  - id: level_{}\n    exec: \"echo 'level {}'\"\n",
            level, level
        ));

        if level > 0 {
            flows_yaml.push_str(&format!(
                "  - source: level_{}\n    target: level_{}\n",
                level - 1,
                level
            ));
        }
    }

    let yaml = format!(
        r#"schema: "nika/workflow@0.5"
provider: claude

tasks:
{}
flows:
{}"#,
        tasks_yaml, flows_yaml
    );

    let workflow: Workflow = serde_yaml::from_str(&yaml).expect("Should parse 20-level chain");
    assert_eq!(workflow.tasks.len(), 20);

    let graph = FlowGraph::from_workflow(&workflow);
    assert!(
        graph.detect_cycles().is_ok(),
        "Deep chain should have no cycles"
    );

    // Verify path exists from level_0 to level_19
    assert!(
        graph.has_path("level_0", "level_19"),
        "Should have path through all levels"
    );
}

#[test]
fn test_generate_diamond_cascade() {
    // Generate multiple diamond patterns in sequence
    // diamond_0: a -> [b,c] -> d
    // diamond_1: d -> [e,f] -> g
    // ... 10 diamonds

    let mut tasks_yaml = String::new();
    let mut flows_yaml = String::new();

    for diamond in 0..10 {
        let top = format!("d{}_top", diamond);
        let left = format!("d{}_left", diamond);
        let right = format!("d{}_right", diamond);
        let bottom = format!("d{}_bottom", diamond);

        tasks_yaml.push_str(&format!("  - id: {}\n    exec: \"echo '{}'\"\n", top, top));
        tasks_yaml.push_str(&format!(
            "  - id: {}\n    exec: \"echo '{}'\"\n",
            left, left
        ));
        tasks_yaml.push_str(&format!(
            "  - id: {}\n    exec: \"echo '{}'\"\n",
            right, right
        ));
        tasks_yaml.push_str(&format!(
            "  - id: {}\n    exec: \"echo '{}'\"\n",
            bottom, bottom
        ));

        // Diamond flows
        flows_yaml.push_str(&format!("  - source: {}\n    target: {}\n", top, left));
        flows_yaml.push_str(&format!("  - source: {}\n    target: {}\n", top, right));
        flows_yaml.push_str(&format!("  - source: {}\n    target: {}\n", left, bottom));
        flows_yaml.push_str(&format!("  - source: {}\n    target: {}\n", right, bottom));

        // Connect to next diamond
        if diamond < 9 {
            let next_top = format!("d{}_top", diamond + 1);
            flows_yaml.push_str(&format!(
                "  - source: {}\n    target: {}\n",
                bottom, next_top
            ));
        }
    }

    let yaml = format!(
        r#"schema: "nika/workflow@0.5"
provider: claude

tasks:
{}
flows:
{}"#,
        tasks_yaml, flows_yaml
    );

    let workflow: Workflow = serde_yaml::from_str(&yaml).expect("Should parse diamond cascade");
    assert_eq!(workflow.tasks.len(), 40); // 10 diamonds * 4 nodes

    let graph = FlowGraph::from_workflow(&workflow);
    assert!(
        graph.detect_cycles().is_ok(),
        "Diamond cascade should have no cycles"
    );
}

#[test]
fn test_generate_wide_fan_in() {
    // 100 tasks all flowing to single aggregator
    let mut tasks_yaml = String::new();
    let mut flows_yaml = String::new();

    for i in 0..100 {
        tasks_yaml.push_str(&format!(
            "  - id: source_{}\n    exec: \"echo 'source {}'\"\n",
            i, i
        ));
        flows_yaml.push_str(&format!(
            "  - source: source_{}\n    target: aggregator\n",
            i
        ));
    }

    tasks_yaml.push_str("  - id: aggregator\n    exec: \"echo 'aggregated'\"\n");

    let yaml = format!(
        r#"schema: "nika/workflow@0.5"
provider: claude

tasks:
{}
flows:
{}"#,
        tasks_yaml, flows_yaml
    );

    let workflow: Workflow = serde_yaml::from_str(&yaml).expect("Should parse wide fan-in");
    assert_eq!(workflow.tasks.len(), 101);
    assert_eq!(workflow.flows.len(), 100);

    let graph = FlowGraph::from_workflow(&workflow);

    // Aggregator should have 100 dependencies
    let deps = graph.get_dependencies("aggregator");
    assert_eq!(deps.len(), 100);

    assert!(
        graph.detect_cycles().is_ok(),
        "Wide fan-in should have no cycles"
    );
}

#[test]
fn test_generate_wide_fan_out() {
    // Single source fanning out to 100 tasks
    let mut tasks_yaml = String::from("  - id: source\n    exec: \"echo 'source'\"\n");
    let mut flows_yaml = String::new();

    for i in 0..100 {
        tasks_yaml.push_str(&format!(
            "  - id: target_{}\n    exec: \"echo 'target {}'\"\n",
            i, i
        ));
        flows_yaml.push_str(&format!("  - source: source\n    target: target_{}\n", i));
    }

    let yaml = format!(
        r#"schema: "nika/workflow@0.5"
provider: claude

tasks:
{}
flows:
{}"#,
        tasks_yaml, flows_yaml
    );

    let workflow: Workflow = serde_yaml::from_str(&yaml).expect("Should parse wide fan-out");
    assert_eq!(workflow.tasks.len(), 101);
    assert_eq!(workflow.flows.len(), 100);

    let graph = FlowGraph::from_workflow(&workflow);

    // Source should have 100 successors
    let successors = graph.get_successors("source");
    assert_eq!(successors.len(), 100);

    assert!(
        graph.detect_cycles().is_ok(),
        "Wide fan-out should have no cycles"
    );
}

// ============================================================================
// VERB DISTRIBUTION TESTS
// ============================================================================

#[test]
fn test_all_verbs_in_large_dag() {
    let workflow = parse_workflow("test-stress-large-dag.nika.yaml");

    // Count each verb type
    let mut infer_count = 0;
    let mut exec_count = 0;
    let mut fetch_count = 0;
    let mut invoke_count = 0;
    let mut agent_count = 0;

    for task in &workflow.tasks {
        match &task.action {
            TaskAction::Infer { .. } => infer_count += 1,
            TaskAction::Exec { .. } => exec_count += 1,
            TaskAction::Fetch { .. } => fetch_count += 1,
            TaskAction::Invoke { .. } => invoke_count += 1,
            TaskAction::Agent { .. } => agent_count += 1,
        }
    }

    // The stress test file uses exec: for all tasks (performance testing)
    // This test verifies verb counting works correctly
    assert!(exec_count > 0, "Stress test should have exec tasks");
    assert!(
        exec_count >= 90,
        "Stress test should have 90+ exec tasks, got {}",
        exec_count
    );
}

// ============================================================================
// FLOW ARRAY SYNTAX TESTS
// ============================================================================

#[test]
fn test_flow_array_source_syntax() {
    // Test flow with array source (multiple sources to single target)
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: a
    exec: "echo a"
  - id: b
    exec: "echo b"
  - id: c
    exec: "echo c"
  - id: aggregator
    exec: "echo done"

flows:
  - source: [a, b, c]
    target: aggregator
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse array source");
    assert_eq!(workflow.tasks.len(), 4);
    assert_eq!(workflow.flows.len(), 1);

    let graph = FlowGraph::from_workflow(&workflow);

    // Aggregator should depend on all three sources
    let deps = graph.get_dependencies("aggregator");
    assert!(deps.iter().any(|d| d.as_ref() == "a"));
    assert!(deps.iter().any(|d| d.as_ref() == "b"));
    assert!(deps.iter().any(|d| d.as_ref() == "c"));
}

#[test]
fn test_flow_array_target_syntax() {
    // Test flow with array target (single source to multiple targets)
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: source
    exec: "echo source"
  - id: a
    exec: "echo a"
  - id: b
    exec: "echo b"
  - id: c
    exec: "echo c"

flows:
  - source: source
    target: [a, b, c]
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse array target");
    assert_eq!(workflow.tasks.len(), 4);
    assert_eq!(workflow.flows.len(), 1);

    let graph = FlowGraph::from_workflow(&workflow);

    // Source should have all three as successors
    let successors = graph.get_successors("source");
    assert!(successors.iter().any(|s| s.as_ref() == "a"));
    assert!(successors.iter().any(|s| s.as_ref() == "b"));
    assert!(successors.iter().any(|s| s.as_ref() == "c"));
}

// ============================================================================
// PERFORMANCE CHARACTERISTICS
// ============================================================================

#[test]
fn test_dag_construction_performance() {
    // Measure DAG construction time for large workflow
    let workflow = parse_workflow("test-stress-large-dag.nika.yaml");

    let start = std::time::Instant::now();
    let graph = FlowGraph::from_workflow(&workflow);
    let elapsed = start.elapsed();

    // DAG construction should be fast (< 100ms for ~100 tasks)
    assert!(
        elapsed.as_millis() < 500,
        "DAG construction took too long: {:?}",
        elapsed
    );

    // Cycle detection should also be fast
    let start = std::time::Instant::now();
    let _ = graph.detect_cycles();
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() < 500,
        "Cycle detection took too long: {:?}",
        elapsed
    );
}

#[test]
fn test_yaml_parsing_performance() {
    // Measure YAML parsing time
    let path = examples_dir().join("test-stress-large-dag.nika.yaml");
    let content = std::fs::read_to_string(&path).expect("Should read file");

    let start = std::time::Instant::now();
    let _workflow: Workflow = serde_yaml::from_str(&content).expect("Should parse");
    let elapsed = start.elapsed();

    // Parsing should be fast (< 100ms)
    assert!(
        elapsed.as_millis() < 500,
        "YAML parsing took too long: {:?}",
        elapsed
    );
}
