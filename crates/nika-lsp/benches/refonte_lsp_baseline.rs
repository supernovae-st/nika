// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! W0 refonte baseline harness · the oracle's hot operations (LSP side).
//!
//! Same contract as nika-schema's `refonte_baseline`: explicit p50/p95 per
//! operation and size, `RB{...}` JSON lines for the runner to snapshot under
//! docs/perf/. Operations = what the server actually serves today: hover ·
//! completion (a `${{ tasks.` member lane) · nika/semanticDocument. Editor
//! rename/references live client-side and code actions are quickfix
//! projections — they join the harness when the corresponding server lanes
//! ship (typed holes / actions are W7 additive).
//!
//! Every measurement is a FULL re-analysis by design: the server re-parses
//! per request (single-file world, no per-doc cache) — the baseline records
//! that architecture honestly; the local-invalidation law is judged against
//! these numbers by future waves.
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

use nika_lsp::analysis::completion::completion;
use nika_lsp::analysis::hover::hover;
use nika_lsp::analysis::semantic_document::semantic_document;
use std::hint::black_box;
use std::time::Instant;

fn workflow(topology: &str, n: usize) -> String {
    let mut s = String::with_capacity(n * 96);
    let slug = topology.replace('_', "-");
    s.push_str(&format!(
        "nika: v1\nworkflow:\n  id: bench-{slug}-{n}\ntasks:\n"
    ));
    for i in 0..n {
        let deps: Vec<usize> = match topology {
            "chain" => (i > 0).then(|| vec![i - 1]).unwrap_or_default(),
            "diamond" => {
                let layer = i / 8;
                if layer == 0 {
                    vec![]
                } else {
                    let base = (layer - 1) * 8;
                    vec![base + (i % 8), base + ((i + 1) % 8)]
                }
            }
            other => panic!("unknown topology {other}"),
        };
        s.push_str(&format!("  t{i}:\n"));
        if !deps.is_empty() {
            let rest: Vec<String> = deps[1..]
                .iter()
                .map(|d| format!("t{d}: succeeded"))
                .collect();
            if !rest.is_empty() {
                s.push_str(&format!("    after: {{ {} }}\n", rest.join(", ")));
            }
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
        0..=500 => 30,
        501..=2000 => 12,
        _ => 5,
    }
}

fn measure(op: &str, topo: &str, n: usize, iters: usize, mut f: impl FnMut()) -> (u128, u128) {
    f(); // warmup
    let mut samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t = Instant::now();
        f();
        samples.push(t.elapsed().as_micros());
    }
    let (p50, p95) = percentiles(samples);
    println!("{op:<18} {topo:<9} {n:<6} {p50:>9} {p95:>10}");
    println!(
        "RB{{\"op\":\"{op}\",\"topo\":\"{topo}\",\"n\":{n},\"p50_us\":{p50},\"p95_us\":{p95}}}"
    );
    (p50, p95)
}

fn main() {
    println!("\nop                 topo      n      p50_us     p95_us");
    let mut slopes: Vec<(String, f64)> = Vec::new();
    for topo in ["chain", "diamond"] {
        let mut at: Vec<(usize, u128, u128, u128)> = Vec::new(); // n, hover, compl, semdoc
        for n in [100usize, 500, 2000, 10_000] {
            let text = workflow(topo, n);
            let mid = n / 2;
            // hover anchor: the declaring id token of the middle task
            let hover_off = text.find(&format!("\n  t{mid}:\n")).expect("mid task") + 3;
            // completion anchor: right after `tasks.` inside a mid-file binding
            let marker = format!("up: ${{{{ tasks.t{}.", mid.saturating_sub(1));
            let compl_off = text.find(&marker).map_or(hover_off, |p| {
                p + marker.len() - format!("t{}.", mid.saturating_sub(1)).len()
            });
            let iters = iters_for(n);
            let (h, _) = measure("hover", topo, n, iters, || {
                black_box(hover(&text, hover_off));
            });
            let (c, _) = measure("completion", topo, n, iters, || {
                black_box(completion(&text, compl_off));
            });
            let (s, _) = measure("semantic_document", topo, n, iters, || {
                black_box(semantic_document(&text));
            });
            at.push((n, h, c, s));
        }
        let g = |col: usize, n: usize| {
            at.iter()
                .find(|r| r.0 == n)
                .map(|r| match col {
                    1 => r.1,
                    2 => r.2,
                    _ => r.3,
                })
                .unwrap()
                .max(1)
        };
        for (col, name) in [(1, "hover"), (2, "completion"), (3, "semantic_document")] {
            let slope = g(col, 10_000) as f64 / g(col, 2000) as f64;
            slopes.push((format!("{name}/{topo}"), slope));
        }
    }
    let mut violations = 0;
    for (name, slope) in &slopes {
        let flag = if *slope > 6.25 { " ← SLOPE" } else { "" };
        if *slope > 6.25 {
            violations += 1;
        }
        println!("SLOPE {name}: 2k→10k ×{slope:.2}{flag}");
    }
    println!("slope violations (>6.25): {violations}");
}
