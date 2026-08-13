// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! W0 refonte baseline harness · parse / analyze / check across topologies.
//!
//! Purpose (plan §6.4 · W0 exit gate 8): a COMMITTED performance baseline the
//! breaking waves are judged against. Own harness (`harness = false`), not
//! criterion: the deliverable is explicit p50/p95 + allocation counts + the
//! 2k→10k slope, emitted as one JSON line per measurement (stdout `RB{...}`)
//! plus a human table — a runner script snapshots them under docs/perf/.
//!
//! Measured stages: parse (cold) · analyze · check · edit-local re-pipeline ·
//! edit-structural re-pipeline. The two edit rows are deliberately expected to
//! match the full pipeline today: the engine is a full-recompute architecture
//! (no incremental invalidation), and the baseline RECORDS that fact so the
//! local-invalidation law has an honest starting point. Constitution
//! topologies that do not exist yet (workflow call graphs · composition ·
//! evidence/decision bundles) join this harness in their own waves.
//!
//! Bench fixtures are valid by construction — loud failure is correct.
// stdout IS this harness's contract (RB lines + slope table are the artifact
// the runner snapshots) — the workspace-wide print ban is lifted here only.
#![allow(clippy::expect_used, clippy::unwrap_used)]
#![allow(clippy::print_stdout, clippy::disallowed_macros)]
// percentile/slope math casts u128↔f64 by nature; the generator panics loud
// on an unknown topology (fixture bug = correct crash); format!-append keeps
// the generator readable. Harness-scoped allowances, never production code.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::format_push_string,
    clippy::panic,
    clippy::obfuscated_if_else
)]

use nika_check::{analyze, check};
use nika_schema::{FileId, ParseMode, parse};
use std::hint::black_box;
use std::time::Instant;

// NOTE: per-op allocation counts need a counting GlobalAlloc — an unsafe impl
// the workspace FORBIDS (12-gate law). Process peak-RSS is measured externally
// by the runner (`/usr/bin/time -l`); per-op allocations are an owned
// extension pending a sanctioned measurement lane.

/// Deterministic topology generator (mirrors the reference-model generator's
/// spirit: exec argv blocks + one `with:` binding per edge so the dataflow
/// and reference passes genuinely run).
fn workflow(topology: &str, n: usize) -> String {
    let mut s = String::with_capacity(n * 96);
    let slug = topology.replace('_', "-");
    s.push_str(&format!("nika: bench-{slug}-{n}\ntasks:\n"));
    for i in 0..n {
        let deps: Vec<usize> = match topology {
            "chain" => (i > 0).then(|| vec![i - 1]).unwrap_or_default(),
            "fan_out" => (i > 0).then(|| vec![0]).unwrap_or_default(),
            "fan_in" => {
                if i + 1 == n {
                    (0..n - 1).collect()
                } else {
                    vec![]
                }
            }
            "diamond" => {
                // successive layers of width 8; each node waits on two parents
                let layer = i / 8;
                if layer == 0 {
                    vec![]
                } else {
                    let base = (layer - 1) * 8;
                    vec![base + (i % 8), base + ((i + 1) % 8)]
                }
            }
            "mesh" => (1..=i.min(4)).map(|k| i - k).collect(),
            other => panic!("unknown topology {other}"),
        };
        s.push_str(&format!("  t{i}:\n"));
        if !deps.is_empty() {
            let rest: Vec<String> = deps[1..].iter().map(|d| format!("t{d}: success")).collect();
            if !rest.is_empty() {
                s.push_str(&format!("    after: {{ {} }}\n", rest.join(", ")));
            }
            // one real data binding so refs/dataflow passes have work to do
            s.push_str(&format!(
                "    with:\n      up: ${{{{ tasks.t{}.output }}}}\n",
                deps[0]
            ));
        }
        s.push_str("    exec:\n      command: [\"true\"]\n");
    }
    s
}

fn percentiles(mut us: Vec<u128>) -> (u128, u128) {
    us.sort_unstable();
    let p = |q: f64| us[((us.len() - 1) as f64 * q) as usize];
    (p(0.50), p(0.95))
}

fn iters_for(n: usize) -> usize {
    match n {
        0..=100 => 40,
        101..=500 => 15,
        501..=2000 => 8,
        _ => 4,
    }
}

struct Row {
    op: &'static str,
    topo: String,
    n: usize,
    p50_us: u128,
    p95_us: u128,
}

fn measure(op: &'static str, topo: &str, n: usize, iters: usize, mut f: impl FnMut()) -> Row {
    // warmup
    f();
    let mut samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t = Instant::now();
        f();
        samples.push(t.elapsed().as_micros());
    }
    let (p50_us, p95_us) = percentiles(samples);
    Row {
        op,
        topo: topo.to_string(),
        n,
        p50_us,
        p95_us,
    }
}

fn main() {
    let sizes = [100usize, 500, 2000, 10_000];
    let topologies = ["chain", "fan_out", "fan_in", "diamond", "mesh"];
    let mut rows: Vec<Row> = Vec::new();

    for topo in topologies {
        for n in sizes {
            let text = workflow(topo, n);
            let iters = iters_for(n);
            rows.push(measure("parse", topo, n, iters, || {
                black_box(parse(&text, FileId::new(0), ParseMode::Strict).expect("valid"));
            }));
            let wf = parse(&text, FileId::new(0), ParseMode::Strict).expect("valid");
            rows.push(measure("analyze", topo, n, iters, || {
                black_box(analyze(&wf).expect("valid"));
            }));
            rows.push(measure("check", topo, n, iters, || {
                black_box(check(&wf));
            }));
        }
    }

    // Local vs structural edit — full re-pipeline both ways today (recorded).
    let base = workflow("diamond", 2000);
    // a LEAF edit — one task grows a field that changes no edge
    let local = base.replace("  t1000:\n", "  t1000:\n    timeout: 30s\n");
    let structural = base.replace(
        "  t1999:\n",
        "  t1999x:\n    exec:\n      command: [\"true\"]\n  t1999:\n",
    );
    for (name, text) in [("edit_local", &local), ("edit_structural", &structural)] {
        rows.push(measure(name, "diamond", 2000, 8, || {
            let wf = parse(text, FileId::new(0), ParseMode::Strict).expect("valid");
            let _ = black_box(analyze(&wf));
            black_box(check(&wf));
        }));
    }

    println!("\nop              topo      n      p50_us     p95_us");
    for r in &rows {
        println!(
            "{:<15} {:<9} {:<6} {:>9} {:>10}",
            r.op, r.topo, r.n, r.p50_us, r.p95_us
        );
        println!(
            "RB{{\"op\":\"{}\",\"topo\":\"{}\",\"n\":{},\"p50_us\":{},\"p95_us\":{}}}",
            r.op, r.topo, r.n, r.p50_us, r.p95_us
        );
    }

    // Slope law: T(10k)/T(2k) ≤ 6.25 (linear=5 · ×1.25 headroom · quadratic=25).
    let mut slope_violations = 0;
    for op in ["parse", "analyze", "check"] {
        for topo in topologies {
            let g = |n: usize| {
                rows.iter()
                    .find(|r| r.op == op && r.topo == topo && r.n == n)
                    .map(|r| r.p50_us.max(1))
                    .unwrap()
            };
            let slope = g(10_000) as f64 / g(2000) as f64;
            let flag = if slope > 6.25 { " ← SLOPE" } else { "" };
            if slope > 6.25 {
                slope_violations += 1;
            }
            println!("SLOPE {op}/{topo}: 2k→10k ×{slope:.2}{flag}");
        }
    }
    println!("slope violations (>6.25): {slope_violations}");
}
