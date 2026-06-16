// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Gate 7 BENCHMARKS · criterion · the parse + analyze hot path.
//!
//! The parser/analyzer is the one component EVERY surface hits: `nika check`,
//! `nika run` (parse before execute), each LSP keystroke / editor save, the
//! `--explain` path. Spec §10 marks Gate 7 NOT exempt for exactly this reason
//! ("the parser is a hot path — every workflow parses"). The envelope: `parse`
//! is a `marked-yaml` tree build + a typed lowering; `analyze` runs the
//! DAG-order + dataflow-typing + IFC passes (super-linear in task count). Both
//! must stay imperceptible on a human-sized workflow and scale gracefully on a
//! large generated one.
//!
//! Reference targets (CPU · informational — the real numbers print on run):
//! - `parse/small_2_tasks`      · a typical hand-written workflow   · target <50µs
//! - `parse/large_200_tasks`    · a 200-node generated DAG          · target <5ms
//! - `analyze/large_200_tasks`  · the super-linear passes           · the one to watch

// Bench fixtures are known-valid by construction; `expect` on the parse of a
// literal fixture is the right failure mode (a broken fixture must fail loud).
#![allow(clippy::expect_used)]

use criterion::{Criterion, criterion_group, criterion_main};
use nika_schema::{FileId, ParseMode, analyze, parse};
use std::hint::black_box;

/// A realistic small workflow: an `infer` producer with a structured-output
/// schema feeding an `exec` consumer that binds two declared output paths.
/// (The `declared_property_path_is_accepted` shape — parses + analyzes clean.)
const SMALL: &str = r#"nika: v1
workflow: bench-small
tasks:
  - id: extract
    infer:
      prompt: "Extract the key entities from the document."
      schema:
        type: object
        additionalProperties: false
        required: [entities]
        properties:
          entities:
            type: array
            items: { type: string }
          count: { type: integer }
  - id: report
    depends_on: [extract]
    exec:
      command: "report ${{ tasks.extract.output.entities }} (${{ tasks.extract.output.count }})"
"#;

/// One `infer` producer + `n` fan-out `exec` consumers, each binding a declared
/// output path so the analyzer's dataflow-typing + DAG passes do real work on
/// every node. Plain `push_str` (no `format!`) keeps the `${{ }}` literal clean.
fn large_workflow(n: usize) -> String {
    let mut s = String::from(
        "nika: v1\nworkflow: bench-large\ntasks:\n  - id: extract\n    infer:\n      \
         prompt: \"Extract entities.\"\n      schema:\n        type: object\n        \
         additionalProperties: false\n        required: [entities]\n        properties:\n          \
         entities:\n            type: array\n            items: { type: string }\n",
    );
    for i in 0..n {
        s.push_str("  - id: report_");
        s.push_str(&i.to_string());
        s.push_str(
            "\n    depends_on: [extract]\n    exec:\n      \
             command: \"report ${{ tasks.extract.output.entities }}\"\n",
        );
    }
    s
}

fn bench_parse(c: &mut Criterion) {
    c.bench_function("parse/small_2_tasks", |b| {
        b.iter(|| parse(black_box(SMALL), FileId::new(0), ParseMode::Lenient));
    });

    let large = large_workflow(200);
    c.bench_function("parse/large_200_tasks", |b| {
        b.iter(|| parse(black_box(&large), FileId::new(0), ParseMode::Lenient));
    });
}

fn bench_analyze(c: &mut Criterion) {
    // analyze() takes the parsed workflow — the super-linear DAG passes.
    let large = large_workflow(200);
    let raw = parse(&large, FileId::new(0), ParseMode::Lenient).expect("bench fixture parses");
    c.bench_function("analyze/large_200_tasks", |b| {
        b.iter(|| analyze(black_box(&raw)));
    });
}

criterion_group!(benches, bench_parse, bench_analyze);
criterion_main!(benches);
