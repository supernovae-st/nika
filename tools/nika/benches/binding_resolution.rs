//! Benchmark: Binding Resolution
//!
//! Measures use: block parsing and value resolution performance.
//! Run: cargo bench --bench binding_resolution

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use nika::binding::{parse_use_entry, ResolvedBindings, UseEntry, WiringSpec};
use nika::store::{DataStore, TaskResult};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

/// Parse use entry string (e.g., "task.path ?? default")
fn bench_parse_use_entry(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_use_entry");

    // Simple path
    group.bench_function("simple_path", |b| {
        b.iter(|| {
            let entry = parse_use_entry(black_box("weather.summary")).unwrap();
            black_box(entry)
        });
    });

    // Nested path
    group.bench_function("nested_path", |b| {
        b.iter(|| {
            let entry = parse_use_entry(black_box("weather.data.temp.celsius")).unwrap();
            black_box(entry)
        });
    });

    // With numeric default
    group.bench_function("with_default_number", |b| {
        b.iter(|| {
            let entry = parse_use_entry(black_box("x.y ?? 0")).unwrap();
            black_box(entry)
        });
    });

    // With string default
    group.bench_function("with_default_string", |b| {
        b.iter(|| {
            let entry = parse_use_entry(black_box(r#"name ?? "Anonymous""#)).unwrap();
            black_box(entry)
        });
    });

    // With complex object default
    group.bench_function("with_default_object", |b| {
        b.iter(|| {
            let entry =
                parse_use_entry(black_box(r#"cfg ?? {"debug": false, "nested": {"a": 1}}"#))
                    .unwrap();
            black_box(entry)
        });
    });

    // With quoted content containing ??
    group.bench_function("quoted_with_operator", |b| {
        b.iter(|| {
            let entry = parse_use_entry(black_box(r#"x ?? "What?? Really??""#)).unwrap();
            black_box(entry)
        });
    });

    group.finish();
}

/// Create UseEntry directly (programmatic API)
fn bench_use_entry_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("use_entry_creation");

    group.bench_function("new", |b| {
        b.iter(|| {
            let entry = UseEntry::new(black_box("weather.summary"));
            black_box(entry)
        });
    });

    group.bench_function("with_default", |b| {
        b.iter(|| {
            let entry = UseEntry::with_default(black_box("weather.temp"), json!(20));
            black_box(entry)
        });
    });

    group.bench_function("new_lazy", |b| {
        b.iter(|| {
            let entry = UseEntry::new_lazy(black_box("future.result"));
            black_box(entry)
        });
    });

    group.bench_function("lazy_with_default", |b| {
        b.iter(|| {
            let entry = UseEntry::lazy_with_default(black_box("optional.value"), json!("fallback"));
            black_box(entry)
        });
    });

    group.finish();
}

/// Benchmark task_id extraction from path
fn bench_task_id_extraction(c: &mut Criterion) {
    let mut group = c.benchmark_group("task_id_extraction");

    let entries = vec![
        ("simple", UseEntry::new("weather")),
        ("one_level", UseEntry::new("weather.summary")),
        (
            "deep_path",
            UseEntry::new("weather.data.temp.celsius.value"),
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

/// Benchmark ResolvedBindings from WiringSpec
fn bench_resolved_bindings(c: &mut Criterion) {
    let mut group = c.benchmark_group("resolved_bindings");

    // Setup datastore with test data
    let store = DataStore::new();
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

    // Small wiring (3 entries)
    {
        let mut wiring = WiringSpec::default();
        wiring.insert("summary".to_string(), UseEntry::new("weather.summary"));
        wiring.insert("temp".to_string(), UseEntry::new("weather.temp"));
        wiring.insert("name".to_string(), UseEntry::new("user.name"));

        group.bench_function("small_wiring_3", |b| {
            b.iter(|| {
                let bindings =
                    ResolvedBindings::from_wiring_spec(Some(black_box(&wiring)), black_box(&store))
                        .unwrap();
                black_box(bindings)
            });
        });
    }

    // Medium wiring (10 entries)
    {
        let mut wiring = WiringSpec::default();
        for i in 0..10 {
            wiring.insert(format!("val_{i}"), UseEntry::new("weather.summary"));
        }

        group.bench_function("medium_wiring_10", |b| {
            b.iter(|| {
                let bindings =
                    ResolvedBindings::from_wiring_spec(Some(black_box(&wiring)), black_box(&store))
                        .unwrap();
                black_box(bindings)
            });
        });
    }

    // With nested path resolution
    {
        let mut wiring = WiringSpec::default();
        wiring.insert(
            "wind_speed".to_string(),
            UseEntry::new("weather.data.wind.speed"),
        );
        wiring.insert(
            "wind_dir".to_string(),
            UseEntry::new("weather.data.wind.direction"),
        );
        wiring.insert("city".to_string(), UseEntry::new("user.profile.city"));

        group.bench_function("nested_paths", |b| {
            b.iter(|| {
                let bindings =
                    ResolvedBindings::from_wiring_spec(Some(black_box(&wiring)), black_box(&store))
                        .unwrap();
                black_box(bindings)
            });
        });
    }

    // With defaults (missing task)
    {
        let mut wiring = WiringSpec::default();
        wiring.insert(
            "missing".to_string(),
            UseEntry::with_default("nonexistent.value", json!("default")),
        );
        wiring.insert(
            "also_missing".to_string(),
            UseEntry::with_default("another.value", json!(42)),
        );

        group.bench_function("with_defaults", |b| {
            b.iter(|| {
                let bindings =
                    ResolvedBindings::from_wiring_spec(Some(black_box(&wiring)), black_box(&store))
                        .unwrap();
                black_box(bindings)
            });
        });
    }

    // Mixed eager and lazy bindings
    {
        let mut wiring = WiringSpec::default();
        wiring.insert("eager1".to_string(), UseEntry::new("weather.summary"));
        wiring.insert("eager2".to_string(), UseEntry::new("weather.temp"));
        wiring.insert("lazy1".to_string(), UseEntry::new_lazy("weather.data"));
        wiring.insert(
            "lazy2".to_string(),
            UseEntry::lazy_with_default("future.result", json!("pending")),
        );

        group.bench_function("mixed_eager_lazy", |b| {
            b.iter(|| {
                let bindings =
                    ResolvedBindings::from_wiring_spec(Some(black_box(&wiring)), black_box(&store))
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

    let store = DataStore::new();
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
    let mut wiring = WiringSpec::default();
    wiring.insert(
        "lazy_simple".to_string(),
        UseEntry::new_lazy("source.result"),
    );
    wiring.insert(
        "lazy_nested".to_string(),
        UseEntry::new_lazy("source.nested.deep.value"),
    );
    wiring.insert(
        "lazy_with_default".to_string(),
        UseEntry::lazy_with_default("missing.value", json!("fallback")),
    );

    let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();

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
    bench_parse_use_entry,
    bench_use_entry_creation,
    bench_task_id_extraction,
    bench_resolved_bindings,
    bench_binding_access,
    bench_lazy_resolution,
);
criterion_main!(benches);
