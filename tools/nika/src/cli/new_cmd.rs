//! New workflow creation subcommand handler

use std::path::PathBuf;

use colored::Colorize;

use nika::error::NikaError;

#[allow(clippy::too_many_arguments)]
pub fn handle_new_command(
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
                println!("{name}");
            } else {
                println!(
                    "  {} {}",
                    format!("{name:<18}").green(),
                    format!("[{category}] {description}").white()
                );
            }
        }
        return Ok(());
    }

    // Determine mode: wizard, template, or custom flags
    // Prefix with _ to avoid unused warning when TUI is disabled
    let _has_flags = template.is_some()
        || verb.is_some()
        || provider.is_some()
        || output.is_some()
        || with_mcp
        || with_include
        || with_artifacts;

    // If wizard flag is set, or no name and no flags, launch wizard
    #[cfg(feature = "tui")]
    if wizard || (name.is_none() && !_has_flags) {
        let path = super::new_wizard::run_wizard(output_dir)?;
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
                    "Unknown template: '{template_name}'. Use --list to see available templates."
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

    // Parse output format
    let output_format = output
        .map(|o| {
            OutputFormat::from_name(&o).ok_or_else(|| NikaError::ValidationError {
                reason: format!("Unknown output format: '{o}'. Valid: text, json, yaml"),
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
