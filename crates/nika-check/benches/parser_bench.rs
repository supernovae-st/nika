// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Criterion benchmarks for the parse + check hot path (Gate 7).
//!
//! The parser + the static-check ladder run on EVERY `nika check` / `nika run`
//! before a single token is spent, so `parse()` + `check()` latency is
//! load-bearing — this is why Gate 7 is NOT exempt for this crate (crate-spec
//! §10). The benches track parse + the full pre-flight across workflow sizes.
//!
//! Targets (Apple M-series · release profile):
//!   - `parse` · 10-task workflow:  p50 < 1 ms (crate-spec §10)
//!   - `check` · 10-task workflow:  p50 < 2 ms (parse + analyze + every pass)
//!
//! Run: `cargo bench -p nika-check`

use std::fmt::Write as _;

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use nika_check::check;
use nika_schema::{FileId, ParseMode, parse};

/// A linear chain of `n` infer tasks (each depends on the previous) — the
/// shape that exercises the parser, the Kahn topo waves, the DAG width /
/// reachability analysis, and the cost + IFC passes at scale. Writes to the
/// `String` never fail, so the `write!` results are intentionally discarded.
fn chain_yaml(n: usize) -> String {
    let mut s = String::from("nika: bench\n\nmodel: mock/echo\n\ntasks:\n");
    for i in 0..n {
        let _ = writeln!(s, "  t{i}:");
        if i > 0 {
            let _ = writeln!(s, "    after: {{ t{}: success }}", i - 1);
        }
        s.push_str("    infer:\n      prompt: \"step\"\n      max_tokens: 64\n");
    }
    s
}

fn bench_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse");
    for n in [1usize, 10, 100] {
        let yaml = chain_yaml(n);
        group.bench_function(format!("{n}-task"), |b| {
            b.iter(|| parse(black_box(&yaml), FileId::new(0), ParseMode::Strict));
        });
    }
    group.finish();
}

fn bench_check(c: &mut Criterion) {
    let mut group = c.benchmark_group("check");
    for n in [1usize, 10, 100] {
        let yaml = chain_yaml(n);
        // Parse once outside the timed loop — the bench isolates the
        // analyze + check-ladder cost on an already-parsed workflow. The
        // fixtures are statically valid, so the `else` arm never runs.
        let Ok(wf) = parse(&yaml, FileId::new(0), ParseMode::Strict) else {
            continue;
        };
        group.bench_function(format!("{n}-task"), |b| {
            b.iter(|| check(black_box(&wf)));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_parse, bench_check);
criterion_main!(benches);
