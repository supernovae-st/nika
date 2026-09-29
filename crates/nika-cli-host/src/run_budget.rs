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

// Host interaction/protocol projection is this module's effect boundary.
#![allow(clippy::disallowed_macros, clippy::print_stdout, clippy::print_stderr)]

use std::collections::BTreeMap;

use nika_check::{CheckReport, CostCeiling};
use nika_schema::raw::RawWorkflow;
use serde_json::Value;

use crate::output::exit;

/// Gate the run on the operator budget. `Err(exit_code)` = refuse
/// (exit 2 · nothing was spent) · `Ok(())` = proceed (possibly after
/// the loud unbounded warning on stderr).
///
/// `model_override` — the floor must price the EFFECTIVE model (#342):
/// `--model m` replaces the envelope default at runtime, and the
/// override form IS the delegation idiom agent surfaces teach — a gate
/// that prices the file's model while the run uses another never fires
/// for exactly that population. This form binds no inputs; a host that
/// holds the invocation's validated bindings uses [`preflight_bound`].
///
/// # Errors
/// The unchanged FILE exit code when the static monetary floor refuses.
pub fn preflight(
    wf: &RawWorkflow,
    report: &CheckReport,
    model_override: Option<&str>,
    max_cost_usd: Option<f64>,
    output_json: bool,
    seated_on_harness: bool,
) -> Result<(), u8> {
    let unbound = BTreeMap::new();
    preflight_bound(
        wf,
        report,
        (model_override, &unbound),
        max_cost_usd,
        output_json,
        seated_on_harness,
    )
}

/// The cost gate over a run's frozen plan and its validated bindings (B11 ·
/// descended from the CLI's run adapter): a run every admitted lane of which
/// sits on a harness (`--access codex` and its kin) is bounded by the seat's
/// subscription, so the cap gates the priced builtins only.
///
/// # Errors
/// As [`preflight_bound`].
pub fn plan_gate(
    wf: &RawWorkflow,
    report: &CheckReport,
    effective: (Option<&str>, &BTreeMap<String, Value>),
    max_cost_usd: Option<f64>,
    output_json: bool,
    plan: &nika_providers::ExecutionAccessPlan,
) -> Result<(), u8> {
    let seated_on_harness = plan.admitted().next().is_some()
        && plan
            .admitted()
            .all(|(_, lane)| matches!(lane.plan.chosen, nika_types::access::AccessClass::Harness));
    preflight_bound(
        wf,
        report,
        effective,
        max_cost_usd,
        output_json,
        seated_on_harness,
    )
}

/// [`preflight`] over the workflow as the run binds it (B11): the
/// `(model_override, bindings)` pair is `--model` and the invocation's
/// validated input values, which the runtime's ONE floor law seats before it
/// prices (`nika_runtime::budget_floor_refusal_bound`), so a fan over a bound
/// input is priced at the items given, never at its declared default. The
/// unbounded warning describes that same effective workflow.
///
/// # Errors
/// The unchanged FILE exit code when the static monetary floor refuses.
pub fn preflight_bound(
    wf: &RawWorkflow,
    report: &CheckReport,
    (model_override, bindings): (Option<&str>, &BTreeMap<String, Value>),
    max_cost_usd: Option<f64>,
    output_json: bool,
    seated_on_harness: bool,
) -> Result<(), u8> {
    let Some(budget) = max_cost_usd else {
        return Ok(());
    };
    let effective = effective_cost(wf, model_override, bindings);
    let cost = effective.as_ref().unwrap_or(&report.cost);
    if let Some(err) = nika_runtime::budget_floor_refusal_bound(
        wf,
        report,
        Some(budget),
        model_override,
        bindings,
        seated_on_harness,
    ) {
        crate::run_protocol::emit_diagnostic(&err.to_string(), output_json);
        return Err(exit::FILE);
    }
    if seated_on_harness && !output_json {
        eprintln!(
            "⚠ --max-cost-usd {budget}: the run is seated on a harness — its subscription \
             bounds the seat's own spend; the cap meters the priced builtins only"
        );
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

/// The cost envelope of the workflow as the run seats it: the CLI `--model`
/// in the envelope (per-task `model:` keeps winning) and the invocation's
/// bindings, through the runtime's ONE resolver. `None` when that is the file.
fn effective_cost(
    wf: &RawWorkflow,
    model_override: Option<&str>,
    bindings: &BTreeMap<String, Value>,
) -> Option<CostCeiling> {
    nika_runtime::effective_workflow(wf, model_override, bindings)
        .map(|seated| nika_check::check(&seated).cost)
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
    use std::collections::BTreeMap;

    use nika_schema::{FileId, ParseMode, parse};
    use serde_json::{Value, json};

    use super::{effective_cost, preflight, preflight_bound};

    #[test]
    fn a_run_seated_on_a_harness_passes_the_cap_with_an_unpriced_cloud_model() {
        // `--access codex` with a model the catalog cannot price: refused unseated, admitted
        // seated (the subscription bounds the seat; the cap meters priced builtins only).
        // Use the runtime's deliberately unpriced canary: a real model may gain a price.
        let yaml = "nika: m\nmodel: \"gemini/nika-b20-unpriced-canary\"\ntasks:\n  \
             a:\n    infer: { prompt: hi, max_tokens: 20 }\n";
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
        let report = nika_check::check(&wf);
        assert_eq!(
            preflight(&wf, &report, None, Some(0.05), false, false),
            Err(crate::output::exit::FILE),
            "unseated, an unpriced cloud model under a cap refuses"
        );
        assert_eq!(
            preflight(&wf, &report, None, Some(0.05), false, true),
            Ok(()),
            "seated on a harness, the same run proceeds"
        );
    }

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
            false,
        );
        assert_eq!(
            refused,
            Err(crate::output::exit::FILE),
            "the effective (overridden) model's floor must trip the gate"
        );
        // The exact same call WITHOUT the override passes — the file's
        // mock floor is zero (the pre-#342 behavior, still correct there).
        assert_eq!(
            preflight(&wf, &report, None, Some(0.000_001), false, false),
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
            preflight(
                &wf,
                &report,
                Some("mock/echo"),
                Some(0.000_001),
                false,
                false
            ),
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
        let cost = effective_cost(&wf, Some("anthropic/claude-sonnet-5"), &BTreeMap::new())
            .expect("the override seats a copy");
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
            preflight(&wf, &report, None, Some(0.001), false, false),
            Err(crate::output::exit::FILE),
            "xAI image floor $0.02 refuses cap $0.001 before any HTTP"
        );
        assert_eq!(
            preflight(&wf, &report, None, Some(1.00), false, false),
            Ok(()),
            "cap 1.00 admits the $0.02 floor"
        );
    }

    /// A fan over `inputs.xs` declaring `default`, each item one paid call.
    fn input_fan(default: &str) -> nika_schema::raw::RawWorkflow {
        let yaml = format!(
            "nika: fan\ninputs:\n  xs: {{ type: {{ array: string }}, required: false, default: {default} }}\ntasks:\n  ask:\n    for_each: {{ items: \"${{{{ inputs.xs }}}}\" }}\n    infer: {{ prompt: hi, max_tokens: 512, model: \"deepseek/deepseek-v4-pro\" }}\n"
        );
        parse(&yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses")
    }

    /// `preflight_bound` under `cap` with `xs` bound to `value` (unbound when `None`).
    fn gate(wf: &nika_schema::raw::RawWorkflow, value: Option<Value>, cap: f64) -> Result<(), u8> {
        let bindings: BTreeMap<String, Value> =
            value.map(|v| ("xs".to_owned(), v)).into_iter().collect();
        let report = nika_check::check(wf);
        preflight_bound(wf, &report, (None, &bindings), Some(cap), false, false)
    }

    /// B11 · B1: the Host preflight prices the items the invocation gives, not the declared
    /// default: one given over five fits where five alone refuse, five given over one refuse
    /// where one alone fits, and the unbound form keeps pricing the default.
    #[test]
    fn a_bound_fan_is_priced_at_the_items_given() {
        let (one, five) = (
            input_fan("[\"a\"]"),
            input_fan("[\"a\", \"b\", \"c\", \"d\", \"e\"]"),
        );
        let per_call = nika_check::check(&one).cost.min_path_total_usd;
        let cap = 3.0 * per_call;
        let refused = Err(crate::output::exit::FILE);
        assert_eq!(
            gate(&five, None, cap),
            refused,
            "the five-item default refuses"
        );
        assert_eq!(
            gate(&five, Some(json!(["a"])), cap),
            Ok(()),
            "one given fits"
        );
        assert_eq!(gate(&one, None, cap), Ok(()), "the one-item default fits");
        let given = json!(["a", "b", "c", "d", "e"]);
        assert_eq!(gate(&one, Some(given), cap), refused, "five given refuse");
        let report = nika_check::check(&five);
        assert_eq!(
            preflight(&five, &report, None, Some(cap), false, false),
            refused,
            "the unbound form still prices the default"
        );
    }

    /// A bound value that is not a list never falls back to the declared default, and an
    /// explicit empty list is zero calls, even under an explicit zero cap.
    #[test]
    fn a_bound_non_list_or_empty_list_never_prices_the_default() {
        let five = input_fan("[\"a\", \"b\", \"c\", \"d\", \"e\"]");
        let per_call = nika_check::check(&five).cost.min_path_total_usd / 5.0;
        assert_eq!(gate(&five, Some(json!("a")), 3.0 * per_call), Ok(()));
        assert_eq!(
            gate(&five, Some(json!([])), 0.0),
            Ok(()),
            "zero items, zero floor"
        );
        assert_eq!(
            gate(&five, None, 0.0),
            Err(crate::output::exit::FILE),
            "the default under a zero cap refuses"
        );
    }

    /// The bound path keeps the harness exemption and the priced-builtin floor.
    #[test]
    fn the_bound_path_keeps_the_harness_and_builtin_controls() {
        let yaml = "nika: m\nmodel: \"gemini/nika-b20-unpriced-canary\"\ntasks:\n  \
             a:\n    infer: { prompt: hi, max_tokens: 20 }\n";
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
        let report = nika_check::check(&wf);
        let bindings = BTreeMap::new();
        let bound =
            |seated| preflight_bound(&wf, &report, (None, &bindings), Some(0.05), false, seated);
        assert_eq!(bound(false), Err(crate::output::exit::FILE));
        assert_eq!(bound(true), Ok(()), "seated on a harness, the run proceeds");
        let wf = parse(
            &image_generate_yaml("xai"),
            FileId::new(0),
            ParseMode::Strict,
        )
        .expect("fixture parses");
        let report = nika_check::check(&wf);
        assert_eq!(
            preflight_bound(&wf, &report, (None, &bindings), Some(0.001), false, false),
            Err(crate::output::exit::FILE),
            "the xAI image floor still refuses a tiny cap"
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
            preflight(&wf, &report, None, Some(0.001), false, false),
            Ok(()),
            "mock image is unpriced — rehearsal under a tight cap stays legal"
        );
    }
}
