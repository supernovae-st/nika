//! Benchmark: Task Execution Simulation
//!
//! Measures execution overhead without actual LLM/HTTP calls.
//! Focuses on DataStore operations, event emission, and binding resolution.
//!
//! Run: cargo bench --bench task_execution

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use nika::binding::ResolvedBindings;
use nika::store::{DataStore, TaskResult};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

/// Benchmark DataStore operations
fn bench_datastore_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("datastore");

    // Insert operations
    group.bench_function("insert", |b| {
        let store = DataStore::new();
        let mut i = 0u64;
        b.iter(|| {
            let task_id = Arc::from(format!("task_{i}"));
            let result = TaskResult::success(json!({"value": i}), Duration::from_millis(100));
            store.insert(black_box(task_id), black_box(result));
            i += 1;
        });
    });

    // Get operations (pre-populated store)
    {
        let store = DataStore::new();
        for i in 0..1000 {
            store.insert(
                Arc::from(format!("task_{i}")),
                TaskResult::success(json!({"value": i}), Duration::from_millis(100)),
            );
        }

        group.bench_function("get_existing", |b| {
            b.iter(|| {
                let result = store.get_output(black_box("task_500"));
                black_box(result)
            });
        });

        group.bench_function("get_missing", |b| {
            b.iter(|| {
                let result = store.get_output(black_box("nonexistent"));
                black_box(result)
            });
        });
    }

    // Concurrent access simulation
    {
        let store = Arc::new(DataStore::new());
        for i in 0..100 {
            store.insert(
                Arc::from(format!("task_{i}")),
                TaskResult::success(json!({"data": i}), Duration::from_millis(i as u64)),
            );
        }

        group.bench_function("concurrent_get", |b| {
            b.iter(|| {
                // Simulate reading from multiple tasks
                for i in 0..10 {
                    let _ = store.get_output(&format!("task_{}", i * 10));
                }
            });
        });
    }

    group.finish();
}

/// Benchmark TaskResult creation
fn bench_task_result(c: &mut Criterion) {
    let mut group = c.benchmark_group("task_result");

    // Success with small output
    group.bench_function("success_small", |b| {
        b.iter(|| {
            let result = TaskResult::success(
                black_box(json!({"status": "ok"})),
                black_box(Duration::from_millis(100)),
            );
            black_box(result)
        });
    });

    // Success with large output
    let large_output = json!({
        "data": (0..100).map(|i| json!({"id": i, "name": format!("item_{i}")})).collect::<Vec<_>>(),
        "metadata": {
            "count": 100,
            "page": 1,
            "total_pages": 10
        }
    });

    group.bench_function("success_large", |b| {
        b.iter(|| {
            let result =
                TaskResult::success(black_box(large_output.clone()), Duration::from_millis(500));
            black_box(result)
        });
    });

    // Failed result
    group.bench_function("failed", |b| {
        b.iter(|| {
            let result = TaskResult::failed(
                black_box("Task execution failed: timeout".to_string()),
                black_box(Duration::from_secs(30)),
            );
            black_box(result)
        });
    });

    group.finish();
}

/// Benchmark task status checks
fn bench_task_status(c: &mut Criterion) {
    let mut group = c.benchmark_group("task_status");

    let success = TaskResult::success(json!({}), Duration::from_millis(100));
    let failed = TaskResult::failed("error".to_string(), Duration::from_millis(100));

    group.bench_function("check_success", |b| {
        b.iter(|| {
            let is_success = success.is_success();
            black_box(is_success)
        });
    });

    group.bench_function("check_failed", |b| {
        b.iter(|| {
            let is_success = failed.is_success();
            black_box(!is_success)
        });
    });

    group.bench_function("get_error", |b| {
        b.iter(|| {
            let error = failed.error();
            black_box(error)
        });
    });

    group.finish();
}

/// Benchmark simulated task execution flow
fn bench_execution_flow(c: &mut Criterion) {
    let mut group = c.benchmark_group("execution_flow");

    // Simulate task execution: resolve bindings -> execute -> store result
    {
        let store = DataStore::new();
        store.insert(
            Arc::from("source"),
            TaskResult::success(
                json!({"message": "Hello", "count": 42}),
                Duration::from_millis(100),
            ),
        );

        // Pre-create bindings
        let mut bindings = ResolvedBindings::new();
        bindings.set("msg", json!("Hello"));
        bindings.set("cnt", json!(42));

        group.bench_function("resolve_and_store", |b| {
            let mut i = 0u64;
            b.iter(|| {
                // Simulate: resolve bindings, process, store result
                let _msg = bindings.get("msg");
                let _cnt = bindings.get("cnt");

                // Simulate processing (just creating output)
                let output = json!({"processed": true, "iteration": i});

                // Store result
                let task_id = Arc::from(format!("task_{i}"));
                store.insert(
                    black_box(task_id),
                    TaskResult::success(black_box(output), Duration::from_millis(50)),
                );

                i += 1;
            });
        });
    }

    // Simulate parallel task completion tracking
    {
        let store = Arc::new(DataStore::new());

        group.bench_function("parallel_completion_10", |b| {
            b.iter(|| {
                // Simulate 10 parallel tasks completing
                for i in 0..10 {
                    let task_id = Arc::from(format!("parallel_{i}"));
                    store.insert(
                        black_box(task_id),
                        TaskResult::success(json!({"result": i}), Duration::from_millis(100)),
                    );
                }

                // Check all completed
                let mut all_done = true;
                for i in 0..10 {
                    if store.get_output(&format!("parallel_{i}")).is_none() {
                        all_done = false;
                    }
                }
                black_box(all_done)
            });
        });
    }

    group.finish();
}

/// Benchmark for_each iteration simulation
fn bench_for_each_simulation(c: &mut Criterion) {
    let mut group = c.benchmark_group("for_each_simulation");

    // Simulate collecting results from parallel iterations
    for item_count in [5, 10, 25, 50, 100].iter() {
        let store = DataStore::new();

        // Pre-populate with iteration results
        for i in 0..*item_count {
            store.insert(
                Arc::from(format!("iter_{i}")),
                TaskResult::success(
                    json!({"item": i, "processed": true}),
                    Duration::from_millis(50),
                ),
            );
        }

        group.throughput(Throughput::Elements(*item_count as u64));

        group.bench_with_input(
            BenchmarkId::new("collect_results", item_count),
            item_count,
            |b, &count| {
                b.iter(|| {
                    let mut results = Vec::with_capacity(count);
                    for i in 0..count {
                        if let Some(output) = store.get_output(&format!("iter_{i}")) {
                            results.push((*output).clone());
                        }
                    }
                    black_box(results)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark binding creation for iteration variables
fn bench_iteration_bindings(c: &mut Criterion) {
    let mut group = c.benchmark_group("iteration_bindings");

    // Simulate creating bindings for each iteration
    for item_count in [10, 25, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("create_bindings", item_count),
            item_count,
            |b, &count| {
                b.iter(|| {
                    let items: Vec<String> = (0..count).map(|i| format!("item_{i}")).collect();

                    let mut all_bindings = Vec::with_capacity(count);
                    for item in &items {
                        let mut bindings = ResolvedBindings::new();
                        bindings.set("item", json!(item));
                        bindings.set("index", json!(all_bindings.len()));
                        all_bindings.push(bindings);
                    }
                    black_box(all_bindings)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark string interning simulation (task IDs)
fn bench_task_id_interning(c: &mut Criterion) {
    let mut group = c.benchmark_group("task_id_interning");

    // Arc<str> creation
    group.bench_function("arc_str_from_string", |b| {
        b.iter(|| {
            let id = Arc::<str>::from(black_box("task_name_example"));
            black_box(id)
        });
    });

    // Arc clone (should be very fast)
    let id = Arc::<str>::from("task_name_example");
    group.bench_function("arc_str_clone", |b| {
        b.iter(|| {
            let cloned = Arc::clone(black_box(&id));
            black_box(cloned)
        });
    });

    // Multiple task IDs
    group.bench_function("create_100_task_ids", |b| {
        b.iter(|| {
            let ids: Vec<Arc<str>> = (0..100).map(|i| Arc::from(format!("task_{i}"))).collect();
            black_box(ids)
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_datastore_operations,
    bench_task_result,
    bench_task_status,
    bench_execution_flow,
    bench_for_each_simulation,
    bench_iteration_bindings,
    bench_task_id_interning,
);
criterion_main!(benches);
