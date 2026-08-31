// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika run <file>` — execute a CHECKED workflow through the real L3
//! runtime (spec §3 · exit 0 ok · 1 workflow failed · 2 file findings).
//!
//! The gauntlet + the lanes: this module is the L4 ORCHESTRATION half —
//! audit-before-run, the gates, the exit codes, the live fold. The
//! production composition (real fs · http · clock · subprocess ·
//! provider registry with env-resolved keys) descended to
//! [`nika_runtime::compose`] 2026-07-22 (compute descends, render
//! stays); the journal's write half lives in [`nika_dap::journal`].
//!
//! ```text
//! parse (Strict) → check → dirty? → render findings · exit 2
//!                        → clean  → runtime.run(prod seams) →
//!                  outcome.ok → exit 0 | exit 1 (the failure card)
//! ```

// Unlike the static verbs (which return a `VerbOutput` for `main` to
// emit), `run` STREAMS live output DURING execution — the render IS the
// run, it cannot be deferred. So this one library module prints directly,
// the same sanctioned exemption `main.rs` carries for the terminal bin.
#![allow(clippy::disallowed_macros, clippy::print_stdout, clippy::print_stderr)]

mod ask;
mod child_runner;
mod inputs;
mod sink;
mod thread;

pub use nika_dap::recover::{RecoveredTrace, recover_events};
pub use sink::{FoldSink, RenderMode};
pub(crate) use thread::run_in_thread;

mod example;
pub use example::example;
use sink::{ExecutionSink, TraceNote, TraceSurface, surface_trace};

mod budget;
mod ceiling;
mod dry_run;
mod epilogue;
mod execution_adapter;
#[cfg(test)]
mod extinction_tests;
mod provenance;
pub use provenance::run_with_repair_target;
mod heartbeat;
mod resume_setup;
mod teardown;
// ADR-111 · the outbound pause delivery lives in the host member
// (ADR-110 precedent — compute descends, render stays).
use nika_cli_host::notify;
use nika_cli_host::run_settlement::{first_failure, plan_waves, sensitive_journey};
pub(crate) use nika_event::source_id::{lf_normal_form, sha256_hex};
use resume_setup::{ResumeSetup, resume_setup};
use teardown::attended_facts;

use nika_dap::journal::{JsonSink, Tee, TraceFileSink};
use nika_dap::resume::ResumeRequest;
use nika_runtime::compose::{capabilities_of, simulated_runtime};
use nika_runtime::scope_to_task;
use nika_service_execution::AuthorizedRuntime;

/// The workflow's semantic hash for the run seal (the proof layer's
/// Merkle commitment over the task leaves — `None` when any task is
/// unprojectable, and an unhashable workflow stays on the unsigned floor).
fn seal_hash(wf: &nika_schema::raw::RawWorkflow) -> Option<String> {
    nika_runtime::proof::ir::merkle_by_task(wf).map(|p| p.workflow.as_hex().to_owned())
}

use std::collections::BTreeMap;
use std::io::Write as _;

use serde_json::Value;

use nika_check::CheckReport;
use nika_runtime::{EventSink, RunOutcome, Runtime, RuntimeError, Stamper};
use nika_schema::raw::RawWorkflow;

use crate::Theme;
use crate::verbs::exit;
pub(crate) use execution_adapter::run_arm_context;
use execution_adapter::{AdmittedWorld, run_admitted};

/// Opportunistic bounded trace collection. Dry-run and explicit opt-out are
/// effect-free; maintenance failure never blocks workflow execution.
fn run_start_gc(no_gc: bool, dry_run: bool) {
    if let Some(line) = nika_cli_host::retention::gc_at_run_start(
        std::path::Path::new(nika_dap::store::TRACE_DIR),
        no_gc,
        dry_run,
    ) {
        eprintln!("{line}");
    }
}

pub(crate) struct RunVerdict {
    pub(crate) code: u8,
    failure: Option<nika_runtime::TaskErrorRecord>,
    paused: Option<PausedLeg>,
    outputs: BTreeMap<String, serde_json::Value>,
    interrupted: bool,
    pub(crate) trace: Option<std::path::PathBuf>,
}

struct PausedLeg {
    pause: nika_runtime::WorkflowPause,
    trace: std::path::PathBuf,
}

impl RunVerdict {
    /// A verdict that carries no task failure (pre-run refusals · lanes
    /// that never reached the runtime).
    fn bare(code: u8) -> Self {
        Self {
            code,
            failure: None,
            paused: None,
            outputs: BTreeMap::new(),
            interrupted: false,
            trace: None,
        }
    }

    fn interrupted() -> Self {
        Self {
            code: exit::WORKFLOW,
            failure: None,
            paused: None,
            outputs: BTreeMap::new(),
            interrupted: true,
            trace: None,
        }
    }

    fn renderer_failed(trace: Option<std::path::PathBuf>, kind: std::io::ErrorKind) -> Self {
        let code = if kind == std::io::ErrorKind::BrokenPipe {
            141
        } else {
            exit::ENV
        };
        Self {
            trace,
            ..Self::bare(code)
        }
    }
}

fn surfaced_trace(surface: TraceSurface) -> Result<TraceSurface, Box<RunVerdict>> {
    if let Some(error) = &surface.note_error {
        let kind = error.kind();
        let _ = writeln!(
            std::io::stderr().lock(),
            "nika run: trace note failed: {error}"
        );
        Err(Box::new(RunVerdict::renderer_failed(
            surface.path.clone(),
            kind,
        )))
    } else {
        Ok(surface)
    }
}

/// Execute one checked workflow through the production runtime and stream its
/// selected human or machine surface. Returns the spec exit code: success,
/// workflow failure, file findings, environment refusal, or pause.
// Fifteen independent parameters are the clap surface (the TraceArgs idiom).
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
#[must_use]
pub fn run(
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
        None,
    )
    .code
}

/// Resolve output/ceiling and run the opportunistic trace collection.
fn preflight(
    output: Option<&str>,
    max_cost_usd: Option<f64>,
    no_gc: bool,
    dry_run: bool,
) -> Result<(bool, Option<f64>), Box<RunVerdict>> {
    let output_json = match output_mode(output) {
        Ok(flag) => flag,
        Err(code) => return Err(Box::new(RunVerdict::bare(code))),
    };
    let max_cost_usd = match ceiling::from_cwd(max_cost_usd) {
        Ok(v) => v,
        Err(e) => {
            epilogue::emit_diagnostic(&e.to_string(), output_json);
            return Err(Box::new(RunVerdict::bare(exit::ENV)));
        }
    };
    run_start_gc(no_gc, dry_run);
    Ok((output_json, max_cost_usd))
}

#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
#[must_use]
fn run_verdict(
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
    interruptible: bool,
    repair_target: Option<nika_display::check_render::RepairTarget>,
) -> RunVerdict {
    let (output_json, max_cost_usd) = match preflight(output, max_cost_usd, no_gc, dry_run) {
        Ok(pair) => pair,
        Err(verdict) => return *verdict,
    };
    let (source, wf, report) =
        match provenance::capture_checked_source(file, repair_target, output_json) {
            Ok(checked) => checked,
            Err(verdict) => return *verdict,
        };
    let file_owned = source.logical_path().to_owned();
    let file = file_owned.as_str();
    if require_signature && let Err(code) = require_signature_gate(&source, output_json) {
        return RunVerdict::bare(code);
    }
    let (_wf, _report, _skills) =
        match scoped_clean_gate(wf, report, task_filter, &source, json, theme, output_json) {
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
        vars,
        resume,
        no_trace_file,
        task_filter,
        no_outputs,
        max_cost_usd,
        interruptible,
    )
}

/// The access announce (D-2026-08-04-N1 · P2.6 + R-4): a pinned path is
/// a behavior the operator chose — said on the human surfaces before the
/// first frame. And when more than one access CAN serve a model, the
/// CHOSEN path is announced with its billing class (R-4 · the rich
/// announce: the choice is a fact worth hearing, never a silent pick).
/// Machine lanes stay silent (the trace carries the fields).
fn announce_access_pin(
    access_pin: Option<&str>,
    (json, output_json): (bool, bool),
    mode: RenderMode,
    report: &CheckReport,
) {
    if json || output_json || mode == RenderMode::Quiet {
        return;
    }
    if let Some(pin) = access_pin {
        eprintln!("access: pinned `{pin}` — unsatisfied refuses, never substitutes");
    }
    // R-4 · the rich announce: only when a real choice existed (>1
    // candidate). Feature-off the vec carries provider rows only, so
    // this stays silent exactly as before (one candidate per provider).
    let probes = nika_cli_host::probe::access_probes_with_harness();
    for m in &report.requirements.models {
        let model = m.model.as_str();
        if model.contains("${{") {
            continue;
        }
        let candidates =
            nika_providers::candidates_for(&probes, nika_providers::provider_of(model));
        if candidates.len() < 2 {
            continue;
        }
        if let Ok(plan) = nika_providers::resolve_access(model, &candidates, None, access_pin) {
            eprintln!(
                "access: {model} → {} ({} · {}) — chosen over {} other path(s)",
                plan.access,
                plan.chosen.as_str(),
                plan.billing.as_str(),
                candidates.len() - 1
            );
        }
    }
}

/// The first leg + the terminal ask (ADR-099 · "interactively it asks").
/// A paused HUMAN run with a terminal present continues in-process:
/// ask on stderr · bind the answer · resume over the just-written
/// trace — the SAME plan-fold + F-P4 ticket path a manual `--resume
/// --answer` takes (one behavior, attested the same way). Ctrl-D or
/// three unparseable tries leaves the durable pause + its taught
/// resume line. Bounded by the task count: a workflow cannot hold
/// more gates than tasks. Machine lanes never ask.
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
fn execute_and_ask(
    runtime: &AuthorizedRuntime,
    (file, source): (&str, &str),
    (wf, report): (&RawWorkflow, &CheckReport),
    resumed: bool,
    vars: &[String],
    model_override: Option<&str>,
    access_pin: Option<&str>,
    max_cost_usd: Option<f64>,
    theme: Theme,
    mode: RenderMode,
    (json, output_json, no_trace_file, no_outputs): (bool, bool, bool, bool),
    interruptible: bool,
    world: &AdmittedWorld,
) -> RunVerdict {
    let rt = match executor(output_json) {
        Ok(rt) => rt,
        Err(code) => return RunVerdict::bare(code),
    };
    // The re-invocation carry (`--var`/`--model`) every taught resume
    // line re-supplies — a required-input workflow refuses a var-less
    // resume, so a taught line without them failed on paste.
    let carry = epilogue::resume_carry(vars, model_override);
    let future = execute(
        runtime,
        (file, wf),
        report,
        json,
        output_json,
        theme,
        mode,
        resumed,
        trace_sink(no_trace_file, world.execution_id, world.trace_id),
        (world.execution_id, &world.snapshot_digest),
        !no_outputs,
        model_override,
        &carry,
    );
    let mut verdict = thread::block_on_run(&rt, future, interruptible);
    let mut legs = 0usize;
    while verdict.code == exit::PAUSED && !json && !output_json && legs <= wf.tasks.len() {
        let Some(leg) = verdict.paused.take() else {
            break;
        };
        if !ask::tty_present() {
            break;
        }
        let ask::Asked::Answer(value) = ask::ask_on_tty(&leg.pause, theme) else {
            break;
        };
        legs += 1;
        let request = ResumeRequest {
            trace: Some(leg.trace),
            from: None,
            answers: vec![format!("{}={value}", leg.pause.task)],
            compat: None,
            // The in-process continuation folds THIS run's own just-written
            // trace — the chain precondition applies unchanged (it holds
            // by construction: the pause frame closes the lifecycle).
            allow_unverified: false,
        };
        verdict = answered_leg(
            (file, source),
            (wf, report),
            &request,
            vars,
            model_override,
            access_pin,
            max_cost_usd,
            theme,
            mode,
            (json, output_json, no_trace_file, !no_outputs),
            &rt,
            world,
        );
    }
    verdict
}

/// One answered continuation of a paused run — re-validate the vars,
/// fold the paused trace (plan + F-P4 ticket), recompose, re-drive.
/// Exactly what a manual `--resume <trace> --answer <task>=<value>`
/// invocation does, minus the process boundary: upstream work
/// cache-hits visibly, the gate binds the attested answer.
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
fn answered_leg(
    (file, source): (&str, &str),
    (wf, report): (&RawWorkflow, &CheckReport),
    request: &ResumeRequest,
    vars: &[String],
    model_override: Option<&str>,
    access_pin: Option<&str>,
    max_cost_usd: Option<f64>,
    theme: Theme,
    mode: RenderMode,
    (json, output_json, no_trace_file, outputs): (bool, bool, bool, bool),
    rt: &tokio::runtime::Runtime,
    world: &AdmittedWorld,
) -> RunVerdict {
    let inputs = match inputs::validated_var_overrides(vars, wf, output_json) {
        Ok(map) => map,
        Err(code) => return RunVerdict::bare(code),
    };
    let setup = match resume_setup(Some(request), wf, source, model_override, output_json) {
        Ok(setup) => setup,
        Err(code) => return RunVerdict::bare(code),
    };
    let runtime = match composed_runtime(
        model_override,
        access_pin,
        inputs,
        setup,
        max_cost_usd,
        (no_trace_file, output_json),
        world,
    ) {
        Ok(rt) => rt,
        Err(code) => return RunVerdict::bare(code),
    };
    let carry = epilogue::resume_carry(vars, model_override);
    rt.block_on(execute(
        &runtime,
        (file, wf),
        report,
        json,
        output_json,
        theme,
        mode,
        true,
        trace_sink(no_trace_file, world.execution_id, world.trace_id),
        (world.execution_id, &world.snapshot_digest),
        outputs,
        model_override,
        &carry,
    ))
}

/// Build the current-thread executor the run blocks on — an executor
/// that will not start is the environment class (printed + enveloped).
///
/// # Errors
///
/// The exit code to return unchanged.
fn executor(output_json: bool) -> Result<tokio::runtime::Runtime, u8> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| {
            let message = format!("cannot start the async executor: {e}");
            eprintln!("nika run: environment: {message}");
            epilogue::emit_error_envelope(&message, output_json);
            exit::ENV
        })
}

/// The `--require-signature` trust gate: verify against an enrolled key
/// before anything executes (exit 2 FILE · already printed + enveloped).
fn require_signature_gate(source: &crate::verbs::RunSource, output_json: bool) -> Result<(), u8> {
    use crate::seal::WorkflowSig;
    let reason = match crate::seal::check_workflow_bytes(
        std::path::Path::new(source.logical_path()),
        source.source().as_bytes(),
    ) {
        WorkflowSig::Valid(_) => return Ok(()),
        WorkflowSig::MissingSidecar => "missing sidecar — `nika sign <file>` mints one".to_owned(),
        WorkflowSig::NoEnrolledKey => "unknown key — nothing enrolled on this machine".to_owned(),
        WorkflowSig::Invalid(why) => why,
    };
    let message = format!("--require-signature: {reason}");
    eprintln!("nika run: {message}");
    epilogue::emit_error_envelope(&message, output_json);
    Err(exit::FILE)
}

/// House-voice a pre-run refusal (the empty-state audit · design §3):
/// the ENV class (an unreadable/missing file) gains the `nika run:`
/// prefix, ONE `fix:` line and a closing newline — a bare
/// `cannot read …(os error 2)` glued to the prompt taught nothing.
/// FILE findings pass through untouched (the check renderer's card is
/// already the teaching surface).
fn refusal_text(out: &crate::verbs::VerbOutput) -> String {
    if out.code != exit::ENV {
        return out.text.clone();
    }
    format!(
        "nika run: {}\n  fix: check the path — bare `nika try` names runnable demos\n",
        out.text.trim_end()
    )
}

/// Validate `--output` up front — `Ok(true)` selects the machine-result
/// mode (spec 01 §"What leaves a run": the resolved `outputs:` object as
/// ONE JSON object on stdout · diagnostics/progress on stderr) · `Ok(false)`
/// the live human render · `Err(exit)` an unknown format (already printed).
fn output_mode(output: Option<&str>) -> Result<bool, u8> {
    match output {
        None => Ok(false),
        Some("json") => Ok(true),
        Some(other) => {
            eprintln!("nika run: unknown --output format `{other}` (expected `json`)");
            Err(exit::ENV)
        }
    }
}

/// Compose the production runtime for one run — extracted so `run` stays
/// under the fn-length cap without losing the composition story.
///
/// The envelope default model · a task's own `model:` overrides it · an
/// exec-only workflow never resolves it (so "" is harmless until an
/// infer/agent task actually needs a model · resolve is loud then). A
/// `--model` override REPLACES the envelope default through this SAME
/// path — a bad id fails loud at resolve time exactly as a bad envelope
/// model does (no separate, lenient validation seam).
///
/// Both runtime capability boundaries (permits.fs + permits.net.http)
/// ride in one value (spec §permits · NIKA-SEC-004) — derived once so
/// neither axis can be wired while the other is forgotten. fs gates the
/// file builtins; net gates the fetch client per-hop (catching dynamic/
/// redirect hosts the static check cannot see). The static check is the
/// other half of each. The validated `--var` overrides merge OVER the
/// envelope defaults at run start (F4).
///
/// # Errors
///
/// The ENV-class composition failure prints + envelopes itself here;
/// the caller returns the exit code untouched.
// The knobs ARE the composition surface (var overrides · resume plan ·
// answers · pause flag · resolved skills) — the same clap-surface idiom
// as `run` itself.
#[allow(clippy::too_many_arguments)]
fn composed_runtime(
    model_override: Option<&str>,
    access_pin: Option<&str>,
    inputs: inputs::ValidatedInputs,
    setup: ResumeSetup,
    max_cost_usd: Option<f64>,
    (_no_trace_file, output_json): (bool, bool),
    world: &AdmittedWorld,
) -> Result<AuthorizedRuntime, u8> {
    let ResumeSetup {
        plan: resume_plan,
        answers,
        paused,
        compat,
        unverified,
    } = setup;
    let inputs::ValidatedInputs {
        values: overrides,
        origins,
    } = inputs;
    let wf = world.driver.workflow();
    let report = world.driver.report();
    let envelope_model = wf.model.as_ref().map_or("", |m| m.value.as_str());
    let default_model = model_override.unwrap_or(envelope_model);
    // R-2 · the boot-manifest access stamps, composer-computed (P3 B5).
    let boot_access = crate::verbs::check::models_rung::boot_access_fields(report, access_pin);
    // F-P3 · the run: declaration rides the SAME composition path (clock ·
    // jitter seed — the stamper half is picked at the drive site). The driver
    // already owns the exact workflow/report/skills admitted with its bytes.
    match world.driver.compose(default_model) {
        Ok(rt) => {
            let rt = rt
                .with_var_overrides(overrides)
                // F-P13 · the input origins (NEP-0014 law 2) — the boot
                // manifest journals where every bound input came from.
                .with_input_origins(origins)
                .with_max_cost_usd(max_cost_usd)
                // ADR-099 rider — ALWAYS armed: a blocked `nika:prompt`
                // pauses durably on EVERY lane; the old `json ||
                // output_json` proxy left a headless TEXT run dying at
                // its own gate.
                .with_prompt_pause(true)
                .with_prompt_answers(answers)
                // F-P4 · the folded resume authority (NEP-0013) — the
                // `--answer` validates against the shown ticket.
                .with_paused_approval(paused)
                // F-P21 · the declared cross-version compat (NEP-0014
                // law 4) — attested on the boot manifest.
                .with_resume_compat(compat)
                // ADR-099 trust amendment · the unverified-trust
                // posture, attested on the boot manifest.
                .with_resume_unverified(unverified)
                // #409 · the override joins the resume identity of every
                // model-less infer/agent task (the model they RUN on).
                .with_model_override(model_override.map(ToOwned::to_owned))
                // D-2026-08-04-N1 · the `--access` pin: unsatisfied refuses BEFORE the prologue.
                .with_access_pin(access_pin.map(ToOwned::to_owned))
                .with_boot_access_fields(boot_access)
                .with_access_probes(nika_cli_host::probe::access_probes_with_harness());
            #[cfg(feature = "access-harness")]
            let rt = match rt.with_harness_from_pin(access_pin) {
                Ok(rt) => rt,
                Err(e) => {
                    return Err(epilogue::env_refusal(
                        &format!("environment: {e}"),
                        output_json,
                    ));
                }
            };
            #[cfg(not(feature = "access-harness"))]
            let rt = rt;
            Ok(match resume_plan {
                Some(plan) => rt.with_resume_plan(plan),
                None => rt,
            })
        }
        Err(e) => Err(epilogue::env_refusal(
            &format!("environment: {e}"),
            output_json,
        )),
    }
}

/// Execute a CHECKED workflow with the MOCK provider and capture the typed
/// `outputs:` — the `nika test` seam (F7). Every envelope and task-local
/// model is replaced by the runtime's `mock/echo` default (offline · zero
/// key · deterministic + schema-conformant since F3) — and since P0-16
/// the composition is the SIMULATED plane ([`simulated_runtime`]): the
/// model swap never left the tool/exec seams real again — net, exec, and
/// write effects REFUSE with « effects disabled under `nika test` », so a
/// mock run produces zero external effects (a workflow that needs one
/// fails its golden run honestly, it never silently performs it). The fold
/// is a DIAGNOSTIC here — it goes to stderr (verdict card on failure
/// only), so the caller owns stdout for its own verdict/diff surface.
/// `skills` = the composer-resolved SKILL.md texts (#473 · the caller
/// gates their findings first, same as `run`).
#[cfg(test)]
pub(crate) fn capture_mock_outputs(
    wf: &RawWorkflow,
    report: &CheckReport,
    skills: BTreeMap<String, String>,
    theme: Theme,
) -> Result<(u8, BTreeMap<String, Value>), String> {
    capture_mock_outputs_with_answers(wf, report, skills, BTreeMap::new(), theme)
}

/// `capture_mock_outputs` with operator-bound prompt decisions for
/// `nika test --answer TASK=VALUE`.
pub(crate) fn capture_mock_outputs_with_answers(
    wf: &RawWorkflow,
    _report: &CheckReport,
    skills: BTreeMap<String, String>,
    answers: BTreeMap<String, Value>,
    theme: Theme,
) -> Result<(u8, BTreeMap<String, Value>), String> {
    let mock_wf = force_mock_models(wf);
    let mut mock_report = nika_check::check(&mock_wf);
    crate::verbs::stamp_judged_semantic(&mock_wf, &mut mock_report);
    let caps = capabilities_of(&mock_wf);
    let runtime = simulated_runtime("mock/echo", caps, mock_wf.run.as_ref().map(|s| &s.value))
        .map_err(|e| e.to_string())?;
    let runtime = runtime.with_skills(skills).with_prompt_answers(answers);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("cannot start the async executor: {e}"))?;
    Ok(rt.block_on(async {
        // F-P3 · the declaration picks the stamper too (seeded/none →
        // deterministic stamps — the test goldens stop drifting).
        let mut stamper =
            nika_runtime::RunSeams::of(mock_wf.run.as_ref().map(|s| &s.value)).stamper();
        let mut sink = FoldSink::new(std::io::stderr().lock(), theme, RenderMode::Quiet);
        let (code, outcome) = drive_raw(
            &runtime,
            &mock_wf,
            &mock_report,
            stamper.as_mut(),
            &mut sink,
        )
        .await;
        // Success is silent (the caller prints the test verdict); a failed
        // mock run surfaces its compact verdict card so the operator sees
        // WHY before the caller's exit.
        if code != exit::OK {
            sink.print_final();
        }
        (code, outcome.outputs)
    }))
}

/// Remove every authored model choice before a golden run. The simulated
/// runtime owns one model, `mock/echo`; a task-local provider pin must not
/// escape that plane merely because it has higher normal-run precedence.
fn force_mock_models(wf: &RawWorkflow) -> RawWorkflow {
    let mut mock = wf.clone();
    mock.model = None;
    for task in &mut mock.tasks {
        match &mut task.value.action {
            nika_schema::raw::RawAction::Infer(action) => action.model = None,
            nika_schema::raw::RawAction::Agent(action) => action.model = None,
            _ => {}
        }
    }
    mock
}

/// The `--task` scope + the clean gate, fused (both run before any
/// effect): the WHOLE-FILE report gates first — the `--task` help's
/// promise (« findings stay whole-file faithful »): a file must be sound
/// even to regenerate one block, so an out-of-cone finding refuses the
/// scoped run exactly like the unscoped one (#411 — the scoped re-check
/// used to REPLACE the full report before the gate looked, and findings
/// outside the ancestor cone vanished). Only then scope to the target's
/// ancestor cone (the sub-DAG re-checks so plan/waves/cost describe
/// exactly what runs), and gate the scoped report too (a cut that
/// orphans a reference must refuse, not run). Dirty either way renders
/// the SAME findings `nika check` does (locked rendering · exit 2 ·
/// stderr in machine mode).
#[allow(clippy::too_many_arguments)]
fn scoped_clean_gate(
    wf: RawWorkflow,
    report: CheckReport,
    task_filter: Option<&str>,
    source: &crate::verbs::RunSource,
    json: bool,
    theme: Theme,
    output_json: bool,
) -> Result<(RawWorkflow, CheckReport, BTreeMap<String, String>), u8> {
    let refuse = || {
        let out = crate::verbs::check::run_source_with_profile(
            source,
            json,
            false,
            crate::verbs::check::Profile::Advisory,
            None,
            theme,
        );
        epilogue::emit_diagnostic(&out.text, output_json);
        out.code
    };
    if !report.is_clean() {
        return Err(refuse());
    }
    let (wf, report) = apply_task_scope(wf, report, task_filter, output_json)?;
    if !report.is_clean() {
        return Err(refuse());
    }
    // `skills:` gate (#473 · pre-effect · the SAME rows check renders).
    let resolved = crate::verbs::resolve_workflow_skills(
        &wf,
        crate::verbs::workflow_base(source.logical_path()),
    );
    if !resolved.findings.is_empty() {
        return Err(refuse());
    }
    Ok((wf, report, resolved.texts))
}

/// Apply the `--task` scope when requested (the regenerate-one-block
/// move). The FULL workflow gated clean already (whole-file spans ·
/// faithful findings — `scoped_clean_gate` refuses BEFORE this cut) —
/// the sub-DAG RE-CHECKS here so the plan/waves/cost describe
/// exactly what will run. Scoping happens before any effect; an unknown
/// id refuses on the diagnostic surface with the environment exit class.
fn apply_task_scope(
    wf: RawWorkflow,
    report: CheckReport,
    task_filter: Option<&str>,
    output_json: bool,
) -> Result<(RawWorkflow, CheckReport), u8> {
    let Some(target) = task_filter else {
        return Ok((wf, report));
    };
    match scope_to_task(wf, target) {
        Ok(sub) => {
            let mut sub_report = nika_check::check(&sub);
            // F-P2 · the scoped report is judged over the CONE — stamp
            // the cone's semantic hash so the trust gate binds it.
            crate::verbs::stamp_judged_semantic(&sub, &mut sub_report);
            Ok((sub, sub_report))
        }
        Err(msg) => {
            epilogue::emit_diagnostic(&msg, output_json);
            Err(exit::ENV)
        }
    }
}

/// Drive the runtime through the chosen sink · return the exit code.
///
/// Every lane tees the run journal (`trace` · a [`TraceFileSink`] ·
/// `.nika/traces/` · spec §3.3) BESIDE its primary surface: the primary's
/// bytes stay exact (the rider can only buffer its own fs error, surfaced
/// after the run · never the exit code). The caller composes the journal
/// (enabled or disabled) like every other seam.
// Trailing bools mirror the clap surface (independent flags ARE bools,
// not a state machine). The workflow rides as (path, parsed) — the
// epilogue hint teaches a command over the SAME file just run.
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
async fn execute(
    runtime: &AuthorizedRuntime,
    (file, wf): (&str, &RawWorkflow),
    report: &CheckReport,
    json: bool,
    output_json: bool,
    theme: Theme,
    mode: RenderMode,
    resumed: bool,
    trace: TraceFileSink,
    identity: (nika_types::id::ExecutionId, &str),
    outputs: bool,
    model_override: Option<&str>,
    carry: &str,
) -> RunVerdict {
    // F-P3 · the run: declaration picks the event-identity seam:
    // `entropy: none | seeded(N)` mints deterministic stamps (replayable
    // journals), ambient keeps the live `UUIDv7`+wall-clock stamper.
    let mut stamper = nika_runtime::RunSeams::of(wf.run.as_ref().map(|s| &s.value)).stamper();
    if output_json {
        execute_output_json_lane(
            runtime,
            (file, wf),
            report,
            stamper.as_mut(),
            theme,
            resumed,
            trace,
            identity.0,
            carry,
        )
        .await
    } else if json {
        execute_json_lane(
            runtime,
            (file, wf),
            report,
            stamper.as_mut(),
            resumed,
            trace,
            identity,
            carry,
        )
        .await
    } else {
        execute_fold_lane(
            runtime,
            wf,
            report,
            stamper.as_mut(),
            file,
            theme,
            (mode, resumed, outputs),
            trace,
            identity.0,
            model_override,
            carry,
        )
        .await
    }
}

/// The machine-result lane (`--output json` · spec 01 §export · 08
/// §composition) — extracted whole (the fold-lane precedent · the
/// fn-length wall): the fold is a DIAGNOSTIC → stderr (Plain
/// deliberately — a pipe, never the live redraw); stdout is ALWAYS one
/// self-sufficient JSON object (F6): the resolved `outputs:` on
/// success · the `{"error":{…}}` envelope on failure · the
/// `{"paused":{…}}` envelope on a human-gate pause (ADR-099 rider).
#[allow(clippy::too_many_arguments)] // the clap-surface idiom (execute's own)
async fn execute_output_json_lane(
    runtime: &AuthorizedRuntime,
    (file, wf): (&str, &RawWorkflow),
    report: &CheckReport,
    stamper: &mut dyn Stamper,
    theme: Theme,
    resumed: bool,
    trace: TraceFileSink,
    execution: nika_types::id::ExecutionId,
    carry: &str,
) -> RunVerdict {
    let mut fold = FoldSink::new(std::io::stderr().lock(), theme, RenderMode::Plain);
    fold.set_plan(plan_waves(wf, report));
    fold.set_trace_recorded(!trace.is_disabled());
    let tee = Tee::new(fold, trace);
    let mut events = ExecutionSink::new(tee, execution);
    let (code, outcome) = drive(runtime, stamper, &mut events).await;
    let (mut sink, mut trace) = events.into_inner().into_parts();
    sink.print_final();
    // R4 — the auth-class tail rides stderr (stdout stays the clean
    // JSON object · the export contract).
    if code == exit::WORKFLOW {
        let mut err = std::io::stderr().lock();
        nika_cli_host::probe::print_seat_escape(
            epilogue::failure_witnesses(sink.view()),
            "nika run: ",
            &mut err,
        );
    }
    // Pause delivery is journaled before the seal (ADR-111).
    if let (Some(p), Some(pause)) = (
        trace.path().map(std::path::Path::to_path_buf),
        outcome.paused.as_ref(),
    ) {
        let hint = epilogue::resume_hint_line(file, &p, pause, carry);
        let label = notify::workflow_label(wf);
        notify::deliver_paused(&label, pause, &p, &hint, stamper, &mut trace).await;
    }
    // Quarantine failed outputs before the seal (F-P14).
    let teardown = attended_facts(wf, report, &outcome, trace.path());
    let trace_path = match surfaced_trace(surface_trace(
        trace,
        TraceNote::Stderr,
        None,
        seal_hash(wf).as_deref(),
        Some(&teardown),
        sensitive_journey(report),
    )) {
        Ok(surface) => surface.path,
        Err(verdict) => return *verdict,
    };
    if let (Some(p), Some(pause)) = (&trace_path, &outcome.paused) {
        eprintln!(
            "nika run: {}",
            epilogue::resume_hint_line(file, p, pause, carry)
        );
    }
    epilogue::print_resume_summary(&outcome, resumed, true);
    let verdict_line = if code == exit::PAUSED {
        outcome
            .paused
            .as_ref()
            .map(|p| epilogue::paused_envelope_line(p, carry))
    } else if code != exit::OK {
        Some(epilogue::run_failure_envelope(sink.view()))
    } else {
        None
    };
    if let Some(e) = sink.into_error() {
        let message = format!("render failed: {e}");
        eprintln!("nika run: {message}");
        println!("{}", epilogue::error_envelope_line(&message));
        return RunVerdict::renderer_failed(trace_path, e.kind());
    }
    match verdict_line {
        Some(line) => println!("{line}"),
        None => println!("{}", epilogue::outputs_json_line(&outcome.outputs)),
    }
    RunVerdict {
        code,
        failure: first_failure(&outcome),
        paused: None,
        outputs: outcome.outputs.clone(),
        interrupted: false,
        trace: trace_path,
    }
}

/// NDJSON lane: stdout stays byte-exact; the trace note rides stderr.
#[allow(clippy::too_many_arguments)] // the clap-surface idiom (execute's own)
async fn execute_json_lane(
    runtime: &AuthorizedRuntime,
    (file, wf): (&str, &RawWorkflow),
    report: &CheckReport,
    stamper: &mut dyn Stamper,
    resumed: bool,
    trace: TraceFileSink,
    identity: (nika_types::id::ExecutionId, &str),
    carry: &str,
) -> RunVerdict {
    let tee = Tee::new(JsonSink::new(std::io::stdout().lock()), trace);
    let mut events = ExecutionSink::new(tee, identity.0);
    let (code, outcome) = drive(runtime, stamper, &mut events).await;
    let (mut sink, mut trace) = events.into_inner().into_parts();
    if let (Some(p), Some(pause)) = (
        trace.path().map(std::path::Path::to_path_buf),
        outcome.paused.as_ref(),
    ) {
        let hint = epilogue::resume_hint_line(file, &p, pause, carry);
        let label = notify::workflow_label(wf);
        notify::deliver_paused(&label, pause, &p, &hint, stamper, &mut trace).await;
    }
    let mut teardown = attended_facts(wf, report, &outcome, trace.path());
    teardown.sdk_receipt = Some(nika_cli_host::run_settlement::local_receipt_binding(
        identity.0, identity.1,
    ));
    let surface = match surfaced_trace(surface_trace(
        trace,
        TraceNote::Stderr,
        None,
        seal_hash(wf).as_deref(),
        Some(&teardown),
        sensitive_journey(report),
    )) {
        Ok(surface) => surface,
        Err(verdict) => return *verdict,
    };
    let trace_path = surface.path.clone();
    if let (Some(p), Some(pause)) = (&trace_path, &outcome.paused) {
        eprintln!(
            "nika run: {}",
            epilogue::resume_hint_line(file, p, pause, carry)
        );
    }
    if let Some(e) = sink.error() {
        eprintln!("nika run: stream write failed: {e}");
        return RunVerdict::renderer_failed(trace_path, e.kind());
    }
    let trace_proof = surface.path.as_deref().zip(surface.proof.as_ref());
    if let Err(e) = nika_cli_host::run_settlement::write_local_run_settlement(
        &mut sink,
        (outcome.ok, outcome.paused.is_some()),
        &outcome.outputs,
        identity.0,
        identity.1,
        trace_proof,
    ) {
        eprintln!("nika run: settlement write failed: {e}");
        return RunVerdict::renderer_failed(trace_path, e.kind());
    }
    epilogue::print_resume_summary(&outcome, resumed, true);
    RunVerdict {
        code,
        failure: first_failure(&outcome),
        paused: None,
        outputs: outcome.outputs.clone(),
        interrupted: false,
        trace: trace_path,
    }
}

/// Human fold lane (`Live` · `Plain` · `Quiet`).
#[allow(clippy::too_many_arguments)]
async fn execute_fold_lane(
    runtime: &AuthorizedRuntime,
    wf: &RawWorkflow,
    report: &CheckReport,
    stamper: &mut dyn Stamper,
    file: &str,
    theme: Theme,
    (mode, resumed, outputs): (RenderMode, bool, bool),
    trace: TraceFileSink,
    execution: nika_types::id::ExecutionId,
    model_override: Option<&str>,
    carry: &str,
) -> RunVerdict {
    let plan = plan_waves(wf, report);
    let map = (mode == RenderMode::Live && theme.accents)
        .then(|| (super::graph::project(wf, report), report.waves.clone()));
    let trace_recorded = !trace.is_disabled();
    let (fold, spinner) = shared_fold(theme, mode, outputs, plan.clone(), map);
    if let Ok(mut f) = fold.lock() {
        f.set_trace_recorded(trace_recorded);
    }
    // Plain's heartbeat keeps a piped local-model run from reading as a hang.
    let pulse = (mode == RenderMode::Plain).then(|| {
        let default_model =
            model_override.unwrap_or_else(|| wf.model.as_ref().map_or("", |m| m.value.as_str()));
        heartbeat::shared(plan, heartbeat::task_labels(wf, default_model))
    });
    let ticker = pulse.clone().map(heartbeat::spawn_ticker);
    let beat = heartbeat::HeartbeatSink::new(pulse);
    let tee = Tee::new(
        Tee::new(sink::FoldHandle(std::sync::Arc::clone(&fold)), beat),
        trace,
    );
    let mut events = ExecutionSink::new(tee, execution);
    let (code, outcome) = drive(runtime, stamper, &mut events).await;
    // The run settled: stop riders before the epilogue.
    if let Some(ticker) = &ticker {
        ticker.abort();
    }
    if let Some(spinner) = &spinner {
        spinner.abort();
    }
    let (_inner, mut trace) = events.into_inner().into_parts();
    if let (Some(p), Some(pause)) = (
        trace.path().map(std::path::Path::to_path_buf),
        outcome.paused.as_ref(),
    ) {
        let hint = epilogue::resume_hint_line(file, &p, pause, carry);
        let label = notify::workflow_label(wf);
        notify::deliver_paused(&label, pause, &p, &hint, stamper, &mut trace).await;
    }
    let Ok(mut sink) = fold.lock() else {
        eprintln!("nika run: render state poisoned");
        return RunVerdict::bare(exit::ENV);
    };
    if mode != RenderMode::Live {
        sink.print_final();
    }
    if mode == RenderMode::Live {
        epilogue::print_flow_epilogue(sink.view(), &outcome.outputs, theme, file, trace_recorded);
    }
    fold_lane_verdict(
        &mut sink,
        (wf, report),
        &outcome,
        trace,
        mode,
        (file, carry),
        resumed,
        code,
    )
}

/// The fold lane's last words (extracted · the fn-length wall): the
/// spec §3.3 `trace:` pointer, the paused teaching line, the resume
/// summary, the render-error gate — and the verdict, carrying the
/// pause up so the terminal ask can continue the run in-process.
#[allow(clippy::too_many_arguments)] // the lane's own teardown facts
fn fold_lane_verdict(
    sink: &mut FoldSink<std::io::Stdout>,
    (wf, report): (&RawWorkflow, &CheckReport),
    outcome: &nika_runtime::RunOutcome,
    trace: TraceFileSink,
    mode: RenderMode,
    (file, carry): (&str, &str),
    resumed: bool,
    code: u8,
) -> RunVerdict {
    // The spec §3.3 final-frame pointer (`trace: …`) — under the frame
    // on the storytelling surfaces.
    let failed_task = sink
        .view()
        .rows()
        .iter()
        .find(|r| r.state == crate::TaskState::Failed)
        .map(|r| r.id.clone());
    // R4 — an auth-class failure (a missing credential · a refused
    // admission) names the seat escape hatch beside its card, when this
    // machine has a signed-in seat.
    if code == exit::WORKFLOW {
        let mut out = std::io::stdout().lock();
        nika_cli_host::probe::print_seat_escape(
            epilogue::failure_witnesses(sink.view()),
            "  ",
            &mut out,
        );
    }
    // F-P14 · the failure lane's quarantine runs BEFORE the seal.
    let teardown = attended_facts(wf, report, outcome, trace.path());
    let trace_path = match surfaced_trace(surface_trace(
        trace,
        if matches!(mode, RenderMode::Quiet | RenderMode::Thread) {
            TraceNote::Silent
        } else {
            TraceNote::Stdout
        },
        failed_task.as_deref(),
        seal_hash(wf).as_deref(),
        Some(&teardown),
        sensitive_journey(report),
    )) {
        Ok(surface) => surface.path,
        Err(verdict) => return *verdict,
    };
    // A paused HUMAN run teaches its exact resume command too (the same
    // line the machine lanes print on stderr — a text-mode pause with no
    // next move taught was the first-run killer, 2026-07-31). Quiet keeps
    // the one teaching line: a pause without it is a dead end, not
    // compactness.
    if let (Some(p), Some(pause)) = (&trace_path, &outcome.paused) {
        println!("    {}", epilogue::resume_hint_line(file, p, pause, carry));
    }
    epilogue::print_resume_summary(outcome, resumed, false);
    if let Some(e) = sink.take_error() {
        eprintln!("nika run: render failed: {e}");
        return RunVerdict::renderer_failed(trace_path, e.kind());
    }
    // The fold lane hands the pause up: the terminal ask continues the
    // run in-process over this trace.
    let paused = match (&trace_path, &outcome.paused) {
        (Some(p), Some(pause)) => Some(PausedLeg {
            pause: pause.clone(),
            trace: p.clone(),
        }),
        _ => None,
    };
    RunVerdict {
        code,
        failure: first_failure(outcome),
        paused,
        outputs: outcome.outputs.clone(),
        interrupted: false,
        trace: trace_path,
    }
}

/// Build the shared fold + its spinner rider (extracted so the fold
/// lane stays under the fn-length ratchet). `Stdout` — not the guard:
/// the rider is a spawned task and the stdout guard is thread-bound;
/// per-write locking is fine because every REDRAW rides one DEC-2026
/// synchronized frame anyway. The shape tails ride the INTERACTIVE
/// surface only; the braille beat spawns for Live + motion only (the
/// fold re-checks both on every tick).
fn shared_fold(
    theme: Theme,
    mode: RenderMode,
    outputs: bool,
    plan: Vec<Vec<String>>,
    map: Option<(crate::verbs::graph::GraphDoc, Vec<Vec<usize>>)>,
) -> (
    sink::SharedFold<std::io::Stdout>,
    Option<tokio::task::JoinHandle<()>>,
) {
    let mut fold = FoldSink::new(std::io::stdout(), theme, mode);
    fold.set_plan(plan);
    if let Some((doc, waves)) = map {
        fold.set_map(doc, waves);
    }
    if mode == RenderMode::Live && outputs {
        fold.show_outputs(true);
    }
    let fold = std::sync::Arc::new(std::sync::Mutex::new(fold));
    let spinner = theme
        .animate
        .then(|| sink::spawn_spinner(std::sync::Arc::clone(&fold)));
    (fold, spinner)
}

/// Run the workflow through a sink + map the outcome to an exit code.
///
/// A `RuntimeError` out of `run` is exit 3, never a panic: NIKA-1708 (the
/// admission refusal · an OPERATOR miss) prints its text — any other class is a SYSTEM breach and says so.
async fn drive_raw<S, T, H, P, D, C>(
    runtime: &Runtime<S, T, H, P, D, C>,
    wf: &RawWorkflow,
    report: &CheckReport,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) -> (u8, RunOutcome)
where
    S: nika_kernel::process::ShellRunDyn + Sync,
    T: nika_kernel::tool_executor::ToolExecuteDyn,
    H: nika_kernel::http::HttpPostDyn + Send + Sync + 'static,
    P: nika_kernel::ai::provider::ProviderInferDyn + nika_kernel::ai::provider::ProviderMeta,
    D: nika_kernel::ai::tool_defs::ToolDefinitionProviderDyn,
    C: nika_kernel::clock::ClockDyn + Sync,
{
    map_run_result(runtime.run(wf, report, stamper, sink).await)
}

async fn drive(
    runtime: &AuthorizedRuntime,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) -> (u8, RunOutcome) {
    map_run_result(runtime.run(stamper, sink).await)
}

fn map_run_result(result: Result<RunOutcome, RuntimeError>) -> (u8, RunOutcome) {
    match result {
        Ok(outcome) => {
            // Paused wins the mapping (ADR-099 rider): non-zero on purpose
            // (`&& next` must not proceed past an unanswered human gate),
            // never the WORKFLOW failure code (a pause is not a defect).
            let code = if outcome.paused.is_some() {
                exit::PAUSED
            } else if outcome.ok {
                exit::OK
            } else {
                exit::WORKFLOW
            };
            (code, outcome)
        }
        Err(err) => {
            use nika_error::traits::NikaErrorCode as _;
            let mut stderr = std::io::stderr().lock();
            if matches!(
                err,
                // The launch-refusal family the OPERATOR can fix (missing
                // input · the `--access` gate D-2026-08-04-N1): the error's
                // own teaching text, never the contract-breach envelope.
                RuntimeError::MissingRequiredInputs { .. }
                    | RuntimeError::AccessNoPath { .. }
                    | RuntimeError::AccessPinUnsatisfied { .. }
                    | RuntimeError::AccessUnknownToken { .. }
                    | RuntimeError::AccessUnavailable { .. }
            ) {
                let _ = writeln!(stderr, "nika run: {err}");
                // R4 — the 1800 witness names the missing credential's
                // custody; the L4 renderer (which knows seats, unlike
                // the provider layer) appends the seat escape hatch —
                // census-derived, so it prints only when TRUE.
                if matches!(err, RuntimeError::AccessNoPath { .. })
                    && let Some(tail) = nika_cli_host::probe::seat_escape_tail()
                {
                    let _ = writeln!(stderr, "nika run: {tail}");
                }
            } else if let RuntimeError::ReportMismatch { .. } = err {
                // Audit-before-run (spec §4): the report does not describe
                // THESE bytes — the file-findings class (the F-P2
                // judged-vs-booted binding), never a system breach.
                let _ = writeln!(stderr, "nika run: {err}");
                return (
                    exit::FILE,
                    RunOutcome::new(false, BTreeMap::new(), BTreeMap::new()),
                );
            } else {
                let _ = writeln!(
                    stderr,
                    "nika run: system: the checked workflow was rejected at run \
                     time ({} · {err}) — this is an engine contract breach, \
                     please report it",
                    err.nika_code()
                );
            }
            (
                exit::ENV,
                RunOutcome::new(false, BTreeMap::new(), BTreeMap::new()),
            )
        }
    }
}

/// The run journal (spec §3.3) — composed like the other seams;
/// `execute` receives the sink, never a flag. The directory constant
/// is the store scan's (one constant, one home).
fn trace_sink(
    no_trace_file: bool,
    execution: nika_types::id::ExecutionId,
    trace: nika_types::id::TraceId,
) -> TraceFileSink {
    let sink = if no_trace_file {
        TraceFileSink::disabled()
    } else {
        TraceFileSink::new(nika_dap::store::TRACE_DIR)
    };
    sink.for_execution(execution, trace)
}

#[cfg(test)]
mod tests;
