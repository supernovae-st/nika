// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The ENERGY reading (NEP-0018 · nika-spec `governance/`) — cost honesty
//! transposed to watt-hours over the catalog's sourced facts
//! ([`nika_catalog::ModelEnergy`] · Wh per million OUTPUT tokens ·
//! provenance × scope axes). The static half, descended from the CLI's
//! check renderer (2026-07-29 · the 15k wall · the trust-plane descent
//! precedent: compute descends, render stays) — one input (the COST
//! envelope), so the two rungs can never disagree on a task's shape.
//! Same ladder as [`crate::cost`], same four words:
//!
//! - a **ceiling** (`≤ N Wh`) only where BOTH a `max_tokens` cap and a
//!   sourced figure exist — measured rows render with their axes, so two
//!   honest numbers stay comparable;
//! - **UNBOUNDED** tasks are counted, never ceiled (the COST lane names
//!   them one by one — one voice, no double list);
//! - a model without a figure is **unpriced**, never `0 Wh`;
//! - watt-hours sum WITHIN a scope class and never across it
//!   ([`EnergyReading::scope_subtotals`]) — a mixed set gets one subtotal
//!   per class, not a refusal and not a meaningless sum.
//!
//! A task whose `for_each` iterates a literal EMPTY collection is counted
//! `never_runs` and gets NO row: it provably never executes, so a ceiling
//! over it would be invented. (Measured 2026-07-29 by the probe that
//! found it: with an `iterations.max(1)` guard the rung printed
//! `≤ 0.087 Wh` for a task COST priced at `$0.0000` — two adjacent rungs
//! disagreeing about the same task.)

use crate::cost::{CostCeiling, UnboundedReason};

/// One measured energy row — a task whose cap AND sourced figure both
/// exist, so a ceiling is claimable (`cap × iterations × attempts ×
/// Wh/Mtok` — the COST envelope's own worst-case shape).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[non_exhaustive]
pub struct EnergyTask {
    /// The task id.
    pub task: String,
    /// The resolved model string.
    pub model: String,
    /// Output tokens per call (the cap).
    pub per_call_tokens: u64,
    /// The ceiling in watt-hours.
    pub wh: f64,
    /// Who produced the figure (the catalog row's provenance axis).
    pub provenance: &'static str,
    /// What the figure covers (`gpu` · `device` · `fleet`) — sums never
    /// cross classes.
    pub scope: &'static str,
    /// The figure's measurement month (the catalog row).
    pub measured_at: &'static str,
}

/// What the classification pass counted but cannot put a ceiling on —
/// each bucket is NAMED in the rung's count line rather than folded into
/// a silent denominator.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct EnergyCounts {
    /// No `max_tokens` cap (or an unknown `for_each` count) — the tasks
    /// COST already names one by one.
    pub uncapped: usize,
    /// Capped, but no sourced Wh figure for the model.
    pub unpriced: usize,
    /// Of those, how many are LOCAL runtimes (whose watts are yours).
    pub unpriced_local: usize,
    /// `for_each` over a literal EMPTY collection — the task provably
    /// never executes, so it has no energy to bound. Distinct from
    /// `unpriced`: this is a proven zero, not an unknown.
    pub never_runs: usize,
    /// Every infer/agent task the COST lane priced (the denominator).
    pub total: usize,
}

/// The whole-workflow energy reading.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
#[non_exhaustive]
pub struct EnergyReading {
    /// Per-task measured rows (only tasks with cap AND sourced figure).
    pub tasks: Vec<EnergyTask>,
    /// The uncapped/unpriced/never-run tally.
    pub counts: EnergyCounts,
    /// Watt-hours summed WITHIN each scope class, never across one — a
    /// `gpu` figure covers the accelerator; a `fleet` figure covers
    /// host + idle + datacenter PUE for the same model (roughly twice
    /// the number), so adding them yields an amount that describes
    /// nothing. Ordered by class name.
    pub scope_subtotals: Vec<(String, f64)>,
}

/// The five sovereign local runtimes — their draw is the operator's
/// wall, unpriced by design (never « free », never `0 Wh`).
const LOCAL_PREFIXES: [&str; 5] = ["ollama/", "lmstudio/", "llamacpp/", "localai/", "vllm/"];

/// Compute the reading over the COST envelope (the ONE task-shape input
/// both rungs read — they can never disagree on the per-task shape).
pub(super) fn reading(cost: &CostCeiling) -> EnergyReading {
    let mut tasks = Vec::new();
    let mut counts = EnergyCounts {
        total: cost.tasks.len(),
        ..EnergyCounts::default()
    };
    for c in &cost.tasks {
        let unknown_iters = matches!(c.unbounded_reason, Some(UnboundedReason::UnknownIterations));
        let Some(tokens) = c.max_tokens.filter(|_| !unknown_iters) else {
            counts.uncapped += 1;
            continue;
        };
        if c.iterations == 0 {
            counts.never_runs += 1;
            continue;
        }
        let model = c.model.as_deref().unwrap_or("?");
        let figure = nika_catalog::find_pricing_for(model).and_then(|p| p.energy.as_ref());
        let Some(e) = figure else {
            counts.unpriced += 1;
            if LOCAL_PREFIXES.iter().any(|p| model.starts_with(p)) {
                counts.unpriced_local += 1;
            }
            continue;
        };
        #[allow(clippy::cast_precision_loss)]
        let wh = (tokens as f64) * (c.iterations as f64) * (c.attempts as f64) * e.wh_per_mtok_out
            / 1_000_000.0;
        tasks.push(EnergyTask {
            task: c.task.clone(),
            model: model.to_owned(),
            per_call_tokens: tokens,
            wh,
            provenance: e.provenance,
            scope: e.scope,
            measured_at: e.measured_at,
        });
    }
    let scope_subtotals = subtotals_of(&tasks);
    EnergyReading {
        tasks,
        counts,
        scope_subtotals,
    }
}

/// Watt-hours summed WITHIN each scope class, ordered by class name —
/// the partition IS the answer (a `gpu` figure covers the accelerator;
/// a `fleet` figure host + idle + datacenter PUE — roughly twice for
/// the same model — so adding across classes describes nothing).
fn subtotals_of(tasks: &[EnergyTask]) -> Vec<(String, f64)> {
    let mut by: std::collections::BTreeMap<&'static str, f64> = std::collections::BTreeMap::new();
    for m in tasks {
        *by.entry(m.scope).or_default() += m.wh;
    }
    by.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::{FileId, ParseMode, parse};

    fn energy_of(yaml: &str) -> EnergyReading {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
        crate::check(&wf).energy
    }

    /// The full classification matrix: measured (cap + figure) ·
    /// uncapped (no cap) · uncapped (unknown fan-out) · `never_runs`
    /// (provable zero) · unpriced (no figure) · `unpriced_local` (a local
    /// runtime) — each bucket lands in its own count, and a `gpu`+`gpu`
    /// pair sums WITHIN the class while `fleet` keeps its own subtotal.
    #[test]
    fn the_classification_matrix_and_the_scope_partition() {
        let e = energy_of(
            "nika: m\ntasks:\n  \
             a:\n    infer: { prompt: hi, max_tokens: 1000, model: \"groq/qwen/qwen3-32b\" }\n  \
             b:\n    infer: { prompt: hi, model: \"groq/qwen/qwen3-32b\" }\n  \
             c:\n    for_each: { items: \"${{ tasks.a.output }}\" }\n    infer: { prompt: hi, max_tokens: 100, model: \"groq/qwen/qwen3-32b\" }\n  \
             d:\n    for_each: { items: [] }\n    infer: { prompt: hi, max_tokens: 100, model: \"groq/qwen/qwen3-32b\" }\n  \
             e:\n    infer: { prompt: hi, max_tokens: 100, model: \"mock/echo\" }\n  \
             f:\n    infer: { prompt: hi, max_tokens: 100, model: \"ollama/qwen3\" }\n",
        );
        assert_eq!(e.counts.total, 6, "{:?}", e.counts);
        assert_eq!(
            e.counts.uncapped, 2,
            "b (no cap) + c (unknown iters): {:?}",
            e.counts
        );
        assert_eq!(e.counts.never_runs, 1, "d (empty for_each): {:?}", e.counts);
        assert_eq!(e.counts.unpriced, 2, "mock + ollama: {:?}", e.counts);
        assert_eq!(e.counts.unpriced_local, 1, "ollama: {:?}", e.counts);
        assert_eq!(e.tasks.len(), 1, "only a is measured: {:?}", e.tasks);
        let row = &e.tasks[0];
        assert_eq!(row.task, "a");
        assert_eq!(row.scope, "gpu");
        assert!(row.wh > 0.0, "a positive ceiling: {}", row.wh);
        let subs: Vec<&str> = e.scope_subtotals.iter().map(|(s, _)| s.as_str()).collect();
        assert_eq!(subs, vec!["gpu"], "one class, one subtotal");
        assert!(
            (e.scope_subtotals[0].1 - row.wh).abs() < 1e-12,
            "the subtotal IS the one row"
        );
    }

    /// The scope algebra, unit-proven because the CATALOG cannot prove
    /// it: all sixteen vendored figures are `gpu` today, so the
    /// multi-class path is unreachable from real data and would ship
    /// untested through the binary. A gpu figure and a fleet figure for
    /// the same model differ ~2x — summing across classes describes
    /// nothing.
    #[test]
    fn watt_hours_sum_within_a_scope_never_across_it() {
        let row = |scope: &'static str, wh: f64| EnergyTask {
            task: "t".to_owned(),
            model: "m".to_owned(),
            per_call_tokens: 1,
            wh,
            provenance: "independent-measured",
            scope,
            measured_at: "2025-12",
        };
        let subs = subtotals_of(&[
            row("gpu", 1.0),
            row("fleet", 4.0),
            row("gpu", 0.5),
            row("device", 2.0),
        ]);
        // Ordered by class name, one subtotal each, no grand total.
        let names: Vec<&str> = subs.iter().map(|(s, _)| s.as_str()).collect();
        assert_eq!(names, vec!["device", "fleet", "gpu"]);
        assert!((subs[0].1 - 2.0).abs() < 1e-12);
        assert!((subs[1].1 - 4.0).abs() < 1e-12);
        assert!((subs[2].1 - 1.5).abs() < 1e-12);
        let empty: Vec<(String, f64)> = subtotals_of(&[]);
        assert!(empty.is_empty(), "nothing measured → no claim at all");
    }

    /// Two measured tasks in one scope class sum WITHIN the class; the
    /// empty workflow reads as zero rows, zero counts — never as `0 Wh`.
    #[test]
    fn subtotals_sum_within_a_class_and_empty_stays_silent() {
        let e = energy_of(
            "nika: m\ntasks:\n  \
             a:\n    infer: { prompt: hi, max_tokens: 1000, model: \"groq/qwen/qwen3-32b\" }\n  \
             b:\n    infer: { prompt: hi, max_tokens: 2000, model: \"groq/qwen/qwen3-32b\" }\n",
        );
        assert_eq!(e.tasks.len(), 2);
        assert_eq!(e.scope_subtotals.len(), 1, "one class");
        let sum: f64 = e.tasks.iter().map(|t| t.wh).sum();
        assert!(
            (e.scope_subtotals[0].1 - sum).abs() < 1e-12,
            "the class subtotal sums its rows"
        );

        let e = energy_of("nika: m\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n");
        assert!(e.tasks.is_empty() && e.scope_subtotals.is_empty());
        assert_eq!(e.counts.total, 0);
    }
}
