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
mod sink;
mod stamp;

pub use compose::{
    ProdRuntime, RuntimeCapabilities, capabilities_of, fs_boundary_of, net_boundary_of,
    production_runtime,
};
pub use sink::{FoldSink, JsonSink, RenderMode};
pub use stamp::SystemStamper;

use std::collections::BTreeMap;
use std::io::Write as _;

use serde_json::Value;

use nika_runtime::{EventSink, Runtime, Stamper};
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
#[must_use]
pub fn run(
    file: &str,
    json: bool,
    output: Option<&str>,
    theme: Theme,
    mode: RenderMode,
    dry_run: bool,
    model_override: Option<&str>,
) -> u8 {
    // `--output json` selects the machine-result mode (spec 01 §"What
    // leaves a run"): the resolved `outputs:` object as one JSON object on
    // stdout · diagnostics/progress on stderr. Absent → the live human
    // render. Validated up front so an unknown format fails before any work.
    let output_json = match output {
        None => false,
        Some("json") => true,
        Some(other) => {
            eprintln!("nika run: unknown --output format `{other}` (expected `json`)");
            return exit::ENV;
        }
    };

    // ── Audit BEFORE run (spec §3 · INV the runtime also enforces) ──
    let (wf, report) = match crate::verbs::load_checked(file) {
        Ok(pair) => pair,
        Err(out) => {
            // Pre-run diagnostics obey the export contract too: in machine
            // mode they go to stderr so a `capture: stdout` consumer never
            // mistakes the "cannot read" text for the JSON result.
            emit_diagnostic(&out.text, output_json);
            return out.code;
        }
    };
    if !report.is_clean() {
        // The SAME findings `nika check` renders — the user must see why
        // it won't run. Reuses the locked check rendering (exit 2). In
        // machine mode the findings go to stderr (stdout stays clean JSON).
        let out = crate::verbs::check::run(file, json, theme);
        emit_diagnostic(&out.text, output_json);
        return out.code;
    }

    // ── Dry-run (spec §10 · "plan only · zero effects") ─────────────
    // The audit passed; STOP here and show the static plan (the same anatomy
    // `nika inspect` renders) without composing any production seam. No fs,
    // no http, no subprocess, no provider call — the run is never reached.
    if dry_run {
        let plan = crate::verbs::inspect::run(file);
        if !plan.text.is_empty() {
            println!("{}", plan.text.trim_end());
        }
        println!("\n  dry-run · plan only · no effects executed");
        return exit::OK;
    }

    // ── Compose the production runtime (real seams · env keys) ──────
    // The envelope default model · a task's own `model:` overrides it ·
    // an exec-only workflow never resolves it (so "" is harmless until
    // an infer/agent task actually needs a model · resolve is loud then).
    // A `--model` override REPLACES the envelope default through this SAME
    // path — a bad id fails loud at resolve time exactly as a bad envelope
    // model does (no separate, lenient validation seam).
    let envelope_model = wf.model.as_ref().map_or("", |m| m.value.as_str());
    let default_model = model_override.unwrap_or(envelope_model);
    // Both runtime capability boundaries (permits.fs + permits.net.http) in
    // one value (spec §permits · NIKA-SEC-004) — derived once so neither axis
    // can be wired while the other is forgotten. fs gates the file builtins;
    // net gates the fetch client per-hop (catching dynamic/redirect hosts the
    // static check cannot see). The static check is the other half of each.
    let caps = capabilities_of(&wf);
    let runtime = match production_runtime(default_model, caps) {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("nika run: environment: {e}");
            return exit::ENV;
        }
    };

    // ── Execute (block the async run on a current-thread executor) ──
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("nika run: environment: cannot start the async executor: {e}");
            return exit::ENV;
        }
    };
    rt.block_on(execute(
        &runtime,
        &wf,
        &report,
        json,
        output_json,
        theme,
        mode,
    ))
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
    let code = run(
        &path.to_string_lossy(),
        false,
        None,
        theme,
        mode,
        false,
        model_override,
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

/// Drive the runtime through the chosen sink · return the exit code.
async fn execute(
    runtime: &ProdRuntime,
    wf: &RawWorkflow,
    report: &CheckReport,
    json: bool,
    output_json: bool,
    theme: Theme,
    mode: RenderMode,
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
        let mut sink = FoldSink::new(std::io::stderr().lock(), theme, RenderMode::Plain);
        let (code, outputs) = drive(runtime, wf, report, &mut stamper, &mut sink).await;
        sink.print_final();
        if let Some(e) = sink.into_error() {
            eprintln!("nika run: render failed: {e}");
            return exit::ENV;
        }
        println!("{}", outputs_json_line(&outputs));
        code
    } else if json {
        let mut sink = JsonSink::new(std::io::stdout().lock());
        let (code, _outputs) = drive(runtime, wf, report, &mut stamper, &mut sink).await;
        if let Some(e) = sink.into_error() {
            eprintln!("nika run: stream write failed: {e}");
            return exit::ENV;
        }
        code
    } else {
        let mut sink = FoldSink::new(std::io::stdout().lock(), theme, mode);
        let (code, _outputs) = drive(runtime, wf, report, &mut stamper, &mut sink).await;
        // `Live` painted in place during the run; `Plain`/`Quiet` folded
        // silently · print the ONE final frame now.
        if mode != RenderMode::Live {
            sink.print_final();
        }
        if let Some(e) = sink.into_error() {
            eprintln!("nika run: render failed: {e}");
            return exit::ENV;
        }
        code
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
) -> (u8, BTreeMap<String, Value>)
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
            let code = if outcome.ok { exit::OK } else { exit::WORKFLOW };
            (code, outcome.outputs)
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
            (exit::ENV, BTreeMap::new())
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
fn emit_diagnostic(text: &str, output_json: bool) {
    if output_json {
        eprint!("{text}");
    } else {
        print!("{text}");
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::{RenderMode, exit, offline_tip_applies, outputs_json_line, run};
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

    /// A noiseless theme (no colour · no animation) for the run tests — they
    /// exercise the COMPOSITION + exit code, not the render surface.
    fn plain_theme() -> Theme {
        Theme {
            color: false,
            ascii: true,
            animate: false,
        }
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
        );
        assert_eq!(
            overridden,
            exit::OK,
            "the override resolved mock/echo, not the envelope's ollama model"
        );
    }
}
