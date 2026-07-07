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
use nika_cli::display::format::{ColorChoice, ColorEnv, LinkChoice, color_enabled, links_enabled};
use nika_cli::verbs::{self, VerbOutput};
use nika_cli::{RunView, Theme, frame};
use nika_event::Event;

#[derive(Parser)]
// The PUBLIC binary name is `nika` (the release renames the nika-cli artifact +
// the Homebrew formula tests `nika --version`); clap embeds THIS name, not the
// filename, so the version/usage/errors must say `nika`, not the seed crate name.
#[command(
    name = "nika",
    bin_name = "nika",
    version,
    about = "nika · the AI workflow engine — operator surface",
    // The lost-user footer (clig.dev · suggest the next command): a bare
    // `nika` is someone asking where to start, not someone reading a
    // reference. Three commands, zero keys, offline.
    after_help = "start here:\n  nika init                                      # wire this repo (editor · agents)\n  nika examples run 01-hello --model mock/echo   # offline proof · zero keys\n  nika doctor                                    # what's configured · what's missing"
)]
struct Cli {
    /// When to colour the output (auto = TTY + `TERM != dumb` · honours
    /// `CLICOLOR_FORCE` · `NO_COLOR` · `CLICOLOR=0` in that order).
    #[arg(long, global = true, value_enum, default_value_t = ColorWhenArg::Auto)]
    color: ColorWhenArg,
    /// When to emit OSC-8 hyperlinks on printed paths (auto = TTY +
    /// `TERM != dumb` · never to pipes; always = force them, for pagers
    /// that pass escapes — tmux/screen may render them as plain text).
    #[arg(long, global = true, value_enum, default_value_t = LinkWhenArg::Auto)]
    hyperlink: LinkWhenArg,
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum LinkWhenArg {
    /// Force hyperlinks on (escape-passing pagers · captured demos).
    Always,
    /// Force hyperlinks off.
    Never,
    /// TTY + `TERM != dumb` — never to pipes (the default).
    Auto,
}

impl LinkWhenArg {
    fn choice(self) -> LinkChoice {
        match self {
            Self::Always => LinkChoice::Always,
            Self::Never => LinkChoice::Never,
            Self::Auto => LinkChoice::Auto,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum ColorWhenArg {
    /// Force colour on (pagers accepting escapes · captured demos).
    Always,
    /// Force colour off (the `--no-color` flags fold here).
    Never,
    /// Resolve from the environment chain + TTY (the default).
    Auto,
}

impl ColorWhenArg {
    /// Fold a verb's legacy `--no-color` sugar into the tri-state: an
    /// explicit off wins (both flags together = the conservative read).
    fn with_no_color(self, no_color: bool) -> ColorChoice {
        if no_color {
            return ColorChoice::Never;
        }
        match self {
            Self::Always => ColorChoice::Always,
            Self::Never => ColorChoice::Never,
            Self::Auto => ColorChoice::Auto,
        }
    }
}

#[derive(Subcommand)]
enum Command {
    /// Static pre-flight: the ADR-092 ladder (audit BEFORE run).
    Check {
        /// Workflow file (`*.nika.yaml`) · `-` reads stdin.
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
    Run(RunArgs),
    /// Golden test: run under the MOCK provider (offline · deterministic)
    /// and compare the typed `outputs:` against `<file>.golden.json`.
    Test {
        /// Workflow file (`*.nika.yaml`).
        file: String,
        /// (Re)write the golden from this run instead of comparing.
        #[arg(long)]
        update: bool,
        /// Disable colour output.
        #[arg(long)]
        no_color: bool,
        /// Force the ASCII glyph theme.
        #[arg(long)]
        ascii: bool,
    },
    /// Static anatomy: tasks · verbs · wave groups · cost · permits.
    Inspect {
        /// Workflow file (`*.nika.yaml`) · `-` reads stdin.
        file: String,
        /// Force the ASCII glyph theme (CI logs · legacy terminals).
        #[arg(long)]
        ascii: bool,
    },
    /// The ONE graph projector (json canonical · mermaid/dot derived).
    Graph {
        /// Workflow file (`*.nika.yaml`) · `-` reads stdin.
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
    Doctor {
        /// TCP-probe the local provider ports (loopback/configured only ·
        /// 300ms cap · nothing is sent on the socket). Offline without it.
        #[arg(long)]
        ping: bool,
        /// Emit the findings as JSON (summary + findings[] · agents/CI
        /// branch on `summary.fail` instead of parsing glyphs).
        #[arg(long)]
        json: bool,
    },
    /// Scaffold a repo (`.vscode` schema wiring · `AGENTS.md` · Cursor rule ·
    /// `.agents/skills` authoring skill). Existing files are skipped —
    /// `--force` overwrites.
    Init {
        /// Target directory (default · the current directory).
        #[arg(default_value = ".")]
        dir: String,
        /// Overwrite existing files.
        #[arg(long)]
        force: bool,
        /// Accept every default — never prompt (pipes and CI are
        /// implicitly `--yes`; prompts only ever appear on a terminal).
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Wire Nika into editor/agent MCP clients (explicit, idempotent).
    Wire {
        /// Client to wire.
        #[arg(value_enum)]
        target: WireTargetArg,
        /// Workspace directory for repo-local clients such as VS Code.
        #[arg(long, default_value = ".")]
        dir: String,
    },
    /// The embedded spec identity (`--canon` prints the SSOT).
    Spec {
        /// Print the canon.yaml single source of truth.
        #[arg(long)]
        canon: bool,
    },
    /// The embedded JSON Schema for `*.nika.yaml`.
    Schema,
    /// The embedded provider/model catalog (models · capabilities · env vars).
    Catalog {
        /// Emit the versioned machine projection (`catalog_version: 1`).
        #[arg(long)]
        json: bool,
    },
    /// The embedded builtin tool catalog (`nika:*` · model-facing schemas).
    Tools {
        /// Emit the versioned machine projection (`tools_version: 1`).
        #[arg(long)]
        json: bool,
    },
    /// Browse the embedded examples.
    Examples {
        #[command(subcommand)]
        action: ExamplesAction,
    },
    /// Instantiate an embedded template skeleton.
    New {
        /// Template name or plain-words intent (`--from '?'` lists the
        /// set). Omitted on a terminal → the guided three-question flow;
        /// omitted in a pipe → fail fast naming this flag.
        #[arg(long)]
        from: Option<String>,
        /// Destination path (`*.nika.yaml`). Optional for the `--from '?'`
        /// discovery query; required to instantiate a template.
        dest: Option<String>,
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
    /// Debug Adapter Protocol server (stdio) — time-travel a recorded
    /// run under a debugger UI: breakpoints on task lines · step forward
    /// AND back through settles · outputs in the variables pane. Replay
    /// re-renders, never re-executes.
    Dap,
    /// Run the language server over stdio (drives the editor extension).
    Lsp,
    /// Run the MCP server over stdio (validate: check/explain · learn:
    /// schema/examples/templates/canon — the in-binary Model Context Protocol
    /// surface for Cursor · Claude Desktop · agents).
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum WireTargetArg {
    Cursor,
    Vscode,
    Windsurf,
    Claude,
    Codex,
    All,
}

impl From<WireTargetArg> for verbs::wire::WireTarget {
    fn from(value: WireTargetArg) -> Self {
        match value {
            WireTargetArg::Cursor => Self::Cursor,
            WireTargetArg::Vscode => Self::Vscode,
            WireTargetArg::Windsurf => Self::Windsurf,
            WireTargetArg::Claude => Self::Claude,
            WireTargetArg::Codex => Self::Codex,
            WireTargetArg::All => Self::All,
        }
    }
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
        /// Override the example's `model:` (`<provider>/<name>`). Use
        /// `--model mock/echo` to preview offline (zero key · zero network).
        #[arg(long, value_name = "PROVIDER/NAME")]
        model: Option<String>,
    },
}

#[derive(Subcommand)]
enum TraceAction {
    /// Re-render a run live (replay = re-render, NEVER re-execute).
    Replay(TraceArgs),
    /// Print the final card only.
    Show(TraceArgs),
    /// Browse per-task outputs: verb · duration · tokens · bounded
    /// preview (full value: `trace peek`).
    Outputs {
        /// Trace NDJSON path (one `nika-event` Event per line).
        trace: PathBuf,
        /// Force the ASCII glyph theme.
        #[arg(long)]
        ascii: bool,
        /// Disable colour output.
        #[arg(long)]
        no_color: bool,
    },
    /// Project the journal to OTLP/JSON lines — every `OTel` tool becomes
    /// a viewer (drag into Jaeger UI ≥1.60 · POST lines to any OTLP/HTTP
    /// endpoint). Local file, zero collector, zero vendor.
    Export {
        /// Trace NDJSON path (one `nika-event` Event per line).
        trace: PathBuf,
        /// Output path (default: `<trace>.otlp.jsonl` beside the journal).
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Include recorded task outputs as span attributes (payloads
        /// stay LOCAL either way — this only widens the exported file).
        #[arg(long)]
        include_content: bool,
    },
    /// Verify the journal's tamper-evidence chain (0.96+): any edited,
    /// inserted, dropped or reordered line breaks every hash after it.
    /// Exit 0 intact · 2 broken · 3 unchained (pre-chain journal).
    Verify {
        /// Trace NDJSON path (one `nika-event` Event per line).
        trace: PathBuf,
    },
    /// Is this run reproducible? Compare a recorded journal against a
    /// fresh one and classify every task: reproduced · nondeterministic
    /// (same def+inputs, different output) · authored · environment ·
    /// status-changed · unverifiable. Exit 0 reproduced · 2 diverged.
    Reproduce {
        /// The RECORDED journal (the reference frame).
        recorded: PathBuf,
        /// A FRESH journal of the same workflow (run it again first).
        fresh: PathBuf,
    },
    /// Read ONE task's full output + its identity (hashes · duration ·
    /// tokens). `--raw` prints the exact value only (pipe it to jq).
    Peek {
        /// Trace NDJSON path (one `nika-event` Event per line).
        trace: PathBuf,
        /// The task id whose output to read.
        task: String,
        /// Print the exact recorded value only (machine-friendly).
        #[arg(long)]
        raw: bool,
        /// Force the ASCII glyph theme.
        #[arg(long)]
        ascii: bool,
        /// Disable colour output.
        #[arg(long)]
        no_color: bool,
    },
    /// The data waterfall: which output fed which task, with recorded
    /// sizes (plan bindings from the workflow file × sizes from the
    /// trace).
    Flow {
        /// Trace NDJSON path (one `nika-event` Event per line).
        trace: PathBuf,
        /// The workflow file the run executed (`*.nika.yaml`) — the
        /// trace records values, the definition records the bindings.
        workflow: String,
        /// Force the ASCII glyph theme.
        #[arg(long)]
        ascii: bool,
        /// Disable colour output.
        #[arg(long)]
        no_color: bool,
    },
}

#[derive(Args)]
// Six independent CLI flags ARE six bools — the clap-surface idiom
// (same as TraceArgs), not a state machine to encode.
#[allow(clippy::struct_excessive_bools)]
struct RunArgs {
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
    /// Plain render: one final storyboard frame, no animation (the
    /// CI-stable surface · also the default when stdout is piped).
    /// A human surface — meaningless with the `--json`/`--output` machine
    /// modes, so refused there (the machine surface owns its rendering).
    #[arg(long, conflicts_with_all = ["json", "output"])]
    no_progress: bool,
    /// Quiet: print only the final verdict card (errors always). A human
    /// surface · refused with `--no-progress` and the machine modes.
    #[arg(long, conflicts_with_all = ["no_progress", "json", "output"])]
    quiet: bool,
    /// Plan only — show the static plan and execute ZERO effects (spec §10).
    /// A human plan preview · refused with the `--json`/`--output` machine
    /// modes (no machine dry-run form yet · would silently corrupt stdout).
    #[arg(long, conflicts_with_all = ["json", "output"])]
    dry_run: bool,
    /// Override the workflow's envelope `model:` (`<provider>/<name>`).
    /// Resolved through the SAME path as an envelope model — a bad id
    /// fails loud when an infer/agent task resolves it. `--model
    /// mock/echo` previews any workflow offline (zero key · zero network).
    #[arg(long, value_name = "PROVIDER/NAME")]
    model: Option<String>,
    /// Set a workflow `vars:` value (repeatable). Overrides a declared
    /// `default:` and satisfies a `required: true` var. The value is
    /// parsed as JSON when it parses (numbers · booleans · arrays),
    /// else taken as a string. Unknown keys are refused.
    #[arg(long = "var", value_name = "KEY=VALUE")]
    var: Vec<String>,
    /// Resume from a prior run's NDJSON trace (`nika run … --json >
    /// trace.ndjson`): every task whose identity matches a journaled
    /// success is skipped with a visible `task_cache_hit` — an edited
    /// task or a changed input always re-runs (ADR-099). A trace without
    /// resume keys runs everything live (a notice, never an error).
    #[arg(long, value_name = "TRACE", conflicts_with = "dry_run")]
    resume: Option<PathBuf>,
    /// Force this task AND its transitive downstream to re-run even on an
    /// identity match (the lever for changes the hashes cannot see —
    /// rotated secret · external state · an infer output to re-roll).
    #[arg(long, value_name = "TASK_ID", requires = "resume")]
    from: Option<String>,
    /// Answer a paused `nika:prompt` at resume (repeatable · ADR-099
    /// rider): binds as the named task's answer — `--answer ok=true` for
    /// confirm, a string for input, one of the choices for choice. The
    /// value parses as JSON when it parses, else rides as a string.
    #[arg(long = "answer", value_name = "TASK=VALUE", requires = "resume")]
    answer: Vec<String>,
    /// Run ONE task and its transitive upstream only (the regenerate-one-
    /// block move): the full workflow still audits (spans · findings stay
    /// whole-file faithful), then execution scopes to the ancestor
    /// sub-DAG and the plan/cost re-derive for exactly what will run.
    /// Workflow `outputs:` are skipped (they may read unscoped tasks).
    #[arg(long, value_name = "TASK_ID", conflicts_with = "resume")]
    task: Option<String>,
    /// Skip the run journal (`.nika/traces/<ts>-<id>.ndjson` · spec §3.3).
    /// Every run writes one by default so `nika trace show|replay`,
    /// `--resume` and the editor's runs view have a file to read.
    /// `NIKA_NO_TRACE_FILE` (any non-empty value) opts out globally.
    #[arg(long)]
    no_trace_file: bool,
    /// Hide the per-task output summaries (`→ {…} · 312B`) on the live
    /// storyboard. Interactive TTY only — pipes · CI · the machine modes
    /// never carry them anyway.
    #[arg(long)]
    no_outputs: bool,
}

#[derive(Args)]
// Five independent CLI flags ARE five bools — the clap-surface idiom, not
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
    /// Hide the per-task output summaries (`→ {…} · 312B`) on the
    /// rendered storyboard. Interactive TTY only — a piped `trace show`
    /// never carries them anyway.
    #[arg(long)]
    no_outputs: bool,
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let color = cli.color;
    let link_when = cli.hyperlink.choice();
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
                verbs::check::run(
                    &file,
                    json,
                    term_theme(color.with_no_color(no_color), ascii, link_when),
                )
            };
            emit(&out)
        }
        Command::Run(args) => run_verb(&args, color, link_when),
        Command::Test {
            file,
            update,
            no_color,
            ascii,
        } => verbs::test::run(
            &file,
            update,
            term_theme(color.with_no_color(no_color), ascii, link_when),
        ),
        Command::Inspect { file, ascii } => emit(&verbs::inspect::run(&file, ascii)),
        Command::Graph { file, format } => {
            let format = match format {
                GraphFormatArg::Json => verbs::graph::GraphFormat::Json,
                GraphFormatArg::Mermaid => verbs::graph::GraphFormat::Mermaid,
                GraphFormatArg::Dot => verbs::graph::GraphFormat::Dot,
            };
            emit(&verbs::graph::run(&file, format))
        }
        Command::Explain { code } => emit(&verbs::explain::run(&code)),
        Command::Doctor { ping, json } => emit(&verbs::doctor::run(ping, json)),
        Command::Init { dir, force, yes } => emit(&verbs::init::run(&dir, force, yes)),
        Command::Wire { target, dir } => emit(&verbs::wire::run(target.into(), &dir)),
        Command::Spec { canon } => emit(&verbs::pack_surface::spec(canon)),
        Command::Schema => emit(&verbs::pack_surface::schema()),
        Command::Catalog { json } => emit(&verbs::catalog::run(json)),
        Command::Tools { json } => emit(&verbs::tools::run(json)),
        Command::Examples { action } => match action {
            ExamplesAction::List => emit(&verbs::pack_surface::examples_list()),
            ExamplesAction::Show { slug } => emit(&verbs::pack_surface::examples_show(&slug)),
            // The L3 run verb shipped — execute the embedded example for real.
            ExamplesAction::Run { slug, model } => verbs::run::example(
                &slug,
                model.as_deref(),
                term_theme(color.with_no_color(false), false, link_when),
            ),
        },
        Command::New { from, dest, force } => emit(&verbs::new::dispatch(
            from.as_deref(),
            dest.as_deref(),
            force,
        )),
        Command::Completions { shell } => {
            write_completions(shell, &mut std::io::stdout());
            0
        }
        Command::Trace { action } => trace_verb(action, color, link_when),
        // The language server OWNS stdout (JSON-RPC) — it must not go through
        // `emit`. It follows the LSP exit-code convention: 0 on a clean
        // shutdown/exit, non-zero (1) otherwise (transport failure, or an
        // `exit` without a prior `shutdown`) — the server-process
        // convention, NOT the verb FILE/WORKFLOW/ENV taxonomy.
        Command::Dap => verbs::dap::run_stdio(),
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

/// Dispatch the `trace` verb family: the live renders (replay · show)
/// plus the static readers (outputs · peek · flow).
fn trace_verb(action: TraceAction, color: ColorWhenArg, link_when: LinkChoice) -> u8 {
    match action {
        TraceAction::Replay(args) => trace_render(&args, true, color, link_when),
        TraceAction::Show(args) => trace_render(&args, false, color, link_when),
        TraceAction::Outputs {
            trace,
            ascii,
            no_color,
        } => {
            let mut theme = term_theme(color.with_no_color(no_color), ascii, link_when);
            // The dur column's bracket accents: TTY comfort only.
            theme.accents = std::io::stdout().is_terminal();
            emit(&verbs::trace::outputs(&trace.to_string_lossy(), theme))
        }
        TraceAction::Verify { trace } => {
            emit(&verbs::trace_verify::verify(&trace.to_string_lossy()))
        }
        TraceAction::Reproduce { recorded, fresh } => emit(&verbs::trace_reproduce::reproduce(
            &recorded.to_string_lossy(),
            &fresh.to_string_lossy(),
        )),
        TraceAction::Export {
            trace,
            out,
            include_content,
        } => emit(&verbs::trace_otel::export(
            &trace.to_string_lossy(),
            out.as_deref()
                .map(|p| p.to_string_lossy().into_owned())
                .as_deref(),
            include_content,
        )),
        TraceAction::Peek {
            trace,
            task,
            raw,
            ascii,
            no_color,
        } => emit(&verbs::trace::peek(
            &trace.to_string_lossy(),
            &task,
            raw,
            term_theme(color.with_no_color(no_color), ascii, link_when),
        )),
        TraceAction::Flow {
            trace,
            workflow,
            ascii,
            no_color,
        } => emit(&verbs::trace::flow(
            &trace.to_string_lossy(),
            &workflow,
            term_theme(color.with_no_color(no_color), ascii, link_when),
        )),
    }
}

/// Unpack the `run` clap surface into the library verb call.
fn run_verb(args: &RunArgs, color: ColorWhenArg, link_when: LinkChoice) -> u8 {
    let resume = args.resume.as_ref().map(|trace| verbs::run::ResumeRequest {
        trace: trace.clone(),
        from: args.from.clone(),
        answers: args.answer.clone(),
    });
    let mode = resolve_run_mode(args.quiet, args.no_progress);
    let mut theme = term_theme(color.with_no_color(args.no_color), args.ascii, link_when);
    // The duration accents ride the interactive surface ONLY — the
    // sober registers (piped · --no-progress · --quiet) keep their
    // exact bytes.
    theme.accents = mode == verbs::run::RenderMode::Live;
    // Duration heat additionally needs colour + the truecolor PROOF.
    theme.heat = theme.accents && theme.color && truecolor_env();
    verbs::run::run(
        &args.file,
        args.json,
        args.output.as_deref(),
        theme,
        mode,
        args.dry_run,
        args.model.as_deref(),
        &args.var,
        resume.as_ref(),
        args.no_trace_file || env_flag("NIKA_NO_TRACE_FILE"),
        args.task.as_deref(),
        args.no_outputs,
    )
}

/// Resolve the live-render surface for `run` (spec §3.5 reduced surfaces):
/// `--quiet` wins → the compact verdict card only; `--no-progress` OR a piped
/// stdout → the plain final storyboard (no animation · CI-stable); otherwise
/// the rich in-place repaint.
fn resolve_run_mode(quiet: bool, no_progress: bool) -> verbs::run::RenderMode {
    use verbs::run::RenderMode;
    if quiet {
        RenderMode::Quiet
    } else if no_progress || !std::io::stdout().is_terminal() {
        RenderMode::Plain
    } else {
        RenderMode::Live
    }
}

/// Write shell completions attached to the PUBLIC binary name — `nika`,
/// never the seed crate's file name (`#compdef nika-cli` would wire
/// completions to a command users never type · found live 2026-07-05).
fn write_completions(shell: clap_complete::Shell, out: &mut dyn std::io::Write) {
    let mut cmd = Cli::command();
    clap_complete::generate(shell, &mut cmd, "nika", out);
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

/// Resolve the colour/glyph/link theme for static (non-animated)
/// surfaces — colour and hyperlinks each ride their own capability
/// chain over the SAME environment facts.
fn term_theme(choice: ColorChoice, ascii: bool, link_when: LinkChoice) -> Theme {
    let tty = std::io::stdout().is_terminal();
    let env = color_env();
    let mut theme = Theme::new(color_enabled(choice, env, tty), ascii, false);
    theme.links = links_enabled(link_when, tty, env.term_dumb);
    theme
}

/// Load events, fold, render — live replay or final card.
fn trace_render(args: &TraceArgs, replay: bool, color: ColorWhenArg, link_when: LinkChoice) -> u8 {
    let events = match load_events(args) {
        Ok(events) => events,
        Err(message) => {
            eprintln!("nika trace: {message}");
            eprintln!(
                "  fix: a trace is the NDJSON a run records — nika run <wf> --json > run.ndjson"
            );
            return verbs::exit::ENV; // environment error (spec §4)
        }
    };

    let tty = std::io::stdout().is_terminal();
    let mut theme = term_theme(color.with_no_color(args.no_color), args.ascii, link_when);
    theme.animate = tty && replay && !env_flag("NIKA_REDUCED_MOTION");
    // The duration accents ride the interactive surface only — a piped
    // `trace show` keeps its exact legacy bytes.
    theme.accents = tty;
    // Duration heat additionally needs colour + the truecolor PROOF.
    theme.heat = tty && theme.color && truecolor_env();

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
            nika_cli::frame_with_outputs(&view, &theme, 0)
        } else {
            frame(&view, &theme, 0)
        };
        print_lines(&lines);
    }
    // The trace surface owns the run overlays (replay = re-render, never
    // re-execute): the waterfall + the verdict card close the read, from
    // any past trace — the same final frame a live TTY run ends on.
    print_lines(&nika_cli::display::flow::waterfall(&view, &theme));
    print_lines(&nika_cli::display::flow::verdict_card(&view, &theme, None));
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
        nika_cli::frame_with_outputs(view, &theme, tick)
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
    recover_events(&raw, &path.display().to_string())
}

/// Parse an NDJSON trace, tolerating a truncated/corrupt TAIL — a crashed run
/// (SIGSEGV · OOM · hard kill) leaves a half-written last line, and recovering
/// it is the whole point of a flight recorder. Delegates to the library's
/// tolerant reader (the SAME one `nika run --resume` folds through — one
/// recovery contract, two consumers) and surfaces the truncation note here.
fn recover_events(raw: &str, label: &str) -> Result<Vec<Event>, String> {
    let recovered = verbs::run::recover_events(raw, label)?;
    if let Some(note) = &recovered.truncated_note {
        eprintln!("nika-cli: {note} — rendering the recovered prefix");
    }
    Ok(recovered.events)
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

/// Did the terminal PROVE truecolor (`COLORTERM=truecolor|24bit`)?
/// The duration-heat ramp fires only on proof — 256-colour terminals
/// get the flat fallback, never an approximated ramp (design §1.5).
fn truecolor_env() -> bool {
    env_value("COLORTERM").is_some_and(|v| v == "truecolor" || v == "24bit")
}

/// Read a presentation variable's VALUE (`CLICOLOR` · `TERM` ·
/// `COLORTERM`) — the same non-secret seam as [`env_flag`].
#[allow(clippy::disallowed_methods)]
fn env_value(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;

    /// The public name is `nika` EVERYWHERE clap speaks (found live
    /// 2026-07-05): the Usage line (`bin_name`) and the generated shell
    /// completions — `#compdef nika-cli` attached completions to a
    /// command users never type, dead on arrival.
    #[test]
    fn clap_surfaces_speak_the_public_binary_name() {
        let help = Cli::command().render_help().to_string();
        assert!(help.contains("Usage: nika "), "usage speaks `nika`: {help}");
        assert!(!help.contains("nika-cli"), "the seed name never leaks");

        let mut zsh = Vec::new();
        write_completions(clap_complete::Shell::Zsh, &mut zsh);
        let zsh = String::from_utf8(zsh).expect("utf8");
        assert!(
            zsh.starts_with("#compdef nika\n"),
            "zsh attaches to `nika`: {}",
            zsh.lines().next().unwrap_or_default()
        );

        let mut bash = Vec::new();
        write_completions(clap_complete::Shell::Bash, &mut bash);
        let bash = String::from_utf8(bash).expect("utf8");
        assert!(
            bash.contains("complete -F _nika -o nosort -o bashdefault -o default nika")
                || (bash.contains("default nika") && !bash.contains("nika-cli")),
            "bash completes `nika`, never nika-cli: {bash}"
        );
        assert!(!bash.contains("nika-cli"), "the seed name never leaks");
    }

    /// One valid serialized Event line — reuse the demo's real events.
    fn valid_line() -> String {
        let events = nika_cli::demo::success();
        let first = events.first().expect("demo has events");
        serde_json::to_string(first).expect("event serializes")
    }

    /// Flight-recorder resilience: a crashed run leaves a truncated last line.
    /// The reader must render the valid PREFIX, not lose the whole trace.
    #[test]
    fn truncated_tail_recovers_the_valid_prefix() {
        let v = valid_line();
        // 2 valid events + a truncated 3rd line (the crash signature).
        let raw = format!("{v}\n{v}\n{{\"id\":{{\"uuid\":\"trunc");
        let events = recover_events(&raw, "t").expect("recovers the valid prefix");
        assert_eq!(events.len(), 2, "both valid events recovered");
    }

    /// A bad FIRST line (nothing recovered) is genuinely unreadable → error;
    /// an empty trace likewise.
    #[test]
    fn bad_first_line_and_empty_are_hard_errors() {
        assert!(recover_events("{not json at all\n", "t").is_err());
        assert!(recover_events("", "t").is_err(), "empty trace errors");
    }
}
