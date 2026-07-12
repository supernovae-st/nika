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
use std::path::{Path, PathBuf};
use std::time::Duration;

mod examples_args;
mod init_args;
mod lazy;
mod registry_args;

use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use nika_cli::display::format::{ColorChoice, ColorEnv, LinkChoice, color_enabled, links_enabled};
use nika_cli::verbs::explain_file::dispatch as explain_dispatch;
use nika_cli::verbs::{self, VerbOutput};
use nika_cli::{RunView, Theme, frame};

use examples_args::{ExamplesAction, examples_verb};
use init_args::{InitArgs, init_verb};
use lazy::{check_lazy, run_lazy};
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
    after_help = "start here:\n  nika welcome                                   # what this machine has · where to start\n  nika init                                      # found this repo — the wizard on a terminal\n  nika new                                       # your first workflow — guided on a terminal\n  nika examples run 01-hello --model mock/echo   # offline proof · zero keys\n  nika doctor                                    # what's configured · what's missing"
)]
struct Cli {
    /// When to colour the output (auto = TTY + `TERM != dumb` · honours
    /// `CLICOLOR_FORCE` · `NO_COLOR` · `CLICOLOR=0` in that order).
    #[arg(long, global = true, value_enum, default_value_t = ColorWhenArg::Auto, display_order = 900)]
    color: ColorWhenArg,
    /// When to emit OSC-8 hyperlinks on printed paths (auto = TTY +
    /// `TERM != dumb` · never to pipes; always = force them, for pagers
    /// that pass escapes — tmux/screen may render them as plain text).
    #[arg(long, global = true, value_enum, default_value_t = LinkWhenArg::Auto, display_order = 901)]
    hyperlink: LinkWhenArg,
    /// The sober umbrella — one flag for scripts, CI and transcripts:
    /// colour off · ASCII glyphs · hyperlinks off · no animation (`run`
    /// renders its plain storyboard). The same result as `--color never
    /// --hyperlink never` plus every verb's `--ascii`/`--no-progress`.
    #[arg(long, global = true, display_order = 902)]
    plain: bool,
    #[command(subcommand)]
    command: Option<Command>,
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
    /// The mirror: what Nika is · what this machine already has (editors ·
    /// local models · key presence · this workspace) · the next commands.
    /// Offline · presence-only · always exit 0 — a greeting, not a gate.
    Welcome {
        /// Emit the versioned machine mirror (`welcome_version: 1`).
        #[arg(long)]
        json: bool,
        /// Force the ASCII glyph theme (CI logs · legacy terminals).
        #[arg(long)]
        ascii: bool,
    },
    /// The whole workspace truth in ONE call — every workflow audited
    /// (verdict · tasks · waves · cost honesty · permits), recent runs
    /// folded from the flight recorder, the machine facts. Capped and
    /// says so; facts, never file contents.
    Context {
        /// Emit the versioned machine aggregate (`context_version: 1`).
        #[arg(long)]
        json: bool,
        /// Force the ASCII glyph theme (CI logs · legacy terminals).
        #[arg(long)]
        ascii: bool,
    },
    /// Audit a workflow BEFORE it runs: plan · cost ceiling · secret
    /// flows · types · tools — every finding teaches its fix.
    Check {
        /// Workflow file(s) (`*.nika.yaml`) · `-` reads stdin · or a verified
        /// `registry:owner/name[@version]` pull (cached + offline; workflow
        /// `permits:` never govern the fetch) · several files audit in sequence
        /// (worst exit wins — the CI shape) · `--json`/`--infer-permits` one-file-per-call.
        /// Omitted with exactly one workflow here → that one is audited.
        #[arg(num_args = 0..)]
        files: Vec<String>,
        /// Emit the machine-readable report (never coloured).
        #[arg(long)]
        json: bool,
        /// Print an inferred `permits:` boundary instead of the report.
        #[arg(long)]
        infer_permits: bool,
        /// Apply the machine-applicable rename repairs (typed
        /// did-you-mean suggestions only: fields · tools · args), rewrite
        /// the file, and re-audit — the in-binary repair loop
        /// (`clippy --fix` shape). One real file; ambiguous tokens are
        /// skipped with a note, never guessed.
        #[arg(long)]
        fix: bool,
        /// Fail (exit 2) when any `native-first` hint remains — an
        /// `exec:` a builtin or MCP tool probably covers. The agent/CI
        /// posture; hints stay advisory without it.
        #[arg(long)]
        native_strict: bool,
        /// Price the static envelope AS IF this `<provider>/<model>`
        /// replaced the envelope default — the preview of `nika run
        /// --model` (per-task `model:` still wins, like the runtime).
        #[arg(long)]
        model: Option<String>,
        /// Disable colour output.
        #[arg(long)]
        no_color: bool,
        /// Force the ASCII glyph theme.
        #[arg(long)]
        ascii: bool,
    },
    /// Run a workflow (the same audit runs first · live render).
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
    /// Teach one error code (cause · category · fix-form) — or narrate a
    /// workflow FILE: what it does · the waves · cost before a token is
    /// spent · what it touches · how to run it.
    Explain {
        /// An error code (`NIKA-440` · bare `440` · `DAG-003`) or a
        /// workflow file path (`*.nika.yaml` · `-` reads stdin).
        code: String,
        /// File form only: emit the versioned machine twin
        /// (`explain_version: 1` · the check report's own vocabulary).
        #[arg(long)]
        json: bool,
        /// File form only: include the learned-truth forecast — duration/
        /// cost/risk priors from YOUR local traces (stats over
        /// `.nika/traces/` · never a model call · never the network).
        #[arg(long)]
        forecast: bool,
    },
    /// Diagnose this machine (binary · config · provider keys · local models).
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
    /// Found a repo (`.vscode` schema wiring · `AGENTS.md` · Cursor rule + MCP ·
    /// `.agents/skills` authoring skill · optional workflow set). Bare on
    /// a terminal the founding wizard runs; flags are the scriptable
    /// twin. Existing files are skipped — `--force` overwrites.
    Init(InitArgs),
    /// Wire Nika into editor/agent MCP clients (explicit, idempotent).
    Wire {
        /// Client to wire.
        #[arg(value_enum)]
        target: verbs::wire::WireTarget,
        /// Workspace directory for repo-local clients such as VS Code.
        #[arg(long, default_value = ".")]
        dir: String,
    },
    /// Local models — serve one on this machine (no cloud, no external daemon).
    Model {
        #[command(subcommand)]
        action: ModelAction,
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
        /// Bare `nika examples` lists — the clap usage screen answered
        /// where a user following init's « next · » expected the slugs
        /// (the user-sim finding).
        #[command(subcommand)]
        action: Option<ExamplesAction>,
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
    /// Generate shell completions (bash · zsh · fish · elvish · powershell).
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
    /// Run the MCP server (validate: check/explain · learn:
    /// schema/examples/templates/canon — the in-binary Model Context Protocol
    /// surface for Cursor · Claude Desktop · agents). Default transport:
    /// stdio; `--transport http` serves Streamable HTTP for managed hosts.
    Mcp {
        /// The wire: `stdio` (the editor/agent default) or `http`
        /// (Streamable HTTP · POST JSON-RPC · spec 2025-11-25).
        #[arg(long, value_enum, default_value_t = McpTransportArg::Stdio)]
        transport: McpTransportArg,
        /// HTTP port (with `--transport http`).
        #[arg(long, default_value_t = 8123)]
        port: u16,
        /// HTTP bind address. Loopback by default — widening this exposes
        /// the server to your network; put TLS + auth (a reverse proxy ·
        /// `NIKA_MCP_TOKEN`) in front before you do.
        #[arg(long, default_value = "127.0.0.1")]
        bind: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum McpTransportArg {
    /// Newline-delimited JSON-RPC over stdin/stdout.
    Stdio,
    /// Streamable HTTP (POST JSON-RPC · origin-gated · loopback default).
    Http,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum GraphFormatArg {
    /// Canonical JSON projection (`graph_format: 1`).
    Json,
    /// Mermaid flowchart.
    Mermaid,
    /// Graphviz dot.
    Dot,
    /// Terminal drawing (waves as columns · real wires · honest fallback).
    Ascii,
}

impl From<GraphFormatArg> for verbs::graph::GraphFormat {
    fn from(arg: GraphFormatArg) -> Self {
        match arg {
            GraphFormatArg::Json => Self::Json,
            GraphFormatArg::Mermaid => Self::Mermaid,
            GraphFormatArg::Dot => Self::Dot,
            GraphFormatArg::Ascii => Self::Ascii,
        }
    }
}

#[derive(Subcommand)]
enum ModelAction {
    /// Serve a GGUF model — an OpenAI-compatible foreground server on
    /// 127.0.0.1 (Ctrl-C stops it · the banner says how workflows reach it).
    Serve {
        /// The model weights (a Qwen3-family `.gguf` file).
        #[arg(long, value_name = "PATH.gguf")]
        model: PathBuf,
        /// The tokenizer file (default: `tokenizer.json` beside the model).
        #[arg(long, value_name = "PATH")]
        tokenizer: Option<PathBuf>,
        /// Loopback port to listen on.
        #[arg(long, default_value_t = verbs::model::DEFAULT_PORT)]
        port: u16,
        /// The model id responses report (default: the model file's name).
        #[arg(long, value_name = "ID")]
        model_id: Option<String>,
    },
}

#[derive(Subcommand)]
enum TraceAction {
    /// Re-render a run live (replay = re-render, NEVER re-execute).
    Replay(TraceArgs),
    /// Print the final card only.
    Show(TraceArgs),
    /// List the workspace trace store (`.nika/traces/`): age · size ·
    /// workflow · terminal state (completed/failed/paused) · the
    /// resume-candidate marker (★ — the newest of each workflow, the
    /// trace retention never collects · ADR-100).
    Ls {
        /// Force the ASCII glyph theme.
        #[arg(long)]
        ascii: bool,
        /// Disable colour output.
        #[arg(long)]
        no_color: bool,
    },
    /// Remove traces from the store — one by name/path, `--older-than
    /// <dur>`, or `--all`. Removing a paused trace refuses without
    /// `--force` and names the unanswered prompt it would destroy
    /// (ADR-100).
    Rm {
        /// The trace to remove — a name from `trace ls` or a path.
        #[arg(required_unless_present_any = ["older_than", "all"],
              conflicts_with_all = ["older_than", "all"])]
        trace: Option<String>,
        /// Remove every trace older than this (`45s` · `30m` · `12h` · `7d`).
        #[arg(long, value_name = "DURATION", conflicts_with = "all")]
        older_than: Option<String>,
        /// Remove every trace in the store.
        #[arg(long)]
        all: bool,
        /// Remove even a paused trace (destroys its unanswered prompt).
        #[arg(long)]
        force: bool,
        /// Force the ASCII glyph theme.
        #[arg(long)]
        ascii: bool,
        /// Disable colour output.
        #[arg(long)]
        no_color: bool,
    },
    /// Browse per-task outputs: verb · duration · tokens · bounded
    /// preview (full value: `trace peek`).
    Outputs {
        /// Trace NDJSON path (default: the workspace's latest trace).
        trace: Option<PathBuf>,
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
        /// Trace NDJSON path (default: the workspace's latest trace).
        trace: Option<PathBuf>,
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
        /// Trace NDJSON path (default: the workspace's latest trace —
        /// `nika trace flow <workflow>` alone reads the last run).
        trace: Option<PathBuf>,
        /// The workflow file the run executed (`*.nika.yaml`) — the
        /// trace records values, the definition records the bindings.
        workflow: Option<String>,
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
    /// Workflow file (`*.nika.yaml`) · or a `registry:owner/name[@version]`
    /// verified pull (cached + offline; `permits:` never govern the fetch).
    /// OMITTED with exactly one workflow in this workspace → that one
    /// runs (announced); zero or several → the honest routing.
    file: Option<String>,
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
    /// With `--json`: ONE versioned plan object (`plan_version: 1` — waves ·
    /// cost ceiling · permits · requirements) instead of the human preview.
    /// `--output` stays refused (an outputs export of a run that never
    /// executed would be a lie).
    #[arg(long, conflicts_with = "output")]
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
    /// Operator run budget over METERED spend (USD). Refuses to start
    /// (exit 2) when the static cost floor already exceeds it; during the
    /// run the crossing call completes and counts, nothing new starts,
    /// unstarted tasks cancel and the run fails NIKA-1704 (exit 1) with
    /// spent-vs-budget — workflow `outputs:` are not resolved on a budget
    /// stop (per-task values live in the trace). Spending EXACTLY the
    /// budget does not trip it. Costs use LIST RATES from the vendored
    /// public catalog — private/proxy/negotiated pricing is not
    /// reflected; local · mock · unpriced work is never blocked (the
    /// budget bounds what the catalog can meter).
    ///
    /// KNOW THREE LIMITS (the budget is a floor-refusal + between-admission
    /// meter, not a per-token cap):
    ///   · CONCURRENCY — the guard stops NEW admissions, so a single WIDE
    ///     wave of parallel tasks dispatches together and may overshoot by
    ///     up to that wave's spend before the crossing is seen. Cap it with
    ///     `max_parallel:` to tighten the window.
    ///   · FLOOR is an OUTPUT-token estimate (`max_tokens` × output rate) —
    ///     input/prompt tokens are not priced statically, so a huge
    ///     `max_tokens` safety ceiling can over-refuse, and an input-heavy
    ///     workflow under-floors (the ledger catches it at run time).
    ///   · UNCATALOGED ≠ FREE — a model absent from the catalog meters as
    ///     $0 (same as local/mock), so a genuinely PAID uncataloged model
    ///     (custom endpoint · brand-new id) runs with no budget protection.
    #[arg(long = "max-cost-usd", value_name = "USD", value_parser = parse_budget_usd)]
    max_cost_usd: Option<f64>,
    /// Skip the opportunistic trace collection for this invocation
    /// (ADR-100: `.nika/traces/` is bounded by default — retention
    /// rides every run start; a collection that removes anything says
    /// so on stderr).
    #[arg(long)]
    no_gc: bool,
}

/// `--max-cost-usd` must be a real, non-negative dollar amount — `NaN`
/// or `inf` would make every comparison false and silently DISARM the
/// guard the operator believes is armed (the exact silent-no-protection
/// class the budget exists to kill).
fn parse_budget_usd(raw: &str) -> Result<f64, String> {
    let value: f64 = raw.parse().map_err(|e| format!("not a number: {e}"))?;
    if !value.is_finite() || value < 0.0 {
        return Err(format!(
            "must be a finite, non-negative USD amount (got {raw})"
        ));
    }
    Ok(value)
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

/// The `check` arm's routing: single file = the pre-variadic path,
/// byte-identical (every existing consumer — hooks · agents · CI — sees
/// exactly what it saw before); several files fan out through
/// [`verbs::check::run_many`]. The machine modes stay one-file-per-call —
/// `report_version: 1` and the inferred boundary are per-file contracts —
/// so `--json`/`--infer-permits` with several files refuse with a teach
/// line at exit 3 (the INVOCATION is wrong, no file was judged), and
/// stdin (`-`) cannot join a multi-file audit.
struct CheckFlags {
    json: bool,
    infer_permits: bool,
    native_strict: bool,
}

fn check_dispatch(
    files: &[String],
    flags: &CheckFlags,
    fix: bool,
    model: Option<&str>,
    theme: Theme,
) -> verbs::VerbOutput {
    let CheckFlags {
        json,
        infer_permits,
        native_strict,
    } = *flags;
    if fix {
        // The repair loop rewrites a file: stdin has nothing to rewrite,
        // --json's report_version is a single immutable audit, several
        // files would interleave rewrites with one summary, and
        // --infer-permits is a different output entirely.
        if json || infer_permits {
            return verbs::fix::refuse(
                "--fix pairs with the plain audit only (not --json / --infer-permits)",
            );
        }
        return match files {
            [file] if file != "-" => verbs::fix::run(file, native_strict, model, theme),
            [_] => verbs::fix::refuse("stdin (`-`) has no file to rewrite — name a real path"),
            _ => {
                verbs::fix::refuse("one file per repair loop — loop the files, one --fix per call")
            }
        };
    }
    if let [file] = files {
        if infer_permits {
            verbs::check::run_infer_permits(file, json)
        } else {
            verbs::check::run(file, json, native_strict, model, theme)
        }
    } else if json || infer_permits {
        verbs::VerbOutput {
            text: "check: --json and --infer-permits report ONE file per call \
                   (report_version 1 is a per-file contract)\n  fix: loop the \
                   files, one check per call\n"
                .to_owned(),
            code: verbs::exit::ENV,
        }
    } else if files.iter().any(|f| f == "-") {
        verbs::VerbOutput {
            text: "check: stdin (`-`) cannot join a multi-file audit\n  fix: \
                   pipe one call per stream, or name the files\n"
                .to_owned(),
            code: verbs::exit::ENV,
        }
    } else {
        verbs::check::run_many(files, native_strict, model, theme)
    }
}

impl Cli {
    /// `--plain` folds the whole sober story BEFORE any resolution —
    /// the downstream chains then see an explicit `never` at the top
    /// rung (colour · links); the ASCII/no-progress halves ride the
    /// same bool at each verb's own seam.
    fn presentation(&self) -> (ColorWhenArg, LinkChoice) {
        if self.plain {
            (ColorWhenArg::Never, LinkChoice::Never)
        } else {
            (self.color, self.hyperlink.choice())
        }
    }
}

/// Bare `nika` on a terminal is the CONCIERGE: the welcome card (what
/// this machine has · where you are · the next gesture) — the first
/// keystroke answers with a gesture, not a wall. Pipes/scripts keep the
/// full usage + exit 2 (a bare `nika` in a script is a usage error; the
/// sober register never changes shape).
fn concierge(plain_theme: Theme) -> std::process::ExitCode {
    if std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        return emit(&verbs::welcome::run(false, plain_theme)).into();
    }
    let mut cmd = <Cli as CommandFactory>::command();
    let _ = cmd.print_help();
    std::process::ExitCode::from(2)
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let (color, link_when) = cli.presentation();
    let plain_theme = term_theme(color.with_no_color(false), cli.plain, link_when);
    let Some(command) = cli.command else {
        return concierge(plain_theme);
    };
    let code = match command {
        Command::Check {
            files,
            json,
            infer_permits,
            fix,
            native_strict,
            model,
            no_color,
            ascii,
        } => check_lazy(
            files,
            &CheckFlags {
                json,
                infer_permits,
                native_strict,
            },
            fix,
            model.as_deref(),
            term_theme(color.with_no_color(no_color), ascii, link_when),
        ),
        Command::Run(args) => run_lazy(args, color, link_when, cli.plain),
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
        Command::Inspect { file, ascii } => {
            emit(&verbs::inspect::run(&file, with_ascii(plain_theme, ascii)))
        }
        Command::Graph { file, format } => {
            emit(&verbs::graph::run(&file, format.into(), plain_theme))
        }
        Command::Context { json, ascii } => {
            emit(&verbs::context::run(json, with_ascii(plain_theme, ascii)))
        }
        Command::Welcome { json, ascii } => {
            emit(&verbs::welcome::run(json, with_ascii(plain_theme, ascii)))
        }
        Command::Explain {
            code,
            json,
            forecast,
        } => emit(&explain_dispatch(&code, json, forecast, plain_theme)),
        Command::Doctor { ping, json } => emit(&verbs::doctor::run(ping, json, plain_theme)),
        Command::Init(args) => emit(&init_verb(&args, plain_theme)),
        Command::Wire { target, dir } => emit(&verbs::wire::run(target, &dir)),
        Command::Model { action } => model_verb(action),
        Command::Spec { canon } => emit(&verbs::pack_surface::spec(canon)),
        Command::Schema => emit(&verbs::pack_surface::schema()),
        Command::Catalog { json } => emit(&verbs::catalog::run(json, plain_theme)),
        Command::Tools { json } => emit(&verbs::tools::run(json, plain_theme)),
        Command::Examples { action } => examples_verb(action, plain_theme),
        Command::New { from, dest, force } => emit(&verbs::new::dispatch(
            from.as_deref(),
            dest.as_deref(),
            force,
            plain_theme,
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
        Command::Dap => nika_dap::run_stdio(),
        Command::Lsp => match nika_lsp::run_stdio() {
            Ok(()) => verbs::exit::OK,
            Err(err) => {
                eprintln!("nika lsp: {err}");
                1
            }
        },
        // The MCP server OWNS stdout (JSON-RPC) — like `lsp`, it must not go
        // through `emit`. Same server-process exit convention: 0 on a clean
        // EOF shutdown, 1 on a transport failure.
        Command::Mcp {
            transport,
            port,
            bind,
        } => mcp_verb(transport, port, &bind),
    };
    std::process::ExitCode::from(code)
}

/// Print a verb's text on the right stream and return its exit code.
/// Findings + successes go to stdout (they ARE the product); only
/// environment errors go to stderr.
fn emit(out: &VerbOutput) -> u8 {
    if out.code == verbs::exit::ENV {
        eprintln!("nika: {}", out.text);
    } else if !out.text.is_empty() {
        println!("{}", out.text.trim_end());
    }
    out.code
}

/// The `model` sub-verbs — a healthy `serve` never returns, so `emit` only prints refusals.
fn model_verb(action: ModelAction) -> u8 {
    let ModelAction::Serve {
        model,
        tokenizer,
        port,
        model_id,
    } = action;
    emit(&verbs::model::serve(
        &model,
        tokenizer.as_deref(),
        port,
        model_id.as_deref(),
    ))
}

/// Name the bare-form pick on stderr — the receipt names its subject.
fn announce_latest(path: &Path) {
    eprintln!(
        "nika trace: reading {} (the workspace latest)",
        path.display()
    );
}

/// The bare form of a static trace reader: no path → the workspace's
/// latest trace, named on stderr · zero traces → the teaching error,
/// exit 3 (ADR-098 environment).
fn resolve_trace(given: Option<PathBuf>) -> Result<PathBuf, u8> {
    if let Some(path) = given {
        return Ok(path);
    }
    if let Some(path) = verbs::trace::manage::latest() {
        announce_latest(&path);
        return Ok(path);
    }
    eprintln!(
        "nika trace: no traces in .nika/traces yet — run a workflow first, or pass a trace path"
    );
    Err(verbs::exit::ENV)
}

/// `nika trace flow` — two positionals, both optional to clap (a
/// required one may not follow an optional one): one arg IS the
/// workflow and the trace defaults, matching the bare-form contract.
fn flow_verb(trace: Option<PathBuf>, workflow: Option<String>, theme: Theme) -> u8 {
    let (trace, workflow) = match (trace, workflow) {
        (trace, Some(workflow)) => (trace, workflow),
        (Some(only), None) if only.extension().and_then(|e| e.to_str()) != Some("ndjson") => {
            (None, only.to_string_lossy().into_owned())
        }
        _ => {
            eprintln!(
                "nika trace: flow needs the workflow file — `nika trace flow [trace] <workflow.nika.yaml>` (the trace records values, the definition records the bindings)"
            );
            return verbs::exit::ENV;
        }
    };
    match resolve_trace(trace) {
        Ok(path) => emit(&verbs::trace::flow(
            &path.to_string_lossy(),
            &workflow,
            theme,
        )),
        Err(code) => code,
    }
}

/// `nika mcp` — serve the read-only MCP surface. stdio owns stdout
/// (JSON-RPC); http binds first so the banner names the RESOLVED
/// address, reads `NIKA_MCP_TOKEN` here (the crate is env-free by
/// discipline), then serves forever.
fn mcp_verb(transport: McpTransportArg, port: u16, bind: &str) -> u8 {
    let served = match transport {
        McpTransportArg::Stdio => nika_mcp::run_stdio(),
        McpTransportArg::Http => match nika_mcp::HttpServer::bind(bind, port) {
            Ok(server) => {
                // The sanctioned env boundary (same seam as config_from_env):
                // the token is operator config crossing into a server hold.
                #[allow(clippy::disallowed_methods)]
                let token = std::env::var("NIKA_MCP_TOKEN").ok();
                let addr = server
                    .addr()
                    .map_or_else(|_| format!("{bind}:{port}"), |a| a.to_string());
                eprintln!(
                    "nika mcp · http://{addr}/mcp · POST JSON-RPC (MCP 2025-11-25) · origin-gated · {} · production TLS belongs to a reverse proxy",
                    if token.is_some() {
                        "bearer auth ON (NIKA_MCP_TOKEN)"
                    } else {
                        "no auth (set NIKA_MCP_TOKEN to require a bearer)"
                    }
                );
                server.serve(token.as_deref())
            }
            Err(err) => Err(err),
        },
    };
    match served {
        Ok(()) => verbs::exit::OK,
        Err(err) => {
            eprintln!("nika mcp: {err}");
            1
        }
    }
}

fn trace_verb(action: TraceAction, color: ColorWhenArg, link_when: LinkChoice) -> u8 {
    match action {
        TraceAction::Replay(args) => trace_render(&args, true, color, link_when),
        TraceAction::Show(args) => trace_render(&args, false, color, link_when),
        TraceAction::Ls { ascii, no_color } => emit(&verbs::trace::manage::ls(term_theme(
            color.with_no_color(no_color),
            ascii,
            link_when,
        ))),
        TraceAction::Rm {
            trace,
            older_than,
            all,
            force,
            ascii,
            no_color,
        } => {
            let target = if all {
                verbs::trace::manage::RmTarget::All
            } else if let Some(raw) = older_than {
                match verbs::trace::manage::parse_older_than(&raw) {
                    Ok(cutoff) => verbs::trace::manage::RmTarget::OlderThan(cutoff),
                    Err(message) => {
                        eprintln!("nika trace: {message}");
                        return verbs::exit::ENV;
                    }
                }
            } else {
                // clap's required_unless_present_any guarantees the handle.
                let Some(handle) = trace else {
                    eprintln!("nika trace: rm needs a trace, --older-than, or --all");
                    return verbs::exit::ENV;
                };
                verbs::trace::manage::RmTarget::One(handle)
            };
            emit(&verbs::trace::manage::rm(
                &target,
                force,
                term_theme(color.with_no_color(no_color), ascii, link_when),
            ))
        }
        TraceAction::Outputs {
            trace,
            ascii,
            no_color,
        } => {
            let trace = match resolve_trace(trace) {
                Ok(path) => path,
                Err(code) => return code,
            };
            let mut theme = term_theme(color.with_no_color(no_color), ascii, link_when);
            // The dur column's bracket accents: TTY comfort only.
            theme.accents = std::io::stdout().is_terminal();
            emit(&verbs::trace::outputs(&trace.to_string_lossy(), theme))
        }
        TraceAction::Verify { trace } => match resolve_trace(trace) {
            Ok(path) => emit(&verbs::trace_verify::verify(&path.to_string_lossy())),
            Err(code) => code,
        },
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
        } => flow_verb(
            trace,
            workflow,
            term_theme(color.with_no_color(no_color), ascii, link_when),
        ),
    }
}

/// Unpack the `run` clap surface into the library verb call.
fn run_verb(args: &RunArgs, color: ColorWhenArg, link_when: LinkChoice, plain: bool) -> u8 {
    let resume = args.resume.as_ref().map(|trace| verbs::run::ResumeRequest {
        trace: trace.clone(),
        from: args.from.clone(),
        answers: args.answer.clone(),
    });
    let mode = resolve_run_mode(args.quiet, args.no_progress || plain);
    let mut theme = term_theme(
        color.with_no_color(args.no_color),
        args.ascii || plain,
        link_when,
    );
    // The duration accents ride the interactive surface ONLY — the
    // sober registers (piped · --no-progress · --quiet) keep their
    // exact bytes.
    theme.accents = mode == verbs::run::RenderMode::Live;
    // Duration heat additionally needs colour + the truecolor PROOF.
    theme.heat = theme.accents && theme.color && truecolor_env();
    // The live storyboard breathes (the braille beat between settles) —
    // interactive surface only, and the motion opt-out wins (the same
    // env the replay honours).
    theme.animate = theme.accents && !env_flag("NIKA_REDUCED_MOTION");
    verbs::run::run(
        args.file.as_deref().unwrap_or_default(),
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
        args.max_cost_usd,
        args.no_gc,
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
    let path = match &args.trace {
        Some(path) => path.clone(),
        // Bare form: the workspace's latest trace (same contract as
        // verify/outputs/flow).
        None => match verbs::trace::manage::latest() {
            Some(path) => {
                announce_latest(&path);
                path
            }
            None => {
                return Err("no trace given and no traces in .nika/traces yet — run a \
                            workflow first, pass a .ndjson path, or try --demo"
                    .to_owned());
            }
        },
    };
    let raw =
        std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    recover_events(&raw, &path.display().to_string())
}

/// Parse an NDJSON trace, tolerating a truncated/corrupt TAIL — a crashed run
/// (SIGSEGV · OOM · hard kill) leaves a half-written last line, and recovering
/// it is the whole point of a flight recorder. Delegates to the library's
/// tolerant reader (the SAME one `nika run --resume` folds through — one
/// recovery contract, two consumers) and surfaces the truncation note here.
fn recover_events(raw: &str, label: &str) -> Result<Vec<Event>, String> {
    let recovered = verbs::run::recover_events(raw, label).map_err(|e| e.to_string())?;
    if let Some(note) = &recovered.truncated_note {
        eprintln!("nika trace: {note} — rendering the recovered prefix");
    }
    Ok(recovered.events)
}

/// Fold a verb's `--ascii` flag onto the shared plain theme — the
/// mirror-family verbs (welcome · context) all speak it.
fn with_ascii(base: Theme, ascii: bool) -> Theme {
    Theme { ascii, ..base }
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

    /// The scaffolded AGENTS.md must teach the LIVE clap tree — a verb
    /// the binary ships and the guide never names is a verb a wired
    /// agent will never reach (inherited from the stalled 2026-07-05
    /// field-fixes branch: the scaffold then taught zero of the new
    /// train). Derived from the tree itself so it can never lag again.
    #[test]
    fn the_scaffolded_agents_md_teaches_the_live_clap_tree() {
        let agents = nika_cli::verbs::init::agents_md();
        for sub in Cli::command().get_subcommands() {
            let name = sub.get_name();
            if name == "help" {
                continue; // clap's auto subcommand — not a teaching target
            }
            assert!(
                agents.contains(name),
                "the scaffolded AGENTS.md must teach `nika {name}`"
            );
        }
        // The flags an agent needs daily — inputs, resume, goldens, scaffold.
        for flag in ["--var", "--resume", "--answer", "--update", "--from"] {
            assert!(
                agents.contains(flag),
                "the scaffolded AGENTS.md must teach `{flag}`"
            );
        }
    }

    #[test]
    fn max_cost_usd_help_names_its_three_real_limits() {
        // The rust-pro review's meta-point: the budget's real boundary
        // was disclosed in code, not at the operator's point of use. The
        // help must keep naming all three so a future trim can't silently
        // restore the false comfort.
        let mut cmd = Cli::command();
        let run = cmd.find_subcommand_mut("run").expect("run subcommand");
        let help = run.render_long_help().to_string();
        for limit in ["CONCURRENCY", "FLOOR", "UNCATALOGED"] {
            assert!(
                help.contains(limit),
                "--max-cost-usd help must name `{limit}`"
            );
        }
    }

    /// Bare `nika examples` answers with the LIST, not the usage
    /// screen — init's « next · » sends users here (user-sim finding).
    #[test]
    fn bare_examples_defaults_to_list() {
        let cli = Cli::try_parse_from(["nika", "examples"]).expect("parses");
        assert!(
            matches!(cli.command, Some(Command::Examples { action: None })),
            "bare form parses (dispatch folds to List)"
        );
        // The explicit form still parses.
        assert!(Cli::try_parse_from(["nika", "examples", "list"]).is_ok());
    }

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
