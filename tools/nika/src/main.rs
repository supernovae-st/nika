//! Nika CLI - DAG workflow runner

mod cli;

use clap::{ArgAction, CommandFactory, Parser, Subcommand, ValueEnum};
use colored::Colorize;
use std::path::{Path, PathBuf};

use nika::ast::output::SchemaRef;
use nika::ast::schema_validator::WorkflowSchemaValidator;
use nika::ast::{expand_includes, parse_workflow, TaskAction};
use nika::dag::{validate_bindings, Dag};
use nika::error::NikaError;
use nika::mcp::validation::{McpValidator, ValidationConfig};
use nika::mcp::{McpClient, McpConfig};
use nika::registry::resolver;
use nika::runtime::Runner;

// ═══════════════════════════════════════════════════════════════════════════
// HELP TEXT
// ═══════════════════════════════════════════════════════════════════════════

const LONG_ABOUT: &str = r#"Nika - DAG workflow runner for AI tasks with MCP integration

Execute YAML-defined workflows using 5 semantic verbs:
  infer:   LLM text generation (Claude, OpenAI, Mistral, Groq, DeepSeek, Gemini, xAI, Native)
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
    nika ui                       TUI (Studio view by default)
    nika ui --view=chat           TUI Chat view
    nika ui --view=runner         TUI Runner view
    nika chat                     TUI Chat (shortcut)
    nika studio [file]            TUI Studio (shortcut)

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
    nika mcp list -w workflow.yaml List servers in workflow
    nika mcp test workflow.yaml s  Test server connection
    nika mcp tools workflow.yaml s List available tools

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
    GEMINI_API_KEY                Google Gemini
    XAI_API_KEY                   xAI (Grok)
    NIKA_MODEL_PATH               Native inference model path

TUI VIEWS (in nika ui):
    [1/s] Studio     File browser + YAML editor + DAG preview
    [2/r] Runner     Real-time execution monitoring
    [3/c] Chat       AI agent conversation
    [4/,] Settings   Provider config, theme, preferences

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
        action: cli::trace::TraceAction,
    },

    /// Manage LLM provider API keys
    #[cfg(feature = "tui")]
    Provider {
        #[command(subcommand)]
        action: cli::provider::ProviderAction,
    },

    /// Manage MCP server connections
    Mcp {
        #[command(subcommand)]
        action: cli::mcp::McpAction,
    },

    /// Manage local LLM models
    ///
    /// Download, list, and manage GGUF models for native inference.
    /// Models are stored in ~/.nika/models/
    #[cfg(feature = "native-inference")]
    #[command(visible_alias = "m")]
    Model {
        #[command(subcommand)]
        action: cli::model::ModelAction,
    },

    /// Manage installed packages (workflows, skills, schemas)
    ///
    /// List, add, remove, and install packages from the SuperNovae registry.
    /// Packages are stored in ~/.nika/packages/
    #[command(visible_alias = "p")]
    Pkg {
        #[command(subcommand)]
        action: cli::pkg::PkgAction,
    },

    /// Generate shell completions
    Completion {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },

    /// Manage Nika configuration
    Config {
        #[command(subcommand)]
        action: cli::config::ConfigAction,
    },

    /// Manage schema versions and migrations
    Schema {
        #[command(subcommand)]
        action: cli::schema::SchemaAction,
    },

    /// Check system health and diagnose issues
    #[command(visible_alias = "d")]
    Doctor {
        /// Run all checks including slow ones (MCP connectivity)
        #[arg(long)]
        full: bool,

        /// Output format: text, json
        #[arg(long, default_value = "text")]
        format: String,
    },

    /// Create a new workflow from template or wizard
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
        action: cli::workflow::WorkflowAction,
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
            let result = run_workflow(&file.display().to_string(), None, None, cli.quiet).await;
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
        None => {
            use clap::CommandFactory;
            if let Err(e) = Cli::command().print_help() {
                eprintln!("Failed to print help: {}", e);
                std::process::exit(1);
            }
            Ok(())
        }

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
        }) => run_workflow(&file, provider, model, quiet).await,

        Some(Commands::Check { file, strict }) => {
            if strict {
                validate_workflow_strict(&file).await
            } else {
                validate_workflow(&file, quiet).await
            }
        }

        Some(Commands::Init {
            permission,
            no_example,
            migrate_keys,
        }) => cli::init::init_project(&permission, no_example, migrate_keys),

        Some(Commands::Trace { action }) => cli::trace::handle_trace_command(action),

        #[cfg(feature = "tui")]
        Some(Commands::Provider { action }) => cli::provider::handle_provider_command(action).await,

        Some(Commands::Mcp { action }) => cli::mcp::handle_mcp_command(action).await,

        #[cfg(feature = "native-inference")]
        Some(Commands::Model { action }) => cli::model::handle_model_command(action, quiet).await,

        Some(Commands::Pkg { action }) => cli::pkg::handle_pkg_command(action).await,

        Some(Commands::Completion { shell }) => {
            clap_complete::generate(shell, &mut Cli::command(), "nika", &mut std::io::stdout());
            Ok(())
        }

        Some(Commands::Config { action }) => cli::config::handle_config_command(action, quiet),

        Some(Commands::Schema { action }) => cli::schema::handle_schema_command(action, quiet),

        Some(Commands::Doctor { full, format }) => {
            cli::doctor::handle_doctor_command(full, &format, quiet).await
        }

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
        }) => cli::new_cmd::handle_new_command(
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

    handle_result(result);
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
/// 1. If starts with '@' (e.g., @workflows/seo-audit) -> Package resolution from ~/.nika/packages/
/// 2. If simple name without path/extension -> Search in .nika/workflows/{name}.nika.yaml
/// 3. Otherwise -> Use as-is (filesystem path)
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
            .join(format!("{}.nika.yaml", reference));

        if local_path.exists() {
            return Ok(local_path);
        }

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
    quiet: bool,
) -> Result<(), NikaError> {
    let resolved_path = resolve_workflow_path(file).await?;

    let yaml = tokio::fs::read_to_string(&resolved_path).await?;

    let validator = WorkflowSchemaValidator::new()?;
    validator.validate_yaml(&yaml)?;

    let workflow = parse_workflow(&yaml)?;

    let base_path = resolved_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let workflow = expand_includes(workflow, base_path)?;

    // Bridge: convert old Workflow back to AnalyzedWorkflow for Runner
    let mut workflow = nika::ast::unlower(workflow)?;

    if let Some(p) = provider_override {
        workflow.provider = Some(p);
    }
    if let Some(m) = model_override {
        workflow.model = Some(m);
    }

    if !quiet {
        nika::display::print_workflow_header(
            workflow.name.as_deref(),
            workflow.provider.as_deref().unwrap_or("(auto)"),
            workflow.model.as_deref().unwrap_or("(default)"),
            workflow.tasks.len(),
        );

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

    let mut runner = Runner::new(workflow)?;
    if quiet {
        runner = runner.quiet();
    }
    let output = runner.run().await?;

    if !quiet && !output.is_empty() {
        println!("{}", "Output:".cyan().bold());
        println!("{}", output);
    }

    Ok(())
}

async fn validate_workflow(file: &str, quiet: bool) -> Result<(), NikaError> {
    let resolved_path = resolve_workflow_path(file).await?;

    let yaml = tokio::fs::read_to_string(&resolved_path).await?;

    let validator = WorkflowSchemaValidator::new()?;
    validator.validate_yaml(&yaml)?;

    let workflow = parse_workflow(&yaml)?;

    let base_path = resolved_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let workflow = expand_includes(workflow, base_path)?;

    let flow_graph = Dag::from_workflow(&workflow)?;
    flow_graph.detect_cycles()?;
    validate_bindings(&workflow, &flow_graph)?;

    // Phase 4: Validate structured output schema files
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
            if let SchemaRef::File(ref path) = spec.schema {
                validate_schema_file(&task.id, path, base_path).await?;
                schema_count += 1;
            }
        }
    }

    if !quiet {
        println!("{} Workflow '{}' is valid", "✓".green(), file);
        println!("  Provider: {}", workflow.provider);
        println!(
            "  Model: {}",
            workflow.model.as_deref().unwrap_or("(default)")
        );
        println!("  Tasks: {}", workflow.tasks.len());
        println!("  Edges: {}", workflow.flow_count());
        if schema_count > 0 {
            println!("  Schemas: {} validated", schema_count);
        }

        // Show DAG visualization for multi-task workflows
        if workflow.tasks.len() > 1 {
            use nika::display::{DagTask, DagTaskStatus, render_dag};
            use std::collections::HashMap;

            let dag_tasks: Vec<DagTask> = workflow.tasks.iter().map(|t| {
                DagTask {
                    id: t.id.clone(),
                    verb: t.action.verb_name().to_string(),
                    status: DagTaskStatus::Pending,
                }
            }).collect();

            let mut deps_map: HashMap<String, Vec<String>> = HashMap::new();
            for task in &workflow.tasks {
                if let Some(ref task_deps) = task.depends_on {
                    deps_map.insert(task.id.clone(), task_deps.clone());
                }
            }

            render_dag(&dag_tasks, &deps_map);
        }
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
                path: format!("{}: {}", path, e),
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
    let resolved_path = resolve_workflow_path(file).await?;

    let yaml = tokio::fs::read_to_string(&resolved_path).await?;

    let schema_validator = WorkflowSchemaValidator::new()?;
    schema_validator.validate_yaml(&yaml)?;

    let workflow = parse_workflow(&yaml)?;

    let base_path = resolved_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let workflow = expand_includes(workflow, base_path)?;

    let flow_graph = Dag::from_workflow(&workflow)?;
    flow_graph.detect_cycles()?;
    validate_bindings(&workflow, &flow_graph)?;

    // Validate structured output schema files
    for task in &workflow.tasks {
        if let Some(ref output) = task.output {
            if let Some(SchemaRef::File(ref path)) = output.schema {
                validate_schema_file(&task.id, path, base_path).await?;
            }
        }
        if let Some(ref spec) = task.structured {
            if let SchemaRef::File(ref path) = spec.schema {
                validate_schema_file(&task.id, path, base_path).await?;
            }
        }
    }

    // Phase 3: MCP parameter validation (strict mode)
    println!(
        "{} Strict mode: validating invoke parameters...",
        "→".cyan()
    );

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
        let mcp_validator = McpValidator::new(ValidationConfig::default());

        let mcp_servers: std::collections::HashSet<&str> = invoke_tasks
            .iter()
            .filter_map(|(_, p)| p.mcp.as_deref())
            .collect();

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

            println!(
                "  {} Connecting to MCP server '{}'...",
                "→".cyan(),
                server_name
            );

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
            println!("    {} Found {} tools", "✓".green(), tools.len());

            mcp_validator.cache().populate(server_name, &tools)?;
        }

        let mut all_valid = true;
        for (task_id, params) in &invoke_tasks {
            let tool_name = params.tool.as_deref().unwrap_or("(resource read)");

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
    println!("  Edges: {}", workflow.flow_count());

    Ok(())
}
