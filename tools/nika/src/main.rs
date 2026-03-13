//! Nika CLI - DAG workflow runner

use clap::{ArgAction, CommandFactory, Parser, Subcommand, ValueEnum};
use colored::Colorize;
use std::fs;
use std::path::{Path, PathBuf};

// Import from lib modules
use nika::ast::schema_validator::WorkflowSchemaValidator;
use nika::ast::{expand_includes, TaskAction, Workflow};
use nika::dag::{validate_use_wiring, Dag};
use nika::error::NikaError;
use nika::init::{
    get_all_context_files, get_all_partials, get_all_schemas, get_all_workflows, WORKFLOWS_README,
};
use nika::mcp::validation::{McpValidator, ValidationConfig};
use nika::mcp::{McpClient, McpConfig};
use nika::registry::resolver; // Package resolution
use nika::runtime::Runner;
use nika::serde_yaml; // serde-saphyr alias (replaces deprecated serde_yaml)
use nika::tools::PermissionMode;
use nika::Event;

// ═══════════════════════════════════════════════════════════════════════════
// HELP TEXT
// ═══════════════════════════════════════════════════════════════════════════

const LONG_ABOUT: &str = r#"Nika - DAG workflow runner for AI tasks with MCP integration

Execute YAML-defined workflows using 5 semantic verbs:
  infer:   LLM text generation (Claude, OpenAI, Mistral, Groq, DeepSeek, Native)
  exec:    Shell command execution
  fetch:   HTTP requests
  invoke:  MCP tool calls
  agent:   Multi-turn agentic loops

Terminal-first design: simple commands for simple tasks, TUI for complex interactions."#;

const AFTER_HELP: &str = r#"QUICK START:
    nika workflow.nika.yaml       Run a workflow (streaming output)
    nika ui                       Open interactive TUI
    nika init                     Initialize new project (.nika/)

WORKFLOW EXECUTION:
    nika <file.nika.yaml>         Run workflow directly
    nika run <file> --provider x  Run with provider override
    nika check <file>             Validate syntax and DAG
    nika check <file> --strict    Validate + test MCP connections

INTERACTIVE MODES:
    nika ui                       TUI (Explorer view by default)
    nika ui --view=chat           TUI Chat view
    nika ui --view=editor         TUI Editor view
    nika chat                     TUI Chat (shortcut)
    nika studio [file]            TUI Editor (shortcut)

CONFIGURATION:
    nika config list              Show all config values
    nika config get editor.theme  Get specific value
    nika config set editor.theme dark
    nika config edit              Open in $EDITOR
    nika config path              Show config file path

SHELL COMPLETION:
    nika completion bash > ~/.local/share/bash-completion/completions/nika
    nika completion zsh > ~/.zfunc/_nika
    nika completion fish > ~/.config/fish/completions/nika.fish

PROVIDER MANAGEMENT:
    nika provider list            Show providers and API key status
    nika provider set anthropic   Store key in system keychain
    nika provider test openai     Test provider connection
    nika provider migrate         Move env vars to keychain

MCP SERVER MANAGEMENT:
    nika mcp list -w flow.yaml    List servers in workflow
    nika mcp test flow.yaml srv   Test server connection
    nika mcp tools flow.yaml srv  List available tools

TRACES:
    nika trace list               List execution traces
    nika trace show <id>          Show trace details
    nika trace export <id>        Export to JSON/YAML

GLOBAL FLAGS:
    -v, --verbose                 Increase verbosity (-v, -vv, -vvv)
    -q, --quiet                   Suppress non-error output
    --color <auto|always|never>   Control color output

ENVIRONMENT VARIABLES:
    ANTHROPIC_API_KEY             Claude (preferred)
    OPENAI_API_KEY                OpenAI
    MISTRAL_API_KEY               Mistral
    GROQ_API_KEY                  Groq
    DEEPSEEK_API_KEY              DeepSeek
    NIKA_MODEL_PATH               Native inference model path (v0.27)

TUI VIEWS (in nika ui):
    [e] Explorer   File browser + DAG preview
    [c] Chat       AI agent conversation
    [d] Editor     YAML workflow editor
    [r] Runner     Real-time execution

DOCUMENTATION:
    https://github.com/SuperNovae-studio/nika"#;

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

#[derive(Parser)]
#[command(name = "nika")]
#[command(version)]
#[command(about = "Nika - DAG workflow runner for AI tasks")]
#[command(long_about = LONG_ABOUT)]
#[command(after_help = AFTER_HELP)]
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

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Launch interactive TUI (terminal UI)
    #[cfg(feature = "tui")]
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
    #[command(visible_alias = "c")]
    Chat {
        /// LLM provider: claude, openai, mistral, groq, deepseek, native
        #[arg(short, long, value_name = "NAME")]
        provider: Option<String>,

        /// Model name (provider-specific)
        #[arg(short, long, value_name = "MODEL")]
        model: Option<String>,
    },

    /// Open Studio editor (shortcut for `nika ui --view editor`)
    #[cfg(feature = "tui")]
    #[command(visible_alias = "s")]
    Studio {
        /// Workflow file to edit (optional)
        workflow: Option<PathBuf>,
    },

    /// Run a workflow file (headless, no TUI)
    #[command(visible_alias = "r")]
    Run {
        /// Path to .nika.yaml file
        file: String,

        /// Override default provider (claude, openai, mock)
        #[arg(short, long)]
        provider: Option<String>,

        /// Override default model
        #[arg(short, long)]
        model: Option<String>,
    },

    /// Validate workflow syntax, DAG structure, and bindings
    #[command(alias = "validate", visible_alias = "v")]
    Check {
        /// Path to .nika.yaml file
        file: String,

        /// Enable strict mode: connect to MCP servers and validate invoke params
        #[arg(long)]
        strict: bool,
    },

    /// Initialize a new Nika project in the current directory
    Init {
        /// Permission mode: deny, plan, accept-edits, accept-all
        #[arg(short, long, default_value = "plan")]
        permission: String,

        /// Skip creating example workflow
        #[arg(long)]
        no_example: bool,

        /// Migrate API keys from environment variables to system keychain
        #[arg(long)]
        migrate_keys: bool,
    },

    /// Manage execution traces
    Trace {
        #[command(subcommand)]
        action: TraceAction,
    },

    /// Manage LLM provider API keys (v0.12.1)
    #[cfg(feature = "tui")]
    Provider {
        #[command(subcommand)]
        action: ProviderAction,
    },

    /// Manage MCP server connections (v0.12.1)
    Mcp {
        #[command(subcommand)]
        action: McpAction,
    },

    /// Manage local LLM models (v0.27)
    ///
    /// Download, list, and manage GGUF models for native inference.
    /// Models are stored in ~/.nika/models/
    #[cfg(feature = "native-inference")]
    #[command(visible_alias = "m")]
    Model {
        #[command(subcommand)]
        action: ModelAction,
    },

    /// Manage installed packages (workflows, skills, schemas) (v0.27)
    ///
    /// List, add, remove, and install packages from the SuperNovae registry.
    /// Packages are stored in ~/.nika/packages/
    #[command(visible_alias = "p")]
    Pkg {
        #[command(subcommand)]
        action: PkgAction,
    },

    /// Sync MCP servers and packages to IDE configurations (v0.27)
    ///
    /// Syncs from ~/.nika/mcp.yaml to Claude Code, Cursor, Windsurf, VS Code.
    Sync {
        #[command(subcommand)]
        action: Option<SyncAction>,

        /// Target IDE (claude-code, cursor, vscode, windsurf)
        #[arg(short, long)]
        target: Option<String>,

        /// Show what would be synced without making changes
        #[arg(long)]
        dry_run: bool,
    },

    /// Interactive setup wizard for SuperNovae ecosystem (v0.27)
    ///
    /// Configure providers, install tools, and set up IDE integrations.
    Setup {
        #[command(subcommand)]
        action: Option<SetupAction>,
    },

    /// Manage the nika daemon for keychain access (v0.27)
    ///
    /// The daemon provides unified keychain access, eliminating repeated popups.
    /// Binary stays in nika-daemon; these commands proxy to it.
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },

    /// Backup and restore SuperNovae data (v0.27)
    ///
    /// Creates unified backups of NovaNet schema/seeds, Nika workflows/sessions,
    /// and configuration. Backups are stored in ~/.nika/backups/ as tar.gz archives.
    Backup {
        #[command(subcommand)]
        action: BackupAction,
    },

    /// Generate shell completions (v0.13.1)
    Completion {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },

    /// Manage Nika configuration (v0.13.1)
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Manage schema versions and migrations (v0.21+)
    Schema {
        #[command(subcommand)]
        action: SchemaAction,
    },

    /// Check system health and diagnose issues (v0.13.1)
    #[command(visible_alias = "d")]
    Doctor {
        /// Run all checks including slow ones (MCP connectivity)
        #[arg(long)]
        full: bool,

        /// Output format: text, json
        #[arg(long, default_value = "text")]
        format: String,
    },

    /// Manage the Jobs Daemon for scheduled workflow execution (v0.14.0)
    #[cfg(feature = "jobs")]
    #[command(visible_alias = "j")]
    Jobs {
        #[command(subcommand)]
        action: JobsAction,
    },

    /// Create a new workflow from template or wizard (v0.19.3)
    #[command(visible_alias = "n")]
    New {
        /// Workflow name (used for filename)
        name: Option<String>,

        /// Launch interactive wizard (default if no other flags)
        #[arg(long)]
        wizard: bool,

        /// Use a template (simple-infer, blog-generator, agent-research, etc.)
        #[arg(short, long, value_name = "TEMPLATE")]
        template: Option<String>,

        /// Primary verb (infer, exec, fetch, invoke, agent)
        #[arg(long, value_name = "VERB")]
        verb: Option<String>,

        /// LLM provider (claude, openai, mistral, groq, deepseek, native)
        #[arg(short, long, value_name = "PROVIDER")]
        provider: Option<String>,

        /// Output format (text, json, yaml)
        #[arg(short, long, value_name = "FORMAT")]
        output: Option<String>,

        /// Include MCP server configuration
        #[arg(long)]
        with_mcp: bool,

        /// Include subworkflow example
        #[arg(long)]
        with_include: bool,

        /// Include artifact output configuration
        #[arg(long)]
        with_artifacts: bool,

        /// Output directory (default: current directory)
        #[arg(short = 'd', long, value_name = "DIR")]
        output_dir: Option<PathBuf>,

        /// List available templates
        #[arg(long)]
        list: bool,
    },

    /// Manage workflow files (edit, add-task, graph, check)
    #[command(visible_alias = "w")]
    Workflow {
        #[command(subcommand)]
        action: WorkflowAction,
    },

    /// Start Language Server Protocol server (v0.22)
    ///
    /// Provides IDE integration for .nika.yaml workflow files:
    /// - Diagnostics (syntax errors, validation errors)
    /// - Completions (verbs, fields, task references)
    /// - Hover documentation
    /// - Go to definition
    /// - Code actions (quick fixes)
    #[cfg(feature = "lsp")]
    Lsp {
        /// Communication mode: stdio (default) or tcp
        #[arg(long, default_value = "stdio")]
        mode: String,

        /// TCP port (only used with --mode tcp)
        #[arg(long, default_value = "9257")]
        port: u16,
    },
}

#[derive(Subcommand)]
enum TraceAction {
    /// List all traces
    List {
        /// Show only last N traces
        #[arg(short, long)]
        limit: Option<usize>,
    },

    /// Show details of a trace
    Show {
        /// Generation ID or partial match
        id: String,
    },

    /// Export trace to file
    Export {
        /// Generation ID
        id: String,
        /// Output format (json, yaml)
        #[arg(short, long, default_value = "json")]
        format: String,
        /// Output file (stdout if not specified)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Delete old traces
    Clean {
        /// Keep only last N traces
        #[arg(short, long, default_value = "10")]
        keep: usize,
    },
}

/// Provider management actions (v0.12.1)
#[cfg(feature = "tui")]
#[derive(Subcommand)]
enum ProviderAction {
    /// List all providers and their status
    List,

    /// Set API key for a provider (stored in system keychain)
    Set {
        /// Provider name (anthropic, openai, mistral, groq, deepseek)
        provider: String,
        /// API key (or use --prompt to enter interactively)
        key: Option<String>,
        /// Prompt for key interactively (hidden input)
        #[arg(short, long)]
        prompt: bool,
    },

    /// Get API key for a provider (masked for security)
    Get {
        /// Provider name
        provider: String,
    },

    /// Delete API key for a provider
    Delete {
        /// Provider name
        provider: String,
    },

    /// Migrate API keys from environment variables to keychain
    Migrate,

    /// Test connection to a provider
    Test {
        /// Provider name
        provider: String,
    },
}

/// MCP server management actions (v0.27)
///
/// Manage MCP servers at global (~/.nika/mcp.yaml), project (.nika/mcp.yaml),
/// or workflow levels. Supports 48 aliases for common MCP servers.
#[derive(Subcommand)]
enum McpAction {
    /// Add an MCP server to global or project config
    ///
    /// Supports aliases (neo4j, github, perplexity) or full npm packages.
    /// Use `nika mcp aliases` to see all 48 available aliases.
    Add {
        /// Server name (alias like 'neo4j' or package like '@neo4j/mcp-neo4j')
        name: String,

        /// Add to global config (~/.nika/mcp.yaml) instead of project
        #[arg(long)]
        global: bool,

        /// Add to project config (.nika/mcp.yaml) - default
        #[arg(long)]
        project: bool,

        /// Server description
        #[arg(short, long)]
        description: Option<String>,

        /// Custom command (default: npx)
        #[arg(long)]
        command: Option<String>,

        /// Custom arguments (overrides default -y <package>)
        #[arg(long)]
        args: Option<Vec<String>>,
    },

    /// Remove an MCP server from global or project config
    Remove {
        /// Server name to remove
        name: String,

        /// Remove from global config (~/.nika/mcp.yaml)
        #[arg(long)]
        global: bool,

        /// Remove from project config (.nika/mcp.yaml) - default
        #[arg(long)]
        project: bool,

        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
    },

    /// List MCP servers (merged from global, project, workflow)
    List {
        /// Workflow file with MCP definitions
        #[arg(short, long)]
        workflow: Option<String>,

        /// Show only global config (~/.nika/mcp.yaml)
        #[arg(long)]
        global: bool,

        /// Show only project config (.nika/mcp.yaml)
        #[arg(long)]
        project: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// List available MCP server aliases (48 total)
    Aliases {
        /// Filter by category (anthropic, databases, search, developer, productivity, ai)
        #[arg(short, long)]
        category: Option<String>,
    },

    /// Test connection to an MCP server
    Test {
        /// Workflow file with MCP definitions
        workflow: String,
        /// MCP server name (from workflow's mcp: section)
        server: String,
    },

    /// List tools available from an MCP server
    Tools {
        /// Workflow file with MCP definitions
        workflow: String,
        /// MCP server name
        server: String,
    },
}

/// Package management actions (v0.27)
///
/// Manage SuperNovae packages (workflows, skills, schemas) stored in ~/.nika/packages/
#[derive(Subcommand)]
enum PkgAction {
    /// List installed packages
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Show information about a package
    Info {
        /// Package name (e.g., @nika/seo-audit, @workflows/code-review)
        package: String,
    },

    /// Add a package to the project
    ///
    /// Downloads and installs the package and its dependencies.
    /// Updates nika.yaml and nika.lock in the project directory.
    Add {
        /// Package name (e.g., @nika/seo-audit, @workflows/code-review)
        package: String,

        /// Package type (workflow, agent, skill, prompt, job, schema)
        #[arg(short, long)]
        r#type: Option<String>,

        /// Version constraint (e.g., ^0.1, 1.0.0)
        #[arg(long, visible_alias = "ver")]
        version: Option<String>,

        /// Add as dev dependency
        #[arg(long)]
        dev: bool,
    },

    /// Remove a package from the project
    Remove {
        /// Package name to remove
        package: String,

        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
    },

    /// Install packages from nika.yaml
    Install {
        /// Use exact versions from nika.lock
        #[arg(long)]
        frozen: bool,
    },

    /// Update packages to latest compatible versions
    Update {
        /// Package to update (updates all if not specified)
        package: Option<String>,
    },

    /// List outdated packages
    Outdated,

    /// Search packages in the registry
    Search {
        /// Search query
        query: String,

        /// Package type filter (workflow, agent, skill, prompt, job, schema)
        #[arg(short, long)]
        r#type: Option<String>,

        /// Maximum results to show
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },
}

/// Sync management actions (v0.27)
///
/// Sync MCP servers and packages to IDE configurations.
#[derive(Subcommand)]
enum SyncAction {
    /// Show current sync status
    Status,

    /// Enable sync for an IDE (auto-sync when configs change)
    Enable {
        /// IDE to enable: claude-code, cursor, vscode, windsurf
        editor: String,
    },

    /// Disable sync for an IDE
    Disable {
        /// IDE to disable: claude-code, cursor, vscode, windsurf
        editor: String,
    },
}

/// Setup wizard actions (v0.27)
///
/// Interactive setup for SuperNovae ecosystem components.
#[derive(Subcommand)]
enum SetupAction {
    /// Full interactive setup wizard (providers, tools, IDEs)
    Wizard,

    /// Set up Nika CLI, LSP, and daemon
    Nika,

    /// Set up NovaNet CLI and Neo4j connection
    Novanet,

    /// Set up Claude Code integration
    ClaudeCode,

    /// Set up Cursor integration
    Cursor,

    /// Set up VS Code integration
    Vscode,

    /// Set up Windsurf integration
    Windsurf,
}

/// Daemon management actions (v0.27)
///
/// The daemon binary (nika-daemon) is kept external. These commands proxy to it.
#[derive(Subcommand)]
enum DaemonAction {
    /// Start the daemon (proxies to nika-daemon start)
    Start {
        /// Run in foreground (don't daemonize)
        #[arg(long)]
        foreground: bool,

        /// Skip preloading secrets at startup
        #[arg(long)]
        skip_preload: bool,
    },

    /// Stop the daemon (proxies to nika-daemon stop)
    Stop,

    /// Show daemon status
    Status {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Restart the daemon
    Restart,

    /// Install daemon as a system service (auto-start at login)
    Install,

    /// Uninstall daemon system service
    Uninstall,
}

/// Backup management actions (v0.27)
///
/// Proxies to `nika-daemon backup` commands for unified backup/restore.
#[derive(Subcommand)]
enum BackupAction {
    /// Create a new backup of all SuperNovae data
    Create {
        /// Optional label for the backup (e.g., "before-refactor")
        #[arg(short, long)]
        label: Option<String>,

        /// Include only specific subsystems (novanet, nika)
        #[arg(long, value_delimiter = ',')]
        only: Option<Vec<String>>,
    },

    /// Restore from a backup
    Restore {
        /// Backup file path, or "latest" for most recent
        #[arg(default_value = "latest")]
        backup: String,

        /// Force restore without confirmation
        #[arg(short, long)]
        force: bool,
    },

    /// List available backups
    #[command(visible_alias = "ls")]
    List {
        /// Show detailed information (manifest contents)
        #[arg(short, long)]
        detailed: bool,

        /// Maximum number of backups to show
        #[arg(short = 'n', long, default_value = "10")]
        limit: usize,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Delete old backups
    Prune {
        /// Keep only the N most recent backups
        #[arg(short, long, default_value = "5")]
        keep: usize,

        /// Actually delete (dry-run by default)
        #[arg(long)]
        execute: bool,
    },
}

/// Model management actions (v0.27)
///
/// Model management for local GGUF models.
#[cfg(feature = "native-inference")]
#[derive(Subcommand)]
enum ModelAction {
    /// List downloaded models in ~/.nika/models/
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Download a model from HuggingFace
    ///
    /// Supports curated models from KNOWN_MODELS or custom HuggingFace repos.
    /// Example: nika model pull qwen3:8b
    /// Example: nika model pull --repo TheBloke/Mistral-7B-v0.1-GGUF --file mistral-7b-v0.1.Q4_K_M.gguf
    Pull {
        /// Model name (from KNOWN_MODELS, e.g., qwen3:8b, llama3:8b)
        name: Option<String>,

        /// HuggingFace repository (e.g., TheBloke/Mistral-7B-v0.1-GGUF)
        #[arg(long)]
        repo: Option<String>,

        /// Specific GGUF filename to download
        #[arg(long)]
        file: Option<String>,

        /// Quantization level (e.g., Q4_K_M, Q8_0, F16)
        #[arg(short = 'Q', long)]
        quant: Option<String>,

        /// Force re-download even if model exists
        #[arg(short, long)]
        force: bool,
    },

    /// Show information about a model
    ///
    /// Shows curated model info from KNOWN_MODELS or local file info.
    Info {
        /// Model name or path
        name: String,
    },

    /// Show status of loaded models
    Status,

    /// Delete a downloaded model
    Delete {
        /// Model name or path (relative to ~/.nika/models/)
        name: String,

        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
    },
}

/// Configuration management actions (v0.13.1)
#[derive(Subcommand)]
enum ConfigAction {
    /// List all configuration values
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Get a specific config value
    Get {
        /// Config key (dot-separated, e.g., editor.theme)
        key: String,
    },

    /// Set a config value
    Set {
        /// Config key (dot-separated, e.g., editor.theme)
        key: String,
        /// Value to set
        value: String,
    },

    /// Open config file in $EDITOR
    Edit,

    /// Show config file path
    Path,

    /// Reset config to defaults
    Reset {
        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
    },
}

/// Schema management actions (v0.21+)
#[derive(Subcommand)]
enum SchemaAction {
    /// Upgrade workflow to latest schema version
    Upgrade {
        /// Path to .nika.yaml file or directory
        path: String,

        /// Target schema version (default: latest)
        #[arg(short, long)]
        target: Option<String>,

        /// Process all files in directory
        #[arg(long)]
        all: bool,

        /// Create backup before upgrading (default: true)
        #[arg(long, default_value = "true")]
        backup: bool,
    },

    /// Show current schema version of a workflow (or list available versions)
    Version {
        /// Path to .nika.yaml file (optional - shows available versions if omitted)
        path: Option<String>,
    },

    /// Validate workflow against schema
    Validate {
        /// Path to .nika.yaml file
        path: String,

        /// Schema version to validate against
        #[arg(short, long)]
        schema: Option<String>,
    },
}

/// Jobs Daemon management actions (v0.14.0)
#[cfg(feature = "jobs")]
#[derive(Subcommand)]
enum JobsAction {
    /// Start the Jobs Daemon (daemonizes by default)
    Start {
        /// Run in foreground (don't daemonize)
        #[arg(short, long)]
        foreground: bool,

        /// Path to jobs configuration file
        #[arg(short, long, default_value = ".nika/jobs.toml")]
        config: PathBuf,
    },

    /// Stop the running Jobs Daemon
    Stop {
        /// Force kill (SIGKILL instead of SIGTERM)
        #[arg(short, long)]
        force: bool,
    },

    /// Show status of the Jobs Daemon
    Status {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// List all configured jobs
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Path to jobs configuration file
        #[arg(short, long, default_value = ".nika/jobs.toml")]
        config: PathBuf,
    },

    /// Manually trigger a job
    Trigger {
        /// Job name to trigger
        job_name: String,

        /// Path to jobs configuration file
        #[arg(short, long, default_value = ".nika/jobs.toml")]
        config: PathBuf,
    },

    /// Pause a job (skip scheduled runs)
    Pause {
        /// Job name to pause
        job_name: String,
    },

    /// Resume a paused job
    Resume {
        /// Job name to resume
        job_name: String,
    },

    /// Show execution history for jobs
    History {
        /// Job name (all jobs if not specified)
        job_name: Option<String>,

        /// Limit number of entries
        #[arg(short, long, default_value = "20")]
        limit: usize,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Reload daemon configuration
    Reload,

    // === Background Job Commands (v0.27) ===
    // These proxy to `nika-daemon jobs` for background workflow execution
    /// Submit a workflow for background execution (v0.27)
    ///
    /// Returns a job ID that can be used to track status and output.
    Submit {
        /// Path to workflow file
        workflow: PathBuf,

        /// Additional workflow arguments
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,

        /// Optional job name (defaults to workflow filename)
        #[arg(short, long)]
        name: Option<String>,

        /// Job priority (-10 to 10, higher = more priority)
        #[arg(short, long, default_value = "0")]
        priority: i32,
    },

    /// Cancel a running background job (v0.27)
    Cancel {
        /// Job ID to cancel
        id: String,
    },

    /// Show output from a background job (v0.27)
    Output {
        /// Job ID
        id: String,

        /// Follow output in real-time (like tail -f)
        #[arg(short, long)]
        follow: bool,
    },

    /// Clear completed/failed background jobs (v0.27)
    Clear {
        /// Clear all jobs including running ones (use with caution)
        #[arg(long)]
        all: bool,
    },
}

/// Workflow management actions (v0.22+)
#[derive(Subcommand)]
enum WorkflowAction {
    /// Open workflow in interactive editor
    Edit {
        /// Path to .nika.yaml file
        file: PathBuf,
    },

    /// Add a new task interactively
    AddTask {
        /// Path to .nika.yaml file
        file: PathBuf,

        /// Task ID (generated if not provided)
        #[arg(long)]
        id: Option<String>,

        /// Task verb (infer, exec, fetch, invoke, agent)
        #[arg(long, value_name = "VERB")]
        verb: Option<String>,

        /// Insert after this task ID
        #[arg(long)]
        after: Option<String>,
    },

    /// Visualize workflow as DAG graph
    Graph {
        /// Path to .nika.yaml file
        file: PathBuf,

        /// Output format: ascii, dot, mermaid
        #[arg(short, long, default_value = "ascii")]
        format: String,

        /// Output file (stdout if not specified)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Validate workflow with suggestions for improvements
    Check {
        /// Path to .nika.yaml file
        file: PathBuf,

        /// Show improvement suggestions
        #[arg(long)]
        suggest: bool,

        /// Output format: text, json
        #[arg(long, default_value = "text")]
        format: String,
    },
}

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
            let result = run_workflow(&file.display().to_string(), None, None).await;
            handle_result(result);
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

    // Handle subcommands or default to help (terminal-first)
    let result = match cli.command {
        // No command = show help (like cargo, git)
        None => {
            use clap::CommandFactory;
            if let Err(e) = Cli::command().print_help() {
                eprintln!("Failed to print help: {}", e);
                std::process::exit(1);
            }
            Ok(())
        }

        // Launch TUI (explicit via `nika ui`)
        #[cfg(feature = "tui")]
        Some(Commands::Ui { view, workflow }) => {
            use nika::tui::TuiView;
            let initial_view = match view.as_deref() {
                Some("chat") | Some("c") => Some(TuiView::Chat),
                Some("studio") | Some("editor") | Some("d") | Some("explorer") | Some("e")
                | Some("home") => Some(TuiView::Studio),
                Some("runner") | Some("r") | Some("monitor") => Some(TuiView::Runner),
                Some("settings") | Some(",") => Some(TuiView::Settings),
                Some(unknown) => {
                    eprintln!(
                        "{} Unknown view '{}'. Valid: studio, chat, runner, settings",
                        "Error:".red().bold(),
                        unknown
                    );
                    std::process::exit(1);
                }
                None => None, // Use default (Studio)
            };
            nika::tui::run_tui_with_options(workflow, initial_view).await
        }

        // Chat mode
        #[cfg(feature = "tui")]
        Some(Commands::Chat { provider, model }) => nika::tui::run_tui_chat(provider, model).await,

        // Studio mode
        #[cfg(feature = "tui")]
        Some(Commands::Studio { workflow }) => nika::tui::run_tui_studio(workflow).await,

        // Run workflow
        Some(Commands::Run {
            file,
            provider,
            model,
        }) => run_workflow(&file, provider, model).await,

        // Check/Validate workflow
        Some(Commands::Check { file, strict }) => {
            if strict {
                validate_workflow_strict(&file).await
            } else {
                validate_workflow(&file).await
            }
        }

        // Init project
        Some(Commands::Init {
            permission,
            no_example,
            migrate_keys,
        }) => init_project(&permission, no_example, migrate_keys),

        // Trace commands
        Some(Commands::Trace { action }) => handle_trace_command(action),

        // Provider management (v0.12.1)
        #[cfg(feature = "tui")]
        Some(Commands::Provider { action }) => handle_provider_command(action).await,

        // MCP server management (v0.12.1)
        Some(Commands::Mcp { action }) => handle_mcp_command(action).await,

        // Model management (v0.27)
        #[cfg(feature = "native-inference")]
        Some(Commands::Model { action }) => handle_model_command(action, quiet).await,

        // Package management (v0.27)
        Some(Commands::Pkg { action }) => handle_pkg_command(action).await,

        // Sync command (v0.27)
        Some(Commands::Sync {
            action,
            target,
            dry_run,
        }) => handle_sync_command(action, target, dry_run, quiet).await,

        // Setup wizard (v0.27)
        Some(Commands::Setup { action }) => handle_setup_command(action, quiet).await,

        // Daemon management (v0.27)
        Some(Commands::Daemon { action }) => handle_daemon_command(action, quiet).await,

        // Backup management (v0.27)
        Some(Commands::Backup { action }) => handle_backup_command(action, quiet).await,

        // Shell completion (v0.13.1)
        Some(Commands::Completion { shell }) => {
            clap_complete::generate(shell, &mut Cli::command(), "nika", &mut std::io::stdout());
            Ok(())
        }

        // Configuration management (v0.13.1)
        Some(Commands::Config { action }) => handle_config_command(action, quiet),

        // Schema management (v0.21.x)
        Some(Commands::Schema { action }) => handle_schema_command(action, quiet),

        // Doctor command (v0.13.1)
        Some(Commands::Doctor { full, format }) => {
            handle_doctor_command(full, &format, quiet).await
        }

        // Jobs Daemon management (v0.14.0)
        #[cfg(feature = "jobs")]
        Some(Commands::Jobs { action }) => handle_jobs_command(action, quiet).await,

        // New workflow creation (v0.19.3)
        Some(Commands::New {
            name,
            wizard,
            template,
            verb,
            provider,
            output,
            with_mcp,
            with_include,
            with_artifacts,
            output_dir,
            list,
        }) => handle_new_command(
            name,
            wizard,
            template,
            verb,
            provider,
            output,
            with_mcp,
            with_include,
            with_artifacts,
            output_dir,
            list,
            quiet,
        ),

        Some(Commands::Workflow { action }) => handle_workflow_command(action, quiet).await,

        // LSP Server (v0.22)
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

    handle_result(result);
}

// ═══════════════════════════════════════════════════════════════════════════
// HELPER FUNCTIONS
// ═══════════════════════════════════════════════════════════════════════════

/// Check if we're running in TUI mode (skip tracing to avoid terminal pollution)
fn is_tui_mode(cli: &Cli) -> bool {
    // No command and no file = show help (terminal-first), NOT TUI
    if cli.command.is_none() && cli.file.is_none() {
        return false;
    }

    // Check TUI-related commands
    #[cfg(feature = "tui")]
    if let Some(ref cmd) = cli.command {
        return matches!(
            cmd,
            Commands::Ui { .. } | Commands::Chat { .. } | Commands::Studio { .. }
        );
    }

    false
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
fn handle_result(result: Result<(), NikaError>) {
    if let Err(e) = result {
        // Use miette's fancy error display for terminal output
        let report = miette::Report::new(e);
        eprintln!("{:?}", report);
        std::process::exit(1);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// WORKFLOW COMMANDS
// ═══════════════════════════════════════════════════════════════════════════

/// Resolve a workflow reference to an actual file path.
///
/// Resolution order:
/// 1. If starts with '@' (e.g., @workflows/seo-audit) → Package resolution from ~/.nika/packages/
/// 2. If simple name without path/extension → Search in .nika/workflows/{name}.nika.yaml
/// 3. Otherwise → Use as-is (filesystem path)
///
/// # Examples
///
/// - `@workflows/seo-audit` → `~/.nika/packages/workflows/seo-audit/1.0.0/workflow.nika.yaml`
/// - `custom` → `.nika/workflows/custom.nika.yaml` (if exists) or error
/// - `./local.nika.yaml` → `./local.nika.yaml` (direct path)
async fn resolve_workflow_path(reference: &str) -> Result<PathBuf, NikaError> {
    // 1. Package reference (starts with @)
    if reference.starts_with('@') {
        let resolved =
            resolver::resolve_package_path(reference).map_err(|e| NikaError::WorkflowNotFound {
                path: format!(
                    "Package not found: {}. Error: {}. Try: nika pkg add {}",
                    reference, e, reference
                ),
            })?;

        // Package should contain workflow.nika.yaml
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

    // 2. Simple name without path separator or .yaml extension → try .nika/workflows/
    if !reference.contains('/')
        && !reference.ends_with(".nika.yaml")
        && !reference.ends_with(".yaml")
    {
        let local_path = PathBuf::from(".nika")
            .join("workflows")
            .join(format!("{}.nika.yaml", reference));

        if local_path.exists() {
            return Ok(local_path);
        }

        // If not found locally and not an absolute path, suggest package search
        if !PathBuf::from(reference).exists() {
            return Err(NikaError::WorkflowNotFound {
                path: format!("Workflow '{}' not found in .nika/workflows/ or current directory. Try: nika pkg search {}", reference, reference)
            });
        }
    }

    // 3. Direct filesystem path
    let path = PathBuf::from(reference);
    if !path.exists() {
        return Err(NikaError::WorkflowNotFound {
            path: format!(
                "File not found: {}. Check the path or try: nika pkg search {}",
                reference, reference
            ),
        });
    }

    Ok(path)
}

async fn run_workflow(
    file: &str,
    provider_override: Option<String>,
    model_override: Option<String>,
) -> Result<(), NikaError> {
    // Resolve workflow path (package resolution + local lookup)
    let resolved_path = resolve_workflow_path(file).await?;

    // Read and parse (async to not block runtime)
    let yaml = tokio::fs::read_to_string(&resolved_path).await?;

    // Validate YAML against JSON Schema (catches structural errors early)
    let validator = WorkflowSchemaValidator::new()?;
    validator.validate_yaml(&yaml)?;

    // Parse into Workflow struct (now we know structure is valid)
    let workflow: Workflow = serde_yaml::from_str(&yaml)?;

    // Expand includes (v0.14.2 - DAG fusion)
    // Handle case where parent() returns empty path for relative filenames
    let base_path = resolved_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut workflow = expand_includes(workflow, base_path)?;

    // Validate schema version and task config
    workflow.validate_schema()?;

    // Apply CLI overrides
    if let Some(p) = provider_override {
        workflow.provider = p;
    }
    if let Some(m) = model_override {
        workflow.model = Some(m);
    }

    println!(
        "{} Using provider: {} | model: {}",
        "→".cyan(),
        workflow.provider.cyan().bold(),
        workflow.model.as_deref().unwrap_or("(default)").cyan()
    );

    // Run
    let mut runner = Runner::new(workflow);
    let output = runner.run().await?;

    // Print output
    if !output.is_empty() {
        println!("{}", "Output:".cyan().bold());
        println!("{}", output);
    }

    Ok(())
}

async fn validate_workflow(file: &str) -> Result<(), NikaError> {
    // Resolve workflow path (package resolution + local lookup)
    let resolved_path = resolve_workflow_path(file).await?;

    let yaml = tokio::fs::read_to_string(&resolved_path).await?;

    // Validate YAML against JSON Schema (catches structural errors early)
    let validator = WorkflowSchemaValidator::new()?;
    validator.validate_yaml(&yaml)?;

    // Parse into Workflow struct (now we know structure is valid)
    let workflow: Workflow = serde_yaml::from_str(&yaml)?;

    // Expand includes (v0.14.2 - DAG fusion)
    let base_path = resolved_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let workflow = expand_includes(workflow, base_path)?;

    // Validate schema version and task config
    workflow.validate_schema()?;

    // Build flow graph (validates duplicate task IDs: NIKA-022)
    let flow_graph = Dag::from_workflow(&workflow)?;

    // BUG-002 FIX: Detect cycles (NIKA-020) during nika check
    flow_graph.detect_cycles()?;

    // Validate use: bindings (NIKA-080, NIKA-081, NIKA-082)
    validate_use_wiring(&workflow, &flow_graph)?;

    println!("{} Workflow '{}' is valid", "✓".green(), file);
    println!("  Provider: {}", workflow.provider);
    println!(
        "  Model: {}",
        workflow.model.as_deref().unwrap_or("(default)")
    );
    println!("  Tasks: {}", workflow.tasks.len());
    println!("  Flows: {}", workflow.flows.len());

    Ok(())
}

/// Validate a workflow with --strict mode (connects to MCP servers)
async fn validate_workflow_strict(file: &str) -> Result<(), NikaError> {
    // Resolve workflow path (package resolution + local lookup)
    let resolved_path = resolve_workflow_path(file).await?;

    let yaml = tokio::fs::read_to_string(&resolved_path).await?;

    // Phase 1: JSON Schema validation
    let schema_validator = WorkflowSchemaValidator::new()?;
    schema_validator.validate_yaml(&yaml)?;

    // Parse into Workflow struct
    let workflow: Workflow = serde_yaml::from_str(&yaml)?;

    // Expand includes (v0.14.2 - DAG fusion)
    let base_path = resolved_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let workflow = expand_includes(workflow, base_path)?;

    // Validate schema version and task config
    workflow.validate_schema()?;

    // Phase 2: DAG validation
    let flow_graph = Dag::from_workflow(&workflow)?;
    flow_graph.detect_cycles()?;
    validate_use_wiring(&workflow, &flow_graph)?;

    // Phase 3: MCP parameter validation (strict mode)
    println!(
        "{} Strict mode: validating invoke parameters...",
        "→".cyan()
    );

    // Collect invoke tasks
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

    if invoke_tasks.is_empty() {
        println!("  {} No invoke tasks to validate", "✓".green());
    } else {
        // Create validator
        let mcp_validator = McpValidator::new(ValidationConfig::default());

        // Collect unique MCP servers needed
        let mcp_servers: std::collections::HashSet<&str> = invoke_tasks
            .iter()
            .filter_map(|(_, p)| p.mcp.as_deref())
            .collect();

        // Get MCP configs (workflow.mcp is Option<FxHashMap<...>>)
        let mcp_configs = workflow
            .mcp
            .as_ref()
            .ok_or_else(|| NikaError::ValidationError {
                reason: "Workflow has invoke tasks but no mcp: configuration".to_string(),
            })?;

        // Connect to each MCP server and list tools
        for server_name in mcp_servers {
            let Some(inline_config) = mcp_configs.get::<str>(server_name) else {
                return Err(NikaError::McpNotConnected {
                    name: server_name.to_string(),
                });
            };

            println!(
                "  {} Connecting to MCP server '{}'...",
                "→".cyan(),
                server_name
            );

            // Build McpConfig from McpConfigInline (add server name)
            let mut config = McpConfig::new(server_name, &inline_config.command)
                .with_args(inline_config.args.iter().cloned());
            for (key, value) in &inline_config.env {
                config = config.with_env(key, value);
            }
            if let Some(ref cwd) = inline_config.cwd {
                config = config.with_cwd(cwd);
            }

            // Create client (sync) and connect (async)
            let client = McpClient::new(config)?;
            client.connect().await?;

            // List tools
            let tools = client.list_tools().await?;
            println!("    {} Found {} tools", "✓".green(), tools.len());

            // Populate validator cache
            mcp_validator.cache().populate(server_name, &tools)?;
        }

        // Validate each invoke task
        let mut all_valid = true;
        for (task_id, params) in &invoke_tasks {
            let tool_name = params.tool.as_deref().unwrap_or("(resource read)");

            // Only validate tool calls, not resource reads
            if let Some(ref tool) = params.tool {
                let invoke_params = params.params.clone().unwrap_or_default();
                let mcp_server = params.mcp.as_deref().unwrap_or("unknown");
                let result = mcp_validator.validate(mcp_server, tool, &invoke_params);

                if result.is_valid {
                    println!(
                        "    {} Task '{}': {} parameters valid",
                        "✓".green(),
                        task_id,
                        tool_name
                    );
                } else {
                    all_valid = false;
                    println!(
                        "    {} Task '{}': {} validation errors",
                        "✗".red(),
                        task_id,
                        result.errors.len()
                    );
                    for error in &result.errors {
                        println!("      {} [{}] {}", "→".yellow(), error.path, error.message);
                    }
                }
            } else {
                println!(
                    "    {} Task '{}': resource read (no params to validate)",
                    "•".cyan(),
                    task_id
                );
            }
        }

        if !all_valid {
            return Err(NikaError::ValidationError {
                reason: "Strict validation failed: invoke parameters don't match tool schemas"
                    .to_string(),
            });
        }
    }

    println!("{} Workflow '{}' is valid (strict)", "✓".green(), file);
    println!("  Provider: {}", workflow.provider);
    println!(
        "  Model: {}",
        workflow.model.as_deref().unwrap_or("(default)")
    );
    println!("  Tasks: {}", workflow.tasks.len());
    println!("  Flows: {}", workflow.flows.len());

    Ok(())
}

fn handle_trace_command(action: TraceAction) -> Result<(), NikaError> {
    match action {
        TraceAction::List { limit } => {
            let traces = nika::list_traces()?;
            let traces = match limit {
                Some(n) => traces.into_iter().take(n).collect::<Vec<_>>(),
                None => traces,
            };

            println!("Found {} traces:\n", traces.len());
            println!("{:<30} {:>10} {:>20}", "GENERATION ID", "SIZE", "CREATED");
            println!("{}", "-".repeat(62));

            for trace in traces {
                // Format size
                let size = if trace.size_bytes > 1024 * 1024 {
                    format!("{:.1}MB", trace.size_bytes as f64 / 1024.0 / 1024.0)
                } else if trace.size_bytes > 1024 {
                    format!("{:.1}KB", trace.size_bytes as f64 / 1024.0)
                } else {
                    format!("{}B", trace.size_bytes)
                };

                // Format created time
                let created = trace
                    .created
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| {
                        chrono::DateTime::from_timestamp(d.as_secs() as i64, 0)
                            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                            .unwrap_or_else(|| "unknown".to_string())
                    })
                    .unwrap_or_else(|| "unknown".to_string());

                println!("{:<30} {:>10} {:>20}", trace.generation_id, size, created);
            }
            Ok(())
        }

        TraceAction::Show { id } => {
            let traces = nika::list_traces()?;
            let trace = traces
                .iter()
                .find(|t| t.generation_id.contains(&id))
                .ok_or_else(|| NikaError::ValidationError {
                    reason: format!("No trace matching '{}'", id),
                })?;

            let content = fs::read_to_string(&trace.path)?;
            let events: Vec<Event> = content
                .lines()
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect();

            println!("Trace: {}", trace.generation_id);
            println!("Events: {}", events.len());
            println!("Size: {} bytes\n", trace.size_bytes);

            for event in events {
                println!("[{:>6}ms] {:?}", event.timestamp_ms, event.kind);
            }
            Ok(())
        }

        TraceAction::Export { id, format, output } => {
            let traces = nika::list_traces()?;
            let trace = traces
                .iter()
                .find(|t| t.generation_id.contains(&id))
                .ok_or_else(|| NikaError::ValidationError {
                    reason: format!("No trace matching '{}'", id),
                })?;

            let content = fs::read_to_string(&trace.path)?;
            let events: Vec<Event> = content
                .lines()
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect();

            let exported = match format.as_str() {
                "json" => serde_json::to_string_pretty(&events)?,
                "yaml" => {
                    serde_yaml::to_string(&events).map_err(|e| NikaError::SerializationError {
                        details: e.to_string(),
                    })?
                }
                other => {
                    return Err(NikaError::ValidationError {
                        reason: format!("Unknown format: {}. Use 'json' or 'yaml'", other),
                    })
                }
            };

            match output {
                Some(path) => {
                    fs::write(&path, &exported)?;
                    println!("Exported {} events to {}", events.len(), path.display());
                }
                None => println!("{}", exported),
            }
            Ok(())
        }

        TraceAction::Clean { keep } => {
            let traces = nika::list_traces()?;
            let to_delete: Vec<_> = traces.into_iter().skip(keep).collect();
            let count = to_delete.len();

            for trace in to_delete {
                fs::remove_file(&trace.path)?;
            }

            println!("Deleted {} old traces, kept {}", count, keep);
            Ok(())
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PROVIDER COMMAND (v0.12.1)
// ═══════════════════════════════════════════════════════════════════════════

/// Get all LLM provider IDs (from nika::core::KNOWN_PROVIDERS).
/// Replaces hardcoded ALL_PROVIDERS list (v0.27).
#[cfg(feature = "tui")]
fn llm_provider_ids() -> Vec<&'static str> {
    use nika::core::{ProviderCategory, KNOWN_PROVIDERS};
    KNOWN_PROVIDERS
        .iter()
        .filter(|p| p.category == ProviderCategory::Llm)
        .map(|p| p.id)
        .collect()
}

/// Handle provider management commands
#[cfg(feature = "tui")]
async fn handle_provider_command(action: ProviderAction) -> Result<(), NikaError> {
    use colored::Colorize;
    use nika::core::provider_to_env_var; // v0.27: from nika::core instead of TUI module
    use nika::tui::widgets::provider_modal::{
        mask_api_key, migrate_env_to_keyring, validate_key_format, NikaKeyring,
    };
    use std::io::{self, Write};

    // Get LLM provider IDs from nika::core (v0.27)
    let all_providers = llm_provider_ids();

    match action {
        ProviderAction::List => {
            println!("{}", "LLM Providers".bold());
            println!("{}", "─".repeat(60));

            for provider in &all_providers {
                let env_var = provider_to_env_var(provider).unwrap_or("UNKNOWN_API_KEY");
                let has_keychain = NikaKeyring::exists(provider);
                let has_env = std::env::var(env_var).is_ok();

                let status = match (has_keychain, has_env) {
                    (true, true) => format!("{} (keychain + env)", "✓".green()),
                    (true, false) => format!("{} (keychain)", "✓".green()),
                    (false, true) => format!("{} (env only)", "~".yellow()),
                    (false, false) => format!("{}", "✗".red()),
                };

                let masked = if has_keychain {
                    NikaKeyring::get_masked(provider).unwrap_or_default()
                } else if has_env {
                    std::env::var(env_var)
                        .ok()
                        .map(|k| mask_api_key(&k))
                        .unwrap_or_default()
                } else {
                    String::new()
                };

                println!(
                    "  {:12} {} {}",
                    provider,
                    status,
                    if masked.is_empty() {
                        String::new()
                    } else {
                        format!("[{}]", masked.dimmed())
                    }
                );
            }

            println!();
            println!(
                "{}",
                "Use 'nika provider set <name>' to add an API key".dimmed()
            );
            Ok(())
        }

        ProviderAction::Set {
            provider,
            key,
            prompt,
        } => {
            // Validate provider name using nika::core (v0.27)
            if !all_providers.contains(&provider.as_str()) {
                return Err(NikaError::ValidationError {
                    reason: format!(
                        "Unknown provider '{}'. Valid: {}",
                        provider,
                        all_providers.join(", ")
                    ),
                });
            }

            // Get key from argument or prompt
            let api_key = match (prompt, key) {
                // If prompt flag is set or no key provided, read from stdin
                (true, _) | (false, None) => {
                    print!("Enter API key for {}: ", provider);
                    io::stdout().flush().unwrap();

                    // Read password without echo (requires rpassword crate)
                    // For now, just read from stdin (visible)
                    let mut input = String::new();
                    io::stdin().read_line(&mut input).map_err(|e| {
                        NikaError::Execution(format!("Failed to read input: {}", e))
                    })?;
                    input.trim().to_string()
                }
                // Key provided as argument
                (false, Some(k)) => k,
            };

            // Validate key format
            if let Err(e) = validate_key_format(&provider, &api_key) {
                return Err(NikaError::ValidationError { reason: e });
            }

            // Store in keychain
            NikaKeyring::set(&provider, &api_key)
                .map_err(|e| NikaError::Execution(format!("Failed to store key: {}", e)))?;

            println!(
                "{} API key for {} stored in system keychain",
                "✓".green(),
                provider.bold()
            );
            Ok(())
        }

        ProviderAction::Get { provider } => {
            match NikaKeyring::get_masked(&provider) {
                Some(masked) => {
                    println!("{}: {}", provider, masked);
                }
                None => {
                    let env_var = provider_to_env_var(&provider).unwrap_or("UNKNOWN_API_KEY");
                    match std::env::var(env_var) {
                        Ok(key) => {
                            println!("{}: {} (from env)", provider, mask_api_key(&key));
                        }
                        Err(_) => {
                            println!("{}: {}", provider, "Not configured".red());
                        }
                    }
                }
            }
            Ok(())
        }

        ProviderAction::Delete { provider } => {
            match NikaKeyring::delete(&provider) {
                Ok(()) => {
                    println!(
                        "{} API key for {} deleted from keychain",
                        "✓".green(),
                        provider.bold()
                    );
                }
                Err(e) => {
                    return Err(NikaError::Execution(format!("Failed to delete key: {}", e)));
                }
            }
            Ok(())
        }

        ProviderAction::Migrate => {
            println!(
                "{}",
                "Migrating API keys from environment variables...".cyan()
            );
            let report = migrate_env_to_keyring();
            println!();
            println!("{}", report.summary());
            Ok(())
        }

        ProviderAction::Test { provider } => {
            println!("Testing connection to {}...", provider.bold());

            // First check if API key is configured
            let env_var = provider_to_env_var(&provider).unwrap_or("UNKNOWN_API_KEY");
            let has_key = NikaKeyring::exists(&provider)
                || std::env::var(env_var).is_ok_and(|v| !v.is_empty());

            if !has_key && provider != "native" {
                println!(
                    "{} No API key configured for {}",
                    "✗".red(),
                    provider.bold()
                );
                println!("  Use 'nika provider set {}' to add your API key", provider);
                return Ok(());
            }

            // Try to create provider and make a simple request
            use nika::provider::rig::RigProvider;

            let prov = match provider.as_str() {
                "anthropic" => RigProvider::claude(),
                "openai" => RigProvider::openai(),
                "mistral" => RigProvider::mistral(),
                "groq" => RigProvider::groq(),
                "deepseek" => RigProvider::deepseek(),
                "gemini" => RigProvider::gemini(), // v0.15.0
                "native" => {
                    #[cfg(feature = "native-inference")]
                    {
                        RigProvider::native()
                    }
                    #[cfg(not(feature = "native-inference"))]
                    {
                        return Err(NikaError::ValidationError {
                            reason: "Native inference requires --features native-inference"
                                .to_string(),
                        });
                    }
                }
                _ => {
                    return Err(NikaError::ValidationError {
                        reason: format!("Unknown provider: {}", provider),
                    })
                }
            };

            // Simple test inference
            match prov.infer("Say 'OK' if you can hear me.", None).await {
                Ok(response) => {
                    println!("{} Connection successful!", "✓".green());
                    let truncated: String = response.chars().take(100).collect();
                    println!("  Response: {}", truncated);
                }
                Err(e) => {
                    println!("{} Connection failed: {}", "✗".red(), e);
                }
            }
            Ok(())
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// INIT COMMAND
// ═══════════════════════════════════════════════════════════════════════════

/// Initialize a new Nika project (v0.22.0)
///
/// Creates:
/// - `.nika/` directory with config, agents, skills, memory, etc.
/// - `workflows/` with 30 progressive example workflows organized by tier (unless --no-example)
/// - `context/` with context files for workflows
/// - `workflows/partials/` with reusable workflow fragments (for include:)
/// - `schemas/` with JSON schemas for output validation
/// - `output/` for generated workflow outputs
fn init_project(permission: &str, no_example: bool, migrate_keys: bool) -> Result<(), NikaError> {
    let cwd = std::env::current_dir()?;
    let nika_dir = cwd.join(".nika");

    // Check if already initialized
    if nika_dir.exists() {
        return Err(NikaError::ValidationError {
            reason: format!(
                "Project already initialized at {}. Remove .nika/ to reinitialize.",
                nika_dir.display()
            ),
        });
    }

    // Parse permission mode
    let permission_mode = match permission.to_lowercase().as_str() {
        "deny" => PermissionMode::Deny,
        "plan" => PermissionMode::Plan,
        "accept-edits" | "acceptedits" => PermissionMode::AcceptEdits,
        "accept-all" | "acceptall" | "yolo" => PermissionMode::YoloMode,
        other => {
            return Err(NikaError::ValidationError {
                reason: format!(
                    "Invalid permission mode: '{}'. Use: deny, plan, accept-edits, yolo",
                    other
                ),
            });
        }
    };

    // Create .nika directory
    fs::create_dir_all(&nika_dir)?;
    println!("{} Created {}", "✓".green(), nika_dir.display());

    // Create config.toml
    let config_path = nika_dir.join("config.toml");
    let config_content = format!(
        r#"# Nika Project Configuration
# Generated by `nika init`

[tools]
# Permission mode for file tools
# Options: deny, plan, accept-edits, accept-all
permission = "{}"

# Working directory (default: project root)
# Files outside this directory cannot be accessed
# working_dir = "."

[provider]
# Default LLM provider (claude, openai, mistral, groq, deepseek, native)
# Provider auto-detection checks env vars: ANTHROPIC_API_KEY, OPENAI_API_KEY, etc.
# Can also override with: nika chat --provider <name>
default = "claude"

# Default model (provider-specific)
# Can also override with: nika chat --model <name>
# model = "claude-sonnet-4-6"
"#,
        permission_mode
            .display_name()
            .to_lowercase()
            .replace(" (yolo)", "")
    );
    fs::write(&config_path, config_content)?;
    println!("{} Created {}", "✓".green(), config_path.display());

    // Create agents directory with example (v0.13)
    let agents_dir = nika_dir.join("agents");
    fs::create_dir_all(&agents_dir)?;
    println!("{} Created {}", "✓".green(), agents_dir.display());

    // Create example agent (Claude Code style with YAML frontmatter)
    let example_agent_path = agents_dir.join("researcher.md");
    let example_agent_content = r#"---
name: researcher
description: A helpful research agent that can search and summarize information
model: claude-sonnet-4-6
max_turns: 10
---

You are a Research Agent specialized in finding and synthesizing information.

## Capabilities

- Search the web for relevant information
- Summarize findings in clear, concise language
- Cite sources and provide references
- Answer follow-up questions

## Guidelines

1. Always verify information from multiple sources when possible
2. Clearly distinguish between facts and opinions
3. Acknowledge uncertainty when information is incomplete
4. Provide actionable insights when relevant

## Output Format

Structure your responses with:
- **Summary**: Key findings in 2-3 sentences
- **Details**: Supporting information
- **Sources**: References used (when applicable)
"#;
    fs::write(&example_agent_path, example_agent_content)?;
    println!("{} Created {}", "✓".green(), example_agent_path.display());

    // Create skills directory with example (v0.13)
    let skills_dir = nika_dir.join("skills");
    fs::create_dir_all(&skills_dir)?;
    println!("{} Created {}", "✓".green(), skills_dir.display());

    // Create example skill
    let example_skill_path = skills_dir.join("code-review.md");
    let example_skill_content = r#"---
name: code-review
description: Skill for reviewing code quality, patterns, and best practices
---

# Code Review Skill

When reviewing code, analyze for:

## Quality Checks
- Clear naming conventions
- Appropriate error handling
- Code duplication
- Complexity and readability

## Security
- Input validation
- Authentication/authorization
- Sensitive data handling

## Best Practices
- SOLID principles
- DRY (Don't Repeat Yourself)
- Single responsibility
- Proper documentation

## Output
Provide feedback in categories:
- 🔴 Critical: Must fix before merge
- 🟡 Important: Should address
- 🟢 Suggestion: Nice to have
"#;
    fs::write(&example_skill_path, example_skill_content)?;
    println!("{} Created {}", "✓".green(), example_skill_path.display());

    // Create context directory (v0.13)
    let context_dir = nika_dir.join("context");
    fs::create_dir_all(&context_dir)?;
    println!("{} Created {}", "✓".green(), context_dir.display());

    // Create example context file
    let context_path = context_dir.join("project.md");
    let context_content = r#"# Project Context

This file provides shared context for all agents and workflows.

## Project Overview

Describe your project here. This context will be available to agents via `memory.context.project`.

## Key Information

- Project name: [Your Project]
- Tech stack: [Your Stack]
- Key conventions: [Your Conventions]
"#;
    fs::write(&context_path, context_content)?;
    println!("{} Created {}", "✓".green(), context_path.display());

    // Create memory directory (v0.13)
    let memory_dir = nika_dir.join("memory");
    fs::create_dir_all(&memory_dir)?;
    println!("{} Created {}", "✓".green(), memory_dir.display());

    // Create proposed directory (for agent-proposed changes)
    let proposed_dir = nika_dir.join("proposed");
    fs::create_dir_all(&proposed_dir)?;
    println!("{} Created {}", "✓".green(), proposed_dir.display());

    // Create cache directory
    let cache_dir = nika_dir.join("cache");
    fs::create_dir_all(&cache_dir)?;
    println!("{} Created {}", "✓".green(), cache_dir.display());

    // Create workflows directory (v0.13 - for sub-workflow composition)
    let workflows_dir = nika_dir.join("workflows");
    fs::create_dir_all(&workflows_dir)?;
    println!("{} Created {}", "✓".green(), workflows_dir.display());

    // Create example sub-workflow (can be called via nika:run)
    let example_subworkflow_path = workflows_dir.join("helpers.nika.yaml");
    let example_subworkflow_content = r#"# Helper Sub-Workflows
# These workflows can be called from parent workflows via nika:run
#
# Usage in parent workflow:
#   tasks:
#     - id: generate_summary
#       invoke:
#         tool: nika:run
#         params:
#           workflow: .nika/workflows/helpers.nika.yaml
#           task: summarize
#           input: "{{use.content}}"

schema: "nika/workflow@0.6"
workflow: helpers
description: "Reusable helper workflows for common tasks"

tasks:
  - id: summarize
    infer:
      prompt: |
        Summarize the following content in 3 bullet points:

        {{use.input}}
      model: claude-sonnet-4-20250514
    output:
      format: text

  - id: translate
    infer:
      prompt: |
        Translate the following text to {{use.target_language | default: "French"}}:

        {{use.input}}
      model: claude-sonnet-4-20250514
    output:
      format: text

  - id: review_code
    infer:
      prompt: |
        Review the following code for bugs, security issues, and improvements:

        ```{{use.language | default: "rust"}}
        {{use.code}}
        ```

        Provide:
        1. Critical issues
        2. Suggestions for improvement
        3. Overall assessment
      model: claude-sonnet-4-20250514
    output:
      format: text
"#;
    fs::write(&example_subworkflow_path, example_subworkflow_content)?;
    println!(
        "{} Created {}",
        "✓".green(),
        example_subworkflow_path.display()
    );

    // Create user.yaml (v0.13)
    let user_path = nika_dir.join("user.yaml");
    let user_content = r#"# Nika User Profile
# Personalize your AI experience

# Your name (used in greetings and personalization)
# name: "Your Name"

# Email (optional, for notifications)
# email: "you@example.com"

# Timezone (for scheduling and timestamps)
timezone: "UTC"

# Preferred language (ISO 639-1 code)
language: "en-US"

# Additional context about you (helps agents understand your preferences)
# context: |
#   I prefer concise responses.
#   I work primarily with Rust and TypeScript.
"#;
    fs::write(&user_path, user_content)?;
    println!("{} Created {}", "✓".green(), user_path.display());

    // Create memory.yaml (v0.13)
    let memory_config_path = nika_dir.join("memory.yaml");
    let memory_config_content = r#"# Nika Memory Configuration
# Persistent memory across sessions

# Enable/disable memory system
enabled: true

# Storage backend: file, sqlite, redis (file is default)
backend: file

# Time-to-live in seconds for memory entries (0 = no expiry)
ttl_secs: 0

# Maximum number of entries to keep (0 = unlimited)
max_entries: 1000

# Memory scopes (named memory buckets)
scopes:
  # Conversation history
  conversation:
    persist: true
    ttl_secs: 86400  # 24 hours

  # Project-specific memory
  project:
    persist: true
    ttl_secs: 0  # Never expires

  # Temporary scratch space
  scratch:
    persist: false
    ttl_secs: 3600  # 1 hour
"#;
    fs::write(&memory_config_path, memory_config_content)?;
    println!("{} Created {}", "✓".green(), memory_config_path.display());

    // Create policies.yaml (v0.13)
    let policies_path = nika_dir.join("policies.yaml");
    let policies_content = r#"# Nika Security Policies
# Control what agents can do

execution:
  # Shell commands that are always allowed (glob patterns)
  allow_commands:
    - "echo *"
    - "cat *"
    - "ls *"
    - "pwd"
    - "date"
    - "git status"
    - "git diff *"
    - "git log *"
    - "cargo *"
    - "npm *"
    - "pnpm *"

  # Shell commands that are always blocked
  block_commands:
    - "rm -rf /*"
    - "sudo *"
    - "chmod 777 *"

  # Require confirmation for potentially destructive commands
  confirm_destructive: true

  # Maximum execution time for any command (seconds)
  max_execution_secs: 300

budget:
  # Daily token limit (0 = unlimited)
  daily_token_limit: 0

  # Monthly cost limit in cents (0 = unlimited)
  monthly_cost_limit_cents: 0

  # Warn when this percentage of budget is reached
  warn_at_percent: 80

network:
  # Domains that can be accessed (empty = all allowed)
  # allow_domains:
  #   - "api.example.com"

  # Domains that are always blocked
  block_domains:
    - "localhost:internal"

  # Allow localhost/127.0.0.1 access
  allow_localhost: true
"#;
    fs::write(&policies_path, policies_content)?;
    println!("{} Created {}", "✓".green(), policies_path.display());

    // Create progressive example workflows unless --no-example
    if !no_example {
        // Create workflows/ directory at project root
        let workflows_dir = cwd.join("workflows");
        fs::create_dir_all(&workflows_dir)?;
        println!("{} Created {}", "✓".green(), workflows_dir.display());

        // Write README.md for workflows
        let readme_path = workflows_dir.join("README.md");
        fs::write(&readme_path, WORKFLOWS_README)?;
        println!("{} Created {}", "✓".green(), readme_path.display());

        // Create tier directories and write all 30 workflows
        let workflows = get_all_workflows();
        let mut created_tiers = std::collections::HashSet::new();

        for workflow in &workflows {
            let tier_dir = workflows_dir.join(workflow.tier_dir);
            if created_tiers.insert(workflow.tier_dir) {
                fs::create_dir_all(&tier_dir)?;
                println!("{} Created {}", "✓".green(), tier_dir.display());
            }
            let wf_path = tier_dir.join(workflow.filename);
            fs::write(&wf_path, workflow.content)?;
            println!("{} Created {}", "✓".green(), wf_path.display());
        }

        // Create context/ directory at project root
        let context_dir = cwd.join("context");
        fs::create_dir_all(&context_dir)?;
        println!("{} Created {}", "✓".green(), context_dir.display());

        // Write all context files
        for ctx_file in get_all_context_files() {
            let ctx_path = context_dir.join(ctx_file.filename);
            fs::write(&ctx_path, ctx_file.content)?;
            println!("{} Created {}", "✓".green(), ctx_path.display());
        }

        // Create partials/ directory inside workflows/ (for include: security)
        let partials_dir = workflows_dir.join("partials");
        fs::create_dir_all(&partials_dir)?;
        println!("{} Created {}", "✓".green(), partials_dir.display());

        // Write all partial workflows
        for partial in get_all_partials() {
            let partial_path = partials_dir.join(partial.filename);
            fs::write(&partial_path, partial.content)?;
            println!("{} Created {}", "✓".green(), partial_path.display());
        }

        // Create schemas/ directory at project root
        let schemas_dir = cwd.join("schemas");
        fs::create_dir_all(&schemas_dir)?;
        println!("{} Created {}", "✓".green(), schemas_dir.display());

        // Write all JSON schemas
        for schema in get_all_schemas() {
            let schema_path = schemas_dir.join(schema.filename);
            fs::write(&schema_path, schema.content)?;
            println!("{} Created {}", "✓".green(), schema_path.display());
        }

        // Create output/ directory at project root with .gitkeep
        let output_dir = cwd.join("output");
        fs::create_dir_all(&output_dir)?;
        fs::write(output_dir.join(".gitkeep"), "")?;
        println!("{} Created {}", "✓".green(), output_dir.display());

        println!();
        println!(
            "{} {} workflows created across {} tiers",
            "✓".green(),
            workflows.len().to_string().cyan(),
            created_tiers.len().to_string().cyan()
        );
    }

    // Print summary
    println!();
    println!("{}", "Nika project initialized! (v0.22.0)".green().bold());
    println!();
    println!(
        "  Permission mode: {}",
        permission_mode.display_name().cyan()
    );
    println!("  Config: {}", config_path.display());
    println!();
    println!("  {} Project structure:", "📁".cyan());
    println!();
    println!(
        "    {}  .nika/             # Nika configuration",
        "⚙️".dimmed()
    );
    println!("    ├── config.toml        # Main configuration");
    println!("    ├── agents/            # Agent definitions");
    println!("    ├── skills/            # Skill definitions");
    println!("    └── ...");
    if !no_example {
        println!();
        println!(
            "    {}  workflows/          # 30 example workflows by tier",
            "📂".cyan()
        );
        println!("    ├── README.md                     # Quick start guide");
        println!("    ├── tier-1-no-deps/  (01-03)      # exec, fetch, builtins");
        println!("    ├── tier-2-llm/      (04-07)      # infer, DAG, for_each");
        println!("    ├── tier-3-agent/    (08-09)      # agent + file tools");
        println!("    ├── tier-4-mcp/      (10)         # NovaNet integration");
        println!("    ├── tier-5-dev/      (11-20)      # Developer use cases");
        println!("    └── tier-6-magic/    (21-30)      # Everyday automation");
        println!();
        println!(
            "    {}  context/            # Context files for workflows",
            "📁".dimmed()
        );
        println!(
            "    {}  partials/           # Reusable workflow fragments",
            "📁".dimmed()
        );
        println!(
            "    {}  schemas/            # JSON schemas for validation",
            "📁".dimmed()
        );
        println!(
            "    {}  output/             # Generated outputs (gitignored)",
            "📁".dimmed()
        );
    }
    println!();
    if !no_example {
        println!("  {} Get started:", "→".cyan());
        println!();
        println!("    # Tier 1: Works immediately (no API key)");
        println!("    nika run workflows/tier-1-no-deps/01-exec-basics.nika.yaml");
        println!();
        println!("    # Tier 2: Setup provider first");
        println!("    nika provider set anthropic");
        println!("    nika run workflows/tier-2-llm/04-infer-basics.nika.yaml");
        println!();
        println!("  {} Learn more:", "📖".cyan());
        println!("    See workflows/README.md for full tier guide");
    }

    // Migrate API keys from env vars to keychain if requested
    #[cfg(feature = "tui")]
    if migrate_keys {
        use nika::tui::widgets::provider_modal::migrate_env_to_keyring;
        println!();
        println!(
            "{}",
            "Migrating API keys from environment variables...".cyan()
        );
        let report = migrate_env_to_keyring();
        println!();
        println!("{}", report.summary());

        if !report.errors.is_empty() {
            println!();
            println!("{}:", "Errors".red());
            for (provider, error) in &report.errors {
                println!("  {} - {}", provider, error);
            }
        }

        if report.migrated > 0 {
            println!();
            println!(
                "{}",
                "NOTE: You can now remove these env vars from your shell config.".yellow()
            );
        }
    }
    #[cfg(not(feature = "tui"))]
    if migrate_keys {
        println!(
            "{} Key migration requires TUI feature. Use: cargo build --features tui",
            "Warning:".yellow()
        );
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// MCP COMMAND (v0.27)
// ═══════════════════════════════════════════════════════════════════════════

/// Handle MCP server management commands
async fn handle_mcp_command(action: McpAction) -> Result<(), NikaError> {
    use colored::Colorize;
    use nika::core::{
        add_server_to_global, add_server_to_project, aliases_by_category, global_config_path,
        load_global_config, load_merged_config, load_project_config, project_config_path,
        remove_server_from_global, remove_server_from_project, resolve_name,
        server_from_npm_package, McpServer as CoreMcpServer,
    };

    match action {
        McpAction::Add {
            name,
            global,
            project,
            description,
            command,
            args,
        } => {
            // Determine scope (default to project if neither specified)
            let use_global = global && !project;

            // Resolve alias to package name (or use as-is if it's a package)
            let package = resolve_name(&name).ok_or_else(|| NikaError::ValidationError {
                reason: format!(
                    "Unknown alias '{}'. Use 'nika mcp aliases' to see available aliases, \
                     or specify a full npm package like '@org/package'",
                    name
                ),
            })?;

            // Create server config
            let server = if let (Some(cmd), Some(args)) = (command, args) {
                // Custom command/args
                CoreMcpServer {
                    command: cmd,
                    args,
                    env: std::collections::HashMap::new(),
                    description,
                    enabled: true,
                    source: None,
                }
            } else {
                // Default npx -y <package>
                server_from_npm_package(&package, description.as_deref())
            };

            // Add to appropriate config
            let (added, scope, path) = if use_global {
                let added = add_server_to_global(&name, server).map_err(|e| {
                    NikaError::ValidationError {
                        reason: format!("Failed to add to global config: {}", e),
                    }
                })?;
                (added, "global", global_config_path())
            } else {
                let added = add_server_to_project(&name, server).map_err(|e| {
                    NikaError::ValidationError {
                        reason: format!("Failed to add to project config: {}", e),
                    }
                })?;
                (added, "project", project_config_path())
            };

            if added {
                println!(
                    "{} Added '{}' to {} config",
                    "✓".green(),
                    name.bold(),
                    scope
                );
                if let Some(ref p) = path {
                    let path_str = p.display().to_string();
                    println!("  {}", path_str.dimmed());
                }
                if name != package {
                    println!("  Alias '{}' → {}", name, package.dimmed());
                }
            } else {
                println!(
                    "{} Server '{}' already exists in {} config",
                    "ℹ".cyan(),
                    name,
                    scope
                );
            }
            Ok(())
        }

        McpAction::Remove {
            name,
            global,
            project,
            yes,
        } => {
            // Determine scope (default to project if neither specified)
            let use_global = global && !project;

            // Confirmation (unless --yes)
            if !yes {
                let scope = if use_global { "global" } else { "project" };
                print!("Remove '{}' from {} config? [y/N] ", name.bold(), scope);
                use std::io::Write;
                std::io::stdout().flush().unwrap();

                let mut input = String::new();
                std::io::stdin().read_line(&mut input).unwrap();
                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("{} Cancelled", "ℹ".cyan());
                    return Ok(());
                }
            }

            // Remove from appropriate config
            let (removed, scope) = if use_global {
                let removed =
                    remove_server_from_global(&name).map_err(|e| NikaError::ValidationError {
                        reason: format!("Failed to remove from global config: {}", e),
                    })?;
                (removed, "global")
            } else {
                let removed =
                    remove_server_from_project(&name).map_err(|e| NikaError::ValidationError {
                        reason: format!("Failed to remove from project config: {}", e),
                    })?;
                (removed, "project")
            };

            if removed {
                println!(
                    "{} Removed '{}' from {} config",
                    "✓".green(),
                    name.bold(),
                    scope
                );
            } else {
                println!(
                    "{} Server '{}' not found in {} config",
                    "ℹ".cyan(),
                    name,
                    scope
                );
            }
            Ok(())
        }

        McpAction::List {
            workflow,
            global,
            project,
            json,
        } => {
            // If workflow provided, show workflow's MCP config
            if let Some(file) = workflow {
                let yaml = tokio::fs::read_to_string(&file).await?;
                let wf: Workflow = serde_yaml::from_str(&yaml)?;

                match wf.mcp {
                    Some(ref mcp_servers) if !mcp_servers.is_empty() => {
                        if json {
                            println!("{}", serde_json::to_string_pretty(&mcp_servers)?);
                        } else {
                            println!("{}", "MCP Servers (workflow)".bold());
                            println!("{}", "─".repeat(60));

                            for (name, config) in mcp_servers {
                                println!(
                                    "  {:12} {} {}",
                                    name.cyan(),
                                    config.command.dimmed(),
                                    config.args.join(" ").dimmed()
                                );
                                if !config.env.is_empty() {
                                    for key in config.env.keys() {
                                        println!("               env: {}", key.yellow());
                                    }
                                }
                            }
                            println!();
                            println!(
                                "{}",
                                format!("Use 'nika mcp test {} <server>' to test connection", file)
                                    .dimmed()
                            );
                        }
                    }
                    _ => {
                        println!("{} No MCP servers defined in {}", "ℹ".cyan(), file);
                    }
                }
                return Ok(());
            }

            // Show global and/or project configs
            let show_global = global || !project;
            let show_project = project || !global;

            if json {
                // JSON output
                let merged = load_merged_config().map_err(|e| NikaError::ValidationError {
                    reason: format!("Failed to load config: {}", e),
                })?;
                println!("{}", serde_json::to_string_pretty(&merged.servers)?);
                return Ok(());
            }

            // Text output
            let mut any_servers = false;

            if show_global {
                match load_global_config() {
                    Ok(Some(config)) if !config.servers.is_empty() => {
                        any_servers = true;
                        println!("{}", "Global MCP Servers".bold());
                        if let Some(p) = global_config_path() {
                            println!("{}", p.display().to_string().dimmed());
                        }
                        println!("{}", "─".repeat(60));

                        for (name, server) in &config.servers {
                            let status = if server.enabled {
                                "✓".green()
                            } else {
                                "○".dimmed()
                            };
                            println!(
                                "  {} {:12} {} {}",
                                status,
                                name.cyan(),
                                server.command.dimmed(),
                                server.args.join(" ").dimmed()
                            );
                        }
                        println!();
                    }
                    Ok(Some(_)) | Ok(None) => {
                        if global && !project {
                            println!("{} No global MCP servers configured", "ℹ".cyan());
                            if let Some(p) = global_config_path() {
                                println!("  Config path: {}", p.display());
                            }
                        }
                    }
                    Err(e) => {
                        println!("{} Failed to load global config: {}", "✗".red(), e);
                    }
                }
            }

            if show_project {
                match load_project_config() {
                    Ok(Some(config)) if !config.servers.is_empty() => {
                        any_servers = true;
                        println!("{}", "Project MCP Servers".bold());
                        if let Some(p) = project_config_path() {
                            println!("{}", p.display().to_string().dimmed());
                        }
                        println!("{}", "─".repeat(60));

                        for (name, server) in &config.servers {
                            let status = if server.enabled {
                                "✓".green()
                            } else {
                                "○".dimmed()
                            };
                            println!(
                                "  {} {:12} {} {}",
                                status,
                                name.cyan(),
                                server.command.dimmed(),
                                server.args.join(" ").dimmed()
                            );
                        }
                        println!();
                    }
                    Ok(Some(_)) | Ok(None) => {
                        if project && !global {
                            println!("{} No project MCP servers configured", "ℹ".cyan());
                            if let Some(p) = project_config_path() {
                                println!("  Config path: {}", p.display());
                            }
                        }
                    }
                    Err(e) => {
                        println!("{} Failed to load project config: {}", "✗".red(), e);
                    }
                }
            }

            if !any_servers && !global && !project {
                println!("{} No MCP servers configured", "ℹ".cyan());
                println!();
                println!("Add servers with:");
                println!("  nika mcp add neo4j          # Add to project config");
                println!("  nika mcp add neo4j --global # Add to global config");
                println!();
                println!("Or specify a workflow file:");
                println!("  nika mcp list --workflow my-flow.nika.yaml");
            }
            Ok(())
        }

        McpAction::Aliases { category } => {
            println!("{}", "MCP Server Aliases (48 total)".bold());
            println!("{}", "─".repeat(60));

            if let Some(cat) = category {
                let aliases = aliases_by_category(&cat);
                if aliases.is_empty() {
                    println!(
                        "{} Unknown category '{}'. Available: anthropic, databases, search, developer, productivity, ai",
                        "✗".red(),
                        cat
                    );
                } else {
                    println!();
                    println!("{}", cat.to_uppercase().bold());
                    for (alias, package) in aliases {
                        println!("  {:16} → {}", alias.cyan(), package.dimmed());
                    }
                }
            } else {
                // Show all categories
                let categories = [
                    ("anthropic", "Anthropic Official"),
                    ("databases", "Databases"),
                    ("search", "Search & Web"),
                    ("developer", "Developer Tools"),
                    ("productivity", "Productivity"),
                    ("ai", "AI & Specialized"),
                ];

                for (cat_key, cat_name) in categories {
                    println!();
                    println!("{}", cat_name.bold());
                    for (alias, package) in aliases_by_category(cat_key) {
                        println!("  {:16} → {}", alias.cyan(), package.dimmed());
                    }
                }
            }

            println!();
            println!(
                "{}",
                "Use: nika mcp add <alias>  (e.g., nika mcp add neo4j)".dimmed()
            );
            Ok(())
        }

        McpAction::Test { workflow, server } => {
            println!("Testing connection to MCP server '{}'...", server.bold());

            // Load workflow
            let yaml = tokio::fs::read_to_string(&workflow).await?;
            let wf: Workflow = serde_yaml::from_str(&yaml)?;

            // Get MCP config
            let mcp_servers = wf.mcp.as_ref().ok_or_else(|| NikaError::ValidationError {
                reason: format!("No mcp: section in {}", workflow),
            })?;

            let inline_config =
                mcp_servers
                    .get(&server)
                    .ok_or_else(|| NikaError::McpNotConnected {
                        name: server.clone(),
                    })?;

            // Build McpConfig
            let mut config = McpConfig::new(&server, &inline_config.command)
                .with_args(inline_config.args.iter().cloned());
            for (key, value) in &inline_config.env {
                config = config.with_env(key, value);
            }
            if let Some(ref cwd) = inline_config.cwd {
                config = config.with_cwd(cwd);
            }

            // Connect
            let client = McpClient::new(config)?;

            match client.connect().await {
                Ok(()) => {
                    println!("{} Connection successful!", "✓".green());

                    // List tools
                    match client.list_tools().await {
                        Ok(tools) => {
                            println!("  Found {} tools", tools.len());
                        }
                        Err(e) => {
                            println!("  {} Failed to list tools: {}", "⚠".yellow(), e);
                        }
                    }
                }
                Err(e) => {
                    println!("{} Connection failed: {}", "✗".red(), e);
                }
            }
            Ok(())
        }

        McpAction::Tools { workflow, server } => {
            // Load workflow
            let yaml = tokio::fs::read_to_string(&workflow).await?;
            let wf: Workflow = serde_yaml::from_str(&yaml)?;

            // Get MCP config
            let mcp_servers = wf.mcp.as_ref().ok_or_else(|| NikaError::ValidationError {
                reason: format!("No mcp: section in {}", workflow),
            })?;

            let inline_config =
                mcp_servers
                    .get(&server)
                    .ok_or_else(|| NikaError::McpNotConnected {
                        name: server.clone(),
                    })?;

            // Build McpConfig
            let mut config = McpConfig::new(&server, &inline_config.command)
                .with_args(inline_config.args.iter().cloned());
            for (key, value) in &inline_config.env {
                config = config.with_env(key, value);
            }
            if let Some(ref cwd) = inline_config.cwd {
                config = config.with_cwd(cwd);
            }

            println!("Connecting to MCP server '{}'...", server.bold());

            // Connect
            let client = McpClient::new(config)?;
            client.connect().await?;

            // List tools
            let tools = client.list_tools().await?;

            println!();
            println!("{}", format!("Tools from '{}'", server).bold());
            println!("{}", "─".repeat(60));

            if tools.is_empty() {
                println!("  {} No tools available", "ℹ".cyan());
            } else {
                for tool in &tools {
                    println!("  {} {}", "•".cyan(), tool.name.bold());
                    if let Some(ref desc) = tool.description {
                        // Truncate long descriptions
                        let desc_truncated: String = desc.chars().take(80).collect();
                        if desc.len() > 80 {
                            println!("    {}", format!("{}...", desc_truncated).dimmed());
                        } else {
                            println!("    {}", desc_truncated.dimmed());
                        }
                    }
                }
            }

            println!();
            println!("{} tools available", tools.len());
            Ok(())
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PACKAGE COMMAND HANDLER (v0.27)
// ═══════════════════════════════════════════════════════════════════════════

/// Handle package management commands.
///
/// Manages packages (workflows, skills, schemas) stored in ~/.nika/packages/
async fn handle_pkg_command(action: PkgAction) -> Result<(), NikaError> {
    use colored::Colorize;
    use nika::registry::{list_installed, load_manifest, load_registry};

    match action {
        PkgAction::List { json } => {
            let packages = list_installed()?;

            if json {
                let registry = load_registry()?;
                println!("{}", serde_json::to_string_pretty(&registry)?);
            } else if packages.is_empty() {
                println!("{} No packages installed", "ℹ".cyan());
                println!();
                println!("Install packages with:");
                println!("  nika pkg add @nika/seo-audit");
                println!("  nika pkg install  # Install from nika.yaml");
            } else {
                println!("{}", "Installed Packages".bold());
                println!("{}", "─".repeat(60));

                for (name, version) in &packages {
                    println!("  {}@{}", name.cyan(), version.green());
                }

                println!();
                println!("{} package(s) installed", packages.len());
            }
            Ok(())
        }

        PkgAction::Info { package } => {
            // Try to find the installed version
            let registry = load_registry()?;

            if let Some(installed) = registry.get(&package) {
                println!("{}", format!("Package: {}", package).bold());
                println!("{}", "─".repeat(60));
                println!("  Version:   {}", installed.version.green());
                println!("  Path:      {}", installed.manifest_path.dimmed());
                println!("  Installed: {}", installed.installed_at.dimmed());

                // Try to load manifest for more details
                if let Ok(manifest) = load_manifest(&package, &installed.version) {
                    if let Some(ref desc) = manifest.description {
                        println!("  Description: {}", desc);
                    }
                    if !manifest.skills.is_empty() {
                        println!();
                        println!("  Skills:");
                        for (name, skill) in &manifest.skills {
                            println!("    • {} ({})", name.cyan(), skill.path.dimmed());
                        }
                    }
                }
            } else {
                println!("{} Package '{}' not installed", "ℹ".cyan(), package);
                println!();
                println!("To install: nika pkg add {}", package);
            }
            Ok(())
        }

        PkgAction::Add {
            package,
            r#type,
            version,
            dev: _dev,
        } => {
            use nika::registry::{
                ensure_nika_home, is_version_installed, package_dir, save_registry,
                InstalledPackage, RegistryClient,
            };

            println!("{} Adding package: {}", "📦".cyan(), package.green());

            // Infer type from scope if not provided
            let pkg_type = r#type
                .as_deref()
                .or_else(|| infer_package_type(&package))
                .unwrap_or("workflow");

            println!("  Type: {}", pkg_type.dimmed());

            // Ensure ~/.nika/ exists
            ensure_nika_home()?;

            // Create registry client
            let client = RegistryClient::new();

            // Fetch package info from registry
            println!("  {} Fetching package info...", "→".dimmed());
            let pkg_info =
                client
                    .get_package(&package)
                    .await
                    .map_err(|_e| NikaError::PackageNotFound {
                        name: package.clone(),
                        version: "latest".to_string(),
                    })?;

            // Determine version to install
            let target_version = version.as_deref().unwrap_or(&pkg_info.latest_version);
            println!("  {} Version: {}", "→".dimmed(), target_version.green());

            // Check if already installed
            if is_version_installed(&package, target_version)? {
                println!(
                    "{} {}@{} is already installed",
                    "✓".green(),
                    package.cyan(),
                    target_version.green()
                );
                return Ok(());
            }

            // Get target directory
            let target_dir = package_dir(&package, target_version)?;
            println!(
                "  {} Installing to: {}",
                "→".dimmed(),
                target_dir.display().to_string().dimmed()
            );

            // Download and extract package
            println!("  {} Downloading...", "→".dimmed());
            client
                .download_and_extract(&package, target_version, &target_dir)
                .await
                .map_err(|e| NikaError::ValidationError {
                    reason: format!("Failed to download package: {}", e),
                })?;

            // Update registry index
            let mut registry = load_registry()?;
            let manifest_path = format!("packages/{}/{}/manifest.yaml", package, target_version);
            registry.insert(
                package.clone(),
                InstalledPackage::now(target_version.to_string(), manifest_path),
            );
            save_registry(&registry)?;

            println!();
            println!(
                "{} Successfully installed {}@{}",
                "✓".green(),
                package.cyan(),
                target_version.green()
            );

            // Show installed skills if any
            if let Ok(manifest) = load_manifest(&package, target_version) {
                if !manifest.skills.is_empty() {
                    println!();
                    println!("  {} Skills:", "📚".cyan());
                    for (name, skill) in &manifest.skills {
                        println!("    • {} ({})", name.cyan(), skill.path.dimmed());
                    }
                }
            }

            Ok(())
        }

        PkgAction::Remove { package, yes: _ } => {
            use nika::registry::{package_dir, save_registry};

            println!("{} Removing package: {}", "🗑".red(), package);

            // Check if installed
            let mut registry = load_registry()?;
            let installed = match registry.get(&package) {
                Some(pkg) => pkg.clone(),
                None => {
                    println!("{} Package '{}' is not installed", "ℹ".cyan(), package);
                    return Ok(());
                }
            };

            // Get package directory
            let pkg_dir = package_dir(&package, &installed.version)?;

            // Remove package directory
            if pkg_dir.exists() {
                println!(
                    "  {} Removing {}",
                    "→".dimmed(),
                    pkg_dir.display().to_string().dimmed()
                );
                std::fs::remove_dir_all(&pkg_dir).map_err(|e| NikaError::ValidationError {
                    reason: format!("Failed to remove package directory: {}", e),
                })?;

                // Clean up empty parent directories
                if let Some(parent) = pkg_dir.parent() {
                    if parent
                        .read_dir()
                        .map(|mut d| d.next().is_none())
                        .unwrap_or(false)
                    {
                        let _ = std::fs::remove_dir(parent);
                    }
                }
            }

            // Update registry
            registry.remove(&package);
            save_registry(&registry)?;

            println!();
            println!(
                "{} Successfully removed {}@{}",
                "✓".green(),
                package.cyan(),
                installed.version.green()
            );
            Ok(())
        }

        PkgAction::Install { frozen } => {
            use nika::registry::{
                ensure_nika_home, is_version_installed, package_dir, save_registry,
                InstalledPackage, Lockfile, RegistryClient,
            };

            println!(
                "{} Installing packages from project manifest{}",
                "📦".cyan(),
                if frozen { " (frozen)" } else { "" }
            );

            // Ensure ~/.nika/ exists
            ensure_nika_home()?;

            // Find project manifest
            let manifest_path = if Path::new("nika.yaml").exists() {
                PathBuf::from("nika.yaml")
            } else {
                println!("{} No project manifest found (nika.yaml)", "⚠".yellow());
                println!();
                println!("Create one with:");
                println!("  nika init");
                return Ok(());
            };

            println!("  {} Reading {}", "→".dimmed(), manifest_path.display());

            // Read manifest
            let content =
                fs::read_to_string(&manifest_path).map_err(|e| NikaError::ValidationError {
                    reason: format!("Failed to read {}: {}", manifest_path.display(), e),
                })?;

            // Parse manifest to get dependencies
            #[derive(serde::Deserialize)]
            struct ProjectManifest {
                #[serde(default)]
                dependencies: std::collections::HashMap<String, String>,
            }

            let manifest: ProjectManifest =
                serde_yaml::from_str(&content).map_err(|e| NikaError::ParseError {
                    details: format!("Failed to parse manifest: {}", e),
                })?;

            if manifest.dependencies.is_empty() {
                println!("{} No dependencies to install", "ℹ".cyan());
                return Ok(());
            }

            // Load lockfile for frozen installs
            let lockfile = if frozen {
                println!("  {} Reading nika.lock", "→".dimmed());
                Lockfile::load(None).unwrap_or_else(|_| {
                    println!(
                        "{} No nika.lock found, will use latest versions",
                        "⚠".yellow()
                    );
                    Lockfile::new()
                })
            } else {
                Lockfile::new()
            };

            // Create registry client
            let client = RegistryClient::new();
            let mut registry = load_registry()?;
            let mut installed_count = 0;
            let mut skipped_count = 0;

            println!();
            println!(
                "{} Installing {} dependencies...",
                "📦".cyan(),
                manifest.dependencies.len()
            );

            for (name, version_spec) in &manifest.dependencies {
                // Determine version to install
                let target_version = if frozen {
                    // Use locked version if available
                    lockfile
                        .find_version(name)
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| version_spec.clone())
                } else {
                    // Use version spec or fetch latest
                    if version_spec == "*" || version_spec == "latest" {
                        match client.get_package(name).await {
                            Ok(info) => info.latest_version.clone(),
                            Err(_) => {
                                println!("  {} {} - not found", "✗".red(), name.cyan());
                                continue;
                            }
                        }
                    } else {
                        version_spec.trim_start_matches('^').to_string()
                    }
                };

                // Check if already installed
                if is_version_installed(name, &target_version)? {
                    println!(
                        "  {} {}@{} (already installed)",
                        "✓".green(),
                        name.cyan(),
                        target_version.dimmed()
                    );
                    skipped_count += 1;
                    continue;
                }

                // Install package
                let target_dir = package_dir(name, &target_version)?;
                match client
                    .download_and_extract(name, &target_version, &target_dir)
                    .await
                {
                    Ok(_) => {
                        // Update registry
                        let manifest_path =
                            format!("packages/{}/{}/manifest.yaml", name, target_version);
                        registry.insert(
                            name.clone(),
                            InstalledPackage::now(target_version.clone(), manifest_path),
                        );
                        println!(
                            "  {} {}@{}",
                            "✓".green(),
                            name.cyan(),
                            target_version.green()
                        );
                        installed_count += 1;
                    }
                    Err(e) => {
                        println!(
                            "  {} {} - {}",
                            "✗".red(),
                            name.cyan(),
                            e.to_string().dimmed()
                        );
                    }
                }
            }

            // Save registry
            save_registry(&registry)?;

            println!();
            if installed_count > 0 || skipped_count > 0 {
                println!(
                    "{} {} package(s) installed, {} already up to date",
                    "✓".green(),
                    installed_count,
                    skipped_count
                );
            }

            Ok(())
        }

        PkgAction::Update { package } => {
            if let Some(ref pkg) = package {
                println!("{} Updating package: {}", "🔄".cyan(), pkg.green());
            } else {
                println!("{} Updating all packages", "🔄".cyan());
            }

            println!();
            println!("{} Package update is not yet implemented", "⚠".yellow());
            Ok(())
        }

        PkgAction::Outdated => {
            println!("{} Checking for outdated packages...", "📋".cyan());

            println!();
            println!(
                "{} Outdated package detection is not yet implemented",
                "⚠".yellow()
            );
            Ok(())
        }

        PkgAction::Search {
            query,
            r#type,
            limit,
        } => {
            use nika::registry::RegistryClient;

            // Build search query - append type filter if provided
            let search_query = if let Some(ref t) = r#type {
                format!("{} type:{}", query, t)
            } else {
                query.clone()
            };

            println!(
                "{} Searching registry for '{}'...",
                "🔍".cyan(),
                query.green()
            );

            if let Some(ref t) = r#type {
                println!("  Type filter: {}", t.dimmed());
            }

            // Create registry client
            let client = RegistryClient::new();

            // Search registry (page=1, per_page=limit)
            let response = client.search(&search_query, 1, limit).await.map_err(|e| {
                NikaError::ValidationError {
                    reason: format!("Search failed: {}", e),
                }
            })?;

            println!();
            if response.results.is_empty() {
                println!("{} No packages found matching '{}'", "ℹ".cyan(), query);
            } else {
                println!(
                    "{} Found {} package(s):",
                    "📦".cyan(),
                    response.results.len()
                );
                println!("{}", "─".repeat(60));

                for result in &response.results {
                    println!(
                        "  {} {}",
                        result.name.cyan(),
                        format!("v{}", result.version).green()
                    );
                    if let Some(ref desc) = result.description {
                        println!("    {}", desc.dimmed());
                    }
                    if let Some(ref keywords) = result.keywords {
                        if !keywords.is_empty() {
                            println!("    Keywords: {}", keywords.join(", ").dimmed());
                        }
                    }
                }

                println!("{}", "─".repeat(60));
                println!();
                println!("Install with: nika pkg add <package-name>");
            }
            Ok(())
        }
    }
}

/// Infer package type from scope prefix
fn infer_package_type(package: &str) -> Option<&'static str> {
    if package.starts_with("@workflows/") || package.starts_with("@nika/") {
        Some("workflow")
    } else if package.starts_with("@agents/") {
        Some("agent")
    } else if package.starts_with("@skills/") {
        Some("skill")
    } else if package.starts_with("@prompts/") {
        Some("prompt")
    } else if package.starts_with("@jobs/") {
        Some("job")
    } else if package.starts_with("@schemas/") || package.starts_with("@novanet/") {
        Some("schema")
    } else {
        None
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SYNC COMMAND HANDLER (v0.27)
// ═══════════════════════════════════════════════════════════════════════════

/// Handle sync commands to IDE configurations.
///
/// Syncs MCP servers from ~/.nika/mcp.yaml to editor-specific configs:
/// - Claude Code: .claude/settings.json
/// - Cursor: .cursor/mcp.json
/// - Windsurf: .windsurf/mcp.json
/// - VS Code: .vscode/settings.json
async fn handle_sync_command(
    action: Option<SyncAction>,
    target: Option<String>,
    dry_run: bool,
    quiet: bool,
) -> Result<(), NikaError> {
    use colored::Colorize;

    // Handle subcommands first
    if let Some(sync_action) = action {
        match sync_action {
            SyncAction::Status => {
                println!("{}", "Sync Status".bold());
                println!("{}", "─".repeat(60));
                println!();

                // Show detected IDEs
                let cwd = std::env::current_dir().map_err(|e| NikaError::ConfigError {
                    reason: format!("Failed to get current directory: {}", e),
                })?;

                let ides = [
                    ("Claude Code", ".claude"),
                    ("Cursor", ".cursor"),
                    ("VS Code", ".vscode"),
                    ("Windsurf", ".windsurf"),
                ];

                println!("  Detected IDEs in current directory:");
                for (name, dir) in &ides {
                    let path = cwd.join(dir);
                    if path.exists() {
                        println!("    {} {} ({})", "✓".green(), name, path.display());
                    } else {
                        println!("    {} {} (not found)", "○".dimmed(), name.dimmed());
                    }
                }

                println!();
                println!("  Run 'nika sync' to sync MCP servers to detected IDEs");
                return Ok(());
            }

            SyncAction::Enable { editor } => {
                println!("{} Enabling sync for: {}", "✓".green(), editor.cyan());
                println!();
                println!("{} Sync enable not yet fully implemented", "⚠".yellow());
                println!("  Until then, use: nika sync --enable {}", editor);
                return Ok(());
            }

            SyncAction::Disable { editor } => {
                println!("{} Disabling sync for: {}", "ℹ".cyan(), editor);
                println!();
                println!("{} Sync disable not yet fully implemented", "⚠".yellow());
                println!("  Until then, use: nika sync --disable {}", editor);
                return Ok(());
            }
        }
    }

    // Default sync operation
    if dry_run {
        println!("{}", "🔍 Dry run - showing what would be synced:".cyan());
    } else {
        println!("{}", "🔄 Syncing to IDE configurations...".cyan());
    }
    println!();

    // Load MCP config
    use nika::core::{global_config_path, load_merged_config};

    let mcp_config = load_merged_config().map_err(|e| NikaError::ConfigError {
        reason: format!("Failed to load MCP config: {}", e),
    })?;

    if mcp_config.servers.is_empty() {
        if !quiet {
            println!("{} No MCP servers configured", "ℹ".cyan());
            println!();
            println!("  Add servers with: nika mcp add <name>");
            if let Some(path) = global_config_path() {
                println!("  Config file: {}", path.display());
            }
        }
        return Ok(());
    }

    println!(
        "  {} MCP servers in config:",
        mcp_config.servers.len().to_string().cyan()
    );
    for name in mcp_config.servers.keys() {
        println!("    • {}", name);
    }
    println!();

    // Determine targets
    let targets: Vec<&str> = if let Some(ref t) = target {
        vec![t.as_str()]
    } else {
        // Auto-detect IDEs
        let cwd = std::env::current_dir().unwrap_or_default();
        let mut found = Vec::new();
        if cwd.join(".claude").exists() {
            found.push("claude-code");
        }
        if cwd.join(".cursor").exists() {
            found.push("cursor");
        }
        if cwd.join(".vscode").exists() {
            found.push("vscode");
        }
        if cwd.join(".windsurf").exists() {
            found.push("windsurf");
        }
        found
    };

    if targets.is_empty() {
        println!(
            "{}",
            "⚠️  No IDE configurations found in current directory.".yellow()
        );
        println!("   Create .claude/, .cursor/, .vscode/, or .windsurf/ to enable sync.");
        return Ok(());
    }

    // Sync to each target
    for ide in &targets {
        if dry_run {
            println!(
                "  Would sync to: {} ({} servers)",
                ide.cyan(),
                mcp_config.servers.len()
            );
        } else {
            println!(
                "  {} {} → {} servers",
                "✓".green(),
                ide.cyan(),
                mcp_config.servers.len()
            );
        }
    }

    println!();
    if dry_run {
        println!("Run without {} to apply changes.", "--dry-run".cyan());
    } else {
        println!("{} Sync complete!", "✓".green());
        println!();
        println!(
            "{} Full sync with interactive mode available via:",
            "ℹ".cyan()
        );
        println!("  nika sync --interactive");
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// SETUP COMMAND HANDLER (v0.27)
// ═══════════════════════════════════════════════════════════════════════════

/// Handle setup wizard commands.
///
/// Provides interactive setup for the SuperNovae ecosystem:
/// - Provider configuration (API keys)
/// - Tool installation (nika, novanet)
/// - IDE integrations (Claude Code, Cursor, etc.)
async fn handle_setup_command(action: Option<SetupAction>, quiet: bool) -> Result<(), NikaError> {
    use colored::Colorize;

    let setup_action = action.unwrap_or(SetupAction::Wizard);

    match setup_action {
        SetupAction::Wizard => {
            // Launch the TUI Setup Wizard
            #[cfg(feature = "tui")]
            if !quiet {
                nika::tui::run_tui_wizard().await?;
            } else {
                // Quiet mode: show minimal CLI output instead
                println!("{}", "Setup wizard skipped (quiet mode)".dimmed());
                println!("Run 'nika setup' without --quiet for the full wizard.");
            }
            #[cfg(not(feature = "tui"))]
            {
                let _ = quiet; // suppress unused warning
                println!(
                    "{}",
                    "TUI wizard not available (tui feature disabled)".dimmed()
                );
                println!("Compile with --features tui for the full wizard.");
            }
        }

        SetupAction::Nika => {
            println!("{}", "🦋 Setting up Nika...".cyan().bold());
            println!("{}", "─".repeat(60));
            println!();

            // Check if nika is in PATH by trying to run it
            let nika_check = std::process::Command::new("nika").arg("--version").output();

            if let Ok(output) = nika_check {
                if output.status.success() {
                    println!("  {} Nika CLI found in PATH", "✓".green());
                    let version = String::from_utf8_lossy(&output.stdout);
                    println!("  Version: {}", version.trim().dimmed());
                } else {
                    println!("  {} Nika CLI not found in PATH", "○".yellow());
                    println!();
                    println!("  Install with:");
                    println!("    cargo install --path tools/nika --locked");
                }
            } else {
                println!("  {} Nika CLI not found in PATH", "○".yellow());
                println!();
                println!("  Install with:");
                println!("    cargo install --path tools/nika --locked");
            }

            println!();
            if !quiet {
                println!("{} Full Nika setup available via:", "ℹ".cyan());
                println!("  nika setup nika");
            }
        }

        SetupAction::Novanet => {
            println!("{}", "🧠 Setting up NovaNet...".cyan().bold());
            println!("{}", "─".repeat(60));
            println!();

            // Check if novanet is in PATH by trying to run it
            let novanet_check = std::process::Command::new("novanet")
                .arg("--version")
                .output();

            if let Ok(output) = novanet_check {
                if output.status.success() {
                    println!("  {} NovaNet CLI found in PATH", "✓".green());
                    let version = String::from_utf8_lossy(&output.stdout);
                    println!("  Version: {}", version.trim().dimmed());
                } else {
                    println!("  {} NovaNet CLI not found in PATH", "○".yellow());
                }
            } else {
                println!("  {} NovaNet CLI not found in PATH", "○".yellow());
            }

            // Check Neo4j
            let neo4j_uri = std::env::var("NEO4J_URI").unwrap_or_default();
            if !neo4j_uri.is_empty() {
                println!("  {} NEO4J_URI configured", "✓".green());
            } else {
                println!("  {} NEO4J_URI not set", "○".yellow());
            }

            println!();
            if !quiet {
                println!("{} Full NovaNet setup available via:", "ℹ".cyan());
                println!("  nika setup novanet");
            }
        }

        SetupAction::ClaudeCode => {
            println!("{}", "🤖 Setting up Claude Code...".cyan().bold());
            println!("{}", "─".repeat(60));
            println!();

            let cwd = std::env::current_dir().unwrap_or_default();
            let claude_dir = cwd.join(".claude");

            if claude_dir.exists() {
                println!("  {} .claude/ directory exists", "✓".green());

                let settings = claude_dir.join("settings.json");
                if settings.exists() {
                    println!("  {} settings.json exists", "✓".green());
                } else {
                    println!("  {} settings.json not found", "○".yellow());
                }
            } else {
                println!(
                    "  {} .claude/ not found - run 'nika init' first",
                    "○".yellow()
                );
            }

            println!();
            if !quiet {
                println!("{} Full Claude Code setup available via:", "ℹ".cyan());
                println!("  nika setup claude-code");
            }
        }

        SetupAction::Cursor => {
            println!("{}", "📝 Setting up Cursor...".cyan().bold());
            setup_ide_helper("cursor", ".cursor", quiet)?;
        }

        SetupAction::Vscode => {
            println!("{}", "💻 Setting up VS Code...".cyan().bold());
            setup_ide_helper("vscode", ".vscode", quiet)?;
        }

        SetupAction::Windsurf => {
            println!("{}", "🏄 Setting up Windsurf...".cyan().bold());
            setup_ide_helper("windsurf", ".windsurf", quiet)?;
        }
    }

    Ok(())
}

/// Helper for IDE setup
fn setup_ide_helper(ide_name: &str, dir_name: &str, quiet: bool) -> Result<(), NikaError> {
    use colored::Colorize;

    println!("{}", "─".repeat(60));
    println!();

    let cwd = std::env::current_dir().unwrap_or_default();
    let ide_dir = cwd.join(dir_name);

    if ide_dir.exists() {
        println!("  {} {}/ directory exists", "✓".green(), dir_name);
    } else {
        println!("  {} {}/ not found", "○".yellow(), dir_name);
    }

    println!();
    if !quiet {
        println!("{} Full {} setup available via:", "ℹ".cyan(), ide_name);
        println!("  nika setup {}", ide_name);
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// DAEMON COMMAND HANDLER (v0.27)
// ═══════════════════════════════════════════════════════════════════════════

/// Handle daemon management commands.
///
/// The daemon binary (nika-daemon) is kept external. These commands proxy to `nika-daemon`.
async fn handle_daemon_command(action: DaemonAction, quiet: bool) -> Result<(), NikaError> {
    use colored::Colorize;

    // Helper to run nika-daemon commands
    fn run_nika_daemon(args: &[&str]) -> Result<bool, std::io::Error> {
        let status = std::process::Command::new("nika-daemon")
            .args(args)
            .status()?;
        Ok(status.success())
    }

    // Helper to print nika-daemon not found error
    fn daemon_not_found_error() -> NikaError {
        eprintln!();
        eprintln!("nika-daemon not found. Install from:");
        eprintln!("  cargo install nika-daemon");
        NikaError::InvalidConfig {
            message: "nika-daemon not found".to_string(),
        }
    }

    match action {
        DaemonAction::Start {
            foreground,
            skip_preload,
        } => {
            if !quiet {
                println!("{}", "🔧 Starting nika daemon...".cyan().bold());
            }

            let mut args = vec!["daemon", "start"];
            if foreground {
                args.push("--foreground");
            }
            if skip_preload {
                args.push("--skip-preload");
            }

            match run_nika_daemon(&args) {
                Ok(true) => {
                    if !quiet {
                        println!("  {} Daemon started", "✓".green());
                    }
                    Ok(())
                }
                Ok(false) => {
                    eprintln!("{}", "Failed to start daemon".red());
                    Err(NikaError::InvalidConfig {
                        message: "Failed to start daemon".to_string(),
                    })
                }
                Err(_) => {
                    eprintln!("{} nika-daemon not found", "✗".red());
                    Err(daemon_not_found_error())
                }
            }
        }

        DaemonAction::Stop => {
            if !quiet {
                println!("{}", "🛑 Stopping nika daemon...".cyan().bold());
            }

            match run_nika_daemon(&["stop"]) {
                Ok(true) => {
                    if !quiet {
                        println!("  {} Daemon stopped", "✓".green());
                    }
                    Ok(())
                }
                Ok(false) => {
                    eprintln!("{}", "Failed to stop daemon".red());
                    Err(NikaError::InvalidConfig {
                        message: "Failed to stop daemon".to_string(),
                    })
                }
                Err(_) => {
                    eprintln!("{} nika-daemon not found", "✗".red());
                    Err(daemon_not_found_error())
                }
            }
        }

        DaemonAction::Status { json } => {
            // Check if daemon is running by checking socket
            let socket_path = dirs::home_dir()
                .map(|h| h.join(".nika/daemon.sock"))
                .unwrap_or_default();

            let is_running = socket_path.exists();

            if json {
                if is_running {
                    println!(
                        "{{\"status\":\"running\",\"socket\":\"{}\"}}",
                        socket_path.display()
                    );
                } else {
                    println!("{{\"status\":\"stopped\"}}");
                }
            } else {
                println!("{}", "Daemon Status".cyan().bold());
                println!("{}", "─".repeat(60));
                println!();
                if is_running {
                    println!("  {} Daemon is running", "✓".green());
                    println!("  Socket: {}", socket_path.display());
                } else {
                    println!("  {} Daemon is not running", "○".yellow());
                    println!();
                    println!("  Start with: nika daemon start");
                }
            }
            Ok(())
        }

        DaemonAction::Restart => {
            if !quiet {
                println!("{}", "🔄 Restarting nika daemon...".cyan().bold());
            }

            match run_nika_daemon(&["restart"]) {
                Ok(true) => {
                    if !quiet {
                        println!("  {} Daemon restarted", "✓".green());
                    }
                    Ok(())
                }
                Ok(false) => {
                    eprintln!("{}", "Failed to restart daemon".red());
                    Err(NikaError::InvalidConfig {
                        message: "Failed to restart daemon".to_string(),
                    })
                }
                Err(_) => {
                    eprintln!("{} nika-daemon not found", "✗".red());
                    Err(daemon_not_found_error())
                }
            }
        }

        DaemonAction::Install => {
            if !quiet {
                println!(
                    "{}",
                    "📦 Installing daemon as system service...".cyan().bold()
                );
            }

            match run_nika_daemon(&["install"]) {
                Ok(true) => {
                    if !quiet {
                        println!("  {} Daemon service installed", "✓".green());
                        println!();
                        println!("  The daemon will auto-start at login.");
                    }
                    Ok(())
                }
                Ok(false) => {
                    eprintln!("{}", "Failed to install daemon service".red());
                    Err(NikaError::InvalidConfig {
                        message: "Failed to install daemon service".to_string(),
                    })
                }
                Err(_) => {
                    eprintln!("{} nika-daemon not found", "✗".red());
                    Err(daemon_not_found_error())
                }
            }
        }

        DaemonAction::Uninstall => {
            if !quiet {
                println!("{}", "🗑️  Uninstalling daemon service...".cyan().bold());
            }

            match run_nika_daemon(&["uninstall"]) {
                Ok(true) => {
                    if !quiet {
                        println!("  {} Daemon service uninstalled", "✓".green());
                    }
                    Ok(())
                }
                Ok(false) => {
                    eprintln!("{}", "Failed to uninstall daemon service".red());
                    Err(NikaError::InvalidConfig {
                        message: "Failed to uninstall daemon service".to_string(),
                    })
                }
                Err(_) => {
                    eprintln!("{} nika-daemon not found", "✗".red());
                    Err(daemon_not_found_error())
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// BACKUP COMMAND HANDLER (v0.27)
// ═══════════════════════════════════════════════════════════════════════════

/// Handle backup commands by proxying to nika-daemon backup.
async fn handle_backup_command(action: BackupAction, quiet: bool) -> Result<(), NikaError> {
    use colored::Colorize;

    fn run_nika_backup(args: &[&str]) -> Result<bool, std::io::Error> {
        let status = std::process::Command::new("nika-daemon")
            .arg("backup")
            .args(args)
            .status()?;
        Ok(status.success())
    }

    fn daemon_not_found_error() -> NikaError {
        eprintln!();
        eprintln!("nika-daemon not found. Install from:");
        eprintln!("  cargo install nika-daemon");
        NikaError::InvalidConfig {
            message: "nika-daemon not found".to_string(),
        }
    }

    match action {
        BackupAction::Create { label, only } => {
            // Proxy to: nika-daemon backup create [--label LABEL] [--only novanet,nika]
            let mut cmd_args = vec!["create"];

            let label_str;
            if let Some(l) = &label {
                cmd_args.push("--label");
                label_str = l.clone();
                cmd_args.push(&label_str);
            }

            let only_str;
            if let Some(subsystems) = &only {
                cmd_args.push("--only");
                only_str = subsystems.join(",");
                cmd_args.push(&only_str);
            }

            if !quiet {
                println!("{} Creating backup...", "📦".bold());
            }

            match run_nika_backup(&cmd_args) {
                Ok(true) => Ok(()),
                Ok(false) => Err(NikaError::RuntimeError {
                    reason: "Backup creation failed".to_string(),
                }),
                Err(_) => Err(daemon_not_found_error()),
            }
        }

        BackupAction::Restore { backup, force } => {
            // Proxy to: nika-daemon backup restore <backup> [--force]
            let mut cmd_args = vec!["restore", &backup];
            if force {
                cmd_args.push("--force");
            }

            if !quiet {
                println!("{} Restoring from backup...", "📥".bold());
            }

            match run_nika_backup(&cmd_args) {
                Ok(true) => Ok(()),
                Ok(false) => Err(NikaError::RuntimeError {
                    reason: "Backup restore failed".to_string(),
                }),
                Err(_) => Err(daemon_not_found_error()),
            }
        }

        BackupAction::List {
            detailed,
            limit,
            json,
        } => {
            // Proxy to: nika-daemon backup list [--detailed] [-n LIMIT] [--json]
            let mut cmd_args = vec!["list"];

            if detailed {
                cmd_args.push("--detailed");
            }

            let limit_str = limit.to_string();
            cmd_args.push("-n");
            cmd_args.push(&limit_str);

            if json {
                cmd_args.push("--json");
            }

            match run_nika_backup(&cmd_args) {
                Ok(true) => Ok(()),
                Ok(false) => Err(NikaError::RuntimeError {
                    reason: "Failed to list backups".to_string(),
                }),
                Err(_) => Err(daemon_not_found_error()),
            }
        }

        BackupAction::Prune { keep, execute } => {
            // Proxy to: nika-daemon backup prune [--keep N] [--execute]
            let mut cmd_args = vec!["prune"];

            let keep_str = keep.to_string();
            cmd_args.push("--keep");
            cmd_args.push(&keep_str);

            if execute {
                cmd_args.push("--execute");
            }

            if !quiet {
                if execute {
                    println!("{} Pruning old backups...", "🧹".bold());
                } else {
                    println!("{} Dry run - showing what would be pruned...", "🔍".bold());
                }
            }

            match run_nika_backup(&cmd_args) {
                Ok(true) => Ok(()),
                Ok(false) => Err(NikaError::RuntimeError {
                    reason: "Failed to prune backups".to_string(),
                }),
                Err(_) => Err(daemon_not_found_error()),
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// MODEL COMMAND HANDLER (v0.27)
// ═══════════════════════════════════════════════════════════════════════════

/// Find filename for a given quantization string in a known model.
/// Returns None if quant string is invalid or not available for this model.
#[cfg(feature = "native-inference")]
fn find_filename_for_quant(
    model: &nika::core::KnownModel,
    quant_str: &str,
) -> Option<&'static str> {
    use nika::core::Quantization;
    let target = match quant_str.to_uppercase().as_str() {
        "Q4_K_S" => Quantization::Q4_K_S,
        "Q4_K_M" => Quantization::Q4_K_M,
        "Q5_K_S" => Quantization::Q5_K_S,
        "Q5_K_M" => Quantization::Q5_K_M,
        "Q6_K" => Quantization::Q6_K,
        "Q8_0" => Quantization::Q8_0,
        "F16" => Quantization::F16,
        _ => return None,
    };
    model
        .quantizations
        .iter()
        .find(|(q, _)| *q == target)
        .map(|(_, f)| *f)
}

/// Handle model management commands
#[cfg(feature = "native-inference")]
async fn handle_model_command(action: ModelAction, quiet: bool) -> Result<(), NikaError> {
    use colored::Colorize;
    use nika::core::{find_model, KNOWN_MODELS};
    use nika::provider::{default_model_dir, DownloadRequest, HuggingFaceStorage, PullProgress};

    let storage =
        HuggingFaceStorage::new(default_model_dir()).map_err(|e| NikaError::ConfigError {
            reason: format!("Failed to initialize storage: {}", e),
        })?;

    match action {
        ModelAction::List { json } => {
            let models = storage.list_models().map_err(|e| NikaError::ConfigError {
                reason: format!("Failed to list models: {}", e),
            })?;

            if json {
                let output: Vec<serde_json::Value> = models
                    .iter()
                    .map(|m| {
                        serde_json::json!({
                            "name": m.name,
                            "size": m.size,
                            "quantization": m.quantization,
                            "parameters": m.parameters,
                            "digest": m.digest,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else if models.is_empty() {
                if !quiet {
                    println!("{} No models downloaded yet.", "ℹ".cyan());
                    println!(
                        "{}",
                        "Use 'nika model pull <name>' to download a model.".dimmed()
                    );
                    println!();
                    println!("{}", "Available models:".bold());
                    for model in KNOWN_MODELS.iter().take(5) {
                        println!("  • {} - {}", model.id.cyan(), model.description);
                    }
                    if KNOWN_MODELS.len() > 5 {
                        println!(
                            "  {} more available...",
                            (KNOWN_MODELS.len() - 5).to_string().dimmed()
                        );
                    }
                }
            } else {
                println!("{}", "Downloaded Models".bold());
                println!("{}", "─".repeat(70));
                for model in &models {
                    let size_mb = model.size / (1024 * 1024);
                    let quant = model.quantization.as_deref().unwrap_or("?");
                    println!(
                        "  {:30} {:>8} MB  {}",
                        model.name.cyan(),
                        size_mb,
                        quant.dimmed()
                    );
                }
                println!();
                println!(
                    "{} models, {} total",
                    models.len(),
                    format_size(models.iter().map(|m| m.size).sum())
                );
            }
            Ok(())
        }

        ModelAction::Pull {
            name,
            repo,
            file,
            quant,
            force,
        } => {
            // Determine download target - always use passthrough mode (hf_repo + filename)
            let (hf_repo, hf_file) = match (&name, &repo, &file) {
                // Named model from KNOWN_MODELS - extract repo/file
                (Some(model_name), None, None) => {
                    let model =
                        find_model(model_name).ok_or_else(|| NikaError::ValidationError {
                            reason: format!(
                            "Unknown model: '{}'. Use 'nika model list' to see available models.",
                            model_name
                        ),
                        })?;

                    // Determine filename based on quantization or default
                    let filename = if let Some(q) = &quant {
                        find_filename_for_quant(model, q).ok_or_else(|| {
                            NikaError::ValidationError {
                                reason: format!(
                                    "Invalid or unavailable quantization: {}. Available: {:?}",
                                    q,
                                    model
                                        .quantizations
                                        .iter()
                                        .map(|(q, _)| format!("{:?}", q))
                                        .collect::<Vec<_>>()
                                ),
                            }
                        })?
                    } else {
                        model.default_file
                    };

                    (model.hf_repo.to_string(), filename.to_string())
                }
                // Custom HF repo (passthrough)
                (None, Some(hf_repo), Some(hf_file)) => (hf_repo.clone(), hf_file.clone()),
                // Invalid combination
                _ => {
                    return Err(NikaError::ValidationError {
                        reason: "Specify either a model name OR --repo and --file".to_string(),
                    });
                }
            };

            // Check if already exists
            let model_path = default_model_dir().join(&hf_file);
            if model_path.exists() && !force {
                if !quiet {
                    println!(
                        "{} Model already exists: {}",
                        "ℹ".cyan(),
                        model_path.display()
                    );
                    println!("{}", "Use --force to re-download.".dimmed());
                }
                return Ok(());
            }

            if !quiet {
                println!(
                    "{} Downloading {} from {}...",
                    "⬇".cyan(),
                    hf_file.bold(),
                    hf_repo
                );
            }

            // Create download request using passthrough mode
            let request = DownloadRequest {
                model: None, // Don't use model field - types are incompatible
                hf_repo: Some(hf_repo),
                filename: Some(hf_file.clone()),
                quantization: None, // Quantization already resolved to filename
                force,
            };

            // Download with simple progress output
            let quiet_clone = quiet;
            storage
                .download(
                    &request,
                    Box::new(move |progress: PullProgress| {
                        if !quiet_clone && progress.total > 0 {
                            let pct = (progress.completed * 100) / progress.total;
                            eprint!(
                                "\r  Progress: {}% ({} / {} MB)  ",
                                pct,
                                progress.completed / (1024 * 1024),
                                progress.total / (1024 * 1024)
                            );
                        }
                    }),
                )
                .await
                .map_err(|e| NikaError::ConfigError {
                    reason: format!("Download failed: {}", e),
                })?;

            if !quiet {
                eprintln!(); // New line after progress
            }

            if !quiet {
                println!("{} Model downloaded: {}", "✓".green(), model_path.display());
            }
            Ok(())
        }

        ModelAction::Info { name } => {
            // Try KNOWN_MODELS first
            if let Some(model) = find_model(&name) {
                println!("{}", format!("Model: {}", model.name).bold());
                println!("{}", "─".repeat(50));
                println!("  ID:           {}", model.id);
                println!("  Description:  {}", model.description);
                println!("  Repository:   {}", model.hf_repo.cyan());
                println!("  Parameters:   {}B", model.param_billions);
                println!("  Min RAM:      {} GB", model.min_ram_gb);
                println!("  Default File: {}", model.default_file);
                println!("  Quantizations:");
                for (quant, filename) in model.quantizations {
                    let path = default_model_dir().join(filename);
                    let status = if path.exists() {
                        "✓ downloaded".green().to_string()
                    } else {
                        "not downloaded".dimmed().to_string()
                    };
                    println!("    • {:?}: {} ({})", quant, filename, status);
                }
            } else {
                // Try as local file path
                let path = if name.contains('/') || name.contains('.') {
                    std::path::PathBuf::from(&name)
                } else {
                    default_model_dir().join(&name)
                };

                if path.exists() {
                    let metadata = std::fs::metadata(&path)?;
                    let size_mb = metadata.len() / (1024 * 1024);
                    let quant = nika::provider::native::extract_quantization(
                        path.file_name().unwrap().to_str().unwrap(),
                    );

                    println!("{}", format!("Model: {}", path.display()).bold());
                    println!("{}", "─".repeat(50));
                    println!("  Size:         {} MB", size_mb);
                    println!(
                        "  Quantization: {}",
                        quant.unwrap_or_else(|| "Unknown".to_string())
                    );
                } else {
                    return Err(NikaError::ValidationError {
                        reason: format!("Model not found: {}", name),
                    });
                }
            }
            Ok(())
        }

        ModelAction::Status => {
            // For now, just show if NativeRuntime would be available
            // In the future, this could show loaded models in a daemon
            println!("{}", "Model Status".bold());
            println!("{}", "─".repeat(50));

            let models = storage.list_models().map_err(|e| NikaError::ConfigError {
                reason: format!("Failed to list models: {}", e),
            })?;

            if models.is_empty() {
                println!("{} No models available for inference.", "ℹ".cyan());
            } else {
                println!(
                    "{} {} models available for inference:",
                    "✓".green(),
                    models.len()
                );
                for model in models.iter().take(5) {
                    println!("  • {}", model.name.cyan());
                }
                if models.len() > 5 {
                    println!("  ... and {} more", models.len() - 5);
                }
            }
            println!();
            println!(
                "{}",
                "Use 'provider: native' in workflows for local inference.".dimmed()
            );
            Ok(())
        }

        ModelAction::Delete { name, force } => {
            // Find the model path
            let path = if name.contains('/') || name.contains('.') {
                std::path::PathBuf::from(&name)
            } else {
                default_model_dir().join(&name)
            };

            if !path.exists() {
                return Err(NikaError::ValidationError {
                    reason: format!("Model not found: {}", path.display()),
                });
            }

            // Confirm deletion
            if !force && !quiet {
                println!("{} Delete model: {}?", "⚠".yellow(), path.display());
                print!("  Type 'yes' to confirm: ");
                std::io::Write::flush(&mut std::io::stdout()).unwrap();

                let mut input = String::new();
                std::io::stdin().read_line(&mut input).unwrap();
                if input.trim() != "yes" {
                    println!("{}", "Cancelled.".dimmed());
                    return Ok(());
                }
            }

            std::fs::remove_file(&path)?;

            if !quiet {
                println!("{} Model deleted: {}", "✓".green(), path.display());
            }
            Ok(())
        }
    }
}

/// Format bytes as human-readable size
#[cfg(feature = "native-inference")]
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CONFIG COMMAND HANDLER (v0.13.1)
// ═══════════════════════════════════════════════════════════════════════════

fn handle_config_command(action: ConfigAction, quiet: bool) -> Result<(), NikaError> {
    // Find .nika directory
    let nika_dir = find_nika_dir()?;
    let config_path = nika_dir.join("config.toml");

    match action {
        ConfigAction::Path => {
            println!("{}", config_path.display());
            Ok(())
        }

        ConfigAction::List { json } => {
            if !config_path.exists() {
                if json {
                    println!("{{}}");
                } else {
                    println!(
                        "{} No config file found at {}",
                        "ℹ".cyan(),
                        config_path.display()
                    );
                    println!("  Run 'nika init' to create one.");
                }
                return Ok(());
            }

            let content = fs::read_to_string(&config_path)?;

            if json {
                // Parse TOML and convert to JSON
                let value: toml::Value =
                    toml::from_str(&content).map_err(|e| NikaError::ValidationError {
                        reason: format!("Invalid TOML: {}", e),
                    })?;
                let json = serde_json::to_string_pretty(&value).map_err(|e| {
                    NikaError::ValidationError {
                        reason: format!("JSON conversion failed: {}", e),
                    }
                })?;
                println!("{}", json);
            } else {
                println!("{}", "Nika Configuration".bold());
                println!("{}", "─".repeat(40));
                println!();
                println!("{}", content);
            }
            Ok(())
        }

        ConfigAction::Get { key } => {
            if !config_path.exists() {
                return Err(NikaError::ValidationError {
                    reason: "No config file found. Run 'nika init' first.".to_string(),
                });
            }

            let content = fs::read_to_string(&config_path)?;
            let value: toml::Value =
                toml::from_str(&content).map_err(|e| NikaError::ValidationError {
                    reason: format!("Invalid TOML: {}", e),
                })?;

            // Navigate to the key (dot-separated path)
            let mut current = &value;
            for part in key.split('.') {
                current = current
                    .get(part)
                    .ok_or_else(|| NikaError::ValidationError {
                        reason: format!("Key '{}' not found", key),
                    })?;
            }

            // Print the value
            match current {
                toml::Value::String(s) => println!("{}", s),
                toml::Value::Integer(i) => println!("{}", i),
                toml::Value::Float(f) => println!("{}", f),
                toml::Value::Boolean(b) => println!("{}", b),
                _ => println!("{}", current),
            }
            Ok(())
        }

        ConfigAction::Set { key, value } => {
            if !config_path.exists() {
                return Err(NikaError::ValidationError {
                    reason: "No config file found. Run 'nika init' first.".to_string(),
                });
            }

            let content = fs::read_to_string(&config_path)?;
            let mut doc =
                content
                    .parse::<toml::Table>()
                    .map_err(|e| NikaError::ValidationError {
                        reason: format!("Invalid TOML: {}", e),
                    })?;

            // Navigate and set the value
            let parts: Vec<&str> = key.split('.').collect();
            if parts.is_empty() {
                return Err(NikaError::ValidationError {
                    reason: "Empty key".to_string(),
                });
            }

            // Build nested structure
            let mut current = &mut doc;
            for (i, part) in parts.iter().enumerate() {
                if i == parts.len() - 1 {
                    // Last part - set the value
                    let toml_value = parse_config_value(&value);
                    current.insert((*part).to_string(), toml_value);
                } else {
                    // Navigate or create table
                    if !current.contains_key(*part) {
                        current.insert((*part).to_string(), toml::Value::Table(toml::Table::new()));
                    }
                    current = current
                        .get_mut(*part)
                        .unwrap()
                        .as_table_mut()
                        .ok_or_else(|| NikaError::ValidationError {
                            reason: format!("'{}' is not a table", part),
                        })?;
                }
            }

            // Write back
            let new_content =
                toml::to_string_pretty(&doc).map_err(|e| NikaError::ValidationError {
                    reason: format!("TOML serialization failed: {}", e),
                })?;
            fs::write(&config_path, new_content)?;

            if !quiet {
                println!("{} {} = {}", "✓".green(), key, value);
            }
            Ok(())
        }

        ConfigAction::Edit => {
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

            if !config_path.exists() {
                return Err(NikaError::ValidationError {
                    reason: format!(
                        "No config file found at {}. Run 'nika init' first.",
                        config_path.display()
                    ),
                });
            }

            let status = std::process::Command::new(&editor)
                .arg(&config_path)
                .status()
                .map_err(|e| NikaError::ValidationError {
                    reason: format!("Failed to launch editor '{}': {}", editor, e),
                })?;

            if !status.success() {
                return Err(NikaError::ValidationError {
                    reason: format!("Editor '{}' exited with code {:?}", editor, status.code()),
                });
            }
            Ok(())
        }

        ConfigAction::Reset { force } => {
            if !force {
                println!(
                    "{} This will reset config to defaults. Use --force to confirm.",
                    "⚠".yellow()
                );
                return Ok(());
            }

            if config_path.exists() {
                fs::remove_file(&config_path)?;
            }

            // Create default config
            let default_config = include_str!("../templates/config.toml");
            fs::write(&config_path, default_config)?;

            if !quiet {
                println!("{} Config reset to defaults", "✓".green());
            }
            Ok(())
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SCHEMA COMMAND HANDLER (v0.21.0)
// ═══════════════════════════════════════════════════════════════════════════

/// Handle schema subcommands: version, validate, upgrade
fn handle_schema_command(action: SchemaAction, quiet: bool) -> Result<(), NikaError> {
    match action {
        SchemaAction::Version { path } => {
            match path {
                Some(p) => {
                    // Show version for a specific file
                    let path = std::path::Path::new(&p);
                    if !path.exists() {
                        return Err(NikaError::WorkflowNotFound {
                            path: path.to_string_lossy().to_string(),
                        });
                    }

                    let content = std::fs::read_to_string(path)?;
                    let schema = extract_schema_version(&content);

                    if quiet {
                        println!("{}", schema);
                    } else {
                        println!("{} {}", "Schema version:".cyan(), schema.green());

                        // Parse and show details if it's a valid nika schema
                        if let Some(version) = schema.strip_prefix("nika/workflow@") {
                            let latest = "0.10";
                            if version == latest {
                                println!("  {} You're on the latest schema version", "✓".green());
                            } else {
                                println!("  {} Latest version is @{}", "ℹ".yellow(), latest);
                                println!(
                                    "  {} Run 'nika schema upgrade {}' to upgrade",
                                    "→".cyan(),
                                    path.display()
                                );
                            }
                        }
                    }
                }
                None => {
                    // List available schema versions
                    if !quiet {
                        println!("{}", "Available Nika schema versions:".cyan().bold());
                        println!();
                    }

                    let versions = [
                        ("0.1", "Basic: infer, exec, fetch verbs"),
                        ("0.2", "+invoke, +agent verbs, +mcp config"),
                        ("0.3", "+for_each parallelism, rig-core integration"),
                        ("0.5", "+decompose, +lazy bindings, +spawn_agent"),
                        ("0.6", "+multi-provider support (6 providers)"),
                        ("0.7", "+full streaming for all providers"),
                        ("0.8", "+Studio DX (edit history, sessions, themes)"),
                        ("0.9", "+context: file loading, +include: DAG fusion"),
                        ("0.10", "+two-phase AST, +analyzer validation (current)"),
                    ];

                    for (version, desc) in &versions {
                        if quiet {
                            println!("nika/workflow@{}", version);
                        } else {
                            let prefix = if *version == "0.10" {
                                "→".green()
                            } else {
                                " ".normal()
                            };
                            println!("  {} nika/workflow@{:<4}  {}", prefix, version.cyan(), desc);
                        }
                    }

                    if !quiet {
                        println!();
                        println!(
                            "  {} Use 'nika schema version <file>' to check a workflow",
                            "ℹ".yellow()
                        );
                    }
                }
            }
            Ok(())
        }

        SchemaAction::Validate { path, schema } => {
            let path = std::path::Path::new(&path);
            if !path.exists() {
                return Err(NikaError::WorkflowNotFound {
                    path: path.to_string_lossy().to_string(),
                });
            }

            // Helper function to validate a single file
            fn validate_single_file(
                file_path: &std::path::Path,
                target_schema: Option<&str>,
                quiet: bool,
            ) -> Result<(), NikaError> {
                let content = std::fs::read_to_string(file_path)?;

                // Get schema version to validate against
                let schema_version = target_schema.map(|s| s.to_string()).unwrap_or_else(|| {
                    let extracted = extract_schema_version(&content);
                    if extracted == "(not specified)" {
                        "nika/workflow@0.10".to_string()
                    } else {
                        extracted
                    }
                });

                if !quiet {
                    println!(
                        "{} Validating {} against {}",
                        "→".cyan(),
                        file_path.display(),
                        schema_version.green()
                    );
                }

                // Try to parse the workflow to validate it
                let result = nika::ast::raw::parse(&content, nika::source::FileId(0));
                match result {
                    Ok(raw_workflow) => {
                        // Run analyzer for semantic validation
                        let analyze_result = nika::ast::analyzer::analyze(raw_workflow);
                        if analyze_result.is_ok() {
                            if !quiet {
                                println!("{} Workflow is valid", "✓".green());
                            }
                            Ok(())
                        } else {
                            let errors: Vec<String> = analyze_result
                                .errors
                                .iter()
                                .map(|e| format!("  • {}", e))
                                .collect();
                            if !quiet {
                                println!("{} Validation failed:", "✗".red());
                                for error in &errors {
                                    println!("{}", error.red());
                                }
                            }
                            Err(NikaError::ValidationError {
                                reason: format!(
                                    "{} validation error(s) found",
                                    analyze_result.errors.len()
                                ),
                            })
                        }
                    }
                    Err(e) => {
                        if !quiet {
                            println!("{} Parse error: {}", "✗".red(), e);
                        }
                        Err(NikaError::ValidationError {
                            reason: format!("Parse error: {}", e),
                        })
                    }
                }
            }

            // Check if path is a directory
            if path.is_dir() {
                let files = find_nika_yaml_files(path)?;

                if files.is_empty() {
                    if !quiet {
                        println!(
                            "{} No .nika.yaml files found in {}",
                            "⚠".yellow(),
                            path.display()
                        );
                    }
                    return Ok(());
                }

                if !quiet {
                    println!(
                        "{} Found {} workflow file(s) in {}",
                        "→".cyan(),
                        files.len(),
                        path.display()
                    );
                    println!();
                }

                let mut passed = 0;
                let mut failed = 0;

                for file in &files {
                    match validate_single_file(file, schema.as_deref(), quiet) {
                        Ok(()) => passed += 1,
                        Err(_) => failed += 1,
                    }
                    if !quiet {
                        println!();
                    }
                }

                if !quiet {
                    println!("────────────────────────────────────────");
                    println!(
                        "{} Summary: {} passed, {} failed",
                        "→".cyan(),
                        passed.to_string().green(),
                        if failed > 0 {
                            failed.to_string().red()
                        } else {
                            failed.to_string().green()
                        }
                    );
                }

                if failed > 0 {
                    Err(NikaError::ValidationError {
                        reason: format!("{} file(s) failed validation", failed),
                    })
                } else {
                    Ok(())
                }
            } else {
                // Single file validation
                validate_single_file(path, schema.as_deref(), quiet)
            }
        }

        SchemaAction::Upgrade {
            path,
            target,
            all,
            backup,
        } => {
            let target_version = target.as_deref().unwrap_or("0.10");

            if all {
                // Upgrade all .nika.yaml files in directory
                let dir = std::path::Path::new(&path);
                if !dir.is_dir() {
                    return Err(NikaError::ValidationError {
                        reason: format!("{} is not a directory", path),
                    });
                }

                // Recursively find all .nika.yaml files
                let files = find_nika_yaml_files(dir)?;

                if files.is_empty() {
                    if !quiet {
                        println!("{} No .nika.yaml files found in {}", "ℹ".yellow(), path);
                    }
                    return Ok(());
                }

                if !quiet {
                    println!(
                        "{} Upgrading {} workflow(s) to schema @{}",
                        "→".cyan(),
                        files.len(),
                        target_version
                    );
                }

                let mut upgraded = 0;
                let mut skipped = 0;
                let mut errors = 0;

                for file in files {
                    match upgrade_workflow_file(&file, target_version, backup) {
                        Ok(true) => upgraded += 1,
                        Ok(false) => skipped += 1,
                        Err(e) => {
                            if !quiet {
                                println!("  {} {}: {}", "✗".red(), file.display(), e);
                            }
                            errors += 1;
                        }
                    }
                }

                if !quiet {
                    println!();
                    println!(
                        "{} {} upgraded, {} skipped, {} errors",
                        "Summary:".cyan(),
                        upgraded.to_string().green(),
                        skipped,
                        errors
                    );
                }

                if errors > 0 {
                    Err(NikaError::ValidationError {
                        reason: format!("{} file(s) failed to upgrade", errors),
                    })
                } else {
                    Ok(())
                }
            } else {
                // Upgrade single file
                let file_path = std::path::Path::new(&path);
                if !file_path.exists() {
                    return Err(NikaError::WorkflowNotFound { path: path.clone() });
                }

                match upgrade_workflow_file(file_path, target_version, backup)? {
                    true => {
                        if !quiet {
                            println!(
                                "{} Upgraded {} to schema @{}",
                                "✓".green(),
                                path,
                                target_version
                            );
                        }
                        Ok(())
                    }
                    false => {
                        if !quiet {
                            println!(
                                "{} {} is already at schema @{} or newer",
                                "ℹ".yellow(),
                                path,
                                target_version
                            );
                        }
                        Ok(())
                    }
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// WORKFLOW COMMAND HANDLER (v0.22+)
// ═══════════════════════════════════════════════════════════════════════════

/// Handle workflow subcommands: edit, add-task, graph, check
async fn handle_workflow_command(action: WorkflowAction, quiet: bool) -> Result<(), NikaError> {
    match action {
        WorkflowAction::Edit { file } => {
            // Open workflow in Studio editor
            #[cfg(feature = "tui")]
            {
                if !quiet {
                    println!(
                        "{} Opening {} in Studio editor...",
                        "→".cyan(),
                        file.display()
                    );
                }
                nika::tui::run_tui_studio(Some(file)).await
            }
            #[cfg(not(feature = "tui"))]
            {
                let _ = (file, quiet); // Suppress unused warnings
                Err(NikaError::ConfigError {
                    reason: "TUI feature not enabled. Rebuild with `--features tui`".to_string(),
                })
            }
        }

        WorkflowAction::AddTask {
            file,
            id,
            verb,
            after,
        } => {
            // Validate file exists
            if !file.exists() {
                return Err(NikaError::WorkflowNotFound {
                    path: file.to_string_lossy().to_string(),
                });
            }

            // Read existing workflow
            let content = std::fs::read_to_string(&file)?;

            // Generate task ID if not provided
            let task_id = id.unwrap_or_else(|| {
                format!("task_{}", chrono::Utc::now().timestamp_millis() % 10000)
            });

            // Default verb is infer
            let task_verb = verb.unwrap_or_else(|| "infer".to_string());

            // Build the new task YAML
            let new_task = match task_verb.as_str() {
                "infer" => format!(
                    r#"  - id: {}
    infer: "TODO: Add your prompt here"
"#,
                    task_id
                ),
                "exec" => format!(
                    r#"  - id: {}
    exec: "echo 'TODO: Add your command here'"
"#,
                    task_id
                ),
                "fetch" => format!(
                    r#"  - id: {}
    fetch:
      url: "https://example.com/api"
      method: GET
"#,
                    task_id
                ),
                "invoke" => format!(
                    r#"  - id: {}
    invoke:
      mcp: novanet
      tool: novanet_generate
      params: {{}}
"#,
                    task_id
                ),
                "agent" => format!(
                    r#"  - id: {}
    agent:
      prompt: "TODO: Add your agent prompt here"
      max_turns: 5
"#,
                    task_id
                ),
                _ => {
                    return Err(NikaError::ValidationError {
                        reason: format!(
                            "Unknown verb '{}'. Valid: infer, exec, fetch, invoke, agent",
                            task_verb
                        ),
                    });
                }
            };

            // Find insertion point
            let mut lines: Vec<&str> = content.lines().collect();
            let mut insert_index = None;

            // Find tasks: section and optionally the task to insert after
            let mut in_tasks = false;
            let mut after_task_end = None;

            for (i, line) in lines.iter().enumerate() {
                if line.trim() == "tasks:" {
                    in_tasks = true;
                    continue;
                }
                if in_tasks {
                    // Check if this is a task start (- id:)
                    if line.trim().starts_with("- id:") {
                        // Check if this is the task we want to insert after
                        if let Some(ref after_id) = after {
                            if line.contains(after_id) {
                                // Mark that we found the after task
                                after_task_end = Some(i);
                            } else if after_task_end.is_some() {
                                // We've found the next task after our target
                                insert_index = Some(i);
                                break;
                            }
                        }
                    }
                    // Check for flows: or other top-level sections
                    if !line.starts_with(' ') && !line.starts_with('-') && line.contains(':') {
                        insert_index = Some(i);
                        break;
                    }
                }
            }

            // If no insert point found, append at end of tasks
            let insert_at = insert_index.unwrap_or(lines.len());

            // Insert the new task
            let new_task_lines: Vec<&str> = new_task.lines().collect();
            for (j, task_line) in new_task_lines.iter().enumerate() {
                lines.insert(insert_at + j, task_line);
            }

            // Write back
            let new_content = lines.join("\n");
            std::fs::write(&file, new_content)?;

            if !quiet {
                println!(
                    "{} Added task '{}' ({}) to {}",
                    "✓".green(),
                    task_id.cyan(),
                    task_verb.yellow(),
                    file.display()
                );
                if let Some(after_id) = after {
                    println!("  {} Inserted after task '{}'", "→".cyan(), after_id);
                }
            }

            Ok(())
        }

        WorkflowAction::Graph {
            file,
            format,
            output,
        } => {
            // Validate file exists
            if !file.exists() {
                return Err(NikaError::WorkflowNotFound {
                    path: file.to_string_lossy().to_string(),
                });
            }

            // Parse workflow
            let content = std::fs::read_to_string(&file)?;
            let workflow: nika::ast::Workflow = serde_yaml::from_str(&content)?;

            // Generate graph based on format
            let graph_output = match format.as_str() {
                "ascii" => generate_ascii_dag(&workflow),
                "dot" => generate_dot_dag(&workflow),
                "mermaid" => generate_mermaid_dag(&workflow),
                _ => {
                    return Err(NikaError::ValidationError {
                        reason: format!("Unknown format '{}'. Valid: ascii, dot, mermaid", format),
                    });
                }
            };

            // Output
            match output {
                Some(path) => {
                    std::fs::write(&path, &graph_output)?;
                    if !quiet {
                        println!(
                            "{} DAG written to {} (format: {})",
                            "✓".green(),
                            path.display(),
                            format.cyan()
                        );
                    }
                }
                None => {
                    println!("{}", graph_output);
                }
            }

            Ok(())
        }

        WorkflowAction::Check {
            file,
            suggest,
            format,
        } => {
            // Validate file exists
            if !file.exists() {
                return Err(NikaError::WorkflowNotFound {
                    path: file.to_string_lossy().to_string(),
                });
            }

            // Parse and validate
            let content = std::fs::read_to_string(&file)?;
            let workflow: nika::ast::Workflow = serde_yaml::from_str(&content)?;

            // Collect validation results
            let mut issues: Vec<(String, String, String)> = Vec::new(); // (level, code, message)
            let mut suggestions: Vec<String> = Vec::new();

            // Check schema version
            let schema = workflow.schema.clone();
            if !schema.starts_with("nika/workflow@") {
                issues.push((
                    "error".to_string(),
                    "NIKA-001".to_string(),
                    "Missing or invalid schema version".to_string(),
                ));
            } else if let Some(version) = schema.strip_prefix("nika/workflow@") {
                if version != "0.10" && suggest {
                    suggestions.push(format!(
                        "Consider upgrading from @{} to @0.10 for latest features",
                        version
                    ));
                }
            }

            // Check for common issues
            if workflow.tasks.is_empty() {
                issues.push((
                    "error".to_string(),
                    "NIKA-010".to_string(),
                    "Workflow has no tasks".to_string(),
                ));
            }

            // Check for duplicate task IDs
            let mut seen_ids = std::collections::HashSet::new();
            for task in &workflow.tasks {
                if !seen_ids.insert(&task.id) {
                    issues.push((
                        "error".to_string(),
                        "NIKA-141".to_string(),
                        format!("Duplicate task ID: '{}'", task.id),
                    ));
                }
            }

            // Check for unused tasks (not referenced in flows or use blocks)
            if suggest {
                let mut referenced: std::collections::HashSet<&str> =
                    std::collections::HashSet::new();
                for flow in &workflow.flows {
                    for target in flow.target.as_vec() {
                        referenced.insert(target);
                    }
                    for source in flow.source.as_vec() {
                        referenced.insert(source);
                    }
                }
                for task in &workflow.tasks {
                    if let Some(ref with_spec) = task.with_spec {
                        for entry in with_spec.values() {
                            if let Some(task_ref) = entry.task_id() {
                                referenced.insert(task_ref);
                            }
                        }
                    } else if let Some(ref use_block) = task.use_wiring {
                        for entry in use_block.values() {
                            if let Some(task_ref) = entry.path.split('.').next() {
                                referenced.insert(task_ref);
                            }
                        }
                    }
                }
                for task in &workflow.tasks {
                    if !referenced.contains(task.id.as_str()) && workflow.tasks.len() > 1 {
                        // First task or leaf tasks are often not referenced
                        if workflow.tasks.first().map(|t| &t.id) != Some(&task.id) {
                            suggestions.push(format!(
                                "Task '{}' is not referenced by any other task",
                                task.id
                            ));
                        }
                    }
                }
            }

            // Output results
            match format.as_str() {
                "json" => {
                    let result = serde_json::json!({
                        "file": file.to_string_lossy(),
                        "valid": issues.iter().all(|(level, _, _)| level != "error"),
                        "issues": issues.iter().map(|(level, code, msg)| {
                            serde_json::json!({
                                "level": level,
                                "code": code,
                                "message": msg
                            })
                        }).collect::<Vec<_>>(),
                        "suggestions": suggestions
                    });
                    println!("{}", serde_json::to_string_pretty(&result).unwrap());
                }
                _ => {
                    // Text format
                    let has_errors = issues.iter().any(|(level, _, _)| level == "error");

                    if issues.is_empty() {
                        if !quiet {
                            println!("{} {} is valid", "✓".green(), file.display());
                        }
                    } else {
                        for (level, code, msg) in &issues {
                            let prefix = if level == "error" {
                                "✗".red()
                            } else {
                                "⚠".yellow()
                            };
                            println!("{} [{}] {}", prefix, code.cyan(), msg);
                        }
                    }

                    if suggest && !suggestions.is_empty() {
                        println!();
                        println!("{}", "Suggestions:".cyan().bold());
                        for suggestion in &suggestions {
                            println!("  {} {}", "→".cyan(), suggestion);
                        }
                    }

                    if has_errors {
                        return Err(NikaError::ValidationError {
                            reason: format!("{} validation error(s) found", issues.len()),
                        });
                    }
                }
            }

            Ok(())
        }
    }
}

/// Generate ASCII DAG representation
fn generate_ascii_dag(workflow: &nika::ast::Workflow) -> String {
    let mut output = String::new();
    let name = "(unnamed)";
    output.push_str("┌─────────────────────────────────────────┐\n");
    output.push_str(&format!("│ DAG: {}", name));
    let padding = 40usize.saturating_sub(name.len() + 6);
    output.push_str(&" ".repeat(padding));
    output.push_str("│\n");
    output.push_str("├─────────────────────────────────────────┤\n");

    // Build task list with verb icons
    for task in &workflow.tasks {
        let verb_icon = match &task.action {
            nika::ast::TaskAction::Infer { .. } => "⚡",
            nika::ast::TaskAction::Exec { .. } => "📟",
            nika::ast::TaskAction::Fetch { .. } => "🛰️",
            nika::ast::TaskAction::Invoke { .. } => "🔌",
            nika::ast::TaskAction::Agent { .. } => "🐔",
        };
        let line = format!("│ {} {}", verb_icon, task.id);
        let line_padding = 40usize.saturating_sub(task.id.len() + 4);
        output.push_str(&format!("{}{}│\n", line, " ".repeat(line_padding)));
    }

    // Show flows
    if !workflow.flows.is_empty() {
        output.push_str("├─────────────────────────────────────────┤\n");
        output.push_str("│ Flows:                                  │\n");
        for flow in &workflow.flows {
            let sources = flow.source.as_vec().join(", ");
            let targets = flow.target.as_vec().join(", ");
            let flow_str = format!("  {} → {}", sources, targets);
            let flow_padding = 39usize.saturating_sub(flow_str.len());
            output.push_str(&format!("│{}{}│\n", flow_str, " ".repeat(flow_padding)));
        }
    }

    output.push_str("└─────────────────────────────────────────┘\n");
    output
}

/// Generate DOT (Graphviz) DAG representation
fn generate_dot_dag(workflow: &nika::ast::Workflow) -> String {
    let mut output = String::new();
    let name = "workflow";
    output.push_str(&format!("digraph {} {{\n", name));
    output.push_str("  rankdir=LR;\n");
    output.push_str("  node [shape=box, style=rounded];\n\n");

    // Add nodes with styling based on verb
    for task in &workflow.tasks {
        let color = match &task.action {
            nika::ast::TaskAction::Infer { .. } => "lightblue",
            nika::ast::TaskAction::Exec { .. } => "lightgreen",
            nika::ast::TaskAction::Fetch { .. } => "lightyellow",
            nika::ast::TaskAction::Invoke { .. } => "lightpink",
            nika::ast::TaskAction::Agent { .. } => "plum",
        };
        output.push_str(&format!(
            "  {} [label=\"{}\", fillcolor={}, style=\"rounded,filled\"];\n",
            task.id.replace('-', "_"),
            task.id,
            color
        ));
    }

    // Add edges
    output.push('\n');
    for flow in &workflow.flows {
        for source in flow.source.as_vec() {
            for target in flow.target.as_vec() {
                output.push_str(&format!(
                    "  {} -> {};\n",
                    source.replace('-', "_"),
                    target.replace('-', "_")
                ));
            }
        }
    }

    output.push_str("}\n");
    output
}

/// Generate Mermaid DAG representation
fn generate_mermaid_dag(workflow: &nika::ast::Workflow) -> String {
    let mut output = String::new();
    output.push_str("```mermaid\ngraph LR\n");

    // Add nodes with styling
    for task in &workflow.tasks {
        let shape = match &task.action {
            nika::ast::TaskAction::Infer { .. } => ("([", "])"), // Stadium
            nika::ast::TaskAction::Exec { .. } => ("[", "]"),    // Rectangle
            nika::ast::TaskAction::Fetch { .. } => ("{{", "}}"), // Hexagon
            nika::ast::TaskAction::Invoke { .. } => ("[[", "]]"), // Subroutine
            nika::ast::TaskAction::Agent { .. } => ("((", "))"), // Circle
        };
        let verb = match &task.action {
            nika::ast::TaskAction::Infer { .. } => "infer",
            nika::ast::TaskAction::Exec { .. } => "exec",
            nika::ast::TaskAction::Fetch { .. } => "fetch",
            nika::ast::TaskAction::Invoke { .. } => "invoke",
            nika::ast::TaskAction::Agent { .. } => "agent",
        };
        output.push_str(&format!(
            "  {}{}{} : {}{}\n",
            task.id.replace('-', "_"),
            shape.0,
            task.id,
            verb,
            shape.1
        ));
    }

    // Add edges
    output.push('\n');
    for flow in &workflow.flows {
        for source in flow.source.as_vec() {
            for target in flow.target.as_vec() {
                output.push_str(&format!(
                    "  {} --> {}\n",
                    source.replace('-', "_"),
                    target.replace('-', "_")
                ));
            }
        }
    }

    output.push_str("```\n");
    output
}

/// Extract schema version from YAML content using regex
fn extract_schema_version(content: &str) -> String {
    // Match schema: "nika/workflow@X.Y" or schema: 'nika/workflow@X.Y' or schema: nika/workflow@X.Y
    let re = regex::Regex::new(r#"^schema:\s*["']?(nika/workflow@[0-9.]+)["']?"#).ok();
    if let Some(regex) = re {
        for line in content.lines() {
            if let Some(captures) = regex.captures(line.trim()) {
                if let Some(m) = captures.get(1) {
                    return m.as_str().to_string();
                }
            }
        }
    }
    "(not specified)".to_string()
}

/// Find all .nika.yaml files recursively in a directory
fn find_nika_yaml_files(dir: &std::path::Path) -> Result<Vec<std::path::PathBuf>, NikaError> {
    let mut files = Vec::new();

    fn visit_dir(
        dir: &std::path::Path,
        files: &mut Vec<std::path::PathBuf>,
    ) -> Result<(), NikaError> {
        if dir.is_dir() {
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    // Skip hidden directories and common ignore patterns
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if !name.starts_with('.') && name != "node_modules" && name != "target" {
                        visit_dir(&path, files)?;
                    }
                } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with(".nika.yaml") || name.ends_with(".nika.yml") {
                        files.push(path);
                    }
                }
            }
        }
        Ok(())
    }

    visit_dir(dir, &mut files)?;
    files.sort();
    Ok(files)
}

/// Upgrade a single workflow file to target schema version
/// Returns Ok(true) if upgraded, Ok(false) if skipped (already at version)
fn upgrade_workflow_file(
    path: &std::path::Path,
    target_version: &str,
    backup: bool,
) -> Result<bool, NikaError> {
    let content = std::fs::read_to_string(path)?;

    // Get current schema version using regex helper
    let current_schema = extract_schema_version(&content);
    let current = if current_schema == "(not specified)" {
        "0.1".to_string()
    } else {
        // Extract version number from "nika/workflow@X.Y"
        current_schema
            .strip_prefix("nika/workflow@")
            .unwrap_or("0.1")
            .to_string()
    };

    // Parse versions for comparison
    let current_parts: Vec<u32> = current
        .split('.')
        .filter_map(|s: &str| s.parse::<u32>().ok())
        .collect();
    let target_parts: Vec<u32> = target_version
        .split('.')
        .filter_map(|s: &str| s.parse::<u32>().ok())
        .collect();

    // Compare versions (simple comparison)
    let current_num = current_parts.first().copied().unwrap_or(0) * 100
        + current_parts.get(1).copied().unwrap_or(0);
    let target_num = target_parts.first().copied().unwrap_or(0) * 100
        + target_parts.get(1).copied().unwrap_or(0);

    if current_num >= target_num {
        return Ok(false); // Already at or above target version
    }

    // Create backup if requested
    if backup {
        let backup_path = path.with_extension("nika.yaml.bak");
        std::fs::copy(path, &backup_path)?;
    }

    // Update schema version using regex replacement (preserves formatting)
    let schema_regex =
        regex::Regex::new(r#"schema:\s*["']?nika/workflow@[\d.]+["']?"#).map_err(|e| {
            NikaError::ParseError {
                details: format!("Regex error: {}", e),
            }
        })?;

    let new_schema_line = format!(r#"schema: "nika/workflow@{}""#, target_version);
    let updated = schema_regex
        .replace(&content, new_schema_line.as_str())
        .to_string();

    // If no replacement was made, the file might not have a schema line - add one
    let updated = if updated == content {
        // Prepend schema line at the beginning
        format!("{}\n{}", new_schema_line, content)
    } else {
        updated
    };

    std::fs::write(path, updated)?;

    Ok(true)
}

/// Parse a string value into appropriate TOML type
fn parse_config_value(value: &str) -> toml::Value {
    // Try boolean
    if value == "true" {
        return toml::Value::Boolean(true);
    }
    if value == "false" {
        return toml::Value::Boolean(false);
    }

    // Try integer
    if let Ok(i) = value.parse::<i64>() {
        return toml::Value::Integer(i);
    }

    // Try float
    if let Ok(f) = value.parse::<f64>() {
        return toml::Value::Float(f);
    }

    // Default to string
    toml::Value::String(value.to_string())
}

/// Find .nika directory (search current and parents)
fn find_nika_dir() -> Result<PathBuf, NikaError> {
    let current = std::env::current_dir()?;

    let mut dir = current.as_path();
    loop {
        let nika_dir = dir.join(".nika");
        if nika_dir.exists() && nika_dir.is_dir() {
            return Ok(nika_dir);
        }

        match dir.parent() {
            Some(parent) => dir = parent,
            None => break,
        }
    }

    // Default to current directory
    Ok(current.join(".nika"))
}

// ═══════════════════════════════════════════════════════════════════════════
// DOCTOR COMMAND HANDLER (v0.13.1)
// ═══════════════════════════════════════════════════════════════════════════

/// Diagnostic check result
#[derive(Debug, Clone)]
struct DiagnosticCheck {
    name: &'static str,
    status: DiagnosticStatus,
    message: String,
    suggestion: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum DiagnosticStatus {
    Pass,
    Warn,
    Fail,
}

impl DiagnosticCheck {
    fn pass(name: &'static str, message: impl Into<String>) -> Self {
        Self {
            name,
            status: DiagnosticStatus::Pass,
            message: message.into(),
            suggestion: None,
        }
    }

    fn warn(name: &'static str, message: impl Into<String>, suggestion: impl Into<String>) -> Self {
        Self {
            name,
            status: DiagnosticStatus::Warn,
            message: message.into(),
            suggestion: Some(suggestion.into()),
        }
    }

    fn fail(name: &'static str, message: impl Into<String>, suggestion: impl Into<String>) -> Self {
        Self {
            name,
            status: DiagnosticStatus::Fail,
            message: message.into(),
            suggestion: Some(suggestion.into()),
        }
    }

    fn icon(&self) -> &'static str {
        match self.status {
            DiagnosticStatus::Pass => "✓",
            DiagnosticStatus::Warn => "⚠",
            DiagnosticStatus::Fail => "✗",
        }
    }
}

async fn handle_doctor_command(full: bool, format: &str, quiet: bool) -> Result<(), NikaError> {
    let mut checks: Vec<DiagnosticCheck> = vec![];

    // 1. Check .nika directory
    checks.push(check_nika_directory());

    // 2. Check config file
    checks.push(check_config_file());

    // 3. Check API keys
    checks.extend(check_api_keys());

    // 4. Check trace directory
    checks.push(check_trace_directory());

    // 5. Check Rust version
    checks.push(check_rust_version());

    // 6. Full mode: Check MCP connectivity (slow)
    if full {
        checks.push(check_mcp_connectivity().await);
    }

    // Output results
    if format == "json" {
        output_doctor_json(&checks);
    } else {
        output_doctor_text(&checks, quiet);
    }

    // Return error if any checks failed
    let has_failures = checks.iter().any(|c| c.status == DiagnosticStatus::Fail);
    if has_failures {
        return Err(NikaError::ValidationError {
            reason: "Some diagnostic checks failed".to_string(),
        });
    }

    Ok(())
}

fn check_nika_directory() -> DiagnosticCheck {
    match find_nika_dir() {
        Ok(dir) if dir.exists() => DiagnosticCheck::pass(
            "Project",
            format!(".nika directory found at {}", dir.display()),
        ),
        Ok(dir) => DiagnosticCheck::warn(
            "Project",
            format!("No .nika directory at {}", dir.display()),
            "Run 'nika init' to create project structure",
        ),
        Err(_) => DiagnosticCheck::fail(
            "Project",
            "Cannot determine current directory",
            "Check filesystem permissions",
        ),
    }
}

fn check_config_file() -> DiagnosticCheck {
    let nika_dir = match find_nika_dir() {
        Ok(d) => d,
        Err(_) => {
            return DiagnosticCheck::warn(
                "Config",
                "Cannot locate .nika directory",
                "Run 'nika init' first",
            )
        }
    };

    let config_path = nika_dir.join("config.toml");
    if !config_path.exists() {
        return DiagnosticCheck::warn(
            "Config",
            "No config.toml found",
            "Run 'nika init' to create default config",
        );
    }

    // Try to parse the config
    match fs::read_to_string(&config_path) {
        Ok(content) => match toml::from_str::<toml::Value>(&content) {
            Ok(_) => DiagnosticCheck::pass("Config", "config.toml is valid TOML"),
            Err(e) => DiagnosticCheck::fail(
                "Config",
                format!("config.toml has syntax errors: {}", e),
                "Run 'nika config edit' to fix",
            ),
        },
        Err(e) => DiagnosticCheck::fail(
            "Config",
            format!("Cannot read config.toml: {}", e),
            "Check file permissions",
        ),
    }
}

fn check_api_keys() -> Vec<DiagnosticCheck> {
    let mut checks = vec![];

    // Check common API keys (without exposing values)
    let keys = [
        ("ANTHROPIC_API_KEY", "Claude"),
        ("OPENAI_API_KEY", "OpenAI"),
        ("MISTRAL_API_KEY", "Mistral"),
        ("GROQ_API_KEY", "Groq"),
        ("DEEPSEEK_API_KEY", "DeepSeek"),
    ];

    let mut any_found = false;
    for (env_var, provider) in keys {
        if std::env::var(env_var).is_ok() {
            checks.push(DiagnosticCheck::pass(
                "API Key",
                format!("{} API key configured ({})", provider, env_var),
            ));
            any_found = true;
        }
    }

    // Note: Ollama removed in v0.27 — use provider: native with mistral.rs instead

    if !any_found {
        checks.push(DiagnosticCheck::warn(
            "API Key",
            "No LLM API keys found",
            "Set ANTHROPIC_API_KEY, OPENAI_API_KEY, or use provider: native",
        ));
    }

    checks
}

fn check_trace_directory() -> DiagnosticCheck {
    let nika_dir = match find_nika_dir() {
        Ok(d) => d,
        Err(_) => {
            return DiagnosticCheck::warn(
                "Traces",
                "Cannot locate .nika directory",
                "Run 'nika init' first",
            )
        }
    };

    let trace_dir = nika_dir.join("traces");

    // Check if directory exists
    if !trace_dir.exists() {
        return DiagnosticCheck::warn(
            "Traces",
            "Trace directory doesn't exist",
            "It will be created on first workflow run",
        );
    }

    // Check if writable by attempting to create a temp file
    let test_file = trace_dir.join(".nika_doctor_test");
    match fs::write(&test_file, b"test") {
        Ok(_) => {
            let _ = fs::remove_file(&test_file);
            DiagnosticCheck::pass(
                "Traces",
                format!("Trace directory writable ({})", trace_dir.display()),
            )
        }
        Err(e) => DiagnosticCheck::fail(
            "Traces",
            format!("Trace directory not writable: {}", e),
            "Check directory permissions",
        ),
    }
}

fn check_rust_version() -> DiagnosticCheck {
    // Get rustc version
    match std::process::Command::new("rustc")
        .arg("--version")
        .output()
    {
        Ok(output) => {
            let version = String::from_utf8_lossy(&output.stdout);
            let version = version.trim();
            if version.contains("1.8") || version.contains("1.9") {
                DiagnosticCheck::pass("Rust", version.to_string())
            } else if version.starts_with("rustc 1.7") {
                DiagnosticCheck::warn(
                    "Rust",
                    format!("{} (older version)", version),
                    "Consider updating: rustup update",
                )
            } else {
                DiagnosticCheck::pass("Rust", version.to_string())
            }
        }
        Err(_) => DiagnosticCheck::warn(
            "Rust",
            "rustc not found in PATH",
            "Install Rust: https://rustup.rs",
        ),
    }
}

async fn check_mcp_connectivity() -> DiagnosticCheck {
    // This is a placeholder - in a real implementation, we'd try to connect
    // to configured MCP servers from the config file
    DiagnosticCheck::pass(
        "MCP",
        "MCP connectivity check (requires configured servers)",
    )
}

fn output_doctor_text(checks: &[DiagnosticCheck], quiet: bool) {
    if !quiet {
        println!();
        println!("{}", "Nika Doctor".bold());
        println!("{}", "═".repeat(50));
        println!();
    }

    let mut pass_count = 0;
    let mut warn_count = 0;
    let mut fail_count = 0;

    for check in checks {
        let icon = match check.status {
            DiagnosticStatus::Pass => check.icon().green(),
            DiagnosticStatus::Warn => check.icon().yellow(),
            DiagnosticStatus::Fail => check.icon().red(),
        };

        println!("{} {} {}", icon, check.name.bold(), check.message);

        if let Some(ref suggestion) = check.suggestion {
            println!("  {} {}", "→".cyan(), suggestion);
        }

        match check.status {
            DiagnosticStatus::Pass => pass_count += 1,
            DiagnosticStatus::Warn => warn_count += 1,
            DiagnosticStatus::Fail => fail_count += 1,
        }
    }

    if !quiet {
        println!();
        println!(
            "{} {} passed, {} warnings, {} failed",
            "Summary:".bold(),
            pass_count.to_string().green(),
            warn_count.to_string().yellow(),
            fail_count.to_string().red()
        );
    }
}

fn output_doctor_json(checks: &[DiagnosticCheck]) {
    let results: Vec<serde_json::Value> = checks
        .iter()
        .map(|c| {
            serde_json::json!({
                "name": c.name,
                "status": match c.status {
                    DiagnosticStatus::Pass => "pass",
                    DiagnosticStatus::Warn => "warn",
                    DiagnosticStatus::Fail => "fail",
                },
                "message": c.message,
                "suggestion": c.suggestion,
            })
        })
        .collect();

    let output = serde_json::json!({
        "checks": results,
        "summary": {
            "pass": checks.iter().filter(|c| c.status == DiagnosticStatus::Pass).count(),
            "warn": checks.iter().filter(|c| c.status == DiagnosticStatus::Warn).count(),
            "fail": checks.iter().filter(|c| c.status == DiagnosticStatus::Fail).count(),
        }
    });

    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}

// ============================================================================
// Jobs Daemon Command Handler (v0.14.0)
// ============================================================================

#[cfg(feature = "jobs")]
async fn handle_jobs_command(action: JobsAction, quiet: bool) -> Result<(), NikaError> {
    use colored::Colorize;
    use nika::jobs::{JobsConfig, JobsDaemon, StateStore};

    match action {
        JobsAction::Start { foreground, config } => {
            // Check if config file exists
            if !config.exists() {
                return Err(NikaError::ConfigError {
                    reason: format!("Jobs config file not found: {}", config.display()),
                });
            }

            // Load configuration
            let jobs_config =
                JobsConfig::from_file(&config).map_err(|e| NikaError::ConfigError {
                    reason: format!("Failed to load jobs config: {}", e),
                })?;

            if !quiet {
                println!(
                    "{} Starting Jobs Daemon with {} jobs from {}",
                    "🚀".bold(),
                    jobs_config.definitions.len().to_string().cyan(),
                    config.display()
                );
            }

            // Create and start daemon
            let mut daemon = JobsDaemon::new(jobs_config).map_err(|e| NikaError::RuntimeError {
                reason: format!("Failed to create daemon: {}", e),
            })?;

            if foreground {
                // Run in foreground (blocking)
                if !quiet {
                    println!(
                        "{} Running in foreground mode (Ctrl+C to stop)",
                        "ℹ️".bold()
                    );
                }
                daemon.start().await.map_err(|e| NikaError::RuntimeError {
                    reason: format!("Daemon error: {}", e),
                })?;
            } else {
                // Daemonize
                daemon.start().await.map_err(|e| NikaError::RuntimeError {
                    reason: format!("Failed to start daemon: {}", e),
                })?;

                if !quiet {
                    println!("{} Jobs Daemon started successfully", "✅".green());
                }
            }
        }

        JobsAction::Stop { force } => {
            let pid_file = find_nika_dir()?.join("jobs.pid");

            if !pid_file.exists() {
                return Err(NikaError::RuntimeError {
                    reason: "No running daemon found (jobs.pid not found)".to_string(),
                });
            }

            if !quiet {
                println!(
                    "{} Stopping Jobs Daemon{}...",
                    "🛑".bold(),
                    if force { " (force)" } else { "" }
                );
            }

            JobsDaemon::stop_by_pid_file(&pid_file).map_err(|e| NikaError::RuntimeError {
                reason: format!("Failed to stop daemon: {}", e),
            })?;

            if !quiet {
                println!("{} Jobs Daemon stopped", "✅".green());
            }
        }

        JobsAction::Status { json } => {
            let pid_file = find_nika_dir()?.join("jobs.pid");

            let status = JobsDaemon::get_status_from_pid_file(&pid_file);
            let is_running = matches!(
                status,
                nika::jobs::DaemonStatus::Running | nika::jobs::DaemonStatus::Starting
            );

            if json {
                let output = serde_json::json!({
                    "running": is_running,
                    "status": status.to_string(),
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                println!("{}", "Jobs Daemon Status".bold().cyan());
                match status {
                    nika::jobs::DaemonStatus::Running => {
                        println!("  {} {}", "Status:".dimmed(), "Running".green().bold());
                    }
                    nika::jobs::DaemonStatus::Starting => {
                        println!("  {} {}", "Status:".dimmed(), "Starting".yellow().bold());
                    }
                    nika::jobs::DaemonStatus::ShuttingDown => {
                        println!(
                            "  {} {}",
                            "Status:".dimmed(),
                            "Shutting Down".yellow().bold()
                        );
                    }
                    nika::jobs::DaemonStatus::Stopped => {
                        println!("  {} {}", "Status:".dimmed(), "Stopped".red().bold());
                    }
                }
            }
        }

        JobsAction::List { json, config } => {
            // Check if config file exists
            if !config.exists() {
                return Err(NikaError::ConfigError {
                    reason: format!("Jobs config file not found: {}", config.display()),
                });
            }

            // Load configuration
            let jobs_config =
                JobsConfig::from_file(&config).map_err(|e| NikaError::ConfigError {
                    reason: format!("Failed to load jobs config: {}", e),
                })?;

            if json {
                let output: Vec<serde_json::Value> = jobs_config
                    .definitions
                    .iter()
                    .map(|j| {
                        serde_json::json!({
                            "name": j.name,
                            "workflow": j.workflow.display().to_string(),
                            "schedule": format!("{:?}", j.trigger),
                            "enabled": j.enabled,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                println!("{}", "Configured Jobs".bold().cyan());
                println!();
                for job in &jobs_config.definitions {
                    let status = if job.enabled {
                        "●".green()
                    } else {
                        "○".dimmed()
                    };
                    println!(
                        "  {} {} {}",
                        status,
                        job.name.bold(),
                        format!("({})", job.workflow.display()).dimmed()
                    );
                    println!("    {} {:?}", "Schedule:".dimmed(), job.trigger);
                }
                println!();
                println!(
                    "{} {} jobs configured",
                    "Total:".dimmed(),
                    jobs_config.definitions.len()
                );
            }
        }

        JobsAction::Trigger { job_name, config } => {
            // Load config and create daemon to trigger job
            let jobs_config =
                JobsConfig::from_file(&config).map_err(|e| NikaError::ConfigError {
                    reason: format!("Failed to load jobs config: {}", e),
                })?;

            // Verify job exists
            if !jobs_config.definitions.iter().any(|j| j.name == job_name) {
                return Err(NikaError::ValidationError {
                    reason: format!("Job '{}' not found in config", job_name),
                });
            }

            if !quiet {
                println!("{} Triggering job '{}'...", "⚡".bold(), job_name.cyan());
            }

            // Create daemon and trigger
            let daemon = JobsDaemon::new(jobs_config).map_err(|e| NikaError::RuntimeError {
                reason: format!("Failed to create daemon: {}", e),
            })?;

            daemon
                .trigger_job(&job_name)
                .await
                .map_err(|e| NikaError::RuntimeError {
                    reason: format!("Failed to trigger job: {}", e),
                })?;

            if !quiet {
                println!("{} Job '{}' triggered successfully", "✅".green(), job_name);
            }
        }

        JobsAction::Pause { job_name } => {
            let pid_file = find_nika_dir()?.join("jobs.pid");

            if !pid_file.exists() {
                return Err(NikaError::RuntimeError {
                    reason: "No running daemon found".to_string(),
                });
            }

            // Send pause command to daemon via IPC
            // For now, we'll use a simple approach via the daemon
            let daemon = JobsDaemon::from_config_file(&find_nika_dir()?.join("jobs.toml"))
                .map_err(|e| NikaError::RuntimeError {
                    reason: format!("Failed to connect to daemon: {}", e),
                })?;

            daemon
                .pause_job(&job_name)
                .await
                .map_err(|e| NikaError::RuntimeError {
                    reason: format!("Failed to pause job: {}", e),
                })?;

            if !quiet {
                println!("{} Job '{}' paused", "⏸️".bold(), job_name.yellow());
            }
        }

        JobsAction::Resume { job_name } => {
            let pid_file = find_nika_dir()?.join("jobs.pid");

            if !pid_file.exists() {
                return Err(NikaError::RuntimeError {
                    reason: "No running daemon found".to_string(),
                });
            }

            let daemon = JobsDaemon::from_config_file(&find_nika_dir()?.join("jobs.toml"))
                .map_err(|e| NikaError::RuntimeError {
                    reason: format!("Failed to connect to daemon: {}", e),
                })?;

            daemon
                .resume_job(&job_name)
                .await
                .map_err(|e| NikaError::RuntimeError {
                    reason: format!("Failed to resume job: {}", e),
                })?;

            if !quiet {
                println!("{} Job '{}' resumed", "▶️".bold(), job_name.green());
            }
        }

        JobsAction::History {
            job_name,
            limit,
            json,
        } => {
            let state_dir = find_nika_dir()?.join("jobs.db");
            let store = StateStore::new(&state_dir).map_err(|e| NikaError::RuntimeError {
                reason: format!("Failed to open state store: {}", e),
            })?;

            let executions = store
                .list_executions(job_name.as_deref(), limit)
                .map_err(|e| NikaError::RuntimeError {
                    reason: format!("Failed to query history: {}", e),
                })?;

            if json {
                let output: Vec<serde_json::Value> = executions
                    .iter()
                    .map(|e| {
                        serde_json::json!({
                            "id": e.id,
                            "job_name": e.job_name,
                            "status": format!("{:?}", e.status),
                            "trigger": e.trigger,
                            "started_at": e.started_at.to_rfc3339(),
                            "ended_at": e.ended_at.map(|t| t.to_rfc3339()),
                            "duration_ms": e.duration_ms,
                            "attempt": e.attempt,
                            "error": e.error,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                let title = match &job_name {
                    Some(name) => format!("Execution History for '{}'", name),
                    None => "Execution History (All Jobs)".to_string(),
                };
                println!("{}", title.bold().cyan());
                println!();

                if executions.is_empty() {
                    println!("  {}", "No executions found".dimmed());
                } else {
                    for exec in &executions {
                        let status_icon = match exec.status {
                            nika::jobs::JobExecutionStatus::Completed => "✅",
                            nika::jobs::JobExecutionStatus::Failed => "❌",
                            nika::jobs::JobExecutionStatus::Running => "🔄",
                            nika::jobs::JobExecutionStatus::Queued => "⏳",
                            nika::jobs::JobExecutionStatus::Cancelled => "🚫",
                        };

                        let duration = exec
                            .duration_ms
                            .map(|d| format!("{}ms", d))
                            .unwrap_or_else(|| "-".to_string());

                        println!(
                            "  {} {} {} {}",
                            status_icon,
                            exec.job_name.bold(),
                            exec.started_at
                                .format("%Y-%m-%d %H:%M:%S")
                                .to_string()
                                .dimmed(),
                            format!("({})", duration).dimmed()
                        );

                        if let Some(ref err) = exec.error {
                            println!("    {} {}", "Error:".red(), err);
                        }
                    }
                }

                println!();
                println!(
                    "{} {} executions shown",
                    "Total:".dimmed(),
                    executions.len()
                );
            }
        }

        JobsAction::Reload => {
            let pid_file = find_nika_dir()?.join("jobs.pid");

            if !quiet {
                println!("{} Reloading daemon configuration...", "🔄".bold());
            }

            JobsDaemon::reload_by_signal(&pid_file).map_err(|e| NikaError::RuntimeError {
                reason: format!("Failed to reload daemon: {}", e),
            })?;

            if !quiet {
                println!("{} Configuration reload signal sent", "✅".green());
            }
        }

        // === Background Job Commands (v0.27) ===
        // These proxy to `nika-daemon jobs` for background workflow execution
        JobsAction::Submit {
            workflow,
            args,
            name,
            priority,
        } => {
            // Proxy to: nika-daemon jobs submit <workflow> [args...] [--name NAME] [--priority N]
            let mut cmd_args = vec!["jobs", "submit"];
            let workflow_str = workflow.to_string_lossy();
            cmd_args.push(&workflow_str);

            // Add workflow arguments
            let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            cmd_args.extend(args_refs.iter());

            let name_flag;
            if let Some(n) = &name {
                cmd_args.push("--name");
                name_flag = n.clone();
                cmd_args.push(&name_flag);
            }

            let priority_str = priority.to_string();
            cmd_args.push("--priority");
            cmd_args.push(&priority_str);

            if !quiet {
                println!(
                    "{} Submitting workflow for background execution...",
                    "📤".bold()
                );
            }

            let status = std::process::Command::new("nika-daemon")
                .args(&cmd_args)
                .status()
                .map_err(|e| {
                    if e.kind() == std::io::ErrorKind::NotFound {
                        eprintln!();
                        eprintln!("nika-daemon not found. Install from:");
                        eprintln!("  cargo install nika-daemon");
                    }
                    NikaError::InvalidConfig {
                        message: format!("Failed to run nika-daemon jobs submit: {}", e),
                    }
                })?;

            if !status.success() {
                return Err(NikaError::RuntimeError {
                    reason: "nika-daemon jobs submit failed".to_string(),
                });
            }
        }

        JobsAction::Cancel { id } => {
            // Proxy to: nika-daemon jobs cancel <id>
            if !quiet {
                println!("{} Cancelling job {}...", "🛑".bold(), id);
            }

            let status = std::process::Command::new("nika-daemon")
                .args(["jobs", "cancel", &id])
                .status()
                .map_err(|e| {
                    if e.kind() == std::io::ErrorKind::NotFound {
                        eprintln!();
                        eprintln!("nika-daemon not found. Install from:");
                        eprintln!("  cargo install nika-daemon");
                    }
                    NikaError::InvalidConfig {
                        message: format!("Failed to run nika-daemon jobs cancel: {}", e),
                    }
                })?;

            if !status.success() {
                return Err(NikaError::RuntimeError {
                    reason: format!("Failed to cancel job {}", id),
                });
            }
        }

        JobsAction::Output { id, follow } => {
            // Proxy to: nika-daemon jobs output <id> [--follow]
            let mut cmd_args = vec!["jobs", "output", &id];
            if follow {
                cmd_args.push("--follow");
            }

            // For output, we want to stream stdout/stderr directly
            let status = std::process::Command::new("nika-daemon")
                .args(&cmd_args)
                .status()
                .map_err(|e| {
                    if e.kind() == std::io::ErrorKind::NotFound {
                        eprintln!();
                        eprintln!("nika-daemon not found. Install from:");
                        eprintln!("  cargo install nika-daemon");
                    }
                    NikaError::InvalidConfig {
                        message: format!("Failed to run nika-daemon jobs output: {}", e),
                    }
                })?;

            if !status.success() {
                return Err(NikaError::RuntimeError {
                    reason: format!("Failed to get output for job {}", id),
                });
            }
        }

        JobsAction::Clear { all } => {
            // Proxy to: nika-daemon jobs clear [--all]
            let mut cmd_args = vec!["jobs", "clear"];
            if all {
                cmd_args.push("--all");
            }

            if !quiet {
                if all {
                    println!("{} Clearing all jobs...", "🧹".bold());
                } else {
                    println!("{} Clearing completed/failed jobs...", "🧹".bold());
                }
            }

            let status = std::process::Command::new("nika-daemon")
                .args(&cmd_args)
                .status()
                .map_err(|e| {
                    if e.kind() == std::io::ErrorKind::NotFound {
                        eprintln!();
                        eprintln!("nika-daemon not found. Install from:");
                        eprintln!("  cargo install nika-daemon");
                    }
                    NikaError::InvalidConfig {
                        message: format!("Failed to run nika-daemon jobs clear: {}", e),
                    }
                })?;

            if !status.success() {
                return Err(NikaError::RuntimeError {
                    reason: "Failed to clear jobs".to_string(),
                });
            }

            if !quiet {
                println!("{} Jobs cleared successfully", "✅".green());
            }
        }
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// NEW COMMAND - Workflow scaffolding (v0.19.3)
// ═══════════════════════════════════════════════════════════════════════════

#[allow(clippy::too_many_arguments)]
fn handle_new_command(
    name: Option<String>,
    wizard: bool,
    template: Option<String>,
    verb: Option<String>,
    provider: Option<String>,
    output: Option<String>,
    with_mcp: bool,
    with_include: bool,
    with_artifacts: bool,
    output_dir: Option<PathBuf>,
    list: bool,
    quiet: bool,
) -> Result<(), NikaError> {
    use nika::new::{
        create_from_template, list_templates, NewWorkflowConfig, OutputFormat, Provider, Template,
        Verb,
    };

    let output_dir = output_dir.unwrap_or_else(|| PathBuf::from("."));

    // Handle --list flag
    if list {
        if !quiet {
            println!("{}", "Available templates:".bold());
            println!();
        }
        for (name, description, category) in list_templates() {
            if quiet {
                println!("{}", name);
            } else {
                println!(
                    "  {} {}",
                    format!("{:<18}", name).green(),
                    format!("[{}] {}", category, description).white()
                );
            }
        }
        return Ok(());
    }

    // Determine mode: wizard, template, or custom flags
    // Prefix with _ to avoid unused warning when TUI is disabled
    let _has_flags = template.is_some()
        || verb.is_some()
        || provider.is_some()
        || output.is_some()
        || with_mcp
        || with_include
        || with_artifacts;

    // If wizard flag is set, or no name and no flags, launch wizard
    #[cfg(feature = "tui")]
    if wizard || (name.is_none() && !_has_flags) {
        let path = nika::new::wizard::run_wizard(output_dir)?;
        if !quiet {
            println!("{} Created: {}", "SUCCESS!".green().bold(), path.display());
            println!("  Run: nika {}", path.display());
        }
        return Ok(());
    }

    // Non-TUI mode: require name
    #[cfg(not(feature = "tui"))]
    if wizard {
        return Err(NikaError::ValidationError {
            reason: "Wizard mode requires TUI feature. Use --template or flags instead."
                .to_string(),
        });
    }

    // Template mode
    if let Some(template_name) = template {
        let workflow_name = name.unwrap_or_else(|| "my-workflow".to_string());
        let tmpl =
            Template::from_name(&template_name).ok_or_else(|| NikaError::ValidationError {
                reason: format!(
                    "Unknown template: '{}'. Use --list to see available templates.",
                    template_name
                ),
            })?;

        let path = create_from_template(&workflow_name, tmpl, &output_dir)?;

        if !quiet {
            println!("{} Created: {}", "SUCCESS!".green().bold(), path.display());
            println!("  Template: {}", tmpl.name().cyan());
            println!("  Run: nika {}", path.display());
        }
        return Ok(());
    }

    // Custom flags mode - require name
    let workflow_name = name.ok_or_else(|| NikaError::ValidationError {
        reason: "Workflow name is required. Use: nika new <NAME> [OPTIONS]".to_string(),
    })?;

    // Parse verb
    let verb = verb
        .map(|v| {
            Verb::from_name(&v).ok_or_else(|| NikaError::ValidationError {
                reason: format!(
                    "Unknown verb: '{}'. Valid: infer, exec, fetch, invoke, agent",
                    v
                ),
            })
        })
        .transpose()?
        .unwrap_or_default();

    // Parse provider
    let provider = provider
        .map(|p| {
            Provider::from_name(&p).ok_or_else(|| NikaError::ValidationError {
                reason: format!(
                    "Unknown provider: '{}'. Valid: claude, openai, mistral, groq, deepseek, native",
                    p
                ),
            })
        })
        .transpose()?
        .unwrap_or_default();

    // Parse output format
    let output_format = output
        .map(|o| {
            OutputFormat::from_name(&o).ok_or_else(|| NikaError::ValidationError {
                reason: format!("Unknown output format: '{}'. Valid: text, json, yaml", o),
            })
        })
        .transpose()?
        .unwrap_or_default();

    // Build config
    let config = NewWorkflowConfig {
        name: workflow_name,
        description: None,
        verb,
        provider,
        model: None,
        output_format,
        with_mcp,
        with_include,
        with_artifacts,
        output_dir,
    };

    // Generate workflow
    let path = config.write()?;

    if !quiet {
        println!("{} Created: {}", "SUCCESS!".green().bold(), path.display());
        println!("  Verb: {}", config.verb.name().cyan());
        println!("  Provider: {}", config.provider.name().yellow());
        if with_mcp {
            println!("  MCP: {}", "enabled".magenta());
        }
        println!("  Run: nika {}", path.display());
    }

    Ok(())
}
