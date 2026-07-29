// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `nika inspect` energy aggregate (NEP-0018 §4 — « the MARKERS are
//! the contract, not the view name »). Consumes the report's OWN energy
//! reading (`nika_check::energy` · one classification, both surfaces —
//! the check ladder's rung renders the same reading in
//! `nika_display::check_render`). Same four words on both: floor ·
//! ceiling · UNBOUNDED · unpriced — and never `0 Wh`.

use nika_check::CheckReport;

use crate::display::vocab::at_most;

/// `≤ N Wh` at a ceiling-honest display grain: a tiny bound rounds UP
/// to 0.001 — this fragment never prints `0.0 Wh` (a zero would claim
/// free inference · NEP-0018 « unknown stays unknown »).
fn fmt_wh(wh: f64) -> String {
    if wh >= 1.0 {
        format!("{wh:.1}")
    } else {
        format!("{:.3}", (wh * 1000.0).ceil() / 1000.0)
    }
}

/// One class → `≤ X Wh` · several → `gpu ≤ X Wh · fleet ≤ Y Wh`, each
/// subtotal wearing its class (watt-hours sum WITHIN a scope class and
/// never across one — the reading's own partition). The comparison mark
/// rides the vocab seam (`≤` · `<=` under `--ascii`).
fn fmt_scope_totals(subs: &[(String, f64)], ascii: bool) -> String {
    let le = at_most(ascii);
    match subs {
        [] => String::new(),
        [(_, wh)] => format!("{le} {} Wh", fmt_wh(*wh)),
        many => many
            .iter()
            .map(|(scope, wh)| format!("{scope} {le} {} Wh", fmt_wh(*wh)))
            .collect::<Vec<_>>()
            .join(" · "),
    }
}

/// The fragment's totals: the per-class sums, with a single class named
/// beside its number (`≤ 0.087 Wh (gpu)`) — the rung puts the class on
/// its count line, the one-line fragment has nowhere else to put it.
fn scoped_totals(subs: &[(String, f64)], ascii: bool) -> String {
    match subs {
        [(scope, _)] => format!("{} ({scope})", fmt_scope_totals(subs, ascii)),
        _ => fmt_scope_totals(subs, ascii),
    }
}

/// The `nika inspect` energy aggregate: one compact fragment for the
/// anatomy header, same markers as the check rung — a per-scope ceiling
/// qualified by its coverage, `unbounded` with the bounded portion
/// named, `unpriced` for a figure that does not exist (never `0 Wh`),
/// and « nothing to bound » for a workflow whose tasks provably never
/// run. `None` when the workflow has no inference task (the cost
/// fragment already says so).
pub(crate) fn inspect_fragment(report: &CheckReport, ascii: bool) -> Option<String> {
    if report.cost.tasks.is_empty() {
        return None;
    }
    let e = &report.energy;
    let n = &e.counts;
    let totals = scoped_totals(&e.scope_subtotals, ascii);
    if n.uncapped > 0 {
        return Some(if e.tasks.is_empty() {
            "energy unbounded".to_owned()
        } else {
            format!("energy unbounded · bounded {totals}")
        });
    }
    if e.tasks.is_empty() {
        return Some(if n.unpriced == 0 && n.never_runs > 0 {
            "no energy to bound (no task runs)".to_owned()
        } else {
            "energy unpriced".to_owned()
        });
    }
    // A ceiling that covers PART of the tasks must say so — an
    // unqualified number reads as the workflow's ceiling (the same
    // « N of M tasks measured » the rung's headline carries).
    if e.tasks.len() < n.total {
        Some(format!(
            "{totals} ({} of {} measured)",
            e.tasks.len(),
            n.total
        ))
    } else {
        Some(totals)
    }
}

#[cfg(test)]
mod fragment_tests {
    use super::inspect_fragment;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn report_of(yaml: &str) -> nika_check::CheckReport {
        nika_check::check(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    /// No inference task → no fragment (the cost fragment already says
    /// « no infer/agent tasks » — nothing to double).
    #[test]
    fn no_inference_tasks_yield_no_fragment() {
        let r = report_of(
            "nika: v1\nworkflow:\n  id: w\ntasks:\n  sh:\n    exec: { command: [\"true\"] }\n",
        );
        assert_eq!(inspect_fragment(&r, false), None);
    }

    /// A capped task on a measured model → a scoped ceiling, full
    /// coverage → no fraction qualifier.
    #[test]
    fn a_measured_capped_task_states_a_scoped_ceiling() {
        let r = report_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: groq/qwen/qwen3-32b\ntasks:\n  t:\n    infer: { prompt: \"x\", max_tokens: 1000 }\n",
        );
        let frag = inspect_fragment(&r, false).expect("fragment");
        assert!(
            frag.starts_with("≤ ") && frag.ends_with(" Wh (gpu)"),
            "scoped ceiling: {frag}"
        );
        assert!(
            !frag.contains("measured"),
            "full coverage is unqualified: {frag}"
        );
        let ascii = inspect_fragment(&r, true).expect("fragment");
        assert!(
            ascii.starts_with("<= ") && !ascii.contains('≤'),
            "ascii parity: {ascii}"
        );
    }

    /// Partial coverage must be QUALIFIED — an unqualified number reads
    /// as the workflow's ceiling while a task's figure is missing.
    #[test]
    fn partial_coverage_names_its_fraction() {
        let r = report_of(
            "nika: v1\nworkflow:\n  id: w\ntasks:\n  a:\n    infer: { prompt: \"x\", max_tokens: 1000, model: groq/qwen/qwen3-32b }\n  b:\n    infer: { prompt: \"x\", max_tokens: 1000, model: anthropic/claude-sonnet-4-6 }\n",
        );
        let frag = inspect_fragment(&r, false).expect("fragment");
        assert!(
            frag.contains("(1 of 2 measured)"),
            "the fraction rides the fragment: {frag}"
        );
    }

    /// An uncapped task → no total ceiling, and the string `0 Wh`
    /// appears nowhere (NEP-0018 conformance shape #3).
    #[test]
    fn an_uncapped_task_is_unbounded_never_zero() {
        let r = report_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: groq/qwen/qwen3-32b\ntasks:\n  t:\n    infer: { prompt: \"x\" }\n",
        );
        let frag = inspect_fragment(&r, false).expect("fragment");
        assert_eq!(frag, "energy unbounded");
        assert!(!frag.contains("0 Wh"));
    }

    /// Uncapped beside a measured task → the bounded portion is named
    /// (the marker set: unbounded, with what IS provable shown).
    #[test]
    fn unbounded_with_a_measured_task_names_the_bounded_portion() {
        let r = report_of(
            "nika: v1\nworkflow:\n  id: w\ntasks:\n  a:\n    infer: { prompt: \"x\", max_tokens: 1000, model: groq/qwen/qwen3-32b }\n  b:\n    infer: { prompt: \"x\", model: groq/qwen/qwen3-32b }\n",
        );
        let frag = inspect_fragment(&r, false).expect("fragment");
        assert!(
            frag.starts_with("energy unbounded · bounded ≤ ") && frag.ends_with(" Wh (gpu)"),
            "bounded portion named: {frag}"
        );
    }

    /// An unmeasured model → `unpriced`, never `0 Wh` (NEP-0018
    /// conformance shape #2).
    #[test]
    fn an_unmeasured_model_is_unpriced_never_zero() {
        let r = report_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  t:\n    infer: { prompt: \"x\", max_tokens: 1000 }\n",
        );
        let frag = inspect_fragment(&r, false).expect("fragment");
        assert_eq!(frag, "energy unpriced");
        assert!(!frag.contains("0 Wh"));
    }

    /// A workflow whose only task provably never runs (literal empty
    /// `for_each`) has nothing to bound — a claim of absence, not a
    /// `0 Wh` figure.
    #[test]
    fn a_never_running_workflow_has_nothing_to_bound() {
        let r = report_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: groq/qwen/qwen3-32b\ntasks:\n  t:\n    for_each: []\n    infer: { prompt: \"x ${{ item }}\", max_tokens: 1000 }\n",
        );
        let frag = inspect_fragment(&r, false).expect("fragment");
        assert_eq!(frag, "no energy to bound (no task runs)");
    }
}
