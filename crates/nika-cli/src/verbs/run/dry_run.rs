// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `--dry-run` lane: model swap, refusal, then one honest preview.

use nika_check::CheckReport;
use nika_schema::raw::RawWorkflow;

use super::{RunVerdict, exit};
use crate::Theme;

/// Apply `--model` and re-check the envelope the preview will describe.
pub(super) fn swap(
    wf: &RawWorkflow,
    model: &str,
) -> Result<(RawWorkflow, CheckReport), Box<(RawWorkflow, CheckReport)>> {
    let swapped = crate::verbs::with_model_override(wf, model);
    let mut report = nika_check::check(&swapped);
    crate::verbs::stamp_judged_semantic(&swapped, &mut report);
    let models = crate::verbs::check::models_rung::unresolvable_models(&report, &swapped);
    if report.is_clean() && models.findings.is_empty() {
        Ok((swapped, report))
    } else {
        Err(Box::new((swapped, report)))
    }
}

/// Preview the overridden pair, or emit the same refusal as `nika check`.
#[allow(clippy::fn_params_excessive_bools, clippy::too_many_arguments)]
pub(super) fn lane(
    file: &str,
    source: &str,
    wf: &RawWorkflow,
    report: &CheckReport,
    skills: &nika_schema::ResolvedSkills,
    repair_target: nika_display::check_render::RepairTarget,
    model_override: Option<&str>,
    json: bool,
    theme: Theme,
    output_json: bool,
) -> RunVerdict {
    if let Some(model) = model_override {
        match swap(wf, model) {
            Ok((wf, report)) => {
                return RunVerdict::bare(verdict(file, &wf, &report, json, theme));
            }
            Err(refused) => {
                let (wf, report) = *refused;
                let out = crate::verbs::check::run_admitted_pair(
                    source,
                    file,
                    repair_target,
                    &wf,
                    &report,
                    skills,
                    json,
                    theme,
                );
                super::epilogue::emit_diagnostic(&out.text, output_json);
                return RunVerdict::bare(out.code);
            }
        }
    }
    RunVerdict::bare(verdict(file, wf, report, json, theme))
}

fn verdict(file: &str, wf: &RawWorkflow, report: &CheckReport, json: bool, theme: Theme) -> u8 {
    if json {
        println!("{:#}", nika_check::plan::payload(file, wf, report));
    } else {
        let plan = crate::verbs::inspect::render_pair(wf, report, theme);
        if !plan.text.is_empty() {
            println!("{}", plan.text.trim_end());
        }
        println!("\n  dry-run · plan only · no effects executed");
    }
    exit::OK
}
