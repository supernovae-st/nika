// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika explain` command handler — human-readable workflow summary.

use std::path::Path;

use colored::Colorize;
use nika_engine::ast::parse_analyzed_with_includes;
use nika_engine::ast::schema_validator::WorkflowSchemaValidator;
use nika_engine::error::NikaError;

use crate::discover::resolve_workflow_path;

/// Print a human-readable summary of a workflow.
pub async fn explain_workflow(file: &str) -> Result<(), NikaError> {
    let resolved = resolve_workflow_path(file).await?;
    let yaml = tokio::fs::read_to_string(&resolved).await?;

    let validator = WorkflowSchemaValidator::new()?;
    validator.validate_yaml(&yaml)?;

    let base_path = resolved
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let workflow = parse_analyzed_with_includes(&yaml, base_path)?;

    // Count verbs
    let mut infer_count = 0u32;
    let mut exec_count = 0u32;
    let mut fetch_count = 0u32;
    let mut invoke_count = 0u32;
    let mut agent_count = 0u32;
    for task in &workflow.tasks {
        match &task.action {
            nika_engine::ast::analyzed::AnalyzedTaskAction::Infer(_) => infer_count += 1,
            nika_engine::ast::analyzed::AnalyzedTaskAction::Exec(_) => exec_count += 1,
            nika_engine::ast::analyzed::AnalyzedTaskAction::Fetch(_) => fetch_count += 1,
            nika_engine::ast::analyzed::AnalyzedTaskAction::Invoke(_) => invoke_count += 1,
            nika_engine::ast::analyzed::AnalyzedTaskAction::Agent(_) => agent_count += 1,
        }
    }

    // Collect required providers
    let default_provider = workflow
        .provider
        .as_ref()
        .map(|p| p.as_str())
        .unwrap_or("anthropic");
    let mut providers: Vec<&str> = vec![default_provider];
    for task in &workflow.tasks {
        if let Some(ref p) = task.provider {
            let name = p.as_str();
            if !providers.contains(&name) {
                providers.push(name);
            }
        }
    }

    // LLM task count for cost estimate
    let llm_tasks = infer_count + agent_count;

    // Count dependency layers (simple: max depth via depends_on chain)
    let task_count = workflow.tasks.len();

    println!();
    println!(
        "  {} {}",
        "Workflow:".bold(),
        workflow.name.as_deref().unwrap_or(file)
    );
    if let Some(ref desc) = workflow.description {
        println!("  {} {}", "Description:".bold(), desc);
    }
    println!();
    println!("  {} tasks", task_count.to_string().cyan().bold(),);
    println!();

    // Verb breakdown
    let mut verbs = Vec::new();
    if infer_count > 0 {
        verbs.push(format!("{infer_count} infer"));
    }
    if exec_count > 0 {
        verbs.push(format!("{exec_count} exec"));
    }
    if fetch_count > 0 {
        verbs.push(format!("{fetch_count} fetch"));
    }
    if invoke_count > 0 {
        verbs.push(format!("{invoke_count} invoke"));
    }
    if agent_count > 0 {
        verbs.push(format!("{agent_count} agent"));
    }
    println!("  {} {}", "Verbs:".bold(), verbs.join(", "));

    // Providers
    let provider_list: Vec<String> = providers.iter().map(|p| p.to_string()).collect();
    println!("  {} {}", "Providers:".bold(), provider_list.join(", "));

    // Model
    if let Some(ref model) = workflow.model {
        println!("  {} {}", "Model:".bold(), model);
    }

    // Estimated cost (rough: ~$0.003 per infer, ~$0.05 per agent turn)
    if llm_tasks > 0 {
        let est_cost = (infer_count as f64) * 0.003 + (agent_count as f64) * 0.05;
        println!(
            "  {} ~${:.2} ({llm_tasks} LLM calls)",
            "Est. cost:".bold(),
            est_cost
        );
    }

    // Required env vars
    let needs_key = providers
        .iter()
        .any(|p| !["mock", "native", "local"].contains(p));
    if needs_key {
        let env_vars: Vec<String> = providers
            .iter()
            .filter(|p| !["mock", "native", "local"].contains(*p))
            .map(|p| format!("{}_API_KEY", p.to_uppercase()))
            .collect();
        println!("  {} {}", "Required:".bold(), env_vars.join(", "));
    }

    println!();
    Ok(())
}
