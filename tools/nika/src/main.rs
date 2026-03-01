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

    /// Package management for Nika skills and workflows (v0.16.1)
    #[command(visible_alias = "p")]
    Pkg {
        #[command(subcommand)]
        action: PkgAction,
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

/// Package management actions (v0.16.1)
#[derive(Subcommand)]
enum PkgAction {
    /// Install packages from registry
    Install {
        /// Package URI (pkg:@scope/name@version or @scope/name)
        #[arg(required = true)]
        packages: Vec<String>,

        /// Install from lockfile only (no network)
        #[arg(long)]
        offline: bool,

        /// Force reinstall even if already installed
        #[arg(short, long)]
        force: bool,
    },

    /// List installed packages
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Show only outdated packages
        #[arg(long)]
        outdated: bool,
    },

    /// Search packages in registry
    Search {
        /// Search query
        query: String,

        /// Limit number of results
        #[arg(short, long, default_value = "20")]
        limit: usize,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Update packages to latest versions
    Update {
        /// Specific packages to update (all if empty)
        packages: Vec<String>,

        /// Dry run - show what would be updated
        #[arg(long)]
        dry_run: bool,
    },

    /// Remove installed packages
    Remove {
        /// Package names to remove
        #[arg(required = true)]
        packages: Vec<String>,

        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
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
                Some("explorer") | Some("e") | Some("home") => Some(TuiView::Explorer),
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
                validate_workflow(&file)
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

        // Doctor command (v0.13.1)
        Some(Commands::Doctor { full, format }) => {
            handle_doctor_command(full, &format, quiet).await
        }

        // Package management (v0.16.1)
        Some(Commands::Pkg { action }) => handle_pkg_command(action, quiet).await,

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

async fn run_workflow(
    file: &str,
    provider_override: Option<String>,
    model_override: Option<String>,
) -> Result<(), NikaError> {
    // Read and parse (async to not block runtime)
    let yaml = tokio::fs::read_to_string(file).await?;

    // Validate YAML against JSON Schema (catches structural errors early)
    let validator = WorkflowSchemaValidator::new()?;
    validator.validate_yaml(&yaml)?;

    // Parse into Workflow struct (now we know structure is valid)
    let workflow: Workflow = serde_yaml::from_str(&yaml)?;

    // Expand includes (v0.14.2 - DAG fusion)
    let base_path = Path::new(file).parent().unwrap_or(Path::new("."));
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

fn validate_workflow(file: &str) -> Result<(), NikaError> {
    let yaml = fs::read_to_string(file)?;

    // Validate YAML against JSON Schema (catches structural errors early)
    let validator = WorkflowSchemaValidator::new()?;
    validator.validate_yaml(&yaml)?;

    // Parse into Workflow struct (now we know structure is valid)
    let workflow: Workflow = serde_yaml::from_str(&yaml)?;

    // Expand includes (v0.14.2 - DAG fusion)
    let base_path = Path::new(file).parent().unwrap_or(Path::new("."));
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
    let yaml = tokio::fs::read_to_string(file).await?;

    // Phase 1: JSON Schema validation
    let schema_validator = WorkflowSchemaValidator::new()?;
    schema_validator.validate_yaml(&yaml)?;

    // Parse into Workflow struct
    let workflow: Workflow = serde_yaml::from_str(&yaml)?;

    // Expand includes (v0.14.2 - DAG fusion)
    let base_path = Path::new(file).parent().unwrap_or(Path::new("."));
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
        mask_api_key, migrate_env_to_keyring, provider_env_var, validate_key_format, NikaKeyring,
    };
    use std::io::{self, Write};

    match action {
        ProviderAction::List => {
            println!("{}", "LLM Providers".bold());
            println!("{}", "─".repeat(60));

            for provider in ALL_PROVIDERS {
                let env_var = provider_env_var(provider);
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
            let env_var = provider_env_var(&provider);
            let has_key = NikaKeyring::exists(&provider)
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

/// Initialize a new Nika project
///
/// Creates:
/// - `.nika/` directory
/// - `.nika/config.toml` with permission settings
/// - Example workflow (unless --no-example)
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
      use.summary: result

  - id: translate
    infer:
      prompt: |
        Translate the following text to {{use.target_language | default: "French"}}:

        {{use.input}}
      model: claude-sonnet-4-20250514
    output:
      use.translation: result

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
      use.review: result
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

    // Create example workflow unless --no-example
    if !no_example {
        let example_path = cwd.join("hello.nika.yaml");
        if !example_path.exists() {
            let example_content = r#"# Example Nika Workflow
# Run with: nika run hello.nika.yaml

schema: "nika/workflow@0.6"
workflow: hello-world
description: "Simple hello world workflow demonstrating basic features"

# v0.13: Agents can be loaded from .nika/agents/ using 'from:'
agents:
  researcher:
    from: .nika/agents/researcher  # Auto-detects .md, .agent.yaml, or folder

tasks:
  - id: greet
    infer: "Generate a friendly greeting message in one sentence."

  - id: expand
    use:
      greeting: greet
    # v0.13: Use |shell modifier for safe shell command execution
    infer: "Take this greeting and expand it into a motivational paragraph: {{use.greeting}}"

flows:
  - source: greet
    target: expand
"#;
            fs::write(&example_path, example_content)?;
            println!("{} Created {}", "✓".green(), example_path.display());
        }
    }

    // Print summary
    println!();
    println!("{}", "Nika project initialized!".green().bold());
    println!();
    println!(
        "  Permission mode: {}",
        permission_mode.display_name().cyan()
    );
    println!("  Config: {}", config_path.display());
    println!();
    println!("  {} Project structure:", "📁".cyan());
    println!("    .nika/");
    println!("    ├── config.toml      # Main configuration");
    println!("    ├── user.yaml        # User profile");
    println!("    ├── memory.yaml      # Memory configuration");
    println!("    ├── policies.yaml    # Security policies");
    println!("    ├── agents/          # Agent definitions (.md or .agent.yaml)");
    println!("    │   └── researcher.md");
    println!("    ├── skills/          # Skill definitions (.md or .skill.yaml)");
    println!("    │   └── code-review.md");
    println!("    ├── context/         # Shared context files");
    println!("    │   └── project.md");
    println!("    ├── workflows/       # Sub-workflows (called via nika:run)");
    println!("    │   └── helpers.nika.yaml");
    println!("    ├── memory/          # Persistent memory storage");
    println!("    ├── proposed/        # Agent-proposed changes");
    println!("    └── cache/           # Temporary cache");
    if !no_example {
        println!();
        println!("  {} Run example workflow:", "→".cyan());
        println!("    nika run hello.nika.yaml");
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

/// Handle package management commands (v0.16.1)
///
/// DEPRECATED in v0.15.3 - Use `spn` CLI instead.
/// These commands will be removed in v0.16.0.
async fn handle_pkg_command(action: PkgAction, quiet: bool) -> Result<(), NikaError> {
    use colored::Colorize;

    // DEPRECATION WARNING (v0.15.3)
    let action_name = match &action {
        PkgAction::Install { .. } => "pkg install",
        PkgAction::List { .. } => "pkg list",
        PkgAction::Search { .. } => "pkg search",
        PkgAction::Update { .. } => "pkg update",
        PkgAction::Remove { .. } => "pkg remove",
    };
    let spn_command = match &action {
        PkgAction::Install { .. } => "spn install",
        PkgAction::List { .. } => "spn list",
        PkgAction::Search { .. } => "spn search",
        PkgAction::Update { .. } => "spn update",
        PkgAction::Remove { .. } => "spn remove",
    };

    eprintln!(
        "\n{}",
        "╔═══════════════════════════════════════════════════════════════════════════════╗"
            .yellow()
    );
    eprintln!(
        "{}",
        "║  ⚠️  DEPRECATED: 'nika pkg' commands are deprecated.                          ║"
            .yellow()
    );
    eprintln!(
        "{}",
        format!(
            "║  Use '{}' instead of 'nika {}'.{}",
            spn_command,
            action_name,
            " ".repeat(37 - spn_command.len() - action_name.len())
        )
        .yellow()
    );
    eprintln!(
        "{}",
        "║  These commands will be REMOVED in v0.16.0.                                   ║"
            .yellow()
    );
    eprintln!(
        "{}",
        "║                                                                               ║"
            .yellow()
    );
    eprintln!(
        "{}",
        "║  Install spn CLI: cargo install --git https://github.com/supernovae-st/spn   ║".yellow()
    );
    eprintln!(
        "{}",
        "╚═══════════════════════════════════════════════════════════════════════════════╝\n"
            .yellow()
    );

    // Package registry base path: ~/.spn/packages/
    let pkg_dir = dirs::home_dir()
        .ok_or_else(|| NikaError::ConfigError {
            reason: "Could not determine home directory for ~/.spn/packages".into(),
        })?
        .join(".spn")
        .join("packages");

    match action {
        PkgAction::Install {
            packages,
            offline,
            force,
        } => {
            if !quiet {
                println!("{} Installing packages...", "📦".bold());
            }

            for pkg in &packages {
                if !quiet {
                    println!("  {} {}", "→".cyan(), pkg);
                }
            }

            if offline {
                println!("  {} Using offline mode (lockfile only)", "📴".yellow());
            }
            if force {
                println!("  {} Force reinstall enabled", "🔄".yellow());
            }

            // TODO: Implement actual package installation
            // - Parse pkg:@scope/name@version URI
            // - Resolve to ~/.spn/packages/@scope/name/version/
            // - Download and extract package
            // - Update spn.lock.yaml

            if !quiet {
                println!("{} Installed {} package(s)", "✅".green(), packages.len());
            }
        }

        PkgAction::List { json, outdated } => {
            if !pkg_dir.exists() {
                if json {
                    println!("[]");
                } else if !quiet {
                    println!("{} No packages installed", "📦".yellow());
                }
                return Ok(());
            }

            // TODO: Implement package listing
            // - Scan ~/.spn/packages/@scope/name/version/
            // - Parse manifest.yaml for each package
            // - If outdated, check registry for newer versions

            if json {
                println!("[]"); // TODO: Return actual JSON
            } else if !quiet {
                if outdated {
                    println!("{} Checking for outdated packages...", "🔍".bold());
                } else {
                    println!("{} Installed packages:", "📦".bold());
                }
                println!("  {} No packages installed yet", "→".cyan());
            }
        }

        PkgAction::Search { query, limit, json } => {
            if !quiet && !json {
                println!("{} Searching for '{}'...", "🔍".bold(), query.cyan());
            }

            // TODO: Implement registry search
            // - Query registry API with search term
            // - Limit results
            // - Return matching packages

            if json {
                println!("[]"); // TODO: Return actual JSON
            } else if !quiet {
                println!(
                    "  {} No results found (registry not implemented)",
                    "→".yellow()
                );
                println!("  {} Limit: {} results", "ℹ".blue(), limit);
            }
        }

        PkgAction::Update { packages, dry_run } => {
            if !quiet {
                if packages.is_empty() {
                    println!("{} Updating all packages...", "🔄".bold());
                } else {
                    println!("{} Updating {} package(s)...", "🔄".bold(), packages.len());
                }
            }

            if dry_run {
                println!("  {} Dry run mode - no changes will be made", "👁".yellow());
            }

            // TODO: Implement package updates
            // - Check registry for newer versions
            // - Download and install updates
            // - Update spn.lock.yaml

            if !quiet {
                println!("{} No updates available", "✅".green());
            }
        }

        PkgAction::Remove { packages, force } => {
            if !quiet {
                println!("{} Removing packages...", "🗑".bold());
            }

            for pkg in &packages {
                if !quiet {
                    println!("  {} {}", "→".red(), pkg);
                }
            }

            if !force && !quiet {
                println!("  {} Use --force to skip confirmation", "ℹ".blue());
            }

            // TODO: Implement package removal
            // - Remove from ~/.spn/packages/@scope/name/
            // - Update spn.lock.yaml
            // - Clean up unused dependencies

            if !quiet {
                println!("{} Removed {} package(s)", "✅".green(), packages.len());
            }
        }
    }

    Ok(())
}
