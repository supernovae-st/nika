//! Nika CLI - Workflow orchestration for Claude Agent SDK (v4.7.1)
//!
//! Architecture v4.7.1: 7 keywords with type inference
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
#[command(about = "CLI for Nika workflow orchestration (v4.7.1)", long_about = None)]
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
        /// Path to workflow file (optional, uses nika.yaml main: if not specified)
        workflow: Option<String>,

        /// LLM provider (claude, openai, ollama)
        #[arg(short, long, default_value = "claude")]
        provider: String,

        /// Input parameters (can be repeated: --input key=value)
        #[arg(short, long = "input", value_name = "KEY=VALUE")]
        inputs: Vec<String>,

        /// Config file with inputs (YAML format)
        #[arg(short, long, value_name = "FILE")]
        config: Option<String>,

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
            inputs,
            config,
            verbose,
        }) => {
            // Resolve workflow path: use provided or find from nika.yaml
            let workflow_path = match workflow {
                Some(path) => path.clone(),
                None => match resolve_main_workflow() {
                    Ok(path) => path,
                    Err(e) => {
                        eprintln!("{} {}", "Error:".red().bold(), e);
                        std::process::exit(1);
                    }
                },
            };

            // Parse inputs from --input args and --config file
            let parsed_inputs = match parse_inputs(inputs, config.as_deref()) {
                Ok(i) => i,
                Err(e) => {
                    eprintln!("{} {}", "Error:".red().bold(), e);
                    std::process::exit(1);
                }
            };

            let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
            if let Err(e) = rt.block_on(run_workflow(
                &workflow_path,
                provider,
                parsed_inputs,
                *verbose,
            )) {
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
  v0.1.0 (Architecture v4.7.1)

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

  Architecture v4.7.1:
    - 7 keywords: agent, subagent, shell, http, mcp, function, llm
    - Type inference from keyword
    - Connection matrix with bridge pattern

  DOCS:
    https://nika.sh/docs

  Built by SuperNovae Studio
"#
    );
}

/// Project manifest (nika.yaml)
#[derive(serde::Deserialize)]
struct ProjectManifest {
    main: Option<String>,
}

/// Config file structure for --config
#[derive(serde::Deserialize, Default)]
struct ConfigFile {
    #[serde(flatten)]
    inputs: std::collections::HashMap<String, serde_yaml::Value>,
}

/// Parse inputs from --input args and --config file
fn parse_inputs(
    args: &[String],
    config_path: Option<&str>,
) -> anyhow::Result<std::collections::HashMap<String, String>> {
    let mut inputs = std::collections::HashMap::new();

    // First, load from config file if provided
    if let Some(path) = config_path {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path))?;
        let config: ConfigFile = serde_yaml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path))?;

        for (key, value) in config.inputs {
            // Skip special keys like 'secrets'
            if key == "secrets" {
                continue;
            }
            let string_value = match value {
                serde_yaml::Value::String(s) => s,
                serde_yaml::Value::Number(n) => n.to_string(),
                serde_yaml::Value::Bool(b) => b.to_string(),
                _ => serde_yaml::to_string(&value)?,
            };
            inputs.insert(key, string_value);
        }
    }

    // Then, override with --input args (they take precedence)
    for arg in args {
        let parts: Vec<&str> = arg.splitn(2, '=').collect();
        if parts.len() != 2 {
            anyhow::bail!("Invalid input format '{}': expected KEY=VALUE", arg);
        }
        inputs.insert(parts[0].to_string(), parts[1].to_string());
    }

    Ok(inputs)
}

use anyhow::Context;

/// Resolve the main workflow from nika.yaml
fn resolve_main_workflow() -> anyhow::Result<String> {
    let manifest_path = Path::new("nika.yaml");

    if !manifest_path.exists() {
        anyhow::bail!(
            "No workflow specified and no nika.yaml found.\n\
             Usage: nika run <workflow.nika.yaml>\n\
             Or create a project with: nika init"
        );
    }

    let content = std::fs::read_to_string(manifest_path)?;
    let manifest: ProjectManifest = serde_yaml::from_str(&content)?;

    match manifest.main {
        Some(main) => {
            let main_path = Path::new(&main);
            if !main_path.exists() {
                anyhow::bail!("Main workflow '{}' not found (defined in nika.yaml)", main);
            }
            Ok(main)
        }
        None => anyhow::bail!(
            "No 'main' field in nika.yaml.\n\
             Add: main: your-workflow.nika.yaml"
        ),
    }
}

/// Run the validate command (v4.7.1)
fn run_validate(path: &str, format: &OutputFormat, verbose: bool) -> anyhow::Result<()> {
    let start_time = std::time::Instant::now();

    println!("{}", "Nika Validator v0.1.0".cyan().bold());
    println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());
    println!();

    if verbose {
        println!(
            "{}",
            "Architecture v4.7.1: 7 keywords with type inference".dimmed()
        );
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

    // Create validator (v4.7.1 - no external rules)
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
                eprintln!(
                    "{} Failed to parse {}: {}",
                    "Error:".red(),
                    file.display(),
                    e
                );
            }
        }
    }

    if verbose {
        let elapsed = start_time.elapsed();
        println!();
        println!(
            "{}",
            format!("Validation complete in {:?}", elapsed).dimmed()
        );
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
async fn run_workflow(
    workflow_path: &str,
    provider: &str,
    inputs: std::collections::HashMap<String, String>,
    verbose: bool,
) -> anyhow::Result<()> {
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

    // Show inputs if any
    if !inputs.is_empty() {
        println!("Inputs: {}", format!("{:?}", inputs).dimmed());
    }

    // Print DAG visualization
    println!();
    println!("{}", "────────────────────────────────────────".dimmed());
    println!("{}", "DAG:".bold());
    print_dag(&workflow);
    println!("{}", "────────────────────────────────────────".dimmed());
    println!();

    // Create runner with v4.7.1 architecture
    let runner = Runner::new(provider)?.verbose(verbose);

    if verbose {
        println!("{}", "Using v4.7.1 runner architecture".dimmed());
        println!(
            "{}",
            "SharedAgentRunner for agent: tasks (shared context)".dimmed()
        );
        println!(
            "{}",
            "IsolatedAgentRunner for subagent: tasks (isolated context)".dimmed()
        );
        println!();
    }

    // Execute workflow with inputs
    let result = runner.run_with_inputs(&workflow, inputs).await?;

    // Print execution results
    println!();
    println!("{}", "────────────────────────────────────────".dimmed());
    println!("{}", "Execution:".bold());
    let failed_tasks: Vec<_> = result.results.iter().filter(|r| !r.success).collect();
    for task_result in &result.results {
        let status = if task_result.success {
            "✓".green()
        } else {
            "✗".red()
        };
        println!("  {} {}", status, task_result.task_id);
    }
    println!();
    println!(
        "  {} {} tasks | {} tokens",
        "→".dimmed(),
        result.tasks_completed,
        result.total_tokens
    );
    println!();
    if failed_tasks.is_empty() {
        println!("  {} Completed", "✓".green().bold());
    } else {
        println!("  {} {} tasks failed", "✗".red().bold(), failed_tasks.len());
    }
    println!("{}", "────────────────────────────────────────".dimmed());

    // Print final output
    let output_task_id = workflow.agent.output.as_deref();
    if let Some(output_id) = output_task_id {
        if let Some(final_result) = result.results.iter().find(|r| r.task_id == output_id) {
            println!();
            println!("{}", "════════════════════════════════════════".yellow());
            println!("{}", format!("OUTPUT ({})", output_id).yellow().bold());
            println!("{}", "════════════════════════════════════════".yellow());
            println!();
            println!("{}", final_result.output);
            println!();
            println!("{}", "════════════════════════════════════════".yellow());
        }
    } else if let Some(last_result) = result.results.last() {
        // Fallback to last task if no output specified
        println!();
        println!("{}", "════════════════════════════════════════".yellow());
        println!(
            "{}",
            format!("OUTPUT ({})", last_result.task_id).yellow().bold()
        );
        println!("{}", "════════════════════════════════════════".yellow());
        println!();
        println!("{}", last_result.output);
        println!();
        println!("{}", "════════════════════════════════════════".yellow());
    }

    Ok(())
}

/// Print DAG visualization
fn print_dag(workflow: &Workflow) {
    use std::collections::{HashMap, HashSet};

    // Build dependency graph
    let mut deps: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut all_tasks: HashSet<&str> = HashSet::new();

    for task in &workflow.tasks {
        all_tasks.insert(&task.id);
        deps.entry(&task.id).or_default();
    }

    for flow in &workflow.flows {
        deps.entry(&flow.target).or_default().push(&flow.source);
    }

    // Find tasks with no dependencies (roots)
    let roots: Vec<&str> = workflow
        .tasks
        .iter()
        .filter(|t| deps.get(t.id.as_str()).is_none_or(|d| d.is_empty()))
        .map(|t| t.id.as_str())
        .collect();

    // Compute layers (topological levels)
    let mut layers: Vec<Vec<&str>> = Vec::new();
    let mut assigned: HashSet<&str> = HashSet::new();

    // Layer 0: roots
    if !roots.is_empty() {
        layers.push(roots.clone());
        for r in &roots {
            assigned.insert(r);
        }
    }

    // Build remaining layers
    loop {
        let mut next_layer: Vec<&str> = Vec::new();
        for task in &workflow.tasks {
            if assigned.contains(task.id.as_str()) {
                continue;
            }
            // Check if all dependencies are assigned
            let task_deps = deps.get(task.id.as_str()).map_or(&[][..], |v| v.as_slice());
            if task_deps.iter().all(|d| assigned.contains(d)) {
                next_layer.push(&task.id);
            }
        }
        if next_layer.is_empty() {
            break;
        }
        for t in &next_layer {
            assigned.insert(t);
        }
        layers.push(next_layer);
    }

    // Get task type icon
    let get_icon = |task_id: &str| -> &str {
        if let Some(task) = workflow.tasks.iter().find(|t| t.id == task_id) {
            match task.keyword() {
                nika::TaskKeyword::Agent => "🤖",
                nika::TaskKeyword::Subagent => "🧠",
                nika::TaskKeyword::Shell => "🔧",
                nika::TaskKeyword::Http => "🌐",
                nika::TaskKeyword::Mcp => "🔌",
                nika::TaskKeyword::Function => "ƒ ",
                nika::TaskKeyword::Llm => "⚡",
            }
        } else {
            "• "
        }
    };

    // Get task type name
    let get_type = |task_id: &str| -> &str {
        if let Some(task) = workflow.tasks.iter().find(|t| t.id == task_id) {
            match task.keyword() {
                nika::TaskKeyword::Agent => "agent",
                nika::TaskKeyword::Subagent => "subagent",
                nika::TaskKeyword::Shell => "shell",
                nika::TaskKeyword::Http => "http",
                nika::TaskKeyword::Mcp => "mcp",
                nika::TaskKeyword::Function => "function",
                nika::TaskKeyword::Llm => "llm",
            }
        } else {
            "?"
        }
    };

    // Print layers
    for (i, layer) in layers.iter().enumerate() {
        println!("  {} Layer {}:", if i == 0 { "┌" } else { "├" }, i);
        for (j, task_id) in layer.iter().enumerate() {
            let is_last = j == layer.len() - 1;
            let branch = if is_last { "└" } else { "├" };
            let icon = get_icon(task_id);
            let task_type = get_type(task_id);
            println!(
                "  │   {} {} {} ({})",
                branch,
                icon,
                task_id,
                task_type.dimmed()
            );
        }
        if i < layers.len() - 1 {
            println!("  │   ↓");
        }
    }
    println!("  └");
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
    println!(
        "  {} Run  {}",
        "3.".dimmed(),
        "nika run main.nika.yaml".cyan()
    );

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
