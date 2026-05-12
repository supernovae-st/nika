// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Gate 7 BENCHMARKS · criterion · BM25 hot-path.
//!
//! Targets per `nika-bm25.md` spec ·
//! - `manning` · 4-doc corpus `top_k` · canonical fixture · sub-µs envelope
//! - `synthetic_100` · 100-doc · ~30 token avg · `top_k(10)` target <1ms
//! - `finalize_100` · 100-doc · target <5ms

use criterion::{Criterion, criterion_group, criterion_main};
use nika_bm25::{BmIndex, BmParams};
use std::hint::black_box;

fn manning_corpus() -> BmIndex {
    let mut idx = BmIndex::new(BmParams::default());
    idx.add_document(1, "auto car insurance");
    idx.add_document(2, "best auto insurance");
    idx.add_document(3, "car insurance best auto");
    idx.add_document(4, "insurance best car");
    idx.finalize();
    idx
}

fn synthetic_corpus(n: u32) -> BmIndex {
    let vocab = [
        "auto",
        "car",
        "insurance",
        "best",
        "fast",
        "cheap",
        "policy",
        "claim",
        "agent",
        "premium",
        "deductible",
        "liability",
        "comprehensive",
        "collision",
    ];
    let mut idx = BmIndex::new(BmParams::default());
    for i in 0..n {
        // Cheap deterministic shuffle · LCG-style for repeatability.
        let mut tokens = String::new();
        let len = 20 + (i % 20);
        for j in 0..len {
            let pick = ((i * 31 + j * 17) as usize) % vocab.len();
            if j > 0 {
                tokens.push(' ');
            }
            tokens.push_str(vocab[pick]);
        }
        idx.add_document(i + 1, &tokens);
    }
    idx.finalize();
    idx
}

fn bench_manning_top_k(c: &mut Criterion) {
    let idx = manning_corpus();
    c.bench_function("manning_top_k_3", |b| {
        b.iter(|| {
            let r = idx.top_k(black_box("best car insurance"), black_box(3));
            black_box(r)
        });
    });
}

fn bench_synthetic_100_top_k(c: &mut Criterion) {
    let idx = synthetic_corpus(100);
    c.bench_function("synthetic_100_top_k_10", |b| {
        b.iter(|| {
            let r = idx.top_k(black_box("best premium policy"), black_box(10));
            black_box(r)
        });
    });
}

fn bench_finalize_100(c: &mut Criterion) {
    c.bench_function("finalize_100", |b| {
        b.iter(|| {
            let idx = synthetic_corpus(black_box(100));
            black_box(idx)
        });
    });
}

// ─── Production-scale benches (post socratic critique 2026-05-12 Q6) ────
//
// rust-perf measured 71× headroom on synthetic_100_top_k_10 (14 µs vs 1ms)
// but production target per ADR-078 + spec §1 hybrid retrieval is 10k-1M
// docs. Linear extrapolation says 14 µs × 100 = 1.4 ms at 10k · STILL
// within target but cache effects + BTreeMap depth + IDF cache misses may
// invalidate the linear-scale assumption. These benches validate.

fn bench_synthetic_10k_top_k(c: &mut Criterion) {
    let idx = synthetic_corpus(10_000);
    c.bench_function("synthetic_10k_top_k_10", |b| {
        b.iter(|| {
            let r = idx.top_k(black_box("best premium policy"), black_box(10));
            black_box(r)
        });
    });
}

fn bench_finalize_10k(c: &mut Criterion) {
    c.bench_function("finalize_10k", |b| {
        b.iter(|| {
            let idx = synthetic_corpus(black_box(10_000));
            black_box(idx)
        });
    });
}

criterion_group!(
    benches,
    bench_manning_top_k,
    bench_synthetic_100_top_k,
    bench_finalize_100,
    bench_synthetic_10k_top_k,
    bench_finalize_10k
);
criterion_main!(benches);
