// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `--max-cost-usd` pre-run preflight — refuse BEFORE any spend
//! when the STATIC floor (cheapest path · gates closed · first-try:
//! the unavoidable exposure `nika check` computes) already exceeds the
//! budget · warn loud when the ceiling cannot bound everything (the
//! budget gates METERED spend only — local/mock work never trips it).

use nika_schema::check::{CheckReport, CostCeiling, UnboundedReason};
use nika_schema::raw::RawWorkflow;

use crate::verbs::exit;

/// Gate the run on the operator budget. `Err(exit_code)` = refuse
/// (exit 2 · nothing was spent) · `Ok(())` = proceed (possibly after
/// the loud unbounded warning on stderr).
///
/// `model_override` — the floor must price the EFFECTIVE model (#342):
/// `--model m` replaces the envelope default at runtime, and the
/// override form IS the delegation idiom agent surfaces teach — a gate
/// that prices the file's model while the run uses another never fires
/// for exactly that population.
pub(super) fn preflight(
    wf: &RawWorkflow,
    report: &CheckReport,
    model_override: Option<&str>,
    max_cost_usd: Option<f64>,
    output_json: bool,
) -> Result<(), u8> {
    let Some(budget) = max_cost_usd else {
        return Ok(());
    };
    let effective;
    let cost = match model_override {
        None => &report.cost,
        Some(m) => {
            effective = effective_cost(wf, m);
            &effective
        }
    };
    if let Some(refusal) = floor_refusal(cost.min_path_total_usd, budget) {
        super::emit_diagnostic(&refusal, output_json);
        return Err(exit::FILE);
    }
    if cost.has_unbounded {
        eprintln!(
            "⚠ --max-cost-usd {budget}: {} — the budget bounds METERED spend \
             only; local/mock work never trips it",
            unbounded_breakdown(cost)
        );
    }
    Ok(())
}

/// The cost envelope with the CLI `--model` substituted for the envelope
/// default (per-task `model:` overrides keep winning, mirroring the
/// runtime's precedence).
fn effective_cost(wf: &RawWorkflow, model_override: &str) -> CostCeiling {
    nika_schema::check::check(&crate::verbs::with_model_override(wf, model_override)).cost
}

/// Tally the unbounded tasks BY THEIR ACTUAL reason (the report carries
/// `unbounded_reason` per task) instead of parroting the fixed
/// disjunction — a priced-but-unbounded task read « unpriced model »,
/// which misleads (the fixable one is `no max_tokens`, not the model).
/// The operator sees WHICH kind they have, and which is fixable.
fn unbounded_breakdown(cost: &CostCeiling) -> String {
    let (mut no_tokens, mut unpriced, mut unknown_iters) = (0_usize, 0_usize, 0_usize);
    for t in cost.tasks.iter().filter(|t| t.usd.is_none()) {
        match t.unbounded_reason {
            Some(UnboundedReason::NoTokenLimit) => no_tokens += 1,
            Some(UnboundedReason::NoPrice) => unpriced += 1,
            // A task with no price AND no ceiling records ONE reason
            // (NoPrice wins in the check ladder); UnknownIterations, an
            // unclassified None, and any FUTURE reason (the enum is
            // #[non_exhaustive]) all count as the generic bucket.
            _ => unknown_iters += 1,
        }
    }
    let total = no_tokens + unpriced + unknown_iters;
    let mut parts = Vec::new();
    if no_tokens > 0 {
        parts.push(format!("{no_tokens} with no `max_tokens`"));
    }
    if unpriced > 0 {
        parts.push(format!("{unpriced} on an unpriced model"));
    }
    if unknown_iters > 0 {
        parts.push(format!("{unknown_iters} with unknown iterations"));
    }
    format!(
        "{total} task(s) have no static ceiling ({})",
        parts.join(" · ")
    )
}

/// `Some(refusal)` when the floor exceeds the budget — pure, so the
/// operator-facing gate is unit-testable. A floor AT the budget passes
/// (spending exactly the budget is not over it).
fn floor_refusal(floor: f64, budget: f64) -> Option<String> {
    (floor > budget).then(|| {
        format!(
            "refusing to start: the workflow's unavoidable cost floor \
             ${floor:.6} exceeds --max-cost-usd ${budget:.6} (cheapest \
             static path · gates closed · first-try) — raise the budget \
             or trim the workflow (`nika check` shows the envelope)\n"
        )
    })
}

/// The operator-facing budget preflight — pure-fn pinned (F4.2: the
/// gate the operator actually touches must not ride untested).
#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::float_cmp
)]
mod tests {
    use nika_schema::{FileId, ParseMode, parse};

    use super::{effective_cost, floor_refusal, preflight, unbounded_breakdown};

    fn breakdown_of(yaml: &str) -> String {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
        unbounded_breakdown(&nika_schema::check::check(&wf).cost)
    }

    /// #342 — the delegation idiom (`--model <p/m> --max-cost-usd <usd>`):
    /// the file says mock (floor $0, would pass); the OVERRIDE is a priced
    /// model whose bounded floor exceeds the budget — the gate must refuse
    /// BEFORE any spend, exactly like the in-file form.
    #[test]
    fn override_prices_the_effective_model_and_refuses_at_the_gate() {
        let yaml = "nika: v1\nworkflow: m\ntasks:\n  \
             - id: a\n    infer: { prompt: hi, max_tokens: 1000000, model: \"mock/echo\" }\n";
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
        let report = nika_schema::check::check(&wf);
        assert_eq!(
            report.cost.min_path_total_usd, 0.0,
            "the FILE's floor is zero — the un-overridden gate would pass"
        );
        // Task-level model wins over the envelope — the override swaps the
        // ENVELOPE default, so the fixture's model must live there instead.
        let yaml = "nika: v1\nworkflow: m\nmodel: \"mock/echo\"\ntasks:\n  \
             - id: a\n    infer: { prompt: hi, max_tokens: 1000000 }\n";
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
        let report = nika_schema::check::check(&wf);
        let refused = preflight(
            &wf,
            &report,
            Some("anthropic/claude-sonnet-5"),
            Some(0.000_001),
            false,
        );
        assert_eq!(
            refused,
            Err(crate::verbs::exit::FILE),
            "the effective (overridden) model's floor must trip the gate"
        );
        // The exact same call WITHOUT the override passes — the file's
        // mock floor is zero (the pre-#342 behavior, still correct there).
        assert_eq!(
            preflight(&wf, &report, None, Some(0.000_001), false),
            Ok(())
        );
    }

    /// The mirror: an expensive in-file model overridden to mock must NOT
    /// refuse — the effective floor is zero (offline preview idiom).
    #[test]
    fn override_to_mock_drops_the_floor_and_passes() {
        let yaml = "nika: v1\nworkflow: m\nmodel: \"anthropic/claude-sonnet-5\"\ntasks:\n  \
             - id: a\n    infer: { prompt: hi, max_tokens: 1000000 }\n";
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
        let report = nika_schema::check::check(&wf);
        assert!(
            report.cost.min_path_total_usd > 0.000_001,
            "the FILE's floor alone would refuse"
        );
        assert_eq!(
            preflight(&wf, &report, Some("mock/echo"), Some(0.000_001), false),
            Ok(()),
            "the effective (mock) floor is zero — no refusal"
        );
    }

    /// Task-level `model:` keeps winning over the CLI override (the
    /// runtime's precedence, mirrored in the effective envelope).
    #[test]
    fn task_level_model_still_beats_the_override_in_the_effective_floor() {
        let yaml = "nika: v1\nworkflow: m\nmodel: \"mock/echo\"\ntasks:\n  \
             - id: a\n    infer: { prompt: hi, max_tokens: 1000000, model: \"mock/echo\" }\n";
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
        let cost = effective_cost(&wf, "anthropic/claude-sonnet-5");
        assert_eq!(
            cost.min_path_total_usd, 0.0,
            "the task pinned mock explicitly — the override must not reprice it"
        );
    }

    #[test]
    fn breakdown_names_each_reason_not_the_fixed_disjunction() {
        // A priced-but-unbounded task must read « no max_tokens », not
        // « unpriced model » — the operator sees which is FIXABLE.
        let msg = breakdown_of(
            "nika: v1\nworkflow: m\ntasks:\n  \
             - id: a\n    infer: { prompt: hi, model: \"anthropic/claude-sonnet-5\" }\n  \
             - id: b\n    infer: { prompt: hi, max_tokens: 100, model: \"mock/echo\" }\n",
        );
        assert!(msg.contains("2 task(s)"), "{msg}");
        assert!(
            msg.contains("1 with no `max_tokens`"),
            "the priced-unbounded task: {msg}"
        );
        assert!(
            msg.contains("1 on an unpriced model"),
            "the mock task: {msg}"
        );
    }

    #[test]
    fn breakdown_counts_only_the_unbounded_tasks() {
        // A fully-bounded task (priced + max_tokens) is never in the tally.
        // id b carries max_tokens so its reason is NoPrice (mock), not
        // NoTokenLimit — proving the unpriced bucket AND the exclusion of
        // the fully-bounded id a in one shot.
        let msg = breakdown_of(
            "nika: v1\nworkflow: m\ntasks:\n  \
             - id: a\n    infer: { prompt: hi, max_tokens: 100, model: \"anthropic/claude-sonnet-5\" }\n  \
             - id: b\n    infer: { prompt: hi, max_tokens: 100, model: \"mock/echo\" }\n",
        );
        assert!(
            msg.contains("1 task(s)"),
            "only the unpriced mock task: {msg}"
        );
        assert!(msg.contains("unpriced model"), "{msg}");
    }

    #[test]
    fn floor_above_budget_refuses_with_both_numbers() {
        let msg = floor_refusal(0.000_019, 0.000_001).expect("refuses");
        assert!(msg.contains("$0.000019"), "floor rides: {msg}");
        assert!(msg.contains("$0.000001"), "budget rides: {msg}");
        assert!(msg.contains("refusing to start"), "{msg}");
        assert!(msg.contains("nika check"), "points at the envelope: {msg}");
    }

    #[test]
    fn floor_at_or_under_budget_passes() {
        // Spending exactly the budget is not over it (mirrors the
        // ledger's crossing semantics).
        assert!(floor_refusal(0.05, 0.05).is_none());
        assert!(floor_refusal(0.0, 0.05).is_none());
        assert!(floor_refusal(0.0, 0.0).is_none());
    }
}
