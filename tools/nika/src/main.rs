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

use nika::ast::output::SchemaRef;
use nika::ast::schema_validator::WorkflowSchemaValidator;
use nika::ast::{
    parse_analyzed, parse_analyzed_with_includes, parse_workflow_with_includes, TaskAction,
};
use nika::dag::{validate_bindings, Dag};
use nika::error::NikaError;
use nika::mcp::validation::{McpValidator, ValidationConfig};
use nika::mcp::{McpClient, McpConfig};
use nika::registry::resolver;
use nika::runtime::Runner;

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
    /// Launch interactive TUI (terminal UI)
    #[cfg(feature = "tui")]
    #[command(next_help_heading = "INTERACTIVE")]
    Ui {
        /// Initial view: explorer, chat, editor, runner, scheduler, settings
        #[arg(long, value_name = "VIEW")]
        view: Option<String>,

        /// Workflow file to load (optional)
        #[arg(value_name = "WORKFLOW")]
        workflow: Option<PathBuf>,
    },

    /// Start interactive chat mode (shortcut for `nika ui --view chat`)
    #[cfg(feature = "tui")]
    #[command(next_help_heading = "INTERACTIVE", visible_alias = "c")]
    Chat {
        /// LLM provider: anthropic, openai, mistral, groq, deepseek, gemini, xai, native
        #[arg(short, long, value_name = "NAME")]
        provider: Option<String>,

        /// Model name (provider-specific)
        #[arg(short, long, value_name = "MODEL")]
        model: Option<String>,
    },

    /// Open Studio editor (shortcut for `nika ui --view editor`)
    #[cfg(feature = "tui")]
    #[command(next_help_heading = "INTERACTIVE", visible_alias = "s")]
    Studio {
        /// Workflow file to edit (optional)
        workflow: Option<PathBuf>,
    },

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

    /// Manage installed packages (workflows, skills, schemas)
    ///
    /// List, add, remove, and install packages from the SuperNovae registry.
    /// Packages are stored in ~/.nika/packages/
    #[command(next_help_heading = "PROJECT", visible_alias = "p")]
    Pkg {
        #[command(subcommand)]
        action: cli::pkg::PkgAction,
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
    print_feature("tui", cfg!(feature = "tui"), "Terminal UI (ratatui)");
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
    if cfg!(feature = "tui") {
        count += 1;
    }
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

    // Determine if we're running TUI (skip tracing to avoid terminal pollution)
    let is_tui = is_tui_mode(&cli);

    // Initialize tracing with verbosity level
    if !is_tui && !cli.quiet {
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

        #[cfg(feature = "tui")]
        Some(Commands::Ui { view, workflow }) => {
            use nika::tui::TuiView;
            let initial_view = match view.as_deref() {
                Some("chat" | "c") => Some(TuiView::Command),
                Some("studio" | "editor" | "d" | "explorer" | "e" | "home") => {
                    Some(TuiView::Studio)
                }
                Some("runner" | "r" | "monitor") => Some(TuiView::Command),
                Some("settings" | ",") => Some(TuiView::Control),
                Some(unknown) => {
                    eprintln!(
                        "{} Unknown view '{}'. Valid: studio, chat, runner, settings",
                        "Error:".red().bold(),
                        unknown
                    );
                    std::process::exit(1);
                }
                None => None,
            };
            nika::tui::run_tui_with_options(workflow, initial_view).await
        }

        #[cfg(feature = "tui")]
        Some(Commands::Chat { provider, model }) => nika::tui::run_tui_chat(provider, model).await,

        #[cfg(feature = "tui")]
        Some(Commands::Studio { workflow }) => nika::tui::run_tui_studio(workflow).await,

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

        Some(Commands::Pkg { action }) => cli::pkg::handle_pkg_command(action, quiet).await,

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

/// Check if we're running in TUI mode (skip tracing to avoid terminal pollution)
fn is_tui_mode(cli: &Cli) -> bool {
    if cli.command.is_none() && cli.file.is_none() {
        return false;
    }

    #[cfg(feature = "tui")]
    if let Some(ref cmd) = cli.command {
        return matches!(
            cmd,
            Commands::Ui { .. } | Commands::Chat { .. } | Commands::Studio { .. }
        );
    }

    false
}

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

/// Check if a file is a Nika workflow (.nika.yaml)
fn is_nika_workflow(file: &Path) -> bool {
    let filename = file
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    filename.ends_with(".nika.yaml") || filename.ends_with(".nika.yml")
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

// ═══════════════════════════════════════════════════════════════════════════
// WORKFLOW COMMANDS
// ═══════════════════════════════════════════════════════════════════════════

/// Download a remote `.nika.yaml` workflow to `.nika/remote/` and return its path.
async fn download_remote_workflow(url: &str) -> Result<PathBuf, NikaError> {
    if !url.ends_with(".nika.yaml") && !url.ends_with(".yaml") {
        return Err(NikaError::WorkflowNotFound {
            path: format!("Remote URL must end with .nika.yaml or .yaml: {url}"),
        });
    }

    let filename = url
        .rsplit('/')
        .next()
        .unwrap_or("remote-workflow.nika.yaml");

    let remote_dir = PathBuf::from(".nika").join("remote");
    tokio::fs::create_dir_all(&remote_dir).await.map_err(|e| {
        NikaError::IoError(std::io::Error::other(format!(
            "Cannot create .nika/remote/: {e}"
        )))
    })?;

    let dest = remote_dir.join(filename);

    eprintln!("  {} Downloading {}", "→".cyan(), url.dimmed());

    let output = tokio::process::Command::new("curl")
        .args(["-fsSL", "--max-time", "30", "-o"])
        .arg(&dest)
        .arg(url)
        .output()
        .await
        .map_err(|e| {
            NikaError::IoError(std::io::Error::other(format!(
                "curl not found or failed: {e}"
            )))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(NikaError::WorkflowNotFound {
            path: format!("Failed to download {url}: {stderr}"),
        });
    }

    let content = tokio::fs::read_to_string(&dest).await.map_err(|e| {
        NikaError::IoError(std::io::Error::other(format!(
            "Cannot read downloaded file: {e}"
        )))
    })?;

    if !content.contains("schema:") && !content.contains("tasks:") {
        let _ = tokio::fs::remove_file(&dest).await;
        return Err(NikaError::WorkflowNotFound {
            path: format!("Downloaded file from {url} does not appear to be a Nika workflow"),
        });
    }

    eprintln!("  {} Downloaded to {}", "✓".green(), dest.display());
    Ok(dest)
}

/// Resolve a workflow reference to an actual file path.
///
/// Resolution order:
/// 0. URL (http/https) -> Download to `.nika/remote/`
/// 1. Package reference (`@name`) -> `~/.nika/packages/`
/// 2. Simple name -> `.nika/workflows/{name}.nika.yaml`
/// 3. Filesystem path -> as-is
async fn resolve_workflow_path(reference: &str) -> Result<PathBuf, NikaError> {
    // 0. Remote URL — download to temp and execute
    if reference.starts_with("http://") || reference.starts_with("https://") {
        return download_remote_workflow(reference).await;
    }

    // 1. Package reference (starts with @)
    if reference.starts_with('@') {
        let resolved =
            resolver::resolve_package_path(reference).map_err(|e| NikaError::WorkflowNotFound {
                path: format!(
                    "Package not found: {reference}. Error: {e}. Try: nika pkg add {reference}"
                ),
            })?;

        let workflow_path = resolved.path.join("workflow.nika.yaml");
        if !workflow_path.exists() {
            return Err(NikaError::WorkflowNotFound {
                path: format!(
                    "Package {} exists but missing workflow.nika.yaml at {}",
                    reference,
                    workflow_path.display()
                ),
            });
        }

        return Ok(workflow_path);
    }

    // 2. Simple name without path separator or .yaml extension -> try .nika/workflows/
    if !reference.contains('/')
        && !reference.ends_with(".nika.yaml")
        && !reference.ends_with(".yaml")
    {
        let local_path = PathBuf::from(".nika")
            .join("workflows")
            .join(format!("{reference}.nika.yaml"));

        if local_path.exists() {
            return Ok(local_path);
        }

        if !PathBuf::from(reference).exists() {
            return Err(NikaError::WorkflowNotFound {
                path: format!("Workflow '{reference}' not found in .nika/workflows/ or current directory. Try: nika pkg search {reference}")
            });
        }
    }

    // 3. Direct filesystem path
    let path = PathBuf::from(reference);
    if !path.exists() {
        return Err(NikaError::WorkflowNotFound {
            path: format!(
                "File not found: {reference}. Check the path or try: nika pkg search {reference}"
            ),
        });
    }

    Ok(path)
}

/// Run a built-in 8-task DAG demo — Nika's manifesto as a workflow.
/// Count *.nika.yaml files recursively, skipping hidden dirs and common junk.
fn count_nika_workflows(dir: &Path) -> usize {
    fn walk(dir: &Path, count: &mut usize) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if path.is_dir() {
                // Skip hidden, node_modules, target
                if name_str.starts_with('.') || name_str == "node_modules" || name_str == "target" {
                    continue;
                }
                walk(&path, count);
            } else if name_str.ends_with(".nika.yaml") {
                *count += 1;
            }
        }
    }
    let mut count = 0;
    walk(dir, &mut count);
    count
}

///
/// Diamond pattern: start → [write, connect, track] → [build, run] → launch → celebrate
/// 8 tasks, 4 layers, fan-out + fan-in, no API key needed.
async fn run_demo(quiet: bool, detail: nika::display::DetailLevel) -> Result<(), NikaError> {
    const DEMO_YAML: &str = r#"schema: "nika/workflow@0.12"
workflow: hello-nika
description: "Welcome to Nika — this is running live right now"

tasks:
  - id: start
    exec: "echo 'Hey! This is Nika — a real DAG running live.'"

  - id: write
    depends_on: [start]
    exec: "echo 'Write YAML. Nika resolves deps and runs it.'"

  - id: connect
    depends_on: [start]
    exec: "echo '7 providers: Claude, GPT, Gemini, Mistral, Groq, xAI, local.'"

  - id: track
    depends_on: [start]
    exec: "echo 'Every token counted. Every cent tracked.'"

  - id: build
    depends_on: [write, connect]
    exec: "echo 'DAG, parallel exec, MCP tools, media pipeline.'"

  - id: run
    depends_on: [connect, track]
    exec: "echo 'Headless CLI, TUI, or embed as a library.'"

  - id: launch
    depends_on: [build, run]
    exec: "echo 'One file. Any AI. Ship it.'"

  - id: celebrate
    depends_on: [launch]
    exec: "echo 'Welcome aboard, captain.'"
"#;

    println!();
    println!(
        "  \u{1f98b} {}  {}",
        format!("nika v{}", env!("CARGO_PKG_VERSION")).bold(),
        "live demo".dimmed()
    );
    println!();
    println!("  {}", "This is a real workflow running live.".dimmed());
    println!(
        "  {}",
        "No API key. No setup. Just a YAML file and a DAG.".dimmed()
    );

    // Show the DAG visualization before running
    {
        use nika::display::{render_dag, DagTask, DagTaskStatus};
        use std::collections::HashMap;

        let names = [
            "start",
            "write",
            "connect",
            "track",
            "build",
            "run",
            "launch",
            "celebrate",
        ];
        let dag_tasks: Vec<DagTask> = names
            .iter()
            .map(|id| DagTask {
                id: (*id).into(),
                verb: "exec".into(),
                status: DagTaskStatus::Pending,
                meta: None,
                tags: vec![],
            })
            .collect();

        let mut deps: HashMap<String, Vec<String>> = HashMap::new();
        deps.insert("write".into(), vec!["start".into()]);
        deps.insert("connect".into(), vec!["start".into()]);
        deps.insert("track".into(), vec!["start".into()]);
        deps.insert("build".into(), vec!["write".into(), "connect".into()]);
        deps.insert("run".into(), vec!["connect".into(), "track".into()]);
        deps.insert("launch".into(), vec!["build".into(), "run".into()]);
        deps.insert("celebrate".into(), vec!["launch".into()]);

        render_dag(&dag_tasks, &deps);
    }

    // Write temp file, run, clean up
    let tmp = std::env::temp_dir().join("nika-demo.nika.yaml");
    tokio::fs::write(&tmp, DEMO_YAML).await?;

    let result = run_workflow(
        &tmp.display().to_string(),
        None,
        None,
        &[],
        None,
        false,
        None,
        None,
        None,
        true, // skip cost confirm for demo
        quiet,
        detail,
        false, // demo always uses live renderer
        "deny",
        false,
    )
    .await;

    let _ = tokio::fs::remove_file(&tmp).await;

    result?;

    println!();
    println!(
        "  {} {}",
        "Next:".cyan().bold(),
        "nika new hello --verb exec".bold(),
    );
    println!(
        "  {}",
        "Create your first workflow. It's just a YAML file.".dimmed()
    );
    println!();

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// AGENT PRESETS
// ═══════════════════════════════════════════════════════════════════════════

fn print_agent_presets() {
    use nika::runtime::resolver::default_presets;

    let presets = default_presets();
    let mut names: Vec<&String> = presets.keys().collect();
    names.sort();

    println!("\n {} {}\n", "⟡".cyan(), "Built-in Agent Presets".bold());
    println!(
        "  {:<12} {:<28} {:<6} {}",
        "NAME".dimmed(),
        "MODEL".dimmed(),
        "TEMP".dimmed(),
        "DESCRIPTION".dimmed(),
    );
    println!("  {}", "─".repeat(80).dimmed());

    for name in &names {
        let agent = &presets[*name];
        let model = agent.model.as_deref().unwrap_or("default");
        let temp = agent
            .temperature
            .map(|t| format!("{:.1}", t))
            .unwrap_or_else(|| "—".to_string());
        // First sentence of system prompt as description
        let desc = agent.system.split('.').next().unwrap_or(&agent.system);
        println!(
            "  {:<12} {:<28} {:<6} {}",
            name.cyan(),
            model.dimmed(),
            temp,
            desc,
        );
    }

    println!(
        "\n  {} Use with: {} or {}",
        "→".dimmed(),
        "agent: <name>".cyan(),
        "preset: <name>".cyan(),
    );
    println!(
        "  {} Override in workflow {} block\n",
        "→".dimmed(),
        "agents:".cyan(),
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// BENCH
// ═══════════════════════════════════════════════════════════════════════════

#[allow(clippy::too_many_arguments)]
async fn run_bench(
    file: &str,
    providers: &[String],
    iterations: usize,
    show_profile: bool,
    json_output: bool,
    eval: bool,
    eval_model: &str,
    _skip_confirm: bool,
    quiet: bool,
) -> Result<(), NikaError> {
    use nika::display::bench_cache;
    use nika::display::renderer::RunStats;
    use nika::event::EventLog;
    use std::time::Instant;

    if iterations == 0 {
        return Err(NikaError::ValidationError {
            reason: "Iterations must be >= 1".to_string(),
        });
    }
    if providers.is_empty() {
        return Err(NikaError::ValidationError {
            reason: "At least one provider is required (--providers)".to_string(),
        });
    }

    // Bootstrap secrets
    let _ = nika::secrets::load_from_daemon_or_fallback().await;

    let resolved_path = resolve_workflow_path(file).await?;
    let yaml = tokio::fs::read_to_string(&resolved_path).await?;

    let validator = WorkflowSchemaValidator::new()?;
    validator.validate_yaml(&yaml)?;

    let base_path = resolved_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));

    let base_workflow = parse_analyzed_with_includes(&yaml, base_path)?;
    let task_count = base_workflow.tasks.len();
    let wf_hash = bench_cache::workflow_hash(&yaml);

    // Print header
    if !quiet && !json_output {
        nika::display::print_bench_header(
            resolved_path
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or("workflow"),
            task_count,
            iterations,
            providers,
        );
    }

    let bench_start = Instant::now();
    let mut all_results: Vec<nika::display::BenchProviderResult> = Vec::new();

    for provider_name in providers {
        let mut iteration_stats: Vec<RunStats> = Vec::new();
        let mut final_output: Option<String> = None;

        for i in 0..iterations {
            if !quiet && !json_output {
                eprint!(
                    "  {} {} [{}/{}]...",
                    nika::display::icons::provider(),
                    provider_name,
                    i + 1,
                    iterations,
                );
            }

            let mut workflow = base_workflow.clone();
            workflow.provider = Some(provider_name.as_str().into());

            let event_log = EventLog::new();
            let mut runner = Runner::with_event_log(workflow, event_log.clone())?
                .with_base_path(base_path.to_path_buf())
                .quiet();

            let iter_start = Instant::now();
            match runner.run().await {
                Ok(output) => {
                    let duration_ms = iter_start.elapsed().as_millis() as u64;
                    let mut stats = RunStats::default();
                    event_log.with_events(|events| {
                        for event in events {
                            stats.apply_event(event);
                        }
                    });

                    if !quiet && !json_output {
                        eprintln!(
                            " {} {:.1}s",
                            "\u{2713}".green(),
                            duration_ms as f64 / 1000.0,
                        );
                    }

                    if !output.is_empty() {
                        final_output = Some(output);
                    }
                    iteration_stats.push(stats);
                }
                Err(e) => {
                    if !quiet && !json_output {
                        eprintln!(" {} {}", "\u{2717}".red(), e);
                    }
                    // Continue with remaining iterations
                }
            }
        }

        if iteration_stats.is_empty() {
            if !quiet && !json_output {
                eprintln!(
                    "  {} All iterations failed for {}",
                    "\u{26A0}".yellow(),
                    provider_name,
                );
            }
            continue;
        }

        // Aggregate across iterations
        let mut result = aggregate_bench_stats(provider_name, &iteration_stats, task_count);

        // Quality evaluation via LLM-as-judge
        if eval {
            if let Some(ref output_text) = final_output {
                if !quiet && !json_output {
                    eprint!("  {} evaluating quality...", "\u{2727}".cyan());
                }
                match evaluate_quality(output_text, eval_model).await {
                    Ok(scores) => {
                        let overall = scores.iter().map(|s| s.score).sum::<f64>()
                            / scores.len().max(1) as f64;
                        result.quality_scores = scores;
                        result.quality_overall = Some(overall);
                        if !quiet && !json_output {
                            eprintln!(" {} {:.0}%", "\u{2713}".green(), overall * 100.0);
                        }
                    }
                    Err(e) => {
                        if !quiet && !json_output {
                            eprintln!(" {} {}", "\u{26A0}".yellow(), e);
                        }
                    }
                }
            }
        }

        all_results.push(result);
    }

    let bench_duration = bench_start.elapsed();

    if json_output {
        // JSON export
        let json_results: Vec<serde_json::Value> = all_results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "provider": r.provider,
                    "model": r.model,
                    "ttft_p50_ms": r.ttft.p50,
                    "ttft_p90_ms": r.ttft.p90,
                    "ttft_p99_ms": r.ttft.p99,
                    "tokens_per_sec": r.tokens_per_sec,
                    "total_secs": r.total_secs,
                    "cost_per_run": r.cost_per_run,
                    "input_tokens": r.input_tokens,
                    "output_tokens": r.output_tokens,
                    "cache_tokens": r.cache_tokens,
                    "quality_overall": r.quality_overall,
                    "quality_scores": r.quality_scores.iter().map(|s| {
                        serde_json::json!({"criterion": s.criterion, "score": s.score})
                    }).collect::<Vec<_>>(),
                })
            })
            .collect();
        let output = serde_json::json!({
            "workflow": file,
            "workflow_hash": wf_hash,
            "iterations": iterations,
            "bench_duration_secs": bench_duration.as_secs_f64(),
            "results": json_results,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&output).unwrap_or_default()
        );
    } else if !quiet {
        // Rich display
        nika::display::print_speed_section(&all_results);
        nika::display::print_cost_section(&all_results);

        if show_profile {
            nika::display::print_profile_section(&all_results);
        }

        nika::display::print_quality_section(&all_results);
        nika::display::print_bench_summary(&all_results, bench_duration, task_count, iterations);
    }

    // Persist to bench cache
    let cache_entry = bench_cache::BenchCacheEntry {
        workflow_hash: wf_hash.clone(),
        timestamp: {
            use std::time::SystemTime;
            let d = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default();
            format!("{}Z", d.as_secs())
        },
        iterations,
        results: all_results
            .iter()
            .map(|r| {
                (
                    r.provider.clone(),
                    bench_cache::CachedProviderResult {
                        model: r.model.clone(),
                        avg_duration_ms: (r.total_secs * 1000.0) as u64,
                        avg_cost_usd: r.cost_per_run,
                        avg_quality: r.quality_overall,
                        ttft_p50_ms: r.ttft.p50,
                        ttft_p90_ms: r.ttft.p90,
                        avg_input_tokens: r.input_tokens,
                        avg_output_tokens: r.output_tokens,
                        avg_tokens_per_sec: r.tokens_per_sec,
                    },
                )
            })
            .collect(),
    };
    if let Err(e) = bench_cache::write_cache(base_path, &cache_entry) {
        if !quiet {
            eprintln!("  {} Cache write failed: {}", "\u{26A0}".yellow(), e);
        }
    }

    Ok(())
}

/// Aggregate RunStats from multiple iterations into a single BenchProviderResult.
/// Evaluate output quality using an LLM-as-judge.
///
/// Sends the workflow output to a judge model with a scoring prompt.
/// Returns quality scores on 3 criteria: accuracy, relevance, completeness.
async fn evaluate_quality(
    output: &str,
    eval_model: &str,
) -> Result<Vec<nika::display::QualityScore>, NikaError> {
    use nika::ast::{InferParams, ResponseFormat, TaskAction};
    use nika::binding::ResolvedBindings;
    use nika::event::EventLog;
    use nika::runtime::TaskExecutor;
    use nika::store::RunContext;
    use std::sync::Arc;

    let detected = cli::verbs::detect_provider();
    let eval_provider = if eval_model.contains("claude") || eval_model.contains("haiku") {
        "anthropic"
    } else if eval_model.starts_with("gpt") || eval_model.starts_with("o1") {
        "openai"
    } else {
        detected.as_deref().unwrap_or("anthropic")
    };

    let truncated = &output[..output.len().min(4000)];
    let judge_prompt = format!(
        "You are a quality evaluator. Score the following AI-generated output on 3 criteria.\n\
         Each score is 0.0 to 1.0 (1.0 = perfect).\n\n\
         OUTPUT TO EVALUATE:\n---\n{truncated}\n---\n\n\
         Respond with ONLY valid JSON (no markdown, no explanation):\n\
         {{\"accuracy\": 0.0, \"relevance\": 0.0, \"completeness\": 0.0}}"
    );

    let infer = InferParams {
        prompt: judge_prompt,
        response_format: Some(ResponseFormat::Json),
        max_tokens: Some(200),
        temperature: Some(0.0),
        ..Default::default()
    };
    let action = TaskAction::Infer { infer };
    let task_id: Arc<str> = Arc::from("bench_eval");

    let event_log = EventLog::new();
    let executor = TaskExecutor::new(eval_provider, Some(eval_model), None, event_log)?;
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika::trust::InvocationSource::Cli);

    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await?;

    let parsed: serde_json::Value =
        serde_json::from_str(result.trim()).map_err(|e| NikaError::ParseError {
            details: format!("Judge response is not valid JSON: {e} — raw: {result}"),
        })?;

    let mut scores = Vec::new();
    for criterion in &["accuracy", "relevance", "completeness"] {
        let score = parsed
            .get(criterion)
            .and_then(|v| v.as_f64())
            .unwrap_or(0.5)
            .clamp(0.0, 1.0);
        scores.push(nika::display::QualityScore {
            criterion: criterion[..1].to_uppercase() + &criterion[1..],
            score,
        });
    }

    Ok(scores)
}

fn aggregate_bench_stats(
    provider_name: &str,
    iteration_stats: &[nika::display::renderer::RunStats],
    _task_count: usize,
) -> nika::display::BenchProviderResult {
    let n = iteration_stats.len() as f64;

    // Aggregate TTFT values across all iterations
    let mut all_ttft: Vec<u64> = iteration_stats
        .iter()
        .flat_map(|s| s.ttft_values.iter().copied())
        .collect();
    all_ttft.sort_unstable();

    let ttft = nika::display::Percentiles {
        p50: percentile(&all_ttft, 50),
        p90: percentile(&all_ttft, 90),
        p99: percentile(&all_ttft, 99),
    };

    // Average tokens
    let avg_input: u64 = (iteration_stats
        .iter()
        .map(|s| s.total_input_tokens)
        .sum::<u64>() as f64
        / n) as u64;
    let avg_output: u64 = (iteration_stats
        .iter()
        .map(|s| s.total_output_tokens)
        .sum::<u64>() as f64
        / n) as u64;
    let avg_cache: u64 = (iteration_stats
        .iter()
        .map(|s| s.total_cache_tokens)
        .sum::<u64>() as f64
        / n) as u64;

    // Average cost
    let avg_cost: f64 = iteration_stats.iter().map(|s| s.total_cost).sum::<f64>() / n;

    // Total duration from task_timeline (sum of all task durations)
    let avg_duration_ms: f64 = iteration_stats
        .iter()
        .map(|s| {
            s.task_timeline
                .iter()
                .map(|(_, _, start, dur)| start + dur)
                .max()
                .unwrap_or(0)
        })
        .sum::<u64>() as f64
        / n;
    let total_secs = avg_duration_ms / 1000.0;

    // Tokens per second
    let tokens_per_sec = if total_secs > 0.0 {
        avg_output as f64 / total_secs
    } else {
        0.0
    };

    // Model name from the first iteration's provider_calls
    let model = iteration_stats
        .iter()
        .flat_map(|s| s.provider_calls.first())
        .map(|pc| pc.model.clone())
        .next()
        .unwrap_or_else(|| "unknown".to_string());

    // Task timeline from the median iteration (by total duration)
    let mut durations: Vec<(usize, u64)> = iteration_stats
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let d = s
                .task_timeline
                .iter()
                .map(|(_, _, start, dur)| start + dur)
                .max()
                .unwrap_or(0);
            (i, d)
        })
        .collect();
    durations.sort_by_key(|(_, d)| *d);
    let median_idx = durations
        .get(durations.len() / 2)
        .map(|(i, _)| *i)
        .unwrap_or(0);
    let median_stats = &iteration_stats[median_idx];

    let task_timeline: Vec<nika::display::BenchTaskTiming> = {
        let max_dur = median_stats
            .task_timeline
            .iter()
            .map(|(_, _, _, dur)| *dur)
            .max()
            .unwrap_or(1);
        median_stats
            .task_timeline
            .iter()
            .map(
                |(task_id, verb, start, dur)| nika::display::BenchTaskTiming {
                    task_id: task_id.clone(),
                    verb: verb.clone(),
                    start_ms: *start,
                    duration_ms: *dur,
                    is_bottleneck: *dur == max_dur,
                },
            )
            .collect()
    };

    nika::display::BenchProviderResult {
        provider: provider_name.to_string(),
        model,
        ttft,
        tokens_per_sec,
        total_secs,
        cost_per_run: avg_cost,
        input_tokens: avg_input,
        output_tokens: avg_output,
        cache_tokens: avg_cache,
        task_timeline,
        quality_scores: vec![],
        quality_overall: None,
    }
}

/// Compute the p-th percentile from a sorted slice.
fn percentile(sorted: &[u64], p: u32) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((p as f64 / 100.0) * (sorted.len() as f64 - 1.0)).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

#[allow(clippy::too_many_arguments)]
async fn run_workflow(
    file: &str,
    provider_override: Option<String>,
    model_override: Option<String>,
    cli_inputs: &[String],
    input_file: Option<&str>,
    interactive: bool,
    output_file: Option<&str>,
    task_filter: Option<&str>,
    from_filter: Option<&str>,
    skip_confirm: bool,
    quiet: bool,
    detail: nika::display::DetailLevel,
    no_live: bool,
    permission: &str,
    resume: bool,
) -> Result<(), NikaError> {
    // Bootstrap secrets: env vars → daemon IPC → vault.
    // Ensures keys stored via `nika keys set` are available without restarting the shell.
    let _ = nika::secrets::load_from_daemon_or_fallback().await;

    let resolved_path = resolve_workflow_path(file).await?;

    let yaml = tokio::fs::read_to_string(&resolved_path).await?;

    let validator = WorkflowSchemaValidator::new()?;
    validator.validate_yaml(&yaml)?;

    let base_path = resolved_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));

    // Parse with include expansion: raw → expand_raw_include → analyze
    // This merges partial workflows BEFORE validation so cross-file references work.
    let mut workflow = parse_analyzed_with_includes(&yaml, base_path)?;

    // Auto-onboarding: if workflow needs an LLM and no API keys are set, run the wizard.
    // Skip for mock/native providers which don't need API keys.
    let is_mock_or_native = provider_override
        .as_deref()
        .or(workflow.provider.as_ref().map(|p| p.as_str()))
        .is_some_and(|p| {
            let lower = p.to_lowercase();
            lower == "mock" || lower == "native" || lower == "local"
        });
    let needs_llm = !is_mock_or_native
        && workflow.tasks.iter().any(|t| {
            matches!(
                t.action,
                nika::ast::analyzed::AnalyzedTaskAction::Infer(_)
                    | nika::ast::analyzed::AnalyzedTaskAction::Agent(_)
            )
        });
    if needs_llm && !cli::onboarding::skip_onboarding() && !cli::onboarding::has_any_provider_key()
    {
        let configured = cli::onboarding::run_onboarding_wizard().await?;
        if !configured {
            return Err(NikaError::ConfigError {
                reason:
                    "No API key configured. Run `nika keys set <provider> <key>` or `nika setup`."
                        .to_string(),
            });
        }
    }

    if let Some(p) = provider_override {
        workflow.provider = Some(p.into());
    }
    if let Some(m) = model_override {
        workflow.model = Some(m);
    }

    // Merge CLI input overrides (YAML defaults < file < CLI flags)
    if let Some(input_file_path) = input_file {
        let file_inputs = load_input_file(input_file_path).await?;
        for (k, v) in file_inputs {
            workflow.inputs.insert(k, v);
        }
    }
    if !cli_inputs.is_empty() {
        let parsed = parse_cli_inputs(cli_inputs)?;
        for (k, v) in parsed {
            workflow.inputs.insert(k, v);
        }
    }

    // Interactive prompts for inputs without defaults
    if interactive && std::io::stdin().is_terminal() {
        let keys: Vec<String> = workflow.inputs.keys().cloned().collect();
        for key in keys {
            let value = &workflow.inputs[&key];
            let has_value = match value {
                serde_json::Value::Null => false,
                serde_json::Value::Object(obj) => obj.contains_key("default"),
                _ => true,
            };
            if !has_value {
                let input: String = cliclack::input(format!("Enter value for '{}':", key))
                    .interact()
                    .map_err(|_| NikaError::ValidationError {
                        reason: format!("Input '{}' required but not provided", key),
                    })?;
                workflow.inputs.insert(key, parse_input_value(&input));
            }
        }
    }

    // Task filtering: --task (single task + deps) or --from (from task onwards)
    if let Some(target) = task_filter {
        filter_tasks_for_target(&mut workflow, target)?;
        if !quiet {
            eprintln!(
                "  {} running task '{}' + dependencies ({} tasks)",
                "Filter:".cyan(),
                target,
                workflow.tasks.len()
            );
        }
    } else if let Some(from_id) = from_filter {
        filter_tasks_from(&mut workflow, from_id)?;
        if !quiet {
            eprintln!(
                "  {} running from '{}' onwards ({} tasks)",
                "Filter:".cyan(),
                from_id,
                workflow.tasks.len()
            );
        }
    }

    // Cost estimation: warn if LLM tasks detected and not --yes
    if !skip_confirm && std::io::stdin().is_terminal() {
        let infer_count = workflow
            .tasks
            .iter()
            .filter(|t| matches!(t.action.verb_name(), "infer" | "agent"))
            .count();
        if infer_count > 0 {
            let provider_name = workflow
                .provider
                .as_ref()
                .map(|p| p.as_str())
                .unwrap_or("anthropic");
            let model_name = workflow
                .model
                .as_deref()
                .unwrap_or("claude-sonnet-4-20250514");
            if let Some(pk) = nika::provider::cost::ProviderKind::parse(provider_name) {
                let avg_tokens = 2000u64;
                let est_cost = nika::provider::cost::calculate_cost(
                    pk,
                    model_name,
                    avg_tokens * infer_count as u64,
                    avg_tokens * infer_count as u64,
                );
                if est_cost > 0.10 {
                    eprintln!(
                        "  {} ~${:.4} ({} LLM tasks, {})",
                        "Estimated cost:".yellow(),
                        est_cost,
                        infer_count,
                        model_name
                    );
                    let confirm: bool = cliclack::confirm("Continue?")
                        .initial_value(true)
                        .interact()
                        .unwrap_or(false);
                    if !confirm {
                        return Err(NikaError::ValidationError {
                            reason: "Cancelled by user".to_string(),
                        });
                    }
                }
            }
        }
    }

    if !quiet && !detail.is_json() {
        let nodes: Vec<&str> = workflow.tasks.iter().map(|t| t.name.as_str()).collect();
        let edges: Vec<(&str, &str)> = workflow
            .tasks
            .iter()
            .flat_map(|task| {
                task.depends_on.iter().filter_map(|dep_id| {
                    workflow
                        .task_table
                        .get_name(*dep_id)
                        .map(|dep_name| (dep_name, task.name.as_str()))
                })
            })
            .collect();
        let depths = nika::dag::flow::compute_layers(&nodes, &edges);
        let layer_count = nika::dag::flow::layer_count(&depths);

        let gen_id = format!("{:08x}", rand::random::<u32>());
        nika::display::header::print_header(
            workflow.name.as_deref(),
            workflow
                .provider
                .as_ref()
                .map(|p| p.as_str())
                .unwrap_or("(auto)"),
            workflow.model.as_deref().unwrap_or("(default)"),
            workflow.tasks.len(),
            layer_count,
            env!("CARGO_PKG_VERSION"),
            &gen_id,
        );

        // Inline DAG summary
        nika::display::header::print_dag_summary(&nodes, &depths);

        // IMP-1: Migration hint for old schema versions
        if let Some(hint) = workflow.schema_version.migration_hint() {
            println!(
                "{} Schema {} is not the latest. Upgrade: {}",
                "⚠".yellow(),
                workflow.schema_version.as_str().yellow(),
                hint.dimmed()
            );
        }
    }

    let perm_mode = match permission {
        "deny" => nika_engine::tools::PermissionMode::Deny,
        "plan" => nika_engine::tools::PermissionMode::Plan,
        "accept-edits" => nika_engine::tools::PermissionMode::AcceptEdits,
        "yolo" | "accept-all" => nika_engine::tools::PermissionMode::YoloMode,
        _ => nika_engine::tools::PermissionMode::AcceptEdits,
    };
    // Load custom endpoints from config.toml for OpenAI-compatible servers
    let config = nika::config::NikaConfig::load()
        .unwrap_or_default()
        .with_env();
    let mut runner = Runner::new(workflow)?
        .with_base_path(base_path.to_path_buf())
        .with_permission_mode(perm_mode);

    // Wire project root + working_dir mode from nika.toml so exec cwd,
    // nika:read security boundary, and from_example paths resolve correctly
    // when workflows live in subdirectories (e.g. workflows/*.nika.yaml).
    if let Ok(project) =
        cli::config::find_project_root_from(&std::env::current_dir().unwrap_or_default())
    {
        runner = runner.with_project_root(project.root.clone());
        if project.source == cli::config::ProjectRootSource::NikaToml {
            if let Some(bootstrap) = cli::config::load_project_config(&project.root) {
                if let Some(wd) = bootstrap.tools.working_dir {
                    runner = runner.with_working_dir_mode(wd);
                }
            }
        }
    }

    // Merge endpoints: project nika.toml wins, user config.toml fills gaps
    let mut all_endpoints = std::collections::HashMap::new();
    // Project-level endpoints go first (win on conflict)
    if let Ok(project) =
        cli::config::find_project_root_from(&std::env::current_dir().unwrap_or_default())
    {
        if let Some(bootstrap) = cli::config::load_project_config(&project.root) {
            all_endpoints = bootstrap.endpoints;
        }
    }
    // User-level endpoints fill gaps (don't override project)
    for (name, ep) in &config.endpoints {
        all_endpoints
            .entry(name.clone())
            .or_insert_with(|| ep.clone());
    }
    if !all_endpoints.is_empty() {
        if let Ok(resolved) = nika::provider::endpoints::resolve_endpoints(&all_endpoints) {
            runner.with_custom_endpoints(resolved);
        }
    }
    if quiet {
        runner = runner.quiet();
    }
    let mut runner = if no_live {
        runner.with_classic_renderer(detail)
    } else {
        runner.with_detail_level(detail)
    };

    // Resume from last run: pre-populate datastore with completed tasks
    if resume {
        use colored::Colorize;
        let traces = nika::event::list_traces()?;
        if let Some(trace) = traces.first() {
            // Safety: reject oversized traces (consistent with SEC-4 50MB limit)
            let trace_size = std::fs::metadata(&trace.path).map(|m| m.len()).unwrap_or(0);
            if trace_size > 50 * 1024 * 1024 {
                if !quiet {
                    eprintln!(
                        "  {} trace too large ({} MB), running from scratch",
                        "Resume:".yellow(),
                        trace_size / (1024 * 1024)
                    );
                }
            } else {
                let content = std::fs::read_to_string(&trace.path)?;
                let mut resumed_count = 0u32;
                // Verify workflow identity via hash in first WorkflowStarted event
                let mut trace_hash: Option<String> = None;
                for line in content.lines() {
                    if let Ok(event) = serde_json::from_str::<nika::event::Event>(line) {
                        match event.kind {
                            nika::event::EventKind::WorkflowStarted { workflow_hash, .. } => {
                                trace_hash = Some(workflow_hash);
                            }
                            nika::event::EventKind::TaskCompleted {
                                task_id,
                                output,
                                duration_ms,
                            } => {
                                runner.datastore().insert(
                                    task_id,
                                    nika::store::TaskResult::success(
                                        output.as_ref().clone(),
                                        std::time::Duration::from_millis(duration_ms),
                                    ),
                                );
                                resumed_count += 1;
                            }
                            _ => {}
                        }
                    }
                }
                // Warn if workflow changed since trace was recorded
                if let Some(ref th) = trace_hash {
                    let current_hash = nika::event::calculate_workflow_hash(&yaml);
                    if *th != current_hash && !quiet {
                        eprintln!(
                            "  {} workflow changed since last run (trace hash mismatch)",
                            "Warning:".yellow()
                        );
                    }
                }
                if !quiet && resumed_count > 0 {
                    eprintln!(
                        "  {} resuming from {} ({} completed tasks cached)",
                        "Resume:".cyan(),
                        trace.generation_id,
                        resumed_count
                    );
                } else if !quiet && resumed_count == 0 {
                    eprintln!(
                        "  {} trace found ({}) but no completed tasks to resume",
                        "Resume:".yellow(),
                        trace.generation_id
                    );
                }
            }
        } else if !quiet {
            eprintln!(
                "  {} no previous trace found, running from scratch",
                "Resume:".yellow()
            );
        }
    }

    let run_output = runner.run().await?;

    // Save task outputs to file if -o/--output specified
    if let Some(output_path) = output_file {
        let results = runner.datastore().iter_results();
        let mut output_map = serde_json::Map::new();
        for (task_id, result) in &results {
            let mut task_obj = serde_json::Map::new();
            task_obj.insert("output".to_string(), (*result.output).clone());
            task_obj.insert(
                "status".to_string(),
                serde_json::json!(format!("{:?}", result.status)),
            );
            task_obj.insert(
                "duration_ms".to_string(),
                serde_json::json!(result.duration.as_millis() as u64),
            );
            output_map.insert(task_id.to_string(), serde_json::Value::Object(task_obj));
        }
        let json = serde_json::to_string_pretty(&serde_json::Value::Object(output_map))
            .unwrap_or_default();
        tokio::fs::write(output_path, &json)
            .await
            .map_err(|e| NikaError::ParseError {
                details: format!("Failed to write output file '{}': {}", output_path, e),
            })?;
        if !quiet {
            eprintln!("  {} {}", "Output saved:".green(), output_path);
        }
    }

    if !quiet && !run_output.is_empty() {
        println!("{}", "Output:".cyan().bold());
        println!("{run_output}");
    }

    Ok(())
}

/// Explain a workflow in human-readable format.
/// Normalize captured output for golden file comparison.
/// Strips non-deterministic fields (duration_ms) and sorts keys for stable ordering.
fn normalize_golden(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut normalized = serde_json::Map::new();
            for (key, val) in map {
                if key == "duration_ms" {
                    continue; // strip non-deterministic timing
                }
                normalized.insert(key.clone(), normalize_golden(val));
            }
            serde_json::Value::Object(normalized)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(normalize_golden).collect())
        }
        other => other.clone(),
    }
}

/// Compare two golden JSON values, returning a list of mismatches.
fn compare_golden(
    actual: &serde_json::Value,
    expected: &serde_json::Value,
    path: &str,
) -> Vec<String> {
    let mut diffs = Vec::new();
    match (actual, expected) {
        (serde_json::Value::Object(a), serde_json::Value::Object(e)) => {
            // Keys in expected but missing in actual
            for key in e.keys() {
                if !a.contains_key(key) {
                    diffs.push(format!("{path}.{key}: missing in actual output"));
                }
            }
            // Keys in actual but missing in expected
            for key in a.keys() {
                if !e.contains_key(key) {
                    diffs.push(format!("{path}.{key}: unexpected key in actual output"));
                }
            }
            // Recurse on shared keys
            for key in e.keys() {
                if let (Some(av), Some(ev)) = (a.get(key), e.get(key)) {
                    diffs.extend(compare_golden(av, ev, &format!("{path}.{key}")));
                }
            }
        }
        (serde_json::Value::Array(a), serde_json::Value::Array(e)) => {
            if a.len() != e.len() {
                diffs.push(format!(
                    "{path}: array length mismatch (actual={}, expected={})",
                    a.len(),
                    e.len()
                ));
            }
            for (i, (av, ev)) in a.iter().zip(e.iter()).enumerate() {
                diffs.extend(compare_golden(av, ev, &format!("{path}[{i}]")));
            }
        }
        (a, e) if a != e => {
            let actual_str = serde_json::to_string(a).unwrap_or_default();
            let expected_str = serde_json::to_string(e).unwrap_or_default();
            // Truncate long values for readability
            let trunc = |s: String| -> String {
                if s.len() > 120 {
                    format!("{}…", &s[..117])
                } else {
                    s
                }
            };
            diffs.push(format!(
                "{path}: value mismatch\n      actual:   {}\n      expected: {}",
                trunc(actual_str),
                trunc(expected_str)
            ));
        }
        _ => {} // equal
    }
    diffs
}

async fn test_workflow(
    file: &str,
    golden: Option<&str>,
    update_snapshot: bool,
    cli_inputs: &[String],
    quiet: bool,
    detail: nika::display::DetailLevel,
) -> Result<(), NikaError> {
    use colored::Colorize;

    let needs_capture = golden.is_some() || update_snapshot;

    // Create temp file for output capture when golden comparison is needed
    let capture_path = if needs_capture {
        let mut path = std::env::temp_dir();
        path.push(format!("nika-test-{}.json", std::process::id()));
        Some(path.to_string_lossy().to_string())
    } else {
        None
    };

    // Run workflow with mock provider (no API keys needed)
    let result = run_workflow(
        file,
        Some("mock".to_string()),
        None,
        cli_inputs,
        None,
        false, // not interactive
        capture_path.as_deref(),
        None,
        None,
        true, // skip cost confirm
        quiet,
        detail,
        true, // no-live for test output
        "deny",
        false,
    )
    .await;

    match &result {
        Ok(()) => {
            if !quiet {
                eprintln!("  {} {}", "PASS".green().bold(), file);
            }
        }
        Err(e) => {
            if !quiet {
                eprintln!("  {} {} — {}", "FAIL".red().bold(), file, e);
            }
            return result;
        }
    }

    // Golden file comparison (if requested)
    if let Some(golden_path) = golden {
        // Read captured output
        let captured_json = if let Some(ref cp) = capture_path {
            let raw =
                tokio::fs::read_to_string(cp)
                    .await
                    .map_err(|e| NikaError::BuiltinToolError {
                        tool: "test".into(),
                        reason: format!("Failed to read captured output: {e}"),
                    })?;
            let val: serde_json::Value =
                serde_json::from_str(&raw).map_err(|e| NikaError::BuiltinToolError {
                    tool: "test".into(),
                    reason: format!("Invalid captured output JSON: {e}"),
                })?;
            normalize_golden(&val)
        } else {
            serde_json::Value::Object(serde_json::Map::new())
        };

        if update_snapshot {
            // Write normalized output to golden file
            let pretty = serde_json::to_string_pretty(&captured_json).unwrap_or_default();
            tokio::fs::write(golden_path, &pretty).await.map_err(|e| {
                NikaError::BuiltinToolError {
                    tool: "test".into(),
                    reason: format!("Failed to write golden file '{}': {e}", golden_path),
                }
            })?;
            if !quiet {
                eprintln!(
                    "  {} golden file updated: {}",
                    "Snapshot:".cyan(),
                    golden_path
                );
            }
        } else if Path::new(golden_path).exists() {
            // Compare output to golden file
            let golden_content = tokio::fs::read_to_string(golden_path).await?;
            let golden_value: serde_json::Value =
                serde_json::from_str(&golden_content).map_err(|e| NikaError::BuiltinToolError {
                    tool: "test".into(),
                    reason: format!("Invalid golden file JSON: {e}"),
                })?;
            let golden_normalized = normalize_golden(&golden_value);

            let diffs = compare_golden(&captured_json, &golden_normalized, "$");
            if diffs.is_empty() {
                if !quiet {
                    eprintln!("  {} golden file matches", "OK".green());
                }
            } else {
                eprintln!(
                    "  {} golden file mismatch ({} difference{}):",
                    "FAIL".red().bold(),
                    diffs.len(),
                    if diffs.len() > 1 { "s" } else { "" }
                );
                for diff in &diffs {
                    eprintln!("    {diff}");
                }
                return Err(NikaError::BuiltinToolError {
                    tool: "test".into(),
                    reason: format!(
                        "Golden file mismatch: {} difference(s). Run with --update-snapshot to update.",
                        diffs.len()
                    ),
                });
            }
        } else {
            return Err(NikaError::BuiltinToolError {
                tool: "test".into(),
                reason: format!(
                    "Golden file not found: {}. Run with --update-snapshot to create it.",
                    golden_path
                ),
            });
        }
    }

    // Clean up temp capture file
    if let Some(ref cp) = capture_path {
        let _ = tokio::fs::remove_file(cp).await;
    }

    result
}

#[allow(clippy::too_many_arguments)]
async fn eval_workflow(
    file: &str,
    dataset_path: &str,
    provider_override: Option<&str>,
    format: &str,
    fail_fast: bool,
    parallel: usize,
    skip_confirm: bool,
    quiet: bool,
    detail: nika::display::DetailLevel,
) -> Result<(), NikaError> {
    use cli::eval;
    use colored::Colorize;

    let entries = eval::load_dataset(dataset_path)?;
    let concurrency = parallel.max(1).min(entries.len());

    if !quiet {
        eprintln!(
            "  {} {} entries from {}{}",
            "Eval:".cyan(),
            entries.len(),
            dataset_path,
            if concurrency > 1 {
                format!(" (parallel: {concurrency})")
            } else {
                String::new()
            }
        );
    }

    // Default to mock provider for safety (no accidental API costs)
    let provider = provider_override
        .map(|s| s.to_string())
        .unwrap_or_else(|| "mock".to_string());

    let mut results = Vec::with_capacity(entries.len());
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(concurrency));

    for (i, entry) in entries.iter().enumerate() {
        let _permit = semaphore.acquire().await.unwrap();
        let start = std::time::Instant::now();
        let cli_inputs = eval::inputs_to_cli_args(&entry.inputs);

        // Create temp file for output capture
        let capture_dir = std::env::temp_dir();
        let capture_path = capture_dir.join(format!("nika-eval-{}-{i}.json", std::process::id()));
        let capture_str = capture_path.to_string_lossy().to_string();

        // Run workflow with output capture
        let run_result = run_workflow(
            file,
            Some(provider.clone()),
            None,
            &cli_inputs,
            None,
            false,
            Some(&capture_str),
            None,
            None,
            skip_confirm || provider == "mock",
            true, // quiet — suppress per-run output
            detail,
            true, // no-live
            "deny",
            false,
        )
        .await;

        let duration_ms = start.elapsed().as_millis() as u64;

        match run_result {
            Ok(()) => {
                // Read captured output
                let captured: std::collections::HashMap<String, serde_json::Value> =
                    if let Ok(raw) = tokio::fs::read_to_string(&capture_path).await {
                        serde_json::from_str(&raw).unwrap_or_default()
                    } else {
                        std::collections::HashMap::new()
                    };
                let _ = tokio::fs::remove_file(&capture_path).await;

                let failures = eval::validate_entry(&captured, &entry.expected);
                let passed = failures.is_empty();

                if !quiet && !passed {
                    eprintln!("  {} entry #{i}", "FAIL".red().bold());
                    for f in &failures {
                        eprintln!("    {f}");
                    }
                } else if !quiet {
                    eprintln!("  {} entry #{i} ({}ms)", "PASS".green(), duration_ms);
                }

                results.push(eval::EvalResult {
                    entry_index: i,
                    passed,
                    failures,
                    duration_ms,
                });

                if !passed && fail_fast {
                    break;
                }
            }
            Err(e) => {
                let _ = tokio::fs::remove_file(&capture_path).await;

                if !quiet {
                    eprintln!("  {} entry #{i} — {e}", "FAIL".red().bold());
                }
                results.push(eval::EvalResult {
                    entry_index: i,
                    passed: false,
                    failures: vec![format!("workflow execution failed: {e}")],
                    duration_ms,
                });
                if fail_fast {
                    break;
                }
            }
        }
    }

    eval::finalize(results, format, quiet)
}

async fn explain_workflow(file: &str) -> Result<(), NikaError> {
    let resolved = resolve_workflow_path(file).await?;
    let yaml = tokio::fs::read_to_string(&resolved).await?;

    let validator = WorkflowSchemaValidator::new()?;
    validator.validate_yaml(&yaml)?;

    let base_path = resolved
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let workflow = parse_analyzed_with_includes(&yaml, base_path)?;

    // Count verbs
    let mut infer_count = 0u32;
    let mut exec_count = 0u32;
    let mut fetch_count = 0u32;
    let mut invoke_count = 0u32;
    let mut agent_count = 0u32;
    for task in &workflow.tasks {
        match &task.action {
            nika::ast::analyzed::AnalyzedTaskAction::Infer(_) => infer_count += 1,
            nika::ast::analyzed::AnalyzedTaskAction::Exec(_) => exec_count += 1,
            nika::ast::analyzed::AnalyzedTaskAction::Fetch(_) => fetch_count += 1,
            nika::ast::analyzed::AnalyzedTaskAction::Invoke(_) => invoke_count += 1,
            nika::ast::analyzed::AnalyzedTaskAction::Agent(_) => agent_count += 1,
        }
    }

    // Collect required providers
    let default_provider = workflow
        .provider
        .as_ref()
        .map(|p| p.as_str())
        .unwrap_or("anthropic");
    let mut providers: Vec<&str> = vec![default_provider];
    for task in &workflow.tasks {
        if let Some(ref p) = task.provider {
            let name = p.as_str();
            if !providers.contains(&name) {
                providers.push(name);
            }
        }
    }

    // LLM task count for cost estimate
    let llm_tasks = infer_count + agent_count;

    // Count dependency layers (simple: max depth via depends_on chain)
    let task_count = workflow.tasks.len();

    println!();
    println!(
        "  {} {}",
        "Workflow:".bold(),
        workflow.name.as_deref().unwrap_or(file)
    );
    if let Some(ref desc) = workflow.description {
        println!("  {} {}", "Description:".bold(), desc);
    }
    println!();
    println!("  {} tasks", task_count.to_string().cyan().bold(),);
    println!();

    // Verb breakdown
    let mut verbs = Vec::new();
    if infer_count > 0 {
        verbs.push(format!("{infer_count} infer"));
    }
    if exec_count > 0 {
        verbs.push(format!("{exec_count} exec"));
    }
    if fetch_count > 0 {
        verbs.push(format!("{fetch_count} fetch"));
    }
    if invoke_count > 0 {
        verbs.push(format!("{invoke_count} invoke"));
    }
    if agent_count > 0 {
        verbs.push(format!("{agent_count} agent"));
    }
    println!("  {} {}", "Verbs:".bold(), verbs.join(", "));

    // Providers
    let provider_list: Vec<String> = providers.iter().map(|p| p.to_string()).collect();
    println!("  {} {}", "Providers:".bold(), provider_list.join(", "));

    // Model
    if let Some(ref model) = workflow.model {
        println!("  {} {}", "Model:".bold(), model);
    }

    // Estimated cost (rough: ~$0.003 per infer, ~$0.05 per agent turn)
    if llm_tasks > 0 {
        let est_cost = (infer_count as f64) * 0.003 + (agent_count as f64) * 0.05;
        println!(
            "  {} ~${:.2} ({llm_tasks} LLM calls)",
            "Est. cost:".bold(),
            est_cost
        );
    }

    // Required env vars
    let needs_key = providers
        .iter()
        .any(|p| !["mock", "native", "local"].contains(p));
    if needs_key {
        let env_vars: Vec<String> = providers
            .iter()
            .filter(|p| !["mock", "native", "local"].contains(*p))
            .map(|p| format!("{}_API_KEY", p.to_uppercase()))
            .collect();
        println!("  {} {}", "Required:".bold(), env_vars.join(", "));
    }

    println!();
    Ok(())
}

async fn validate_workflow(file: &str, quiet: bool, security: bool) -> Result<(), NikaError> {
    use nika::display::{
        print_check_header, print_check_summary, print_check_warnings, print_phase,
        print_phase_skipped, PhaseResult,
    };
    use std::time::Instant;

    let total_start = Instant::now();
    let resolved_path = resolve_workflow_path(file).await?;

    let yaml = tokio::fs::read_to_string(&resolved_path).await?;

    // Phase 1: Schema validation
    let t = Instant::now();
    let validator = WorkflowSchemaValidator::new()?;
    validator.validate_yaml(&yaml)?;
    let schema_elapsed = t.elapsed();

    // Phase 2: Parse (decomposed to capture analyzer warnings)
    let t = Instant::now();
    let base_path = resolved_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));

    let raw = nika::ast::raw::parse(&yaml, nika::source::FileId(0)).map_err(|e| {
        NikaError::ParseError {
            details: format!("[{}] {}", e.kind.code(), e.message),
        }
    })?;
    let raw = nika::ast::expand_raw_include(raw, base_path)?;
    let analyze_result = nika::ast::analyzer::analyze(raw);

    // Capture warnings before into_result() drops them
    let analyzer_warnings: Vec<String> = analyze_result
        .warnings
        .iter()
        .map(|w| {
            let code = w.kind.code();
            if let Some(ref suggestion) = w.suggestion {
                format!("[{}] {} ({})", code, w.message, suggestion)
            } else {
                format!("[{}] {}", code, w.message)
            }
        })
        .collect();

    let analyzed = analyze_result.into_result().map_err(|errors| {
        let messages: Vec<String> = errors
            .iter()
            .map(|e| format!("[{}] {}", e.kind.code(), e))
            .collect();
        NikaError::ValidationError {
            reason: messages.join("; "),
        }
    })?;

    // Run taint analysis on the analyzed AST (before lowering consumes it)
    let taint_report = if security {
        use nika::trust::InvocationSource;
        Some(nika::ast::analyzer::taint::TaintAnalyzer::analyze(
            &analyzed,
            InvocationSource::Cli,
        ))
    } else {
        None
    };

    let workflow = nika::ast::lower::lower(analyzed)?;
    let parse_elapsed = t.elapsed();
    let includes_elapsed = std::time::Duration::ZERO;

    // Phase 4: DAG
    let t = Instant::now();
    let flow_graph = Dag::from_workflow(&workflow)?;
    let dag_cycle_result = flow_graph.detect_cycles();
    let dag_elapsed = t.elapsed();

    if let Err(ref e) = dag_cycle_result {
        if !quiet {
            print_check_header(file, false, env!("CARGO_PKG_VERSION"));
            print_phase(&PhaseResult {
                name: "schema",
                passed: true,
                detail: format!("YAML valid against @{}", workflow.schema),
                duration_ms: schema_elapsed.as_millis() as u64,
                errors: vec![],
                hints: vec![],
            });
            print_phase(&PhaseResult {
                name: "parse",
                passed: true,
                detail: format!(
                    "{} tasks \u{00B7} provider: {} \u{00B7} model: {}",
                    workflow.tasks.len(),
                    workflow.provider,
                    workflow.model.as_deref().unwrap_or("(default)")
                ),
                duration_ms: parse_elapsed.as_millis() as u64,
                errors: vec![],
                hints: vec![],
            });
            print_phase(&PhaseResult {
                name: "includes",
                passed: true,
                detail: "resolved".to_string(),
                duration_ms: includes_elapsed.as_millis() as u64,
                errors: vec![],
                hints: vec![],
            });
            print_phase(&PhaseResult {
                name: "dag",
                passed: false,
                detail: "CYCLE DETECTED".to_string(),
                duration_ms: dag_elapsed.as_millis() as u64,
                errors: vec![e.to_string()],
                hints: vec![
                    "Remove one dependency to break the cycle.".to_string(),
                    "Common fix: use with: binding instead of depends_on.".to_string(),
                ],
            });
            print_phase_skipped("bindings", "DAG invalid");
            print_phase_skipped("schemas", "DAG invalid");
            println!();
            print_check_summary(
                false,
                total_start.elapsed().as_millis() as u64,
                workflow.tasks.len(),
                workflow.flow_count(),
                0,
                0,
                None,
                &[("NIKA-020", "Circular dependency detected")],
            );
        }
        return dag_cycle_result;
    }

    // Phase 5: Bindings
    let t = Instant::now();
    validate_bindings(&workflow, &flow_graph)?;
    let bindings_elapsed = t.elapsed();

    // Phase 6: Validate structured output schema files
    let t = Instant::now();
    let mut schema_count = 0u32;
    for task in &workflow.tasks {
        // Check output.schema file references
        if let Some(ref output) = task.output {
            if let Some(SchemaRef::File(ref path)) = output.schema {
                validate_schema_file(&task.id, path, base_path).await?;
                schema_count += 1;
            }
        }
        // Check structured.schema file references
        if let Some(ref spec) = task.structured {
            if let Some(SchemaRef::File(ref path)) = spec.schema {
                validate_schema_file(&task.id, path, base_path).await?;
                schema_count += 1;
            }
        }
    }
    let schemas_elapsed = t.elapsed();

    // Phase 7: Security hints (shell escape warnings)
    let t = Instant::now();
    let mut security_hints: Vec<String> = Vec::new();
    {
        let binding_re = regex::Regex::new(r"\{\{(with\.[^}]+|inputs\.[^}]+)\}\}").unwrap();
        let shell_guard_re = regex::Regex::new(r"\|\s*shell\b").unwrap();
        for task in &workflow.tasks {
            if let TaskAction::Exec { exec } = &task.action {
                if exec.shell == Some(true) {
                    for cap in binding_re.captures_iter(&exec.command) {
                        let inner = &cap[1];
                        if !shell_guard_re.is_match(inner) {
                            security_hints.push(format!(
                                "task '{}': shell: true with unescaped {{{{{}}}}} — use | shell transform",
                                task.id, inner
                            ));
                        }
                    }
                }
            }
        }
    }
    let security_elapsed = t.elapsed();

    // Phase 8: Provider API keys (BUG 6 / NIKA-032)
    let t = Instant::now();
    let mut provider_warnings: Vec<String> = Vec::new();
    {
        let mut providers_used: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        providers_used.insert(workflow.provider.to_string());

        // Collect per-task providers from analyzed AST
        if let Ok(analyzed) = parse_analyzed(&yaml) {
            for task in &analyzed.tasks {
                if let Some(ref p) = task.provider {
                    providers_used.insert(p.to_string());
                }
            }
        }

        for provider_name in &providers_used {
            if let Some(provider) = nika::core::find_provider(provider_name) {
                if provider.requires_key && !nika::secrets::has_provider_key(provider) {
                    provider_warnings.push(format!(
                        "{} not set (provider '{}' used in workflow)",
                        provider.env_var, provider_name
                    ));
                }
            }
        }
    }
    let providers_elapsed = t.elapsed();

    if !quiet {
        print_check_header(file, false, env!("CARGO_PKG_VERSION"));

        // Phase 1: Schema
        print_phase(&PhaseResult {
            name: "schema",
            passed: true,
            detail: format!("YAML valid against @{}", workflow.schema),
            duration_ms: schema_elapsed.as_millis() as u64,
            errors: vec![],
            hints: vec![],
        });

        // Phase 2: Parse
        print_phase(&PhaseResult {
            name: "parse",
            passed: true,
            detail: format!(
                "{} tasks \u{00B7} provider: {} \u{00B7} model: {}",
                workflow.tasks.len(),
                workflow.provider,
                workflow.model.as_deref().unwrap_or("(default)")
            ),
            duration_ms: parse_elapsed.as_millis() as u64,
            errors: vec![],
            hints: vec![],
        });

        // Phase 3: Includes
        print_phase(&PhaseResult {
            name: "includes",
            passed: true,
            detail: "resolved".to_string(),
            duration_ms: includes_elapsed.as_millis() as u64,
            errors: vec![],
            hints: vec![],
        });

        // Phase 4: DAG
        print_phase(&PhaseResult {
            name: "dag",
            passed: true,
            detail: format!("{} edges \u{00B7} acyclic", workflow.flow_count()),
            duration_ms: dag_elapsed.as_millis() as u64,
            errors: vec![],
            hints: vec![],
        });

        // Phase 5: Bindings
        print_phase(&PhaseResult {
            name: "bindings",
            passed: true,
            detail: "all references valid".to_string(),
            duration_ms: bindings_elapsed.as_millis() as u64,
            errors: vec![],
            hints: vec![],
        });

        // Phase 6: Schemas
        let schemas_detail = if schema_count > 0 {
            format!("{schema_count} validated")
        } else {
            "none required".to_string()
        };
        print_phase(&PhaseResult {
            name: "schemas",
            passed: true,
            detail: schemas_detail,
            duration_ms: schemas_elapsed.as_millis() as u64,
            errors: vec![],
            hints: vec![],
        });

        // Phase 7: Security
        if security_hints.is_empty() {
            print_phase(&PhaseResult {
                name: "security",
                passed: true,
                detail: "no issues".to_string(),
                duration_ms: security_elapsed.as_millis() as u64,
                errors: vec![],
                hints: vec![],
            });
        } else {
            print_phase(&PhaseResult {
                name: "security",
                passed: true, // warnings, not errors
                detail: format!("{} hint(s)", security_hints.len()),
                duration_ms: security_elapsed.as_millis() as u64,
                errors: vec![],
                hints: security_hints,
            });
        }

        // Phase 8: Provider API keys
        if provider_warnings.is_empty() {
            print_phase(&PhaseResult {
                name: "providers",
                passed: true,
                detail: "all API keys present".to_string(),
                duration_ms: providers_elapsed.as_millis() as u64,
                errors: vec![],
                hints: vec![],
            });
        } else {
            print_phase(&PhaseResult {
                name: "providers",
                passed: false,
                detail: format!("{} missing API key(s)", provider_warnings.len()),
                duration_ms: providers_elapsed.as_millis() as u64,
                errors: provider_warnings.clone(),
                hints: vec!["Run 'nika keys set <name>' to configure API keys".to_string()],
            });
        }

        // Phase 9: Nika Shield taint analysis (--security flag)
        if let Some(ref report) = taint_report {
            let summary = report.trust_summary();
            let trusted = summary.get(&nika::trust::TrustLevel::Trusted).unwrap_or(&0);
            let generated = summary
                .get(&nika::trust::TrustLevel::ModelGenerated)
                .unwrap_or(&0);
            let tainted = summary
                .get(&nika::trust::TrustLevel::ModelTainted)
                .unwrap_or(&0);
            let untrusted = summary
                .get(&nika::trust::TrustLevel::Untrusted)
                .unwrap_or(&0);

            if report.is_clean() {
                print_phase(&PhaseResult {
                    name: "shield",
                    passed: true,
                    detail: format!(
                        "{trusted} Trusted \u{00B7} {generated} ModelGenerated \u{00B7} {tainted} ModelTainted \u{00B7} {untrusted} Untrusted"
                    ),
                    duration_ms: 0,
                    errors: vec![],
                    hints: vec![],
                });
            } else {
                let warning_msgs: Vec<String> = report
                    .warnings
                    .iter()
                    .map(|w| {
                        format!(
                            "[{}] {}\n         Recommendation: {}",
                            w.code(),
                            w.message(),
                            w.recommendation()
                        )
                    })
                    .collect();
                print_phase(&PhaseResult {
                    name: "shield",
                    passed: true, // warnings, not hard errors
                    detail: format!(
                        "{} warning(s) \u{00B7} {trusted}T {generated}G {tainted}M {untrusted}U",
                        report.warnings.len()
                    ),
                    duration_ms: 0,
                    errors: vec![],
                    hints: warning_msgs,
                });
            }
        }

        // Show DAG visualization for multi-task workflows
        if workflow.tasks.len() > 1 {
            use nika::display::{render_dag, DagTask, DagTaskStatus};
            use std::collections::HashMap;

            let dag_tasks: Vec<DagTask> = workflow
                .tasks
                .iter()
                .map(|t| DagTask {
                    id: t.id.clone(),
                    verb: t.action.verb_name().to_string(),
                    status: DagTaskStatus::Pending,
                    meta: None,
                    tags: Vec::new(),
                })
                .collect();

            let mut deps_map: HashMap<String, Vec<String>> = HashMap::new();
            for task in &workflow.tasks {
                if let Some(ref task_deps) = task.depends_on {
                    deps_map.insert(task.id.clone(), task_deps.clone());
                }
            }

            render_dag(&dag_tasks, &deps_map);
        }

        // Compute layer count for summary
        let layer_count = {
            let mut depths: std::collections::HashMap<&str, usize> =
                workflow.tasks.iter().map(|t| (t.id.as_str(), 0)).collect();
            let mut changed = true;
            let mut iters = 0;
            while changed && iters < 100 {
                changed = false;
                iters += 1;
                for task in &workflow.tasks {
                    if let Some(ref task_deps) = task.depends_on {
                        for dep in task_deps {
                            if let Some(&dep_depth) = depths.get(dep.as_str()) {
                                let new_depth = dep_depth + 1;
                                if new_depth > depths[task.id.as_str()] {
                                    depths.insert(&task.id, new_depth);
                                    changed = true;
                                }
                            }
                        }
                    }
                }
            }
            depths.values().max().copied().unwrap_or(0) + 1
        };

        // Analyzer warnings (surfaced from analyze phase)
        print_check_warnings(&analyzer_warnings);

        // Summary footer
        println!();
        print_check_summary(
            true,
            total_start.elapsed().as_millis() as u64,
            workflow.tasks.len(),
            workflow.flow_count(),
            layer_count,
            schema_count,
            None,
            &[],
        );
    }

    Ok(())
}

/// Validate a schema file exists and contains valid JSON.
async fn validate_schema_file(
    task_id: &str,
    path: &str,
    base_path: &Path,
) -> Result<(), NikaError> {
    let resolved = base_path.join(path);
    if !resolved.exists() {
        return Err(NikaError::SchemaFileNotFound {
            task_id: task_id.to_string(),
            path: path.to_string(),
        });
    }

    let content =
        tokio::fs::read_to_string(&resolved)
            .await
            .map_err(|e| NikaError::SchemaFileNotFound {
                task_id: task_id.to_string(),
                path: format!("{path}: {e}"),
            })?;

    serde_json::from_str::<serde_json::Value>(&content).map_err(|e| {
        NikaError::SchemaFileInvalid {
            task_id: task_id.to_string(),
            path: path.to_string(),
            reason: e.to_string(),
        }
    })?;

    Ok(())
}

/// Validate a workflow with --strict mode (connects to MCP servers)
async fn validate_workflow_strict(file: &str) -> Result<(), NikaError> {
    use nika::display::{
        print_check_header, print_check_summary, print_mcp_validation, print_phase,
        print_phase_skipped, McpCallValidation, McpCheckResult, McpParamError, PhaseResult,
    };
    use std::time::Instant;

    let total_start = Instant::now();
    let resolved_path = resolve_workflow_path(file).await?;

    let yaml = tokio::fs::read_to_string(&resolved_path).await?;

    // Phase 1: Schema validation
    let t = Instant::now();
    let schema_validator = WorkflowSchemaValidator::new()?;
    schema_validator.validate_yaml(&yaml)?;
    let schema_elapsed = t.elapsed();

    // Phase 2: Parse
    let t = Instant::now();
    let base_path = resolved_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let workflow = parse_workflow_with_includes(&yaml, base_path)?;
    let parse_elapsed = t.elapsed();
    let includes_elapsed = std::time::Duration::ZERO;

    // Phase 4: DAG
    let t = Instant::now();
    let flow_graph = Dag::from_workflow(&workflow)?;
    let dag_cycle_result = flow_graph.detect_cycles();
    let dag_elapsed = t.elapsed();

    if let Err(ref e) = dag_cycle_result {
        print_check_header(file, true, env!("CARGO_PKG_VERSION"));
        print_phase(&PhaseResult {
            name: "schema",
            passed: true,
            detail: format!("YAML valid against @{}", workflow.schema),
            duration_ms: schema_elapsed.as_millis() as u64,
            errors: vec![],
            hints: vec![],
        });
        print_phase(&PhaseResult {
            name: "parse",
            passed: true,
            detail: format!(
                "{} tasks \u{00B7} provider: {} \u{00B7} model: {}",
                workflow.tasks.len(),
                workflow.provider,
                workflow.model.as_deref().unwrap_or("(default)")
            ),
            duration_ms: parse_elapsed.as_millis() as u64,
            errors: vec![],
            hints: vec![],
        });
        print_phase(&PhaseResult {
            name: "includes",
            passed: true,
            detail: "resolved".to_string(),
            duration_ms: includes_elapsed.as_millis() as u64,
            errors: vec![],
            hints: vec![],
        });
        print_phase(&PhaseResult {
            name: "dag",
            passed: false,
            detail: "CYCLE DETECTED".to_string(),
            duration_ms: dag_elapsed.as_millis() as u64,
            errors: vec![e.to_string()],
            hints: vec![
                "Remove one dependency to break the cycle.".to_string(),
                "Common fix: use with: binding instead of depends_on.".to_string(),
            ],
        });
        print_phase_skipped("bindings", "DAG invalid");
        print_phase_skipped("schemas", "DAG invalid");
        println!();
        print_check_summary(
            false,
            total_start.elapsed().as_millis() as u64,
            workflow.tasks.len(),
            workflow.flow_count(),
            0,
            0,
            None,
            &[("NIKA-020", "Circular dependency detected")],
        );
        return dag_cycle_result;
    }

    // Phase 5: Bindings
    let t = Instant::now();
    validate_bindings(&workflow, &flow_graph)?;
    let bindings_elapsed = t.elapsed();

    // Phase 6: Validate structured output schema files
    let t = Instant::now();
    let mut schema_count = 0u32;
    for task in &workflow.tasks {
        if let Some(ref output) = task.output {
            if let Some(SchemaRef::File(ref path)) = output.schema {
                validate_schema_file(&task.id, path, base_path).await?;
                schema_count += 1;
            }
        }
        if let Some(ref spec) = task.structured {
            if let Some(SchemaRef::File(ref path)) = spec.schema {
                validate_schema_file(&task.id, path, base_path).await?;
                schema_count += 1;
            }
        }
    }
    let schemas_elapsed = t.elapsed();

    // Phase 7: Security hints (shell escape warnings)
    let t = Instant::now();
    let mut strict_security_hints: Vec<String> = Vec::new();
    {
        let binding_re = regex::Regex::new(r"\{\{(with\.[^}]+|inputs\.[^}]+)\}\}").unwrap();
        for task in &workflow.tasks {
            if let TaskAction::Exec { exec } = &task.action {
                if exec.shell == Some(true) {
                    for cap in binding_re.captures_iter(&exec.command) {
                        let inner = &cap[1];
                        if !inner.contains("| shell") {
                            strict_security_hints.push(format!(
                                "task '{}': shell: true with unescaped {{{{{}}}}} — use | shell transform",
                                task.id, inner
                            ));
                        }
                    }
                }
            }
        }
    }
    let strict_security_elapsed = t.elapsed();

    // Phase 8: Provider API keys
    let t = Instant::now();
    let mut provider_warnings: Vec<String> = Vec::new();
    {
        let mut providers_used: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        providers_used.insert(workflow.provider.to_string());

        if let Ok(analyzed) = parse_analyzed(&yaml) {
            for task in &analyzed.tasks {
                if let Some(ref p) = task.provider {
                    providers_used.insert(p.to_string());
                }
            }
        }

        for provider_name in &providers_used {
            if let Some(provider) = nika::core::find_provider(provider_name) {
                if provider.requires_key && !nika::secrets::has_provider_key(provider) {
                    provider_warnings.push(format!(
                        "{} not set (provider '{}' used in workflow)",
                        provider.env_var, provider_name
                    ));
                }
            }
        }
    }
    let providers_elapsed = t.elapsed();

    // MCP parameter validation (strict mode)
    let invoke_tasks: Vec<_> = workflow
        .tasks
        .iter()
        .filter_map(|t| {
            if let TaskAction::Invoke { invoke: ref params } = t.action {
                Some((t.id.as_str(), params))
            } else {
                None
            }
        })
        .collect();

    // Also collect agent tasks that reference MCP servers
    let agent_tasks: Vec<(&str, Vec<String>)> = workflow
        .tasks
        .iter()
        .filter_map(|t| {
            if let TaskAction::Agent { agent: ref params } = t.action {
                if !params.mcp.is_empty() {
                    Some((t.id.as_str(), params.mcp.clone()))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    let mut mcp_results: Vec<McpCheckResult> = Vec::new();
    let mut all_valid = true;
    let mut total_calls = 0u32;
    let mut valid_calls = 0u32;
    let mut total_param_errors = 0u32;

    if !invoke_tasks.is_empty() || !agent_tasks.is_empty() {
        let mcp_validator = McpValidator::new(ValidationConfig::default());

        let mut mcp_servers: std::collections::HashSet<&str> = invoke_tasks
            .iter()
            .filter_map(|(_, p)| p.mcp.as_deref())
            .collect();

        // Add MCP servers referenced by agent tasks
        for (_, servers) in &agent_tasks {
            for server in servers {
                mcp_servers.insert(server.as_str());
            }
        }

        let mcp_configs = workflow
            .mcp
            .as_ref()
            .ok_or_else(|| NikaError::ValidationError {
                reason: "Workflow has invoke tasks but no mcp: configuration".to_string(),
            })?;

        for server_name in mcp_servers {
            let Some(inline_config) = mcp_configs.get::<str>(server_name) else {
                return Err(NikaError::McpNotConnected {
                    name: server_name.to_string(),
                });
            };

            let connect_start = Instant::now();

            let mut config = McpConfig::new(server_name, &inline_config.command)
                .with_args(inline_config.args.iter().cloned());
            for (key, value) in &inline_config.env {
                config = config.with_env(key, value);
            }
            if let Some(ref cwd) = inline_config.cwd {
                config = config.with_cwd(cwd);
            }

            let client = McpClient::new(config)?;
            client.connect().await?;

            let tools = client.list_tools().await?;
            let connect_ms = connect_start.elapsed().as_millis() as u64;

            mcp_validator.cache().populate(server_name, &tools)?;

            // Validate invoke tasks targeting this server
            let mut validations: Vec<McpCallValidation> = Vec::new();
            for (task_id, params) in &invoke_tasks {
                if params.mcp.as_deref() != Some(server_name) {
                    continue;
                }
                total_calls += 1;

                if let Some(ref tool) = params.tool {
                    let invoke_params = params.params.clone().unwrap_or_default();
                    let result = mcp_validator.validate(server_name, tool, &invoke_params);

                    if result.is_valid {
                        valid_calls += 1;
                        validations.push(McpCallValidation {
                            task_id: task_id.to_string(),
                            tool_name: tool.clone(),
                            valid: true,
                            errors: vec![],
                        });
                    } else {
                        all_valid = false;
                        let errors: Vec<McpParamError> = result
                            .errors
                            .iter()
                            .map(|e| McpParamError {
                                path: e.path.clone(),
                                message: e.message.clone(),
                            })
                            .collect();
                        total_param_errors += errors.len() as u32;
                        validations.push(McpCallValidation {
                            task_id: task_id.to_string(),
                            tool_name: tool.clone(),
                            valid: false,
                            errors,
                        });
                    }
                } else {
                    // Resource read -- no params to validate
                    valid_calls += 1;
                    validations.push(McpCallValidation {
                        task_id: task_id.to_string(),
                        tool_name: "(resource read)".to_string(),
                        valid: true,
                        errors: vec![],
                    });
                }
            }

            // Agent tasks — connectivity validated, tools are dynamic
            for (task_id, servers) in &agent_tasks {
                if servers.iter().any(|s| s.as_str() == server_name) {
                    total_calls += 1;
                    valid_calls += 1;
                    validations.push(McpCallValidation {
                        task_id: task_id.to_string(),
                        tool_name: "(agent: dynamic tools)".to_string(),
                        valid: true,
                        errors: vec![],
                    });
                }
            }

            mcp_results.push(McpCheckResult {
                server_name: server_name.to_string(),
                tool_count: tools.len(),
                connect_ms,
                validations,
            });
        }
    }

    // Print all output
    print_check_header(file, true, env!("CARGO_PKG_VERSION"));

    // Phase 1: Schema
    print_phase(&PhaseResult {
        name: "schema",
        passed: true,
        detail: format!("YAML valid against @{}", workflow.schema),
        duration_ms: schema_elapsed.as_millis() as u64,
        errors: vec![],
        hints: vec![],
    });

    // Phase 2: Parse
    print_phase(&PhaseResult {
        name: "parse",
        passed: true,
        detail: format!(
            "{} tasks \u{00B7} provider: {} \u{00B7} model: {}",
            workflow.tasks.len(),
            workflow.provider,
            workflow.model.as_deref().unwrap_or("(default)")
        ),
        duration_ms: parse_elapsed.as_millis() as u64,
        errors: vec![],
        hints: vec![],
    });

    // Phase 3: Includes
    print_phase(&PhaseResult {
        name: "includes",
        passed: true,
        detail: "resolved".to_string(),
        duration_ms: includes_elapsed.as_millis() as u64,
        errors: vec![],
        hints: vec![],
    });

    // Phase 4: DAG
    print_phase(&PhaseResult {
        name: "dag",
        passed: true,
        detail: format!("{} edges \u{00B7} acyclic", workflow.flow_count()),
        duration_ms: dag_elapsed.as_millis() as u64,
        errors: vec![],
        hints: vec![],
    });

    // Phase 5: Bindings
    print_phase(&PhaseResult {
        name: "bindings",
        passed: true,
        detail: "all references valid".to_string(),
        duration_ms: bindings_elapsed.as_millis() as u64,
        errors: vec![],
        hints: vec![],
    });

    // Phase 6: Schemas
    let schemas_detail = if schema_count > 0 {
        format!("{schema_count} validated")
    } else {
        "none required".to_string()
    };
    print_phase(&PhaseResult {
        name: "schemas",
        passed: true,
        detail: schemas_detail,
        duration_ms: schemas_elapsed.as_millis() as u64,
        errors: vec![],
        hints: vec![],
    });

    // Phase 7: Security
    if strict_security_hints.is_empty() {
        print_phase(&PhaseResult {
            name: "security",
            passed: true,
            detail: "no issues".to_string(),
            duration_ms: strict_security_elapsed.as_millis() as u64,
            errors: vec![],
            hints: vec![],
        });
    } else {
        print_phase(&PhaseResult {
            name: "security",
            passed: true, // warnings, not errors
            detail: format!("{} hint(s)", strict_security_hints.len()),
            duration_ms: strict_security_elapsed.as_millis() as u64,
            errors: vec![],
            hints: strict_security_hints,
        });
    }

    // Phase 8: Provider API keys
    if provider_warnings.is_empty() {
        print_phase(&PhaseResult {
            name: "providers",
            passed: true,
            detail: "all API keys present".to_string(),
            duration_ms: providers_elapsed.as_millis() as u64,
            errors: vec![],
            hints: vec![],
        });
    } else {
        print_phase(&PhaseResult {
            name: "providers",
            passed: false,
            detail: format!("{} missing API key(s)", provider_warnings.len()),
            duration_ms: providers_elapsed.as_millis() as u64,
            errors: provider_warnings,
            hints: vec!["Run 'nika keys set <name>' to configure API keys".to_string()],
        });
    }

    // MCP Validation section
    if !mcp_results.is_empty() {
        print_mcp_validation(&mcp_results);
    }

    // Show DAG visualization for multi-task workflows
    if workflow.tasks.len() > 1 {
        use nika::display::{render_dag, DagTask, DagTaskStatus};
        use std::collections::HashMap;

        // Build a set of task IDs that failed MCP validation
        let failed_task_ids: std::collections::HashSet<String> = mcp_results
            .iter()
            .flat_map(|r| &r.validations)
            .filter(|v| !v.valid)
            .map(|v| v.task_id.clone())
            .collect();

        let dag_tasks: Vec<DagTask> = workflow
            .tasks
            .iter()
            .map(|t| {
                let status = if failed_task_ids.contains(&t.id) {
                    DagTaskStatus::Failed
                } else if invoke_tasks.iter().any(|(id, _)| *id == t.id)
                    || agent_tasks.iter().any(|(id, _)| *id == t.id)
                {
                    DagTaskStatus::Success
                } else {
                    DagTaskStatus::Pending
                };
                DagTask {
                    id: t.id.clone(),
                    verb: t.action.verb_name().to_string(),
                    status,
                    meta: None,
                    tags: Vec::new(),
                }
            })
            .collect();

        let mut deps_map: HashMap<String, Vec<String>> = HashMap::new();
        for task in &workflow.tasks {
            if let Some(ref task_deps) = task.depends_on {
                deps_map.insert(task.id.clone(), task_deps.clone());
            }
        }

        render_dag(&dag_tasks, &deps_map);
    }

    // Compute layer count for summary
    let layer_count = {
        let mut depths: std::collections::HashMap<&str, usize> =
            workflow.tasks.iter().map(|t| (t.id.as_str(), 0)).collect();
        let mut changed = true;
        let mut iters = 0;
        while changed && iters < 100 {
            changed = false;
            iters += 1;
            for task in &workflow.tasks {
                if let Some(ref task_deps) = task.depends_on {
                    for dep in task_deps {
                        if let Some(&dep_depth) = depths.get(dep.as_str()) {
                            let new_depth = dep_depth + 1;
                            if new_depth > depths[task.id.as_str()] {
                                depths.insert(&task.id, new_depth);
                                changed = true;
                            }
                        }
                    }
                }
            }
        }
        depths.values().max().copied().unwrap_or(0) + 1
    };

    // Build error codes for summary
    let mut error_codes: Vec<(&str, &str)> = Vec::new();
    if !all_valid {
        error_codes.push((
            "NIKA-100",
            "Strict validation failed: invoke parameter mismatch",
        ));
    }

    // Strict info for summary
    let strict_info = if total_calls > 0 {
        Some((valid_calls, total_calls, total_param_errors))
    } else {
        None
    };

    // Summary footer
    println!();
    print_check_summary(
        all_valid,
        total_start.elapsed().as_millis() as u64,
        workflow.tasks.len(),
        workflow.flow_count(),
        layer_count,
        schema_count,
        strict_info,
        &error_codes,
    );

    if !all_valid {
        return Err(NikaError::ValidationError {
            reason: "Strict validation failed: invoke parameters don't match tool schemas"
                .to_string(),
        });
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// TASK FILTERING (--task / --from)
// ═══════════════════════════════════════════════════════════════════════════

/// Filter workflow to keep only the target task and its transitive dependencies.
fn filter_tasks_for_target(
    workflow: &mut nika::ast::analyzed::AnalyzedWorkflow,
    target_id: &str,
) -> Result<(), NikaError> {
    // Verify target exists
    if !workflow.tasks.iter().any(|t| t.name == target_id) {
        return Err(NikaError::ValidationError {
            reason: format!(
                "Task '{}' not found. Available: {}",
                target_id,
                workflow
                    .tasks
                    .iter()
                    .map(|t| t.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        });
    }

    // BFS to collect transitive dependencies
    let mut required: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut queue: std::collections::VecDeque<String> = std::collections::VecDeque::new();
    required.insert(target_id.to_string());
    queue.push_back(target_id.to_string());

    while let Some(task_name) = queue.pop_front() {
        if let Some(task) = workflow.tasks.iter().find(|t| t.name == task_name) {
            // Explicit depends_on
            for dep_id in &task.depends_on {
                if let Some(dep_name) = workflow.task_table.get_name(*dep_id) {
                    if required.insert(dep_name.to_string()) {
                        queue.push_back(dep_name.to_string());
                    }
                }
            }
            // Implicit deps from with: bindings
            for dep_id in &task.implicit_deps {
                if let Some(dep_name) = workflow.task_table.get_name(*dep_id) {
                    if required.insert(dep_name.to_string()) {
                        queue.push_back(dep_name.to_string());
                    }
                }
            }
        }
    }

    workflow
        .tasks
        .retain(|t| required.contains(t.name.as_str()));
    Ok(())
}

/// Filter workflow to keep the from task and all its transitive successors.
///
/// Uses forward BFS through successor edges (inverse of dependency traversal).
fn filter_tasks_from(
    workflow: &mut nika::ast::analyzed::AnalyzedWorkflow,
    from_id: &str,
) -> Result<(), NikaError> {
    if !workflow.tasks.iter().any(|t| t.name == from_id) {
        return Err(NikaError::ValidationError {
            reason: format!(
                "Task '{}' not found. Available: {}",
                from_id,
                workflow
                    .tasks
                    .iter()
                    .map(|t| t.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        });
    }

    // Forward BFS: find from_id + all transitive successors
    let mut keep: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut queue: std::collections::VecDeque<String> = std::collections::VecDeque::new();
    keep.insert(from_id.to_string());
    queue.push_back(from_id.to_string());

    while let Some(current) = queue.pop_front() {
        for task in &workflow.tasks {
            // Check if this task depends on `current` (making it a successor)
            let is_successor =
                task.depends_on
                    .iter()
                    .chain(task.implicit_deps.iter())
                    .any(|dep_id| {
                        workflow
                            .task_table
                            .get_name(*dep_id)
                            .is_some_and(|name| name == current)
                    });
            if is_successor && keep.insert(task.name.clone()) {
                queue.push_back(task.name.clone());
            }
        }
    }

    workflow.tasks.retain(|t| keep.contains(t.name.as_str()));
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// AUTO-DISCOVER + INTERACTIVE HELPERS
// ═══════════════════════════════════════════════════════════════════════════

/// Auto-discover a workflow when no file argument is provided.
async fn resolve_or_discover_workflow(quiet: bool) -> Result<String, NikaError> {
    let workflows = discover_workflows(".").await?;
    match workflows.len() {
        0 => Err(NikaError::ValidationError {
            reason: "No .nika.yaml files found in current directory. Try: nika init".to_string(),
        }),
        1 => {
            if !quiet {
                eprintln!("  {} {}", "Auto-discovered:".dimmed(), workflows[0]);
            }
            Ok(workflows.into_iter().next().expect("len checked == 1"))
        }
        _ => pick_workflow(&workflows),
    }
}

/// Discover .nika.yaml files in the given directory (non-recursive).
async fn discover_workflows(dir: &str) -> Result<Vec<String>, NikaError> {
    let mut entries = tokio::fs::read_dir(dir)
        .await
        .map_err(|e| NikaError::ParseError {
            details: format!("Failed to read directory '{}': {}", dir, e),
        })?;
    let mut found = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| NikaError::ParseError {
            details: format!("Failed to read directory entry: {}", e),
        })?
    {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".nika.yaml") {
            found.push(name);
        }
    }
    found.sort();
    Ok(found)
}

/// Interactive workflow picker when multiple .nika.yaml files found.
fn pick_workflow(workflows: &[String]) -> Result<String, NikaError> {
    if !std::io::stdin().is_terminal() {
        return Err(NikaError::ValidationError {
            reason: format!(
                "Multiple .nika.yaml files found ({}). Specify one: nika run <file>",
                workflows.join(", ")
            ),
        });
    }
    let items: Vec<(String, String, &str)> = workflows
        .iter()
        .map(|w| (w.clone(), w.clone(), ""))
        .collect();
    let selected: String = cliclack::select("Which workflow?")
        .items(&items)
        .interact()
        .map_err(|e| NikaError::ValidationError {
            reason: format!("Workflow selection cancelled: {}", e),
        })?;
    Ok(selected)
}

// ═══════════════════════════════════════════════════════════════════════════
// DRY RUN
// ═══════════════════════════════════════════════════════════════════════════

/// Lightweight {{inputs.X}} template substitution for dry-run display.
/// Only resolves `{{inputs.<key>}}` patterns — no full datastore needed.
fn simple_input_resolve<'a, I>(template: &str, inputs: I) -> String
where
    I: IntoIterator<Item = (&'a String, &'a serde_json::Value)>,
{
    let mut result = template.to_string();
    for (key, value) in inputs {
        let pattern = format!("{{{{inputs.{}}}}}", key);
        if result.contains(&pattern) {
            let replacement = match value {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            result = result.replace(&pattern, &replacement);
        }
    }
    result
}

/// Show execution plan without running anything.
#[allow(clippy::too_many_arguments)]
async fn dry_run_workflow(
    file: &str,
    provider_override: Option<String>,
    model_override: Option<String>,
    cli_inputs: &[String],
    input_file: Option<&str>,
    task_filter: Option<&str>,
    from_filter: Option<&str>,
) -> Result<(), NikaError> {
    let resolved_path = resolve_workflow_path(file).await?;
    let yaml = tokio::fs::read_to_string(&resolved_path).await?;
    let validator = WorkflowSchemaValidator::new()?;
    validator.validate_yaml(&yaml)?;
    let base_path = resolved_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut workflow = parse_analyzed_with_includes(&yaml, base_path)?;

    // Apply overrides
    if let Some(p) = provider_override {
        workflow.provider = Some(p.into());
    }
    if let Some(m) = model_override {
        workflow.model = Some(m);
    }
    if let Some(ifp) = input_file {
        let file_inputs = load_input_file(ifp).await?;
        for (k, v) in file_inputs {
            workflow.inputs.insert(k, v);
        }
    }
    if !cli_inputs.is_empty() {
        let parsed = parse_cli_inputs(cli_inputs)?;
        for (k, v) in parsed {
            workflow.inputs.insert(k, v);
        }
    }

    // Apply task filters
    if let Some(target) = task_filter {
        filter_tasks_for_target(&mut workflow, target)?;
    } else if let Some(from_id) = from_filter {
        filter_tasks_from(&mut workflow, from_id)?;
    }

    // Header
    println!("\n  {}", "DRY RUN — no tasks will execute".yellow().bold());
    println!();

    // Show resolved inputs
    if !workflow.inputs.is_empty() {
        println!("  {}", "Inputs:".bold());
        for (k, v) in &workflow.inputs {
            println!(
                "    {} = {}",
                k.cyan(),
                serde_json::to_string(v).unwrap_or_default()
            );
        }
        println!();
    }

    // Compute DAG layers
    let nodes: Vec<&str> = workflow.tasks.iter().map(|t| t.name.as_str()).collect();
    let edges: Vec<(&str, &str)> = workflow
        .tasks
        .iter()
        .flat_map(|task| {
            task.depends_on.iter().filter_map(|dep_id| {
                workflow
                    .task_table
                    .get_name(*dep_id)
                    .map(|dep_name| (dep_name, task.name.as_str()))
            })
        })
        .collect();
    let depths = nika::dag::flow::compute_layers(&nodes, &edges);
    let max_layer = depths.values().max().copied().unwrap_or(0);

    // Execution plan
    println!("  {}", "Execution Plan:".bold());
    for layer in 0..=max_layer {
        let mut tasks_in_layer: Vec<&str> = depths
            .iter()
            .filter(|(_, &d)| d == layer)
            .map(|(&name, _)| name)
            .collect();
        tasks_in_layer.sort();
        println!(
            "    Layer {} (parallel): {}",
            layer,
            tasks_in_layer.join(", ").cyan()
        );
    }
    println!();

    // Per-task details
    let default_provider = workflow
        .provider
        .as_ref()
        .map(|p| p.as_str())
        .unwrap_or("(auto)");
    let default_model = workflow.model.as_deref().unwrap_or("(default)");

    println!("  {}", "Tasks:".bold());
    for task in &workflow.tasks {
        let verb = task.action.verb_name();
        let resolved_provider = task
            .provider
            .as_ref()
            .map(|p| simple_input_resolve(p.as_str(), &workflow.inputs))
            .unwrap_or_else(|| default_provider.to_string());
        let resolved_model = task
            .model
            .as_deref()
            .map(|m| simple_input_resolve(m, &workflow.inputs))
            .unwrap_or_else(|| default_model.to_string());
        let provider = resolved_provider.as_str();
        let model = resolved_model.as_str();
        let deps: Vec<&str> = task
            .depends_on
            .iter()
            .filter_map(|id| workflow.task_table.get_name(*id))
            .collect();
        let dep_str = if deps.is_empty() {
            String::new()
        } else {
            format!(" deps=[{}]", deps.join(", "))
        };
        println!(
            "    {} [{}] provider={} model={}{}",
            task.name.cyan(),
            verb,
            provider.dimmed(),
            model.dimmed(),
            dep_str.dimmed()
        );
    }
    println!();

    // LLM task count + cost estimate
    let llm_tasks: Vec<_> = workflow
        .tasks
        .iter()
        .filter(|t| matches!(t.action.verb_name(), "infer" | "agent"))
        .collect();
    let infer_count = llm_tasks.len();
    if infer_count > 0 {
        use nika::provider::cost::{calculate_cost, ProviderKind};
        // Estimate: ~500 input tokens, ~1000 output tokens per LLM task
        let mut total_cost = 0.0;
        for task in &llm_tasks {
            let prov_resolved = task
                .provider
                .as_ref()
                .map(|p| simple_input_resolve(p.as_str(), &workflow.inputs))
                .or_else(|| workflow.provider.as_ref().map(|p| p.to_string()))
                .unwrap_or_else(|| "anthropic".to_string());
            let model_resolved = task
                .model
                .as_deref()
                .map(|m| simple_input_resolve(m, &workflow.inputs))
                .or_else(|| workflow.model.clone())
                .unwrap_or_else(|| "claude-sonnet-4-6".to_string());
            let prov_str = prov_resolved.as_str();
            let model_str = model_resolved.as_str();
            let provider_kind = ProviderKind::parse(prov_str).unwrap_or(ProviderKind::Claude);
            total_cost += calculate_cost(provider_kind, model_str, 500, 1000);
        }
        println!(
            "  {} {} LLM tasks, {} total — estimated cost: ${:.4}",
            "Summary:".bold(),
            infer_count,
            workflow.tasks.len(),
            total_cost
        );
    } else {
        println!(
            "  {} {} tasks (no LLM calls)",
            "Summary:".bold(),
            workflow.tasks.len()
        );
    }
    println!();

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// CLI INPUT OVERRIDES
// ═══════════════════════════════════════════════════════════════════════════

/// Smart type coercion for CLI input values.
///
/// Rules (in order):
/// 1. `"true"` / `"false"` → Bool
/// 2. `"null"` → Null
/// 3. Parseable as i64 → integer
/// 4. Parseable as f64 → float
/// 5. Starts with `{` or `[` → try JSON, fallback string
/// 6. Everything else → String
fn parse_input_value(s: &str) -> serde_json::Value {
    use serde_json::Value;
    match s {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        "null" => Value::Null,
        _ => {
            if let Ok(n) = s.parse::<i64>() {
                return serde_json::json!(n);
            }
            // Only coerce to float for plain decimal notation (not scientific like "1e3")
            if !s.contains('e') && !s.contains('E') {
                if let Ok(n) = s.parse::<f64>() {
                    return serde_json::json!(n);
                }
            }
            if s.starts_with('{') || s.starts_with('[') {
                if let Ok(v) = serde_json::from_str::<Value>(s) {
                    return v;
                }
            }
            Value::String(s.to_string())
        }
    }
}

/// Parse `-i key=value` CLI flags into an ordered map.
fn parse_cli_inputs(raw: &[String]) -> Result<Vec<(String, serde_json::Value)>, NikaError> {
    let mut result = Vec::new();
    for item in raw {
        let (key, value) = item.split_once('=').ok_or_else(|| {
            // Detect common mistake: passing a bare value without a key
            let hint = if item.starts_with("http") {
                format!(
                    "Got '{item}' but -i expects KEY=VALUE format.\n\n  Example: nika run workflow.nika.yaml -i url={item}\n\n  Multiple inputs: -i url={item} -i lang=en\n  From file:       --input-file inputs.json",
                    item = item
                )
            } else {
                format!(
                    "Got '{item}' but -i expects KEY=VALUE format.\n\n  Example: nika run workflow.nika.yaml -i name={item}\n\n  Check your workflow's `inputs:` block for the expected key names.",
                    item = item
                )
            };
            NikaError::ValidationError { reason: hint }
        })?;
        result.push((key.to_string(), parse_input_value(value)));
    }
    Ok(result)
}

/// Load inputs from a JSON/YAML file (or stdin with "-").
async fn load_input_file(path: &str) -> Result<Vec<(String, serde_json::Value)>, NikaError> {
    let content = if path == "-" {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|_| NikaError::ParseError {
                details: "Failed to read from stdin".to_string(),
            })?;
        buf
    } else {
        tokio::fs::read_to_string(path)
            .await
            .map_err(|e| NikaError::ParseError {
                details: format!("Failed to read input file '{}': {}", path, e),
            })?
    };

    // Auto-detect format: .json → JSON first, everything else → YAML first
    let value: serde_json::Value = if path.ends_with(".json") {
        serde_json::from_str(&content).map_err(|e| NikaError::ParseError {
            details: format!("Invalid JSON in '{}': {}", path, e),
        })?
    } else if path == "-" {
        // stdin: try JSON first, then YAML
        serde_json::from_str(&content).or_else(|_| {
            nika::serde_yaml::from_str(&content).map_err(|e| NikaError::ParseError {
                details: format!("Invalid JSON/YAML on stdin: {}", e),
            })
        })?
    } else {
        // .yaml, .yml, or anything else → YAML
        nika::serde_yaml::from_str(&content).map_err(|e| NikaError::ParseError {
            details: format!("Invalid YAML in '{}': {}", path, e),
        })?
    };

    // Must be a mapping at top level
    let map = value
        .as_object()
        .ok_or_else(|| NikaError::ValidationError {
            reason: format!(
                "Input file '{}' must be a JSON/YAML mapping (got {})",
                path,
                match &value {
                    serde_json::Value::Array(_) => "array",
                    serde_json::Value::String(_) => "string",
                    serde_json::Value::Number(_) => "number",
                    serde_json::Value::Bool(_) => "boolean",
                    serde_json::Value::Null => "null",
                    _ => "unknown",
                }
            ),
        })?;

    Ok(map.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ═══════════════════════════════════════════════════════════════
    // parse_input_value
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn input_value_string() {
        assert_eq!(parse_input_value("hello"), json!("hello"));
    }

    #[test]
    fn input_value_integer() {
        assert_eq!(parse_input_value("5"), json!(5));
        assert_eq!(parse_input_value("-42"), json!(-42));
        assert_eq!(parse_input_value("0"), json!(0));
    }

    #[test]
    fn input_value_float() {
        assert_eq!(parse_input_value("1.5"), json!(1.5));
        assert_eq!(parse_input_value("-0.5"), json!(-0.5));
    }

    #[test]
    fn input_value_bool() {
        assert_eq!(parse_input_value("true"), json!(true));
        assert_eq!(parse_input_value("false"), json!(false));
    }

    #[test]
    fn input_value_null() {
        assert_eq!(parse_input_value("null"), json!(null));
    }

    #[test]
    fn input_value_json_object() {
        assert_eq!(
            parse_input_value(r#"{"a":1,"b":"x"}"#),
            json!({"a": 1, "b": "x"})
        );
    }

    #[test]
    fn input_value_json_array() {
        assert_eq!(parse_input_value(r#"["x","y"]"#), json!(["x", "y"]));
    }

    #[test]
    fn input_value_broken_json_fallback_string() {
        assert_eq!(parse_input_value("{broken"), json!("{broken"));
        assert_eq!(parse_input_value("[not json"), json!("[not json"));
    }

    #[test]
    fn input_value_string_with_digits() {
        assert_eq!(parse_input_value("5 apples"), json!("5 apples"));
        assert_eq!(parse_input_value("v2.0"), json!("v2.0"));
    }

    // ═══════════════════════════════════════════════════════════════
    // parse_cli_inputs
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn cli_inputs_valid() {
        let raw = vec!["locale=fr-FR".to_string(), "count=5".to_string()];
        let result = parse_cli_inputs(&raw).unwrap();
        assert_eq!(result[0], ("locale".to_string(), json!("fr-FR")));
        assert_eq!(result[1], ("count".to_string(), json!(5)));
    }

    #[test]
    fn cli_inputs_missing_equals() {
        let raw = vec!["no-equals".to_string()];
        let result = parse_cli_inputs(&raw);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("no-equals"),
            "Error should mention the input: {err}"
        );
    }

    #[test]
    fn cli_inputs_value_with_equals() {
        // key=val=ue should split on FIRST = only
        let raw = vec!["url=https://example.com?a=1".to_string()];
        let result = parse_cli_inputs(&raw).unwrap();
        assert_eq!(result[0].1, json!("https://example.com?a=1"));
    }

    #[test]
    fn cli_inputs_empty_value() {
        let raw = vec!["key=".to_string()];
        let result = parse_cli_inputs(&raw).unwrap();
        assert_eq!(result[0].1, json!(""));
    }

    // ═══════════════════════════════════════════════════════════════
    // simple_input_resolve
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn simple_input_resolve_replaces_template() {
        let mut inputs = std::collections::HashMap::new();
        inputs.insert("model".to_string(), json!("gpt-4o"));
        assert_eq!(simple_input_resolve("{{inputs.model}}", &inputs), "gpt-4o");
    }

    #[test]
    fn simple_input_resolve_no_template() {
        let inputs: std::collections::HashMap<String, serde_json::Value> =
            std::collections::HashMap::new();
        assert_eq!(simple_input_resolve("openai", &inputs), "openai");
    }

    #[test]
    fn simple_input_resolve_unresolved_stays() {
        let inputs: std::collections::HashMap<String, serde_json::Value> =
            std::collections::HashMap::new();
        assert_eq!(
            simple_input_resolve("{{inputs.missing}}", &inputs),
            "{{inputs.missing}}"
        );
    }

    #[test]
    fn simple_input_resolve_numeric_value() {
        let mut inputs = std::collections::HashMap::new();
        inputs.insert("count".to_string(), json!(42));
        assert_eq!(simple_input_resolve("{{inputs.count}}", &inputs), "42");
    }

    // ═══════════════════════════════════════════════════════════════
    // CLI arg validation
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn cli_args_no_short_option_conflicts() {
        // Verify all clap subcommands parse without short-option conflicts
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }

    // ═══════════════════════════════════════════════════════════════
    // Golden file comparison
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn normalize_golden_strips_duration_ms() {
        let input = json!({
            "task1": {
                "output": "hello",
                "status": "Success",
                "duration_ms": 42
            }
        });
        let normalized = normalize_golden(&input);
        assert_eq!(
            normalized,
            json!({
                "task1": {
                    "output": "hello",
                    "status": "Success"
                }
            })
        );
    }

    #[test]
    fn normalize_golden_strips_nested_duration() {
        let input = json!({
            "outer": {
                "inner": { "duration_ms": 100, "value": 42 },
                "duration_ms": 200
            }
        });
        let normalized = normalize_golden(&input);
        assert!(!normalized.to_string().contains("duration_ms"));
        assert_eq!(normalized["outer"]["inner"]["value"], json!(42));
    }

    #[test]
    fn normalize_golden_preserves_arrays() {
        let input = json!([
            { "output": "a", "duration_ms": 1 },
            { "output": "b", "duration_ms": 2 }
        ]);
        let normalized = normalize_golden(&input);
        assert_eq!(normalized, json!([{ "output": "a" }, { "output": "b" }]));
    }

    #[test]
    fn compare_golden_identical_returns_empty() {
        let a = json!({"task1": {"output": "hello"}});
        let b = json!({"task1": {"output": "hello"}});
        let diffs = compare_golden(&a, &b, "$");
        assert!(diffs.is_empty(), "Expected no diffs, got: {diffs:?}");
    }

    #[test]
    fn compare_golden_detects_value_mismatch() {
        let actual = json!({"task1": {"output": "hello"}});
        let expected = json!({"task1": {"output": "world"}});
        let diffs = compare_golden(&actual, &expected, "$");
        assert_eq!(diffs.len(), 1);
        assert!(diffs[0].contains("task1"));
        assert!(diffs[0].contains("output"));
        assert!(diffs[0].contains("value mismatch"));
    }

    #[test]
    fn compare_golden_detects_missing_key() {
        let actual = json!({"task1": {}});
        let expected = json!({"task1": {"output": "hello"}});
        let diffs = compare_golden(&actual, &expected, "$");
        assert!(!diffs.is_empty());
        assert!(diffs.iter().any(|d| d.contains("missing")));
    }

    #[test]
    fn compare_golden_detects_unexpected_key() {
        let actual = json!({"task1": {"output": "hello", "extra": true}});
        let expected = json!({"task1": {"output": "hello"}});
        let diffs = compare_golden(&actual, &expected, "$");
        assert!(!diffs.is_empty());
        assert!(diffs.iter().any(|d| d.contains("unexpected")));
    }

    #[test]
    fn compare_golden_detects_array_length_mismatch() {
        let actual = json!({"items": [1, 2, 3]});
        let expected = json!({"items": [1, 2]});
        let diffs = compare_golden(&actual, &expected, "$");
        assert!(!diffs.is_empty());
        assert!(diffs.iter().any(|d| d.contains("array length")));
    }
}
