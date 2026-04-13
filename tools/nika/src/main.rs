// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Nika CLI - DAG workflow runner

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod cli;

use clap::{ArgAction, CommandFactory, Parser, Subcommand, ValueEnum};
use colored::Colorize;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use nika::error::NikaError;

// Extracted to nika-cli in Phase 15.
use cli::bench::run_bench;
use cli::check::{validate_workflow, validate_workflow_strict};
use cli::demo::{print_agent_presets, run_demo};
use cli::discover::{count_nika_workflows, is_nika_workflow, resolve_or_discover_workflow};
use cli::eval::eval_workflow;
use cli::explain::explain_workflow;
use cli::run::{dry_run_workflow, run_workflow};
use cli::test_cmd::test_workflow;

// ═══════════════════════════════════════════════════════════════════════════
// HELP TEXT
// ═══════════════════════════════════════════════════════════════════════════

const LONG_ABOUT: &str = "\
Nika \u{2014} Semantic YAML workflow engine for AI tasks

5 verbs: infer (LLM), exec (shell), fetch (HTTP), invoke (MCP), agent (multi-turn)
9 providers: anthropic, openai, mistral, groq, deepseek, gemini, xai, native, mock

Run `nika help` for the full command reference with examples.";

const AFTER_HELP: &str = "\
QUICK START:
    nika infer \"Explain AI\"       Quick LLM call
    nika run workflow.nika.yaml   Run a workflow
    nika help                     Full command reference
    nika help verbs               The 5 semantic verbs
    nika help providers           Provider status
    nika help examples            Common patterns

DOCUMENTATION:
    https://github.com/supernovae-st/nika";

// ═══════════════════════════════════════════════════════════════════════════
// BUILD METADATA
// ═══════════════════════════════════════════════════════════════════════════

/// Extended version string with channel, git hash, and build time.
fn long_version() -> &'static str {
    const VERSION: &str = env!("CARGO_PKG_VERSION");
    const CHANNEL: &str = env!("NIKA_BUILD_CHANNEL");
    const HASH: &str = env!("NIKA_GIT_HASH");

    // Use a static OnceLock so we compute the string once
    use std::sync::OnceLock;
    static LONG: OnceLock<String> = OnceLock::new();
    LONG.get_or_init(|| {
        let ts: u64 = env!("NIKA_BUILD_TIMESTAMP").parse().unwrap_or(0);
        let ago = if ts == 0 {
            "unknown".to_string()
        } else {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let delta = now.saturating_sub(ts);
            if delta < 60 {
                "just now".to_string()
            } else if delta < 3600 {
                format!("{}min ago", delta / 60)
            } else if delta < 86400 {
                format!("{}h ago", delta / 3600)
            } else {
                format!("{}d ago", delta / 86400)
            }
        };
        match CHANNEL {
            "release" => format!("{VERSION} (release)"),
            _ => format!("{VERSION}-{CHANNEL} ({HASH}, built {ago})"),
        }
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// CLI STRUCTURE
// ═══════════════════════════════════════════════════════════════════════════

/// Color output mode (like cargo/git)
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum ColorChoice {
    /// Auto-detect based on terminal support
    #[default]
    Auto,
    /// Always use colors
    Always,
    /// Never use colors
    Never,
}

/// Styled help text for the CLI (cosmic theme).
fn cli_styles() -> clap::builder::Styles {
    use clap::builder::styling::{AnsiColor, Style};
    clap::builder::Styles::styled()
        .header(
            Style::new()
                .bold()
                .fg_color(Some(AnsiColor::Magenta.into())),
        )
        .usage(Style::new().bold().fg_color(Some(AnsiColor::Cyan.into())))
        .literal(Style::new().fg_color(Some(AnsiColor::Green.into())))
        .placeholder(Style::new().fg_color(Some(AnsiColor::Cyan.into())))
        .valid(Style::new().fg_color(Some(AnsiColor::Green.into())))
        .invalid(Style::new().fg_color(Some(AnsiColor::Red.into())))
        .error(Style::new().bold().fg_color(Some(AnsiColor::Red.into())))
}

#[derive(Parser)]
#[command(name = "nika")]
#[command(version, long_version = long_version())]
#[command(about = "Nika - DAG workflow runner for AI tasks")]
#[command(long_about = LONG_ABOUT)]
#[command(after_help = AFTER_HELP)]
#[command(styles = cli_styles())]
#[command(disable_help_subcommand = true)]
struct Cli {
    /// Workflow file to run directly (e.g., workflow.nika.yaml)
    #[arg(value_name = "WORKFLOW")]
    file: Option<PathBuf>,

    /// Increase verbosity (-v info, -vv debug, -vvv trace)
    #[arg(short, long, action = ArgAction::Count, global = true)]
    verbose: u8,

    /// Suppress all output except errors
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Color output: auto, always, never
    #[arg(long, default_value = "auto", global = true, value_enum)]
    color: ColorChoice,

    /// Detail level for run output: max (default), default, min, json
    #[arg(long, default_value = "max", global = true)]
    detail: nika::display::DetailLevel,

    /// Disable live animated display (use classic append-only output)
    #[arg(long, global = true)]
    no_live: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a workflow file (headless, no TUI)
    #[command(next_help_heading = "WORKFLOWS", visible_alias = "r")]
    Run {
        /// Path to .nika.yaml file (auto-discovered if omitted)
        file: Option<String>,

        /// Override default provider (anthropic, openai, mistral, groq, deepseek, gemini, xai, native, mock)
        #[arg(short, long)]
        provider: Option<String>,

        /// Override default model
        #[arg(short, long)]
        model: Option<String>,

        /// Override workflow input (repeatable): -i url=https://example.com -i lang=en
        #[arg(short = 'i', long = "input", value_name = "KEY=VALUE")]
        inputs: Vec<String>,

        /// Load inputs from JSON/YAML file (or "-" for stdin)
        #[arg(long, value_name = "FILE")]
        input_file: Option<String>,

        /// Validate and show execution plan without running
        #[arg(long)]
        dry_run: bool,

        /// Save all task outputs to a JSON file
        #[arg(short = 'o', long = "output", value_name = "FILE")]
        output: Option<String>,

        /// Skip interactive prompts (fail on missing inputs)
        #[arg(long)]
        no_interactive: bool,

        /// Run only this task and its dependencies
        #[arg(long, value_name = "TASK_ID")]
        task: Option<String>,

        /// Run from this task onwards (skip earlier layers)
        #[arg(long, value_name = "TASK_ID")]
        from: Option<String>,

        /// Skip cost confirmation (auto-accept)
        #[arg(short = 'y', long)]
        yes: bool,

        /// Permission mode for file tools: deny, plan, accept-edits, yolo
        #[arg(long, default_value = "accept-edits")]
        permission: String,

        /// Resume from last run: skip tasks that already succeeded
        #[arg(long)]
        resume: bool,
    },

    /// Call an LLM directly (no workflow needed)
    ///
    /// Examples:
    ///   nika infer "Explain quantum computing"
    ///   cat file.txt | nika infer "Summarize" --stdin
    ///   nika infer "Extract names" --from-example '{"names":[""]}'
    #[command(next_help_heading = "5 VERBS", visible_alias = "i")]
    Infer {
        /// Prompt (use "-" for stdin)
        prompt: String,
        /// Provider (auto-detected from API keys if omitted)
        #[arg(short, long)]
        provider: Option<String>,
        /// Model (supports provider/model: anthropic/claude-sonnet-4-6)
        #[arg(short, long)]
        model: Option<String>,
        /// System prompt
        #[arg(short, long)]
        system: Option<String>,
        /// Temperature (0.0-2.0)
        #[arg(short, long)]
        temperature: Option<f64>,
        /// Max output tokens
        #[arg(long)]
        max_tokens: Option<u32>,
        /// Force JSON output
        #[arg(long)]
        json: bool,
        /// Structured output from example (inline JSON or file path)
        #[arg(long, value_name = "EXAMPLE")]
        from_example: Option<String>,
        /// Read context from stdin (prepended to prompt)
        #[arg(long)]
        stdin: bool,
        /// Skip interactive prompts (for scripts/CI/VPS)
        #[arg(long)]
        no_interactive: bool,
        /// Suppress non-essential output
        #[arg(short, long)]
        quiet: bool,
    },

    /// Fetch a URL with smart extraction (9 modes)
    ///
    /// Examples:
    ///   nika fetch https://blog.com --extract article
    ///   nika fetch https://api.x.com/data --extract jsonpath --selector ".items"
    #[command(next_help_heading = "5 VERBS", visible_alias = "f")]
    Fetch {
        /// URL to fetch
        url: String,
        /// Extraction mode
        #[arg(short, long, value_parser = ["markdown", "article", "text",
            "selector", "metadata", "links", "jsonpath", "feed", "llm_txt"])]
        extract: Option<String>,
        /// CSS selector or JSONPath expression
        #[arg(long)]
        selector: Option<String>,
        /// HTTP method (default: GET)
        #[arg(short = 'X', long)]
        method: Option<String>,
        /// HTTP header (repeatable): -H "Key: Value"
        #[arg(short = 'H', long = "header", value_name = "KEY:VALUE")]
        headers: Vec<String>,
        /// Request body
        #[arg(long)]
        body: Option<String>,
        /// JSON body (auto Content-Type)
        #[arg(long, value_name = "JSON")]
        json_body: Option<String>,
        /// Response mode: full | binary
        #[arg(long, value_parser = ["full", "binary"])]
        response: Option<String>,
        /// Timeout in seconds
        #[arg(long)]
        timeout: Option<u64>,
        /// Skip interactive prompts (for scripts/CI/VPS)
        #[arg(long)]
        no_interactive: bool,
    },

    /// Call a builtin nika:* tool or MCP server tool
    ///
    /// Examples:
    ///   nika invoke nika:dimensions photo.jpg
    ///   nika invoke nika:thumbnail photo.jpg --params '{"width":200}'
    ///   nika invoke --list
    #[command(next_help_heading = "5 VERBS")]
    Invoke {
        /// Tool: nika:thumbnail, server::tool_name
        tool: Option<String>,
        /// File (auto-mapped to "source" param)
        #[arg(value_name = "FILE")]
        file: Option<String>,
        /// Tool parameters as JSON
        #[arg(long, value_name = "JSON")]
        params: Option<String>,
        /// MCP server name
        #[arg(long)]
        mcp: Option<String>,
        /// Timeout in seconds
        #[arg(long)]
        timeout: Option<u64>,
        /// List available builtin tools
        #[arg(long)]
        list: bool,
        /// Skip interactive prompts (for scripts/CI/VPS)
        #[arg(long)]
        no_interactive: bool,
    },

    /// Run a multi-turn AI agent with tools
    ///
    /// Examples:
    ///   nika agent "Research AI workflows" --tool web_search --turns 5
    ///   nika agent --list
    #[command(next_help_heading = "5 VERBS", visible_alias = "a")]
    Agent {
        /// Agent objective (required unless --list)
        prompt: Option<String>,
        /// List available agent presets (builtins + workflow-defined)
        #[arg(long)]
        list: bool,
        /// Provider
        #[arg(short, long)]
        provider: Option<String>,
        /// Model
        #[arg(short, long)]
        model: Option<String>,
        /// System prompt
        #[arg(short, long)]
        system: Option<String>,
        /// Available tool (repeatable)
        #[arg(short, long = "tool")]
        tools: Vec<String>,
        /// MCP server (repeatable)
        #[arg(long = "mcp")]
        mcp_servers: Vec<String>,
        /// Max turns (default: 10)
        #[arg(long, default_value = "10")]
        turns: u32,
        /// Max tokens per turn
        #[arg(long)]
        max_tokens: Option<u32>,
        /// Temperature
        #[arg(short = 'T', long)]
        temperature: Option<f64>,
        /// Read context from stdin
        #[arg(long)]
        stdin: bool,
        /// Skip interactive prompts (for scripts/CI/VPS)
        #[arg(long)]
        no_interactive: bool,
    },

    /// Validate workflow syntax, DAG structure, and bindings
    #[command(
        next_help_heading = "WORKFLOWS",
        alias = "validate",
        visible_alias = "v"
    )]
    Check {
        /// Path to .nika.yaml file
        file: String,

        /// Enable strict mode: connect to MCP servers and validate invoke params
        #[arg(long)]
        strict: bool,

        /// Run Nika Shield taint analysis (trust propagation + security warnings)
        #[arg(long)]
        security: bool,
    },

    /// Test a workflow with mock provider (no API keys needed)
    ///
    /// Runs the workflow with provider=mock, validates that all tasks complete
    /// successfully, and optionally compares output to a golden file.
    #[command(next_help_heading = "WORKFLOWS", visible_alias = "t")]
    Test {
        /// Path to .nika.yaml file (or glob pattern)
        file: String,

        /// Compare output to golden JSON file (fail if different)
        #[arg(long, value_name = "FILE")]
        golden: Option<String>,

        /// Update golden file with current output (snapshot mode)
        #[arg(long)]
        update_snapshot: bool,

        /// Override workflow input (repeatable): -i url=https://example.com
        #[arg(short = 'i', long = "input", value_name = "KEY=VALUE")]
        inputs: Vec<String>,
    },

    /// Lint a workflow for best practices (beyond syntax validation)
    ///
    /// Checks for: missing descriptions, unused tasks, missing retry,
    /// high concurrency, hardcoded secrets, missing timeouts.
    #[command(next_help_heading = "WORKFLOWS", visible_alias = "l")]
    Lint {
        /// Path to .nika.yaml file
        file: String,
    },

    /// Evaluate workflow quality against a dataset of assertions
    ///
    /// Runs a workflow multiple times with different inputs, validates
    /// each output against expected assertions, and reports PASS/FAIL.
    ///
    /// Dataset format: JSON array of {inputs, expected: {tasks: {task_id: assertions}}}.
    /// Assertions: output_contains, output_min_words, output_max_words, output_matches_schema.
    #[command(next_help_heading = "WORKFLOWS", visible_alias = "e")]
    Eval {
        /// Path to .nika.yaml workflow file
        file: String,

        /// Dataset file: JSON array with inputs + expected assertions
        #[arg(long, value_name = "FILE")]
        dataset: String,

        /// Override provider (default: mock for safety)
        #[arg(short = 'P', long)]
        provider: Option<String>,

        /// Output format: text | json
        #[arg(long, default_value = "text")]
        format: String,

        /// Fail on first assertion failure
        #[arg(long)]
        fail_fast: bool,

        /// Run N entries in parallel (default: 1 = sequential)
        #[arg(long, default_value = "1")]
        parallel: usize,

        /// Skip cost confirmation
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// Explain a workflow in human-readable format
    ///
    /// Parse the YAML, analyze the DAG, and print a summary: task count,
    /// layers, verbs used, providers required, estimated cost.
    #[command(next_help_heading = "WORKFLOWS")]
    Explain {
        /// Path to .nika.yaml file
        file: String,
    },

    /// Benchmark a workflow across multiple providers
    ///
    /// Runs the same workflow with different providers/endpoints, collects
    /// speed, cost, and quality metrics, displays comparison.
    ///
    /// Examples:
    ///   nika bench workflow.nika.yaml --providers anthropic,openai
    ///   nika bench workflow.nika.yaml --providers h100,anthropic --iterations 5
    ///   nika bench workflow.nika.yaml --providers mock --json
    #[command(next_help_heading = "WORKFLOWS", visible_alias = "b")]
    Bench {
        /// Path to .nika.yaml workflow file
        file: String,

        /// Comma-separated list of providers to benchmark
        #[arg(short = 'P', long, value_delimiter = ',', required = true)]
        providers: Vec<String>,

        /// Number of iterations per provider (default: 3)
        #[arg(short = 'n', long, default_value = "3")]
        iterations: usize,

        /// Show per-task profile (Gantt bars)
        #[arg(long)]
        profile: bool,

        /// Output results as JSON
        #[arg(long)]
        json: bool,

        /// Evaluate output quality using LLM-as-judge (requires API key)
        #[arg(long)]
        eval: bool,

        /// Judge model for quality evaluation (default: claude-haiku-4-5)
        #[arg(long, default_value = "claude-haiku-4-5")]
        eval_model: String,

        /// Skip cost confirmation
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// Initialize a Nika project (nika.toml + .nika/ + starter workflow)
    #[command(next_help_heading = "PROJECT")]
    Init {
        /// Permission mode: deny, plan, accept-edits, accept-all
        #[arg(short, long, default_value = "plan")]
        permission: String,

        /// Migrate API keys from environment variables to encrypted vault
        #[arg(long)]
        migrate_keys: bool,

        /// Generate interactive course files (12 levels, 44 exercises)
        #[arg(long)]
        course: bool,

        /// Skip interactive prompts (use defaults + CLI flags)
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// Interactive learning course
    #[command(next_help_heading = "LEARNING", visible_alias = "learn")]
    Course {
        #[command(subcommand)]
        action: cli::course::CourseAction,
    },

    /// Manage execution traces
    #[command(next_help_heading = "SYSTEM")]
    Trace {
        #[command(subcommand)]
        action: cli::trace::TraceAction,
    },

    /// Manage API keys and secrets
    #[command(next_help_heading = "MODELS & PROVIDERS", visible_alias = "k")]
    Keys {
        #[command(subcommand)]
        action: Option<cli::keys::KeysAction>,
        /// Output as JSON (for bare `nika keys --json`)
        #[arg(long, global = true)]
        json: bool,
        /// Show all details
        #[arg(long, short, global = true)]
        verbose: bool,
    },

    /// Manage LLM provider catalog (models, pricing, testing)
    #[command(next_help_heading = "MODELS & PROVIDERS")]
    Provider {
        #[command(subcommand)]
        action: cli::provider::ProviderAction,
    },

    /// Manage serve API tokens (multi-tenant auth)
    #[command(next_help_heading = "SYSTEM")]
    Token {
        #[command(subcommand)]
        action: cli::token::TokenAction,
    },

    /// Manage MCP server connections
    #[command(next_help_heading = "MODELS & PROVIDERS")]
    Mcp {
        #[command(subcommand)]
        action: cli::mcp::McpAction,
    },

    /// (moved to `nika keys`)
    #[command(hide = true)]
    Vault {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        _args: Vec<String>,
    },

    /// Clean project runtime state (traces, cache, media orphans)
    #[command(next_help_heading = "PROJECT")]
    Clean {
        /// Preview what would be removed without deleting
        #[arg(long)]
        dry_run: bool,

        /// Also remove serve.db and sessions
        #[arg(long)]
        all: bool,
    },

    /// Discover builtin tools (nika:*) and their parameter schemas
    #[command(next_help_heading = "SYSTEM")]
    Tools {
        #[command(subcommand)]
        action: cli::tools_cmd::ToolsAction,
    },

    /// Manage LLM models — cloud pricing + local GGUF
    ///
    /// `nika model` (no subcommand) lists all cloud models with pricing.
    /// Use `nika model list`, `nika model info <name>`, `nika model recommend`.
    /// Local model management (pull, delete, vision) requires native-inference.
    #[command(next_help_heading = "MODELS & PROVIDERS", visible_alias = "m")]
    Model {
        #[command(subcommand)]
        action: Option<cli::model_cmd::ModelAction>,
    },

    /// Show the full command reference or deep-dive into a topic
    ///
    /// Topics: verbs, providers, templates, examples
    #[command(next_help_heading = "HELP")]
    Help {
        /// Topic to explore (verbs, providers, templates, examples)
        topic: Option<String>,
    },

    /// Manage media store (list, stats, clean)
    ///
    /// List, inspect, and garbage-collect binary files stored in the
    /// Content-Addressable Store (CAS) at .nika/media/store/
    #[command(next_help_heading = "PROJECT")]
    Media {
        #[command(subcommand)]
        action: cli::media::MediaAction,
    },

    /// Generate shell completions (bash, zsh, fish, powershell)
    #[command(next_help_heading = "SYSTEM")]
    Completion {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },

    /// Manage Nika configuration
    #[command(next_help_heading = "PROJECT")]
    Config {
        #[command(subcommand)]
        action: cli::config::ConfigAction,
    },

    /// Manage schema versions and migrations
    #[command(next_help_heading = "SYSTEM")]
    Schema {
        #[command(subcommand)]
        action: cli::schema::SchemaAction,
    },

    /// Show compiled feature flags and capabilities
    #[command(next_help_heading = "SYSTEM")]
    Features,

    /// Browse and extract showcase workflows
    #[command(next_help_heading = "LEARNING")]
    Showcase {
        #[command(subcommand)]
        action: cli::showcase::ShowcaseAction,
    },

    /// Switch between dev and release channels
    #[command(next_help_heading = "SYSTEM")]
    Switch {
        /// Channel to switch to (dev, release)
        #[command(subcommand)]
        action: Option<cli::switch::SwitchAction>,

        /// One-time setup (create dirs, build dev, install hook)
        #[arg(long)]
        setup: bool,

        /// Force rebuild dev binary now
        #[arg(long)]
        build: bool,
    },

    /// Check system health and diagnose issues
    #[command(next_help_heading = "SYSTEM", visible_alias = "d")]
    Doctor {
        /// Run all checks including slow ones (MCP connectivity)
        #[arg(long)]
        full: bool,

        /// Output format: text, json
        #[arg(long, default_value = "text")]
        format: String,

        /// Auto-fix issues (runs machine setup)
        #[arg(long)]
        fix: bool,
    },

    /// Create a new workflow file
    #[command(next_help_heading = "WORKFLOWS", visible_alias = "n")]
    New {
        /// Workflow name (used for filename)
        name: Option<String>,

        /// Primary verb (infer, exec, fetch, invoke, agent)
        #[arg(long, value_name = "VERB")]
        verb: Option<String>,

        /// LLM provider (anthropic, openai, mistral, groq, deepseek, gemini, xai, native, mock)
        #[arg(short, long, value_name = "PROVIDER")]
        provider: Option<String>,

        /// Output directory (default: current directory)
        #[arg(short = 'd', long, value_name = "DIR")]
        output_dir: Option<PathBuf>,
    },

    /// Manage workflow files (edit, add-task, graph, check)
    #[command(next_help_heading = "WORKFLOWS", visible_alias = "w")]
    Workflow {
        #[command(subcommand)]
        action: cli::workflow::WorkflowAction,
    },

    /// Manage LLM response cache
    #[cfg(unix)]
    #[command(next_help_heading = "SYSTEM")]
    Cache {
        #[command(subcommand)]
        action: cli::cache_cmd::CacheAction,
    },

    /// Create a recurring schedule for a workflow
    #[cfg(unix)]
    #[command(next_help_heading = "SYSTEM")]
    Every(cli::every::EveryArgs),

    /// Manage cron schedules
    #[cfg(unix)]
    #[command(next_help_heading = "SYSTEM", alias = "schedules")]
    Schedule {
        #[command(subcommand)]
        action: cli::schedule::ScheduleAction,
    },

    /// Manage background jobs via daemon
    #[cfg(unix)]
    #[command(next_help_heading = "SYSTEM")]
    Job {
        #[command(subcommand)]
        action: cli::jobs::JobAction,
    },

    /// Manage background daemon (secrets, jobs, cache)
    #[cfg(unix)]
    #[command(next_help_heading = "SYSTEM")]
    Daemon {
        #[command(subcommand)]
        action: cli::daemon::DaemonAction,
    },

    /// Start HTTP API server for remote workflow execution
    ///
    /// Exposes a REST API for submitting, monitoring, and cancelling
    /// workflow jobs. Authentication via Bearer token.
    ///
    /// Examples:
    ///   NIKA_SERVE_TOKEN=secret nika serve
    ///   NIKA_SERVE_TOKEN=secret nika serve --bind 0.0.0.0:8080
    ///   NIKA_SERVE_TOKEN=secret nika serve --workflows ./pipelines --concurrency 4
    #[cfg(feature = "serve")]
    #[command(next_help_heading = "SYSTEM")]
    Serve {
        /// Socket address to bind (default: 0.0.0.0:3000, env: NIKA_SERVE_BIND)
        #[arg(short, long, value_name = "ADDR")]
        bind: Option<String>,

        /// Directory containing .nika.yaml workflows (env: NIKA_SERVE_WORKFLOWS)
        #[arg(short, long, value_name = "DIR")]
        workflows: Option<String>,

        /// Max concurrent workflow executions (env: NIKA_SERVE_MAX_CONCURRENT)
        #[arg(short, long, default_value = "6")]
        concurrency: usize,

        /// Per-job timeout in seconds (env: NIKA_SERVE_TIMEOUT)
        #[arg(short, long, default_value = "300")]
        timeout: u64,

        /// SQLite database path for job persistence (env: NIKA_SERVE_DB)
        #[arg(long, value_name = "PATH")]
        db: Option<String>,
    },

    /// Configure API keys and providers (interactive first-run wizard)
    #[command(next_help_heading = "SYSTEM")]
    Setup,

    /// Show version, build info, and channel
    #[command(next_help_heading = "SYSTEM")]
    Version,

    /// The cosmos awaits
    #[command(hide = true)]
    Cosmic,

    /// Show environment: version, providers, MCP, paths, config
    #[command(next_help_heading = "SYSTEM")]
    Env,

    /// Visualize workflow DAG (shortcut for `nika workflow graph`)
    #[command(next_help_heading = "WORKFLOWS")]
    Graph {
        /// Path to .nika.yaml file
        file: String,

        /// Output format: ascii, dot, mermaid
        #[arg(long, default_value = "ascii")]
        format: String,
    },

    /// Start Language Server Protocol server
    ///
    /// Provides IDE integration for .nika.yaml workflow files:
    /// - Diagnostics (syntax errors, validation errors)
    /// - Completions (verbs, fields, task references)
    /// - Hover documentation
    /// - Go to definition
    /// - Code actions (quick fixes)
    #[cfg(feature = "lsp")]
    #[command(hide = true)]
    Lsp {
        /// Communication mode: stdio (default) or tcp
        #[arg(long, default_value = "stdio")]
        mode: String,

        /// TCP port (only used with --mode tcp)
        #[arg(long, default_value = "9257")]
        port: u16,
    },
}

// ═══════════════════════════════════════════════════════════════════════════
// ═══════════════════════════════════════════════════════════════════════════
// FEATURES
// ═══════════════════════════════════════════════════════════════════════════

async fn print_env_info(_quiet: bool) {
    use colored::Colorize;

    println!("{}", "Nika Environment".magenta().bold());
    println!();

    // Version
    println!("  {} nika {}", "Version:".cyan(), long_version());

    // Channel
    println!("  {} {}", "Channel:".cyan(), env!("NIKA_BUILD_CHANNEL"));

    // Paths
    println!();
    println!(
        "  {} {}",
        "CWD:".cyan(),
        std::env::current_dir().unwrap_or_default().display()
    );
    if let Ok(project) =
        cli::config::find_project_root_from(&std::env::current_dir().unwrap_or_default())
    {
        println!("  {} {}", "Project:".cyan(), project.root.display());
    }
    println!(
        "  {} {}",
        "Home:".cyan(),
        std::env::var("HOME")
            .map(|h| format!("{}/.nika", h))
            .unwrap_or_else(|_| "unknown".to_string())
    );

    // API key status
    println!();
    println!("  {}", "Providers:".cyan());
    let providers = [
        ("ANTHROPIC_API_KEY", "anthropic"),
        ("OPENAI_API_KEY", "openai"),
        ("MISTRAL_API_KEY", "mistral"),
        ("GROQ_API_KEY", "groq"),
        ("DEEPSEEK_API_KEY", "deepseek"),
        ("GEMINI_API_KEY", "gemini"),
        ("XAI_API_KEY", "xai"),
    ];
    for (env_var, name) in &providers {
        let status = if std::env::var(env_var).is_ok() {
            "configured".green().to_string()
        } else {
            "not set".dimmed().to_string()
        };
        println!("    {} {}", format!("{name}:").bold(), status);
    }

    // Features summary
    println!();
    println!("  {} 64 transforms, 63 tools", "Engine:".cyan());
}

fn print_features() {
    use colored::Colorize;

    println!(
        "{}",
        format!("Nika v{} -- Compiled Features", env!("CARGO_PKG_VERSION")).bold()
    );
    println!();

    // Core features
    println!("{}", "Core".bold().underline());
    print_feature(
        "native-inference",
        cfg!(feature = "native-inference"),
        "Local GGUF models (mistral.rs)",
    );
    print_feature("lsp", cfg!(feature = "lsp"), "Language Server Protocol");
    println!();

    // Media Tier 2
    println!("{}", "Media (Tier 2 -- media-core)".bold().underline());
    print_feature(
        "media-thumbnail",
        cfg!(feature = "media-thumbnail"),
        "SIMD image resize",
    );
    print_feature(
        "media-metadata",
        cfg!(feature = "media-metadata"),
        "EXIF/audio metadata",
    );
    print_feature(
        "media-optimize",
        cfg!(feature = "media-optimize"),
        "Lossless PNG optimization",
    );
    print_feature(
        "media-svg",
        cfg!(feature = "media-svg"),
        "SVG to PNG rasterization",
    );
    println!();

    // Media Tier 3
    println!("{}", "Media (Tier 3 -- opt-in)".bold().underline());
    print_feature(
        "media-phash",
        cfg!(feature = "media-phash"),
        "Perceptual image hashing",
    );
    print_feature(
        "media-pdf",
        cfg!(feature = "media-pdf"),
        "PDF text extraction",
    );
    print_feature(
        "media-chart",
        cfg!(feature = "media-chart"),
        "Chart generation (bar/line/pie)",
    );
    print_feature(
        "media-provenance",
        cfg!(feature = "media-provenance"),
        "C2PA content credentials",
    );
    print_feature("media-qr", cfg!(feature = "media-qr"), "QR code validation");
    print_feature(
        "media-iqa",
        cfg!(feature = "media-iqa"),
        "Image quality assessment (DSSIM)",
    );
    print_feature(
        "media-compression",
        cfg!(feature = "media-compression"),
        "Zstd CAS compression",
    );
    println!();

    // Fetch extraction
    println!("{}", "Fetch Extraction".bold().underline());
    print_feature(
        "fetch-html",
        cfg!(feature = "fetch-html"),
        "CSS selectors + metadata + links",
    );
    print_feature(
        "fetch-markdown",
        cfg!(feature = "fetch-markdown"),
        "HTML to Markdown",
    );
    print_feature(
        "fetch-article",
        cfg!(feature = "fetch-article"),
        "Readability extraction",
    );
    print_feature(
        "fetch-feed",
        cfg!(feature = "fetch-feed"),
        "RSS/Atom/JSON Feed",
    );
    println!();

    // Summary
    let total = count_features();
    println!("{}", format!("{total}/22 features enabled").bold());
    println!(
        "{}",
        "Run `nika media tools` for detailed tool status".dimmed()
    );
}

fn print_feature(name: &str, enabled: bool, desc: &str) {
    use colored::Colorize;
    use nika::display::StatusIcon;
    if enabled {
        println!("  {} {:20} {}", StatusIcon::Ok, name, desc);
    } else {
        println!(
            "  {} {:20} {} {}",
            StatusIcon::Fail,
            name,
            desc.dimmed(),
            "(cargo install nika --features ...)".dimmed()
        );
    }
}

fn count_features() -> usize {
    let mut count = 0;
    if cfg!(feature = "native-inference") {
        count += 1;
    }
    if cfg!(feature = "lsp") {
        count += 1;
    }
    if cfg!(feature = "media-thumbnail") {
        count += 1;
    }
    if cfg!(feature = "media-metadata") {
        count += 1;
    }
    if cfg!(feature = "media-optimize") {
        count += 1;
    }
    if cfg!(feature = "media-svg") {
        count += 1;
    }
    if cfg!(feature = "media-phash") {
        count += 1;
    }
    if cfg!(feature = "media-pdf") {
        count += 1;
    }
    if cfg!(feature = "media-chart") {
        count += 1;
    }
    if cfg!(feature = "media-provenance") {
        count += 1;
    }
    if cfg!(feature = "media-qr") {
        count += 1;
    }
    if cfg!(feature = "media-iqa") {
        count += 1;
    }
    if cfg!(feature = "media-compression") {
        count += 1;
    }
    if cfg!(feature = "fetch-html") {
        count += 1;
    }
    if cfg!(feature = "fetch-markdown") {
        count += 1;
    }
    if cfg!(feature = "fetch-article") {
        count += 1;
    }
    if cfg!(feature = "fetch-feed") {
        count += 1;
    }
    count
}

// ═══════════════════════════════════════════════════════════════════════════
// MAIN
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::main]
async fn main() {
    // Load .env file (ignore if not present)
    let _ = dotenvy::dotenv();

    let cli = Cli::parse();

    // Apply color settings
    match cli.color {
        ColorChoice::Always => colored::control::set_override(true),
        ColorChoice::Never => colored::control::set_override(false),
        ColorChoice::Auto => {} // Use default detection
    }

    // Initialize tracing with verbosity level
    if !cli.quiet {
        let level = match cli.verbose {
            0 => tracing::Level::WARN,  // Default: warnings only
            1 => tracing::Level::INFO,  // -v: info
            2 => tracing::Level::DEBUG, // -vv: debug
            _ => tracing::Level::TRACE, // -vvv: trace
        };

        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::from_default_env().add_directive(level.into()),
            )
            .init();
    }

    // Handle positional file argument first (nika workflow.nika.yaml)
    if let Some(ref file) = cli.file {
        if cli.command.is_some() {
            eprintln!(
                "{} Cannot use both positional file and subcommand",
                "Error:".red().bold()
            );
            std::process::exit(1);
        }

        // Check if it's a .nika.yaml file
        if is_nika_workflow(file) {
            let result = run_workflow(
                &file.display().to_string(),
                None,
                None,
                &[],
                None,
                true,
                None,
                None,
                None,
                false,
                cli.quiet,
                cli.detail,
                cli.no_live,
                "accept-edits",
                false,
            )
            .await;
            handle_result(result).await;
            return;
        } else {
            eprintln!(
                "{} Expected .nika.yaml file, got: {}",
                "Error:".red().bold(),
                file.display()
            );
            eprintln!("  {} Use: nika run {}", "Hint:".yellow(), file.display());
            std::process::exit(1);
        }
    }

    // Extract global flags for use in handlers
    let quiet = cli.quiet;
    let detail = cli.detail;

    // Auto-setup: run machine setup on first non-skipped command (not CI).
    // Returns true if setup just ran (so we skip the redundant quick scan).
    let setup_just_ran = maybe_run_auto_setup(&cli.command, quiet);

    // Fast path: silently update AI rules on ANY command after version change.
    // Pure filesystem I/O (<0.5ms when version matches), no subprocess overhead.
    // Ensures `nika run` after `brew upgrade` gets fresh rules immediately.
    if !setup_just_ran && !cli::machine::is_ci() {
        cli::machine::fast_rule_update();
    }

    // Quick editor scan: detect newly installed editors and install rules.
    // Only runs when machine is already set up (adds ~5ms).
    // Skip if auto-setup just ran — it already scanned all editors.
    if !setup_just_ran && cli::machine::machine_setup_status() == cli::machine::MachineStatus::Ready
    {
        cli::machine::quick_editor_scan();
    }

    // Handle subcommands or default to help (terminal-first)
    let result = match cli.command {
        None => {
            use cli::machine::MachineStatus;
            // Auto-setup editors on first run
            match cli::machine::machine_setup_status() {
                MachineStatus::NeverSetup | MachineStatus::NeedsUpdate => {
                    if !quiet {
                        cli::machine::run_machine_setup();
                    }
                }
                MachineStatus::Ready => {}
            }

            // Smart welcome: adapt based on project + setup state
            let has_project = Path::new("nika.toml").exists() || Path::new(".nika").exists();
            let has_providers = !cli::onboarding::skip_onboarding();

            if has_project {
                // Mode 3: In a project — show contextual status
                let project_root = cli::config::find_project_root_from(
                    &std::env::current_dir().unwrap_or_default(),
                )
                .ok();
                let version = env!("CARGO_PKG_VERSION");
                let cwd = std::env::current_dir()
                    .ok()
                    .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                    .unwrap_or_default();

                println!();
                println!(
                    "  {} v{}{}",
                    "N I K A".bold(),
                    version,
                    if cwd.is_empty() {
                        String::new()
                    } else {
                        format!("  {}", cwd.dimmed())
                    }
                );
                println!();

                // Show provider status
                if has_providers {
                    use nika::core::{ProviderCategory, KNOWN_PROVIDERS};
                    let configured: Vec<&str> = KNOWN_PROVIDERS
                        .iter()
                        .filter(|p| p.category == ProviderCategory::Llm)
                        .filter(|p| {
                            std::env::var(p.env_var)
                                .map(|v| !v.is_empty())
                                .unwrap_or(false)
                        })
                        .map(|p| p.id)
                        .collect();
                    if !configured.is_empty() {
                        println!("  Provider:   {}", configured.join(", ").cyan());
                    }
                }

                // Count workflows
                let workflow_count = count_nika_workflows(Path::new("."));
                if workflow_count > 0 {
                    println!(
                        "  Workflows:  {} file(s)",
                        workflow_count.to_string().cyan()
                    );
                }

                if let Some(ref proj) = project_root {
                    let source = match proj.source {
                        cli::config::ProjectRootSource::NikaToml => "nika.toml",
                        cli::config::ProjectRootSource::DotNika => ".nika/",
                        cli::config::ProjectRootSource::Fallback => "defaults",
                    };
                    println!("  Config:     {}", source.dimmed());
                }

                println!();
                println!("    {}   Execute a workflow", "nika run <file>".bold());
                println!("    {}           Open TUI", "nika ui".bold());
                println!("    {}        System health", "nika doctor".bold());
                println!();

                Ok(())
            } else if has_providers {
                // Mode 2: Setup done, no project
                let version = env!("CARGO_PKG_VERSION");
                println!();
                println!("  {} v{}", "N I K A".bold(), version);
                println!();
                println!("  Not in a project directory.");
                println!();
                println!(
                    "    {}         Initialize a project here",
                    "nika init".bold()
                );
                println!("    {}        Quick LLM call", "nika infer".bold());
                println!(
                    "    {}  Learn with 44 exercises",
                    "nika init --course".bold()
                );
                println!();
                Ok(())
            } else {
                // Mode 1: No setup, no project — run demo
                run_demo(quiet, detail).await
            }
        }

        Some(Commands::Run {
            file,
            provider,
            model,
            inputs,
            input_file,
            dry_run,
            output,
            no_interactive,
            task,
            from,
            yes,
            permission,
            resume,
        }) => {
            // Auto-discover workflow if no file specified
            let resolved_file = match file {
                Some(f) => f,
                None => match resolve_or_discover_workflow(quiet).await {
                    Ok(f) => f,
                    Err(e) => return handle_result(Err(e)).await,
                },
            };

            if dry_run {
                dry_run_workflow(
                    &resolved_file,
                    provider,
                    model,
                    &inputs,
                    input_file.as_deref(),
                    task.as_deref(),
                    from.as_deref(),
                )
                .await
            } else {
                run_workflow(
                    &resolved_file,
                    provider,
                    model,
                    &inputs,
                    input_file.as_deref(),
                    !no_interactive,
                    output.as_deref(),
                    task.as_deref(),
                    from.as_deref(),
                    yes,
                    quiet,
                    detail,
                    cli.no_live,
                    &permission,
                    resume,
                )
                .await
            }
        }

        Some(Commands::Infer {
            prompt,
            provider,
            model,
            system,
            temperature,
            max_tokens,
            json,
            from_example,
            stdin,
            no_interactive,
            quiet: verb_quiet,
        }) => {
            if no_interactive {
                cli::onboarding::set_no_onboarding();
            }
            cli::verbs::handle_infer(
                prompt,
                provider,
                model,
                system,
                temperature,
                max_tokens,
                json,
                from_example,
                stdin,
                quiet || verb_quiet,
            )
            .await
        }

        Some(Commands::Fetch {
            url,
            extract,
            selector,
            method,
            headers,
            body,
            json_body,
            response,
            timeout,
            no_interactive,
        }) => {
            if no_interactive {
                cli::onboarding::set_no_onboarding();
            }
            cli::verbs::handle_fetch(
                url, extract, selector, method, headers, body, json_body, response, timeout, quiet,
            )
            .await
        }

        Some(Commands::Invoke {
            tool,
            file,
            params,
            mcp,
            timeout,
            list,
            no_interactive,
        }) => {
            if no_interactive {
                cli::onboarding::set_no_onboarding();
            }
            cli::verbs::handle_invoke(tool, file, params, mcp, timeout, list, quiet).await
        }

        Some(Commands::Agent {
            prompt,
            list,
            provider,
            model,
            system,
            tools,
            mcp_servers,
            turns,
            max_tokens,
            temperature,
            stdin,
            no_interactive,
        }) => {
            if no_interactive {
                cli::onboarding::set_no_onboarding();
            }
            if list {
                print_agent_presets();
                Ok(())
            } else {
                let prompt = prompt.unwrap_or_else(|| {
                    eprintln!("Error: prompt is required (use --list to see presets)");
                    std::process::exit(1);
                });
                cli::verbs::handle_agent(
                    prompt,
                    provider,
                    model,
                    system,
                    tools,
                    mcp_servers,
                    turns,
                    max_tokens,
                    temperature,
                    stdin,
                    quiet,
                )
                .await
            }
        }

        Some(Commands::Check {
            file,
            strict,
            security,
        }) => {
            if strict {
                validate_workflow_strict(&file).await
            } else {
                validate_workflow(&file, quiet, security).await
            }
        }

        Some(Commands::Test {
            file,
            golden,
            update_snapshot,
            inputs,
        }) => {
            test_workflow(
                &file,
                golden.as_deref(),
                update_snapshot,
                &inputs,
                quiet,
                detail,
            )
            .await
        }

        Some(Commands::Lint { file }) => cli::lint::handle_lint_command(&file, quiet).await,

        Some(Commands::Eval {
            file,
            dataset,
            provider,
            format,
            fail_fast,
            parallel,
            yes,
        }) => {
            eval_workflow(
                &file,
                &dataset,
                provider.as_deref(),
                &format,
                fail_fast,
                parallel,
                yes,
                quiet,
                detail,
            )
            .await
        }

        Some(Commands::Explain { file }) => explain_workflow(&file).await,

        Some(Commands::Bench {
            file,
            providers,
            iterations,
            profile,
            json,
            eval,
            eval_model,
            yes,
        }) => {
            run_bench(
                &file,
                &providers,
                iterations,
                profile,
                json,
                eval,
                &eval_model,
                yes,
                quiet,
            )
            .await
        }

        Some(Commands::Init {
            permission,
            migrate_keys,
            course,
            yes,
        }) => {
            if course {
                cli::init::init_course()
            } else {
                let interactive = !yes && std::io::stdin().is_terminal();
                cli::init::init_project(&permission, migrate_keys, interactive).await
            }
        }

        Some(Commands::Course { action }) => cli::course::handle_course_command(action, quiet),

        Some(Commands::Trace { action }) => cli::trace::handle_trace_command(action, quiet),

        Some(Commands::Clean { dry_run, all }) => {
            let current = std::env::current_dir().map_err(NikaError::from);
            match current.and_then(|cwd| cli::config::find_project_root_from(&cwd)) {
                Ok(project) => {
                    let nika_dir = project.root.join(".nika");
                    if !nika_dir.exists() {
                        println!(
                            "{} No .nika/ directory found — nothing to clean",
                            nika_engine::display::StatusIcon::Info
                        );
                        Ok(())
                    } else {
                        let opts = cli::clean::CleanOptions {
                            dry_run,
                            all,
                            quiet,
                        };
                        let report = cli::clean::run_clean(&nika_dir, &opts);
                        match report {
                            Ok(r) => {
                                cli::clean::print_report(&r, quiet);
                                Ok(())
                            }
                            Err(e) => Err(e),
                        }
                    }
                }
                Err(e) => Err(e),
            }
        }

        Some(Commands::Keys {
            action,
            json,
            verbose,
        }) => cli::keys::handle_keys_command(action, json, verbose, quiet).await,

        Some(Commands::Provider { action }) => {
            cli::provider::handle_provider_command(action, quiet).await
        }

        Some(Commands::Token { action }) => cli::token::run(action).await,

        Some(Commands::Vault { .. }) => {
            eprintln!(
                "  {} Did you mean? {}",
                "\u{2717}".red().bold(),
                "nika keys".cyan()
            );
            eprintln!("  Vault commands moved to: {}", "nika keys".bold());
            std::process::exit(1);
        }

        Some(Commands::Tools { action }) => {
            cli::tools_cmd::handle_tools_command(action);
            Ok(())
        }

        Some(Commands::Setup) => cli::onboarding::handle_setup_command(quiet).await,

        Some(Commands::Version) => {
            println!("nika {}", long_version());
            Ok(())
        }

        Some(Commands::Env) => {
            print_env_info(quiet).await;
            Ok(())
        }

        Some(Commands::Graph { file, format }) => {
            cli::workflow::handle_workflow_command(
                cli::workflow::WorkflowAction::Graph {
                    file: PathBuf::from(&file),
                    format,
                    output: None,
                },
                quiet,
            )
            .await
        }

        Some(Commands::Cosmic) => {
            cli::help::print_cosmic();
            Ok(())
        }

        Some(Commands::Mcp { action }) => cli::mcp::handle_mcp_command(action, quiet).await,

        Some(Commands::Media { action }) => cli::media::handle_media_command(action, quiet).await,

        Some(Commands::Model { action }) => {
            cli::model_cmd::handle_model_command(action, quiet).await
        }

        Some(Commands::Help { topic }) => {
            match topic {
                Some(t) => {
                    if !cli::help::print_topic(&t) {
                        eprintln!(
                            "{} Unknown topic '{}'. Available: verbs, providers, templates, examples",
                            "Error:".red().bold(),
                            t
                        );
                        std::process::exit(1);
                    }
                }
                None => cli::help::print_help(&Cli::command()),
            }
            Ok(())
        }

        Some(Commands::Completion { shell }) => {
            clap_complete::generate(shell, &mut Cli::command(), "nika", &mut std::io::stdout());
            Ok(())
        }

        Some(Commands::Config { action }) => cli::config::handle_config_command(action, quiet),

        Some(Commands::Schema { action }) => cli::schema::handle_schema_command(action, quiet),

        Some(Commands::Features) => {
            print_features();
            Ok(())
        }

        Some(Commands::Showcase { action }) => {
            cli::showcase::handle_showcase_command(action, quiet)
        }

        Some(Commands::Switch {
            action,
            setup,
            build,
        }) => {
            if setup {
                cli::switch::do_setup(quiet).await
            } else if build {
                cli::switch::do_build(quiet).await
            } else {
                cli::switch::handle_switch_command(action, quiet)
            }
        }

        Some(Commands::Doctor { full, format, fix }) => {
            cli::doctor::handle_doctor_command(full, &format, quiet, fix).await
        }

        #[cfg(unix)]
        Some(Commands::Daemon { action }) => {
            cli::daemon::handle_daemon_command(action, quiet).await
        }

        #[cfg(feature = "serve")]
        Some(Commands::Serve {
            bind,
            workflows,
            concurrency,
            timeout,
            db,
        }) => {
            // Build ServeConfig directly from CLI args + env (no deprecated set_var)
            let mk = |reason: String| nika::NikaError::ConfigError { reason };
            match (|| -> Result<nika_serve::config::ServeConfig, nika::NikaError> {
                let bind_addr = bind
                    .or_else(|| std::env::var("NIKA_SERVE_BIND").ok())
                    .unwrap_or_else(|| "0.0.0.0:3000".into())
                    .parse()
                    .map_err(|e| mk(format!("invalid bind address: {e}")))?;
                let auth_token = std::env::var("NIKA_SERVE_TOKEN")
                    .map_err(|_| mk("NIKA_SERVE_TOKEN env var must be set".into()))?;
                Ok(nika_serve::config::ServeConfig {
                    bind: bind_addr,
                    workflows_dir: workflows
                        .or_else(|| std::env::var("NIKA_SERVE_WORKFLOWS").ok())
                        .unwrap_or_else(|| "./workflows".into())
                        .into(),
                    max_concurrent: concurrency,
                    job_timeout_secs: timeout,
                    max_output_bytes: 1024 * 1024,
                    db_path: db
                        .or_else(|| std::env::var("NIKA_SERVE_DB").ok())
                        .unwrap_or_else(|| ".nika/serve.db".into())
                        .into(),
                    storage_url: std::env::var("NIKA_STORAGE_URL").ok(),
                    auth_token,
                    cors_origin: std::env::var("NIKA_SERVE_CORS_ORIGIN").ok(),
                    executor_mode: match std::env::var("NIKA_SERVE_EXECUTOR")
                        .as_deref()
                        .unwrap_or("subprocess")
                    {
                        "embedded" => nika_serve::config::ExecutorMode::Embedded,
                        _ => nika_serve::config::ExecutorMode::Subprocess,
                    },
                    rate_per_second: std::env::var("NIKA_SERVE_RATE_LIMIT")
                        .ok()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(10),
                    rate_burst: std::env::var("NIKA_SERVE_RATE_BURST")
                        .ok()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(30),
                    gc_retention_secs: std::env::var("NIKA_SERVE_GC_RETENTION")
                        .ok()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(7 * 24 * 3600),
                    gc_interval_secs: std::env::var("NIKA_SERVE_GC_INTERVAL")
                        .ok()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(3600),
                    project_root: cli::config::find_project_root_from(
                        &std::env::current_dir().unwrap_or_default(),
                    )
                    .ok()
                    .map(|p| p.root),
                    working_dir_mode: cli::config::find_project_root_from(
                        &std::env::current_dir().unwrap_or_default(),
                    )
                    .ok()
                    .and_then(|p| cli::config::load_project_config(&p.root))
                    .and_then(|c| c.tools.working_dir),
                })
            })() {
                Ok(config) => nika_serve::run_server(config)
                    .await
                    .map_err(|e| mk(format!("serve error: {e}"))),
                Err(e) => Err(e),
            }
        }

        #[cfg(unix)]
        Some(Commands::Every(args)) => cli::every::handle_every_command(args, quiet).await,

        #[cfg(unix)]
        Some(Commands::Schedule { action }) => {
            cli::schedule::handle_schedule_command(action, quiet).await
        }

        #[cfg(unix)]
        Some(Commands::Job { action }) => cli::jobs::handle_job_command(action, quiet).await,

        #[cfg(unix)]
        Some(Commands::Cache { action }) => {
            cli::cache_cmd::handle_cache_command(action, quiet).await
        }

        Some(Commands::New {
            name,
            verb,
            provider,
            output_dir,
        }) => cli::new_cmd::handle_new_command(name, verb, provider, output_dir, quiet),

        Some(Commands::Workflow { action }) => {
            cli::workflow::handle_workflow_command(action, quiet).await
        }

        #[cfg(feature = "lsp")]
        Some(Commands::Lsp { mode, port }) => {
            if mode == "stdio" {
                nika::lsp::run_stdio()
                    .await
                    .map_err(|e| nika::NikaError::ConfigError {
                        reason: format!("LSP server error: {}", e),
                    })
            } else if mode == "tcp" {
                Err(nika::NikaError::ConfigError {
                    reason: format!("TCP mode not yet implemented (port: {})", port),
                })
            } else {
                Err(nika::NikaError::ConfigError {
                    reason: format!("Unknown LSP mode: {}. Use 'stdio' or 'tcp'.", mode),
                })
            }
        }
    };

    handle_result(result).await;
}

// ═══════════════════════════════════════════════════════════════════════════
// HELPERS
// ═══════════════════════════════════════════════════════════════════════════

fn should_skip_auto_setup(cmd: &Option<Commands>) -> bool {
    // Whitelist: only these commands trigger auto-setup.
    // Everything else is headless, scriptable, or machine-internal.
    // If you add a new command, it will NOT trigger setup by default — safe.
    match cmd {
        // Bare `nika` (no subcommand) — interactive entry point.
        None => false,
        // Doctor: checks health, auto-setup runs first so it can report accurately.
        Some(Commands::Doctor { .. }) => false,
        // Init: project setup — auto-setup ensures machine is ready first.
        Some(Commands::Init { .. }) => false,
        // Setup: explicit setup wizard — must trigger machine setup.
        Some(Commands::Setup) => false,
        // Version: just print info, no setup needed.
        Some(Commands::Version) => false,
        // Test: runs with mock provider, no setup needed.
        Some(Commands::Test { .. }) => false,
        // Env: just print info, no setup needed.
        Some(Commands::Env) => false,
        // Graph: just print DAG, no setup needed.
        Some(Commands::Graph { .. }) => false,
        // New: creating a workflow — user-facing, benefits from setup.
        Some(Commands::New { .. }) => false,
        // Daemon: `nika daemon start` is the post-install entry point
        // from install.sh and must trigger first-run setup silently.
        #[cfg(unix)]
        Some(Commands::Daemon { .. }) => false,
        // Everything else: headless, non-interactive, scriptable, or machine-internal.
        // Run, Check, Infer, Fetch, Invoke, Agent, Bench, Serve, Lint, Eval, Lsp,
        // Completion, Provider, Model, Config, Trace, Workflow, Pkg, Media, etc.
        _ => true,
    }
}

/// Returns true if auto-setup actually ran (callers can skip redundant work).
fn maybe_run_auto_setup(cmd: &Option<Commands>, quiet: bool) -> bool {
    if should_skip_auto_setup(cmd) {
        return false;
    }
    if cli::machine::is_ci() {
        return false;
    }
    if cli::machine::is_machine_setup() {
        return false;
    }
    if !quiet {
        println!("  {} Setting up Nika for your editors...\n", "◇".cyan());
    }
    cli::machine::run_machine_setup();
    if !quiet {
        println!();
    }
    true
}

/// Handle result from any command
async fn handle_result(result: Result<(), NikaError>) {
    if let Err(e) = result {
        // If MissingApiKey and TTY, offer onboarding wizard
        if matches!(e, NikaError::MissingApiKey { .. })
            && std::io::stdin().is_terminal()
            && !cli::onboarding::skip_onboarding()
            && !cli::onboarding::has_any_provider_key()
        {
            if let Ok(true) = cli::onboarding::run_onboarding_wizard().await {
                // Key was configured — user should re-run their command
                eprintln!();
                eprintln!("  Re-run your command to use the new API key.");
                std::process::exit(0);
            }
        }
        let report = miette::Report::new(e);
        eprintln!("{report:?}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════════════════════
    // CLI arg validation
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn cli_args_no_short_option_conflicts() {
        // Verify all clap subcommands parse without short-option conflicts
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
