//! Benchmark: DAG Validation
//!
//! Measures FlowGraph construction and validation performance.
//! Run: cargo bench --bench dag_validation

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use nika::{validate_use_wiring, FlowGraph, Workflow};

/// Generate a linear workflow (A -> B -> C -> ...)
fn generate_linear_workflow(size: usize) -> Workflow {
    let mut yaml = String::from(
        r#"schema: "nika/workflow@0.5"
provider: claude
tasks:
"#,
    );

    for i in 0..size {
        yaml.push_str(&format!(
            r#"  - id: task_{i}
    infer: "Task {i}"
"#
        ));
    }

    if size > 1 {
        yaml.push_str("\nflows:\n");
        for i in 0..(size - 1) {
            yaml.push_str(&format!(
                "  - source: task_{}\n    target: task_{}\n",
                i,
                i + 1
            ));
        }
    }

    serde_yaml::from_str(&yaml).unwrap()
}

/// Generate a diamond DAG: A -> (B, C) -> D
fn generate_diamond_workflow(width: usize) -> Workflow {
    let mut yaml = String::from(
        r#"schema: "nika/workflow@0.5"
provider: claude
tasks:
  - id: source
    infer: "Source"
  - id: sink
    infer: "Sink"
"#,
    );

    // Add middle tasks
    for i in 0..width {
        yaml.push_str(&format!(
            r#"  - id: middle_{i}
    infer: "Middle {i}"
"#
        ));
    }

    // source -> all middles, all middles -> sink
    yaml.push_str("\nflows:\n");
    for i in 0..width {
        yaml.push_str(&format!("  - source: source\n    target: middle_{i}\n"));
        yaml.push_str(&format!("  - source: middle_{i}\n    target: sink\n"));
    }

    serde_yaml::from_str(&yaml).unwrap()
}

/// Generate a wide parallel workflow (many independent tasks)
fn generate_parallel_workflow(size: usize) -> Workflow {
    let mut yaml = String::from(
        r#"schema: "nika/workflow@0.5"
provider: claude
tasks:
"#,
    );

    for i in 0..size {
        yaml.push_str(&format!(
            r#"  - id: task_{i}
    infer: "Parallel task {i}"
"#
        ));
    }

    // No flows - all tasks are independent
    serde_yaml::from_str(&yaml).unwrap()
}

/// Generate workflow with use: bindings
fn generate_workflow_with_bindings(size: usize) -> Workflow {
    let mut yaml = String::from(
        r#"schema: "nika/workflow@0.5"
provider: claude
tasks:
  - id: source
    infer: "Source data"
"#,
    );

    for i in 0..size {
        yaml.push_str(&format!(
            r#"  - id: consumer_{i}
    use:
      data: source.result
    infer: "Process {{{{use.data}}}}"
"#
        ));
    }

    yaml.push_str("\nflows:\n");
    for i in 0..size {
        yaml.push_str(&format!("  - source: source\n    target: consumer_{i}\n"));
    }

    serde_yaml::from_str(&yaml).unwrap()
}

fn bench_flowgraph_construction(c: &mut Criterion) {
    let mut group = c.benchmark_group("flowgraph_from_workflow");

    // Linear DAG
    for size in [10, 50, 100, 250].iter() {
        let workflow = generate_linear_workflow(*size);

        group.bench_with_input(BenchmarkId::new("linear", size), &workflow, |b, wf| {
            b.iter(|| {
                let graph = FlowGraph::from_workflow(black_box(wf));
                black_box(graph)
            });
        });
    }

    // Diamond DAG
    for width in [10, 50, 100].iter() {
        let workflow = generate_diamond_workflow(*width);

        group.bench_with_input(BenchmarkId::new("diamond", width), &workflow, |b, wf| {
            b.iter(|| {
                let graph = FlowGraph::from_workflow(black_box(wf));
                black_box(graph)
            });
        });
    }

    // Parallel (no dependencies)
    for size in [10, 50, 100, 250].iter() {
        let workflow = generate_parallel_workflow(*size);

        group.bench_with_input(BenchmarkId::new("parallel", size), &workflow, |b, wf| {
            b.iter(|| {
                let graph = FlowGraph::from_workflow(black_box(wf));
                black_box(graph)
            });
        });
    }

    group.finish();
}

fn bench_cycle_detection(c: &mut Criterion) {
    let mut group = c.benchmark_group("cycle_detection");

    // Linear DAG - no cycles
    for size in [10, 50, 100, 250].iter() {
        let workflow = generate_linear_workflow(*size);
        let graph = FlowGraph::from_workflow(&workflow);

        group.bench_with_input(BenchmarkId::new("linear_no_cycle", size), &graph, |b, g| {
            b.iter(|| {
                let result = g.detect_cycles();
                black_box(result)
            });
        });
    }

    // Diamond DAG - no cycles
    for width in [10, 50, 100].iter() {
        let workflow = generate_diamond_workflow(*width);
        let graph = FlowGraph::from_workflow(&workflow);

        group.bench_with_input(
            BenchmarkId::new("diamond_no_cycle", width),
            &graph,
            |b, g| {
                b.iter(|| {
                    let result = g.detect_cycles();
                    black_box(result)
                });
            },
        );
    }

    group.finish();
}

fn bench_has_path(c: &mut Criterion) {
    let mut group = c.benchmark_group("has_path");

    // Linear DAG - path from start to end (worst case)
    for size in [10, 50, 100, 250].iter() {
        let workflow = generate_linear_workflow(*size);
        let graph = FlowGraph::from_workflow(&workflow);

        group.bench_with_input(
            BenchmarkId::new("linear_full_path", size),
            &graph,
            |b, g| {
                b.iter(|| {
                    let result = g.has_path("task_0", &format!("task_{}", size - 1));
                    black_box(result)
                });
            },
        );
    }

    // Diamond - path from source to sink
    for width in [10, 50, 100].iter() {
        let workflow = generate_diamond_workflow(*width);
        let graph = FlowGraph::from_workflow(&workflow);

        group.bench_with_input(
            BenchmarkId::new("diamond_source_to_sink", width),
            &graph,
            |b, g| {
                b.iter(|| {
                    let result = g.has_path("source", "sink");
                    black_box(result)
                });
            },
        );
    }

    group.finish();
}

fn bench_get_dependencies(c: &mut Criterion) {
    let mut group = c.benchmark_group("get_dependencies");

    // Diamond - get dependencies of sink (many predecessors)
    for width in [10, 50, 100, 250].iter() {
        let workflow = generate_diamond_workflow(*width);
        let graph = FlowGraph::from_workflow(&workflow);

        group.bench_with_input(BenchmarkId::new("diamond_sink", width), &graph, |b, g| {
            b.iter(|| {
                let deps = g.get_dependencies("sink");
                black_box(deps)
            });
        });
    }

    group.finish();
}

fn bench_get_final_tasks(c: &mut Criterion) {
    let mut group = c.benchmark_group("get_final_tasks");

    // Linear - one final task
    for size in [10, 50, 100, 250].iter() {
        let workflow = generate_linear_workflow(*size);
        let graph = FlowGraph::from_workflow(&workflow);

        group.bench_with_input(BenchmarkId::new("linear", size), &graph, |b, g| {
            b.iter(|| {
                let finals = g.get_final_tasks();
                black_box(finals)
            });
        });
    }

    // Parallel - all tasks are final
    for size in [10, 50, 100, 250].iter() {
        let workflow = generate_parallel_workflow(*size);
        let graph = FlowGraph::from_workflow(&workflow);

        group.bench_with_input(BenchmarkId::new("parallel", size), &graph, |b, g| {
            b.iter(|| {
                let finals = g.get_final_tasks();
                black_box(finals)
            });
        });
    }

    group.finish();
}

fn bench_validate_use_wiring(c: &mut Criterion) {
    let mut group = c.benchmark_group("validate_use_wiring");

    for size in [5, 10, 25, 50].iter() {
        let workflow = generate_workflow_with_bindings(*size);
        let graph = FlowGraph::from_workflow(&workflow);

        group.bench_with_input(
            BenchmarkId::new("consumers", size),
            &(&workflow, &graph),
            |b, (wf, g)| {
                b.iter(|| {
                    let result = validate_use_wiring(black_box(wf), black_box(g));
                    black_box(result)
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_flowgraph_construction,
    bench_cycle_detection,
    bench_has_path,
    bench_get_dependencies,
    bench_get_final_tasks,
    bench_validate_use_wiring,
);
criterion_main!(benches);
