//! Nika CLI - DAG workflow runner

mod cli;

use clap::{ArgAction, CommandFactory, Parser, Subcommand, ValueEnum};
use colored::Colorize;
use std::path::{Path, PathBuf};

use nika::ast::output::SchemaRef;
use nika::ast::schema_validator::WorkflowSchemaValidator;
use nika::ast::{expand_includes, parse_analyzed, parse_workflow, TaskAction};
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

CONTENT & TEMPLATES:
    nika new                      Create workflow from template or wizard
    nika new --list               List available templates
    nika showcase list            Browse 115 showcase workflows
    nika showcase extract <name>  Extract showcase to current dir

LEARNING:
    nika init --course            Generate 12-level interactive course
    nika course status            Show constellation progress map
    nika course next              Open next exercise

DIAGNOSTICS:
    nika doctor                   Check system health
    nika doctor --fix             Auto-repair machine setup
    nika trace list               List execution traces
    nika trace show <id>          Show trace details

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
    NIKA_NATIVE_MODEL_PATH        Native inference model path

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

    /// Detail level for run output: max (default), default, min, json
    #[arg(long, default_value = "max", global = true)]
    detail: nika::display::DetailLevel,

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

        /// Permission mode for file tools: deny, plan, accept-edits, yolo
        #[arg(long, default_value = "accept-edits")]
        permission: String,
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

        /// Generate interactive course files (12 levels, 44 exercises)
        #[arg(long)]
        course: bool,

        /// Minimal init (config only, no examples)
        #[arg(long)]
        minimal: bool,
    },

    /// Interactive learning course
    #[command(visible_alias = "learn")]
    Course {
        #[command(subcommand)]
        action: cli::course::CourseAction,
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

    /// Manage media store (list, stats, clean)
    ///
    /// List, inspect, and garbage-collect binary files stored in the
    /// Content-Addressable Store (CAS) at .nika/media/store/
    Media {
        #[command(subcommand)]
        action: cli::media::MediaAction,
    },

    /// Generate shell completions
    #[command(hide = true)]
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
    #[command(hide = true)]
    Schema {
        #[command(subcommand)]
        action: cli::schema::SchemaAction,
    },

    /// Show compiled feature flags and capabilities
    #[command(hide = true)]
    Features,

    /// Browse and extract showcase workflows
    Showcase {
        #[command(subcommand)]
        action: cli::showcase::ShowcaseAction,
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

        /// Auto-fix issues (runs machine setup)
        #[arg(long)]
        fix: bool,
    },

    /// Create a new workflow file
    #[command(visible_alias = "n")]
    New {
        /// Workflow name (used for filename)
        name: Option<String>,

        /// Primary verb (infer, exec, fetch, invoke, agent)
        #[arg(long, value_name = "VERB")]
        verb: Option<String>,

        /// LLM provider (claude, openai, mistral, groq, deepseek, native)
        #[arg(short, long, value_name = "PROVIDER")]
        provider: Option<String>,

        /// Output directory (default: current directory)
        #[arg(short = 'd', long, value_name = "DIR")]
        output_dir: Option<PathBuf>,
    },

    /// Manage workflow files (edit, add-task, graph, check)
    #[command(hide = true, visible_alias = "w")]
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
    print_feature(
        "nika-daemon",
        cfg!(feature = "nika-daemon"),
        "Unified secret management",
    );
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
    if enabled {
        println!("  {} {:20} {}", "✓".green(), name, desc);
    } else {
        println!(
            "  {} {:20} {} {}",
            "✗".red(),
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
    if cfg!(feature = "nika-daemon") {
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
                cli.quiet,
                cli.detail,
                "accept-edits",
            )
            .await;
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
    let detail = cli.detail;

    // Auto-setup: run machine setup on first non-skipped command (not CI).
    maybe_run_auto_setup(&cli.command, quiet);

    // Quick editor scan: detect newly installed editors and install rules.
    // Only runs when machine is already set up (adds ~5ms).
    if cli::machine::machine_setup_status() == cli::machine::MachineStatus::Ready {
        cli::machine::quick_editor_scan();
    }

    // Handle subcommands or default to help (terminal-first)
    let result = match cli.command {
        None => {
            // Adaptive behavior based on context
            use cli::machine::MachineStatus;
            match cli::machine::machine_setup_status() {
                MachineStatus::NeverSetup => {
                    // First time ever: guide to init
                    println!();
                    println!(
                        "  \u{1f98b} {}",
                        format!("nika v{}", env!("CARGO_PKG_VERSION")).bold()
                    );
                    println!();
                    println!(
                        "  Welcome! Run {} to get started.",
                        "nika init".cyan().bold()
                    );
                    println!("  This will set up your machine and create a project.");
                    println!();
                    Ok(())
                }
                MachineStatus::NeedsUpdate => {
                    // User upgraded nika — nudge them to re-run init
                    println!();
                    println!(
                        "  \u{1f98b} {}",
                        format!("nika v{}", env!("CARGO_PKG_VERSION")).bold()
                    );
                    println!();
                    println!(
                        "  Upgraded to v{}! Run {} to update editor rules.",
                        env!("CARGO_PKG_VERSION"),
                        "nika init".cyan()
                    );
                    println!();
                    Ok(())
                }
                MachineStatus::Ready => {
                    if !Path::new(".nika").exists() {
                        // Machine setup done, but no project here
                        println!();
                        println!("  {} No project in current directory.", "\u{25cb}".dimmed());
                        println!();
                        println!(
                            "  {} {} start a new project",
                            "nika init".cyan().bold(),
                            "\u{2014}".dimmed()
                        );
                        println!(
                            "  {} {} run a workflow file",
                            "nika <file>".cyan().bold(),
                            "\u{2014}".dimmed()
                        );
                        println!(
                            "  {} {} check system health",
                            "nika doctor".cyan().bold(),
                            "\u{2014}".dimmed()
                        );
                        println!(
                            "  {} {} all commands",
                            "nika --help".cyan().bold(),
                            "\u{2014}".dimmed()
                        );
                        println!();
                        Ok(())
                    } else {
                        // Project exists: show help
                        use clap::CommandFactory;
                        if let Err(e) = Cli::command().print_help() {
                            eprintln!("Failed to print help: {e}");
                            std::process::exit(1);
                        }
                        Ok(())
                    }
                }
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
            permission,
        }) => run_workflow(&file, provider, model, quiet, detail, &permission).await,

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
            course,
            minimal,
        }) => {
            if course {
                // Generate interactive course
                use nika_engine::init::course::generator::{generate_course, CourseConfig};
                let config = CourseConfig {
                    dest: std::path::PathBuf::from("nika-course"),
                    ..CourseConfig::default()
                };
                match generate_course(&config) {
                    Ok(result) => {
                        println!(
                            "\n  {} Course generated! {} levels, {} exercises\n  Provider: {} (auto-detected)\n  Location: {}\n  Run: cd {} && nika course status\n",
                            "✓".green(),
                            result.levels,
                            result.exercises,
                            result.provider,
                            result.root.display(),
                            result.root.display(),
                        );
                        Ok(())
                    }
                    Err(e) => {
                        eprintln!("Course generation failed: {e}");
                        Err(e)
                    }
                }
            } else {
                cli::init::init_project(&permission, no_example || minimal, migrate_keys).await
            }
        }

        Some(Commands::Course { action }) => cli::course::handle_course_command(action),

        Some(Commands::Trace { action }) => cli::trace::handle_trace_command(action),

        #[cfg(feature = "tui")]
        Some(Commands::Provider { action }) => cli::provider::handle_provider_command(action).await,

        Some(Commands::Mcp { action }) => cli::mcp::handle_mcp_command(action).await,

        Some(Commands::Media { action }) => cli::media::handle_media_command(action, quiet).await,

        #[cfg(feature = "native-inference")]
        Some(Commands::Model { action }) => cli::model::handle_model_command(action, quiet).await,

        Some(Commands::Pkg { action }) => cli::pkg::handle_pkg_command(action).await,

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

        Some(Commands::Doctor { full, format, fix }) => {
            cli::doctor::handle_doctor_command(full, &format, quiet, fix).await
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

fn should_skip_auto_setup(cmd: &Option<Commands>) -> bool {
    match cmd {
        None => false,
        #[cfg(feature = "lsp")]
        Some(Commands::Lsp { .. }) => true,
        Some(Commands::Completion { .. }) => true,
        Some(Commands::Features) => true,
        Some(Commands::Schema { .. }) => true,
        Some(Commands::Doctor { .. }) => true,
        _ => {
            // Skip TUI commands if tui feature enabled
            #[cfg(feature = "tui")]
            if matches!(
                cmd,
                Some(Commands::Ui { .. } | Commands::Chat { .. } | Commands::Studio { .. })
            ) {
                return true;
            }
            false
        }
    }
}

fn maybe_run_auto_setup(cmd: &Option<Commands>, quiet: bool) {
    if should_skip_auto_setup(cmd) {
        return;
    }
    if cli::machine::is_ci() {
        return;
    }
    if cli::machine::is_machine_setup() {
        return;
    }
    if !quiet {
        println!("  {} Setting up Nika for your editors...\n", "◇".cyan());
    }
    cli::machine::run_machine_setup();
    if !quiet {
        println!();
    }
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
        eprintln!("{report:?}");
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

async fn run_workflow(
    file: &str,
    provider_override: Option<String>,
    model_override: Option<String>,
    quiet: bool,
    detail: nika::display::DetailLevel,
    permission: &str,
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

    if !quiet && !detail.is_json() {
        let layer_count = {
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
            nika::dag::flow::layer_count(&depths)
        };
        let gen_id = format!("{:08x}", rand::random::<u32>());
        nika::display::header::print_header(
            workflow.name.as_deref(),
            workflow.provider.as_deref().unwrap_or("(auto)"),
            workflow.model.as_deref().unwrap_or("(default)"),
            workflow.tasks.len(),
            layer_count,
            env!("CARGO_PKG_VERSION"),
            &gen_id,
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

    let perm_mode = match permission {
        "deny" => nika_engine::tools::PermissionMode::Deny,
        "plan" => nika_engine::tools::PermissionMode::Plan,
        "accept-edits" => nika_engine::tools::PermissionMode::AcceptEdits,
        "yolo" | "accept-all" => nika_engine::tools::PermissionMode::YoloMode,
        _ => nika_engine::tools::PermissionMode::AcceptEdits,
    };
    let mut runner = Runner::new(workflow)?.with_permission_mode(perm_mode);
    if quiet {
        runner = runner.quiet();
    }
    let mut runner = runner.with_detail_level(detail);
    let output = runner.run().await?;

    if !quiet && !output.is_empty() {
        println!("{}", "Output:".cyan().bold());
        println!("{output}");
    }

    Ok(())
}

async fn validate_workflow(file: &str, quiet: bool) -> Result<(), NikaError> {
    use nika::display::{
        print_check_header, print_check_summary, print_phase, print_phase_skipped, PhaseResult,
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

    // Phase 2: Parse
    let t = Instant::now();
    let workflow = parse_workflow(&yaml)?;
    let parse_elapsed = t.elapsed();

    let base_path = resolved_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));

    // Phase 3: Includes
    let t = Instant::now();
    let workflow = expand_includes(workflow, base_path)?;
    let includes_elapsed = t.elapsed();

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
            if let SchemaRef::File(ref path) = spec.schema {
                validate_schema_file(&task.id, path, base_path).await?;
                schema_count += 1;
            }
        }
    }
    let schemas_elapsed = t.elapsed();

    // Phase 7: Provider API keys (BUG 6 / NIKA-032)
    let t = Instant::now();
    let mut provider_warnings: Vec<String> = Vec::new();
    {
        let mut providers_used = std::collections::HashSet::new();
        providers_used.insert(workflow.provider.clone());

        // Collect per-task providers from analyzed AST
        if let Ok(analyzed) = parse_analyzed(&yaml) {
            for task in &analyzed.tasks {
                if let Some(ref p) = task.provider {
                    providers_used.insert(p.clone());
                }
            }
        }

        for provider_name in &providers_used {
            if let Some(provider) = nika::core::find_provider(provider_name) {
                if provider.requires_key && !provider.has_env_key() {
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

        // Phase 7: Provider API keys
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
                hints: vec!["Run 'nika provider set <name>' to configure API keys".to_string()],
            });
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
    let workflow = parse_workflow(&yaml)?;
    let parse_elapsed = t.elapsed();

    let base_path = resolved_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));

    // Phase 3: Includes
    let t = Instant::now();
    let workflow = expand_includes(workflow, base_path)?;
    let includes_elapsed = t.elapsed();

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
            if let SchemaRef::File(ref path) = spec.schema {
                validate_schema_file(&task.id, path, base_path).await?;
                schema_count += 1;
            }
        }
    }
    let schemas_elapsed = t.elapsed();

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
