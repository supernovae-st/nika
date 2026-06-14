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

use std::io::Write as _;

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
pub fn run(file: &str, json: bool, theme: Theme) -> u8 {
    // ── Audit BEFORE run (spec §3 · INV the runtime also enforces) ──
    let (wf, report) = match crate::verbs::load_checked(file) {
        Ok(pair) => pair,
        Err(out) => {
            print!("{}", out.text);
            return out.code;
        }
    };
    if !report.is_clean() {
        // The SAME findings `nika check` renders — the user must see why
        // it won't run. Reuses the locked check rendering (exit 2).
        let out = crate::verbs::check::run(file, json, theme);
        print!("{}", out.text);
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
    rt.block_on(execute(&runtime, &wf, &report, json, theme))
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
    run(&path.to_string_lossy(), false, theme)
}

/// Drive the runtime through the chosen sink · return the exit code.
async fn execute(
    runtime: &ProdRuntime,
    wf: &RawWorkflow,
    report: &CheckReport,
    json: bool,
    theme: Theme,
) -> u8 {
    let mut stamper = SystemStamper::new();
    if json {
        let mut sink = JsonSink::new(std::io::stdout().lock());
        let code = drive(runtime, wf, report, &mut stamper, &mut sink).await;
        if let Some(e) = sink.into_error() {
            eprintln!("nika run: stream write failed: {e}");
            return exit::ENV;
        }
        code
    } else {
        let interactive = std::io::IsTerminal::is_terminal(&std::io::stdout());
        let mut sink = FoldSink::new(std::io::stdout().lock(), theme, interactive);
        let code = drive(runtime, wf, report, &mut stamper, &mut sink).await;
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
) -> u8
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
            if outcome.ok {
                exit::OK
            } else {
                exit::WORKFLOW
            }
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
            exit::ENV
        }
    }
}
