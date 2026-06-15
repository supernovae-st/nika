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

pub use compose::{ProdRuntime, fs_boundary_of, production_runtime};
pub use sink::{FoldSink, JsonSink};
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
#[must_use]
pub fn run(file: &str, json: bool, output: Option<&str>, theme: Theme) -> u8 {
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

    // ── Compose the production runtime (real seams · env keys) ──────
    // The envelope default model · a task's own `model:` overrides it ·
    // an exec-only workflow never resolves it (so "" is harmless until
    // an infer/agent task actually needs a model · resolve is loud then).
    let default_model = wf.model.as_ref().map_or("", |m| m.value.as_str());
    // The declared permits.fs boundary the file builtins enforce at run
    // time (spec §permits · NIKA-SEC-004) — a path escaping the boundary
    // fails before the I/O (the static check is the other half).
    let fs_boundary = fs_boundary_of(&wf);
    let runtime = match production_runtime(default_model, fs_boundary) {
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
    rt.block_on(execute(&runtime, &wf, &report, json, output_json, theme))
}

/// `nika examples run <slug>` — execute one EMBEDDED example through the
/// real runtime (the pack ships offline · zero network for the exec/
/// mock-model examples). Stages the embedded YAML to a temp file (the
/// verb reads a path) and runs it.
#[must_use]
pub fn example(slug: &str, theme: Theme) -> u8 {
    let Some(yaml) = nika_pack::example(slug) else {
        eprintln!("unknown example `{slug}` — `nika examples list` names the embedded set");
        return exit::FILE;
    };
    // The slug comes from the embedded set (path-safe) · stage it beside
    // a stable name so a re-run overwrites rather than litters.
    let path = std::env::temp_dir().join(format!("nika-example-{slug}.nika.yaml"));
    if let Err(e) = std::fs::write(&path, yaml) {
        eprintln!("nika run: environment: cannot stage example `{slug}`: {e}");
        return exit::ENV;
    }
    run(&path.to_string_lossy(), false, None, theme)
}

/// Drive the runtime through the chosen sink · return the exit code.
async fn execute(
    runtime: &ProdRuntime,
    wf: &RawWorkflow,
    report: &CheckReport,
    json: bool,
    output_json: bool,
    theme: Theme,
) -> u8 {
    let mut stamper = SystemStamper::new();
    if output_json {
        // Machine-result mode (spec 01 §export contract): the live fold is
        // a DIAGNOSTIC → stderr; the resolved `outputs:` object is the ONE
        // JSON object on stdout, never interleaved. This powers the v0.1
        // sub-workflow composition `exec: nika run sub.yaml --output json`
        // + `capture: stdout` (spec 08 §composition).
        // `interactive = false` deliberately: the fold goes to stderr (a
        // pipe in the composition path · never the live TTY redraw); the
        // one final frame is what matters, not the animation.
        let mut sink = FoldSink::new(std::io::stderr().lock(), theme, false);
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
        let interactive = std::io::IsTerminal::is_terminal(&std::io::stdout());
        let mut sink = FoldSink::new(std::io::stdout().lock(), theme, interactive);
        let (code, _outputs) = drive(runtime, wf, report, &mut stamper, &mut sink).await;
        // Non-interactive folded silently · print the ONE final frame.
        if !interactive {
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
mod tests {
    use super::outputs_json_line;
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
}
