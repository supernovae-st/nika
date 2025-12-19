//! Nika CLI - Workflow orchestration for Claude Agent SDK
//!
//! Usage:
//!   nika run <workflow>     Run a workflow
//!   nika validate [path]    Validate workflow files
//!   nika init               Initialize a new .nika project
//!   nika install <package>  Install a community package
//!   nika tui                Launch interactive TUI dashboard

use clap::{Parser, Subcommand, ValueEnum};
use colored::Colorize;
use nika::{ValidationError, ValidationResult, Validator};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "nika")]
#[command(author = "SuperNovae Studio")]
#[command(version = "0.1.0")]
#[command(about = "CLI for Nika workflow orchestration", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a workflow file
    Run {
        /// Path to workflow file (.wf.yaml)
        workflow: String,
    },
    /// Validate workflow files
    Validate {
        /// Path to validate (file or directory, defaults to .nika/)
        #[arg(default_value = ".nika")]
        path: String,

        /// Output format
        #[arg(short, long, default_value = "pretty")]
        format: OutputFormat,

        /// Path to spec/validation directory (defaults to spec/validation)
        #[arg(long, default_value = "spec/validation")]
        rules: String,

        /// Show detailed validation steps
        #[arg(short, long)]
        verbose: bool,
    },
    /// Initialize a new .nika project
    Init {
        /// Project name
        #[arg(default_value = ".")]
        name: String,
    },
    /// Install a community package
    Install {
        /// Package name (e.g., @nika/support-template)
        package: String,
    },
    /// Launch interactive TUI dashboard
    Tui,
    /// Authenticate with registry
    Auth {
        #[command(subcommand)]
        command: AuthCommands,
    },
    /// Publish package to registry
    Publish {
        /// Path to node or workflow file
        path: PathBuf,
    },
}

#[derive(Subcommand)]
enum AuthCommands {
    /// Login to registry
    Login,
    /// Logout from registry
    Logout,
    /// Show authentication status
    Status,
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
        Some(Commands::Run { workflow }) => {
            println!("🚀 Nika v0.1.0");
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!("Running workflow: {}", workflow);
            println!();
            println!("⚠️  Not implemented yet.");
            println!("    This will parse the .wf.yaml and call Claude SDK via subprocess.");
        }
        Some(Commands::Validate {
            path,
            format,
            rules,
            verbose,
        }) => {
            if let Err(e) = run_validate(path, format, rules, *verbose) {
                eprintln!("{} {}", "Error:".red().bold(), e);
                std::process::exit(1);
            }
        }
        Some(Commands::Init { name }) => {
            println!("📁 Initializing Nika project: {}", name);
            println!();
            println!("⚠️  Not implemented yet.");
            println!("    This will create .nika/ directory structure.");
        }
        Some(Commands::Install { package }) => {
            println!("📦 Installing: {}", package);
            println!();
            println!("⚠️  Not implemented yet.");
            println!("    This will fetch from npm registry.");
        }
        Some(Commands::Tui) => {
            println!("🖥️  Launching TUI dashboard...");
            println!();
            println!("⚠️  Not implemented yet.");
            println!("    This will launch Ratatui interface.");
        }
        Some(Commands::Auth { command }) => match command {
            AuthCommands::Login => {
                if let Err(e) = nika::auth::login() {
                    eprintln!("{} {}", "Error:".red().bold(), e);
                    std::process::exit(1);
                }
            }
            AuthCommands::Logout => {
                if let Err(e) = nika::auth::logout() {
                    eprintln!("{} {}", "Error:".red().bold(), e);
                    std::process::exit(1);
                }
            }
            AuthCommands::Status => {
                if let Err(e) = nika::auth::status() {
                    eprintln!("{} {}", "Error:".red().bold(), e);
                    std::process::exit(1);
                }
            }
        },
        Some(Commands::Publish { path }) => {
            if let Err(e) = nika::publish::publish(path) {
                eprintln!("{} {}", "Error:".red().bold(), e);
                std::process::exit(1);
            }
        }
        None => {
            // No subcommand - show welcome message
            println!(
                r#"
  ███╗   ██╗██╗██╗  ██╗ █████╗
  ████╗  ██║██║██║ ██╔╝██╔══██╗
  ██╔██╗ ██║██║█████╔╝ ███████║
  ██║╚██╗██║██║██╔═██╗ ██╔══██║
  ██║ ╚████║██║██║  ██╗██║  ██║
  ╚═╝  ╚═══╝╚═╝╚═╝  ╚═╝╚═╝  ╚═╝

  Native Intelligence Kernel for Agents
  v0.1.0

  USAGE:
    nika <command> [options]

  COMMANDS:
    run <workflow>     Run a .wf.yaml workflow
    validate [path]    Validate workflow files
    init [name]        Initialize new .nika project
    install <package>  Install community package
    tui                Launch TUI dashboard
    auth <subcommand>  Authenticate with registry
    publish <path>     Publish package to registry

  EXAMPLES:
    nika run support.wf.yaml
    nika validate
    nika init my-project
    nika install @nika/support-template
    nika auth login
    nika publish .nika/workflows/my-workflow.wf.yaml

  DOCS:
    https://nika.sh/docs

  Built by SuperNovae Studio
"#
            );
        }
    }
}

/// Run the validate command
fn run_validate(
    path: &str,
    format: &OutputFormat,
    rules_dir: &str,
    verbose: bool,
) -> anyhow::Result<()> {
    let start_time = std::time::Instant::now();

    if verbose {
        eprintln!("{}", "🔍 Nika Validator v0.1.0".cyan().bold());
        eprintln!("   Loading rules from {:?}...", rules_dir);
    } else {
        println!("{}", "🔍 Nika Validator v0.1.0".cyan().bold());
        println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());
        println!();
    }

    // Load validator from rules
    let rules_path = Path::new(rules_dir);
    let validator = Validator::from_spec_dir(rules_path).map_err(|e| {
        anyhow::anyhow!(
            "Failed to load validation rules from {:?}: {}",
            rules_path,
            e
        )
    })?;

    if verbose {
        eprintln!(
            "   Found {} node types from spec",
            validator.node_types().lookup.len()
        );
    }

    // Load custom nodes from project directory (if .nika/nodes/ exists)
    let target_path = Path::new(path);
    let project_dir = if target_path.is_file() {
        target_path.parent().unwrap_or(Path::new("."))
    } else {
        target_path
    };

    if verbose {
        eprintln!("   Loading custom nodes from {:?}...", project_dir);
    }

    let validator = validator
        .with_custom_nodes(project_dir)
        .unwrap_or_else(|e| {
            if verbose {
                eprintln!("   {}", format!("Warning: {}", e).yellow());
            } else {
                eprintln!("{}", format!("⚠️  Warning: {}", e).yellow());
            }
            Validator::from_spec_dir(rules_path).unwrap()
        });

    if verbose {
        eprintln!(
            "   Total node types: {}",
            validator.node_types().lookup.len()
        );
        eprintln!("   Validating {:?}...", path);
    } else {
        println!(
            "{}",
            format!(
                "📋 Loaded {} node types",
                validator.node_types().lookup.len()
            )
            .dimmed()
        );
        println!();
    }

    // Find workflow files
    let files = find_workflow_files(target_path)?;

    if files.is_empty() {
        println!(
            "{}",
            format!("⚠️  No workflow files found in {}", path).yellow()
        );
        return Ok(());
    }

    if verbose {
        eprintln!("   Found {} workflow file(s)", files.len());
        for file in &files {
            eprintln!("     • {}", file.display());
        }
        eprintln!();
    } else {
        println!(
            "{}",
            format!("📁 Found {} workflow file(s)", files.len()).dimmed()
        );
        println!();
    }

    // Validate each file
    let mut results = Vec::new();
    for file in &files {
        if verbose {
            eprintln!("   Validating {}...", file.display());
        }

        match validator.validate_file(file) {
            Ok(result) => {
                if verbose {
                    eprintln!(
                        "     → {} nodes, {} edges, {} errors, {} warnings",
                        result.node_count,
                        result.edge_count,
                        result.errors.len(),
                        result.warnings.len()
                    );
                }
                results.push(result);
            }
            Err(e) => {
                eprintln!("{} Failed to parse {}: {}", "❌".red(), file.display(), e);
            }
        }
    }

    if verbose {
        let elapsed = start_time.elapsed();
        eprintln!();
        eprintln!("   Validation complete in {:?}", elapsed);
        eprintln!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());
        eprintln!();
    }

    // Output results
    match format {
        OutputFormat::Pretty => output_pretty(&results),
        OutputFormat::Json => output_json(&results)?,
        OutputFormat::Compact => output_compact(&results),
    }

    // Exit code based on errors
    let total_errors: usize = results.iter().map(|r| r.errors.len()).sum();
    if total_errors > 0 {
        std::process::exit(1);
    }

    Ok(())
}

/// Find all workflow files in a path
fn find_workflow_files(path: &Path) -> anyhow::Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();

    if path.is_file() {
        // Single file
        files.push(path.to_path_buf());
    } else if path.is_dir() {
        // Directory - find all .wf.yaml, .node.yaml, .nika.yaml files
        for entry in walkdir::WalkDir::new(path)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let name = entry.file_name().to_string_lossy();
            if name.ends_with(".wf.yaml")
                || name.ends_with(".node.yaml")
                || name.ends_with(".nika.yaml")
            {
                files.push(entry.path().to_path_buf());
            }
        }
    }

    Ok(files)
}

/// Pretty output format
fn output_pretty(results: &[ValidationResult]) {
    for result in results {
        if result.is_valid() && !result.has_warnings() {
            println!(
                "{} {}: {} nodes, {} edges",
                "✅".green(),
                result.file_path,
                result.node_count,
                result.edge_count
            );
        } else if result.is_valid() {
            println!(
                "{} {}: {} nodes, {} edges ({} warnings)",
                "⚠️".yellow(),
                result.file_path,
                result.node_count,
                result.edge_count,
                result.warnings.len()
            );
        } else {
            println!(
                "{} {}: {} errors, {} warnings",
                "❌".red(),
                result.file_path,
                result.errors.len(),
                result.warnings.len()
            );
        }

        // Print errors
        for error in &result.errors {
            print_error(error);
        }

        // Print warnings
        for warning in &result.warnings {
            print_warning(warning);
        }

        println!();
    }

    // Summary
    let total_files = results.len();
    let passed = results.iter().filter(|r| r.is_valid()).count();
    let failed = total_files - passed;
    let warnings: usize = results.iter().map(|r| r.warnings.len()).sum();

    println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());
    println!(
        "Summary: {} files | {} {} | {} {} | {} {}",
        total_files,
        passed,
        "✅".green(),
        failed,
        "❌".red(),
        warnings,
        "⚠️".yellow()
    );
}

/// Print a validation error
fn print_error(error: &ValidationError) {
    let layer = error.layer();
    let layer_badge = format!("[L{}:{}]", layer as u8, layer).dimmed();

    println!("  {} {} {}", "❌".red(), layer_badge, error);

    if let Some(suggestion) = error.suggestion() {
        println!("     {} {}", "💡".cyan(), suggestion.cyan());
    }
}

/// Print a validation warning
fn print_warning(warning: &ValidationError) {
    let layer = warning.layer();
    let layer_badge = format!("[L{}:{}]", layer as u8, layer).dimmed();

    println!("  {} {} {}", "⚠️".yellow(), layer_badge, warning);

    if let Some(suggestion) = warning.suggestion() {
        println!("     {} {}", "💡".cyan(), suggestion.cyan());
    }
}

/// Compact output format
fn output_compact(results: &[ValidationResult]) {
    for result in results {
        let status = if result.is_valid() { "✅" } else { "❌" };
        println!(
            "{} {} ({}n, {}e, {}err, {}warn)",
            status,
            result.file_path,
            result.node_count,
            result.edge_count,
            result.errors.len(),
            result.warnings.len()
        );
    }
}

/// JSON output format
fn output_json(results: &[ValidationResult]) -> anyhow::Result<()> {
    // Convert to serializable format
    let json_results: Vec<_> = results
        .iter()
        .map(|r| {
            serde_json::json!({
                "file": r.file_path,
                "valid": r.is_valid(),
                "node_count": r.node_count,
                "edge_count": r.edge_count,
                "error_count": r.errors.len(),
                "warning_count": r.warnings.len(),
                "errors": r.errors.iter().map(|e| format!("{}", e)).collect::<Vec<_>>(),
                "warnings": r.warnings.iter().map(|w| format!("{}", w)).collect::<Vec<_>>(),
            })
        })
        .collect();

    println!("{}", serde_json::to_string_pretty(&json_results)?);
    Ok(())
}
