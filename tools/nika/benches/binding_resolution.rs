//! Benchmark: Binding Resolution
//!
//! Measures with: block parsing and value resolution performance.
//! Run: cargo bench --bench binding_resolution

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use nika::binding::{parse_binding_entry, BindingEntry, BindingSpec, ResolvedBindings};
use nika::store::{RunContext, TaskResult};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

/// Parse binding entry string (e.g., "task.path ?? default")
fn bench_parse_binding_entry(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_binding_entry");

    // Simple path
    group.bench_function("simple_path", |b| {
        b.iter(|| {
            let entry = parse_binding_entry(black_box("weather.summary")).unwrap();
            black_box(entry)
        });
    });

    // Nested path
    group.bench_function("nested_path", |b| {
        b.iter(|| {
            let entry = parse_binding_entry(black_box("weather.data.temp.celsius")).unwrap();
            black_box(entry)
        });
    });

    // With numeric default
    group.bench_function("with_default_number", |b| {
        b.iter(|| {
            let entry = parse_binding_entry(black_box("x.y ?? 0")).unwrap();
            black_box(entry)
        });
    });

    // With string default
    group.bench_function("with_default_string", |b| {
        b.iter(|| {
            let entry = parse_binding_entry(black_box(r#"name ?? "Anonymous""#)).unwrap();
            black_box(entry)
        });
    });

    // With complex object default
    group.bench_function("with_default_object", |b| {
        b.iter(|| {
            let entry =
                parse_binding_entry(black_box(r#"cfg ?? {"debug": false, "nested": {"a": 1}}"#))
                    .unwrap();
            black_box(entry)
        });
    });

    // With quoted content containing ??
    group.bench_function("quoted_with_operator", |b| {
        b.iter(|| {
            let entry = parse_binding_entry(black_box(r#"x ?? "What?? Really??""#)).unwrap();
            black_box(entry)
        });
    });

    group.finish();
}

/// Create BindingEntry directly (programmatic API)
fn bench_binding_entry_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("binding_entry_creation");

    group.bench_function("new", |b| {
        b.iter(|| {
            let entry = BindingEntry::new(black_box("weather.summary"));
            black_box(entry)
        });
    });

    group.bench_function("with_default", |b| {
        b.iter(|| {
            let entry = BindingEntry::with_default(black_box("weather.temp"), json!(20));
            black_box(entry)
        });
    });

    group.bench_function("new_lazy", |b| {
        b.iter(|| {
            let entry = BindingEntry::new_lazy(black_box("future.result"));
            black_box(entry)
        });
    });

    group.bench_function("lazy_with_default", |b| {
        b.iter(|| {
            let entry =
                BindingEntry::lazy_with_default(black_box("optional.value"), json!("fallback"));
            black_box(entry)
        });
    });

    group.finish();
}

/// Benchmark task_id extraction from path
fn bench_task_id_extraction(c: &mut Criterion) {
    let mut group = c.benchmark_group("task_id_extraction");

    let entries = vec![
        ("simple", BindingEntry::new("weather")),
        ("one_level", BindingEntry::new("weather.summary")),
        (
            "deep_path",
            BindingEntry::new("weather.data.temp.celsius.value"),
        ),
    ];

    for (name, entry) in entries {
        group.bench_function(name, |b| {
            b.iter(|| {
                let task_id = entry.task_id();
                black_box(task_id)
            });
        });
    }

    group.finish();
}

/// Benchmark ResolvedBindings from BindingSpec
fn bench_resolved_bindings(c: &mut Criterion) {
    let mut group = c.benchmark_group("resolved_bindings");

    // Setup datastore with test data
    let store = RunContext::new(nika::trust::InvocationSource::Test);
    store.insert(
        Arc::from("weather"),
        TaskResult::success(
            json!({
                "summary": "Sunny",
                "temp": 25,
                "data": {
                    "humidity": 60,
                    "wind": {
                        "speed": 10,
                        "direction": "N"
                    }
                }
            }),
            Duration::from_secs(1),
        ),
    );
    store.insert(
        Arc::from("user"),
        TaskResult::success(
            json!({
                "name": "Alice",
                "profile": {
                    "age": 30,
                    "city": "Paris"
                }
            }),
            Duration::from_secs(1),
        ),
    );

    // Small spec (3 entries)
    {
        let mut spec = BindingSpec::default();
        spec.insert("summary".to_string(), BindingEntry::new("weather.summary"));
        spec.insert("temp".to_string(), BindingEntry::new("weather.temp"));
        spec.insert("name".to_string(), BindingEntry::new("user.name"));

        group.bench_function("small_spec_3", |b| {
            b.iter(|| {
                let bindings =
                    ResolvedBindings::from_binding_spec(Some(black_box(&spec)), black_box(&store))
                        .unwrap();
                black_box(bindings)
            });
        });
    }

    // Medium spec (10 entries)
    {
        let mut spec = BindingSpec::default();
        for i in 0..10 {
            spec.insert(format!("val_{i}"), BindingEntry::new("weather.summary"));
        }

        group.bench_function("medium_spec_10", |b| {
            b.iter(|| {
                let bindings =
                    ResolvedBindings::from_binding_spec(Some(black_box(&spec)), black_box(&store))
                        .unwrap();
                black_box(bindings)
            });
        });
    }

    // With nested path resolution
    {
        let mut spec = BindingSpec::default();
        spec.insert(
            "wind_speed".to_string(),
            BindingEntry::new("weather.data.wind.speed"),
        );
        spec.insert(
            "wind_dir".to_string(),
            BindingEntry::new("weather.data.wind.direction"),
        );
        spec.insert("city".to_string(), BindingEntry::new("user.profile.city"));

        group.bench_function("nested_paths", |b| {
            b.iter(|| {
                let bindings =
                    ResolvedBindings::from_binding_spec(Some(black_box(&spec)), black_box(&store))
                        .unwrap();
                black_box(bindings)
            });
        });
    }

    // With defaults (missing task)
    {
        let mut spec = BindingSpec::default();
        spec.insert(
            "missing".to_string(),
            BindingEntry::with_default("nonexistent.value", json!("default")),
        );
        spec.insert(
            "also_missing".to_string(),
            BindingEntry::with_default("another.value", json!(42)),
        );

        group.bench_function("with_defaults", |b| {
            b.iter(|| {
                let bindings =
                    ResolvedBindings::from_binding_spec(Some(black_box(&spec)), black_box(&store))
                        .unwrap();
                black_box(bindings)
            });
        });
    }

    // Mixed eager and lazy bindings
    {
        let mut spec = BindingSpec::default();
        spec.insert("eager1".to_string(), BindingEntry::new("weather.summary"));
        spec.insert("eager2".to_string(), BindingEntry::new("weather.temp"));
        spec.insert("lazy1".to_string(), BindingEntry::new_lazy("weather.data"));
        spec.insert(
            "lazy2".to_string(),
            BindingEntry::lazy_with_default("future.result", json!("pending")),
        );

        group.bench_function("mixed_eager_lazy", |b| {
            b.iter(|| {
                let bindings =
                    ResolvedBindings::from_binding_spec(Some(black_box(&spec)), black_box(&store))
                        .unwrap();
                black_box(bindings)
            });
        });
    }

    group.finish();
}

/// Benchmark binding access patterns
fn bench_binding_access(c: &mut Criterion) {
    let mut group = c.benchmark_group("binding_access");

    // Setup bindings
    let mut bindings = ResolvedBindings::new();
    for i in 0..100 {
        bindings.set(format!("key_{i}"), json!(i));
    }

    // Direct get
    group.bench_function("get_existing", |b| {
        b.iter(|| {
            let value = bindings.get(black_box("key_50"));
            black_box(value)
        });
    });

    group.bench_function("get_missing", |b| {
        b.iter(|| {
            let value = bindings.get(black_box("nonexistent"));
            black_box(value)
        });
    });

    // Serialize to Value (for event logging)
    group.bench_function("to_value_100_entries", |b| {
        b.iter(|| {
            let value = bindings.to_value();
            black_box(value)
        });
    });

    // Small bindings serialization
    let mut small_bindings = ResolvedBindings::new();
    for i in 0..5 {
        small_bindings.set(format!("key_{i}"), json!({"nested": i}));
    }

    group.bench_function("to_value_5_entries", |b| {
        b.iter(|| {
            let value = small_bindings.to_value();
            black_box(value)
        });
    });

    group.finish();
}

/// Benchmark lazy binding resolution
fn bench_lazy_resolution(c: &mut Criterion) {
    let mut group = c.benchmark_group("lazy_resolution");

    let store = RunContext::new(nika::trust::InvocationSource::Test);
    store.insert(
        Arc::from("source"),
        TaskResult::success(
            json!({
                "result": "computed value",
                "nested": {
                    "deep": {
                        "value": 42
                    }
                }
            }),
            Duration::from_secs(1),
        ),
    );

    // Setup lazy bindings
    let mut spec = BindingSpec::default();
    spec.insert(
        "lazy_simple".to_string(),
        BindingEntry::new_lazy("source.result"),
    );
    spec.insert(
        "lazy_nested".to_string(),
        BindingEntry::new_lazy("source.nested.deep.value"),
    );
    spec.insert(
        "lazy_with_default".to_string(),
        BindingEntry::lazy_with_default("missing.value", json!("fallback")),
    );

    let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();

    group.bench_function("get_resolved_simple", |b| {
        b.iter(|| {
            let value = bindings
                .get_resolved(black_box("lazy_simple"), black_box(&store))
                .unwrap();
            black_box(value)
        });
    });

    group.bench_function("get_resolved_nested", |b| {
        b.iter(|| {
            let value = bindings
                .get_resolved(black_box("lazy_nested"), black_box(&store))
                .unwrap();
            black_box(value)
        });
    });

    group.bench_function("get_resolved_with_default", |b| {
        b.iter(|| {
            let value = bindings
                .get_resolved(black_box("lazy_with_default"), black_box(&store))
                .unwrap();
            black_box(value)
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_parse_binding_entry,
    bench_binding_entry_creation,
    bench_task_id_extraction,
    bench_resolved_bindings,
    bench_binding_access,
    bench_lazy_resolution,
);
criterion_main!(benches);
