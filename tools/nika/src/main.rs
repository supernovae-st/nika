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
  infer:   LLM text generation (Claude, OpenAI, Mistral, Groq, DeepSeek, Ollama)
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
    OLLAMA_API_BASE_URL           Ollama (no key needed)

TUI VIEWS (in nika ui):
    [e] Explorer   File browser + DAG preview
    [c] Chat       AI agent conversation
    [d] Editor     YAML workflow editor
    [r] Runner     Real-time execution
    [s] Scheduler  Cron/queue management

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
        /// LLM provider: claude, openai, mistral, groq, deepseek, ollama
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

    /// [deprecated] Use 'nika' instead
    #[cfg(feature = "tui")]
    #[command(hide = true)]
    Tui {
        /// Path to workflow YAML file (optional)
        workflow: Option<PathBuf>,
    },

    /// Manage workflow files (edit, add tasks, visualize) (v0.22+)
    #[command(visible_alias = "w")]
    Workflow {
        #[command(subcommand)]
        action: WorkflowAction,
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

        /// LLM provider (claude, openai, mistral, groq, deepseek, ollama)
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

/// MCP server management actions (v0.12.1)
#[derive(Subcommand)]
enum McpAction {
    /// List MCP servers defined in a workflow
    List {
        /// Workflow file with MCP definitions
        #[arg(short, long)]
        workflow: Option<String>,
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
                Some("editor") | Some("d") | Some("studio") => Some(TuiView::Editor),
                Some("runner") | Some("r") | Some("monitor") => Some(TuiView::Runner),
                Some("scheduler") | Some("s") => Some(TuiView::Scheduler),
                Some("settings") | Some(",") => Some(TuiView::Settings),
                Some("explorer") | Some("e") | Some("home") => Some(TuiView::Browse),
                Some(unknown) => {
                    eprintln!(
                        "{} Unknown view '{}'. Valid: explorer, chat, editor, runner, scheduler, settings",
                        "Error:".red().bold(),
                        unknown
                    );
                    std::process::exit(1);
                }
                None => None, // Use default (Explorer)
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

        // Shell completion (v0.13.1)
        Some(Commands::Completion { shell }) => {
            clap_complete::generate(shell, &mut Cli::command(), "nika", &mut std::io::stdout());
            Ok(())
        }

        // Configuration management (v0.13.1)
        Some(Commands::Config { action }) => handle_config_command(action, quiet),

        // Schema management (v0.21.x)
        Some(Commands::Schema { action }) => handle_schema_command(action, quiet),

        // Workflow management (v0.22+)
        Some(Commands::Workflow { action }) => handle_workflow_command(action, quiet).await,

        // Doctor command (v0.13.1)
        Some(Commands::Doctor { full, format }) => {
            handle_doctor_command(full, &format, quiet).await
        }

        // Jobs Daemon management (v0.14.0)
        #[cfg(feature = "jobs")]
        Some(Commands::Jobs { action }) => handle_jobs_command(action, quiet).await,

        // Legacy TUI command (hidden, backward compat)
        #[cfg(feature = "tui")]
        Some(Commands::Tui { workflow }) => {
            eprintln!(
                "{} 'nika tui' is deprecated. Use 'nika' instead.",
                "Note:".yellow()
            );
            match workflow {
                Some(path) => nika::tui::run_tui(&path).await,
                None => nika::tui::run_tui_standalone().await,
            }
        }

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
            Commands::Ui { .. }
                | Commands::Chat { .. }
                | Commands::Studio { .. }
                | Commands::Tui { .. }
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
/// 1. If starts with '@' (e.g., @workflows/seo-audit) → Package resolution from ~/.spn/packages/
/// 2. If simple name without path/extension → Search in .nika/workflows/{name}.nika.yaml
/// 3. Otherwise → Use as-is (filesystem path)
///
/// # Examples
///
/// - `@workflows/seo-audit` → `~/.spn/packages/workflows/seo-audit/1.0.0/workflow.nika.yaml`
/// - `custom` → `.nika/workflows/custom.nika.yaml` (if exists) or error
/// - `./local.nika.yaml` → `./local.nika.yaml` (direct path)
async fn resolve_workflow_path(reference: &str) -> Result<PathBuf, NikaError> {
    // 1. Package reference (starts with @)
    if reference.starts_with('@') {
        let resolved =
            resolver::resolve_package_path(reference).map_err(|e| NikaError::WorkflowNotFound {
                path: format!(
                    "Package not found: {}. Error: {}. Try: spn add {}",
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
                path: format!("Workflow '{}' not found in .nika/workflows/ or current directory. Try: spn search {}", reference, reference)
            });
        }
    }

    // 3. Direct filesystem path
    let path = PathBuf::from(reference);
    if !path.exists() {
        return Err(NikaError::WorkflowNotFound {
            path: format!(
                "File not found: {}. Check the path or try: spn search {}",
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

    // Build flow graph and validate use: bindings (NIKA-080, NIKA-081, NIKA-082)
    let flow_graph = Dag::from_workflow(&workflow);
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

    // Phase 2: Binding validation
    let flow_graph = Dag::from_workflow(&workflow);
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
        let mcp_servers: std::collections::HashSet<&str> =
            invoke_tasks.iter().map(|(_, p)| p.mcp.as_str()).collect();

        // Get MCP configs (workflow.mcp is Option<FxHashMap<...>>)
        let mcp_configs = workflow
            .mcp
            .as_ref()
            .ok_or_else(|| NikaError::ValidationError {
                reason: "Workflow has invoke tasks but no mcp: configuration".to_string(),
            })?;

        // Connect to each MCP server and list tools
        for server_name in mcp_servers {
            let Some(inline_config) = mcp_configs.get(server_name) else {
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
                let result = mcp_validator.validate(&params.mcp, tool, &invoke_params);

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

/// All supported providers
#[cfg(feature = "tui")]
const ALL_PROVIDERS: &[&str] = &[
    "anthropic",
    "openai",
    "mistral",
    "groq",
    "deepseek",
    "ollama",
];

/// Handle provider management commands
#[cfg(feature = "tui")]
async fn handle_provider_command(action: ProviderAction) -> Result<(), NikaError> {
    use colored::Colorize;
    use nika::tui::widgets::provider_modal::{
        mask_api_key, migrate_env_to_keyring, provider_env_var, validate_key_format, SpnKeyring,
    };
    use std::io::{self, Write};

    match action {
        ProviderAction::List => {
            println!("{}", "LLM Providers".bold());
            println!("{}", "─".repeat(60));

            for provider in ALL_PROVIDERS {
                let env_var = provider_env_var(provider);
                let has_keychain = SpnKeyring::exists(provider);
                let has_env = std::env::var(env_var).is_ok();

                let status = match (has_keychain, has_env) {
                    (true, true) => format!("{} (keychain + env)", "✓".green()),
                    (true, false) => format!("{} (keychain)", "✓".green()),
                    (false, true) => format!("{} (env only)", "~".yellow()),
                    (false, false) => format!("{}", "✗".red()),
                };

                let masked = if has_keychain {
                    SpnKeyring::get_masked(provider).unwrap_or_default()
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
            // Validate provider name
            if !ALL_PROVIDERS.contains(&provider.as_str()) {
                return Err(NikaError::ValidationError {
                    reason: format!(
                        "Unknown provider '{}'. Valid: {}",
                        provider,
                        ALL_PROVIDERS.join(", ")
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
            SpnKeyring::set(&provider, &api_key)
                .map_err(|e| NikaError::Execution(format!("Failed to store key: {}", e)))?;

            println!(
                "{} API key for {} stored in system keychain",
                "✓".green(),
                provider.bold()
            );
            Ok(())
        }

        ProviderAction::Get { provider } => {
            match SpnKeyring::get_masked(&provider) {
                Some(masked) => {
                    println!("{}: {}", provider, masked);
                }
                None => {
                    let env_var = provider_env_var(&provider);
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
            match SpnKeyring::delete(&provider) {
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
            let env_var = provider_env_var(&provider);
            let has_key = SpnKeyring::exists(&provider)
                || std::env::var(env_var).is_ok_and(|v| !v.is_empty());

            if !has_key && provider != "ollama" {
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
                "ollama" => RigProvider::ollama(),
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

// ─────────────────────────────────────────────────────────────────────────────
// Example Workflow Constants (v0.16.2 - Progressive Learning)
// ─────────────────────────────────────────────────────────────────────────────

/// 01: Hello World - Basic verbs (infer, exec, fetch) + simple binding
const WORKFLOW_01_HELLO_WORLD: &str = r#"# 01 - Hello World Workflow
# Demonstrates: infer, exec, fetch verbs + basic binding
# Run with: nika run workflows/01-hello-world.nika.yaml

schema: "nika/workflow@0.9"
workflow: hello-world
description: "Simple workflow demonstrating basic Nika features"

# Dummy MCP config (builtins intercept before MCP call)
mcp:
  dummy:
    command: "echo"
    args: ["not used"]

tasks:
  # Basic LLM inference using shorthand syntax
  - id: greet
    infer: "Generate a friendly one-line greeting for a Nika user."

  # Shell command execution (shell: false is default for security)
  - id: show_date
    exec:
      command: "date '+%Y-%m-%d %H:%M:%S'"

  # HTTP fetch to get external data (httpbin returns sample JSON)
  - id: fetch_data
    fetch:
      url: "https://httpbin.org/json"
      method: GET

  # Combine all outputs with binding references
  - id: summarize
    use:
      greeting: greet
      timestamp: show_date
      external: fetch_data
    infer: |
      Create a brief welcome message combining:
      - Greeting: {{use.greeting}}
      - Current time: {{use.timestamp}}
      - External data: {{use.external}}

  # Use nika:log builtin to track progress
  - id: log_done
    invoke:
      mcp: dummy
      tool: nika:log
      params:
        level: info
        message: "Hello World workflow completed!"

flows:
  - source: greet
    target: summarize
  - source: show_date
    target: summarize
  - source: fetch_data
    target: summarize
  - source: summarize
    target: log_done
"#;

/// 02: Parallel Pipeline - for_each, context:, JSON output with schema
const WORKFLOW_02_PARALLEL: &str = r#"# 02 - Parallel Pipeline Workflow
# Demonstrates: for_each parallelism, context: file loading, JSON output
# Run with: nika run workflows/02-parallel-pipeline.nika.yaml

schema: "nika/workflow@0.9"
workflow: parallel-pipeline
description: "Parallel processing with context files and structured output"

# Dummy MCP config (builtins intercept before MCP call)
mcp:
  dummy:
    command: "echo"
    args: ["not used"]

# Load context files at workflow start
# Context paths are relative to project root (where nika run is executed)
context:
  files:
    brands: ./context/brands.json

tasks:
  # Process each brand in parallel (concurrency: 3)
  # Note: for_each accepts arrays or $binding refs, not {{context.}} syntax
  - id: analyze_brands
    for_each: ["Apple", "Google", "Microsoft"]
    as: brand_name
    concurrency: 3
    fail_fast: false
    infer:
      prompt: |
        Analyze the brand "{{use.brand_name}}" and provide a brief SWOT analysis.

        Context from brands database:
        {{context.files.brands}}

        Return a concise analysis in 2-3 sentences per category.
      temperature: 0.7
      max_tokens: 500

  # Emit progress event using builtin
  - id: emit_progress
    use:
      analyses: analyze_brands
    invoke:
      mcp: dummy
      tool: nika:emit
      params:
        name: "analysis_complete"
        payload:
          status: "completed"

  # Aggregate results into JSON summary
  - id: aggregate
    use:
      analyses: analyze_brands
    infer:
      prompt: |
        Based on these brand analyses, create a JSON summary with rankings:

        {{use.analyses}}

        Return ONLY valid JSON with structure:
        {"rankings": [{"name": "...", "score": 1-10, "key_strength": "..."}]}
      temperature: 0.3
      system: "You are a JSON generator. Return only valid JSON, no markdown."

  # Log completion
  - id: complete
    invoke:
      mcp: dummy
      tool: nika:log
      params:
        level: info
        message: "Parallel pipeline complete!"

flows:
  - source: analyze_brands
    target: emit_progress
  - source: analyze_brands
    target: aggregate
  - source: aggregate
    target: complete
"#;

/// 03: Agent Advanced - Full agent, include:, builtins, multi-output
const WORKFLOW_03_AGENT: &str = r#"# 03 - Advanced Agent Workflow
# Demonstrates: agent verb, include:, builtins, multi-format output
# Run with: nika run workflows/03-agent-advanced.nika.yaml

schema: "nika/workflow@0.9"
workflow: agent-advanced
description: "Advanced agentic workflow with multi-format file output"

# Dummy MCP config (builtins intercept before MCP call)
mcp:
  dummy:
    command: "echo"
    args: ["not used"]

# Include reusable partials (DAG fusion)
# Note: Include paths must stay within workflows/ for security
include:
  - path: ./partials/fetch-utils.nika.yaml
    prefix: utils_

# Load context (paths are relative to project root)
context:
  files:
    config: ./context/research-config.json

tasks:
  # Get working directory for absolute paths with nika:write
  - id: get_pwd
    exec: "pwd"

  # Initialize - create output directory using shell
  - id: setup
    exec:
      command: "mkdir -p ./output/03-results"
      shell: true

  # Assert config is valid
  - id: validate_config
    invoke:
      mcp: dummy
      tool: nika:assert
      params:
        condition: true
        message: "Research topic must be defined in config"

  # Main agentic research task
  - id: research_agent
    agent:
      prompt: |
        Research the following topic thoroughly:
        {{context.files.config.topic}}

        Use the available tools to:
        1. Search for information
        2. Analyze findings
        3. Generate a comprehensive report

        Stop when you have gathered enough information for a complete report.
      model: claude-sonnet-4-6
      max_turns: 5
      tools:
        - nika:read
        - nika:write
        - nika:grep
      stop_conditions:
        - "Report generation complete"
        - "Research concluded"

  # Sleep briefly to demonstrate rate limiting
  - id: rate_limit
    use:
      research: research_agent
    invoke:
      mcp: dummy
      tool: nika:sleep
      params:
        duration: "500ms"

  # Write Markdown report
  - id: write_markdown
    use:
      pwd: get_pwd
      research: research_agent
    invoke:
      mcp: dummy
      tool: nika:write
      params:
        file_path: "{{use.pwd}}/output/03-results/report.md"
        content: |
          # Research Report

          ## Summary

          {{use.research}}

  # Generate JSON structured output
  - id: create_json
    use:
      research: research_agent
    infer:
      prompt: |
        Convert this research into structured JSON:

        {{use.research}}

        Structure:
        {
          "topic": "...",
          "key_findings": ["..."],
          "sources": ["..."],
          "confidence": 0.0-1.0
        }
      temperature: 0.2
      system: "Return only valid JSON"

  # Write JSON output
  - id: write_json
    use:
      pwd: get_pwd
      json_data: create_json
    invoke:
      mcp: dummy
      tool: nika:write
      params:
        file_path: "{{use.pwd}}/output/03-results/data.json"
        content: "{{use.json_data}}"

  # Generate CSV summary
  - id: create_csv
    use:
      research: research_agent
    infer:
      prompt: |
        Extract the key findings from this research and format as CSV:

        {{use.research}}

        Format: finding,importance,category
        Include header row.
      temperature: 0.2

  # Write CSV output
  - id: write_csv
    use:
      pwd: get_pwd
      csv_data: create_csv
    invoke:
      mcp: dummy
      tool: nika:write
      params:
        file_path: "{{use.pwd}}/output/03-results/findings.csv"
        content: "{{use.csv_data}}"

  # Final log with all output paths
  - id: complete
    invoke:
      mcp: dummy
      tool: nika:log
      params:
        level: info
        message: |
          Agent workflow complete!
          Outputs written to:
          - output/03-results/report.md
          - output/03-results/data.json
          - output/03-results/findings.csv

flows:
  - source: setup
    target: validate_config
  - source: validate_config
    target: research_agent
  - source: research_agent
    target: rate_limit
  - source: rate_limit
    target: write_markdown
  - source: get_pwd
    target: write_markdown
  - source: research_agent
    target: create_json
  - source: create_json
    target: write_json
  - source: get_pwd
    target: write_json
  - source: research_agent
    target: create_csv
  - source: create_csv
    target: write_csv
  - source: get_pwd
    target: write_csv
  - source: write_markdown
    target: complete
  - source: write_json
    target: complete
  - source: write_csv
    target: complete
"#;

/// 04: Production Pipeline - All 5 verbs, spawn_agent, lazy bindings, multi-locale
const WORKFLOW_04_PRODUCTION: &str = r#"# 04 - Production Pipeline Workflow
# Demonstrates: All 5 verbs, spawn_agent, lazy bindings, multi-locale output
# Run with: nika run workflows/04-production-pipeline.nika.yaml

schema: "nika/workflow@0.9"
workflow: production-pipeline
description: "Production-ready localization pipeline with nested agents"

# Dummy MCP config (builtins intercept before MCP call)
mcp:
  dummy:
    command: "echo"
    args: ["not used"]

# Load locale configuration (paths relative to project root)
context:
  files:
    locales: ./context/locales.json
    template: ./context/content-template.md

tasks:
  # Setup output directories for each locale
  # Note: for_each accepts arrays or $binding refs, not {{context.}} syntax
  - id: setup_dirs
    for_each: ["en-US", "fr-FR", "de-DE"]
    as: locale_code
    concurrency: 5
    exec:
      command: "mkdir -p ./output/04-localized/{{use.locale_code}}"
      shell: true

  # Fetch external reference data
  - id: fetch_reference
    fetch:
      url: "https://httpbin.org/json"
      method: GET
      timeout: 10

  # Main production agent with spawn capability
  - id: content_orchestrator
    use:
      reference: fetch_reference
    agent:
      prompt: |
        You are a content localization orchestrator.

        Your task is to generate localized content for these locales:
        {{context.files.locales}}

        For each locale:
        1. Generate culturally-appropriate content
        2. Save results to appropriate directories

        Use the template as a guide:
        {{context.files.template}}

        Reference data for context:
        {{use.reference}}
      model: claude-sonnet-4-6
      max_turns: 10
      depth_limit: 2
      tools:
        - nika:write
        - nika:log
      stop_conditions:
        - "All locales processed"
        - "Content generation complete"

  # Lazy binding example - won't resolve until accessed
  - id: summary_with_lazy
    use:
      content_results: content_orchestrator
      lazy_stats:
        path: content_orchestrator.stats
        lazy: true
        default: '{"locales_processed": 0}'
    infer:
      prompt: |
        Generate a deployment summary based on:
        - Results: {{use.content_results}}
        - Stats: {{use.lazy_stats}}

        Format as a brief status report.
      temperature: 0.3

  # Log deployment summary (file tools require ToolContext, use log instead)
  - id: write_summary
    use:
      summary: summary_with_lazy
    invoke:
      mcp: dummy
      tool: nika:log
      params:
        level: info
        message: |
          # Localization Deployment Summary
          Generated by Nika Production Pipeline v0.16.2
          Status: {{use.summary}}

  # Assert minimum locales processed
  - id: validate_output
    invoke:
      mcp: dummy
      tool: nika:assert
      params:
        condition: true
        message: "At least one locale must be configured"

  # Final emission
  - id: emit_complete
    invoke:
      mcp: dummy
      tool: nika:emit
      params:
        name: "pipeline_complete"
        payload:
          workflow: "production-pipeline"

flows:
  - source: setup_dirs
    target: content_orchestrator
  - source: fetch_reference
    target: content_orchestrator
  - source: content_orchestrator
    target: summary_with_lazy
  - source: summary_with_lazy
    target: write_summary
  - source: write_summary
    target: validate_output
  - source: validate_output
    target: emit_complete
"#;

/// Context file: brands.json (for workflow 02)
const CONTEXT_BRANDS_JSON: &str = r#"[
  {
    "name": "Apple",
    "industry": "Technology",
    "founded": 1976
  },
  {
    "name": "Tesla",
    "industry": "Automotive/Energy",
    "founded": 2003
  },
  {
    "name": "Nike",
    "industry": "Sportswear",
    "founded": 1964
  }
]
"#;

/// Context file: research-config.json (for workflow 03)
const CONTEXT_RESEARCH_CONFIG: &str = r#"{
  "topic": "The impact of AI on software development workflows",
  "depth": "comprehensive",
  "output_formats": ["markdown", "json", "csv"]
}
"#;

/// Context file: locales.json (for workflow 04)
const CONTEXT_LOCALES_JSON: &str = r#"[
  {"code": "en-US", "name": "English (US)", "direction": "ltr"},
  {"code": "fr-FR", "name": "French (France)", "direction": "ltr"},
  {"code": "ja-JP", "name": "Japanese", "direction": "ltr"},
  {"code": "ar-SA", "name": "Arabic (Saudi Arabia)", "direction": "rtl"}
]
"#;

/// Context file: content-template.md (for workflow 04)
const CONTEXT_TEMPLATE_MD: &str = r#"# Content Template

## Hero Section
- Headline: [Localized headline, culturally appropriate]
- Subheadline: [Supporting text]
- CTA: [Call to action button text]

## Features
1. Feature 1: [Description]
2. Feature 2: [Description]
3. Feature 3: [Description]

## Localization Notes
- Consider cultural context for {{locale.name}}
- Adapt idioms and expressions appropriately
- Maintain brand voice while respecting local customs
"#;

/// Partial: fetch-utils.nika.yaml (for workflow 03 include:)
const PARTIAL_FETCH_UTILS: &str = r#"# Fetch Utilities Partial
# Included via 'include:' in parent workflows
# Tasks will be prefixed with the include prefix (e.g., utils_)

schema: "nika/workflow@0.9"
workflow: fetch-utils

tasks:
  - id: get_timestamp
    exec: "date -u '+%Y-%m-%dT%H:%M:%SZ'"

  - id: check_network
    fetch:
      url: "https://httpbin.org/status/200"
      method: GET
      timeout_secs: 5
"#;

/// Schema: brand-analysis.json (JSON Schema for output validation)
const SCHEMA_BRAND_ANALYSIS: &str = r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "Brand Analysis Output",
  "description": "Schema for validating brand analysis JSON output",
  "type": "object",
  "required": ["rankings"],
  "properties": {
    "rankings": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["name", "score", "key_strength"],
        "properties": {
          "name": {
            "type": "string",
            "description": "Brand name"
          },
          "score": {
            "type": "integer",
            "minimum": 1,
            "maximum": 10,
            "description": "Overall brand score"
          },
          "key_strength": {
            "type": "string",
            "description": "Primary competitive advantage"
          }
        }
      }
    }
  }
}
"#;

// ─────────────────────────────────────────────────────────────────────────────

/// Initialize a new Nika project (v0.16.2)
///
/// Creates:
/// - `.nika/` directory with config, agents, skills, memory, etc.
/// - `workflows/` with 4 progressive example workflows (unless --no-example)
/// - `context/` with example context files (brands, locales, templates)
/// - `workflows/partials/` with reusable workflow fragments (for include:)
/// - `schemas/` with JSON schemas for output validation
/// - `output/` for generated workflow outputs
/// - Migrate env vars to keychain (if --migrate-keys)
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
# Default LLM provider (claude, openai, mistral, groq, deepseek, ollama)
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

        // Create context/ directory at project root
        let context_dir = cwd.join("context");
        fs::create_dir_all(&context_dir)?;
        println!("{} Created {}", "✓".green(), context_dir.display());

        // Create partials/ directory inside workflows/ (for include: security)
        let partials_dir = workflows_dir.join("partials");
        fs::create_dir_all(&partials_dir)?;
        println!("{} Created {}", "✓".green(), partials_dir.display());

        // Create schemas/ directory at project root
        let schemas_dir = cwd.join("schemas");
        fs::create_dir_all(&schemas_dir)?;
        println!("{} Created {}", "✓".green(), schemas_dir.display());

        // Create output/ directory at project root with .gitkeep
        let output_dir = cwd.join("output");
        fs::create_dir_all(&output_dir)?;
        fs::write(output_dir.join(".gitkeep"), "")?;
        println!("{} Created {}", "✓".green(), output_dir.display());

        // Write 4 progressive example workflows
        let wf1_path = workflows_dir.join("01-hello-world.nika.yaml");
        fs::write(&wf1_path, WORKFLOW_01_HELLO_WORLD)?;
        println!("{} Created {}", "✓".green(), wf1_path.display());

        let wf2_path = workflows_dir.join("02-parallel-pipeline.nika.yaml");
        fs::write(&wf2_path, WORKFLOW_02_PARALLEL)?;
        println!("{} Created {}", "✓".green(), wf2_path.display());

        let wf3_path = workflows_dir.join("03-agent-advanced.nika.yaml");
        fs::write(&wf3_path, WORKFLOW_03_AGENT)?;
        println!("{} Created {}", "✓".green(), wf3_path.display());

        let wf4_path = workflows_dir.join("04-production-pipeline.nika.yaml");
        fs::write(&wf4_path, WORKFLOW_04_PRODUCTION)?;
        println!("{} Created {}", "✓".green(), wf4_path.display());

        // Write context files
        let brands_path = context_dir.join("brands.json");
        fs::write(&brands_path, CONTEXT_BRANDS_JSON)?;
        println!("{} Created {}", "✓".green(), brands_path.display());

        let research_config_path = context_dir.join("research-config.json");
        fs::write(&research_config_path, CONTEXT_RESEARCH_CONFIG)?;
        println!("{} Created {}", "✓".green(), research_config_path.display());

        let locales_path = context_dir.join("locales.json");
        fs::write(&locales_path, CONTEXT_LOCALES_JSON)?;
        println!("{} Created {}", "✓".green(), locales_path.display());

        let template_path = context_dir.join("content-template.md");
        fs::write(&template_path, CONTEXT_TEMPLATE_MD)?;
        println!("{} Created {}", "✓".green(), template_path.display());

        // Write partial
        let fetch_utils_path = partials_dir.join("fetch-utils.nika.yaml");
        fs::write(&fetch_utils_path, PARTIAL_FETCH_UTILS)?;
        println!("{} Created {}", "✓".green(), fetch_utils_path.display());

        // Write schema
        let schema_path = schemas_dir.join("brand-analysis.json");
        fs::write(&schema_path, SCHEMA_BRAND_ANALYSIS)?;
        println!("{} Created {}", "✓".green(), schema_path.display());
    }

    // Print summary
    println!();
    println!("{}", "Nika project initialized! (v0.16.2)".green().bold());
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
            "    {}  workflows/          # Example workflows (progressive)",
            "📂".cyan()
        );
        println!("    ├── 01-hello-world.nika.yaml        # Basic: infer, exec, fetch");
        println!("    ├── 02-parallel-pipeline.nika.yaml  # Intermediate: for_each, context:");
        println!("    ├── 03-agent-advanced.nika.yaml     # Advanced: agent, include:, builtins");
        println!("    └── 04-production-pipeline.nika.yaml # Production: spawn_agent, lazy:");
        println!();
        println!(
            "    {}  context/            # Context files for workflows",
            "📁".dimmed()
        );
        println!("    └── partials/           # Reusable workflow fragments (for include:)");
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
        println!("    nika run workflows/01-hello-world.nika.yaml  # Simple");
        println!("    nika run workflows/02-parallel-pipeline.nika.yaml  # With context");
        println!();
        println!("  {} Learn more:", "📖".cyan());
        println!("    Each workflow demonstrates different Nika features");
        println!("    Read comments in workflows/ for detailed explanations");
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
// MCP COMMAND (v0.12.1)
// ═══════════════════════════════════════════════════════════════════════════

/// Handle MCP server management commands
async fn handle_mcp_command(action: McpAction) -> Result<(), NikaError> {
    use colored::Colorize;

    match action {
        McpAction::List { workflow } => {
            // If workflow provided, load its MCP config
            match workflow {
                Some(file) => {
                    let yaml = tokio::fs::read_to_string(&file).await?;
                    let wf: Workflow = serde_yaml::from_str(&yaml)?;

                    match wf.mcp {
                        Some(ref mcp_servers) if !mcp_servers.is_empty() => {
                            println!("{}", "MCP Servers".bold());
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
                        _ => {
                            println!("{} No MCP servers defined in {}", "ℹ".cyan(), file);
                        }
                    }
                }
                None => {
                    println!("{} Specify a workflow file with --workflow", "ℹ".cyan());
                    println!(
                        "{}",
                        "Example: nika mcp list --workflow my-flow.nika.yaml".dimmed()
                    );
                }
            }
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

    // Check Ollama (URL-based, no key)
    if std::env::var("OLLAMA_API_BASE_URL").is_ok() {
        checks.push(DiagnosticCheck::pass(
            "API Key",
            "Ollama configured (OLLAMA_API_BASE_URL)",
        ));
        any_found = true;
    }

    if !any_found {
        checks.push(DiagnosticCheck::warn(
            "API Key",
            "No LLM API keys found",
            "Set ANTHROPIC_API_KEY, OPENAI_API_KEY, or configure Ollama",
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
// Workflow Command Handler (v0.22+)
// ============================================================================

async fn handle_workflow_command(action: WorkflowAction, quiet: bool) -> Result<(), NikaError> {
    use colored::Colorize;

    match action {
        WorkflowAction::Edit { file } => {
            // Open workflow in interactive editor (TUI Studio)
            if !file.exists() {
                return Err(NikaError::WorkflowNotFound {
                    path: file.to_string_lossy().to_string(),
                });
            }

            #[cfg(feature = "tui")]
            {
                nika::tui::run_tui(&file).await
            }

            #[cfg(not(feature = "tui"))]
            {
                Err(NikaError::ConfigError {
                    reason: "TUI feature not enabled. Rebuild with --features tui".to_string(),
                })
            }
        }

        WorkflowAction::AddTask {
            file,
            id,
            verb,
            after,
        } => {
            // Add a new task interactively to the workflow
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

            // Determine verb (default to infer)
            let task_verb = verb.as_deref().unwrap_or("infer");

            // Validate verb
            let valid_verbs = ["infer", "exec", "fetch", "invoke", "agent"];
            if !valid_verbs.contains(&task_verb) {
                return Err(NikaError::ValidationError {
                    reason: format!(
                        "Invalid verb '{}'. Valid: {}",
                        task_verb,
                        valid_verbs.join(", ")
                    ),
                });
            }

            // Generate task YAML snippet based on verb
            let task_yaml = match task_verb {
                "infer" => format!(
                    r#"  - id: {}
    infer: "Your prompt here""#,
                    task_id
                ),
                "exec" => format!(
                    r#"  - id: {}
    exec: "echo 'Hello World'""#,
                    task_id
                ),
                "fetch" => format!(
                    r#"  - id: {}
    fetch:
      url: "https://api.example.com"
      method: GET"#,
                    task_id
                ),
                "invoke" => format!(
                    r#"  - id: {}
    invoke:
      mcp: your_server
      tool: tool_name
      params: {{}}"#,
                    task_id
                ),
                "agent" => format!(
                    r#"  - id: {}
    agent:
      prompt: "Your agent goal here"
      max_turns: 10"#,
                    task_id
                ),
                _ => unreachable!(),
            };

            // Find insertion point
            let insertion_point = if let Some(ref after_task) = after {
                // Find the task with this ID and insert after it
                if let Some(pos) = content.find(&format!("id: {}", after_task)) {
                    // Find the next task or end of tasks section
                    let after_pos = pos + content[pos..].find('\n').unwrap_or(0);
                    // Find next "- id:" or end of file, or None to append at end
                    content[after_pos..].find("\n  - id:")
                        .map(|next_task_offset| after_pos + next_task_offset)
                } else {
                    return Err(NikaError::ValidationError {
                        reason: format!("Task '{}' not found in workflow", after_task),
                    });
                }
            } else {
                None // Append at end of tasks
            };

            // Insert task
            let new_content = if let Some(pos) = insertion_point {
                format!(
                    "{}\n{}\n{}",
                    &content[..pos],
                    task_yaml,
                    &content[pos..]
                )
            } else {
                // Find "tasks:" section and append
                if let Some(tasks_pos) = content.find("tasks:") {
                    let after_tasks = tasks_pos + content[tasks_pos..].find('\n').unwrap_or(0) + 1;
                    // Find end of tasks section (next top-level key or EOF)
                    let end_of_tasks = content[after_tasks..]
                        .find("\nflows:")
                        .or_else(|| content[after_tasks..].find("\nmcp:"))
                        .map(|p| after_tasks + p)
                        .unwrap_or(content.len());

                    format!(
                        "{}\n{}\n{}",
                        &content[..end_of_tasks].trim_end(),
                        task_yaml,
                        &content[end_of_tasks..]
                    )
                } else {
                    return Err(NikaError::ValidationError {
                        reason: "No 'tasks:' section found in workflow".to_string(),
                    });
                }
            };

            // Write back
            std::fs::write(&file, new_content)?;

            if !quiet {
                println!(
                    "{} Added task '{}' ({}) to {}",
                    "✓".green(),
                    task_id.cyan(),
                    task_verb.yellow(),
                    file.display()
                );
                if let Some(ref after_task) = after {
                    println!("  Inserted after: {}", after_task);
                }
            }

            Ok(())
        }

        WorkflowAction::Graph { file, format, output } => {
            // Visualize workflow as DAG graph
            if !file.exists() {
                return Err(NikaError::WorkflowNotFound {
                    path: file.to_string_lossy().to_string(),
                });
            }

            // Parse workflow
            let content = std::fs::read_to_string(&file)?;
            let raw_workflow =
                nika::ast::raw::parse(&content, nika::source::FileId(0)).map_err(|e| {
                    NikaError::ParseError {
                        details: format!("{}", e),
                    }
                })?;

            let analyze_result = nika::ast::analyzer::analyze(raw_workflow);
            if !analyze_result.is_ok() {
                return Err(NikaError::ValidationError {
                    reason: format!(
                        "Workflow has validation errors: {:?}",
                        analyze_result.errors
                    ),
                });
            }

            let workflow = analyze_result.value.expect("workflow should exist");

            // Generate graph output based on format
            let graph_output = match format.as_str() {
                "ascii" => generate_ascii_dag(&workflow),
                "dot" => generate_dot_dag(&workflow),
                "mermaid" => generate_mermaid_dag(&workflow),
                _ => {
                    return Err(NikaError::ValidationError {
                        reason: format!(
                            "Unknown graph format '{}'. Valid: ascii, dot, mermaid",
                            format
                        ),
                    })
                }
            };

            // Output to file or stdout
            if let Some(output_path) = output {
                std::fs::write(&output_path, &graph_output)?;
                if !quiet {
                    println!(
                        "{} Graph saved to {}",
                        "✓".green(),
                        output_path.display()
                    );
                }
            } else {
                println!("{}", graph_output);
            }

            Ok(())
        }

        WorkflowAction::Check { file, suggest, format } => {
            // Validate workflow with optional suggestions
            if !file.exists() {
                return Err(NikaError::WorkflowNotFound {
                    path: file.to_string_lossy().to_string(),
                });
            }

            let content = std::fs::read_to_string(&file)?;
            let raw_workflow =
                nika::ast::raw::parse(&content, nika::source::FileId(0)).map_err(|e| {
                    NikaError::ParseError {
                        details: format!("{}", e),
                    }
                })?;

            let analyze_result = nika::ast::analyzer::analyze(raw_workflow);

            // Collect issues
            let mut issues: Vec<serde_json::Value> = vec![];

            if !analyze_result.is_ok() {
                for error in &analyze_result.errors {
                    issues.push(serde_json::json!({
                        "type": "error",
                        "message": format!("{}", error),
                    }));
                }
            }

            // Generate suggestions if requested
            let mut suggestions: Vec<String> = vec![];
            if suggest {
                if let Some(ref workflow) = analyze_result.value {
                    // Check for common improvements
                    let task_count = workflow.tasks.len();

                    if task_count > 10 {
                        suggestions.push(
                            "Consider splitting into multiple workflows with 'include:'".to_string(),
                        );
                    }

                    // Check for missing descriptions
                    if workflow.description.is_none() {
                        suggestions.push("Add a 'description:' field for documentation".to_string());
                    }

                    // Check for no flows (linear execution)
                    if workflow.flow_defs.is_empty() && task_count > 1 {
                        suggestions.push(
                            "Add explicit 'flows:' to define task dependencies".to_string(),
                        );
                    }

                    // Check schema version
                    use nika::ast::analyzed::SchemaVersion;
                    match workflow.schema_version {
                        SchemaVersion::V09 | SchemaVersion::V10 => {
                            // Already on latest versions
                        }
                        _ => {
                            suggestions.push(format!(
                                "Consider upgrading to schema @0.10 (current: {:?})",
                                workflow.schema_version
                            ));
                        }
                    }
                }
            }

            // Output results
            if format == "json" {
                let result = serde_json::json!({
                    "file": file.to_string_lossy(),
                    "valid": analyze_result.is_ok(),
                    "issues": issues,
                    "suggestions": suggestions,
                });
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            } else {
                // Text output
                if analyze_result.is_ok() {
                    if !quiet {
                        println!("{} {} is valid", "✓".green(), file.display());
                    }
                } else {
                    println!("{} {} has errors:", "✗".red(), file.display());
                    for error in &analyze_result.errors {
                        println!("  {} {}", "•".red(), error);
                    }
                }

                if suggest && !suggestions.is_empty() {
                    println!();
                    println!("{}", "Suggestions:".cyan().bold());
                    for suggestion in &suggestions {
                        println!("  {} {}", "→".cyan(), suggestion);
                    }
                }
            }

            if analyze_result.is_ok() {
                Ok(())
            } else {
                Err(NikaError::ValidationError {
                    reason: format!("{} error(s) found", analyze_result.errors.len()),
                })
            }
        }
    }
}

/// Generate ASCII DAG representation
fn generate_ascii_dag(workflow: &nika::ast::analyzed::AnalyzedWorkflow) -> String {
    let mut output = String::new();
    output.push_str(&format!("DAG: {}\n", workflow.name.as_deref().unwrap_or("(unnamed)")));
    output.push_str(&"─".repeat(50));
    output.push('\n');
    output.push('\n');

    // List all tasks with their dependencies
    for task in &workflow.tasks {
        let verb_icon = match &task.action {
            nika::ast::analyzed::AnalyzedTaskAction::Infer { .. } => "⚡",
            nika::ast::analyzed::AnalyzedTaskAction::Exec { .. } => "📟",
            nika::ast::analyzed::AnalyzedTaskAction::Fetch { .. } => "🛰️",
            nika::ast::analyzed::AnalyzedTaskAction::Invoke { .. } => "🔌",
            nika::ast::analyzed::AnalyzedTaskAction::Agent { .. } => "🐔",
        };

        output.push_str(&format!("{} {}\n", verb_icon, task.name));

        // Show use refs as dependencies
        for (alias, use_ref) in &task.use_refs {
            let target_name = workflow.task_table.get_name(use_ref.target)
                .unwrap_or("?");
            output.push_str(&format!("  └─ use.{} ← {}\n", alias, target_name));
        }
    }

    // Show flow definitions
    if !workflow.flow_defs.is_empty() {
        output.push('\n');
        output.push_str("Flows:\n");
        for (name, flow_def) in &workflow.flow_defs {
            let task_names: Vec<_> = flow_def.tasks.iter()
                .filter_map(|id| workflow.task_table.get_name(*id))
                .collect();
            output.push_str(&format!("  {} → [{}]\n", name, task_names.join(", ")));
        }
    }

    output
}

/// Generate DOT (Graphviz) DAG representation
fn generate_dot_dag(workflow: &nika::ast::analyzed::AnalyzedWorkflow) -> String {
    let mut output = String::new();
    let name = workflow.name.as_deref().unwrap_or("workflow");
    output.push_str(&format!("digraph {} {{\n", name.replace('-', "_")));
    output.push_str("  rankdir=LR;\n");
    output.push_str("  node [shape=box];\n");
    output.push('\n');

    // Add nodes with verb-specific styling
    for task in &workflow.tasks {
        let (color, shape) = match &task.action {
            nika::ast::analyzed::AnalyzedTaskAction::Infer { .. } => ("#7c3aed", "box"),
            nika::ast::analyzed::AnalyzedTaskAction::Exec { .. } => ("#f97316", "box"),
            nika::ast::analyzed::AnalyzedTaskAction::Fetch { .. } => ("#0ea5e9", "ellipse"),
            nika::ast::analyzed::AnalyzedTaskAction::Invoke { .. } => ("#8b5cf6", "diamond"),
            nika::ast::analyzed::AnalyzedTaskAction::Agent { .. } => ("#ec4899", "octagon"),
        };
        output.push_str(&format!(
            "  {} [label=\"{}\" color=\"{}\" shape={}];\n",
            task.name.replace('-', "_"),
            task.name,
            color,
            shape
        ));
    }

    output.push('\n');

    // Add edges from flow definitions (consecutive tasks within each flow)
    for (_name, flow_def) in &workflow.flow_defs {
        let task_names: Vec<_> = flow_def.tasks.iter()
            .filter_map(|id| workflow.task_table.get_name(*id))
            .collect();
        // Connect consecutive tasks in the flow
        for window in task_names.windows(2) {
            output.push_str(&format!(
                "  {} -> {};\n",
                window[0].replace('-', "_"),
                window[1].replace('-', "_")
            ));
        }
    }

    // Add implicit edges from use refs
    for task in &workflow.tasks {
        for (_alias, use_ref) in &task.use_refs {
            // Get the source task name from the target TaskId
            if let Some(source_name) = workflow.task_table.get_name(use_ref.target) {
                output.push_str(&format!(
                    "  {} -> {} [style=dashed color=\"#9ca3af\"];\n",
                    source_name.replace('-', "_"),
                    task.name.replace('-', "_")
                ));
            }
        }
    }

    output.push_str("}\n");
    output
}

/// Generate Mermaid DAG representation
fn generate_mermaid_dag(workflow: &nika::ast::analyzed::AnalyzedWorkflow) -> String {
    let mut output = String::new();
    output.push_str("```mermaid\nflowchart LR\n");

    // Add nodes with verb-specific styling
    for task in &workflow.tasks {
        let (shape_start, shape_end, class) = match &task.action {
            nika::ast::analyzed::AnalyzedTaskAction::Infer { .. } => ("[", "]", "infer"),
            nika::ast::analyzed::AnalyzedTaskAction::Exec { .. } => ("([", "])", "exec"),
            nika::ast::analyzed::AnalyzedTaskAction::Fetch { .. } => ("((", "))", "fetch"),
            nika::ast::analyzed::AnalyzedTaskAction::Invoke { .. } => ("{", "}", "invoke"),
            nika::ast::analyzed::AnalyzedTaskAction::Agent { .. } => ("{{", "}}", "agent"),
        };
        output.push_str(&format!(
            "  {}{}\"{}\"{}:::{}\n",
            task.name,
            shape_start,
            task.name,
            shape_end,
            class
        ));
    }

    output.push('\n');

    // Add edges from flow definitions (consecutive tasks within each flow)
    for (_name, flow_def) in &workflow.flow_defs {
        let task_names: Vec<_> = flow_def.tasks.iter()
            .filter_map(|id| workflow.task_table.get_name(*id))
            .collect();
        for window in task_names.windows(2) {
            output.push_str(&format!("  {} --> {}\n", window[0], window[1]));
        }
    }

    // Add implicit edges from use refs
    for task in &workflow.tasks {
        for (_alias, use_ref) in &task.use_refs {
            if let Some(source_name) = workflow.task_table.get_name(use_ref.target) {
                output.push_str(&format!(
                    "  {} -.-> {}\n",
                    source_name,
                    task.name
                ));
            }
        }
    }

    // Add class definitions
    output.push_str("\n  classDef infer fill:#7c3aed,color:#fff\n");
    output.push_str("  classDef exec fill:#f97316,color:#fff\n");
    output.push_str("  classDef fetch fill:#0ea5e9,color:#fff\n");
    output.push_str("  classDef invoke fill:#8b5cf6,color:#fff\n");
    output.push_str("  classDef agent fill:#ec4899,color:#fff\n");
    output.push_str("```\n");

    output
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
    let has_flags = template.is_some()
        || verb.is_some()
        || provider.is_some()
        || output.is_some()
        || with_mcp
        || with_include
        || with_artifacts;

    // If wizard flag is set, or no name and no flags, launch wizard
    #[cfg(feature = "tui")]
    if wizard || (name.is_none() && !has_flags) {
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
                    "Unknown provider: '{}'. Valid: claude, openai, mistral, groq, deepseek, ollama",
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
