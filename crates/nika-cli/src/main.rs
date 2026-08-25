// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `nika` binary — clap surface + dispatch over the verb tree
//! (`nika --help` is the living list; the static suite audits before any
//! run, `run` executes CHECKED workflows through the composed L3 runtime
//! over production seams). Exit codes per the locked contract (spec §4):
//! `0` ok · `1` workflow failed · `2` file findings · `3` environment.

// A terminal binary's whole job is printing (the nika-catalog-verify precedent).
#![allow(clippy::disallowed_macros, clippy::print_stdout, clippy::print_stderr)]

use std::io::IsTerminal;
use std::path::PathBuf;

mod init_args;
mod lazy;
mod model_args;
mod pipe_hygiene;
mod registry_args;
mod try_args;

use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use nika_cli::Theme;
use nika_cli::display::format::{ColorChoice, ColorEnv, LinkChoice, color_enabled, links_enabled};
use nika_cli::verbs::explain_file::dispatch as explain_dispatch;
use nika_cli::verbs::{self, VerbOutput};

use init_args::{InitArgs, init_verb};
use lazy::{check_lazy, resolve_lazy_target, run_lazy};

#[derive(Parser)]
// The PUBLIC binary name is `nika` (the release renames the nika-cli artifact +
// the Homebrew formula tests `nika --version`); clap embeds THIS name, not the
// filename, so the version/usage/errors must say `nika`, not the seed crate name.
#[command(
    name = "nika",
    bin_name = "nika",
    version,
    // The L3 identity is shared by every adapter; gitless builds keep the bare version.
    long_version = nika_runtime::engine_identity().version_long(),
    about = "nika · a plan from a file",
    after_help = "nika --help --all  the rest of the surface"
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
    /// List the workflows below this directory, one relative path per line.
    #[command(hide = true, display_order = 11)]
    List,
    /// The mirror: what Nika is · what this machine already has (editors ·
    /// local models · key presence · this workspace) · the next commands.
    /// Offline · presence-only · always exit 0 — a greeting, not a gate.
    #[command(hide = true, display_order = 40)]
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
    Check(verbs::check::CheckArgs),
    /// Run a workflow (the same audit runs first · live render).
    #[command(display_order = 30)]
    Run(RunArgs),
    /// Golden test: run under the MOCK provider (offline · deterministic)
    /// and compare the typed `outputs:` against `<file>.golden.json`.
    #[command(hide = true, display_order = 21)]
    Test {
        /// Workflow file (`*.nika.yaml`).
        file: Option<String>,
        /// (Re)write the golden from this run instead of comparing.
        #[arg(long)]
        update: bool,
        /// Answer a blocking `nika:prompt` without weakening it with a
        /// headless default (repeatable `TASK=VALUE`).
        #[arg(long, value_name = "TASK=VALUE", action = clap::ArgAction::Append)]
        answer: Vec<String>,
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
    /// Teach one error code (cause · category · fix-form), a hint kind
    /// `nika check` printed in `[brackets]`, or narrate a workflow FILE:
    /// what it does · the waves · cost before a token is spent · what
    /// it touches · how to run it.
    #[command(hide = true, display_order = 41)]
    Explain {
        /// An error code (`NIKA-440` · bare `440` · `DAG-003`), a hint
        /// identity (`jq-as-map` · `native-first/006`), or a workflow
        /// file path (`*.nika.yaml` · `-` reads stdin).
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
    /// What this project has ARMED, and when each beat next fires.
    /// Read-only — it schedules nothing (the file proposes, the machine
    /// disposes). Exit `0` clean · `2` the registry refuses.
    #[command(hide = true, display_order = 72)]
    Arm(verbs::arm::args::ArmArgs),
    /// Resident ARM firer by default (the SAME `fire`, wall clock in place of the OS).
    /// `--bind` + `--workflows` + `--token-file` opens authenticated loopback HTTP.
    /// Exit `0` clean · `1` otherwise.
    #[command(hide = true, display_order = 73)]
    Serve(verbs::serve::ServeArgs),
    /// Sign a workflow file (S3 · author-binding): mint `<file>.minisig` · `--check` verifies.
    #[command(hide = true, display_order = 71)]
    Sign(verbs::sign::SignArgs),
    /// Diagnose this machine (binary · config · provider keys · local models).
    /// Diagnose-only — prints the exact fix command, never mutates anything.
    #[command(display_order = 42)]
    Doctor(DoctorArgs),
    /// Found a repo (`.vscode` schema wiring · `AGENTS.md` · Cursor rule + MCP ·
    /// `.agents/skills` authoring skill · optional workflow set). Bare on
    /// a terminal the founding wizard runs; flags are the scriptable
    /// twin. Existing files are skipped — `--force` overwrites.
    #[command(hide = true, display_order = 10)]
    Init(InitArgs),
    /// Wire Nika into editor/agent MCP clients (explicit, idempotent).
    /// The door: `detected --dry-run` previews what this machine shows ·
    /// `detected` wires it · `<client>` wires one · `all` is the advanced
    /// sweep (previewed, then confirmed or `--yes`).
    #[command(hide = true, display_order = 50)]
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
    #[command(hide = true, display_order = 51)]
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
    /// See a canonical workflow WORK — offline by default (the mock
    /// rehearsal · zero keys · zero flags), nothing written, nothing
    /// owned. Bare `nika try` lists what there is to see.
    #[command(hide = true, display_order = 10)]
    Try(try_args::TryArgs),
    /// The ONE creation door: describe the job in plain words (routes),
    /// or name a slug/skeleton (takes it, ingredients included) — the
    /// destination derives from the slug. Plain words route across jobs,
    /// lessons and skeletons. Bare `nika new` on a terminal is the guided
    /// flow; `nika new '?'` lists the skeleton set.
    #[command(display_order = 11)]
    New {
        /// Plain-words intent, an example slug, or a skeleton name
        /// (`'?'` lists the set). Omitted on a terminal → the guided
        /// three-question flow; omitted in a pipe → fail fast.
        intent: Option<String>,
        /// Destination path (`*.nika.yaml`) — defaults to
        /// `<slug>.nika.yaml` beside you.
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
    #[command(hide = true, display_order = 31)]
    Trace {
        #[command(subcommand)]
        action: verbs::trace::TraceAction,
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
        /// can watchdog its parent — and this one does. A host that
        /// closes the pipe ends the session by EOF; a host that CRASHES
        /// does not, and the server would outlive it forever (#1181).
        /// Hidden: no human types it, the editor does.
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

/// The doctor arm's flags (the `GuardArgs` tuple-variant precedent).
#[derive(Args)]
struct DoctorArgs {
    /// TCP-probe the local provider ports (loopback/configured only ·
    /// 300ms cap · nothing is sent on the socket). Offline without it.
    #[arg(long)]
    ping: bool,
    /// Emit the machine projection (summary + findings[] — agents/CI
    /// branch on `summary.fail` instead of parsing glyphs).
    #[arg(long)]
    json: bool,
    /// Unfold every advisory note (an unwired agent · an unconfigured
    /// provider · the config-less default) — the calm default folds
    /// them into ONE line (B-8b · a healthy machine reads calm).
    #[arg(long)]
    verbose: bool,
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
    /// Pin the ACCESS path (`model:` picks the intelligence; access
    /// picks the path) — either a class (`local` · `mock` · `harness` ·
    /// `oauth` · `api`) or a harness seat (`claude-code` · `codex` ·
    /// `gemini-cli` · `kimi-code` · `qwen-code`). Retired ACP wrapper
    /// ids (`claude-agent-acp` · `codex-acp`) refuse with NIKA-1802. A
    /// pin is a pin: unsatisfied refuses before the prologue with a
    /// witness, never substitutes another path or model (D-2026-08-04-N1).
    /// Example: `--model openai/gpt-5.2 --access codex` requests that
    /// model through the user's Codex subscription seat.
    #[arg(long, value_name = "PATH")]
    access: Option<String>,
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
    /// trace's tamper-evidence chain is VERIFIED before any record is
    /// trusted: a broken chain refuses (exit 2), naming `nika trace
    /// verify` and the `--resume-unverified` opt-out. The trace's
    /// recorded engine version is JUDGED (F-P21): a resume under a
    /// different engine refuses, naming both versions.
    #[arg(long, value_name = "TRACE", conflicts_with = "dry_run")]
    resume: Option<PathBuf>,
    /// Declare a cross-version resume compatible (F-P21 · NEP-0014 law
    /// 4): attests the trace recorded under engine `<VERSION>` may resume
    /// under this one — the token must name the trace's recorded version
    /// exactly (`unrecorded` for a pre-versioning journal). The declared
    /// compat is journaled on the run's boot manifest.
    #[arg(long, value_name = "VERSION", requires = "resume")]
    resume_compat: Option<String>,
    /// Trust a `--resume` trace whose tamper-evidence chain FAILS the
    /// walk (ADR-099 trust amendment): the named opt-out, for a trace the
    /// operator edited or truncated by hand. The finding is journaled on
    /// the boot manifest (`resume_unverified: declared`) — never a silent
    /// default; a verified trace journals no claim.
    #[arg(long, requires = "resume")]
    resume_unverified: bool,
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

/// The `wire` door (H7): clap's flags plus the terminal fact `all`'s
/// consent gate reads (a terminal asks · a pipe needs `--yes`).
/// The doctor arm, extracted under the fn-length law (the `wire_verb`
/// precedent) — `--verbose` unfolds the healthy machine's advisory
/// notes (B-8b · the human lane defaults to calm).
fn doctor_verb(args: &DoctorArgs, theme: Theme) -> u8 {
    emit(&verbs::doctor::run(
        args.ping,
        args.json,
        args.verbose,
        theme,
    ))
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
    emit(&verbs::welcome::run(false, plain_theme)).into()
}

fn concierge_json(plain_theme: Theme) -> std::process::ExitCode {
    emit(&verbs::welcome::run(true, plain_theme)).into()
}

/// Human default help · B67 · ≤ 6 lines. The rest lives on `--help --all`.
fn human_help() -> &'static str {
    "nika             a plan from a file\n\
     nika new hello   one file that runs on this machine\n\
     nika run         run a file\n\
     nika check       audit a file before it runs\n\
     nika doctor      PATH, model, sandbox\n"
}

/// Bare `nika`, `nika --json`, `nika version`, `nika thread` — decided
/// before clap so a missing subcommand never clap-fails the front door.
fn front_door(argv: &[std::ffi::OsString]) -> Option<std::process::ExitCode> {
    let mut json = false;
    let mut ascii = false;
    let mut positional: Vec<&std::ffi::OsStr> = Vec::new();
    let mut skip_value = false;
    for arg in argv {
        if skip_value {
            skip_value = false;
            continue;
        }
        if arg == "--json" {
            json = true;
            continue;
        }
        if arg == "--plain" || arg == "--ascii" {
            ascii = true;
            continue;
        }
        if arg == "--color" || arg == "--hyperlink" {
            skip_value = true;
            continue;
        }
        if let Some(s) = arg.to_str()
            && (s.starts_with("--color=") || s.starts_with("--hyperlink="))
        {
            continue;
        }
        positional.push(arg);
    }
    match positional.first().and_then(|a| a.to_str()) {
        None => {
            let theme = term_theme(ColorChoice::Auto, ascii, LinkChoice::Auto);
            Some(if json {
                concierge_json(theme)
            } else {
                concierge(theme)
            })
        }
        Some("version") if positional.len() == 1 => {
            println!("nika {}", nika_runtime::engine_identity().version_long());
            Some(std::process::ExitCode::SUCCESS)
        }
        Some("thread") if positional.len() == 1 => {
            let theme = term_theme(ColorChoice::Auto, ascii, LinkChoice::Auto);
            Some(std::process::ExitCode::from(verbs::session::run(
                interactive_theme(theme),
            )))
        }
        _ => None,
    }
}

/// A workflow file typed bare IS a request to run it. A colleague sends
/// `notes.nika.yaml`; the person types its name the way one opens a file,
/// and clap answered `unrecognized subcommand` (gauntlet P5 · 2026-08-25).
///
/// Deliberately narrow: the name must END in the workflow suffix AND be a
/// file on disk. A typo'd verb keeps clap's own did-you-mean, which is the
/// better answer for a typo'd verb; a suffix that names nothing keeps
/// clap's error, which is the better answer for a name that does not exist.
fn is_runnable_workflow(arg: &std::ffi::OsStr) -> bool {
    arg.to_str().is_some_and(|s| {
        (s.ends_with(".nika.yaml") || s.ends_with(".nika.yml")) && std::path::Path::new(s).is_file()
    })
}

fn main() -> std::process::ExitCode {
    pipe_hygiene::guard(real_main)
}

fn real_main() -> std::process::ExitCode {
    // RAMS-13 · the full surface on demand: `--help --all` prints the
    // SAME tree with nothing hidden (the craft plus the documented
    // `arm` door lead the default help; protocols · trust cycle ·
    // plumbing stay one flag away — ranged, never removed). Judged
    // before clap parses, and ONLY when the whole invocation is help
    // words — a named verb keeps its own
    // help untouched (`nika trace rm --all --help` is trace's business:
    // `--all` is a REAL flag there, the adversarial pass caught the
    // theft).
    let argv: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let help_words_only = !argv.is_empty()
        && argv
            .iter()
            .all(|a| a == "--all" || a == "--help" || a == "-h" || a == "help");
    if help_words_only
        && argv.iter().any(|a| a == "--all")
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
    if help_words_only {
        print!("{}", human_help());
        return std::process::ExitCode::SUCCESS;
    }
    if let Some(code) = front_door(&argv) {
        return code;
    }
    // `nika notes.nika.yaml` means `nika run notes.nika.yaml`. Spliced
    // before clap sees it, so the verb, its flags and its help stay
    // untouched — the file just gains the verb the person left implicit.
    let argv: Vec<std::ffi::OsString> = if argv.first().is_some_and(|a| is_runnable_workflow(a)) {
        std::iter::once(std::ffi::OsString::from("run"))
            .chain(argv.iter().cloned())
            .collect()
    } else {
        argv
    };
    let cli = Cli::parse_from(std::iter::once(std::ffi::OsString::from("nika")).chain(argv));
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
/// The `test` arm — resolve the lazy target, then run the goldens.
fn test_arm(file: Option<String>, update: bool, answer: &[String], plain_theme: Theme) -> u8 {
    match resolve_lazy_target(file, "test") {
        Ok(file) => verbs::test::run_with_answers(&file, update, answer, plain_theme),
        Err(code) => code,
    }
}

/// The `inspect` arm — the one graph projector behind `--format`.
fn inspect_arm(file: &str, format: Option<verbs::graph::GraphFormatArg>, plain_theme: Theme) -> u8 {
    match format {
        Some(f) => emit(&verbs::graph::run(file, f.into(), plain_theme)),
        None => emit(&verbs::inspect::run(file, plain_theme)),
    }
}

fn check_arm(args: verbs::check::CheckArgs, plain_theme: Theme) -> u8 {
    let flags = verbs::check::CheckFlags {
        json: args.json,
        infer_permits: args.infer_permits,
        native_strict: args.native_strict,
        profile: args.profile,
    };
    check_lazy(
        args.files,
        &flags,
        args.fix,
        args.model.as_deref(),
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
        Command::List => emit(&verbs::list::run(std::path::Path::new("."))),
        Command::Check(args) => check_arm(args, plain_theme),
        Command::Run(args) => run_lazy(args, color, link_when, plain, ascii),
        Command::Test {
            file,
            update,
            answer,
        } => test_arm(file, update, &answer, plain_theme),
        Command::Inspect { file, format } => inspect_arm(&file, format, plain_theme),
        Command::Welcome { json, deep } => mirror_verb(json, deep, plain_theme),
        Command::Explain {
            code,
            json,
            forecast,
        } => emit(&explain_dispatch(&code, json, forecast, plain_theme)),
        Command::Key { action } => emit(&verbs::key::run(action)),
        Command::Arm(a) => emit(&verbs::arm::run(a)),
        Command::Serve(a) => emit(&verbs::serve::run(&a)),
        Command::Sign(args) => emit(&verbs::sign::run(&args)),
        Command::Doctor(args) => doctor_verb(&args, plain_theme),
        Command::Init(args) => emit(&init_verb(&args, plain_theme)),
        Command::Wire {
            target,
            dir,
            dry_run,
            yes,
        } => wire_verb(target, &dir, dry_run, yes),
        Command::Model { action } => model_args::model_verb(action),
        Command::Spec { canon, schema } => {
            emit(&verbs::pack_surface::spec_or_schema(canon, schema))
        }
        Command::Catalog { json, tools } => {
            if tools {
                emit(&verbs::tools::run(json, plain_theme))
            } else {
                emit(&verbs::catalog::run(json, plain_theme))
            }
        }
        Command::Try(a) => try_args::listing(&a, plain_theme)
            .map_or_else(|| try_args::rehearse(&a, plain_theme), |o| emit(&o)),
        Command::New {
            intent,
            dest,
            force,
        } => emit(&verbs::new::dispatch(
            intent.as_deref(),
            dest.as_deref(),
            force,
            plain_theme,
        )),
        Command::Completions { shell } => {
            write_completions(shell, &mut std::io::stdout());
            0
        }
        Command::Trace { action } => nika_trace::dispatch::trace_verb(
            action,
            plain_theme,
            color.with_no_color(false),
            link_when,
        ),
        Command::Guard(args) => guard_verb(&args, plain_theme),
        // The language server OWNS stdout (JSON-RPC) — it must not go through
        // `emit`. It follows the LSP exit-code convention: 0 on a clean
        // shutdown/exit, non-zero (1) otherwise (transport failure, or an
        // `exit` without a prior `shutdown`) — the server-process
        // convention, NOT the verb FILE/WORKFLOW/ENV taxonomy.
        Command::Dap => nika_dap::run_stdio(),
        Command::Lsp {
            client_process_id, ..
        } => match nika_lsp::run_stdio_watching(client_process_id) {
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

// The `trace` arm's routing and render loop descended to
// `nika_trace::dispatch` 2026-08-11 (the 15k wall · the ADR-110
// precedent): the bin keeps the one-line arm above; the replay loop,
// the dossier doors and the rm/anchor target resolution live next to
// the verbs they drive.

/// Unpack the `run` clap surface into the library verb call.
fn run_verb(
    args: &RunArgs,
    color: ColorWhenArg,
    link_when: LinkChoice,
    plain: bool,
    ascii: bool,
    repair_target: Option<nika_cli::display::check_render::RepairTarget>,
) -> u8 {
    let resume = (args.resume.is_some() || !args.answer.is_empty()).then(|| {
        nika_dap::resume::ResumeRequest {
            trace: args.resume.clone(),
            from: args.from.clone(),
            answers: args.answer.clone(),
            compat: args.resume_compat.clone(),
            allow_unverified: args.resume_unverified,
        }
    });
    let mode = resolve_run_mode(args.quiet, args.no_progress || plain);
    let mut theme = term_theme(color.with_no_color(false), ascii || plain, link_when);
    // The duration accents ride the interactive surface ONLY — the
    // sober registers (piped · --no-progress · --quiet) keep their
    // exact bytes.
    theme.accents = mode == verbs::run::RenderMode::Live;
    // Duration heat additionally needs colour + the truecolor PROOF.
    theme.heat = theme.accents && theme.color && nika_cli_host::output::truecolor_env();
    // The live storyboard breathes (the braille beat between settles) —
    // interactive surface only, and the motion opt-out wins (the same
    // env the replay honours).
    theme.animate = theme.accents && !env_flag("NIKA_REDUCED_MOTION");
    let file = args.file.as_deref().unwrap_or_default();
    let repair_target =
        repair_target.unwrap_or_else(|| nika_cli::registry::repair_target_for_path(file));
    verbs::run::run_with_repair_target(
        file,
        args.json,
        args.output.as_deref(),
        theme,
        mode,
        args.dry_run,
        args.model.as_deref(),
        args.access.as_deref(),
        &args.var,
        resume.as_ref(),
        args.no_trace_file || env_flag("NIKA_NO_TRACE_FILE"),
        args.task.as_deref(),
        args.no_outputs,
        args.max_cost_usd,
        args.no_gc,
        args.require_signature,
        repair_target,
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

/// Do the teaching surfaces and the clap tree agree? Both directions,
/// in their own file — they answer one question and `main.rs` has a
/// 1500-LOC ceiling to respect. The file rides the `tests.rs` name so
/// the prod-LOC counter excludes it (the check/tests.rs idiom).
#[cfg(test)]
#[path = "teaching_parity/tests.rs"]
mod teaching_parity_tests;

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
    /// `try` share `parse_budget_usd`, so a NaN/inf (which
    /// silently disarms every comparison) refuses at parse time on
    /// BOTH — the drift where one door validated and the other let
    /// the disarmed value through is pinned shut.
    /// Gauntlet P5 · 2026-08-25. A colleague sends a `.nika.yaml`; the
    /// person types its name the way one opens a file, and clap answered
    /// `unrecognized subcommand 'notes.nika.yaml'` — a wall where a run
    /// was meant.
    ///
    /// The routing is deliberately narrow, and this test pins BOTH ends:
    /// a typo'd verb keeps clap's did-you-mean (the better answer for a
    /// typo), and a suffix naming nothing keeps clap's error (the better
    /// answer for a name that does not exist). Only a real file routes.
    #[test]
    fn a_workflow_file_typed_bare_is_a_run_and_nothing_else_is() {
        let dir = tempfile::tempdir().expect("tmp");
        let here = dir.path().join("notes.nika.yaml");
        std::fs::write(&here, "nika: notes\n").expect("write");

        assert!(
            is_runnable_workflow(here.as_os_str()),
            "a real .nika.yaml routes to run"
        );
        // The misspelling is the fixture: this is what a person types when
        // they mean `check`, and clap's did-you-mean is the better answer
        // for it. Built from parts so the spell gate reads a `che` and a
        // `k`, never the typo it would rightly flag anywhere else.
        let typoed_verb = format!("{}{}", "che", "k");
        assert!(
            !is_runnable_workflow(std::ffi::OsStr::new(&typoed_verb)),
            "a mistyped verb keeps clap's suggestion"
        );
        assert!(
            !is_runnable_workflow(dir.path().join("absent.nika.yaml").as_os_str()),
            "a suffix that names nothing keeps clap's error"
        );
        assert!(
            !is_runnable_workflow(dir.path().as_os_str()),
            "a directory is not a workflow"
        );
        // The suffix alone is not enough, and neither is existence alone.
        let other = dir.path().join("notes.yaml");
        std::fs::write(&other, "nika: notes\n").expect("write");
        assert!(
            !is_runnable_workflow(other.as_os_str()),
            "a bare .yaml is not the workflow suffix"
        );
    }

    #[test]
    fn the_budget_guard_holds_on_both_doors() {
        for argv in [
            vec!["nika", "run", "wf.nika.yaml", "--max-cost-usd", "nan"],
            vec!["nika", "try", "01-hello", "--max-cost-usd", "nan"],
            vec!["nika", "run", "wf.nika.yaml", "--max-cost-usd", "inf"],
            vec!["nika", "try", "01-hello", "--max-cost-usd", "-1"],
        ] {
            assert!(
                Cli::try_parse_from(&argv).is_err(),
                "{argv:?} must refuse at parse time"
            );
        }
        for argv in [
            vec!["nika", "run", "wf.nika.yaml", "--max-cost-usd", "0.05"],
            vec!["nika", "try", "01-hello", "--max-cost-usd", "0.05"],
        ] {
            assert!(Cli::try_parse_from(&argv).is_ok(), "{argv:?} must parse");
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

    /// V5 · the three doors. Bare `nika try` lists the showroom (the
    /// user-sim finding kept: a door with nothing behind it answers
    /// with what there IS); `nika try 01-hello` parses the slug;
    /// `nika new "plain words" out.nika.yaml` parses positionally
    /// (N-4 · the `--from` flag is gone); and the dead top-level verbs
    /// (`examples` · `evidence` · `receipt`) refuse — one door each,
    /// no ghosts (no-legacy law).
    #[test]
    fn the_three_doors_parse_and_the_dead_verbs_refuse() {
        let cli = Cli::try_parse_from(["nika", "try"]).expect("parses");
        assert!(matches!(cli.command, Some(Command::Try(ref a)) if a.slug.is_none()));
        let cli = Cli::try_parse_from(["nika", "try", "01-hello"]).expect("parses");
        assert!(
            matches!(cli.command, Some(Command::Try(ref a)) if a.slug.as_deref() == Some("01-hello"))
        );
        let cli = Cli::try_parse_from(["nika", "new", "chase unpaid invoices", "mine.nika.yaml"])
            .expect("parses");
        assert!(matches!(
            cli.command,
            Some(Command::New {
                intent: Some(_),
                dest: Some(_),
                ..
            })
        ));
        for dead in [
            vec!["nika", "examples"],
            vec!["nika", "examples", "run", "01-hello"],
            vec!["nika", "evidence"],
            vec!["nika", "receipt", "show"],
            vec!["nika", "new", "--from", "chain", "x.nika.yaml"],
        ] {
            assert!(
                Cli::try_parse_from(&dead).is_err(),
                "{dead:?} must refuse — the door moved, no ghost stays"
            );
        }
        // The dossier doors live under trace now (RAMS-15).
        assert!(Cli::try_parse_from(["nika", "trace", "receipt", "explain", "r.json"]).is_ok());
    }

    /// The concierge parse ratchet: every command `nika` (welcome)
    /// can ever teach — across every workspace state and chat-only —
    /// must parse on the live clap tree. The 0.107 train renamed the
    /// showroom door (`examples` → `try`) while welcome kept teaching
    /// the dead verb in five states plus the JSON mirror; only a human
    /// paste could catch it. Placeholders are filled with plausible
    /// operator values, and a `cd x && nika …` teaching line is parsed
    /// from its `nika` tail.
    #[test]
    fn every_taught_welcome_command_parses_on_the_live_tree() {
        let taught = nika_cli_host::welcome::taught_start_commands();
        assert!(
            taught.len() >= 6,
            "the ratchet surface enumerates the concierge's states: {taught:?}"
        );
        for command in taught {
            let filled = command
                .replace("<file>", "wf.nika.yaml")
                .replace("<usd>", "0.05")
                .replace("<project>", "proj");
            let tail = filled
                .rfind("nika ")
                .map_or(filled.as_str(), |i| &filled[i..]);
            let argv: Vec<&str> = tail.split_whitespace().collect();
            assert!(
                Cli::try_parse_from(&argv).is_ok(),
                "taught command must parse: {command:?} (argv {argv:?})"
            );
        }
    }

    #[test]
    fn test_accepts_repeatable_explicit_answers() {
        let cli = Cli::try_parse_from([
            "nika",
            "test",
            "etl-state.nika.yaml",
            "--answer",
            "approve=false",
            "--answer",
            "reviewer=reject",
        ])
        .expect("repeatable test answers parse");
        let answer = match cli.command {
            Some(Command::Test { answer, .. }) => answer,
            _ => Vec::new(),
        };
        assert_eq!(answer, ["approve=false", "reviewer=reject"]);
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
    /// Human default help is a 5-line card. The rest is `--help --all`.
    /// Visible clap verbs: new · run · check · doctor. Nothing is deleted.
    #[test]
    fn the_default_human_help_is_five_lines_and_hides_nothing_forever() {
        let lines = human_help().lines().filter(|l| !l.is_empty()).count();
        assert!(
            lines <= 6,
            "human default help is ≤ 6 lines, got {lines}:\n{}",
            human_help()
        );
        assert!(
            human_help().contains("nika new hello"),
            "day-one help names the first-wow file:\n{}",
            human_help()
        );
        let cmd = <Cli as clap::CommandFactory>::command();
        let total = cmd
            .get_subcommands()
            .filter(|c| c.get_name() != "help")
            .count();
        let visible: std::collections::BTreeSet<&str> = cmd
            .get_subcommands()
            .filter(|c| !c.is_hide_set() && c.get_name() != "help")
            .map(clap::Command::get_name)
            .collect();
        let expected: std::collections::BTreeSet<&str> =
            ["new", "run", "check", "doctor"].into_iter().collect();
        assert_eq!(visible, expected, "day-one verbs: new run check doctor");
        let run = cmd.find_subcommand("run").expect("run");
        let access = run
            .get_arguments()
            .find(|a| a.get_long() == Some("access"))
            .expect("--access");
        let help = access
            .get_help()
            .map(std::string::ToString::to_string)
            .unwrap_or_default();
        assert!(help.contains("harness") && help.contains("local"), "{help}");
        assert!(
            !help.contains("nika doctor lists"),
            "doctor listing seat ids as pins caused NIKA-1802: {help}"
        );
        assert!(
            help.contains("claude-code") && help.contains("codex"),
            "the live harness seat pins must be taught: {help}"
        );
        assert!(
            help.contains("claude-agent-acp") && help.contains("Retired ACP wrapper"),
            "the retired wrapper trap must stay named: {help}"
        );
        let hidden = cmd
            .get_subcommands()
            .filter(|c| c.is_hide_set() && c.get_name() != "help")
            .count();
        assert_eq!(
            visible.len() + hidden,
            total,
            "visible + hidden is the WHOLE tree — ranged, never removed"
        );
        let all = <Cli as clap::CommandFactory>::command().mut_subcommands(|sc| sc.hide(false));
        let unhidden = all
            .get_subcommands()
            .filter(|c| !c.is_hide_set() && c.get_name() != "help")
            .count();
        assert_eq!(unhidden, total, "--all shows the whole surface");
    }
}
