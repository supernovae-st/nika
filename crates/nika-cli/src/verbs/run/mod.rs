// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika run <file>` — execute a CHECKED workflow through the real L3
//! runtime (spec §3 · exit 0 ok · 1 workflow failed · 2 file findings).
//!
//! The composer: this module is the L4 half that the runtime's seams
//! (`Stamper` · `EventSink`) and the verb crates' generics expect. It
//! wires PRODUCTION effects (real fs · http · clock · subprocess ·
//! provider registry with env-resolved keys) and the display fold.
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

mod compose;
mod inputs;
pub(crate) use compose::config_from_env;
mod resume;
mod sink;
mod stamp;

pub use compose::{
    ProdRuntime, RuntimeCapabilities, capabilities_of, fs_boundary_of, net_boundary_of,
    production_runtime,
};
pub use nika_dap::recover::{RecoveredTrace, recover_events};
pub use resume::ResumeRequest;
pub use sink::{FoldSink, JsonSink, RenderMode};
use sink::{TraceNote, surface_trace};
pub use stamp::SystemStamper;

mod budget;
mod epilogue;
mod heartbeat;
mod scope;
pub(crate) use nika_dap::source_id::{lf_normal_form, sha256_hex};
use scope::scope_to_task;

use sink::{TRACE_DIR, Tee, TraceFileSink};

use std::collections::BTreeMap;
use std::io::Write as _;

use serde_json::Value;

use nika_runtime::resume::ResumePlan;
use nika_runtime::{EventSink, RunOutcome, Runtime, Stamper};
use nika_schema::check::CheckReport;
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

// Fourteen independent CLI parameters ARE the clap surface — the same idiom
// as TraceArgs' bools, not a state machine to encode in a struct.
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
) -> RunVerdict {
    // `--output` validated up front so an unknown format fails before any
    // work (machine-result mode · see `output_mode`).
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

    // ── `--task` scope + clean gate + `--var` overrides (pre-effect) ──
    let (wf, report) =
        match scoped_clean_gate(wf, report, task_filter, file, json, theme, output_json) {
            Ok(pair) => pair,
            Err(code) => return RunVerdict::bare(code),
        };
    let overrides = match inputs::validated_var_overrides(vars, &wf, output_json) {
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
    // ADR-099: the pause rider binds to the NON-INTERACTIVE machine
    // surfaces only — human TTY/plain keep the PROMPT-001 contract.
    let pause_on_prompt = json || output_json;

    // ── Compose the production runtime (real seams · env keys) ──────
    let runtime = match composed_runtime(
        &wf,
        &source,
        model_override,
        overrides,
        setup.plan,
        setup.answers,
        pause_on_prompt,
        max_cost_usd,
        output_json,
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
        resume.is_some(),
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
    let plan = match resume {
        None => None,
        Some(req) => Some(load_resume_plan(req, wf, output_json)?),
    };
    let answers = resume::parse_answers(resume.map_or(&[][..], |r| r.answers.as_slice()), wf)
        .map_err(|message| {
            eprintln!("nika run: {message}");
            epilogue::emit_error_envelope(&message, output_json);
            exit::ENV
        })?;
    Ok(ResumeSetup { plan, answers })
}

/// Read + fold the `--resume` trace into the runtime skip plan (ADR-099).
/// Honest degradation is the contract: a keyless trace (older engine)
/// yields an EMPTY plan + a notice — never an error; an unreadable file
/// or an unknown `--from` id is refused loudly (environment class).
///
/// # Errors
///
/// The exit code (already printed + enveloped) — ENV for every refusal.
fn load_resume_plan(
    req: &ResumeRequest,
    wf: &RawWorkflow,
    output_json: bool,
) -> Result<ResumePlan, u8> {
    let label = req.trace.display().to_string();
    let refuse = |message: String| {
        eprintln!("nika run: {message}");
        epilogue::emit_error_envelope(&message, output_json);
        exit::ENV
    };
    let raw = std::fs::read_to_string(&req.trace)
        .map_err(|e| refuse(format!("--resume: cannot read {label}: {e}")))?;
    let recovered =
        recover_events(&raw, &label).map_err(|message| refuse(format!("--resume: {message}")))?;
    if let Some(note) = &recovered.truncated_note {
        eprintln!("nika run: {note}");
    }
    let fold = resume::fold_plan(&recovered.events);
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
        resume::apply_from(&mut plan, wf, from)
            .map_err(|message| refuse(format!("--resume: {message}")))?;
    }
    Ok(plan)
}

/// `nika examples run <slug>` — execute one EMBEDDED example through the
/// real runtime (the pack ships offline · zero network for the exec/
/// mock-model examples). Stages the embedded YAML to a temp file (the
/// verb reads a path) and runs it.
///
/// `model_override` — `Some(m)` (from `--model m`) swaps the example's
/// envelope model for `m` (so `--model mock/echo` previews offline). On a
/// FAILED run with NO override, a rescue tip keyed on the failure KIND is
/// printed to stderr (#145: the offline-model nudge for infer/provider
/// failures · the real missing dependency for an exec `program not
/// found`) · the original exit code is returned unchanged.
#[must_use]
pub fn example(
    slug: &str,
    model_override: Option<&str>,
    vars: &[String],
    quiet: bool,
    theme: Theme,
) -> u8 {
    let Some(yaml) = nika_pack::example(slug) else {
        eprintln!("unknown example `{slug}` — `nika examples list` names the embedded set");
        return exit::FILE;
    };
    // The slug comes from the embedded set (path-safe) · stage it beside
    // a stable name so a re-run overwrites rather than litters. A slug may
    // carry a `showcase/` prefix — flatten the separator so the temp name
    // stays a single path component.
    let stem = slug.replace('/', "-");
    let path = std::env::temp_dir().join(format!("nika-example-{stem}.nika.yaml"));
    if let Err(e) = std::fs::write(&path, yaml) {
        eprintln!("nika run: environment: cannot stage example `{slug}`: {e}");
        return exit::ENV;
    }
    // The example renders live on a TTY, plain when piped, silent on
    // `--quiet` (the verdict line still lands).
    let mode = if quiet {
        RenderMode::Quiet
    } else if std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        RenderMode::Live
    } else {
        RenderMode::Plain
    };
    if mode == RenderMode::Live {
        example_predisplay(slug, yaml, theme);
    }
    // The interactive duration accents follow the same TTY gate; heat
    // additionally needs colour + the truecolor proof.
    let mut theme = theme;
    theme.accents = mode == RenderMode::Live;
    theme.heat = theme.accents && theme.color && crate::verbs::truecolor_env();
    let verdict = run_verdict(
        &path.to_string_lossy(),
        false,
        None,
        theme,
        mode,
        false,
        model_override,
        vars,
        None,
        // No run journal: the example is staged to a TEMP file — `.nika/
        // traces/` belongs to workspace runs (the same drive underneath,
        // deliberately disabled here).
        true,
        // Examples always run whole (tiny by design · no scoping surface).
        None,
        false,
        None,
        // An example runs a TEMP-staged file, not a workspace run — the
        // workspace's trace store is not this invocation's to collect.
        true,
    );
    // The example's own envelope model — what we suggest overriding when a
    // run fails offline. A parse miss leaves it empty (the infer tip then
    // never fires · the run already surfaced the real finding).
    let model = example_model(yaml);
    if let Some(tip) = example_tip(slug, &verdict, model_override.is_some(), &model) {
        eprintln!("\n  {tip}");
    }
    verdict.code
}

/// The pre-display (TTY only): the SOURCE before the run — an example
/// is a teaching artifact, and the lesson reads better before the
/// tokens than after. Dim-framed, verbatim (the comments ARE the
/// curriculum); pipes keep their exact bytes.
fn example_predisplay(slug: &str, yaml: &str, theme: Theme) {
    let file = format!(
        "{}.nika.yaml",
        slug.strip_suffix(".nika.yaml").unwrap_or(slug)
    );
    println!(
        "{} {} {}",
        theme.logo(),
        theme.paint(crate::display::theme::Role::Strong, &file),
        theme.paint(
            crate::display::theme::Role::Dim,
            "— the source, then the run"
        ),
    );
    // Trim the machine boilerplate (SPDX · schema modeline · their
    // trailing blank) — the lesson starts at the title comment.
    let mut started = false;
    for line in yaml.lines() {
        let t = line.trim_start_matches(['#', ' ']);
        if !started
            && (t.starts_with("SPDX") || t.starts_with("yaml-language-server") || t.is_empty())
        {
            continue;
        }
        started = true;
        println!(
            "  {} {line}",
            theme.paint(crate::display::theme::Role::Dim, "│")
        );
    }
    println!();
}

/// The example's envelope `model:` string (empty when the YAML has no
/// model or won't parse). Best-effort — drives only the offline-hint
/// decision, never the run itself.
fn example_model(yaml: &str) -> String {
    nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .ok()
    .and_then(|wf| wf.model.map(|m| m.value))
    .unwrap_or_default()
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
    report: &nika_schema::check::CheckReport,
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
fn dry_run_json(file: &str, wf: &RawWorkflow, report: &nika_schema::check::CheckReport) -> u8 {
    println!("{:#}", dry_run_payload(file, wf, report));
    exit::OK
}

/// The pure projection behind [`dry_run_json`] (unit-pinned): waves
/// resolved from indices to task ids, one `{id, verb}` row per task,
/// and the report's own cost/permits/requirements objects verbatim.
fn dry_run_payload(
    file: &str,
    wf: &RawWorkflow,
    report: &nika_schema::check::CheckReport,
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
// The 7 knobs ARE the composition surface (var overrides · resume plan ·
// answers · pause flag) — the same clap-surface idiom as `run` itself.
#[allow(clippy::too_many_arguments)]
fn composed_runtime(
    wf: &RawWorkflow,
    source: &str,
    model_override: Option<&str>,
    overrides: BTreeMap<String, Value>,
    resume_plan: Option<ResumePlan>,
    answers: BTreeMap<String, Value>,
    pause_on_prompt: bool,
    max_cost_usd: Option<f64>,
    output_json: bool,
) -> Result<ProdRuntime, u8> {
    let envelope_model = wf.model.as_ref().map_or("", |m| m.value.as_str());
    let default_model = model_override.unwrap_or(envelope_model);
    let caps = capabilities_of(wf);
    match production_runtime(default_model, caps) {
        Ok(rt) => {
            let rt = rt
                .with_var_overrides(overrides)
                .with_max_cost_usd(max_cost_usd)
                .with_prompt_pause(pause_on_prompt)
                .with_prompt_answers(answers)
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
        Err(e) => {
            eprintln!("nika run: environment: {e}");
            epilogue::emit_error_envelope(&e.to_string(), output_json);
            Err(exit::ENV)
        }
    }
}

/// Execute a CHECKED workflow with the MOCK provider and capture the typed
/// `outputs:` — the `nika test` seam (F7). The envelope model is replaced
/// by `mock/echo` through the SAME composition path as `--model` (offline ·
/// zero key · deterministic + schema-conformant since F3). The fold is a
/// DIAGNOSTIC here — it goes to stderr (verdict card on failure only), so
/// the caller owns stdout for its own verdict/diff surface.
///
/// # Errors
///
/// A composition/executor failure (environment class) as a human-readable
/// message — the caller maps it to `exit::ENV`.
pub(crate) fn capture_mock_outputs(
    wf: &RawWorkflow,
    report: &CheckReport,
    theme: Theme,
) -> Result<(u8, BTreeMap<String, Value>), String> {
    let caps = capabilities_of(wf);
    let runtime = production_runtime("mock/echo", caps).map_err(|e| e.to_string())?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("cannot start the async executor: {e}"))?;
    Ok(rt.block_on(async {
        let mut stamper = SystemStamper::new();
        let mut sink = FoldSink::new(std::io::stderr().lock(), theme, RenderMode::Quiet);
        let (code, outcome) = drive(&runtime, wf, report, &mut stamper, &mut sink).await;
        // Success is silent (the caller prints the test verdict); a failed
        // mock run surfaces its compact verdict card so the operator sees
        // WHY before the caller's exit.
        if code != exit::OK {
            sink.print_final();
        }
        (code, outcome.outputs)
    }))
}

/// The rescue tip under a FAILED example, keyed on the failure KIND
/// (#145 · the exit code alone misdirected: an example that carries BOTH
/// a model and exec tasks used to earn the mock-model nudge on a missing
/// binary — a swap that cannot conjure the program). `None` = say
/// nothing: success · pause · pre-run refusals · an explicit `--model`
/// override · failure classes neither a model swap nor an install would
/// rescue. Pure · so the policy is unit-tested without staging or
/// running anything.
#[must_use]
fn example_tip(
    slug: &str,
    verdict: &RunVerdict,
    override_given: bool,
    model: &str,
) -> Option<String> {
    if verdict.code == exit::OK || override_given {
        return None;
    }
    let failure = verdict.failure.as_ref()?;
    // Infer/provider failures — the "no local model running" case: the
    // offline preview is one flag away (the funnel's highest-intent P0).
    if failure.code.starts_with("NIKA-INFER-") {
        if model.is_empty() || model == "mock/echo" {
            return None;
        }
        return Some(format!(
            "tip: no local model running? preview this example offline →\n        nika examples run {slug} --model mock/echo"
        ));
    }
    // A missing program — name the REAL dependency (the ✖ line above
    // carries the code; this states the way out).
    if failure.code == "NIKA-EXEC-002" {
        let program = failure
            .message
            .split("program not found: ")
            .nth(1)
            .map(str::trim)
            .filter(|p| !p.is_empty());
        return Some(match program {
            Some(p) => format!(
                "tip: this example shells out to `{p}` — not found on this machine;\n        install it, or browse offline-friendly examples → nika examples list"
            ),
            None => "tip: this example shells out to a program this machine does not \
                     have\n        (the ✖ line names it) — install it, or try → nika examples list"
                .to_owned(),
        });
    }
    None
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
) -> Result<(RawWorkflow, CheckReport), u8> {
    if !report.is_clean() {
        let out = crate::verbs::check::run(file, json, false, None, theme);
        epilogue::emit_diagnostic(&out.text, output_json);
        return Err(out.code);
    }
    let (wf, report) = apply_task_scope(wf, report, task_filter, output_json)?;
    if !report.is_clean() {
        let out = crate::verbs::check::run(file, json, false, None, theme);
        epilogue::emit_diagnostic(&out.text, output_json);
        return Err(out.code);
    }
    Ok((wf, report))
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
            let sub_report = nika_schema::check(&sub);
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
// The 8th parameter is the `--resume` summary switch — same clap-surface
// The trailing parameters are the `--resume` summary switch + the
// outputs-tail switch — same clap-surface idiom as `run` itself (four
// independent flags ARE four bools, not a state machine). The workflow
// rides as (path, parsed) — the epilogue hint teaches a command over
// the SAME file the operator just ran.
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
    let mut stamper = SystemStamper::new();
    if output_json {
        // Machine-result mode (spec 01 §export contract): the live fold is
        // a DIAGNOSTIC → stderr; the resolved `outputs:` object is the ONE
        // JSON object on stdout, never interleaved. This powers the v0.1
        // sub-workflow composition `exec: nika run sub.yaml --output json`
        // + `capture: stdout` (spec 08 §composition).
        // `Plain` deliberately: the fold goes to stderr (a pipe in the
        // composition path · never the live TTY redraw); the one final
        // storyboard frame is what matters, not the animation.
        let mut fold = FoldSink::new(std::io::stderr().lock(), theme, RenderMode::Plain);
        fold.set_plan(plan_waves(wf, report));
        let mut tee = Tee::new(fold, trace);
        let (code, outcome) = drive(runtime, wf, report, &mut stamper, &mut tee).await;
        let (mut sink, trace) = tee.into_parts();
        sink.print_final();
        let trace_path = surface_trace(trace, TraceNote::Stderr, None);
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
        let mut tee = Tee::new(JsonSink::new(std::io::stdout().lock()), trace);
        let (code, outcome) = drive(runtime, wf, report, &mut stamper, &mut tee).await;
        let (sink, trace) = tee.into_parts();
        // stdout stays NDJSON verbatim (byte-identical with or without the
        // journal) — the trace note rides on stderr here.
        let trace_path = surface_trace(trace, TraceNote::Stderr, None);
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
    } else {
        execute_fold_lane(
            runtime,
            wf,
            report,
            &mut stamper,
            file,
            theme,
            (mode, resumed, outputs),
            trace,
            model_override,
        )
        .await
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
    stamper: &mut SystemStamper,
    file: &str,
    theme: Theme,
    (mode, resumed, outputs): (RenderMode, bool, bool),
    trace: TraceFileSink,
    model_override: Option<&str>,
) -> RunVerdict {
    let plan = plan_waves(wf, report);
    let (fold, spinner) = shared_fold(theme, mode, outputs, plan.clone());
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
    let _ = surface_trace(
        trace,
        if mode == RenderMode::Quiet {
            TraceNote::Silent
        } else {
            TraceNote::Stdout
        },
        failed_task.as_deref(),
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
) -> (
    sink::SharedFold<std::io::Stdout>,
    Option<tokio::task::JoinHandle<()>>,
) {
    let mut fold = FoldSink::new(std::io::stdout(), theme, mode);
    fold.set_plan(plan);
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
/// A `RuntimeError` on a CLEAN report is a SYSTEM contract breach (the
/// checker proved it clean · the runtime should never reject it) — exit
/// 3 with the wire code, never a panic (the zero-unwrap policy).
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
            let _ = writeln!(
                stderr,
                "nika run: system: the checked workflow was rejected at run \
                 time ({} · {err}) — this is an engine contract breach, \
                 please report it",
                err.nika_code()
            );
            (
                exit::ENV,
                RunOutcome::new(false, BTreeMap::new(), BTreeMap::new()),
            )
        }
    }
}

/// The run journal (spec §3.3) — composed like the other seams;
/// `execute` receives the sink, never a flag.
fn trace_sink(no_trace_file: bool) -> TraceFileSink {
    if no_trace_file {
        TraceFileSink::disabled()
    } else {
        TraceFileSink::new(TRACE_DIR)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::{RenderMode, RunVerdict, dry_run_payload, example_tip, exit, run, scope_to_task};
    use crate::Theme;
    use serde_json::json;

    /// The #332 plan object: waves resolve indices → task ids, one
    /// `{id, verb}` row per task, the report's cost/permits/requirements
    /// ride verbatim, and `effects_executed` states the contract.
    #[test]
    fn dry_run_payload_projects_the_versioned_plan() {
        let yaml = "nika: v1\nworkflow: demo\nmodel: mock/echo\ntasks:\n  - id: a\n    exec: { command: [\"echo\", \"x\"] }\n  - id: b\n    depends_on: [a]\n    infer: { prompt: \"go ${{ tasks.a.output }}\", max_tokens: 10 }\n\noutputs:\n  out: \"${{ tasks.b.output }}\"\n";
        let wf = nika_schema::parse(
            yaml,
            nika_schema::source::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        let report = nika_schema::check(&wf);
        let p = dry_run_payload("demo.nika.yaml", &wf, &report);
        assert_eq!(p["plan_version"], 1);
        assert_eq!(p["workflow"], "demo");
        assert_eq!(p["waves"], json!([["a"], ["b"]]));
        assert_eq!(p["tasks"][0]["verb"], "exec");
        assert_eq!(p["tasks"][1]["verb"], "infer");
        assert_eq!(p["effects_executed"], false);
        assert_eq!(p["permits"]["source"], "floor");
        assert!(p["cost"].is_object() && p["requirements"].is_object());
    }

    /// A failed verdict carrying one typed task error (the policy's input).
    fn failed(code: &str, message: &str) -> RunVerdict {
        RunVerdict {
            code: exit::WORKFLOW,
            failure: Some(nika_runtime::TaskErrorRecord {
                code: code.to_owned(),
                message: message.to_owned(),
                transient: false,
            }),
        }
    }

    /// The rescue-tip policy (pure · the heart of the UX decision) is
    /// keyed on the failure KIND (#145): only an infer/provider failure
    /// earns the offline-model nudge; a missing program names the real
    /// dependency instead of suggesting a model swap that cannot fix it.
    #[test]
    fn example_tip_keys_on_the_failure_kind() {
        let infer = failed("NIKA-INFER-001", "provider call failed: model not found");
        // FAIL on infer + no override + a local model → the right nudge.
        let tip = example_tip("01-hello", &infer, false, "ollama/llama3.1")
            .expect("the infer failure earns the offline nudge");
        assert!(tip.contains("--model mock/echo"), "{tip}");
        assert!(tip.contains("01-hello"), "the retry names the slug: {tip}");
        // A clean run never needs the tip.
        let ok = RunVerdict::bare(exit::OK);
        assert!(example_tip("01-hello", &ok, false, "ollama/llama3.1").is_none());
        // The user already overrode the model · suggesting it again is noise.
        assert!(example_tip("01-hello", &infer, true, "ollama/llama3.1").is_none());
        // mock/echo needs no provider · a failure there is a real bug, not
        // a missing local model — so the offline tip would mislead.
        assert!(example_tip("01-hello", &infer, false, "mock/echo").is_none());
        // No envelope model (a parse miss) · the nudge would mislead.
        assert!(example_tip("01-hello", &infer, false, "").is_none());
    }

    /// THE misdirection pin (#145 operator finding): an exec `program not
    /// found` — even on an example that ALSO declares a model — must name
    /// the missing program, never the mock-model swap.
    #[test]
    fn example_tip_exec_failure_names_the_program_not_the_model() {
        let exec = failed("NIKA-EXEC-002", "program not found: cargo test");
        let tip = example_tip("03-exec-pipeline", &exec, false, "ollama/llama3.1")
            .expect("the missing program earns its own tip");
        assert!(tip.contains("`cargo test`"), "{tip}");
        assert!(
            !tip.contains("mock/echo"),
            "no model swap for a missing binary: {tip}"
        );
        // An unparseable exec message still teaches, generically.
        let vague = failed("NIKA-EXEC-002", "spawn refused");
        let tip = example_tip("03-exec-pipeline", &vague, false, "ollama/llama3.1")
            .expect("the exec class still explains itself");
        assert!(tip.contains("nika examples list"), "{tip}");
        assert!(!tip.contains("mock/echo"), "{tip}");
    }

    /// Failure classes neither a model swap nor an install would rescue
    /// (builtin errors · workflow-level breaches with no failed record)
    /// stay silent — a tip that cannot help is noise.
    #[test]
    fn example_tip_stays_silent_on_unrescuable_classes() {
        let builtin = failed("NIKA-BUILTIN-READ-001", "cannot read ./missing.json");
        assert!(example_tip("01-hello", &builtin, false, "ollama/llama3.1").is_none());
        // A workflow-level failure with no failed task record (typed-output
        // breach) carries nothing to key on — silence, not a guess.
        let bare_fail = RunVerdict::bare(exit::WORKFLOW);
        assert!(example_tip("01-hello", &bare_fail, false, "ollama/llama3.1").is_none());
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
            "nika: v1\nworkflow: override-infer\nmodel: ollama/llama3.1\ntasks:\n  - id: think\n    infer: { prompt: \"hello\" }\n",
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
            "nika: v1\nworkflow: override-swap\nmodel: ollama/llama3.1\ntasks:\n  - id: ask\n    infer: { prompt: \"bonjour\" }\n",
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
        );
        assert_eq!(
            overridden,
            exit::OK,
            "the override resolved mock/echo, not the envelope's ollama model"
        );
    }

    // ── `--var` (F4) — the required-var class was UNRUNNABLE from the CLI ──

    /// The workflow of the field repro: a `required: true` var with no
    /// default. Before F4 there was NO way to run it from the CLI.
    const REQUIRED_VAR_WF: &str = "nika: v1\nworkflow: needs-var\nmodel: mock/echo\nvars:\n  topic:\n    type: string\n    required: true\ntasks:\n  - id: ask\n    infer: { prompt: \"about ${{ vars.topic }}\" }\n";

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
        )
    }

    #[test]
    fn var_flag_satisfies_a_required_var() {
        // Without the flag the first `${{ vars.topic }}` reference fails
        // the task (NIKA-VAR-001) → workflow failed.
        assert_eq!(
            run_with_vars("var-missing.nika.yaml", &[]),
            exit::WORKFLOW,
            "an unbound required var still fails the run"
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

    /// `--task` scope · the diamond proves ancestors-only semantics: the
    /// target + transitive upstream survive · siblings and downstream drop
    /// · outputs clear (they may read unscoped tasks).
    #[test]
    fn scope_to_task_keeps_the_ancestor_cone() {
        let yaml = "nika: v1\nworkflow: diamond\nmodel: mock/echo\ntasks:\n  - id: discover\n    invoke: { tool: \"nika:glob\", args: { pattern: \"*.md\" } }\n  - id: stats\n    depends_on: [discover]\n    infer: { prompt: \"count ${{ tasks.discover.output }}\" }\n  - id: digest\n    depends_on: [discover]\n    infer: { prompt: \"sum ${{ tasks.discover.output }}\" }\n  - id: report\n    depends_on: [stats, digest]\n    infer: { prompt: \"merge ${{ tasks.stats.output }} ${{ tasks.digest.output }}\" }\noutputs:\n  all: ${{ tasks.report.output }}\n";
        let wf = nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("diamond parses");

        let stats_only = scope_to_task(wf.clone(), "stats").expect("stats scopes");
        let ids: Vec<&str> = stats_only
            .tasks
            .iter()
            .map(|t| t.value.id.value.as_str())
            .collect();
        assert_eq!(
            ids,
            vec!["discover", "stats"],
            "target + its one ancestor · document order"
        );
        assert!(stats_only.outputs.is_empty(), "outputs drop under scope");

        let full = scope_to_task(wf.clone(), "report").expect("report scopes");
        assert_eq!(full.tasks.len(), 4, "the sink's cone is the whole diamond");

        let err = scope_to_task(wf, "nope").expect_err("unknown id refused");
        assert!(
            err.contains("nope") && err.contains("discover"),
            "names the id + the available set"
        );
    }

    /// The scoped sub-workflow re-checks CLEAN — the plan/waves/cost the
    /// run renders describe exactly the cone, not the original file.
    #[test]
    fn scoped_workflow_rechecks_clean() {
        let yaml = "nika: v1\nworkflow: pair\nmodel: mock/echo\ntasks:\n  - id: a\n    infer: { prompt: \"hi\" }\n  - id: b\n    depends_on: [a]\n    infer: { prompt: \"use ${{ tasks.a.output }}\" }\n";
        let wf = nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("pair parses");
        let sub = scope_to_task(wf, "a").expect("a scopes");
        let report = nika_schema::check(&sub);
        assert!(
            report.is_clean(),
            "the cone stands alone (no dangling refs)"
        );
        assert_eq!(sub.tasks.len(), 1);
    }
}
