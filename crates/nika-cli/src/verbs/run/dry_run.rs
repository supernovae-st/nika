// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `--dry-run` lane (spec §10 · "plan only · zero effects") — the
//! seat swap, then the preview: the human card or the `plan_version: 1`
//! object, both projected from the SAME pair.
//!
//! Split out of `run/mod.rs` for #1051. The lane is a coherent unit and
//! `run_verdict` sat at exactly 100/100 lines with `mod.rs` at 1461/1500,
//! so the fix had nowhere to live in place.

use nika_check::CheckReport;
use nika_schema::raw::RawWorkflow;

use super::{RunVerdict, exit};
use crate::Theme;

/// The preview describes the run that WOULD happen, so `--model` has to
/// reach it. Mirrors [`super::apply_task_scope`]: the transform
/// RE-CHECKS, because a workflow whose seat was swapped has a different
/// price, a different provider, and — when that provider does not
/// resolve in this binary — a different verdict.
///
/// Measured on 0.111.0 before this fix, against `nika new
/// snippets/hello-ai` unedited: `run --dry-run --model nonexistent/model`
/// printed the FILE's `ollama/llama3.2:3b` at exit 0, while `check
/// --model nonexistent/model` refused the same swap at exit 2. The
/// preview was the only one of the three surfaces that could not see the
/// flag at all, and the `plan_version: 1` object carried the wrong
/// `model` too — a machine reading the plan was misled, not only a
/// reader.
///
/// `Err` carries the SWAPPED report: the swap is what failed, so a card
/// built from the file would go green for a seat nobody asked for.
pub(super) fn swap(
    wf: &RawWorkflow,
    model: &str,
) -> Result<(RawWorkflow, CheckReport), Box<CheckReport>> {
    let swapped = crate::verbs::with_model_override(wf, model);
    let mut report = nika_check::check(&swapped);
    // F-P2 · the report is judged over the SWAPPED envelope · stamp its
    // semantic hash so the binding stays honest on this lane too.
    crate::verbs::stamp_judged_semantic(&swapped, &mut report);
    // The verdict is COMPOSED exactly as the check lane composes it:
    // `is_clean()` is the narrow criterion and MODELS sits outside it, so
    // deciding on it alone would wave through the very swap this exists
    // to catch (the P0-11 shape — `✔ audited` printed under a `✖ MODELS`
    // rung). `skills:` needs no re-run: a seat swap cannot move them, and
    // `scoped_clean_gate` already gated them upstream.
    let models = crate::verbs::check::models_rung::unresolvable_models(&report, &swapped);
    if report.is_clean() && models.findings.is_empty() {
        Ok((swapped, report))
    } else {
        Err(Box::new(report))
    }
}

/// The whole lane: swap the seat when asked, then preview or refuse.
#[allow(clippy::fn_params_excessive_bools)]
pub(super) fn lane(
    file: &str,
    wf: &RawWorkflow,
    report: &CheckReport,
    model_override: Option<&str>,
    json: bool,
    theme: Theme,
    output_json: bool,
) -> RunVerdict {
    let Some(model) = model_override else {
        return RunVerdict::bare(verdict(file, wf, report, json, theme));
    };
    let Ok((wf, report)) = swap(wf, model) else {
        let out = crate::verbs::check::run(file, json, false, model_override, theme);
        super::epilogue::emit_diagnostic(&out.text, output_json);
        return RunVerdict::bare(out.code);
    };
    RunVerdict::bare(verdict(file, &wf, &report, json, theme))
}

/// The dry-run fork: the human preview, or the #332 machine plan.
fn verdict(file: &str, wf: &RawWorkflow, report: &CheckReport, json: bool, theme: Theme) -> u8 {
    if json {
        emit_json(file, wf, report)
    } else {
        render(wf, report, theme)
    }
}

/// `--dry-run` (spec §10 · "plan only · zero effects"): the audit passed
/// — render the static plan (the same anatomy `nika inspect` renders)
/// without composing any production seam. No fs, no http, no subprocess,
/// no provider call — the run is never reached.
///
/// Renders the pair it is HANDED (#1051): it used to call
/// `inspect::run(file, …)`, which re-read and re-checked the file, so the
/// card described the file while the JSON described the swap.
fn render(wf: &RawWorkflow, report: &CheckReport, theme: Theme) -> u8 {
    let plan = crate::verbs::inspect::render_pair(wf, report, theme);
    if !plan.text.is_empty() {
        println!("{}", plan.text.trim_end());
    }
    println!("\n  dry-run · plan only · no effects executed");
    exit::OK
}

/// `--dry-run --json` (#332): ONE versioned plan object on stdout — what
/// the run WOULD do, projected from the SAME report the audit already
/// computed (waves resolved to task ids · per-task verb/model · the cost
/// ceiling · the affirmative permits · the caller requirements). CI and
/// PR renderers read this instead of composing `check --json` +
/// `explain --json` and reconstructing the plan client-side.
/// `plan_version` follows the check-report discipline: additive keys
/// never bump it.
fn emit_json(file: &str, wf: &RawWorkflow, report: &CheckReport) -> u8 {
    println!("{:#}", payload(file, wf, report));
    exit::OK
}

/// The pure projection behind [`emit_json`] (unit-pinned): waves resolved
/// from indices to task ids, one `{id, verb}` row per task, and the
/// report's own cost/permits/requirements objects verbatim.
pub(super) fn payload(file: &str, wf: &RawWorkflow, report: &CheckReport) -> serde_json::Value {
    let ids: Vec<&str> = wf.tasks.iter().map(|t| t.value.id.value.as_str()).collect();
    let waves: Vec<Vec<&str>> = report
        .waves
        .iter()
        .map(|w| w.iter().filter_map(|&i| ids.get(i).copied()).collect())
        .collect();
    let tasks: Vec<serde_json::Value> = wf
        .tasks
        .iter()
        .map(|t| {
            serde_json::json!({
                "id": t.value.id.value,
                "verb": t.value.action.verb(),
            })
        })
        .collect();
    serde_json::json!({
        "plan_version": 1,
        "workflow": wf.workflow.as_ref().map(|w| w.value.as_str()),
        "file": file,
        "dry_run": true,
        "effects_executed": false,
        "waves": waves,
        "tasks": tasks,
        "cost": report.cost,
        "permits": report.permits,
        "requirements": report.requirements,
    })
}
