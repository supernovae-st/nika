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

mod child_runner;
mod inputs;
mod sink;

pub use nika_dap::recover::{RecoveredTrace, recover_events};
pub use sink::{FoldSink, RenderMode};

mod example;
pub use example::example;
use sink::{TraceNote, surface_trace};

mod budget;
mod epilogue;
mod heartbeat;
mod teardown;
pub(crate) use nika_event::source_id::{lf_normal_form, sha256_hex};
use teardown::attended_facts;

use nika_dap::journal::{JsonSink, Tee, TraceFileSink};
use nika_dap::resume::ResumeRequest;
use nika_runtime::compose::{ProdRuntime, capabilities_of, production_runtime};
use nika_runtime::scope_to_task;

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
use nika_runtime::resume::ResumePlan;
use nika_runtime::{EventSink, RunOutcome, Runtime, RuntimeError, Stamper};
use nika_schema::raw::RawWorkflow;

use crate::Theme;
use crate::verbs::exit;

/// `nika run <file>` — the verb (spec §4 exit contract).
///
/// Streams the run to stdout (live fold · or NDJSON under `--json`) and
/// returns the exit code: `0` ok · `1` workflow failed · `2` file
/// findings (audit-before-run · the dirty report never executes) · `3`
/// environment (unreadable file · TLS init · a system contract breach).
///
/// `model_override` — when `Some(m)`, `m` REPLACES the workflow envelope's
/// `model:` as the resolved default (so `examples run … --model mock/echo`
/// previews offline). It travels the SAME composition path as an envelope
/// model, so a bad id fails loud identically (the registry surfaces its
/// typed error when an infer/agent task actually resolves it).
///
/// `vars` — the repeatable `--var KEY=VALUE` overrides (F4): each key must
/// be a declared workflow var (unknown keys are refused · exit 3); values
/// override a `default:` and satisfy a `required: true` var.
///
/// `resume` — `--resume <trace>` (ADR-099): fold the prior run's NDJSON
/// journal into a skip plan; the runtime recomputes each task's identity
/// and skips iff BOTH hashes match (visible `task_cache_hit` · never
/// silent). `--from <task_id>` forces a subtree to re-run.
///
/// `no_trace_file` — skip the run journal (`.nika/traces/` · spec §3.3):
/// `--no-trace-file` / `NIKA_NO_TRACE_FILE` opt out; `examples run`
/// disables it too (a staged temp-file run is not a workspace run).
// Ten independent CLI parameters ARE the clap surface — the same idiom
// as TraceArgs' four bools, not a state machine to encode in a struct.
/// `no_outputs` — `--no-outputs` (the comprehension pass): suppress the
/// shape tails on the Live storyboard. Only the interactive TTY surface
/// ever grows tails — pipes · CI · the machine modes stay byte-unchanged
/// with or without the flag.
///
/// `no_gc` — `--no-gc` (ADR-100 D2): skip the opportunistic trace
/// collection for this invocation. Retention otherwise rides every run
/// start (bounded by default · no daemon).
/// Opportunistic trace GC (ADR-100 D2) — maintenance rides usage. Before
/// the run · `--no-gc` skips · `--dry-run` never collects (plan only ·
/// zero effects) · fail-open (a broken collection never blocks a run). A
/// collection that removed anything speaks EXACTLY ONE stderr line —
/// silent deletion is forbidden, and stderr keeps the machine surfaces
/// (`--json` · `--output json` stdout) byte-frozen.
fn run_start_gc(no_gc: bool, dry_run: bool) {
    if let Some(line) = super::trace::retention::gc_at_run_start(
        std::path::Path::new(super::trace::store::TRACE_DIR),
        no_gc,
        dry_run,
    ) {
        eprintln!("{line}");
    }
}

/// The run's verdict: the process exit code PLUS the first failed task's
/// typed error (spec 05 wire code + message). The exit code alone cannot
/// say WHY a run failed — the examples wrapper keys its rescue tip on the
/// failure KIND (#145: a missing program must never earn the mock-model
/// nudge an infer failure deserves). Module-internal on purpose: the
/// public verb contract stays the exit code.
struct RunVerdict {
    /// The process exit code (the `exit::*` vocabulary).
    code: u8,
    /// The first failed task's error record (record order — deterministic).
    /// `None` on success, pause, and every pre-run refusal.
    failure: Option<nika_runtime::TaskErrorRecord>,
}

impl RunVerdict {
    /// A verdict that carries no task failure (pre-run refusals · lanes
    /// that never reached the runtime).
    const fn bare(code: u8) -> Self {
        Self {
            code,
            failure: None,
        }
    }
}

/// The first failed task's typed error, in record (task-id) order — the
/// deterministic pick when several tasks failed in one wave.
fn first_failure(outcome: &RunOutcome) -> Option<nika_runtime::TaskErrorRecord> {
    outcome
        .records
        .values()
        .find(|r| r.status == nika_runtime::TaskStatus::Failure)
        .and_then(|r| r.error.clone())
}

// Fifteen independent CLI parameters ARE the clap surface (the TraceArgs idiom).
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
        vars,
        resume,
        no_trace_file,
        task_filter,
        no_outputs,
        max_cost_usd,
        no_gc,
        require_signature,
    )
    .code
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
    vars: &[String],
    resume: Option<&ResumeRequest>,
    no_trace_file: bool,
    task_filter: Option<&str>,
    no_outputs: bool,
    max_cost_usd: Option<f64>,
    no_gc: bool,
    require_signature: bool,
) -> RunVerdict {
    let output_json = match output_mode(output) {
        Ok(flag) => flag,
        Err(code) => return RunVerdict::bare(code),
    };
    run_start_gc(no_gc, dry_run);

    // ── Audit BEFORE run (spec §3 · INV the runtime also enforces) ──
    let (source, wf, report) = match crate::verbs::load_checked_with_source(file) {
        Ok(pair) => pair,
        Err(out) => {
            // Machine mode: diagnostics ride stderr so a `capture: stdout`
            // consumer never mistakes them for the JSON result.
            epilogue::emit_diagnostic(&refusal_text(&out), output_json);
            return RunVerdict::bare(out.code);
        }
    };

    if require_signature && let Err(code) = require_signature_gate(file, output_json) {
        return RunVerdict::bare(code);
    }
    // ── `--task` scope + clean/skills gates + `--var` overrides ─────
    let (wf, report, skills) =
        match scoped_clean_gate(wf, report, task_filter, file, json, theme, output_json) {
            Ok(triple) => triple,
            Err(code) => return RunVerdict::bare(code),
        };
    let inputs = match inputs::validated_var_overrides(vars, &wf, output_json) {
        Ok(map) => map,
        Err(code) => return RunVerdict::bare(code),
    };

    // ── Dry-run (spec §10 · "plan only · zero effects") ─────────────
    if dry_run {
        return RunVerdict::bare(dry_run_verdict(file, &wf, &report, json, theme));
    }

    // ── `--max-cost-usd` preflight — BEFORE any spend (budget.rs) ──
    if let Err(code) = budget::preflight(&wf, &report, model_override, max_cost_usd, output_json) {
        return RunVerdict::bare(code);
    }

    // ── `--resume` / `--answer` (ADR-099) — plan + answers up front ──
    let setup = match resume_setup(resume, &wf, output_json) {
        Ok(setup) => setup,
        Err(code) => return RunVerdict::bare(code),
    };

    // ── Compose the production runtime (real seams · env keys) ──────
    let runtime = match composed_runtime(
        &wf,
        (file, &source),
        model_override,
        inputs,
        setup,
        json || output_json, // ADR-099 pause rider: NON-INTERACTIVE surfaces only
        max_cost_usd,
        skills,
        (no_trace_file, output_json),
    ) {
        Ok(rt) => rt,
        Err(code) => return RunVerdict::bare(code),
    };

    // ── Execute (block the async run on a current-thread executor) ──
    let rt = match executor(output_json) {
        Ok(rt) => rt,
        Err(code) => return RunVerdict::bare(code),
    };
    rt.block_on(execute(
        &runtime,
        (file, &wf),
        &report,
        json,
        output_json,
        theme,
        mode,
        resume.is_some_and(|r| r.trace.is_some()),
        trace_sink(no_trace_file),
        !no_outputs,
        model_override,
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

/// The validated `--resume`/`--answer` inputs the composition consumes.
struct ResumeSetup {
    /// The folded skip plan (`None` = no `--resume` requested).
    plan: Option<ResumePlan>,
    /// The validated `--answer task=value` map (empty without answers).
    answers: BTreeMap<String, Value>,
    /// The F-P4 resume authority (NEP-0013) — the approval ticket folded
    /// from the paused trace (`None` on a fresh run or a pre-F-P4 trace).
    paused: Option<nika_runtime::approval::PausedApproval>,
    /// The F-P21 declared compat (NEP-0014 law 4) — the recorded engine
    /// version the operator allowed the crossing from (`Some` only when
    /// a cross-version resume proceeds under `--resume-compat`).
    compat: Option<String>,
}

/// Validate + fold the whole `--resume` surface (plan · `--from` ·
/// `--answer`) BEFORE composing — every refusal is the ENV class,
/// already printed + enveloped.
///
/// # Errors
///
/// The exit code to return unchanged.
fn resume_setup(
    resume: Option<&ResumeRequest>,
    wf: &RawWorkflow,
    output_json: bool,
) -> Result<ResumeSetup, u8> {
    let (plan, paused, compat) = match resume {
        None => (None, None, None),
        Some(req) => match req.trace.as_deref() {
            // The answers-only form (F4): no trace, no plan — the answers
            // below ride into the gate map and wait for the ask.
            None => (None, None, None),
            Some(trace) => {
                let (plan, paused, compat) = load_resume_plan(req, trace, wf, output_json)?;
                (Some(plan), paused, compat)
            }
        },
    };
    let answers =
        nika_dap::resume::parse_answers(resume.map_or(&[][..], |r| r.answers.as_slice()), wf)
            .map_err(|message| {
                eprintln!("nika run: {message}");
                epilogue::emit_error_envelope(&message, output_json);
                exit::ENV
            })?;
    Ok(ResumeSetup {
        plan,
        answers,
        paused,
        compat,
    })
}

/// Read + fold the `--resume` trace into the runtime skip plan (ADR-099)
/// plus the F-P4 paused ticket (NEP-0013) plus the F-P21 version verdict
/// (NEP-0014 law 4). The cross-version judgment comes FIRST: a resume
/// under an engine different from the recording one is an explicit
/// refusal naming both versions — or rides a declared compat
/// (`--resume-compat` · attested on the run's boot manifest). Honest
/// degradation stays the contract for the KEYS: a keyless trace (older
/// engine) yields an EMPTY plan + a notice — never an error; an
/// unreadable file or an unknown `--from` id is refused loudly
/// (environment class).
///
/// # Errors
///
/// The exit code (already printed + enveloped) — ENV for every refusal.
fn load_resume_plan(
    req: &ResumeRequest,
    trace: &std::path::Path,
    wf: &RawWorkflow,
    output_json: bool,
) -> Result<
    (
        ResumePlan,
        Option<nika_runtime::approval::PausedApproval>,
        Option<String>,
    ),
    u8,
> {
    let label = trace.display().to_string();
    let refuse = |message: String| {
        eprintln!("nika run: {message}");
        epilogue::emit_error_envelope(&message, output_json);
        exit::ENV
    };
    let raw = std::fs::read_to_string(trace)
        .map_err(|e| refuse(format!("--resume: cannot read {label}: {e}")))?;
    let recovered =
        recover_events(&raw, &label).map_err(|message| refuse(format!("--resume: {message}")))?;
    if let Some(note) = &recovered.truncated_note {
        eprintln!("nika run: {note}");
    }
    // F-P21 (NEP-0014 law 4) — the version judgment BEFORE the fold:
    // judged, never assumed (the silent cross-version degradation dies).
    let judgment = nika_dap::resume::judge_version(&recovered.events, env!("CARGO_PKG_VERSION"));
    let compat = match nika_dap::resume::judge_resume(&judgment, req.compat.as_deref()) {
        nika_dap::resume::CompatVerdict::Proceed { compat_with } => {
            if let Some(recorded) = &compat_with {
                eprintln!(
                    "nika run: --resume: cross-version compat declared — the trace was \
                     recorded under engine {recorded}, this engine is {} (attested on \
                     the run's boot manifest)",
                    env!("CARGO_PKG_VERSION")
                );
            }
            compat_with
        }
        nika_dap::resume::CompatVerdict::Refuse(message) => {
            return Err(refuse(format!("--resume: {message}")));
        }
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future variant — enum and caller ship together; fail loud beats silently-wrong output"
        )]
        other => unreachable!("unknown compat verdict: {other:?}"),
    };
    let fold = nika_dap::resume::fold_plan(&recovered.events);
    if fold.plan.is_empty() {
        // Nothing skippable — an older engine's trace or a run with no
        // journaled successes. The run proceeds fully live (never an error).
        eprintln!("nika run: --resume: {label} carries no resume keys — running everything live");
    } else if fold.keyless + fold.unreadable > 0 {
        eprintln!(
            "nika run: --resume: {} record(s) without a usable resume key — those tasks run live",
            fold.keyless + fold.unreadable
        );
    }
    let mut plan = fold.plan;
    if let Some(from) = &req.from {
        nika_dap::resume::apply_from(&mut plan, wf, from)
            .map_err(|message| refuse(format!("--resume: {message}")))?;
    }
    Ok((plan, fold.paused, compat))
}

/// The `--require-signature` trust gate: verify against an enrolled key
/// before anything executes (exit 2 FILE · already printed + enveloped).
fn require_signature_gate(file: &str, output_json: bool) -> Result<(), u8> {
    use crate::seal::WorkflowSig;
    let reason = match crate::seal::check_workflow(std::path::Path::new(file)) {
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
        "nika run: {}\n  fix: check the path — `nika examples list` names runnable demos\n",
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

/// `--dry-run` (spec §10 · "plan only · zero effects"): the audit passed —
/// render the static plan (the same anatomy `nika inspect` renders) without
/// composing any production seam. No fs, no http, no subprocess, no
/// provider call — the run is never reached.
fn render_dry_run(file: &str, theme: Theme) -> u8 {
    let plan = crate::verbs::inspect::run(file, theme);
    if !plan.text.is_empty() {
        println!("{}", plan.text.trim_end());
    }
    println!("\n  dry-run · plan only · no effects executed");
    exit::OK
}

/// The dry-run fork: the human preview, or the #332 machine plan.
fn dry_run_verdict(
    file: &str,
    wf: &RawWorkflow,
    report: &nika_check::CheckReport,
    json: bool,
    theme: Theme,
) -> u8 {
    if json {
        dry_run_json(file, wf, report)
    } else {
        render_dry_run(file, theme)
    }
}

/// `--dry-run --json` (#332): ONE versioned plan object on stdout — what
/// the run WOULD do, projected from the SAME report the audit already
/// computed (waves resolved to task ids · per-task verb/model · the cost
/// ceiling · the affirmative permits · the caller requirements). CI and
/// PR renderers read this instead of composing `check --json` +
/// `explain --json` and reconstructing the plan client-side.
/// `plan_version` follows the check-report discipline: additive keys
/// never bump it.
fn dry_run_json(file: &str, wf: &RawWorkflow, report: &nika_check::CheckReport) -> u8 {
    println!("{:#}", dry_run_payload(file, wf, report));
    exit::OK
}

/// The pure projection behind [`dry_run_json`] (unit-pinned): waves
/// resolved from indices to task ids, one `{id, verb}` row per task,
/// and the report's own cost/permits/requirements objects verbatim.
fn dry_run_payload(
    file: &str,
    wf: &RawWorkflow,
    report: &nika_check::CheckReport,
) -> serde_json::Value {
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
    wf: &RawWorkflow,
    (file, source): (&str, &str),
    model_override: Option<&str>,
    inputs: inputs::ValidatedInputs,
    setup: ResumeSetup,
    pause_on_prompt: bool,
    max_cost_usd: Option<f64>,
    skills: BTreeMap<String, String>,
    (no_trace_file, output_json): (bool, bool),
) -> Result<ProdRuntime, u8> {
    let ResumeSetup {
        plan: resume_plan,
        answers,
        paused,
        compat,
    } = setup;
    let inputs::ValidatedInputs {
        values: overrides,
        origins,
    } = inputs;
    let envelope_model = wf.model.as_ref().map_or("", |m| m.value.as_str());
    let default_model = model_override.unwrap_or(envelope_model);
    let caps = capabilities_of(wf);
    // F-P3 · the run: declaration rides the SAME composition path (clock ·
    // jitter seed — the stamper half is picked at the drive site).
    match production_runtime(default_model, caps, wf.run.as_ref().map(|s| &s.value)) {
        Ok(rt) => {
            let rt = rt
                // The child seam (spec 14) — children resolve against THIS file.
                .with_child_runner(std::sync::Arc::new(child_runner::ProdChildRunner::new(
                    file,
                    !no_trace_file,
                )))
                .with_var_overrides(overrides)
                // F-P13 · the input origins (NEP-0014 law 2) — the boot
                // manifest journals where every bound input came from.
                .with_input_origins(origins)
                .with_max_cost_usd(max_cost_usd)
                .with_prompt_pause(pause_on_prompt)
                .with_prompt_answers(answers)
                // F-P4 · the folded resume authority (NEP-0013) — the
                // `--answer` validates against the shown ticket.
                .with_paused_approval(paused)
                // F-P21 · the declared cross-version compat (NEP-0014
                // law 4) — attested on the boot manifest.
                .with_resume_compat(compat)
                // #473 · composer-resolved SKILL.md texts (`## Skills`
                // injection + the referencing tasks' resume identity).
                .with_skills(skills)
                // Spec 14 law 10 (def_hash tier) · the child closure
                // digests join the calling tasks' resume identity — an
                // edited child (or grandchild) re-runs the call instead
                // of serving the old child's cached output (ADR-099
                // trap 6 across the file boundary).
                .with_child_closures(child_runner::closure_digests(
                    wf,
                    std::path::Path::new(file),
                ))
                // #409 · the override joins the resume identity of every
                // model-less infer/agent task (the model they RUN on).
                .with_model_override(model_override.map(ToOwned::to_owned))
                // The run's identity: the journal names the definition it
                // recorded (sha256 of the exact bytes this composer read).
                .with_source_sha256(sha256_hex(source.as_bytes()));
            // A CRLF/BOM source ALSO records its LF normal form, so drift
            // checks can tell a re-encode from an edit. LF sources skip
            // the field (the forms coincide — the journal stays lean).
            let raw_sha = sha256_hex(source.as_bytes());
            let lf_sha = sha256_hex(lf_normal_form(source).as_bytes());
            let rt = if lf_sha == raw_sha {
                rt
            } else {
                rt.with_source_sha256_lf(lf_sha)
            };
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
/// `outputs:` — the `nika test` seam (F7). The envelope model is replaced
/// by `mock/echo` through the SAME composition path as `--model` (offline ·
/// zero key · deterministic + schema-conformant since F3). The fold is a
/// DIAGNOSTIC here — it goes to stderr (verdict card on failure only), so
/// the caller owns stdout for its own verdict/diff surface. `skills` =
/// the composer-resolved SKILL.md texts (#473 · the caller gates their
/// findings first, same as `run`).
pub(crate) fn capture_mock_outputs(
    wf: &RawWorkflow,
    report: &CheckReport,
    skills: BTreeMap<String, String>,
    theme: Theme,
) -> Result<(u8, BTreeMap<String, Value>), String> {
    let caps = capabilities_of(wf);
    let runtime = production_runtime("mock/echo", caps, wf.run.as_ref().map(|s| &s.value))
        .map_err(|e| e.to_string())?;
    let runtime = runtime.with_skills(skills);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("cannot start the async executor: {e}"))?;
    Ok(rt.block_on(async {
        // F-P3 · the declaration picks the stamper too (seeded/none →
        // deterministic stamps — the test goldens stop drifting).
        let mut stamper = nika_runtime::RunSeams::of(wf.run.as_ref().map(|s| &s.value)).stamper();
        let mut sink = FoldSink::new(std::io::stderr().lock(), theme, RenderMode::Quiet);
        let (code, outcome) = drive(&runtime, wf, report, stamper.as_mut(), &mut sink).await;
        // Success is silent (the caller prints the test verdict); a failed
        // mock run surfaces its compact verdict card so the operator sees
        // WHY before the caller's exit.
        if code != exit::OK {
            sink.print_final();
        }
        (code, outcome.outputs)
    }))
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
    file: &str,
    json: bool,
    theme: Theme,
    output_json: bool,
) -> Result<(RawWorkflow, CheckReport, BTreeMap<String, String>), u8> {
    let refuse = || {
        let out = crate::verbs::check::run(file, json, false, None, theme);
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
    let resolved = crate::verbs::resolve_workflow_skills(&wf);
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

/// The static wave plan as task ids (the check report's schedule) —
/// injected into the display fold so the ∥ lane markers and the DAG-shape
/// glyph speak the scheduler's truth, not a reconstruction.
fn plan_waves(wf: &RawWorkflow, report: &CheckReport) -> Vec<Vec<String>> {
    report
        .waves
        .iter()
        .map(|wave| {
            wave.iter()
                .filter_map(|&i| wf.tasks.get(i).map(|t| t.value.id.value.clone()))
                .collect()
        })
        .collect()
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
    runtime: &ProdRuntime,
    (file, wf): (&str, &RawWorkflow),
    report: &CheckReport,
    json: bool,
    output_json: bool,
    theme: Theme,
    mode: RenderMode,
    resumed: bool,
    trace: TraceFileSink,
    outputs: bool,
    model_override: Option<&str>,
) -> RunVerdict {
    // F-P3 · the run: declaration picks the event-identity seam:
    // `entropy: none | seeded(N)` mints deterministic stamps (replayable
    // journals), ambient keeps the live `UUIDv7`+wall-clock stamper.
    let mut stamper = nika_runtime::RunSeams::of(wf.run.as_ref().map(|s| &s.value)).stamper();
    if output_json {
        // Machine-result mode (spec 01 §export · 08 §composition): the
        // fold is a DIAGNOSTIC → stderr (Plain deliberately — a pipe,
        // never the live redraw); the resolved `outputs:` object is the
        // ONE JSON object on stdout, never interleaved.
        let mut fold = FoldSink::new(std::io::stderr().lock(), theme, RenderMode::Plain);
        fold.set_plan(plan_waves(wf, report));
        let mut tee = Tee::new(fold, trace);
        let (code, outcome) = drive(runtime, wf, report, stamper.as_mut(), &mut tee).await;
        let (mut sink, trace) = tee.into_parts();
        sink.print_final();
        // F-P14 · la dette du run: a FAILED run quarantines its semi-written
        // outputs BEFORE the seal attests the end (None elsewhere — key OUT).
        let teardown = attended_facts(wf, report, &outcome, trace.path());
        let trace_path = surface_trace(
            trace,
            TraceNote::Stderr,
            None,
            seal_hash(wf).as_deref(),
            Some(&teardown),
        );
        // A paused run teaches its exact resume command on stderr — the
        // pause sibling of the failure lane's `autopsy:` line.
        if let (Some(p), Some(pause)) = (&trace_path, &outcome.paused) {
            eprintln!("nika run: {}", epilogue::resume_hint_line(file, p, pause));
        }
        epilogue::print_resume_summary(&outcome, resumed, true);
        // Built BEFORE the sink is consumed — the failure envelope reads
        // the folded view (the failed row's detail carries the wire code).
        // A PAUSED run gets its own additive envelope (`{"paused":{…}}` ·
        // ADR-099 rider · run state `paused` in the report vocabulary).
        let verdict_line = if code == exit::PAUSED {
            outcome.paused.as_ref().map(epilogue::paused_envelope_line)
        } else if code != exit::OK {
            Some(epilogue::run_failure_envelope(sink.view()))
        } else {
            None
        };
        if let Some(e) = sink.into_error() {
            let message = format!("render failed: {e}");
            eprintln!("nika run: {message}");
            println!("{}", epilogue::error_envelope_line(&message));
            return RunVerdict::bare(exit::ENV);
        }
        // stdout is ALWAYS one self-sufficient JSON object (F6): the
        // resolved `outputs:` on success · the `{"error":{…}}` envelope on
        // failure (it used to stay empty/`{}` — a machine consumer had to
        // scrape stderr to learn WHY) · the `{"paused":{…}}` envelope on a
        // human-gate pause.
        match verdict_line {
            Some(line) => println!("{line}"),
            None => println!("{}", epilogue::outputs_json_line(&outcome.outputs)),
        }
        RunVerdict {
            code,
            failure: first_failure(&outcome),
        }
    } else if json {
        execute_json_lane(
            runtime,
            (file, wf),
            report,
            stamper.as_mut(),
            resumed,
            trace,
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
            model_override,
        )
        .await
    }
}

/// The NDJSON machine lane (`--json`) — extracted whole (the fold-lane
/// precedent · the fn-length wall): stdout stays NDJSON verbatim
/// (byte-identical with or without the journal), the trace note rides
/// stderr.
async fn execute_json_lane(
    runtime: &ProdRuntime,
    (file, wf): (&str, &RawWorkflow),
    report: &CheckReport,
    stamper: &mut dyn Stamper,
    resumed: bool,
    trace: TraceFileSink,
) -> RunVerdict {
    let mut tee = Tee::new(JsonSink::new(std::io::stdout().lock()), trace);
    let (code, outcome) = drive(runtime, wf, report, stamper, &mut tee).await;
    let (sink, trace) = tee.into_parts();
    // F-P14 · the failure lane's quarantine runs BEFORE the seal.
    let teardown = attended_facts(wf, report, &outcome, trace.path());
    let trace_path = surface_trace(
        trace,
        TraceNote::Stderr,
        None,
        seal_hash(wf).as_deref(),
        Some(&teardown),
    );
    if let (Some(p), Some(pause)) = (&trace_path, &outcome.paused) {
        eprintln!("nika run: {}", epilogue::resume_hint_line(file, p, pause));
    }
    if let Some(e) = sink.into_error() {
        eprintln!("nika run: stream write failed: {e}");
        return RunVerdict::bare(exit::ENV);
    }
    epilogue::print_resume_summary(&outcome, resumed, true);
    RunVerdict {
        code,
        failure: first_failure(&outcome),
    }
}

/// The human fold lane (`Live` · `Plain` · `Quiet`) — extracted whole so
/// `execute` stays a lane DISPATCHER (fn-length ratchet · the three lanes
/// are peers, not one long body). Storytelling surfaces get the flow
/// epilogue + the spec §3.3 `trace:` pointer; `--quiet` keeps its
/// compact-card promise.
// The mode/resumed/outputs trio rides as one tuple — the same
// clap-surface idiom as `execute` itself (three independent switches).
#[allow(clippy::too_many_arguments)]
async fn execute_fold_lane(
    runtime: &ProdRuntime,
    wf: &RawWorkflow,
    report: &CheckReport,
    stamper: &mut dyn Stamper,
    file: &str,
    theme: Theme,
    (mode, resumed, outputs): (RenderMode, bool, bool),
    trace: TraceFileSink,
    model_override: Option<&str>,
) -> RunVerdict {
    let plan = plan_waves(wf, report);
    // The living map's topology — the SAME checked projection graph/
    // inspect trust; Live+accents only (the sink gates again).
    let map = (mode == RenderMode::Live && theme.accents)
        .then(|| (super::graph::project(wf, report), report.waves.clone()));
    let (fold, spinner) = shared_fold(theme, mode, outputs, plan.clone(), map);
    // #321 — the plain lane's stderr liveness rider (`still running ·
    // <task> · <n>s · <model>` every ~10s): a piped local-model run
    // must never read as a hang. Plain ONLY — Live already repaints ·
    // Quiet promised compactness · the machine lanes stream NDJSON. An
    // inert handle keeps one code path (the disabled-journal idiom).
    let pulse = (mode == RenderMode::Plain).then(|| {
        // The EFFECTIVE default model (the same substitution
        // composed_runtime made): the beat's static labels must name
        // what will actually resolve, `--model` override included.
        let default_model =
            model_override.unwrap_or_else(|| wf.model.as_ref().map_or("", |m| m.value.as_str()));
        heartbeat::shared(plan, heartbeat::task_labels(wf, default_model))
    });
    let ticker = pulse.clone().map(heartbeat::spawn_ticker);
    let beat = heartbeat::HeartbeatSink::new(pulse);
    let mut tee = Tee::new(
        Tee::new(sink::FoldHandle(std::sync::Arc::clone(&fold)), beat),
        trace,
    );
    let (code, outcome) = drive(runtime, wf, report, stamper, &mut tee).await;
    // The run settled — the riders must not speak over the epilogue.
    // (Aborted tasks reap at the executor's leisure; the fold lock below
    // is contention-free regardless — same thread, sync section.)
    if let Some(ticker) = &ticker {
        ticker.abort();
    }
    if let Some(spinner) = &spinner {
        spinner.abort();
    }
    let (_inner, trace) = tee.into_parts();
    let Ok(mut sink) = fold.lock() else {
        // A poisoned fold = a render-side panic already reported by the
        // runtime; the verdict must still leave honestly.
        eprintln!("nika run: render state poisoned");
        return RunVerdict::bare(exit::ENV);
    };
    // `Live` painted in place during the run; `Plain`/`Quiet` folded
    // silently · print the ONE final frame now.
    if mode != RenderMode::Live {
        sink.print_final();
    }
    // The Live (TTY) final frame carries the flow epilogue: the wall-
    // time waterfall + the outputs pointer (design §2c). The sober
    // registers stay untouched — CI logs never grow chart art.
    if mode == RenderMode::Live {
        epilogue::print_flow_epilogue(sink.view(), &outcome.outputs, theme, file);
    }
    // The spec §3.3 final-frame pointer (`trace: …`) — under the frame
    // on the storytelling surfaces.
    let failed_task = sink
        .view()
        .rows()
        .iter()
        .find(|r| r.state == crate::TaskState::Failed)
        .map(|r| r.id.clone());
    // F-P14 · the failure lane's quarantine runs BEFORE the seal.
    let teardown = attended_facts(wf, report, &outcome, trace.path());
    let _ = surface_trace(
        trace,
        if mode == RenderMode::Quiet {
            TraceNote::Silent
        } else {
            TraceNote::Stdout
        },
        failed_task.as_deref(),
        seal_hash(wf).as_deref(),
        Some(&teardown),
    );
    epilogue::print_resume_summary(&outcome, resumed, false);
    if let Some(e) = sink.take_error() {
        eprintln!("nika run: render failed: {e}");
        return RunVerdict::bare(exit::ENV);
    }
    RunVerdict {
        code,
        failure: first_failure(&outcome),
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
async fn drive<S, T, H, P, D, C>(
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
    match runtime.run(wf, report, stamper, sink).await {
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
            if let RuntimeError::MissingRequiredInputs { .. } = err {
                let _ = writeln!(stderr, "nika run: {err}");
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
fn trace_sink(no_trace_file: bool) -> TraceFileSink {
    if no_trace_file {
        TraceFileSink::disabled()
    } else {
        TraceFileSink::new(nika_dap::store::TRACE_DIR)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::collections::BTreeMap;

    use super::{RenderMode, capture_mock_outputs, dry_run_payload, exit, run};
    use crate::Theme;
    use serde_json::json;

    /// The #332 plan object: waves resolve indices → task ids, one
    /// `{id, verb}` row per task, the report's cost/permits/requirements
    /// ride verbatim, and `effects_executed` states the contract.
    #[test]
    fn dry_run_payload_projects_the_versioned_plan() {
        let yaml = "nika: v1\nworkflow:\n  id: demo\nmodel: mock/echo\ntasks:\n  a:\n    exec: { command: [\"echo\", \"x\"] }\n  b:\n    with:\n      prev: ${{ tasks.a.output }}\n    infer: { prompt: \"go ${{ with.prev }}\", max_tokens: 10 }\n\noutputs:\n  out: \"${{ tasks.b.output }}\"\n";
        let wf = nika_schema::parse(
            yaml,
            nika_schema::source::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        let report = nika_check::check(&wf);
        let p = dry_run_payload("demo.nika.yaml", &wf, &report);
        assert_eq!(p["plan_version"], 1);
        assert_eq!(p["workflow"], "demo");
        assert_eq!(p["waves"], json!([["a"], ["b"]]));
        assert_eq!(p["tasks"][0]["verb"], "exec");
        assert_eq!(p["tasks"][1]["verb"], "infer");
        assert_eq!(p["effects_executed"], false);
        assert_eq!(p["permits"]["source"], "absent");
        assert!(p["cost"].is_object() && p["requirements"].is_object());
    }

    /// The empty-state voice (design §3 rider): an ENV refusal (missing
    /// file) carries the house prefix + ONE fix line + a closing
    /// newline; FILE findings pass through to the check card untouched.
    #[test]
    fn refusal_text_teaches_only_the_env_class() {
        let env = crate::verbs::VerbOutput {
            text: "cannot read demo.yaml: No such file or directory (os error 2)".to_owned(),
            code: exit::ENV,
        };
        let voiced = super::refusal_text(&env);
        assert!(
            voiced.starts_with("nika run: cannot read demo.yaml"),
            "{voiced}"
        );
        assert!(voiced.contains("fix: check the path"), "{voiced}");
        assert!(voiced.ends_with('\n'), "closes its own line: {voiced:?}");

        let findings = crate::verbs::VerbOutput {
            text: "PARSE X  [NIKA-PARSE-009] two verbs".to_owned(),
            code: exit::FILE,
        };
        assert_eq!(super::refusal_text(&findings), findings.text);
    }

    /// A noiseless theme (no colour · no animation) for the run tests — they
    /// exercise the COMPOSITION + exit code, not the render surface.
    fn plain_theme() -> Theme {
        Theme::new(false, true, false)
    }

    fn stage(name: &str, yaml: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("nika-cli-run-mod-tests");
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let path = dir.join(name);
        std::fs::write(&path, yaml).expect("fixture written");
        path
    }

    /// `--model mock/echo` on a workflow whose envelope is a LOCAL model
    /// resolves + runs to SUCCESS offline — mock/echo needs no provider, so
    /// the override is the offline-preview path the example tip suggests.
    #[test]
    fn model_override_runs_a_local_model_workflow_offline() {
        let wf = stage(
            "override-infer.nika.yaml",
            "nika: v1\nworkflow:\n  id: override-infer\nmodel: ollama/llama3.1\ntasks:\n  think:\n    infer: { prompt: \"hello\" }\n",
        );
        let code = run(
            &wf.to_string_lossy(),
            false,
            None,
            plain_theme(),
            RenderMode::Plain,
            false,
            Some("mock/echo"),
            &[],
            None,
            true, // tests never write .nika/traces (cwd hygiene)
            None, // whole-workflow runs (scoping has its own tests)
            false,
            None,
            true,
            false, // unsigned-tolerant (the signature gate has its own test)
        );
        assert_eq!(
            code,
            exit::OK,
            "the mock/echo override runs the local-model workflow offline"
        );
    }

    /// The override actually CHANGES the resolved model: the same workflow
    /// that needs a provider (`ollama/llama3.1`) succeeds because the
    /// override swapped in the keyless/networkless mock — proving the
    /// envelope model was not the one resolved.
    #[test]
    fn model_override_replaces_the_resolved_model() {
        let wf = stage(
            "override-swap.nika.yaml",
            "nika: v1\nworkflow:\n  id: override-swap\nmodel: ollama/llama3.1\ntasks:\n  ask:\n    infer: { prompt: \"bonjour\" }\n",
        );
        // With the override → mock/echo resolves with no provider → OK.
        let overridden = run(
            &wf.to_string_lossy(),
            false,
            None,
            plain_theme(),
            RenderMode::Plain,
            false,
            Some("mock/echo"),
            &[],
            None,
            true, // tests never write .nika/traces (cwd hygiene)
            None, // whole-workflow runs (scoping has its own tests)
            false,
            None,
            true,
            false, // unsigned-tolerant (the signature gate has its own test)
        );
        assert_eq!(
            overridden,
            exit::OK,
            "the override resolved mock/echo, not the envelope's ollama model"
        );
    }

    // ── `--answer` without `--resume` (the 2026-07-30 audit · F4) ───────

    /// The CI one-pass gate: a FRESH run with a pre-seeded answer
    /// consumes it at the gate and completes — no trace, no resume, no
    /// TTY. Before F4 the clap surface refused the pairing outright
    /// (`requires = "resume"`); the gate map always could consume it.
    #[test]
    fn answer_without_resume_preseeds_the_gate() {
        let wf = stage(
            "answer-fresh.nika.yaml",
            "nika: v1\nworkflow:\n  id: gated\npermits: { exec: [\"echo\"], tools: [\"nika:prompt\"] }\ntasks:\n  ask:\n    invoke: { tool: \"nika:prompt\", args: { mode: \"confirm\", message: \"ship?\" } }\n  done:\n    after: { ask: success }\n    exec: { command: [\"echo\", \"shipped\"] }\n",
        );
        let req = nika_dap::resume::ResumeRequest {
            trace: None, // the answers-only form — no plan, no paused ticket
            from: None,
            answers: vec!["ask=true".to_owned()],
            compat: None,
        };
        let code = run(
            &wf.to_string_lossy(),
            false,
            None,
            plain_theme(),
            RenderMode::Plain,
            false,
            None,
            &[],
            Some(&req),
            true, // tests never write .nika/traces (cwd hygiene)
            None, // whole-workflow runs (scoping has its own tests)
            false,
            None,
            true,
            false, // unsigned-tolerant (the signature gate has its own test)
        );
        assert_eq!(
            code,
            exit::OK,
            "the pre-seeded answer clears the gate on a fresh run"
        );
    }

    /// The same pairing still validates the answer keys against the
    /// workflow — an unknown task id refuses at admission (the parse
    /// never relaxes with the new form).
    #[test]
    fn answer_without_resume_still_refuses_an_unknown_task() {
        let wf = stage(
            "answer-unknown.nika.yaml",
            "nika: v1\nworkflow:\n  id: gated\npermits: { tools: [\"nika:prompt\"] }\ntasks:\n  ask:\n    invoke: { tool: \"nika:prompt\", args: { mode: \"confirm\", message: \"ship?\" } }\n",
        );
        let req = nika_dap::resume::ResumeRequest {
            trace: None,
            from: None,
            answers: vec!["ghost=true".to_owned()],
            compat: None,
        };
        let code = run(
            &wf.to_string_lossy(),
            false,
            None,
            plain_theme(),
            RenderMode::Plain,
            false,
            None,
            &[],
            Some(&req),
            true,
            None,
            false,
            None,
            true,
            false,
        );
        assert_eq!(
            code,
            exit::ENV,
            "an answer for a task that does not exist refuses at admission"
        );
    }

    // ── `--var` (F4) — the required-var class was UNRUNNABLE from the CLI ──

    /// The workflow of the field repro: a `required: true` var with no
    /// default. Before F4 there was NO way to run it from the CLI.
    const REQUIRED_VAR_WF: &str = "nika: v1\nworkflow:\n  id: needs-var\nmodel: mock/echo\ninputs:\n  topic:\n    type: string\n    required: true\ntasks:\n  ask:\n    infer: { prompt: \"about ${{ inputs.topic }}\" }\n";

    fn run_with_vars(name: &str, vars: &[String]) -> u8 {
        let wf = stage(name, REQUIRED_VAR_WF);
        run(
            &wf.to_string_lossy(),
            false,
            None,
            plain_theme(),
            RenderMode::Plain,
            false,
            None,
            vars,
            None,
            true, // tests never write .nika/traces (cwd hygiene)
            None, // whole-workflow runs (scoping has its own tests)
            false,
            None,
            true,
            false, // unsigned-tolerant (the signature gate has its own test)
        )
    }

    #[test]
    fn var_flag_satisfies_a_required_var() {
        // Without the flag the run refuses at ADMISSION (issue #603 ·
        // NIKA-1708 · exit 3) — before the DAG spends a task; the mid-DAG
        // NIKA-VAR-001 at the first `${{ inputs.topic }}` read was the bug.
        assert_eq!(
            run_with_vars("var-missing.nika.yaml", &[]),
            exit::ENV,
            "an unsatisfied required input refuses at admission (#603)"
        );
        // With `--var topic=rust` the SAME workflow runs green.
        assert_eq!(
            run_with_vars("var-provided.nika.yaml", &["topic=rust".to_owned()]),
            exit::OK,
            "--var makes the required-var workflow runnable"
        );
    }

    #[test]
    fn var_flag_refuses_unknown_keys_and_bad_shapes() {
        // A typo'd key must refuse LOUDLY (exit 3 · never silently ignored).
        assert_eq!(
            run_with_vars("var-unknown.nika.yaml", &["topik=rust".to_owned()]),
            exit::ENV,
            "unknown --var key is refused"
        );
        // A pair without `=` is an operator input error, same class.
        assert_eq!(
            run_with_vars("var-shape.nika.yaml", &["topic".to_owned()]),
            exit::ENV,
            "malformed --var pair is refused"
        );
    }

    /// #473 e2e (mock · offline): the resolved-skills wiring is
    /// LOAD-BEARING through the production composition — the same
    /// skills-carrying agent workflow settles GREEN when the composer's
    /// map rides `with_skills`, and fails with the check-time code when
    /// an embedder skips it (the wiring, proven from the CLI seam; the
    /// injected system BYTES are pinned at the runtime's provider seam).
    #[test]
    fn capture_mock_outputs_carries_the_resolved_skills() {
        // Uniqueness: pid + an atomic discriminator — a pid-only dir is
        // shared by EVERY test in the process and parallel tests collide
        // (one's cleanup wiped the other's fixture under gate load — the
        // 2026-07-21 NIKA-AGENT-003 flake).
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "nika-run-skills-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let skill = dir.join("SKILL.md");
        std::fs::write(&skill, "---\nname: s\ndescription: d\n---\nBe careful.\n")
            .expect("fixture skill");
        let yaml = format!(
            "nika: v1\nworkflow:\n  id: w\nmodel: mock/echo\ntasks:\n  go:\n    agent: {{ prompt: \"hi\", skills: [\"{}\"] }}\n",
            skill.display()
        );
        let wf = nika_schema::parse(
            &yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        let report = nika_check::check(&wf);
        assert!(report.is_clean(), "the pure ladder is fs-free");

        let resolved = crate::verbs::resolve_workflow_skills(&wf);
        assert!(resolved.findings.is_empty(), "the skill file resolves");
        let theme = Theme::new(false, true, false);
        let (code, _) = capture_mock_outputs(&wf, &report, resolved.texts, theme)
            .expect("composition succeeds");
        assert_eq!(code, exit::OK, "skills composed → the mock run is green");

        // The control: WITHOUT the map the dispatch refuses (proves the
        // seam is load-bearing, not decorative).
        let (code, _) = capture_mock_outputs(&wf, &report, BTreeMap::new(), theme)
            .expect("composition still succeeds");
        assert_eq!(
            code,
            exit::WORKFLOW,
            "no skills map → NIKA-AGENT-003 task failure"
        );
    }

    /// `--require-signature` refuses an unsigned workflow BEFORE any task
    /// executes: exit 2, and the exec task's sentinel is never created.
    /// The counterfactual (the file itself is runnable) rides a dry-run —
    /// plan only, zero effects.
    #[test]
    fn require_signature_refuses_unsigned_before_execution() {
        let sentinel =
            std::env::temp_dir().join(format!("nika-sig-gate-sentinel-{}", std::process::id()));
        let _ = std::fs::remove_file(&sentinel);
        let yaml = format!(
            "nika: v1\nworkflow:\n  id: sig-gate\nmodel: mock/echo\npermits: {{ exec: [\"touch\"] }}\ntasks:\n  touch:\n    exec: {{ command: [\"touch\", \"{}\"] }}\n",
            sentinel.display()
        );
        let wf = stage("sig-gate.nika.yaml", &yaml);
        let gated = run(
            &wf.to_string_lossy(),
            false,
            None,
            plain_theme(),
            RenderMode::Plain,
            false,
            None,
            &[],
            None,
            true, // tests never write .nika/traces (cwd hygiene)
            None,
            false,
            None,
            true,
            true, // --require-signature
        );
        assert_eq!(
            gated,
            exit::FILE,
            "unsigned + --require-signature must refuse (exit 2)"
        );
        assert!(
            !sentinel.exists(),
            "the gate fired BEFORE execution — the exec task never ran"
        );
        // The counterfactual: the SAME file without the flag plans green.
        let planned = run(
            &wf.to_string_lossy(),
            false,
            None,
            plain_theme(),
            RenderMode::Plain,
            true, // --dry-run: plan only, zero effects
            None,
            &[],
            None,
            true,
            None,
            false,
            None,
            true,
            false, // unsigned-tolerant default
        );
        assert_eq!(planned, exit::OK, "the workflow itself is runnable");
    }
}
