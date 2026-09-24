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
        inputs::InputBindings::Operator(vars),
        resume,
        no_trace_file,
        task_filter,
        no_outputs,
        max_cost_usd,
        no_gc,
        require_signature,
        Some(repair_target),
    )
    .code
}

/// Execute with an optional literal JSON-object stdin channel (`Some("-")`).
/// `--var` and workflow-source stdin conflict with that channel; refusals use
/// the run machine envelope before execution. Existing callers can keep [`run`].
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
#[must_use]
pub fn run_with_inputs_json(
    file: &str,
    json: bool,
    output: Option<&str>,
    theme: Theme,
    mode: RenderMode,
    dry_run: bool,
    model_override: Option<&str>,
    access_pin: Option<&str>,
    vars: &[String],
    inputs_json: Option<&str>,
    resume: Option<&ResumeRequest>,
    no_trace_file: bool,
    task_filter: Option<&str>,
    no_outputs: bool,
    max_cost_usd: Option<f64>,
    no_gc: bool,
    require_signature: bool,
    host: impl Into<nika_cli_host::lane::RunHostOptions>,
) -> u8 {
    let host = host.into();
    if host.cost_review_stdio
        && (!json || inputs_json.is_some() || file == "-" || resume.is_some() || dry_run)
    {
        epilogue::emit_diagnostic(
            "Run review requires --json, a file and a fresh invocation without stdin inputs",
            true,
        );
        return exit::ENV;
    }
    let literal = match super::literal_inputs::capture(
        inputs_json,
        vars,
        file,
        json || output == Some("json"),
    ) {
        Ok(value) => value,
        Err(code) => return code,
    };
    let binding = literal.as_ref().map_or(
        inputs::InputBindings::Operator(vars),
        inputs::InputBindings::Literal,
    );
    run_verdict(
        file,
        json,
        output,
        theme,
        mode,
        dry_run,
        model_override,
        access_pin,
        binding,
        resume,
        no_trace_file,
        task_filter,
        no_outputs,
        max_cost_usd,
        no_gc,
        require_signature,
        host,
    )
    .code
}

pub(super) fn capture_checked_source(
    file: &str,
    repair_target: Option<nika_display::check_render::RepairTarget>,
    (output_json, json): (bool, bool),
) -> Result<(crate::verbs::RunSource, RawWorkflow, CheckReport), Box<RunVerdict>> {
    let source = repair_target
        .map_or_else(
            || crate::verbs::RunSource::capture(file),
            |target| crate::verbs::RunSource::capture_with_repair_target(file, target),
        )
        .map_err(|out| refuse_source(&out, output_json, json))?;
    let (wf, report) = crate::verbs::load_checked_run_source(&source)
        .map_err(|out| refuse_source(&out, output_json, json))?;
    Ok((source, wf, report))
}

/// The source refusal on the right lane: under `--json` (the frames
/// lane) ONE JSON object on stdout — the same `parse_fatal` shape `check
/// --json` prints — so a CI reader parses the exit-3 lane on both verbs
/// (W3-F10); the human and `--output json` lanes keep their voices.
fn refuse_source(out: &crate::verbs::VerbOutput, output_json: bool, json: bool) -> Box<RunVerdict> {
    if json {
        epilogue::emit_check_refusal(&crate::verbs::check::parse_fatal_json(out).text);
    } else {
        epilogue::emit_diagnostic(&refusal_text(out), output_json);
    }
    Box::new(RunVerdict::bare(out.code))
}

#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
#[must_use]
pub(crate) fn run_verdict(
    file: &str,
    json: bool,
    output: Option<&str>,
    theme: Theme,
    mode: RenderMode,
    dry_run: bool,
    model_override: Option<&str>,
    access_pin: Option<&str>,
    binding: inputs::InputBindings<'_>,
    resume: Option<&ResumeRequest>,
    no_trace_file: bool,
    task_filter: Option<&str>,
    no_outputs: bool,
    max_cost_usd: Option<f64>,
    no_gc: bool,
    require_signature: bool,
    host: impl Into<nika_cli_host::lane::RunHostOptions>,
) -> RunVerdict {
    let host = host.into();
    let invocation_cost = max_cost_usd;
    let (output_json, max_cost_usd) = match preflight(output, json, max_cost_usd, no_gc, dry_run) {
        Ok(pair) => pair,
        Err(verdict) => return *verdict,
    };
    let (source, wf, report) =
        match provenance::capture_checked_source(file, host.repair_target, (output_json, json)) {
            Ok(checked) => checked,
            Err(verdict) => return *verdict,
        };
    let file_owned = source.logical_path().to_owned();
    let file = file_owned.as_str();
    if require_signature && let Err(code) = require_signature_gate(&source, output_json || json) {
        return RunVerdict::bare(code);
    }
    let (_wf, _report, _skills) = match scoped_clean_gate(
        wf,
        report,
        task_filter,
        &source,
        json,
        theme,
        (output_json, model_override),
    ) {
        Ok(triple) => triple,
        Err(code) => return RunVerdict::bare(code),
    };
    run_admitted(
        file,
        &source,
        (json, output_json),
        theme,
        mode,
        dry_run,
        model_override,
        access_pin,
        binding,
        resume,
        no_trace_file,
        task_filter,
        no_outputs,
        max_cost_usd,
        invocation_cost,
        host.cost_review_stdio,
    )
}
