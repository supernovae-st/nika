// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `--max-cost-usd` pre-run preflight — refuse BEFORE any spend
//! when the STATIC floor (cheapest path · gates closed · first-try:
//! the unavoidable exposure `nika check` computes) already exceeds the
//! budget · warn loud when the ceiling cannot bound everything (the
//! budget gates METERED spend only — local/mock work never trips it).

use nika_schema::check::CheckReport;

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
        let unbounded = report.cost.tasks.iter().filter(|t| t.usd.is_none()).count();
        eprintln!(
            "⚠ --max-cost-usd {budget}: {unbounded} task(s) have no static ceiling \
             (no token bound · unknown iterations · unpriced model) — the budget \
             bounds METERED spend only; local/mock work never trips it"
        );
    }
    Ok(())
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
    use super::floor_refusal;

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
