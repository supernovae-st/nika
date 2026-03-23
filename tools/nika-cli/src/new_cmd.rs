//! New workflow creation subcommand handler

use std::path::PathBuf;

use colored::Colorize;

use nika_engine::error::NikaError;

pub fn handle_new_command(
    name: Option<String>,
    verb: Option<String>,
    provider: Option<String>,
    output_dir: Option<PathBuf>,
    quiet: bool,
) -> Result<(), NikaError> {
    use nika_engine::new::{NewWorkflowConfig, OutputFormat, Provider, Verb};

    let output_dir = output_dir.unwrap_or_else(|| PathBuf::from("."));

    // Require name
    let workflow_name = name.ok_or_else(|| NikaError::ValidationError {
        reason: "Workflow name is required. Use: nika new <NAME> [--verb exec]".to_string(),
    })?;

    // Parse verb
    let verb = verb
        .map(|v| {
            Verb::from_name(&v).ok_or_else(|| NikaError::ValidationError {
                reason: format!("Unknown verb: '{v}'. Valid: infer, exec, fetch, invoke, agent"),
            })
        })
        .transpose()?
        .unwrap_or_default();

    // Parse provider
    let provider = provider
        .map(|p| {
            Provider::from_name(&p).ok_or_else(|| NikaError::ValidationError {
                reason: format!(
                    "Unknown provider: '{p}'. Valid: claude, openai, mistral, groq, deepseek, native"
                ),
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
        output_format: OutputFormat::default(),
        with_mcp: false,
        with_include: false,
        with_artifacts: false,
        output_dir,
    };

    // Generate workflow
    let path = config.write()?;

    if !quiet {
        println!("{} Created: {}", "SUCCESS!".green().bold(), path.display());
        println!("  Verb: {}", config.verb.name().cyan());
        println!("  Provider: {}", config.provider.name().yellow());
        println!("  Run: nika {}", path.display());
    }

    Ok(())
}
