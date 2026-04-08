// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Micro-benchmarks using std time measurements.
//!
//! These are quick inline benchmarks for development.
//! For detailed benchmarks, use the Criterion benchmarks in benches/

use nika::ast::parse_workflow;
use nika::dag::Dag;
use std::time::Instant;

// ============================================================================
// PARSING BENCHMARKS
// ============================================================================

#[test]
fn bench_parse_simple_workflow() {
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: bench-simple
description: "Simple benchmark workflow"

tasks:
  - id: task1
    exec: "echo hello"
"#;

    let iterations = 1000;
    let start = Instant::now();

    for _ in 0..iterations {
        let _ = parse_workflow(yaml).unwrap();
    }

    let elapsed = start.elapsed();
    let per_iter = elapsed / iterations;

    println!(
        "Parse simple workflow: {:?} total, {:?} per iteration",
        elapsed, per_iter
    );

    // Should parse in under 2000µs per iteration (relaxed for debug builds + CI)
    assert!(
        per_iter.as_micros() < 2000,
        "Parsing too slow: {:?}",
        per_iter
    );
}

#[test]
fn bench_parse_complex_workflow() {
    // Generate a complex workflow with 50 tasks
    let mut yaml = String::from(
        r#"
schema: "nika/workflow@0.12"
workflow: bench-complex
description: "Complex benchmark workflow"
provider: claude

tasks:
"#,
    );

    for i in 0..50 {
        yaml.push_str(&format!("  - id: task_{}\n", i));
        if i > 0 {
            yaml.push_str(&format!("    depends_on: [task_{}]\n", i - 1));
        }
        yaml.push_str(&format!("    exec: \"echo task_{}\"\n", i));
    }

    let iterations = 100;
    let start = Instant::now();

    for _ in 0..iterations {
        let _ = parse_workflow(&yaml).unwrap();
    }

    let elapsed = start.elapsed();
    let per_iter = elapsed / iterations;

    println!(
        "Parse complex workflow (50 tasks): {:?} total, {:?} per iteration",
        elapsed, per_iter
    );

    // Should parse in under 50ms per iteration (relaxed for debug builds)
    assert!(
        per_iter.as_millis() < 50,
        "Parsing too slow: {:?}",
        per_iter
    );
}

// ============================================================================
// DAG BENCHMARKS
// ============================================================================

#[test]
fn bench_dag_construction_small() {
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: bench-dag-small
description: "Small DAG"

tasks:
  - id: A
    exec: "echo A"
  - id: B
    exec: "echo B"
  - id: C
    exec: "echo C"

"#;

    let workflow = parse_workflow(yaml).unwrap();

    let iterations = 10000;
    let start = Instant::now();

    for _ in 0..iterations {
        let graph = Dag::from_workflow(&workflow).unwrap();
        let _ = graph.detect_cycles();
    }

    let elapsed = start.elapsed();
    let per_iter = elapsed / iterations;

    println!(
        "DAG construction (3 nodes): {:?} total, {:?} per iteration",
        elapsed, per_iter
    );

    // Should construct in under 500µs (relaxed for debug builds)
    assert!(
        per_iter.as_micros() < 500,
        "DAG construction too slow: {:?}",
        per_iter
    );
}

#[test]
fn bench_dag_construction_large() {
    // Generate a large DAG with 100 tasks
    let mut yaml = String::from(
        r#"
schema: "nika/workflow@0.12"
workflow: bench-dag-large
description: "Large DAG"

tasks:
"#,
    );

    for i in 0..100 {
        yaml.push_str(&format!("  - id: task_{}\n", i));
        if i > 0 {
            yaml.push_str(&format!("    depends_on: [task_{}]\n", i - 1));
        }
        yaml.push_str(&format!("    exec: \"echo task_{}\"\n", i));
    }

    let workflow = parse_workflow(&yaml).unwrap();

    let iterations = 1000;
    let start = Instant::now();

    for _ in 0..iterations {
        let graph = Dag::from_workflow(&workflow).unwrap();
        let _ = graph.detect_cycles();
    }

    let elapsed = start.elapsed();
    let per_iter = elapsed / iterations;

    println!(
        "DAG construction (100 nodes): {:?} total, {:?} per iteration",
        elapsed, per_iter
    );

    // Should construct in under 10ms (relaxed for debug builds)
    assert!(
        per_iter.as_millis() < 10,
        "DAG construction too slow: {:?}",
        per_iter
    );
}

#[test]
fn bench_cycle_detection() {
    // Generate a DAG with 50 tasks
    let mut yaml = String::from(
        r#"
schema: "nika/workflow@0.12"
workflow: bench-cycle
description: "Cycle detection benchmark"

tasks:
"#,
    );

    for i in 0..50 {
        yaml.push_str(&format!("  - id: task_{}\n", i));
        if i > 0 {
            let mut deps = vec![format!("task_{}", i - 1)];
            if i > 2 {
                deps.push(format!("task_{}", i - 2));
            }
            yaml.push_str(&format!("    depends_on: [{}]\n", deps.join(", ")));
        }
        yaml.push_str(&format!("    exec: \"echo task_{}\"\n", i));
    }

    let workflow = parse_workflow(&yaml).unwrap();
    let graph = Dag::from_workflow(&workflow).unwrap();

    let iterations = 10000;
    let start = Instant::now();

    for _ in 0..iterations {
        let _ = graph.detect_cycles();
    }

    let elapsed = start.elapsed();
    let per_iter = elapsed / iterations;

    println!(
        "Cycle detection (50 nodes): {:?} total, {:?} per iteration",
        elapsed, per_iter
    );

    // Should detect in under 2000µs (relaxed for debug builds + CI)
    assert!(
        per_iter.as_micros() < 2000,
        "Cycle detection too slow: {:?}",
        per_iter
    );
}

// ============================================================================
// MEMORY BENCHMARKS
// ============================================================================

#[test]
fn bench_workflow_memory_size() {
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: bench-memory
description: "Memory size benchmark"

tasks:
  - id: task1
    exec: "echo hello"
  - id: task2
    infer: "Generate text"
  - id: task3
    fetch:
      url: "https://example.com"
      method: GET
"#;

    let workflow = parse_workflow(yaml).unwrap();

    // Approximate memory size
    let size = std::mem::size_of_val(&workflow);

    println!("Workflow struct size: {} bytes", size);

    // Should be reasonable size (under 1KB for small workflow)
    assert!(size < 1024, "Workflow too large: {} bytes", size);
}

// ============================================================================
// COMPARISON BENCHMARKS
// ============================================================================

#[test]
fn bench_comparison_report() {
    println!("\n=== PERFORMANCE BENCHMARK SUMMARY ===\n");

    // Run all benchmarks and collect results
    let benchmarks: Vec<(&str, u32, fn())> = vec![
        ("Parse simple workflow", 1000, benchmark_parse_simple),
        (
            "Parse complex workflow (50 tasks)",
            100,
            benchmark_parse_complex,
        ),
        ("DAG construction (3 nodes)", 10000, benchmark_dag_small),
        ("DAG construction (100 nodes)", 1000, benchmark_dag_large),
        ("Cycle detection (50 nodes)", 10000, benchmark_cycle_detect),
    ];

    for (name, iters, bench_fn) in benchmarks {
        let start = Instant::now();
        for _ in 0..iters {
            bench_fn();
        }
        let elapsed = start.elapsed();
        let per_iter = elapsed / iters;
        println!("{}: {:?} ({} iterations)", name, per_iter, iters);
    }

    println!("\n=====================================\n");
}

fn benchmark_parse_simple() {
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: bench
tasks:
  - id: t1
    exec: "echo"
"#;
    let _ = parse_workflow(yaml).unwrap();
}

fn benchmark_parse_complex() {
    let mut yaml = String::from("schema: \"nika/workflow@0.12\"\nworkflow: bench\ntasks:\n");
    for i in 0..50 {
        yaml.push_str(&format!("  - id: t{}\n    exec: \"echo\"\n", i));
    }
    let _ = parse_workflow(&yaml).unwrap();
}

fn benchmark_dag_small() {
    let yaml = "schema: \"nika/workflow@0.12\"\nworkflow: b\ntasks:\n  - id: a\n    exec: \"e\"\n";
    let w = parse_workflow(yaml).unwrap();
    let g = Dag::from_workflow(&w).unwrap();
    let _ = g.detect_cycles();
}

fn benchmark_dag_large() {
    let mut yaml = String::from("schema: \"nika/workflow@0.12\"\nworkflow: b\ntasks:\n");
    for i in 0..100 {
        yaml.push_str(&format!("  - id: t{}\n    exec: \"e\"\n", i));
    }
    let w = parse_workflow(&yaml).unwrap();
    let g = Dag::from_workflow(&w).unwrap();
    let _ = g.detect_cycles();
}

fn benchmark_cycle_detect() {
    let mut yaml = String::from("schema: \"nika/workflow@0.12\"\nworkflow: b\ntasks:\n");
    for i in 0..50 {
        yaml.push_str(&format!("  - id: t{}\n", i));
        if i > 0 {
            yaml.push_str(&format!("    depends_on: [t{}]\n", i - 1));
        }
        yaml.push_str("    exec: \"e\"\n");
    }
    let w = parse_workflow(&yaml).unwrap();
    let g = Dag::from_workflow(&w).unwrap();
    let _ = g.detect_cycles();
}
