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
mod resume;
mod sink;
mod stamp;

pub use compose::{
    ProdRuntime, RuntimeCapabilities, capabilities_of, fs_boundary_of, net_boundary_of,
    production_runtime,
};
pub use resume::{RecoveredTrace, ResumeRequest, recover_events};
pub use sink::{FoldSink, JsonSink, RenderMode};
pub use stamp::SystemStamper;

mod scope;
use scope::scope_to_task;

use sink::{TRACE_DIR, Tee, TraceFileSink};

use std::collections::BTreeMap;
use std::io::Write as _;

use serde_json::Value;

use nika_runtime::resume::ResumePlan;
use nika_runtime::{EventSink, RunOutcome, Runtime, Stamper, WorkflowPause};
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
// Ten independent CLI parameters ARE the clap surface — the same idiom
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
) -> u8 {
    // `--output` validated up front so an unknown format fails before any
    // work (machine-result mode · see `output_mode`).
    let output_json = match output_mode(output) {
        Ok(flag) => flag,
        Err(code) => return code,
    };

    // ── Audit BEFORE run (spec §3 · INV the runtime also enforces) ──
    let (source, wf, report) = match crate::verbs::load_checked_with_source(file) {
        Ok(pair) => pair,
        Err(out) => {
            // Pre-run diagnostics obey the export contract too: in machine
            // mode they go to stderr so a `capture: stdout` consumer never
            // mistakes the "cannot read" text for the JSON result.
            emit_diagnostic(&refusal_text(&out), output_json);
            return out.code;
        }
    };

    // ── `--task` scope + clean gate + `--var` overrides (all before
    //    any effect — the whole operator-input preflight) ──
    let (wf, report) =
        match scoped_clean_gate(wf, report, task_filter, file, json, theme, output_json) {
            Ok(pair) => pair,
            Err(code) => return code,
        };
    let overrides = match validated_var_overrides(vars, &wf, output_json) {
        Ok(map) => map,
        Err(code) => return code,
    };

    // ── Dry-run (spec §10 · "plan only · zero effects") ─────────────
    if dry_run {
        return render_dry_run(file, theme.ascii);
    }

    // ── `--resume` / `--answer` (ADR-099) — plan + answers up front ──
    let setup = match resume_setup(resume, &wf, output_json) {
        Ok(setup) => setup,
        Err(code) => return code,
    };
    // ADR-099: the pause rider binds to the NON-INTERACTIVE machine
    // surfaces only (`--json` · `--output json`); human TTY/plain
    // surfaces keep today's PROMPT-001 contract untouched.
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
        output_json,
    ) {
        Ok(rt) => rt,
        Err(code) => return code,
    };

    // ── Execute (block the async run on a current-thread executor) ──
    let rt = match executor(output_json) {
        Ok(rt) => rt,
        Err(code) => return code,
    };
    // The run journal (spec §3.3) — composed HERE like the other seams;
    // `execute` receives the sink, never a flag.
    let trace = if no_trace_file {
        TraceFileSink::disabled()
    } else {
        TraceFileSink::new(TRACE_DIR)
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
        trace,
        !no_outputs,
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
            emit_error_envelope(&message, output_json);
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
            emit_error_envelope(&message, output_json);
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
        emit_error_envelope(&message, output_json);
        exit::ENV
    };
    let raw = std::fs::read_to_string(&req.trace)
        .map_err(|e| refuse(format!("--resume: cannot read {label}: {e}")))?;
    let recovered = resume::recover_events(&raw, &label)
        .map_err(|message| refuse(format!("--resume: {message}")))?;
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
/// FAILED run with NO override and a non-`mock/echo` model (the common
/// "no local provider running" case), an actionable offline hint is
/// printed to stderr · the original exit code is returned unchanged.
#[must_use]
pub fn example(slug: &str, model_override: Option<&str>, theme: Theme) -> u8 {
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
    // The example renders live on a TTY, plain when piped (no flags here).
    let mode = if std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        RenderMode::Live
    } else {
        RenderMode::Plain
    };
    // The interactive duration accents follow the same TTY gate; heat
    // additionally needs colour + the truecolor proof.
    let mut theme = theme;
    theme.accents = mode == RenderMode::Live;
    theme.heat = theme.accents && theme.color && crate::verbs::truecolor_env();
    let code = run(
        &path.to_string_lossy(),
        false,
        None,
        theme,
        mode,
        false,
        model_override,
        &[],
        None,
        // No run journal: the example is staged to a TEMP file — `.nika/
        // traces/` belongs to workspace runs (the same drive underneath,
        // deliberately disabled here).
        true,
        // Examples always run whole (tiny by design · no scoping surface).
        None,
        false,
    );
    // The example's own envelope model — what we suggest overriding when a
    // run fails offline. A parse miss leaves it empty (the tip then never
    // fires · the run already surfaced the real finding).
    let model = example_model(yaml);
    if offline_tip_applies(code, model_override.is_some(), &model) {
        eprintln!(
            "\n  tip: no local model running? preview this example offline →\n        nika examples run {slug} --model mock/echo"
        );
    }
    code
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
fn render_dry_run(file: &str, ascii: bool) -> u8 {
    let plan = crate::verbs::inspect::run(file, ascii);
    if !plan.text.is_empty() {
        println!("{}", plan.text.trim_end());
    }
    println!("\n  dry-run · plan only · no effects executed");
    exit::OK
}

/// Parse the repeatable `--var KEY=VALUE` overrides and validate every
/// key against the workflow's declared `vars:` — an unknown key is
/// refused with the declared set (a typo'd override silently doing
/// nothing would be the worst outcome). Values parse as JSON when they
/// parse (numbers · booleans · arrays · quoted strings), else ride as
/// plain strings: `--var topic=news` is the string `"news"`,
/// `--var limit=5` the number `5`.
fn parse_var_overrides(
    pairs: &[String],
    wf: &RawWorkflow,
) -> Result<BTreeMap<String, Value>, String> {
    let declared: Vec<&str> = wf.vars.iter().map(|(k, _)| k.value.as_str()).collect();
    let mut overrides = BTreeMap::new();
    for pair in pairs {
        let (key, raw) = match pair.split_once('=') {
            Some((k, v)) if !k.trim().is_empty() => (k.trim(), v),
            _ => return Err(format!("--var expects KEY=VALUE, got `{pair}`")),
        };
        if !declared.contains(&key) {
            return Err(if declared.is_empty() {
                format!("--var {key}: this workflow declares no `vars:`")
            } else {
                format!(
                    "--var {key}: unknown var — the workflow declares: {}",
                    declared.join(" · ")
                )
            });
        }
        let value =
            serde_json::from_str::<Value>(raw).unwrap_or_else(|_| Value::String(raw.to_owned()));
        overrides.insert(key.to_owned(), value);
    }
    Ok(overrides)
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
/// The run's source identity: sha256 hex over the exact bytes read.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

fn composed_runtime(
    wf: &RawWorkflow,
    source: &str,
    model_override: Option<&str>,
    overrides: BTreeMap<String, Value>,
    resume_plan: Option<ResumePlan>,
    answers: BTreeMap<String, Value>,
    pause_on_prompt: bool,
    output_json: bool,
) -> Result<ProdRuntime, u8> {
    let envelope_model = wf.model.as_ref().map_or("", |m| m.value.as_str());
    let default_model = model_override.unwrap_or(envelope_model);
    let caps = capabilities_of(wf);
    match production_runtime(default_model, caps) {
        Ok(rt) => {
            let rt = rt
                .with_var_overrides(overrides)
                .with_prompt_pause(pause_on_prompt)
                .with_prompt_answers(answers)
                // The run's identity: the journal names the definition it
                // recorded (sha256 of the exact bytes this composer read).
                .with_source_sha256(sha256_hex(source.as_bytes()));
            Ok(match resume_plan {
                Some(plan) => rt.with_resume_plan(plan),
                None => rt,
            })
        }
        Err(e) => {
            eprintln!("nika run: environment: {e}");
            emit_error_envelope(&e.to_string(), output_json);
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

/// Should the offline-preview tip fire after an example run?
///
/// True only when ALL hold: the run FAILED (`code != exit::OK`), the user
/// gave NO `--model` override (the tip suggests exactly that), and the
/// example's model is NOT already `mock/echo` (which needs no provider, so
/// a failure there is a real bug, not a missing local model). Pure · so
/// the policy is unit-tested without staging or running anything.
#[must_use]
fn offline_tip_applies(exit_code: u8, override_given: bool, model: &str) -> bool {
    // No envelope model → the failure cannot be "no local model running"
    // (a pure-exec example failing on a missing program would get a
    // misleading nudge — the mock override wouldn't change its outcome).
    exit_code != exit::OK && !override_given && !model.is_empty() && model != "mock/echo"
}

/// The static wave plan as task ids (the check report's schedule) —
/// injected into the display fold so the ∥ lane markers and the DAG-shape
/// glyph speak the scheduler's truth, not a reconstruction.
/// `--var` overrides (F4), parsed + validated BEFORE any work: an
/// unknown key refuses loudly (a typo'd override silently doing nothing
/// would be the worst outcome) · the operator-input exit class (3).
fn validated_var_overrides(
    vars: &[String],
    wf: &RawWorkflow,
    output_json: bool,
) -> Result<std::collections::BTreeMap<String, serde_json::Value>, u8> {
    parse_var_overrides(vars, wf).map_err(|message| {
        eprintln!("nika run: {message}");
        emit_error_envelope(&message, output_json);
        exit::ENV
    })
}

/// The `--task` scope + the clean gate, fused (both run before any
/// effect): scope to the target's ancestor cone when requested (the
/// sub-DAG re-checks so plan/waves/cost describe exactly what runs) ·
/// then refuse a dirty report with the SAME findings `nika check`
/// renders (locked rendering · exit 2 · stderr in machine mode).
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
    let (wf, report) = apply_task_scope(wf, report, task_filter, output_json)?;
    if !report.is_clean() {
        let out = crate::verbs::check::run(file, json, theme);
        emit_diagnostic(&out.text, output_json);
        return Err(out.code);
    }
    Ok((wf, report))
}

/// Apply the `--task` scope when requested (the regenerate-one-block
/// move). The FULL workflow audited already (whole-file spans · faithful
/// findings) — the sub-DAG RE-CHECKS here so the plan/waves/cost describe
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
            emit_diagnostic(&msg, output_json);
            Err(exit::ENV)
        }
    }
}

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
) -> u8 {
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
        surface_trace(trace, TraceNote::Stderr);
        print_resume_summary(&outcome, resumed, true);
        // Built BEFORE the sink is consumed — the failure envelope reads
        // the folded view (the failed row's detail carries the wire code).
        // A PAUSED run gets its own additive envelope (`{"paused":{…}}` ·
        // ADR-099 rider · run state `paused` in the report vocabulary).
        let verdict_line = if code == exit::PAUSED {
            outcome.paused.as_ref().map(paused_envelope_line)
        } else if code != exit::OK {
            Some(run_failure_envelope(sink.view()))
        } else {
            None
        };
        if let Some(e) = sink.into_error() {
            let message = format!("render failed: {e}");
            eprintln!("nika run: {message}");
            println!("{}", error_envelope_line(&message));
            return exit::ENV;
        }
        // stdout is ALWAYS one self-sufficient JSON object (F6): the
        // resolved `outputs:` on success · the `{"error":{…}}` envelope on
        // failure (it used to stay empty/`{}` — a machine consumer had to
        // scrape stderr to learn WHY) · the `{"paused":{…}}` envelope on a
        // human-gate pause.
        match verdict_line {
            Some(line) => println!("{line}"),
            None => println!("{}", outputs_json_line(&outcome.outputs)),
        }
        code
    } else if json {
        let mut tee = Tee::new(JsonSink::new(std::io::stdout().lock()), trace);
        let (code, outcome) = drive(runtime, wf, report, &mut stamper, &mut tee).await;
        let (sink, trace) = tee.into_parts();
        // stdout stays NDJSON verbatim (byte-identical with or without the
        // journal) — the trace note rides on stderr here.
        surface_trace(trace, TraceNote::Stderr);
        if let Some(e) = sink.into_error() {
            eprintln!("nika run: stream write failed: {e}");
            return exit::ENV;
        }
        print_resume_summary(&outcome, resumed, true);
        code
    } else {
        execute_fold_lane(
            runtime,
            wf,
            report,
            &mut stamper,
            file,
            theme,
            mode,
            resumed,
            trace,
            outputs,
        )
        .await
    }
}

/// The human fold lane (`Live` · `Plain` · `Quiet`) — extracted whole so
/// `execute` stays a lane DISPATCHER (fn-length ratchet · the three lanes
/// are peers, not one long body). Storytelling surfaces get the flow
/// epilogue + the spec §3.3 `trace:` pointer; `--quiet` keeps its
/// compact-card promise.
#[allow(clippy::too_many_arguments)]
async fn execute_fold_lane(
    runtime: &ProdRuntime,
    wf: &RawWorkflow,
    report: &CheckReport,
    stamper: &mut SystemStamper,
    file: &str,
    theme: Theme,
    mode: RenderMode,
    resumed: bool,
    trace: TraceFileSink,
    outputs: bool,
) -> u8 {
    let mut fold = FoldSink::new(std::io::stdout().lock(), theme, mode);
    fold.set_plan(plan_waves(wf, report));
    // The shape tails ride the INTERACTIVE surface only (`Live` = TTY):
    // the piped/`--no-progress`/`--quiet` registers keep their exact
    // bytes — CI logs and scripts never grow tails.
    if mode == RenderMode::Live && outputs {
        fold.show_outputs(true);
    }
    let mut tee = Tee::new(fold, trace);
    let (code, outcome) = drive(runtime, wf, report, stamper, &mut tee).await;
    let (mut sink, trace) = tee.into_parts();
    // `Live` painted in place during the run; `Plain`/`Quiet` folded
    // silently · print the ONE final frame now.
    if mode != RenderMode::Live {
        sink.print_final();
    }
    // The Live (TTY) final frame carries the flow epilogue: the wall-
    // time waterfall + the outputs pointer (design §2c). The sober
    // registers stay untouched — CI logs never grow chart art.
    if mode == RenderMode::Live {
        print_flow_epilogue(sink.view(), &outcome.outputs, theme, file);
    }
    // The spec §3.3 final-frame pointer (`trace: …`) — under the frame
    // on the storytelling surfaces.
    surface_trace(
        trace,
        if mode == RenderMode::Quiet {
            TraceNote::Silent
        } else {
            TraceNote::Stdout
        },
    );
    print_resume_summary(&outcome, resumed, false);
    if let Some(e) = sink.into_error() {
        eprintln!("nika run: render failed: {e}");
        return exit::ENV;
    }
    code
}

/// Where the run journal's `trace:` pointer lands (per lane).
#[derive(Clone, Copy)]
enum TraceNote {
    /// The human storytelling surfaces (`Live` · `Plain`) — the spec §3.3
    /// final-frame pointer, printed under the frame.
    Stdout,
    /// The machine lanes (`--json` · `--output json`) — their stdout is a
    /// byte-exact contract, so the pointer rides the diagnostic stream.
    Stderr,
    /// `--quiet` — the compact-card promise holds (no pointer · the
    /// journal is still written · an fs error still reaches stderr).
    Silent,
}

/// Surface the run journal AFTER the run — NEVER the exit code (the sink
/// contract: journaling is a rider, a broken rider is a note, not a
/// failure). An fs error goes to stderr with the path when one was opened;
/// a written journal prints its `trace:` pointer per [`TraceNote`].
fn surface_trace(trace: TraceFileSink, note: TraceNote) {
    let path = trace.path().map(std::path::Path::to_path_buf);
    // The printed head is the chain's free external anchor: CI logs and
    // scrollback hold it, so a rewritten-whole journal no longer matches
    // the record of the run that printed it (tamper-EVIDENT → checkable).
    let head8 = trace.chain_head()[..16].to_owned();
    let count = trace.chain_len();
    if let Some(e) = trace.into_error() {
        // Name the file when the failure struck AFTER the open (a partial
        // journal on disk) — the operator sees exactly what to distrust.
        match &path {
            Some(p) => eprintln!(
                "nika run: trace file {}: {e} — the run itself is unaffected",
                p.display()
            ),
            None => eprintln!("nika run: trace file: {e} — the run itself is unaffected"),
        }
        return;
    }
    let Some(path) = path else {
        return; // disabled · or a run that emitted zero events
    };
    match note {
        TraceNote::Stdout => {
            println!(
                "    trace: {} · {count} events · chain {head8}",
                path.display()
            );
        }
        TraceNote::Stderr => {
            eprintln!(
                "nika run: trace: {} · {count} events · chain {head8}",
                path.display()
            );
        }
        TraceNote::Silent => {}
    }
}

/// The `--resume` post-run summary (`resumed · N skipped · M ran live`) —
/// printed ONLY when a resume was requested (a fresh run's surfaces stay
/// byte-identical). Machine modes route it to stderr (stdout is the
/// contract surface); human modes print it under the final frame.
fn print_resume_summary(outcome: &RunOutcome, resumed: bool, to_stderr: bool) {
    if !resumed {
        return;
    }
    let ran_live = outcome
        .records
        .values()
        .filter(|r| r.started_at.is_some())
        .count();
    let line = resume::summary_line(outcome.cache_hits.len(), ran_live);
    if to_stderr {
        eprintln!("{line}");
    } else {
        println!("\n  {line}");
    }
}

/// The TTY final-frame epilogue: the post-run waterfall (real durations ·
/// real overlap · pure fold of the run's own event stream) then the
/// shareable verdict card, its outputs note naming what left the run —
/// closed by the explore hint. SEAM (stated, not faked): a live run
/// writes no trace file today, so the hint teaches the two-step that
/// works NOW (record with `--json`, then browse); when auto-trace
/// recording ships, this collapses to the recorded path.
fn print_flow_epilogue(
    view: &crate::RunView,
    outputs: &BTreeMap<String, Value>,
    theme: Theme,
    file: &str,
) {
    for line in crate::display::flow::waterfall(view, &theme) {
        println!("{line}");
    }
    let note = outputs_note(outputs);
    for line in crate::display::flow::verdict_card(view, &theme, note.as_deref()) {
        println!("{line}");
    }
    // The workflow path is CLICKABLE on link-capable terminals (OSC-8 ·
    // file:// — the one real file in the hint; the ndjson names are the
    // suggested two-step, not files that exist yet).
    let file_cell = crate::verbs::linked_path(theme, file);
    let record =
        format!("nika run {file_cell} --json > run.ndjson · nika trace outputs run.ndjson");
    println!(
        "  {}",
        crate::display::vocab::hint(theme, "explore", &record)
    );
}

/// The card's outputs note: `outputs → key (type) · key2 (type)` — the
/// export contract's shape at a glance (types only, never a data dump).
/// Two keys shown, the rest counted.
fn outputs_note(outputs: &BTreeMap<String, Value>) -> Option<String> {
    if outputs.is_empty() {
        return None;
    }
    let mut parts: Vec<String> = outputs
        .iter()
        .take(2)
        .map(|(key, value)| format!("{key} ({})", json_type_name(value)))
        .collect();
    if outputs.len() > 2 {
        parts.push(format!("+{} more", outputs.len() - 2));
    }
    Some(format!("outputs → {}", parts.join(" · ")))
}

/// The JSON type vocabulary for the outputs pointer — names only, never
/// values (a summary line, not a data leak into the scrollback).
fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
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

/// The export contract's stdout payload (spec 01 §"What leaves a run"): the
/// resolved workflow `outputs:` as ONE JSON object on a single line. An
/// empty map (no `outputs:` declared · or references that no longer
/// resolve) renders `{}` — stdout is ALWAYS a single JSON object in
/// `--output json` mode, a stable machine contract for the composition
/// path (`exec: nika run sub --output json` + `capture: stdout`).
fn outputs_json_line(outputs: &BTreeMap<String, Value>) -> String {
    serde_json::to_string(outputs).unwrap_or_else(|_| "{}".to_owned())
}

/// Route a human-readable diagnostic to the spec-correct stream: stderr in
/// `--output json` mode (stdout MUST stay a clean JSON object · the export
/// contract · `capture: stdout` composition), stdout in the human modes.
/// In machine mode the failure ALSO lands on stdout as the `{"error":{…}}`
/// envelope (F6) — the machine surface is self-sufficient, success or not.
fn emit_diagnostic(text: &str, output_json: bool) {
    if output_json {
        eprint!("{text}");
        println!("{}", error_envelope_line(envelope_message(text)));
    } else {
        print!("{text}");
    }
}

/// Print the machine failure envelope when in `--output json` mode (the
/// ENV-class exits inside `run` share this one seam).
fn emit_error_envelope(message: &str, output_json: bool) {
    if output_json {
        println!("{}", error_envelope_line(message));
    }
}

/// ONE `{"paused":{…}}` line — the machine pause contract (ADR-099 rider
/// · additive beside the success/error envelopes): the prompt payload a
/// consumer needs to deliver an answer (`--answer <task>=<value>` at
/// resume · or a serve webhook later).
fn paused_envelope_line(pause: &WorkflowPause) -> String {
    serde_json::json!({
        "paused": {
            "task": pause.task,
            "mode": pause.mode,
            "message": pause.message,
            "choices": pause.choices,
        }
    })
    .to_string()
}

/// ONE `{"error":{"code":…,"message":…}}` line — the machine failure
/// contract (F6). `code` is the first NIKA wire code found in the message
/// (`null` when the failure class carries none, e.g. an unreadable file).
fn error_envelope_line(message: &str) -> String {
    serde_json::json!({
        "error": { "code": first_nika_code(message), "message": message }
    })
    .to_string()
}

/// Best-effort wire-code extraction: the first `NIKA-…` token in a
/// diagnostic (findings render `[NIKA-PARSE-009]` · run details lead with
/// `NIKA-431 · …`). Never invents — no token, no code.
fn first_nika_code(text: &str) -> Option<&str> {
    let start = text.find("NIKA-")?;
    let rest = &text[start..];
    let end = rest
        .find(|c: char| !(c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-'))
        .unwrap_or(rest.len());
    let code = rest[..end].trim_end_matches('-');
    // A bare `NIKA-` prefix with no digits is prose, not a code.
    (code.len() > "NIKA-".len() && code.bytes().any(|b| b.is_ascii_digit())).then_some(code)
}

/// The one-line message for a findings-render envelope: the first line
/// carrying a wire code (the render wraps it in section noise), else the
/// first non-empty line.
fn envelope_message(text: &str) -> &str {
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let first = lines.next().unwrap_or(text);
    std::iter::once(first)
        .chain(lines)
        .find(|l| l.contains("NIKA-"))
        .unwrap_or(first)
        .trim()
}

/// The failure envelope for a run that EXECUTED and failed: the first
/// failed task row's detail (it carries the wire code), else the
/// workflow-level detail (run-end typed-output breaches), else a stable
/// fallback — stdout never goes silent on a machine consumer.
fn run_failure_envelope(view: &crate::RunView) -> String {
    let failed = view
        .rows()
        .iter()
        .find(|r| r.state == crate::TaskState::Failed);
    let message = match failed {
        Some(row) if row.detail.is_empty() => format!("task `{}` failed", row.id),
        Some(row) => format!("task `{}` failed — {}", row.id, row.detail),
        None => view
            .workflow_detail
            .clone()
            .unwrap_or_else(|| "workflow failed".to_owned()),
    };
    error_envelope_line(&message)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::{RenderMode, exit, offline_tip_applies, outputs_json_line, run, scope_to_task};
    use crate::Theme;
    use serde_json::{Value, json};
    use std::collections::BTreeMap;

    #[test]
    fn outputs_json_line_is_one_sorted_object() {
        let mut m: BTreeMap<String, Value> = BTreeMap::new();
        m.insert("total".to_owned(), json!(60));
        m.insert("count".to_owned(), json!(3));
        // BTreeMap key order → `count` before `total`: a single line,
        // deterministic across runs (the machine consumer can jq it).
        assert_eq!(outputs_json_line(&m), r#"{"count":3,"total":60}"#);
        assert!(!outputs_json_line(&m).contains('\n'));
    }

    #[test]
    fn outputs_json_line_empty_is_braces() {
        // No `outputs:` declared → still a JSON object on stdout.
        assert_eq!(outputs_json_line(&BTreeMap::new()), "{}");
    }

    /// The offline-hint policy (pure · the heart of the UX decision). The
    /// tip fires ONLY when a non-mock example FAILED with no `--model`
    /// override — exactly the "no local model running" case.
    #[test]
    fn offline_tip_policy_fires_only_when_actionable() {
        // FAIL + no override + a local model → the tip is the right nudge.
        assert!(offline_tip_applies(
            exit::WORKFLOW,
            false,
            "ollama/llama3.1"
        ));
        assert!(offline_tip_applies(exit::ENV, false, "ollama/llama3.1"));
        // A clean run never needs the tip.
        assert!(!offline_tip_applies(exit::OK, false, "ollama/llama3.1"));
        // The user already overrode the model · suggesting it again is noise.
        assert!(!offline_tip_applies(
            exit::WORKFLOW,
            true,
            "ollama/llama3.1"
        ));
        // mock/echo needs no provider · a failure there is a real bug, not a
        // missing local model — so the offline tip would mislead.
        assert!(!offline_tip_applies(exit::WORKFLOW, false, "mock/echo"));
        // No envelope model (a pure-exec example, or a parse miss) · the
        // failure cannot be model-related — the nudge would mislead (the
        // cold-user e2e hit exactly this on 16-exec-pipeline).
        assert!(!offline_tip_applies(exit::WORKFLOW, false, ""));
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

    // ── F6 · the `--output json` machine failure envelope ────────────

    /// The envelope is ONE JSON object with the `{"error":{code,message}}`
    /// shape · the code is extracted, never invented.
    #[test]
    fn error_envelope_is_one_object_with_extracted_code() {
        let line = super::error_envelope_line("task failed — NIKA-VAR-001 · unresolved reference");
        let v: Value = serde_json::from_str(&line).expect("envelope is JSON");
        assert_eq!(v["error"]["code"], json!("NIKA-VAR-001"));
        assert!(
            v["error"]["message"]
                .as_str()
                .expect("message is a string")
                .contains("unresolved"),
        );
        assert!(!line.contains('\n'), "one line — the machine contract");

        // No wire code in the failure class (unreadable file) → null, not
        // a hallucinated code.
        let env_line = super::error_envelope_line("cannot read wf.yaml: No such file");
        let v: Value = serde_json::from_str(&env_line).expect("envelope is JSON");
        assert!(v["error"]["code"].is_null());
    }

    /// Wire-code extraction: bracketed findings, leading run details,
    /// per-builtin long codes — and NO false positive on bare prose.
    #[test]
    fn first_nika_code_finds_real_codes_only() {
        assert_eq!(
            super::first_nika_code("PARSE ✗  [NIKA-PARSE-009] two verbs"),
            Some("NIKA-PARSE-009")
        );
        assert_eq!(
            super::first_nika_code("NIKA-431 · provider API error"),
            Some("NIKA-431")
        );
        assert_eq!(
            super::first_nika_code("x NIKA-BUILTIN-JQ-001 y"),
            Some("NIKA-BUILTIN-JQ-001")
        );
        assert_eq!(super::first_nika_code("the NIKA- prefix alone"), None);
        assert_eq!(super::first_nika_code("no code here"), None);
    }

    /// A findings render condenses to the line that carries the code.
    #[test]
    fn envelope_message_prefers_the_code_line() {
        let text =
            "nika check · wf.yaml\n X CONFORM  [NIKA-CEL-001] bad when\n  verdict: 1 finding\n";
        assert_eq!(
            super::envelope_message(text),
            "X CONFORM  [NIKA-CEL-001] bad when"
        );
        // No code anywhere → the first non-empty line.
        assert_eq!(
            super::envelope_message("\ncannot read x: gone\ndetail\n"),
            "cannot read x: gone"
        );
    }

    /// The run-failure envelope reads the folded view: the failed row's
    /// detail (which carries the wire code) becomes the machine message.
    #[test]
    fn run_failure_envelope_carries_the_failed_task_detail() {
        let mut view = crate::RunView::new();
        for ev in crate::demo::failure() {
            view.apply(&ev);
        }
        let line = super::run_failure_envelope(&view);
        let v: Value = serde_json::from_str(&line).expect("envelope is JSON");
        assert_eq!(v["error"]["code"], json!("NIKA-431"), "{line}");
        assert!(
            v["error"]["message"]
                .as_str()
                .expect("message present")
                .contains("task `"),
            "{line}"
        );

        // An empty view (nothing folded) still yields a stable envelope.
        let empty = super::run_failure_envelope(&crate::RunView::new());
        let v: Value = serde_json::from_str(&empty).expect("fallback is JSON");
        assert_eq!(v["error"]["message"], json!("workflow failed"));
    }

    #[test]
    fn parse_var_overrides_types_json_else_string() {
        let wf = nika_schema::parse(
            "nika: v1\nworkflow: t\nvars:\n  topic: { type: string, required: true }\n  limit: { type: integer, default: 3 }\n  flags: [\"a\"]\ntasks:\n  - id: t\n    exec: { command: \"true\" }\n",
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");

        // JSON-if-parses: numbers · arrays land typed; bare words as strings.
        let overrides = super::parse_var_overrides(
            &[
                "topic=quantum news".to_owned(),
                "limit=5".to_owned(),
                "flags=[\"x\",\"y\"]".to_owned(),
            ],
            &wf,
        )
        .expect("valid overrides");
        assert_eq!(overrides["topic"], json!("quantum news"));
        assert_eq!(overrides["limit"], json!(5));
        assert_eq!(overrides["flags"], json!(["x", "y"]));

        // The unknown-key refusal NAMES the declared set (actionable).
        let err = super::parse_var_overrides(&["ghost=1".to_owned()], &wf)
            .expect_err("unknown key refused");
        assert!(err.contains("ghost"), "{err}");
        assert!(err.contains("topic"), "lists the declared vars: {err}");

        // `=` in the VALUE is preserved (split_once · key=v=w).
        let eq = super::parse_var_overrides(&["topic=a=b".to_owned()], &wf)
            .expect("value may carry '='");
        assert_eq!(eq["topic"], json!("a=b"));
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
