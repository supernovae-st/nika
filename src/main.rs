//! Nika CLI - DAG workflow runner

use clap::{Parser, Subcommand};
use colored::Colorize;
use std::fs;

mod context;
mod dag;
mod datastore;
mod error;
mod executor;
mod output_policy;
mod provider;
mod runner;
mod task;
mod template;
mod use_block;
mod workflow;

use error::{FixSuggestion, NikaError};
use runner::Runner;
use workflow::Workflow;

#[derive(Parser)]
#[command(name = "nika")]
#[command(about = "Nika - DAG workflow runner for AI tasks")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
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

    /// Validate a workflow file (parse only)
    Validate {
        /// Path to .nika.yaml file
        file: String,
    },
}

#[tokio::main]
async fn main() {
    // Load .env file (ignore if not present)
    let _ = dotenvy::dotenv();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Run { file, provider, model } => {
            run_workflow(&file, provider, model).await
        }
        Commands::Validate { file } => validate_workflow(&file),
    };

    if let Err(e) = result {
        eprintln!("{} {}", "Error:".red().bold(), e);
        if let Some(suggestion) = e.fix_suggestion() {
            eprintln!("  {} {}", "Fix:".yellow(), suggestion);
        }
        std::process::exit(1);
    }
}

async fn run_workflow(
    file: &str,
    provider_override: Option<String>,
    model_override: Option<String>,
) -> Result<(), NikaError> {
    // Read and parse (async to not block runtime)
    let yaml = tokio::fs::read_to_string(file).await?;
    let mut workflow: Workflow = serde_yaml::from_str(&yaml)?;

    // Validate schema
    if workflow.schema != "nika/workflow@0.1" {
        return Err(NikaError::Template(format!(
            "Invalid schema: expected 'nika/workflow@0.1', got '{}'",
            workflow.schema
        )));
    }

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
    let workflow: Workflow = serde_yaml::from_str(&yaml)?;

    // Validate schema
    if workflow.schema != "nika/workflow@0.1" {
        return Err(NikaError::Template(format!(
            "Invalid schema: expected 'nika/workflow@0.1', got '{}'",
            workflow.schema
        )));
    }

    println!("{} Workflow '{}' is valid", "✓".green(), file);
    println!("  Provider: {}", workflow.provider);
    println!("  Model: {}", workflow.model.as_deref().unwrap_or("(default)"));
    println!("  Tasks: {}", workflow.tasks.len());
    println!("  Flows: {}", workflow.flows.len());

    Ok(())
}
