// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The bin-side dispatch half of the trace plane — the `trace` arm's
//! routing and render loop, descended from `nika-cli`'s `main.rs`
//! 2026-08-11 (the 15k wall · the ADR-110 cli-host precedent). The bin
//! keeps a one-line arm that calls [`trace_verb`]; everything else — the
//! replay loop, the dossier doors' routing, the rm/anchor target
//! resolution — lives here, next to the verbs it drives.

// A terminal surface's whole job is printing (the nika-cli main.rs idiom ·
// the nika-catalog-verify precedent).
#![allow(clippy::disallowed_macros, clippy::print_stdout, clippy::print_stderr)]

use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::time::Duration;

use nika_event::Event;

use crate::display::format::{ColorChoice, ColorEnv, LinkChoice, color_enabled, links_enabled};
use crate::display::theme::Theme;
use crate::{RunView, VerbOutput, exit, frame, frame_with_outputs};
use crate::{evidence, receipt, trace, trace_anchor, trace_otel, trace_reproduce, trace_verify};

/// Print a verb's text on the right stream and return its exit code.
/// Findings + successes go to stdout (they ARE the product); only
/// environment errors go to stderr.
fn emit(out: &VerbOutput) -> u8 {
    if out.code == exit::ENV {
        eprintln!("nika: {}", out.text);
    } else if !out.text.is_empty() {
        println!("{}", out.text.trim_end());
    }
    out.code
}

/// The `evidence` arm's routing: resolve the trace (store handle or
/// bare-latest), then export the pack through the verbs seam.
fn evidence_run(args: evidence::EvidenceArgs) -> u8 {
    match trace::manage::resolve_trace(args.trace) {
        Ok(path) => emit(&evidence::export(
            &path.to_string_lossy(),
            args.out.as_deref(),
            args.workflow.as_deref(),
            args.json,
            args.full,
        )),
        Err(code) => code,
    }
}

/// `nika trace verify [TRACES…]` — several paths (the shell glob) go
/// per-file/worst-of; zero or one keeps the existing voice byte-stable
/// (bare form resolves the latest · one arg resolves store handles).
fn verify_verb(mut traces: Vec<PathBuf>, opts: &trace_verify::VerifyOptions) -> u8 {
    if traces.len() > 1 {
        return emit(&trace_verify::verify_many_with(&traces, opts));
    }
    match trace::manage::resolve_trace(traces.pop()) {
        Ok(path) => emit(&trace_verify::verify_with(&path.to_string_lossy(), opts)),
        Err(code) => code,
    }
}

/// The `trace` arm — one dispatch per subcommand. `theme` is the bin's
/// resolved sober-register theme; `color`/`link_when` re-resolve for the
/// replay/show render (a piped `trace show` keeps its exact bytes).
#[must_use]
pub fn trace_verb(
    action: trace::TraceAction,
    theme: Theme,
    color: ColorChoice,
    link_when: LinkChoice,
) -> u8 {
    match action {
        trace::TraceAction::Replay(args) => {
            trace_render(&args, true, color, link_when, theme.ascii)
        }
        // RAMS-15 · the dossier doors live under trace (read · export ·
        // prove) — the old top-level verbs were four doors on one run.
        trace::TraceAction::Evidence { args } => evidence_run(args),
        trace::TraceAction::Receipt { action } => emit(&receipt::run(action)),
        trace::TraceAction::Show(args) => trace_render(&args, false, color, link_when, theme.ascii),
        trace::TraceAction::Ls {} => emit(&trace::manage::ls(theme)),
        trace::TraceAction::Rm {
            trace,
            older_than,
            all,
            force,
        } => match rm_target(trace, older_than, all) {
            Ok(target) => emit(&trace::manage::rm(&target, force, theme)),
            Err(code) => code,
        },
        trace::TraceAction::Outputs { trace } => {
            let trace = match trace::manage::resolve_trace(trace) {
                Ok(path) => path,
                Err(code) => return code,
            };
            let mut theme = theme;
            // The dur column's bracket accents: TTY comfort only.
            theme.accents = std::io::stdout().is_terminal();
            emit(&trace::outputs(&trace.to_string_lossy(), theme))
        }
        trace::TraceAction::Verify {
            traces,
            key,
            anchored,
            replay,
        } => verify_verb(
            traces,
            &trace_verify::VerifyOptions {
                key,
                anchored,
                replay,
            },
        ),
        trace::TraceAction::Anchor {
            trace,
            rekor_url,
            tsa_url,
        } => anchor_verb(trace, &rekor_url, &tsa_url),
        trace::TraceAction::Reproduce { recorded, fresh } => emit(&trace_reproduce::reproduce(
            &recorded.to_string_lossy(),
            &fresh.to_string_lossy(),
        )),
        trace::TraceAction::Export {
            trace,
            out,
            include_content,
        } => emit(&trace_otel::export(
            &trace::manage::resolve_store_handle(&trace).to_string_lossy(),
            out.as_deref()
                .map(|p| p.to_string_lossy().into_owned())
                .as_deref(),
            include_content,
        )),
        trace::TraceAction::Peek { trace, task, raw } => {
            emit(&trace::peek(&trace.to_string_lossy(), &task, raw, theme))
        }
        trace::TraceAction::Flow { trace, workflow } => {
            match trace::manage::flow_verb(trace, workflow, theme) {
                Ok(out) => emit(&out),
                Err(code) => code,
            }
        }
    }
}

/// The `rm` arm's target resolution — one of three mutually-exclusive
/// spellings (clap owns the arity; this owns the semantics).
fn rm_target(
    trace: Option<String>,
    older_than: Option<String>,
    all: bool,
) -> Result<trace::manage::RmTarget, u8> {
    if all {
        return Ok(trace::manage::RmTarget::All);
    }
    if let Some(raw) = older_than {
        return match trace::manage::parse_older_than(&raw) {
            Ok(cutoff) => Ok(trace::manage::RmTarget::OlderThan(cutoff)),
            Err(message) => {
                eprintln!("nika trace: {message}");
                Err(exit::ENV)
            }
        };
    }
    // clap's required_unless_present_any guarantees the handle.
    let Some(handle) = trace else {
        eprintln!("nika trace: rm needs a trace, --older-than, or --all");
        return Err(exit::ENV);
    };
    Ok(trace::manage::RmTarget::One(handle))
}

/// The `anchor` arm's routing: resolve the trace (store handle or
/// bare-latest), then notarize through the verbs seam.
fn anchor_verb(trace: Option<PathBuf>, rekor_url: &str, tsa_url: &str) -> u8 {
    match trace::manage::resolve_trace(trace) {
        Ok(path) => emit(&trace_anchor::run(
            &path.to_string_lossy(),
            rekor_url,
            tsa_url,
        )),
        Err(code) => code,
    }
}

/// Load events, fold, render — live replay or final card.
fn trace_render(
    args: &trace::TraceArgs,
    replay: bool,
    color: ColorChoice,
    link_when: LinkChoice,
    ascii: bool,
) -> u8 {
    let events = match trace::manage::load_events(args) {
        Ok(events) => events,
        Err(message) => {
            eprintln!("nika trace: {message}");
            eprintln!(
                "  fix: a trace is the NDJSON a run records — nika run <wf> --json > run.ndjson"
            );
            return exit::ENV; // environment error (spec §4)
        }
    };

    let tty = std::io::stdout().is_terminal();
    let mut theme = term_theme(color, ascii, link_when);
    theme.animate = tty && replay && !env_flag("NIKA_REDUCED_MOTION");
    // The duration accents ride the interactive surface only — a piped
    // `trace show` keeps its exact legacy bytes.
    theme.accents = tty;
    // Duration heat additionally needs colour + the truecolor PROOF.
    theme.heat = tty && theme.color && nika_cli_host::output::truecolor_env();
    // The shape tails ride the interactive surface only: a TTY render
    // (show OR replay) carries them unless `--no-outputs`; a piped
    // `trace show` keeps its exact legacy bytes.
    let outputs = tty && !args.no_outputs;
    let mut view = RunView::new();
    if theme.animate {
        live_replay(&events, &mut view, theme, args.speed, outputs);
    } else {
        for event in &events {
            view.apply(event);
        }
        let lines = if outputs {
            frame_with_outputs(&view, &theme, 0)
        } else {
            frame(&view, &theme, 0)
        };
        print_lines(&lines);
    }
    // The trace surface owns the run overlays (replay = re-render, never
    // re-execute): the waterfall + the verdict card close the read, from
    // any past trace — the same final frame a live TTY run ends on. The
    // fruit rides in its PURE form (paths + the model's last word, no
    // sizes: stat would read today's disk against a past run's claim).
    print_lines(&crate::display::flow::waterfall(&view, &theme));
    let mut notes: Vec<String> = crate::display::fruit::written_files(&view)
        .iter()
        .map(|f| format!("{} {}", f.verb, f.path))
        .collect();
    if let Some((_task, text)) = crate::display::fruit::last_said(&view)
        && let Some(quote) = crate::display::shape::summarize(text, 46)
    {
        notes.push(format!("said {quote}"));
    }
    print_lines(&crate::display::flow::verdict_card(&view, &theme, &notes));
    // The locked exit contract: 0 = run ok · 1 = workflow failed.
    u8::from(view.verdict != Some(true))
}

/// Replay with compressed timing: spinner ticks between events, frames
/// redrawn in place (cursor-up + clear).
fn live_replay(events: &[Event], view: &mut RunView, theme: Theme, speed: f64, outputs: bool) {
    let mut drawn = 0usize;
    let mut tick = 0usize;
    let mut last_ms = events.first().map_or(0, |e| e.timestamp.unix_ms());
    for event in events {
        let ts = event.timestamp.unix_ms();
        let gap_ms = ts.saturating_sub(last_ms).max(0);
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        // display pacing only — precision is irrelevant at 80ms granularity
        let steps = ((gap_ms as f64 / speed.max(0.1) / 80.0).ceil() as u64).clamp(1, 50);
        for _ in 0..steps {
            drawn = redraw(view, theme, tick, drawn, outputs);
            tick += 1;
            std::thread::sleep(Duration::from_millis(80));
        }
        last_ms = ts;
        view.apply(event);
        drawn = redraw(view, theme, tick, drawn, outputs);
    }
}

/// Draw one frame in place; returns the line count for the next clear.
fn redraw(view: &RunView, theme: Theme, tick: usize, drawn: usize, outputs: bool) -> usize {
    let lines = if outputs {
        frame_with_outputs(view, &theme, tick)
    } else {
        frame(view, &theme, tick)
    };
    let mut out = std::io::stdout().lock();
    if drawn > 0 {
        // Cursor up over the previous frame, then clear to end of screen.
        let _ = write!(out, "\x1b[{drawn}F\x1b[J");
    }
    let _ = writeln!(out, "{}", lines.join("\n"));
    let _ = out.flush();
    lines.len()
}

fn print_lines(lines: &[String]) {
    if lines.is_empty() {
        return; // an empty overlay (solo-task waterfall · no verdict) prints nothing
    }
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{}", lines.join("\n"));
}

/// Resolve the colour/glyph/link theme for the trace render — colour and
/// hyperlinks each ride their own capability chain over the SAME
/// environment facts.
fn term_theme(choice: ColorChoice, ascii: bool, link_when: LinkChoice) -> Theme {
    let tty = std::io::stdout().is_terminal();
    let env = color_env();
    let mut theme = Theme::new(color_enabled(choice, env, tty), ascii, false);
    theme.links = links_enabled(link_when, tty, env.term_dumb);
    theme
}

/// Collect the colour-relevant environment facts once (the pure priority
/// chain lives in `display::format::color_enabled` — this is its I/O half).
fn color_env() -> ColorEnv {
    ColorEnv {
        force: env_value("CLICOLOR_FORCE").is_some_and(|v| !v.is_empty() && v != "0"),
        no_color: env_flag("NO_COLOR"),
        clicolor_zero: env_value("CLICOLOR").is_some_and(|v| v == "0"),
        term_dumb: env_value("TERM").is_some_and(|v| v == "dumb"),
    }
}

/// Read a boolean presentation flag from the environment.
///
/// Presentation flags (`NO_COLOR` · `NIKA_REDUCED_MOTION` ·
/// `NIKA_NO_TRACE_FILE`) are not secrets — the workspace
/// `disallowed_methods` ban on `std::env::var` exists to route SECRET
/// reads through the kernel vault seam, which has no business in a
/// colour toggle. Scoped allow, single seam.
#[allow(clippy::disallowed_methods)]
fn env_flag(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|v| !v.is_empty())
}

/// Read a presentation variable's VALUE (`CLICOLOR` · `TERM`) — the same
/// non-secret seam as [`env_flag`]. The truecolor PROOF reads through the
/// host member's [`nika_cli_host::output::truecolor_env`], never a local
/// shadow.
#[allow(clippy::disallowed_methods)]
fn env_value(name: &str) -> Option<String> {
    std::env::var(name).ok()
}
