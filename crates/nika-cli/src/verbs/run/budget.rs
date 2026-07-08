// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `--max-cost-usd` pre-run preflight — refuse BEFORE any spend
//! when the STATIC floor (cheapest path · gates closed · first-try:
//! the unavoidable exposure `nika check` computes) already exceeds the
//! budget · warn loud when the ceiling cannot bound everything (the
//! budget gates METERED spend only — local/mock work never trips it).

use nika_schema::check::{CheckReport, CostCeiling, UnboundedReason};

use crate::verbs::exit;

/// Gate the run on the operator budget. `Err(exit_code)` = refuse
/// (exit 2 · nothing was spent) · `Ok(())` = proceed (possibly after
/// the loud unbounded warning on stderr).
pub(super) fn preflight(
    report: &CheckReport,
    max_cost_usd: Option<f64>,
    output_json: bool,
) -> Result<(), u8> {
    let Some(budget) = max_cost_usd else {
        return Ok(());
    };
    if let Some(refusal) = floor_refusal(report.cost.min_path_total_usd, budget) {
        super::emit_diagnostic(&refusal, output_json);
        return Err(exit::FILE);
    }
    if report.cost.has_unbounded {
        eprintln!(
            "⚠ --max-cost-usd {budget}: {} — the budget bounds METERED spend \
             only; local/mock work never trips it",
            unbounded_breakdown(&report.cost)
        );
    }
    Ok(())
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
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use nika_schema::{FileId, ParseMode, parse};

    use super::{floor_refusal, unbounded_breakdown};

    fn breakdown_of(yaml: &str) -> String {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
        unbounded_breakdown(&nika_schema::check::check(&wf).cost)
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
