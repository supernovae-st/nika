//! Benchmark: YAML Workflow Parsing
//!
//! Measures parse_workflow performance across workflow sizes.
//! Run: cargo bench --bench workflow_parsing

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use nika::Workflow;

/// Generate a workflow YAML with N tasks
fn generate_workflow_yaml(task_count: usize) -> String {
    let mut yaml = String::from(
        r#"schema: "nika/workflow@0.5"
provider: claude

tasks:
"#,
    );

    for i in 0..task_count {
        yaml.push_str(&format!(
            r#"  - id: task_{i}
    infer:
      prompt: "Generate content for task {i}"
      model: claude-sonnet-4-20250514
"#
        ));
    }

    // Add flows for sequential dependencies (task_0 -> task_1 -> ... -> task_N-1)
    if task_count > 1 {
        yaml.push_str("\nflows:\n");
        for i in 0..(task_count - 1) {
            yaml.push_str(&format!(
                "  - source: task_{}\n    target: task_{}\n",
                i,
                i + 1
            ));
        }
    }

    yaml
}

/// Generate a workflow with complex use: bindings
fn generate_workflow_with_bindings(task_count: usize) -> String {
    let mut yaml = String::from(
        r#"schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: source_task
    infer:
      prompt: "Generate source data"
"#,
    );

    for i in 0..task_count {
        yaml.push_str(&format!(
            r#"  - id: consumer_{i}
    use:
      data: source_task.result
      fallback: source_task.result ?? "default"
    infer:
      prompt: "Process {{{{use.data}}}} with fallback {{{{use.fallback}}}}"
"#
        ));
    }

    // All consumers depend on source
    yaml.push_str("\nflows:\n");
    for i in 0..task_count {
        yaml.push_str(&format!(
            "  - source: source_task\n    target: consumer_{}\n",
            i
        ));
    }

    yaml
}

/// Generate a workflow with for_each parallelism
fn generate_parallel_workflow(item_count: usize) -> String {
    let items: Vec<String> = (0..item_count).map(|i| format!("\"item_{i}\"")).collect();

    format!(
        r#"schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: parallel_task
    for_each: [{items}]
    as: item
    concurrency: 5
    fail_fast: true
    infer:
      prompt: "Process {{{{use.item}}}}"
"#,
        items = items.join(", ")
    )
}

fn bench_parse_workflow(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_workflow");

    // Benchmark different workflow sizes
    for size in [1, 5, 10, 25, 50, 100].iter() {
        let yaml = generate_workflow_yaml(*size);
        group.throughput(Throughput::Bytes(yaml.len() as u64));

        group.bench_with_input(BenchmarkId::new("linear", size), &yaml, |b, yaml| {
            b.iter(|| {
                let workflow: Workflow = serde_yaml::from_str(black_box(yaml)).unwrap();
                black_box(workflow)
            });
        });
    }

    group.finish();
}

fn bench_parse_workflow_with_bindings(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_workflow_bindings");

    for size in [5, 10, 25, 50].iter() {
        let yaml = generate_workflow_with_bindings(*size);
        group.throughput(Throughput::Bytes(yaml.len() as u64));

        group.bench_with_input(BenchmarkId::new("with_use", size), &yaml, |b, yaml| {
            b.iter(|| {
                let workflow: Workflow = serde_yaml::from_str(black_box(yaml)).unwrap();
                black_box(workflow)
            });
        });
    }

    group.finish();
}

fn bench_parse_parallel_workflow(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_parallel_workflow");

    for size in [5, 10, 25, 50, 100].iter() {
        let yaml = generate_parallel_workflow(*size);
        group.throughput(Throughput::Bytes(yaml.len() as u64));

        group.bench_with_input(BenchmarkId::new("for_each", size), &yaml, |b, yaml| {
            b.iter(|| {
                let workflow: Workflow = serde_yaml::from_str(black_box(yaml)).unwrap();
                black_box(workflow)
            });
        });
    }

    group.finish();
}

fn bench_validate_schema(c: &mut Criterion) {
    let mut group = c.benchmark_group("validate_schema");

    // Parse once, validate repeatedly
    let yaml = generate_workflow_yaml(50);
    let workflow: Workflow = serde_yaml::from_str(&yaml).unwrap();

    group.bench_function("50_tasks", |b| {
        b.iter(|| {
            let result = workflow.validate_schema();
            black_box(result)
        });
    });

    // With for_each validation
    let parallel_yaml = generate_parallel_workflow(50);
    let parallel_workflow: Workflow = serde_yaml::from_str(&parallel_yaml).unwrap();

    group.bench_function("for_each_50_items", |b| {
        b.iter(|| {
            let result = parallel_workflow.validate_schema();
            black_box(result)
        });
    });

    group.finish();
}

fn bench_compute_hash(c: &mut Criterion) {
    let mut group = c.benchmark_group("compute_hash");

    for size in [10, 50, 100].iter() {
        let yaml = generate_workflow_yaml(*size);
        let workflow: Workflow = serde_yaml::from_str(&yaml).unwrap();

        group.bench_with_input(
            BenchmarkId::new("xxh3_64", size),
            &workflow,
            |b, workflow| {
                b.iter(|| {
                    let hash = workflow.compute_hash();
                    black_box(hash)
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_parse_workflow,
    bench_parse_workflow_with_bindings,
    bench_parse_parallel_workflow,
    bench_validate_schema,
    bench_compute_hash,
);
criterion_main!(benches);
