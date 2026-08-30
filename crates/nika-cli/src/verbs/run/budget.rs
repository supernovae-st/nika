// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `--max-cost-usd` pre-run preflight — refuse BEFORE any spend
//! when the STATIC floor (cheapest path · gates closed · first-try:
//! the unavoidable exposure `nika check` computes) already exceeds the
//! budget · warn loud when the ceiling cannot bound everything (the
//! budget gates METERED spend only — local/mock work never trips it).
//!
//! The gate's pure mechanics — the floor refusal + the unbounded-reason
//! tally — descended to [`nika_runtime`] 2026-07-22 (the launch-gate
//! family beside `required_inputs_refusal`); this module is the
//! operator-facing surface (texts · streams · exit codes).

use nika_check::{CheckReport, CostCeiling};
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
    if let Some(err) = nika_runtime::budget_floor_refusal(wf, report, Some(budget), model_override)
    {
        super::epilogue::emit_diagnostic(&err.to_string(), output_json);
        return Err(exit::FILE);
    }
    if cost.has_unbounded {
        eprintln!(
            "⚠ --max-cost-usd {budget}: {} — the budget bounds METERED spend \
             only; local/mock work never trips it",
            nika_runtime::unbounded_breakdown(cost)
        );
    }
    Ok(())
}

/// The cost envelope with the CLI `--model` substituted for the envelope
/// default (per-task `model:` overrides keep winning, mirroring the
/// runtime's precedence).
fn effective_cost(wf: &RawWorkflow, model_override: &str) -> CostCeiling {
    nika_check::check(&crate::verbs::with_model_override(wf, model_override)).cost
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

    use super::{effective_cost, preflight};

    /// #342 — the delegation idiom (`--model <p/m> --max-cost-usd <usd>`):
    /// the file says mock (floor $0, would pass); the OVERRIDE is a priced
    /// model whose bounded floor exceeds the budget — the gate must refuse
    /// BEFORE any spend, exactly like the in-file form.
    #[test]
    fn override_prices_the_effective_model_and_refuses_at_the_gate() {
        let yaml = "nika: m\ntasks:\n  \
             a:\n    infer: { prompt: hi, max_tokens: 1000000, model: \"mock/echo\" }\n";
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
        let report = nika_check::check(&wf);
        assert_eq!(
            report.cost.min_path_total_usd, 0.0,
            "the FILE's floor is zero — the un-overridden gate would pass"
        );
        // Task-level model wins over the envelope — the override swaps the
        // ENVELOPE default, so the fixture's model must live there instead.
        let yaml = "nika: m\nmodel: \"mock/echo\"\ntasks:\n  \
             a:\n    infer: { prompt: hi, max_tokens: 1000000 }\n";
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
        let report = nika_check::check(&wf);
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
        let yaml = "nika: m\nmodel: \"anthropic/claude-sonnet-5\"\ntasks:\n  \
             a:\n    infer: { prompt: hi, max_tokens: 1000000 }\n";
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
        let report = nika_check::check(&wf);
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
        let yaml = "nika: m\nmodel: \"mock/echo\"\ntasks:\n  \
             a:\n    infer: { prompt: hi, max_tokens: 1000000, model: \"mock/echo\" }\n";
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
        let cost = effective_cost(&wf, "anthropic/claude-sonnet-5");
        assert_eq!(
            cost.min_path_total_usd, 0.0,
            "the task pinned mock explicitly — the override must not reprice it"
        );
    }

    fn image_generate_yaml(provider: &str) -> String {
        format!(
            "nika: b24\npermits: {{ tools: [\"nika:image_generate\"], fs: {{ write: [\"./out/**\"] }} }}\ntasks:\n  og:\n    invoke: {{ tool: \"nika:image_generate\", args: {{ provider: {provider}, prompt: \"a monarch butterfly\", output_dir: \"./out\" }} }}\n"
        )
    }

    /// B24 / issue 1296: a priced builtin whose catalog floor already
    /// exceeds `--max-cost-usd` must refuse at the CLI preflight — the
    /// same constructor the runtime admission gate speaks.
    #[test]
    fn priced_image_builtin_refuses_a_tiny_cap_and_passes_a_generous_one() {
        let wf = parse(
            &image_generate_yaml("xai"),
            FileId::new(0),
            ParseMode::Strict,
        )
        .expect("fixture parses");
        let report = nika_check::check(&wf);
        assert!(report.is_clean(), "fixture checks clean: {report:?}");
        assert_eq!(
            preflight(&wf, &report, None, Some(0.001), false),
            Err(crate::verbs::exit::FILE),
            "xAI image floor $0.02 refuses cap $0.001 before any HTTP"
        );
        assert_eq!(
            preflight(&wf, &report, None, Some(1.00), false),
            Ok(()),
            "cap 1.00 admits the $0.02 floor"
        );
    }

    #[test]
    fn mock_image_builtin_passes_a_tiny_cap() {
        let wf = parse(
            &image_generate_yaml("mock"),
            FileId::new(0),
            ParseMode::Strict,
        )
        .expect("fixture parses");
        let report = nika_check::check(&wf);
        assert_eq!(
            preflight(&wf, &report, None, Some(0.001), false),
            Ok(()),
            "mock image is unpriced — rehearsal under a tight cap stays legal"
        );
    }
}
