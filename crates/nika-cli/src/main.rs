// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `nika` binary — clap surface + dispatch over the verb tree
//! (`nika --help` is the living list; the static suite audits before any
//! run, `run` executes CHECKED workflows through the composed L3 runtime
//! over production seams). Exit codes per the locked contract (spec §4):
//! `0` ok · `1` workflow failed · `2` file findings · `3` environment.

// A terminal binary's whole job is printing (the nika-catalog-verify precedent).
#![allow(clippy::disallowed_macros, clippy::print_stdout, clippy::print_stderr)]

use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::time::Duration;

mod examples_args;
mod init_args;
mod lazy;
mod model_args;
mod registry_args;

use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use nika_cli::display::format::{ColorChoice, ColorEnv, LinkChoice, color_enabled, links_enabled};
use nika_cli::verbs::explain_file::dispatch as explain_dispatch;
use nika_cli::verbs::{self, VerbOutput};
use nika_cli::{RunView, Theme, frame};

use examples_args::{ExamplesAction, examples_verb};
use init_args::{InitArgs, init_verb};
use lazy::{check_lazy, resolve_lazy_target, run_lazy};
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
    after_help = "the map (the craft · 12 verbs):\n  begin     examples · new · init     # see it work · one file · found a repo\n  prove     check · test              # audit before tokens · goldens\n  run       run · trace               # the living DAG · the flight recorder\n  machine   welcome · doctor · model · wire\n  learn     explain\n\nthe full surface (protocols · trust cycle · plumbing): nika --help --all\n\nstart here:\n  nika                                           # the concierge (a terminal greets you)\n  nika examples run 01-hello --model mock/echo   # offline proof · zero keys\n  nika init                                      # found this repo — the wizard"
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
    /// Force the ASCII glyph twins everywhere (CI logs · legacy
    /// terminals) — colour stays; `--plain` is the full sober umbrella.
    #[arg(long, global = true, display_order = 903)]
    ascii: bool,
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
    #[command(display_order = 40)]
    Welcome {
        /// Emit the versioned machine projection (`welcome_version: 1`).
        #[arg(long)]
        json: bool,
        /// The whole workspace truth (every workflow audited · recent
        /// runs · machine facts) — the deep half of the mirror (the old
        /// `context` verb, one roof).
        #[arg(long)]
        deep: bool,
    },
    /// Audit a workflow BEFORE it runs: plan · cost ceiling · secret
    /// flows · types · tools — every finding teaches its fix.
    #[command(display_order = 20)]
    Check {
        /// Workflow file(s) (`*.nika.yaml`) · `-` reads stdin · or a verified
        /// `registry:owner/name[@version]` pull (cached + offline; workflow
        /// `permits:` never govern the fetch) · several files audit in sequence
        /// (worst exit wins — the CI shape) · `--json`/`--infer-permits` one-file-per-call.
        /// Omitted with exactly one workflow here → that one is audited.
        #[arg(num_args = 0..)]
        files: Vec<String>,
        /// Emit the versioned machine projection (`report_version: 1`).
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
        /// The readiness posture on the audit's risk grade (uncapped
        /// spend · glob/wildcard grants): `advisory` displays the grade
        /// on the verdict card, `operational` also fails (exit 2) when
        /// the grade is high or unbounded — the agent/CI readiness gate.
        #[arg(long, value_enum, default_value_t = verbs::check::Profile::Advisory)]
        profile: verbs::check::Profile,
        /// Price the static envelope AS IF this `<provider>/<model>`
        /// replaced the envelope default — the preview of `nika run
        /// --model` (per-task `model:` still wins, like the runtime).
        #[arg(long)]
        model: Option<String>,
    },
    /// Run a workflow (the same audit runs first · live render).
    #[command(display_order = 30)]
    Run(RunArgs),
    /// Golden test: run under the MOCK provider (offline · deterministic)
    /// and compare the typed `outputs:` against `<file>.golden.json`.
    #[command(display_order = 21)]
    Test {
        /// Workflow file (`*.nika.yaml`).
        file: Option<String>,
        /// (Re)write the golden from this run instead of comparing.
        #[arg(long)]
        update: bool,
    },
    /// Static anatomy: tasks · verbs · wave groups · cost · permits —
    /// and the ONE graph projector (`--format json|mermaid|dot` for the
    /// machine surfaces · human stays the default).
    #[command(hide = true, display_order = 43)]
    Inspect {
        /// Workflow file (`*.nika.yaml`) · `-` reads stdin.
        file: String,
        /// Project the graph instead of the human anatomy (json
        /// canonical · mermaid/dot derived — the docs/site surfaces).
        #[arg(long, value_enum)]
        format: Option<verbs::graph::GraphFormatArg>,
    },
    /// Teach one error code (cause · category · fix-form) — or narrate a
    /// workflow FILE: what it does · the waves · cost before a token is
    /// spent · what it touches · how to run it.
    #[command(display_order = 41)]
    Explain {
        /// An error code (`NIKA-440` · bare `440` · `DAG-003`) or a
        /// workflow file path (`*.nika.yaml` · `-` reads stdin).
        code: String,
        /// File form only: emit the versioned machine projection
        /// (`explain_version: 1` · the check report's own vocabulary).
        #[arg(long)]
        json: bool,
        /// File form only: include the learned-truth forecast — duration/
        /// cost/risk priors from YOUR local traces (stats over
        /// `.nika/traces/` · never a model call · never the network).
        #[arg(long)]
        forecast: bool,
    },
    /// The run-signing key lifecycle (mint · TOFU fingerprint · rotate — old pubs stay verifiable).
    #[command(hide = true, display_order = 70)]
    Key {
        #[command(subcommand)]
        action: verbs::key::KeyAction,
    },
    /// Sign a workflow file (S3 · author-binding): mint `<file>.minisig` · `--check` verifies.
    #[command(hide = true, display_order = 71)]
    Sign(verbs::sign::SignArgs),
    /// Diagnose this machine (binary · config · provider keys · local models).
    /// Diagnose-only — prints the exact fix command, never mutates anything.
    #[command(display_order = 42)]
    Doctor {
        /// TCP-probe the local provider ports (loopback/configured only ·
        /// 300ms cap · nothing is sent on the socket). Offline without it.
        #[arg(long)]
        ping: bool,
        /// Emit the machine projection (summary + findings[] — agents/CI
        /// branch on `summary.fail` instead of parsing glyphs).
        #[arg(long)]
        json: bool,
    },
    /// Found a repo (`.vscode` schema wiring · `AGENTS.md` · Cursor rule + MCP ·
    /// `.agents/skills` authoring skill · optional workflow set). Bare on
    /// a terminal the founding wizard runs; flags are the scriptable
    /// twin. Existing files are skipped — `--force` overwrites.
    #[command(display_order = 10)]
    Init(InitArgs),
    /// Wire Nika into editor/agent MCP clients (explicit, idempotent).
    /// The door: `detected --dry-run` previews what this machine shows ·
    /// `detected` wires it · `<client>` wires one · `all` is the advanced
    /// sweep (previewed, then confirmed or `--yes`).
    #[command(display_order = 50)]
    Wire {
        /// Client to wire (`detected` = only the clients this machine shows).
        #[arg(value_enum)]
        target: verbs::wire::WireTarget,
        /// Workspace directory for repo-local clients such as VS Code.
        #[arg(long, default_value = ".")]
        dir: String,
        /// Print the per-client plan (created/updated/current/manual) —
        /// writes nothing.
        #[arg(long)]
        dry_run: bool,
        /// Consent to `all` without a prompt (scripts · CI — a terminal asks).
        #[arg(long)]
        yes: bool,
    },
    /// Local models — pull from the Hugging Face Hub, serve on this
    /// machine, list/rm the disk (ONE models dir · no external daemon).
    #[command(display_order = 51)]
    Model {
        #[command(subcommand)]
        action: model_args::ModelAction,
    },
    /// The embedded spec identity (`--canon` prints the SSOT).
    #[command(hide = true, display_order = 44)]
    Spec {
        /// Print the canon.yaml single source of truth.
        #[arg(long)]
        canon: bool,
        /// Print the embedded JSON Schema for `*.nika.yaml` (the old
        /// `schema` verb, one roof).
        #[arg(long, conflicts_with = "canon")]
        schema: bool,
    },
    /// The embedded provider/model catalog (models · capabilities · env vars).
    #[command(hide = true, display_order = 52)]
    Catalog {
        /// Emit the versioned machine projection (`catalog_version: 1`).
        #[arg(long)]
        json: bool,
        /// The `nika:*` builtin tool catalog instead (what `invoke`
        /// reaches without MCP — the old `tools` verb, one roof).
        #[arg(long)]
        tools: bool,
    },
    /// Browse the embedded examples.
    #[command(display_order = 12)]
    Examples {
        /// Bare `nika examples` lists — the clap usage screen answered
        /// where a user following init's « next · » expected the slugs
        /// (the user-sim finding).
        #[command(subcommand)]
        action: Option<ExamplesAction>,
    },
    /// Instantiate an embedded template skeleton.
    #[command(display_order = 11)]
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
    #[command(hide = true, display_order = 63)]
    Completions {
        /// Target shell.
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// Read the flight recorder (replay or summarize a run).
    #[command(display_order = 31)]
    Trace {
        #[command(subcommand)]
        action: verbs::trace::TraceAction,
    },
    /// Export the evidence pack for one run (journal + manifest + receipt + VERIFY.md).
    #[command(hide = true, display_order = 32)]
    Evidence {
        #[command(flatten)]
        args: verbs::evidence::EvidenceArgs,
    },
    /// Read a run receipt — `explain` renders its readable projection
    /// (stable text · a READING, never a proof).
    #[command(hide = true, display_order = 33)]
    Receipt {
        #[command(subcommand)]
        action: verbs::receipt::ReceiptAction,
    },
    /// The hook's judge (hidden — the wired `guard-run.sh` shim calls it,
    /// agents never type it): read a host hook payload (`--stdin`) or one
    /// command line (`--command`), find every effective `nika run`, audit
    /// the EXACT file in-process, and answer the hook protocol. P0-7 +
    /// P0-15: a red file or a priced model without `--max-cost-usd` is
    /// denied; an unjudgeable run is a VISIBLE `guard_unavailable`, never
    /// a silent allow. The run belongs to the human — guard JUDGES, it
    /// never executes.
    #[command(hide = true)]
    Guard(GuardArgs),
    /// Debug Adapter Protocol server (stdio) — time-travel a recorded
    /// run under a debugger UI: breakpoints on task lines · step forward
    /// AND back through settles · outputs in the variables pane. Replay
    /// re-renders, never re-executes.
    #[command(hide = true, display_order = 62)]
    Dap,
    /// Run the language server over stdio (drives the editor extension).
    #[command(hide = true, display_order = 61)]
    Lsp {
        /// LSP-host convention flag: vscode-languageclient, nvim and
        /// helix spawn `<server> --stdio` by habit. Stdio is this
        /// server's ONLY transport, so the flag is a no-op — but
        /// refusing it killed every spawn from a client that passes it,
        /// with exit 2 before the first byte of JSON-RPC (the v0.106.0
        /// extension post-mortem: the language server had never once
        /// run in production because of this refusal). Hidden: it
        /// teaches nothing a human needs to type.
        #[arg(long, hide = true)]
        stdio: bool,
        /// Same convention family: hosts pass their own PID so a server
        /// can watchdog its parent. Accepted, currently unread.
        #[arg(long = "clientProcessId", hide = true)]
        client_process_id: Option<u32>,
    },
    /// Run the MCP server (validate: check/explain · learn:
    /// schema/examples/templates/canon — the in-binary Model Context Protocol
    /// surface for Cursor · Claude Desktop · agents). Default transport:
    /// stdio; `--transport http` serves Streamable HTTP for managed hosts.
    /// `approve` runs the CLIENT side: the MCP tool-pinning re-approval over
    /// the servers configured in `.nika/mcp_servers.json`.
    #[command(hide = true, display_order = 60)]
    Mcp {
        #[command(subcommand)]
        action: Option<verbs::mcp_pins::McpAction>,
        /// The wire: `stdio` (the editor/agent default) or `http`
        /// (Streamable HTTP · POST JSON-RPC · spec 2025-11-25).
        #[arg(long, value_enum, default_value_t = verbs::mcp_pins::McpTransportArg::Stdio)]
        transport: verbs::mcp_pins::McpTransportArg,
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

/// The hidden `guard` arm's flags (the `RunArgs` tuple-variant precedent).
#[derive(Args)]
struct GuardArgs {
    /// Read the host hook JSON payload from stdin (the shim's wire:
    /// Cursor `{command, cwd}` · Claude Code `PreToolUse`
    /// `{tool_input:{command}, cwd}` — sniffed by `hook_event_name`).
    #[arg(long)]
    stdin: bool,
    /// Judge ONE shell command line instead of a hook payload.
    #[arg(long, value_name = "LINE", conflicts_with = "stdin")]
    command: Option<String>,
    /// The directory the command runs in (with `--command`; the
    /// payload's `cwd` wins on the stdin wire, the process cwd
    /// otherwise).
    #[arg(long, value_name = "DIR", requires = "command")]
    cwd: Option<String>,
    /// The human reading (allow · deny · `guard_unavailable` + why)
    /// instead of the hook JSON protocol.
    #[arg(long)]
    human: bool,
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
    /// Set a workflow `inputs:` value (repeatable). Overrides a declared
    /// `default:` and satisfies a `required: true` input. JSON when it
    /// parses (numbers · booleans · arrays), else a string. Unknown keys refused.
    #[arg(long = "var", value_name = "KEY=VALUE")]
    var: Vec<String>,
    /// Resume from a prior run's NDJSON trace (`nika run … --json >
    /// trace.ndjson`): every task whose identity matches a journaled
    /// success is skipped with a visible `task_cache_hit` — an edited
    /// task or a changed input always re-runs (ADR-099). A trace without
    /// resume keys runs everything live (a notice, never an error). The
    /// trace's recorded engine version is JUDGED (F-P21): a resume under
    /// a different engine refuses, naming both versions.
    #[arg(long, value_name = "TRACE", conflicts_with = "dry_run")]
    resume: Option<PathBuf>,
    /// Declare a cross-version resume compatible (F-P21 · NEP-0014 law
    /// 4): attests the trace recorded under engine `<VERSION>` may resume
    /// under this one — the token must name the trace's recorded version
    /// exactly (`unrecorded` for a pre-versioning journal). The declared
    /// compat is journaled on the run's boot manifest.
    #[arg(long, value_name = "VERSION", requires = "resume")]
    resume_compat: Option<String>,
    /// Force this task AND its transitive downstream to re-run even on an
    /// identity match (the lever for changes the hashes cannot see —
    /// rotated secret · external state · an infer output to re-roll).
    #[arg(long, value_name = "TASK_ID", requires = "resume")]
    from: Option<String>,
    /// Answer a `nika:prompt` gate (repeatable · ADR-099 rider): binds as
    /// the named task's answer — `--answer ok=true` for confirm, a string
    /// for input, one of the choices for choice. The value parses as JSON
    /// when it parses, else rides as a string. Without `--resume` the
    /// answer is PRE-SEEDED on the fresh run: it waits in the gate map
    /// and is consumed when the task asks (the CI one-pass gate).
    #[arg(long = "answer", value_name = "TASK=VALUE")]
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
    /// Refuse to run an unsigned or invalidly-signed workflow (exit 2 ·
    /// checked BEFORE any task executes). OPT-IN — default is unsigned-tolerant.
    #[arg(long)]
    require_signature: bool,
}

/// `--max-cost-usd` must be a real, non-negative dollar amount — `NaN`
/// or `inf` would make every comparison false and silently DISARM the
/// guard the operator believes is armed (the exact silent-no-protection
/// class the budget exists to kill).
pub(crate) fn parse_budget_usd(raw: &str) -> Result<f64, String> {
    let value: f64 = raw.parse().map_err(|e| format!("not a number: {e}"))?;
    if !value.is_finite() || value < 0.0 {
        return Err(format!(
            "must be a finite, non-negative USD amount (got {raw})"
        ));
    }
    Ok(value)
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
/// The mirror's two depths — the greeting, or the whole workspace
/// truth (the old `context` verb, one roof).
fn mirror_verb(json: bool, deep: bool, theme: Theme) -> u8 {
    if deep {
        emit(&verbs::context::run(json, theme))
    } else {
        emit(&verbs::welcome::run(json, theme))
    }
}

/// The pack identity's two dumps — the spec card, `--canon`, or the
/// JSON Schema (the old `schema` verb, one roof).
fn spec_verb(canon: bool, schema: bool) -> u8 {
    if schema {
        emit(&verbs::pack_surface::schema())
    } else {
        emit(&verbs::pack_surface::spec(canon))
    }
}

/// The `wire` door (H7): clap's flags plus the terminal fact `all`'s
/// consent gate reads (a terminal asks · a pipe needs `--yes`).
fn wire_verb(target: verbs::wire::WireTarget, dir: &str, dry_run: bool, yes: bool) -> u8 {
    let interactive = std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
    emit(&verbs::wire::run_with(
        target,
        dir,
        verbs::wire::WireOptions {
            dry_run,
            yes,
            interactive,
        },
    ))
}

fn concierge(plain_theme: Theme) -> std::process::ExitCode {
    // TTY or pipe, the front door answers with the mirror (gauntlet
    // 2026-07-31: the taught « a terminal greets you » card exited 2 in
    // a pipe — an agent's first contact read as breakage, and spec §4
    // reserves 2 for FILE findings). Welcome is offline and always 0;
    // `--help` stays the reference card.
    emit(&verbs::welcome::run(false, plain_theme)).into()
}

fn main() -> std::process::ExitCode {
    // RAMS-13 · the full surface on demand: `--help --all` prints the
    // SAME tree with nothing hidden (12 craft verbs lead the default
    // help; protocols · trust cycle · plumbing stay one flag away —
    // ranged, never removed). Judged before clap parses so `--all`
    // never becomes a real flag on any verb.
    let argv: Vec<std::ffi::OsString> = std::env::args_os().collect();
    if argv.iter().any(|a| a == "--all")
        && argv
            .iter()
            .any(|a| a == "--help" || a == "-h" || a == "help")
    {
        let mut cmd = <Cli as clap::CommandFactory>::command();
        cmd = cmd.mut_subcommands(|sc| sc.hide(false));
        // Rendering help to stdout is this binary's whole job here; a
        // closed pipe is the caller's choice, never a crash.
        let _ = cmd.print_long_help();
        return std::process::ExitCode::SUCCESS;
    }
    let cli = Cli::parse();
    let (color, link_when) = cli.presentation();
    let plain_theme = term_theme(
        color.with_no_color(false),
        cli.ascii || cli.plain,
        link_when,
    );
    let Some(command) = cli.command else {
        return concierge(plain_theme);
    };
    let code = dispatch_verb(command, plain_theme, color, link_when, cli.plain, cli.ascii);
    std::process::ExitCode::from(code)
}

/// The check arm's plumbing — folded out of the dispatch so the seam
/// stays one line per verb.
#[allow(
    clippy::fn_params_excessive_bools,
    clippy::too_many_arguments,
    clippy::needless_pass_by_value
)]
fn check_arm(
    files: Vec<String>,
    json: bool,
    infer_permits: bool,
    native_strict: bool,
    profile: verbs::check::Profile,
    fix: bool,
    model: Option<String>,
    plain_theme: Theme,
) -> u8 {
    let flags = verbs::check::CheckFlags {
        json,
        infer_permits,
        native_strict,
        profile,
    };
    check_lazy(
        files,
        &flags,
        fix,
        model.as_deref(),
        interactive_theme(plain_theme),
    )
}

/// One arm per subcommand — the dispatch seam `main` hands to.
fn dispatch_verb(
    command: Command,
    plain_theme: Theme,
    color: ColorWhenArg,
    link_when: LinkChoice,
    plain: bool,
    ascii: bool,
) -> u8 {
    match command {
        Command::Check {
            files,
            json,
            infer_permits,
            fix,
            native_strict,
            profile,
            model,
        } => check_arm(
            files,
            json,
            infer_permits,
            native_strict,
            profile,
            fix,
            model,
            plain_theme,
        ),
        Command::Run(args) => run_lazy(args, color, link_when, plain, ascii),
        Command::Test { file, update } => match resolve_lazy_target(file, "test") {
            Ok(file) => verbs::test::run(&file, update, plain_theme),
            Err(code) => code,
        },
        Command::Inspect { file, format } => match format {
            Some(f) => emit(&verbs::graph::run(&file, f.into(), plain_theme)),
            None => emit(&verbs::inspect::run(&file, plain_theme)),
        },
        Command::Welcome { json, deep } => mirror_verb(json, deep, plain_theme),
        Command::Explain {
            code,
            json,
            forecast,
        } => emit(&explain_dispatch(&code, json, forecast, plain_theme)),
        Command::Key { action } => emit(&verbs::key::run(action)),
        Command::Sign(args) => emit(&verbs::sign::run(&args)),
        Command::Doctor { ping, json } => emit(&verbs::doctor::run(ping, json, plain_theme)),
        Command::Init(args) => emit(&init_verb(&args, plain_theme)),
        Command::Wire {
            target,
            dir,
            dry_run,
            yes,
        } => wire_verb(target, &dir, dry_run, yes),
        Command::Model { action } => model_args::model_verb(action),
        Command::Spec { canon, schema } => spec_verb(canon, schema),
        Command::Catalog { json, tools } => {
            if tools {
                emit(&verbs::tools::run(json, plain_theme))
            } else {
                emit(&verbs::catalog::run(json, plain_theme))
            }
        }
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
        Command::Trace { action } => trace_verb(action, plain_theme, color, link_when),
        Command::Evidence { args } => evidence_run(args),
        Command::Receipt { action } => emit(&verbs::receipt::run(action)),
        Command::Guard(args) => guard_verb(&args, plain_theme),
        // The language server OWNS stdout (JSON-RPC) — it must not go through
        // `emit`. It follows the LSP exit-code convention: 0 on a clean
        // shutdown/exit, non-zero (1) otherwise (transport failure, or an
        // `exit` without a prior `shutdown`) — the server-process
        // convention, NOT the verb FILE/WORKFLOW/ENV taxonomy.
        Command::Dap => nika_dap::run_stdio(),
        Command::Lsp { .. } => match nika_lsp::run_stdio() {
            Ok(()) => verbs::exit::OK,
            Err(err) => {
                eprintln!("nika lsp: {err}");
                1
            }
        },
        // The MCP server OWNS stdout (JSON-RPC · like `lsp` · never `emit`) —
        // exit 0 on clean EOF, 1 on transport failure; verify/approve are
        // ordinary verbs and DO go through emit.
        Command::Mcp {
            action,
            transport,
            port,
            bind,
        } => verbs::mcp_pins::mcp_verb(action, transport, port, &bind),
    }
}

/// The `guard` arm's routing — the hook wire OWNS stdout like lsp/mcp:
/// the verdict JSON is the protocol on EVERY exit class (0 allow · 2
/// deny · 3 `guard_unavailable`), so this bypasses `emit` (which routes
/// exit 3 to stderr — the host would see nothing, the exact
/// silent-degradation class the verb exists to kill).
fn guard_verb(args: &GuardArgs, theme: Theme) -> u8 {
    let out = verbs::guard::run(
        args.stdin,
        args.command.as_deref(),
        args.cwd.as_deref(),
        args.human,
        theme,
    );
    if !out.text.is_empty() {
        println!("{}", out.text.trim_end());
    }
    out.code
}

/// The `evidence` arm's routing: resolve the trace (store handle or
/// bare-latest), then export the pack through the verbs seam.
fn evidence_run(args: verbs::evidence::EvidenceArgs) -> u8 {
    match verbs::trace::manage::resolve_trace(args.trace) {
        Ok(path) => emit(&verbs::evidence::export(
            &path.to_string_lossy(),
            args.out.as_deref(),
            args.workflow.as_deref(),
            args.json,
        )),
        Err(code) => code,
    }
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

/// `nika mcp` — serve the read-only MCP surface. stdio owns stdout
/// (JSON-RPC); http binds first so the banner names the RESOLVED
/// address, reads `NIKA_MCP_TOKEN` here (the crate is env-free by
/// discipline), then serves forever. The client subcommand (the
/// tool-pinning re-approval) dispatches to the verbs layer instead.
/// `nika trace verify [TRACES…]` — several paths (the shell glob) go
/// per-file/worst-of; zero or one keeps the existing voice byte-stable
/// (bare form resolves the latest · one arg resolves store handles).
fn verify_verb(mut traces: Vec<PathBuf>, opts: &verbs::trace_verify::VerifyOptions) -> u8 {
    if traces.len() > 1 {
        return emit(&verbs::trace_verify::verify_many_with(&traces, opts));
    }
    match verbs::trace::manage::resolve_trace(traces.pop()) {
        Ok(path) => emit(&verbs::trace_verify::verify_with(
            &path.to_string_lossy(),
            opts,
        )),
        Err(code) => code,
    }
}

fn trace_verb(
    action: verbs::trace::TraceAction,
    theme: Theme,
    color: ColorWhenArg,
    link_when: LinkChoice,
) -> u8 {
    match action {
        verbs::trace::TraceAction::Replay(args) => {
            trace_render(&args, true, color, link_when, theme.ascii)
        }
        verbs::trace::TraceAction::Show(args) => {
            trace_render(&args, false, color, link_when, theme.ascii)
        }
        verbs::trace::TraceAction::Ls {} => emit(&verbs::trace::manage::ls(theme)),
        verbs::trace::TraceAction::Rm {
            trace,
            older_than,
            all,
            force,
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
            emit(&verbs::trace::manage::rm(&target, force, theme))
        }
        verbs::trace::TraceAction::Outputs { trace } => {
            let trace = match verbs::trace::manage::resolve_trace(trace) {
                Ok(path) => path,
                Err(code) => return code,
            };
            let mut theme = theme;
            // The dur column's bracket accents: TTY comfort only.
            theme.accents = std::io::stdout().is_terminal();
            emit(&verbs::trace::outputs(&trace.to_string_lossy(), theme))
        }
        verbs::trace::TraceAction::Verify {
            traces,
            key,
            anchored,
            replay,
        } => verify_verb(
            traces,
            &verbs::trace_verify::VerifyOptions {
                key,
                anchored,
                replay,
            },
        ),
        verbs::trace::TraceAction::Anchor {
            trace,
            rekor_url,
            tsa_url,
        } => anchor_verb(trace, &rekor_url, &tsa_url),
        verbs::trace::TraceAction::Reproduce { recorded, fresh } => {
            emit(&verbs::trace_reproduce::reproduce(
                &recorded.to_string_lossy(),
                &fresh.to_string_lossy(),
            ))
        }
        verbs::trace::TraceAction::Export {
            trace,
            out,
            include_content,
        } => emit(&verbs::trace_otel::export(
            &verbs::trace::manage::resolve_store_handle(&trace).to_string_lossy(),
            out.as_deref()
                .map(|p| p.to_string_lossy().into_owned())
                .as_deref(),
            include_content,
        )),
        verbs::trace::TraceAction::Peek { trace, task, raw } => emit(&verbs::trace::peek(
            &trace.to_string_lossy(),
            &task,
            raw,
            theme,
        )),
        verbs::trace::TraceAction::Flow { trace, workflow } => {
            match verbs::trace::manage::flow_verb(trace, workflow, theme) {
                Ok(out) => emit(&out),
                Err(code) => code,
            }
        }
    }
}

/// The `anchor` arm's routing: resolve the trace (store handle or
/// bare-latest), then notarize through the verbs seam.
fn anchor_verb(trace: Option<PathBuf>, rekor_url: &str, tsa_url: &str) -> u8 {
    match verbs::trace::manage::resolve_trace(trace) {
        Ok(path) => emit(&verbs::trace_anchor::run(
            &path.to_string_lossy(),
            rekor_url,
            tsa_url,
        )),
        Err(code) => code,
    }
}

/// Unpack the `run` clap surface into the library verb call.
fn run_verb(
    args: &RunArgs,
    color: ColorWhenArg,
    link_when: LinkChoice,
    plain: bool,
    ascii: bool,
) -> u8 {
    let resume = (args.resume.is_some() || !args.answer.is_empty()).then(|| {
        nika_dap::resume::ResumeRequest {
            trace: args.resume.clone(),
            from: args.from.clone(),
            answers: args.answer.clone(),
            compat: args.resume_compat.clone(),
        }
    });
    let mode = resolve_run_mode(args.quiet, args.no_progress || plain);
    let mut theme = term_theme(color.with_no_color(false), ascii || plain, link_when);
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
        args.require_signature,
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
/// Lift the accents band onto a theme when the surface is genuinely
/// interactive (colour resolved on · a real TTY · glyphs available) —
/// the one-shot read verbs' twin of the run's Live gate.
fn interactive_theme(mut theme: Theme) -> Theme {
    theme.accents = theme.color && !theme.ascii && std::io::stdout().is_terminal();
    theme
}

fn term_theme(choice: ColorChoice, ascii: bool, link_when: LinkChoice) -> Theme {
    let tty = std::io::stdout().is_terminal();
    let env = color_env();
    let mut theme = Theme::new(color_enabled(choice, env, tty), ascii, false);
    theme.links = links_enabled(link_when, tty, env.term_dumb);
    theme
}

/// Load events, fold, render — live replay or final card.
fn trace_render(
    args: &verbs::trace::TraceArgs,
    replay: bool,
    color: ColorWhenArg,
    link_when: LinkChoice,
    ascii: bool,
) -> u8 {
    let events = match verbs::trace::manage::load_events(args) {
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
    let mut theme = term_theme(color.with_no_color(false), ascii, link_when);
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
    // any past trace — the same final frame a live TTY run ends on. The
    // fruit rides in its PURE form (paths + the model's last word, no
    // sizes: stat would read today's disk against a past run's claim).
    print_lines(&nika_cli::display::flow::waterfall(&view, &theme));
    let mut notes: Vec<String> = nika_cli::display::fruit::written_files(&view)
        .iter()
        .map(|f| format!("{} {}", f.verb, f.path))
        .collect();
    if let Some((_task, text)) = nika_cli::display::fruit::last_said(&view)
        && let Some(quote) = nika_cli::display::shape::summarize(text, 46)
    {
        notes.push(format!("said {quote}"));
    }
    print_lines(&nika_cli::display::flow::verdict_card(
        &view, &theme, &notes,
    ));
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
    /// The budget guard is ONE guard on both doors: `run` and
    /// `examples run` share `parse_budget_usd`, so a NaN/inf (which
    /// silently disarms every comparison) refuses at parse time on
    /// BOTH — the drift where one door validated and the other let
    /// the disarmed value through is pinned shut.
    #[test]
    fn the_budget_guard_holds_on_both_doors() {
        for argv in [
            vec!["nika", "run", "wf.nika.yaml", "--max-cost-usd", "nan"],
            vec![
                "nika",
                "examples",
                "run",
                "01-hello",
                "--max-cost-usd",
                "nan",
            ],
            vec!["nika", "run", "wf.nika.yaml", "--max-cost-usd", "inf"],
            vec![
                "nika",
                "examples",
                "run",
                "01-hello",
                "--max-cost-usd",
                "-1",
            ],
        ] {
            assert!(
                Cli::try_parse_from(&argv).is_err(),
                "{argv:?} must refuse at parse time"
            );
        }
        for argv in [
            vec!["nika", "run", "wf.nika.yaml", "--max-cost-usd", "0.05"],
            vec![
                "nika",
                "examples",
                "run",
                "01-hello",
                "--max-cost-usd",
                "0.05",
            ],
        ] {
            assert!(Cli::try_parse_from(&argv).is_ok(), "{argv:?} must parse");
        }
    }

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

    /// The LSP-host convention flags parse as no-ops. Refusing them was
    /// the v0.106.0 extension post-mortem: vscode-languageclient spawns
    /// `nika lsp --stdio`, the unknown flag exited 2 before the first
    /// byte of JSON-RPC, and the language server had never once run in
    /// production. nvim and helix pass the same flags by habit.
    #[test]
    fn lsp_accepts_the_host_convention_flags() {
        for argv in [
            vec!["nika", "lsp"],
            vec!["nika", "lsp", "--stdio"],
            vec!["nika", "lsp", "--clientProcessId=42"],
            vec!["nika", "lsp", "--stdio", "--clientProcessId=42"],
        ] {
            let cli = Cli::try_parse_from(&argv).expect("host convention parses");
            assert!(
                matches!(cli.command, Some(Command::Lsp { .. })),
                "{argv:?} lands on Lsp"
            );
        }
        // The transports this server does NOT speak stay refused — a
        // client asking for ipc/pipe/socket must learn immediately, not
        // hang on a stdio server that will never dial back.
        for argv in [
            vec!["nika", "lsp", "--node-ipc"],
            vec!["nika", "lsp", "--pipe=/tmp/x"],
            vec!["nika", "lsp", "--socket=9257"],
        ] {
            assert!(
                Cli::try_parse_from(&argv).is_err(),
                "{argv:?} must stay refused"
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
    /// THE LAW (RAMS-13 · census over 19 personas: 12 of 23 verbs
    /// reached by <=1 user, yet all 23 hit 11 first-timers in the
    /// face): the default help shows AT MOST the 12 craft verbs; the
    /// full tree stays one flag away (`--help --all`) and NOTHING is
    /// removed — visible + hidden is the whole enum, invariant. Ranged,
    /// never deleted: `key`/`sign`/`mcp`/`lsp` serve — just not on day
    /// one.
    #[test]
    fn the_default_help_shows_the_craft_and_hides_nothing_forever() {
        let cmd = <Cli as clap::CommandFactory>::command();
        let total = cmd
            .get_subcommands()
            .filter(|c| c.get_name() != "help")
            .count();
        let visible: Vec<&str> = cmd
            .get_subcommands()
            .filter(|c| !c.is_hide_set() && c.get_name() != "help")
            .map(clap::Command::get_name)
            .collect();
        assert!(
            visible.len() <= 12,
            "the first screen is the craft, not a manifesto: {visible:?}"
        );
        for craft in [
            "examples", "new", "init", "check", "run", "test", "trace", "welcome", "doctor",
            "model", "wire", "explain",
        ] {
            assert!(
                visible.contains(&craft),
                "`{craft}` is the day-one craft and must stay visible: {visible:?}"
            );
        }
        let hidden = cmd
            .get_subcommands()
            .filter(|c| c.is_hide_set() && c.get_name() != "help")
            .count();
        assert_eq!(
            visible.len() + hidden,
            total,
            "visible + hidden is the WHOLE tree — ranged, never removed"
        );
        // The un-hide pass reaches every verb: the --all surface shows
        // exactly the full enum (the sum stays invariant by law).
        let all = <Cli as clap::CommandFactory>::command().mut_subcommands(|sc| sc.hide(false));
        let unhidden = all
            .get_subcommands()
            .filter(|c| !c.is_hide_set() && c.get_name() != "help")
            .count();
        assert_eq!(unhidden, total, "--all shows the whole surface");
    }
}
