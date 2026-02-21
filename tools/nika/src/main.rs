//! Nika CLI - DAG workflow runner

use clap::{Parser, Subcommand};
use colored::Colorize;
use std::fs;
use std::path::{Path, PathBuf};

// Import from lib modules
use nika::ast::schema_validator::WorkflowSchemaValidator;
use nika::ast::{TaskAction, Workflow};
use nika::dag::{validate_use_wiring, FlowGraph};
use nika::error::{FixSuggestion, NikaError};
use nika::mcp::validation::{McpValidator, ValidationConfig};
use nika::mcp::{McpClient, McpConfig};
use nika::runtime::Runner;
use nika::tools::PermissionMode;
use nika::Event;

// ═══════════════════════════════════════════════════════════════════════════
// HELP TEXT
// ═══════════════════════════════════════════════════════════════════════════

const LONG_ABOUT: &str = r#"Nika - DAG workflow runner for AI tasks with MCP integration

Nika executes YAML-defined workflows using 5 semantic verbs:
  ⚡ infer:   LLM text generation
  📟 exec:    Shell command execution
  🛰️ fetch:   HTTP requests
  🔌 invoke:  MCP tool calls
  🐔 agent:   Multi-turn agentic loops

Launch without arguments to open the interactive TUI."#;

const AFTER_HELP: &str = r#"EXAMPLES:
    nika                              Launch interactive TUI (Home view)
    nika workflow.nika.yaml           Run a workflow directly
    nika chat                         Start conversational AI agent
    nika chat --provider openai       Chat with OpenAI
    nika run my-flow.nika.yaml        Run workflow (explicit)
    nika check my-flow.nika.yaml      Validate workflow syntax
    nika check flow.yaml --strict     Validate with MCP connections
    nika studio my-flow.nika.yaml     Open workflow in editor
    nika init                         Initialize a new project
    nika trace list                   View execution traces

VIEWS (in TUI):
    [a] Chat     Conversational agent interface
    [h] Home     Browse and select workflows
    [s] Studio   Edit YAML with live validation
    [m] Monitor  Real-time execution observer

KEYBOARD:
    Tab          Navigate views
    ?            Show help
    q            Quit"#;

// ═══════════════════════════════════════════════════════════════════════════
// CLI STRUCTURE
// ═══════════════════════════════════════════════════════════════════════════

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

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start interactive chat mode (agent conversation)
    #[cfg(feature = "tui")]
    Chat {
        /// Override provider (claude, openai)
        #[arg(short, long)]
        provider: Option<String>,

        /// Override model
        #[arg(short, long)]
        model: Option<String>,
    },

    /// Open Studio editor for a workflow
    #[cfg(feature = "tui")]
    Studio {
        /// Workflow file to edit (optional)
        workflow: Option<PathBuf>,
    },

    /// Run a workflow file
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

    /// Validate a workflow file
    #[command(alias = "validate")]
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
    },

    /// Manage execution traces
    Trace {
        #[command(subcommand)]
        action: TraceAction,
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

#[tokio::main]
async fn main() {
    // Load .env file (ignore if not present)
    let _ = dotenvy::dotenv();

    let cli = Cli::parse();

    // Determine if we're running TUI (skip tracing to avoid terminal pollution)
    let is_tui = is_tui_mode(&cli);

    if !is_tui {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::from_default_env()
                    .add_directive(tracing::Level::INFO.into()),
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

    // Handle subcommands or default to TUI
    let result = match cli.command {
        // No command = launch TUI (Home view)
        None => {
            #[cfg(feature = "tui")]
            {
                nika::tui::run_tui_standalone().await
            }
            #[cfg(not(feature = "tui"))]
            {
                eprintln!("{} TUI feature not enabled", "Error:".red().bold());
                eprintln!("  {} Use: nika run <workflow.nika.yaml>", "Hint:".yellow());
                std::process::exit(1);
            }
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
        }) => init_project(&permission, no_example),

        // Trace commands
        Some(Commands::Trace { action }) => handle_trace_command(action),

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
    // No command and no file = TUI standalone
    if cli.command.is_none() && cli.file.is_none() {
        return true;
    }

    // Check TUI-related commands
    #[cfg(feature = "tui")]
    if let Some(ref cmd) = cli.command {
        return matches!(
            cmd,
            Commands::Chat { .. } | Commands::Studio { .. } | Commands::Tui { .. }
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
        eprintln!("{} {}", "Error:".red().bold(), e);
        if let Some(suggestion) = e.fix_suggestion() {
            eprintln!("  {} {}", "Fix:".yellow(), suggestion);
        }
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
    let mut workflow: Workflow = serde_yaml::from_str(&yaml)?;

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
    let runner = Runner::new(workflow);
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

    // Validate schema version and task config
    workflow.validate_schema()?;

    // Build flow graph and validate use: bindings (NIKA-080, NIKA-081, NIKA-082)
    let flow_graph = FlowGraph::from_workflow(&workflow);
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

    // Validate schema version and task config
    workflow.validate_schema()?;

    // Phase 2: Binding validation
    let flow_graph = FlowGraph::from_workflow(&workflow);
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
                "yaml" => serde_yaml::to_string(&events)?,
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
// INIT COMMAND
// ═══════════════════════════════════════════════════════════════════════════

/// Initialize a new Nika project
///
/// Creates:
/// - `.nika/` directory
/// - `.nika/config.toml` with permission settings
/// - Example workflow (unless --no-example)
fn init_project(permission: &str, no_example: bool) -> Result<(), NikaError> {
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
# Default LLM provider (claude, openai)
# Can be overridden with NIKA_PROVIDER env var or --provider flag
default = "claude"

# Default model (provider-specific)
# Can be overridden with NIKA_MODEL env var or --model flag
# model = "claude-sonnet-4-20250514"
"#,
        permission_mode
            .display_name()
            .to_lowercase()
            .replace(" (yolo)", "")
    );
    fs::write(&config_path, config_content)?;
    println!("{} Created {}", "✓".green(), config_path.display());

    // Create example workflow unless --no-example
    if !no_example {
        let example_path = cwd.join("hello.nika.yaml");
        if !example_path.exists() {
            let example_content = r#"# Example Nika Workflow
# Run with: nika run hello.nika.yaml

schema: nika/workflow@0.2
workflow: hello-world
description: "Simple hello world workflow demonstrating basic features"

# Default provider and model
provider: claude
# model: claude-sonnet-4-20250514

tasks:
  - id: greet
    infer: "Generate a friendly greeting message in one sentence."
    use.greeting: result

  - id: expand
    infer: "Take this greeting and expand it into a motivational paragraph: {{use.greeting}}"
    use.message: result

flows:
  - from: greet
    to: expand
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
    if !no_example {
        println!();
        println!("  {} Run example workflow:", "→".cyan());
        println!("    nika run hello.nika.yaml");
    }

    Ok(())
}
