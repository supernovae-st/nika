//! Nika CLI - Workflow orchestration for Claude Agent SDK (v4.6)
//!
//! Architecture v4.6: 7 keywords with type inference
//! (agent, subagent, shell, http, mcp, function, llm)
//!
//! Usage:
//!   nika validate [path]    Validate workflow files
//!   nika run <workflow>     Run a workflow (requires runtime)
//!   nika init               Initialize a new .nika project
//!   nika tui                Launch interactive TUI dashboard

use clap::{Parser, Subcommand, ValueEnum};
use colored::Colorize;
use nika::{init_project, Runner, ValidationError, ValidationResult, Validator, Workflow};
use std::path::Path;
use walkdir::WalkDir;

#[derive(Parser)]
#[command(name = "nika")]
#[command(author = "SuperNovae Studio")]
#[command(version = "0.1.0")]
#[command(about = "CLI for Nika workflow orchestration (v4.6)", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Validate workflow files
    Validate {
        /// Path to validate (file or directory, defaults to current dir)
        #[arg(default_value = ".")]
        path: String,

        /// Output format
        #[arg(short, long, default_value = "pretty")]
        format: OutputFormat,

        /// Show verbose output
        #[arg(short, long)]
        verbose: bool,
    },
    /// Run a workflow file
    Run {
        /// Path to workflow file (.nika.yaml)
        workflow: String,

        /// LLM provider (claude, openai, ollama)
        #[arg(short, long, default_value = "claude")]
        provider: String,

        /// Show verbose output
        #[arg(short, long)]
        verbose: bool,
    },
    /// Initialize a new .nika project
    Init {
        /// Project name
        #[arg(default_value = ".")]
        name: String,
    },
    /// Launch interactive TUI dashboard
    Tui,
}

#[derive(Clone, ValueEnum)]
enum OutputFormat {
    Pretty,
    Json,
    Compact,
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Validate {
            path,
            format,
            verbose,
        }) => {
            if let Err(e) = run_validate(path, format, *verbose) {
                eprintln!("{} {}", "Error:".red().bold(), e);
                std::process::exit(1);
            }
        }
        Some(Commands::Run {
            workflow,
            provider,
            verbose,
        }) => {
            if let Err(e) = run_workflow(workflow, provider, *verbose) {
                eprintln!("{} {}", "Error:".red().bold(), e);
                std::process::exit(1);
            }
        }
        Some(Commands::Init { name }) => {
            if let Err(e) = run_init(name) {
                eprintln!("{} {}", "Error:".red().bold(), e);
                std::process::exit(1);
            }
        }
        Some(Commands::Tui) => {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
            if let Err(e) = rt.block_on(nika::tui::run(None)) {
                eprintln!("{} TUI error: {}", "Error:".red().bold(), e);
                std::process::exit(1);
            }
        }
        None => {
            print_banner();
        }
    }
}

fn print_banner() {
    println!(
        r#"
  ███╗   ██╗██╗██╗  ██╗ █████╗
  ████╗  ██║██║██║ ██╔╝██╔══██╗
  ██╔██╗ ██║██║█████╔╝ ███████║
  ██║╚██╗██║██║██╔═██╗ ██╔══██║
  ██║ ╚████║██║██║  ██╗██║  ██║
  ╚═╝  ╚═══╝╚═╝╚═╝  ╚═╝╚═╝  ╚═╝

  Native Intelligence Kernel for Agents
  v0.1.0 (Architecture v4.6)

  USAGE:
    nika <command> [options]

  COMMANDS:
    validate [path]    Validate .nika.yaml workflow files
    run <workflow>     Run a workflow (requires runtime)
    init [name]        Initialize new .nika project
    tui                Launch TUI dashboard

  EXAMPLES:
    nika validate
    nika validate my-workflow.nika.yaml
    nika run support.nika.yaml
    nika init my-project

  Architecture v4.6:
    - 7 keywords: agent, subagent, shell, http, mcp, function, llm
    - Type inference from keyword
    - Connection matrix with bridge pattern

  DOCS:
    https://nika.sh/docs

  Built by SuperNovae Studio
"#
    );
}

/// Run the validate command (v4.6)
fn run_validate(path: &str, format: &OutputFormat, verbose: bool) -> anyhow::Result<()> {
    let start_time = std::time::Instant::now();

    println!("{}", "Nika Validator v0.1.0".cyan().bold());
    println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());
    println!();

    if verbose {
        println!("{}", "Architecture v4.6: 7 keywords with type inference".dimmed());
        println!("{}", "Rules embedded - no external files needed".dimmed());
        println!();
    }

    // Find workflow files
    let target_path = Path::new(path);
    let files = find_workflow_files(target_path)?;

    if files.is_empty() {
        println!(
            "{}",
            format!("No workflow files found in {}", path).yellow()
        );
        return Ok(());
    }

    println!(
        "{}",
        format!("Found {} workflow file(s)", files.len()).dimmed()
    );
    println!();

    // Create validator (v4.6 - no external rules)
    let validator = Validator::new();

    // Validate each file
    let mut results = Vec::new();
    for file in &files {
        if verbose {
            println!("Validating {}...", file.display());
        }

        match validator.validate_file(file) {
            Ok(result) => {
                if verbose {
                    println!(
                        "  {} tasks, {} flows, {} errors, {} warnings",
                        result.task_count,
                        result.flow_count,
                        result.error_count(),
                        result.warning_count()
                    );
                }
                results.push(result);
            }
            Err(e) => {
                eprintln!("{} Failed to parse {}: {}", "Error:".red(), file.display(), e);
            }
        }
    }

    if verbose {
        let elapsed = start_time.elapsed();
        println!();
        println!("{}", format!("Validation complete in {:?}", elapsed).dimmed());
    }
    println!();

    // Output results
    match format {
        OutputFormat::Pretty => output_pretty(&results),
        OutputFormat::Json => output_json(&results)?,
        OutputFormat::Compact => output_compact(&results),
    }

    // Exit code based on errors
    let total_errors: usize = results.iter().map(|r| r.error_count()).sum();
    if total_errors > 0 {
        std::process::exit(1);
    }

    Ok(())
}

/// Run a workflow
fn run_workflow(workflow_path: &str, provider: &str, verbose: bool) -> anyhow::Result<()> {
    println!("{}", "Nika Runner v0.1.0".cyan().bold());
    println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());
    println!();

    // Check file exists
    let path = Path::new(workflow_path);
    if !path.exists() {
        anyhow::bail!("Workflow file not found: {}", workflow_path);
    }

    // Parse workflow
    let content = std::fs::read_to_string(path)?;
    let workflow: Workflow = serde_yaml::from_str(&content)?;

    // Validate first
    let validator = Validator::new();
    let validation = validator.validate(&workflow, workflow_path);

    if !validation.is_valid() {
        println!("{} Workflow validation failed:", "Error:".red().bold());
        for error in &validation.errors {
            println!("  {} {}", "✗".red(), error);
        }
        std::process::exit(1);
    }

    println!(
        "Workflow: {} ({} tasks, {} flows)",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("workflow"),
        workflow.tasks.len(),
        workflow.flows.len()
    );
    println!("Provider: {}", provider.cyan());
    println!();

    // Run workflow
    let runner = Runner::new(provider)?.verbose(verbose);
    let result = runner.run(&workflow)?;

    // Output results
    println!();
    println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());
    println!(
        "Completed: {} tasks | Failed: {} | Tokens: ~{}",
        result.tasks_completed.to_string().green(),
        if result.tasks_failed > 0 {
            result.tasks_failed.to_string().red()
        } else {
            result.tasks_failed.to_string().normal()
        },
        result.total_tokens
    );

    if result.tasks_failed > 0 {
        std::process::exit(1);
    }

    Ok(())
}

/// Initialize a new project
fn run_init(name: &str) -> anyhow::Result<()> {
    println!("{}", "Nika v0.1.0".cyan().bold());
    println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());
    println!();

    let cwd = std::env::current_dir()?;
    let result = init_project(name, &cwd)?;

    println!("{} Project initialized!", "✓".green());
    println!();
    println!("Created:");
    for file in &result.files_created {
        println!("  {} {}", "•".dimmed(), file);
    }
    println!();
    println!("Next steps:");
    println!("  {} Edit {}", "1.".dimmed(), "main.nika.yaml".cyan());
    println!("  {} Run  {}", "2.".dimmed(), "nika validate".cyan());
    println!("  {} Run  {}", "3.".dimmed(), "nika run main.nika.yaml".cyan());

    Ok(())
}

/// Find all workflow files in a path
fn find_workflow_files(path: &Path) -> anyhow::Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();

    if path.is_file() {
        files.push(path.to_path_buf());
    } else if path.is_dir() {
        for entry in WalkDir::new(path)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let name = entry.file_name().to_string_lossy();
            if name.ends_with(".nika.yaml") {
                files.push(entry.path().to_path_buf());
            }
        }
    }

    Ok(files)
}

/// Pretty output format
fn output_pretty(results: &[ValidationResult]) {
    for result in results {
        let errors = result.error_count();
        let warnings = result.warning_count();

        if errors == 0 && warnings == 0 {
            println!(
                "{} {}: {} tasks, {} flows",
                "".green(),
                result.file_path,
                result.task_count,
                result.flow_count
            );
        } else if errors == 0 {
            println!(
                "{} {}: {} tasks, {} flows ({} warnings)",
                "".yellow(),
                result.file_path,
                result.task_count,
                result.flow_count,
                warnings
            );
        } else {
            println!(
                "{} {}: {} errors, {} warnings",
                "".red(),
                result.file_path,
                errors,
                warnings
            );
        }

        // Print errors and warnings
        for error in &result.errors {
            print_validation_item(error);
        }

        println!();
    }

    // Summary
    let total_files = results.len();
    let passed = results.iter().filter(|r| r.is_valid()).count();
    let failed = total_files - passed;
    let total_warnings: usize = results.iter().map(|r| r.warning_count()).sum();

    println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());
    println!(
        "Summary: {} files | {} passed | {} failed | {} warnings",
        total_files,
        passed.to_string().green(),
        if failed > 0 {
            failed.to_string().red()
        } else {
            failed.to_string().normal()
        },
        if total_warnings > 0 {
            total_warnings.to_string().yellow()
        } else {
            total_warnings.to_string().normal()
        }
    );
}

/// Print a validation error or warning
fn print_validation_item(error: &ValidationError) {
    if error.is_warning() {
        println!("  {} {}", "".yellow(), error);
    } else {
        println!("  {} {}", "".red(), error);
    }
}

/// Compact output format
fn output_compact(results: &[ValidationResult]) {
    for result in results {
        let status = if result.is_valid() { "OK" } else { "ERR" };
        println!(
            "{} {} ({}t, {}f, {}e, {}w)",
            status,
            result.file_path,
            result.task_count,
            result.flow_count,
            result.error_count(),
            result.warning_count()
        );
    }
}

/// JSON output format
fn output_json(results: &[ValidationResult]) -> anyhow::Result<()> {
    let json_results: Vec<_> = results
        .iter()
        .map(|r| {
            serde_json::json!({
                "file": r.file_path,
                "valid": r.is_valid(),
                "task_count": r.task_count,
                "flow_count": r.flow_count,
                "error_count": r.error_count(),
                "warning_count": r.warning_count(),
                "errors": r.errors.iter()
                    .filter(|e| !e.is_warning())
                    .map(|e| format!("{}", e))
                    .collect::<Vec<_>>(),
                "warnings": r.errors.iter()
                    .filter(|e| e.is_warning())
                    .map(|e| format!("{}", e))
                    .collect::<Vec<_>>(),
            })
        })
        .collect();

    println!("{}", serde_json::to_string_pretty(&json_results)?);
    Ok(())
}
