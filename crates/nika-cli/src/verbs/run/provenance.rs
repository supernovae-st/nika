// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Registry/cache provenance for `nika run`.

#![allow(clippy::wildcard_imports)]
use super::*;

/// [`run`] keeping registry-cache provenance through the clean-gate.
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
#[must_use]
pub fn run_with_repair_target(
    file: &str,
    json: bool,
    output: Option<&str>,
    theme: Theme,
    mode: RenderMode,
    dry_run: bool,
    model_override: Option<&str>,
    access_pin: Option<&str>,
    vars: &[String],
    resume: Option<&ResumeRequest>,
    no_trace_file: bool,
    task_filter: Option<&str>,
    no_outputs: bool,
    max_cost_usd: Option<f64>,
    no_gc: bool,
    require_signature: bool,
    repair_target: nika_display::check_render::RepairTarget,
) -> u8 {
    run_verdict(
        file,
        json,
        output,
        theme,
        mode,
        dry_run,
        model_override,
        access_pin,
        vars,
        resume,
        no_trace_file,
        task_filter,
        no_outputs,
        max_cost_usd,
        no_gc,
        require_signature,
        false,
        Some(repair_target),
    )
    .code
}

pub(super) fn capture_checked_source(
    file: &str,
    repair_target: Option<nika_display::check_render::RepairTarget>,
    output_json: bool,
) -> Result<(crate::verbs::RunSource, RawWorkflow, CheckReport), Box<RunVerdict>> {
    let source = repair_target
        .map_or_else(
            || crate::verbs::RunSource::capture(file),
            |target| crate::verbs::RunSource::capture_with_repair_target(file, target),
        )
        .map_err(|out| {
            epilogue::emit_diagnostic(&refusal_text(&out), output_json);
            Box::new(RunVerdict::bare(out.code))
        })?;
    let (wf, report) = crate::verbs::load_checked_run_source(&source).map_err(|out| {
        epilogue::emit_diagnostic(&refusal_text(&out), output_json);
        Box::new(RunVerdict::bare(out.code))
    })?;
    Ok((source, wf, report))
}
