// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `nika-cli` dev binary — the seed of the `nika` verb tree.
//!
//! Today: the full STATIC suite (`check` · `inspect` · `graph` ·
//! `explain` · `spec` · `schema` · `examples` · `new` · `completions`) +
//! `trace replay|show` (the flight-recorder reader · spec §7). Everything
//! is auditable-before-run, and the `run` verb executes a CHECKED workflow
//! through the composed `nika-runtime` (L3) over production seams
//! (`nika-builtin` is admitted · no mocks).
//! Exit codes follow the locked contract (spec §4): `0` ok · `1` workflow
//! failed · `2` file findings · `3` environment error.

// A terminal binary's whole job is printing — the same exemption as the
// nika-catalog-verify binary and the nika-schema check example.
#![allow(clippy::disallowed_macros, clippy::print_stdout, clippy::print_stderr)]

use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::time::Duration;

use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use nika_cli::verbs::{self, VerbOutput};
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
    /// Static pre-flight: the ADR-092 ladder (audit BEFORE run).
    Check {
        /// Workflow file (`*.nika.yaml`).
        file: String,
        /// Emit the machine-readable report (never coloured).
        #[arg(long)]
        json: bool,
        /// Print an inferred `permits:` boundary instead of the report.
        #[arg(long)]
        infer_permits: bool,
        /// Disable colour output.
        #[arg(long)]
        no_color: bool,
        /// Force the ASCII glyph theme.
        #[arg(long)]
        ascii: bool,
    },
    /// Execute a CHECKED workflow through the L3 runtime (live render).
    Run {
        /// Workflow file (`*.nika.yaml`).
        file: String,
        /// Stream NDJSON events instead of the live render (CI · agents).
        #[arg(long)]
        json: bool,
        /// Print the typed `outputs:` as ONE JSON object on stdout
        /// (progress → stderr) · the export contract · powers
        /// `exec: nika run sub.yaml --output json` + `capture: stdout`.
        #[arg(long, value_name = "FORMAT", conflicts_with = "json")]
        output: Option<String>,
        /// Disable colour output.
        #[arg(long)]
        no_color: bool,
        /// Force the ASCII glyph theme.
        #[arg(long)]
        ascii: bool,
    },
    /// Static anatomy: tasks · verbs · DAG tree · cost · permits.
    Inspect {
        /// Workflow file (`*.nika.yaml`).
        file: String,
    },
    /// The ONE graph projector (json canonical · mermaid/dot derived).
    Graph {
        /// Workflow file (`*.nika.yaml`).
        file: String,
        /// Output format.
        #[arg(long, value_enum, default_value_t = GraphFormatArg::Json)]
        format: GraphFormatArg,
    },
    /// Teach one error code (cause · category · fix-form).
    Explain {
        /// The code (`NIKA-440` or bare `440`).
        code: String,
    },
    /// Diagnose the environment (binary · config · provider keys · spec §8).
    /// Diagnose-only — prints the exact fix command, never mutates anything.
    Doctor,
    /// Scaffold a repo (`.vscode` schema wiring · `AGENTS.md`). Existing files
    /// are skipped — `--force` overwrites.
    Init {
        /// Target directory (default · the current directory).
        #[arg(default_value = ".")]
        dir: String,
        /// Overwrite existing files.
        #[arg(long)]
        force: bool,
    },
    /// The embedded spec identity (`--canon` prints the SSOT).
    Spec {
        /// Print the canon.yaml single source of truth.
        #[arg(long)]
        canon: bool,
    },
    /// The embedded JSON Schema for `*.nika.yaml`.
    Schema,
    /// Browse the embedded examples.
    Examples {
        #[command(subcommand)]
        action: ExamplesAction,
    },
    /// Instantiate an embedded template skeleton.
    New {
        /// Template name (see `nika new --from '?'` for the set).
        #[arg(long)]
        from: String,
        /// Destination path (`*.nika.yaml`).
        dest: String,
        /// Overwrite an existing destination.
        #[arg(long)]
        force: bool,
    },
    /// Generate shell completions from the clap tree (spec §9).
    Completions {
        /// Target shell.
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// Read the flight recorder (replay or summarize a run).
    Trace {
        #[command(subcommand)]
        action: TraceAction,
    },
    /// Run the language server over stdio (drives the editor extension).
    Lsp,
    /// Run the MCP server over stdio (exposes check/explain to Cursor · Claude
    /// Desktop · agents · the in-binary Model Context Protocol surface).
    Mcp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum GraphFormatArg {
    /// Canonical JSON projection (`graph_format: 1`).
    Json,
    /// Mermaid flowchart.
    Mermaid,
    /// Graphviz dot.
    Dot,
}

#[derive(Subcommand)]
enum ExamplesAction {
    /// List the embedded example slugs.
    List,
    /// Print one embedded example.
    Show {
        /// Example slug (from `list`).
        slug: String,
    },
    /// Run an embedded example through the shipped L3 runtime (live render).
    Run {
        /// Example slug (from `list`).
        slug: String,
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
        Command::Check {
            file,
            json,
            infer_permits,
            no_color,
            ascii,
        } => {
            let out = if infer_permits {
                verbs::check::run_infer_permits(&file, json)
            } else {
                verbs::check::run(&file, json, term_theme(no_color, ascii))
            };
            emit(&out)
        }
        Command::Run {
            file,
            json,
            output,
            no_color,
            ascii,
        } => verbs::run::run(&file, json, output.as_deref(), term_theme(no_color, ascii)),
        Command::Inspect { file } => emit(&verbs::inspect::run(&file)),
        Command::Graph { file, format } => {
            let format = match format {
                GraphFormatArg::Json => verbs::graph::GraphFormat::Json,
                GraphFormatArg::Mermaid => verbs::graph::GraphFormat::Mermaid,
                GraphFormatArg::Dot => verbs::graph::GraphFormat::Dot,
            };
            emit(&verbs::graph::run(&file, format))
        }
        Command::Explain { code } => emit(&verbs::explain::run(&code)),
        Command::Doctor => emit(&verbs::doctor::run()),
        Command::Init { dir, force } => emit(&verbs::init::run(&dir, force)),
        Command::Spec { canon } => emit(&verbs::pack_surface::spec(canon)),
        Command::Schema => emit(&verbs::pack_surface::schema()),
        Command::Examples { action } => match action {
            ExamplesAction::List => emit(&verbs::pack_surface::examples_list()),
            ExamplesAction::Show { slug } => emit(&verbs::pack_surface::examples_show(&slug)),
            // The L3 run verb shipped — execute the embedded example for real.
            ExamplesAction::Run { slug } => verbs::run::example(&slug, term_theme(false, false)),
        },
        Command::New { from, dest, force } => emit(&verbs::new::run(&from, &dest, force)),
        Command::Completions { shell } => {
            let mut cmd = Cli::command();
            clap_complete::generate(shell, &mut cmd, "nika-cli", &mut std::io::stdout());
            0
        }
        Command::Trace { action } => match action {
            TraceAction::Replay(args) => trace_render(&args, true),
            TraceAction::Show(args) => trace_render(&args, false),
        },
        // The language server OWNS stdout (JSON-RPC) — it must not go through
        // `emit`. It follows the LSP exit-code convention: 0 on a clean
        // shutdown/exit, non-zero (1) otherwise (transport failure, or an
        // `exit` without a prior `shutdown`) — the server-process
        // convention, NOT the verb FILE/WORKFLOW/ENV taxonomy.
        Command::Lsp => match nika_lsp::run_stdio() {
            Ok(()) => verbs::exit::OK,
            Err(err) => {
                eprintln!("nika-cli: lsp: {err}");
                1
            }
        },
        // The MCP server OWNS stdout (JSON-RPC) — like `lsp`, it must not go
        // through `emit`. Same server-process exit convention: 0 on a clean
        // EOF shutdown, 1 on a transport failure.
        Command::Mcp => match nika_mcp::run_stdio() {
            Ok(()) => verbs::exit::OK,
            Err(err) => {
                eprintln!("nika-cli: mcp: {err}");
                1
            }
        },
    };
    std::process::ExitCode::from(code)
}

/// Print a verb's text on the right stream and return its exit code.
/// Findings + successes go to stdout (they ARE the product); only
/// environment errors go to stderr.
fn emit(out: &VerbOutput) -> u8 {
    if out.code == verbs::exit::ENV {
        eprintln!("nika-cli: {}", out.text);
    } else if !out.text.is_empty() {
        println!("{}", out.text.trim_end());
    }
    out.code
}

/// Resolve the colour/glyph theme for static (non-animated) surfaces.
fn term_theme(no_color: bool, ascii: bool) -> Theme {
    let tty = std::io::stdout().is_terminal();
    Theme {
        color: tty && !no_color && !env_flag("NO_COLOR"),
        ascii,
        animate: false,
    }
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
