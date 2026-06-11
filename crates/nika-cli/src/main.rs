// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `nika-cli` dev binary — the seed of the `nika` verb tree.
//!
//! Ships `trace replay|show` today (the flight-recorder reader · spec §7),
//! folding either the deterministic demo storyboards or a real trace
//! NDJSON. Exit codes already follow the locked contract (spec §4):
//! `0` run ok · `1` workflow failed · `3` environment error.

// A terminal binary's whole job is printing — the same exemption as the
// nika-catalog-verify binary and the nika-schema check example.
#![allow(clippy::disallowed_macros, clippy::print_stdout, clippy::print_stderr)]

use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::time::Duration;

use clap::{Args, Parser, Subcommand};
use nika_cli::{RunView, Theme, frame};
use nika_event::Event;

#[derive(Parser)]
#[command(name = "nika-cli", version, about = "nika operator surface (WIP seed)")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Read the flight recorder (replay or summarize a run).
    Trace {
        #[command(subcommand)]
        action: TraceAction,
    },
}

#[derive(Subcommand)]
enum TraceAction {
    /// Re-render a run live (replay = re-render, NEVER re-execute).
    Replay(TraceArgs),
    /// Print the final card only.
    Show(TraceArgs),
}

#[derive(Args)]
// Four independent CLI flags ARE four bools — the clap-surface idiom, not
// a state machine to encode.
#[allow(clippy::struct_excessive_bools)]
struct TraceArgs {
    /// Trace NDJSON path (one `nika-event` Event per line).
    trace: Option<PathBuf>,
    /// Render the built-in success storyboard.
    #[arg(long, conflicts_with = "trace")]
    demo: bool,
    /// Render the built-in failure storyboard.
    #[arg(long, conflicts_with_all = ["trace", "demo"])]
    demo_fail: bool,
    /// Replay time compression (6 = 6× faster than recorded).
    #[arg(long, default_value_t = 6.0)]
    speed: f64,
    /// Force the ASCII glyph theme (CI logs · legacy terminals).
    #[arg(long)]
    ascii: bool,
    /// Disable colour output.
    #[arg(long)]
    no_color: bool,
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let code = match cli.command {
        Command::Trace { action } => match action {
            TraceAction::Replay(args) => trace_render(&args, true),
            TraceAction::Show(args) => trace_render(&args, false),
        },
    };
    std::process::ExitCode::from(code)
}

/// Load events, fold, render — live replay or final card.
fn trace_render(args: &TraceArgs, replay: bool) -> u8 {
    let events = match load_events(args) {
        Ok(events) => events,
        Err(message) => {
            eprintln!("nika-cli: {message}");
            return 3; // environment error (spec §4)
        }
    };

    let tty = std::io::stdout().is_terminal();
    let theme = Theme {
        color: tty && !args.no_color && !env_flag("NO_COLOR"),
        ascii: args.ascii,
        animate: tty && replay && !env_flag("NIKA_REDUCED_MOTION"),
    };

    let mut view = RunView::new();
    if theme.animate {
        live_replay(&events, &mut view, theme, args.speed);
    } else {
        for event in &events {
            view.apply(event);
        }
        print_lines(&frame(&view, &theme, 0));
    }
    // The locked exit contract: 0 = run ok · 1 = workflow failed.
    u8::from(view.verdict != Some(true))
}

/// Replay with compressed timing: spinner ticks between events, frames
/// redrawn in place (cursor-up + clear).
fn live_replay(events: &[Event], view: &mut RunView, theme: Theme, speed: f64) {
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
            drawn = redraw(view, theme, tick, drawn);
            tick += 1;
            std::thread::sleep(Duration::from_millis(80));
        }
        last_ms = ts;
        view.apply(event);
        drawn = redraw(view, theme, tick, drawn);
    }
}

/// Draw one frame in place; returns the line count for the next clear.
fn redraw(view: &RunView, theme: Theme, tick: usize, drawn: usize) -> usize {
    let lines = frame(view, &theme, tick);
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
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{}", lines.join("\n"));
}

fn load_events(args: &TraceArgs) -> Result<Vec<Event>, String> {
    if args.demo {
        return Ok(nika_cli::demo::success());
    }
    if args.demo_fail {
        return Ok(nika_cli::demo::failure());
    }
    let Some(path) = &args.trace else {
        return Err("no trace given — pass a .ndjson path or --demo / --demo-fail".to_owned());
    };
    let raw = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let mut events = Vec::new();
    for (lineno, line) in raw.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let event: Event = serde_json::from_str(line)
            .map_err(|e| format!("{}:{}: bad event: {e}", path.display(), lineno + 1))?;
        events.push(event);
    }
    if events.is_empty() {
        return Err(format!("{}: empty trace", path.display()));
    }
    Ok(events)
}

/// Read a boolean presentation flag from the environment.
///
/// Presentation flags (`NO_COLOR` · `NIKA_REDUCED_MOTION`) are not secrets —
/// the workspace `disallowed_methods` ban on `std::env::var` exists to route
/// SECRET reads through the kernel vault seam, which has no business in a
/// colour toggle. Scoped allow, single seam.
#[allow(clippy::disallowed_methods)]
fn env_flag(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|v| !v.is_empty())
}
