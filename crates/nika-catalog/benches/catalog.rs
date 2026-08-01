// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Criterion benchmarks for catalog hot paths (Gate 7).
//!
//! Targets (on Apple M-series, release profile):
//!   - `find_pricing_scoped`:   p50 ≤ 2 µs (N=62 today, N=500 target)
//!   - `model_capabilities`:    p50 ≤ 1 µs (N=42 rules)
//!
//! Run: `cargo bench -p nika-catalog`

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};

fn bench_find_pricing_scoped(c: &mut Criterion) {
    c.bench_function("find_pricing_scoped (exact)", |b| {
        b.iter(|| nika_catalog::find_pricing_scoped(black_box("openai"), black_box("gpt-4o")));
    });

    c.bench_function("find_pricing_scoped (contains)", |b| {
        b.iter(|| {
            nika_catalog::find_pricing_scoped(
                black_box("anthropic"),
                black_box("claude-sonnet-4-20250514"),
            )
        });
    });

    c.bench_function("find_pricing_scoped (miss)", |b| {
        b.iter(|| {
            nika_catalog::find_pricing_scoped(
                black_box("openai"),
                black_box("nonexistent-model-xyz"),
            )
        });
    });
}

fn bench_model_capabilities(c: &mut Criterion) {
    c.bench_function("model_capabilities (exact rule)", |b| {
        b.iter(|| nika_catalog::model_capabilities(black_box("openai"), black_box("o3")));
    });

    c.bench_function("model_capabilities (prefix rule)", |b| {
        b.iter(|| {
            nika_catalog::model_capabilities(
                black_box("anthropic"),
                black_box("claude-sonnet-4-6-20250514"),
            )
        });
    });

    c.bench_function("model_capabilities (fallback defaults)", |b| {
        b.iter(|| {
            nika_catalog::model_capabilities(
                black_box("unknown-provider"),
                black_box("unknown-model"),
            )
        });
    });
}

fn bench_find_pricing(c: &mut Criterion) {
    c.bench_function("find_pricing (exact)", |b| {
        b.iter(|| nika_catalog::find_pricing(black_box("gpt-4o-mini")));
    });

    c.bench_function("find_pricing (contains)", |b| {
        b.iter(|| nika_catalog::find_pricing(black_box("claude-opus-4-20250514")));
    });
}

criterion_group!(
    benches,
    bench_find_pricing_scoped,
    bench_model_capabilities,
    bench_find_pricing,
);
criterion_main!(benches);
