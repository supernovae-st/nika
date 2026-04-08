// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika run` and `nika run --dry-run` command handlers.
//!
//! Orchestrates the full workflow execution pipeline: resolve path → parse →
//! validate → onboard → merge inputs → filter tasks → estimate cost →
//! render header → build Runner → resume → execute → save outputs.

use std::io::IsTerminal;
use std::path::Path;

use colored::Colorize;
use nika_engine::ast::parse_analyzed_with_includes;
use nika_engine::ast::schema_validator::WorkflowSchemaValidator;
use nika_engine::error::NikaError;
use nika_engine::runtime::Runner;

use crate::discover::resolve_workflow_path;
use crate::inputs::{load_input_file, parse_cli_inputs, parse_input_value, simple_input_resolve};
use crate::task_filter::{filter_tasks_for_target, filter_tasks_from};

/// Execute a workflow file end-to-end.
#[allow(clippy::too_many_arguments)]
pub async fn run_workflow(
    file: &str,
    provider_override: Option<String>,
    model_override: Option<String>,
    cli_inputs: &[String],
    input_file: Option<&str>,
    interactive: bool,
    output_file: Option<&str>,
    task_filter: Option<&str>,
    from_filter: Option<&str>,
    skip_confirm: bool,
    quiet: bool,
    detail: nika_engine::display::DetailLevel,
    no_live: bool,
    permission: &str,
    resume: bool,
) -> Result<(), NikaError> {
    // Bootstrap secrets: env vars → daemon IPC → vault.
    // Ensures keys stored via `nika keys set` are available without restarting the shell.
    let _ = nika_engine::secrets::load_from_daemon_or_fallback().await;

    let resolved_path = resolve_workflow_path(file).await?;

    let yaml = tokio::fs::read_to_string(&resolved_path).await?;

    let validator = WorkflowSchemaValidator::new()?;
    validator.validate_yaml(&yaml)?;

    let base_path = resolved_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));

    // Parse with include expansion: raw → expand_raw_include → analyze
    // This merges partial workflows BEFORE validation so cross-file references work.
    let mut workflow = parse_analyzed_with_includes(&yaml, base_path)?;

    // Auto-onboarding: if workflow needs an LLM and no API keys are set, run the wizard.
    // Skip for mock/native providers which don't need API keys.
    let is_mock_or_native = provider_override
        .as_deref()
        .or(workflow.provider.as_ref().map(|p| p.as_str()))
        .is_some_and(|p| {
            let lower = p.to_lowercase();
            lower == "mock" || lower == "native" || lower == "local"
        });
    let needs_llm = !is_mock_or_native
        && workflow.tasks.iter().any(|t| {
            matches!(
                t.action,
                nika_engine::ast::analyzed::AnalyzedTaskAction::Infer(_)
                    | nika_engine::ast::analyzed::AnalyzedTaskAction::Agent(_)
            )
        });
    if needs_llm
        && !crate::onboarding::skip_onboarding()
        && !crate::onboarding::has_any_provider_key()
    {
        let configured = crate::onboarding::run_onboarding_wizard().await?;
        if !configured {
            return Err(NikaError::ConfigError {
                reason:
                    "No API key configured. Run `nika keys set <provider> <key>` or `nika setup`."
                        .to_string(),
            });
        }
    }

    if let Some(p) = provider_override {
        workflow.provider = Some(p.into());
    }
    if let Some(m) = model_override {
        workflow.model = Some(m);
    }

    // Merge CLI input overrides (YAML defaults < file < CLI flags)
    if let Some(input_file_path) = input_file {
        let file_inputs = load_input_file(input_file_path).await?;
        for (k, v) in file_inputs {
            workflow.inputs.insert(k, v);
        }
    }
    if !cli_inputs.is_empty() {
        let parsed = parse_cli_inputs(cli_inputs)?;
        for (k, v) in parsed {
            workflow.inputs.insert(k, v);
        }
    }

    // Interactive prompts for inputs without defaults
    if interactive && std::io::stdin().is_terminal() {
        let keys: Vec<String> = workflow.inputs.keys().cloned().collect();
        for key in keys {
            let value = &workflow.inputs[&key];
            let has_value = match value {
                serde_json::Value::Null => false,
                serde_json::Value::Object(obj) => obj.contains_key("default"),
                _ => true,
            };
            if !has_value {
                let input: String = cliclack::input(format!("Enter value for '{}':", key))
                    .interact()
                    .map_err(|_| NikaError::ValidationError {
                        reason: format!("Input '{}' required but not provided", key),
                    })?;
                workflow.inputs.insert(key, parse_input_value(&input));
            }
        }
    }

    // Task filtering: --task (single task + deps) or --from (from task onwards)
    if let Some(target) = task_filter {
        filter_tasks_for_target(&mut workflow, target)?;
        if !quiet {
            eprintln!(
                "  {} running task '{}' + dependencies ({} tasks)",
                "Filter:".cyan(),
                target,
                workflow.tasks.len()
            );
        }
    } else if let Some(from_id) = from_filter {
        filter_tasks_from(&mut workflow, from_id)?;
        if !quiet {
            eprintln!(
                "  {} running from '{}' onwards ({} tasks)",
                "Filter:".cyan(),
                from_id,
                workflow.tasks.len()
            );
        }
    }

    // Cost estimation: warn if LLM tasks detected and not --yes
    if !skip_confirm && std::io::stdin().is_terminal() {
        let infer_count = workflow
            .tasks
            .iter()
            .filter(|t| matches!(t.action.verb_name(), "infer" | "agent"))
            .count();
        if infer_count > 0 {
            let provider_name = workflow
                .provider
                .as_ref()
                .map(|p| p.as_str())
                .unwrap_or("anthropic");
            let model_name = workflow
                .model
                .as_deref()
                .unwrap_or("claude-sonnet-4-20250514");
            if let Some(pk) = nika_engine::provider::cost::ProviderKind::parse(provider_name) {
                let avg_tokens = 2000u64;
                let est_cost = nika_engine::provider::cost::calculate_cost(
                    pk,
                    model_name,
                    avg_tokens * infer_count as u64,
                    avg_tokens * infer_count as u64,
                );
                if est_cost > 0.10 {
                    eprintln!(
                        "  {} ~${:.4} ({} LLM tasks, {})",
                        "Estimated cost:".yellow(),
                        est_cost,
                        infer_count,
                        model_name
                    );
                    let confirm: bool = cliclack::confirm("Continue?")
                        .initial_value(true)
                        .interact()
                        .unwrap_or(false);
                    if !confirm {
                        return Err(NikaError::ValidationError {
                            reason: "Cancelled by user".to_string(),
                        });
                    }
                }
            }
        }
    }

    if !quiet && !detail.is_json() {
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
        let depths = nika_engine::dag::flow::compute_layers(&nodes, &edges);
        let layer_count = nika_engine::dag::flow::layer_count(&depths);

        let gen_id = format!("{:08x}", rand::random::<u32>());
        nika_engine::display::header::print_header(
            workflow.name.as_deref(),
            workflow
                .provider
                .as_ref()
                .map(|p| p.as_str())
                .unwrap_or("(auto)"),
            workflow.model.as_deref().unwrap_or("(default)"),
            workflow.tasks.len(),
            layer_count,
            env!("CARGO_PKG_VERSION"),
            &gen_id,
        );

        // Inline DAG summary
        nika_engine::display::header::print_dag_summary(&nodes, &depths);

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
    // Load custom endpoints from config.toml for OpenAI-compatible servers
    let config = nika_engine::config::NikaConfig::load()
        .unwrap_or_default()
        .with_env();
    let mut runner = Runner::new(workflow)?
        .with_base_path(base_path.to_path_buf())
        .with_permission_mode(perm_mode);

    // Wire project root + working_dir mode from nika.toml so exec cwd,
    // nika:read security boundary, and from_example paths resolve correctly
    // when workflows live in subdirectories (e.g. workflows/*.nika.yaml).
    if let Ok(project) =
        crate::config::find_project_root_from(&std::env::current_dir().unwrap_or_default())
    {
        runner = runner.with_project_root(project.root.clone());
        if project.source == crate::config::ProjectRootSource::NikaToml {
            if let Some(bootstrap) = crate::config::load_project_config(&project.root) {
                if let Some(wd) = bootstrap.tools.working_dir {
                    runner = runner.with_working_dir_mode(wd);
                }
            }
        }
    }

    // Merge endpoints: project nika.toml wins, user config.toml fills gaps
    let mut all_endpoints = std::collections::HashMap::new();
    // Project-level endpoints go first (win on conflict)
    if let Ok(project) =
        crate::config::find_project_root_from(&std::env::current_dir().unwrap_or_default())
    {
        if let Some(bootstrap) = crate::config::load_project_config(&project.root) {
            all_endpoints = bootstrap.endpoints;
        }
    }
    // User-level endpoints fill gaps (don't override project)
    for (name, ep) in &config.endpoints {
        all_endpoints
            .entry(name.clone())
            .or_insert_with(|| ep.clone());
    }
    if !all_endpoints.is_empty() {
        if let Ok(resolved) = nika_engine::provider::endpoints::resolve_endpoints(&all_endpoints) {
            runner.with_custom_endpoints(resolved);
        }
    }
    if quiet {
        runner = runner.quiet();
    }
    let mut runner = if no_live {
        runner.with_classic_renderer(detail)
    } else {
        runner.with_detail_level(detail)
    };

    // Resume from last run: pre-populate datastore with completed tasks
    if resume {
        let traces = nika_engine::event::list_traces()?;
        if let Some(trace) = traces.first() {
            // Safety: reject oversized traces (consistent with SEC-4 50MB limit)
            let trace_size = std::fs::metadata(&trace.path).map(|m| m.len()).unwrap_or(0);
            if trace_size > 50 * 1024 * 1024 {
                if !quiet {
                    eprintln!(
                        "  {} trace too large ({} MB), running from scratch",
                        "Resume:".yellow(),
                        trace_size / (1024 * 1024)
                    );
                }
            } else {
                let content = std::fs::read_to_string(&trace.path)?;
                let mut resumed_count = 0u32;
                // Verify workflow identity via hash in first WorkflowStarted event
                let mut trace_hash: Option<String> = None;
                for line in content.lines() {
                    if let Ok(event) =
                        serde_json::from_str::<nika_engine::event::Event>(line)
                    {
                        match event.kind {
                            nika_engine::event::EventKind::WorkflowStarted {
                                workflow_hash,
                                ..
                            } => {
                                trace_hash = Some(workflow_hash);
                            }
                            nika_engine::event::EventKind::TaskCompleted {
                                task_id,
                                output,
                                duration_ms,
                            } => {
                                runner.datastore().insert(
                                    task_id,
                                    nika_engine::store::TaskResult::success(
                                        output.as_ref().clone(),
                                        std::time::Duration::from_millis(duration_ms),
                                    ),
                                );
                                resumed_count += 1;
                            }
                            _ => {}
                        }
                    }
                }
                // Warn if workflow changed since trace was recorded
                if let Some(ref th) = trace_hash {
                    let current_hash = nika_engine::event::calculate_workflow_hash(&yaml);
                    if *th != current_hash && !quiet {
                        eprintln!(
                            "  {} workflow changed since last run (trace hash mismatch)",
                            "Warning:".yellow()
                        );
                    }
                }
                if !quiet && resumed_count > 0 {
                    eprintln!(
                        "  {} resuming from {} ({} completed tasks cached)",
                        "Resume:".cyan(),
                        trace.generation_id,
                        resumed_count
                    );
                } else if !quiet && resumed_count == 0 {
                    eprintln!(
                        "  {} trace found ({}) but no completed tasks to resume",
                        "Resume:".yellow(),
                        trace.generation_id
                    );
                }
            }
        } else if !quiet {
            eprintln!(
                "  {} no previous trace found, running from scratch",
                "Resume:".yellow()
            );
        }
    }

    let run_output = runner.run().await?;

    // Save task outputs to file if -o/--output specified
    if let Some(output_path) = output_file {
        let results = runner.datastore().iter_results();
        let mut output_map = serde_json::Map::new();
        for (task_id, result) in &results {
            let mut task_obj = serde_json::Map::new();
            task_obj.insert("output".to_string(), (*result.output).clone());
            task_obj.insert(
                "status".to_string(),
                serde_json::json!(format!("{:?}", result.status)),
            );
            task_obj.insert(
                "duration_ms".to_string(),
                serde_json::json!(result.duration.as_millis() as u64),
            );
            output_map.insert(task_id.to_string(), serde_json::Value::Object(task_obj));
        }
        let json = serde_json::to_string_pretty(&serde_json::Value::Object(output_map))
            .unwrap_or_default();
        tokio::fs::write(output_path, &json)
            .await
            .map_err(|e| NikaError::ParseError {
                details: format!("Failed to write output file '{}': {}", output_path, e),
            })?;
        if !quiet {
            eprintln!("  {} {}", "Output saved:".green(), output_path);
        }
    }

    if !quiet && !run_output.is_empty() {
        println!("{}", "Output:".cyan().bold());
        println!("{run_output}");
    }

    Ok(())
}

/// Show execution plan without running anything.
#[allow(clippy::too_many_arguments)]
pub async fn dry_run_workflow(
    file: &str,
    provider_override: Option<String>,
    model_override: Option<String>,
    cli_inputs: &[String],
    input_file: Option<&str>,
    task_filter: Option<&str>,
    from_filter: Option<&str>,
) -> Result<(), NikaError> {
    let resolved_path = resolve_workflow_path(file).await?;
    let yaml = tokio::fs::read_to_string(&resolved_path).await?;
    let validator = WorkflowSchemaValidator::new()?;
    validator.validate_yaml(&yaml)?;
    let base_path = resolved_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut workflow = parse_analyzed_with_includes(&yaml, base_path)?;

    // Apply overrides
    if let Some(p) = provider_override {
        workflow.provider = Some(p.into());
    }
    if let Some(m) = model_override {
        workflow.model = Some(m);
    }
    if let Some(ifp) = input_file {
        let file_inputs = load_input_file(ifp).await?;
        for (k, v) in file_inputs {
            workflow.inputs.insert(k, v);
        }
    }
    if !cli_inputs.is_empty() {
        let parsed = parse_cli_inputs(cli_inputs)?;
        for (k, v) in parsed {
            workflow.inputs.insert(k, v);
        }
    }

    // Apply task filters
    if let Some(target) = task_filter {
        filter_tasks_for_target(&mut workflow, target)?;
    } else if let Some(from_id) = from_filter {
        filter_tasks_from(&mut workflow, from_id)?;
    }

    // Header
    println!("\n  {}", "DRY RUN — no tasks will execute".yellow().bold());
    println!();

    // Show resolved inputs
    if !workflow.inputs.is_empty() {
        println!("  {}", "Inputs:".bold());
        for (k, v) in &workflow.inputs {
            println!(
                "    {} = {}",
                k.cyan(),
                serde_json::to_string(v).unwrap_or_default()
            );
        }
        println!();
    }

    // Compute DAG layers
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
    let depths = nika_engine::dag::flow::compute_layers(&nodes, &edges);
    let max_layer = depths.values().max().copied().unwrap_or(0);

    // Execution plan
    println!("  {}", "Execution Plan:".bold());
    for layer in 0..=max_layer {
        let mut tasks_in_layer: Vec<&str> = depths
            .iter()
            .filter(|(_, &d)| d == layer)
            .map(|(&name, _)| name)
            .collect();
        tasks_in_layer.sort();
        println!(
            "    Layer {} (parallel): {}",
            layer,
            tasks_in_layer.join(", ").cyan()
        );
    }
    println!();

    // Per-task details
    let default_provider = workflow
        .provider
        .as_ref()
        .map(|p| p.as_str())
        .unwrap_or("(auto)");
    let default_model = workflow.model.as_deref().unwrap_or("(default)");

    println!("  {}", "Tasks:".bold());
    for task in &workflow.tasks {
        let verb = task.action.verb_name();
        let resolved_provider = task
            .provider
            .as_ref()
            .map(|p| simple_input_resolve(p.as_str(), &workflow.inputs))
            .unwrap_or_else(|| default_provider.to_string());
        let resolved_model = task
            .model
            .as_deref()
            .map(|m| simple_input_resolve(m, &workflow.inputs))
            .unwrap_or_else(|| default_model.to_string());
        let provider = resolved_provider.as_str();
        let model = resolved_model.as_str();
        let deps: Vec<&str> = task
            .depends_on
            .iter()
            .filter_map(|id| workflow.task_table.get_name(*id))
            .collect();
        let dep_str = if deps.is_empty() {
            String::new()
        } else {
            format!(" deps=[{}]", deps.join(", "))
        };
        println!(
            "    {} [{}] provider={} model={}{}",
            task.name.cyan(),
            verb,
            provider.dimmed(),
            model.dimmed(),
            dep_str.dimmed()
        );
    }
    println!();

    // LLM task count + cost estimate
    let llm_tasks: Vec<_> = workflow
        .tasks
        .iter()
        .filter(|t| matches!(t.action.verb_name(), "infer" | "agent"))
        .collect();
    let infer_count = llm_tasks.len();
    if infer_count > 0 {
        use nika_engine::provider::cost::{calculate_cost, ProviderKind};
        // Estimate: ~500 input tokens, ~1000 output tokens per LLM task
        let mut total_cost = 0.0;
        for task in &llm_tasks {
            let prov_resolved = task
                .provider
                .as_ref()
                .map(|p| simple_input_resolve(p.as_str(), &workflow.inputs))
                .or_else(|| workflow.provider.as_ref().map(|p| p.to_string()))
                .unwrap_or_else(|| "anthropic".to_string());
            let model_resolved = task
                .model
                .as_deref()
                .map(|m| simple_input_resolve(m, &workflow.inputs))
                .or_else(|| workflow.model.clone())
                .unwrap_or_else(|| "claude-sonnet-4-6".to_string());
            let prov_str = prov_resolved.as_str();
            let model_str = model_resolved.as_str();
            let provider_kind = ProviderKind::parse(prov_str).unwrap_or(ProviderKind::Claude);
            total_cost += calculate_cost(provider_kind, model_str, 500, 1000);
        }
        println!(
            "  {} {} LLM tasks, {} total — estimated cost: ${:.4}",
            "Summary:".bold(),
            infer_count,
            workflow.tasks.len(),
            total_cost
        );
    } else {
        println!(
            "  {} {} tasks (no LLM calls)",
            "Summary:".bold(),
            workflow.tasks.len()
        );
    }
    println!();

    Ok(())
}
