// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Analyzer: raw::Workflow → analyzed::Workflow
//!
//! This is the core Phase 2 transformation that:
//! 1. Validates schema version
//! 2. Builds task table (interned IDs)
//! 3. Resolves all task references (`with:` bindings → WithSpec, `depends_on:` → `Vec<TaskId>`)
//! 4. Extracts implicit dependencies from WithEntry task references
//! 5. Resolves `include:` specifications
//! 6. Detects cyclic dependencies
//! 7. Collects all errors with precise spans

mod cycle_detection;
mod task_table;
mod validation;
mod verb_analysis;

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use super::errors::{AnalyzeError, AnalyzeErrorKind, AnalyzeResult};
use super::suggestions::find_similar;
use crate::ast::analyzed::{
    AnalyzedAgentAction, AnalyzedContextFile, AnalyzedExecAction, AnalyzedFetchAction,
    AnalyzedForEach, AnalyzedIncludeSpec, AnalyzedInferAction, AnalyzedInvokeAction,
    AnalyzedMcpServer, AnalyzedOnError, AnalyzedOutput, AnalyzedRetry, AnalyzedTask,
    AnalyzedTaskAction, AnalyzedWorkflow, HttpMethod, McpFromSource, McpTransport, OnErrorAction,
    OutputFormat, TaskId, TaskTable,
};
use crate::ast::raw::{
    RawAgentAction, RawExecAction, RawFetchAction, RawInferAction, RawInvokeAction, RawTask,
    RawTaskAction, RawWorkflow,
};
use crate::ast::schema::SchemaVersion;
use crate::ast::templatable::Templatable;
use crate::binding::{parse_with_entry, WithSpec};
use crate::catalogs::ModelResolver;
use crate::source::{Span, Spanned};

// Import submodule functions
use cycle_detection::*;
use task_table::*;
use validation::*;
use verb_analysis::*;

/// Analyzer context - holds state during analysis.
struct AnalyzerContext {
    /// Task name to ID mapping
    task_table: TaskTable,
    /// Task name to span mapping (for duplicate detection)
    task_spans: HashMap<String, Span>,
    /// Prefixes declared in `include:` — tasks matching these are resolved post-analysis
    include_prefixes: Vec<String>,
    /// Collected errors
    errors: Vec<AnalyzeError>,
    /// Collected warnings
    warnings: Vec<AnalyzeError>,
}

impl AnalyzerContext {
    fn new() -> Self {
        Self {
            task_table: TaskTable::new(),
            task_spans: HashMap::new(),
            include_prefixes: Vec::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    fn add_error(&mut self, error: AnalyzeError) {
        self.errors.push(error);
    }

    fn add_warning(&mut self, warning: AnalyzeError) {
        self.warnings.push(warning);
    }

    /// Check if a task name matches a declared include prefix.
    ///
    /// When `include:` declares `prefix: seo_`, references like `$seo_generate_title`
    /// should not be flagged as unknown — they'll be resolved after DAG fusion.
    fn is_included_task(&self, task_name: &str) -> bool {
        self.include_prefixes
            .iter()
            .any(|prefix| task_name.starts_with(prefix))
    }
}

/// Validate a raw workflow without producing an analyzed workflow.
///
/// Runs all validation checks (schema, feature gates, duplicate tasks,
/// unknown task references, invalid bindings, cycle detection) and returns
/// collected errors. This is cheaper than `analyze()` when you only need
/// to check if a workflow is valid.
///
/// # Arguments
///
/// * `raw` - The raw workflow from Phase 1 parsing
///
/// # Returns
///
/// An `AnalyzeResult<()>` — `Ok(())` if valid, or errors if not.
///
/// # Example
///
/// ```ignore
/// let raw = raw::parse(&source, file_id)?;
/// let result = validate(&raw);
/// if result.is_err() {
///     for error in &result.errors {
///         eprintln!("{}: {}", error.kind.code(), error);
///     }
/// }
/// ```
pub fn validate(raw: &RawWorkflow) -> AnalyzeResult<()> {
    let mut ctx = AnalyzerContext::new();

    // 1. Validate schema version
    let version = analyze_schema(raw, &mut ctx).unwrap_or(SchemaVersion::V01);

    // 2. Validate version-gated features
    validate_feature_gates(raw, version, &mut ctx);

    // 2.5. Collect include prefixes (tasks with these prefixes are resolved post-analysis)
    collect_include_prefixes(raw, &mut ctx);

    // 2b. Validate workflow-level model name is recognized
    // Only warn when no explicit provider — with explicit provider, user knows which
    // provider serves the model (e.g. llama models on groq).
    if raw.provider.is_none() {
        if let Some(ref wf_model) = raw.model {
            check_model_name(wf_model, &mut ctx);
        }
    }

    // 2b2. Validate provider names are recognized
    let all_provider_names: Vec<&str> = crate::catalogs::KNOWN_PROVIDERS
        .iter()
        .flat_map(|p| std::iter::once(p.id).chain(p.aliases.iter().copied()))
        .collect();
    // Check workflow-level provider
    if let Some(ref wf_provider) = raw.provider {
        if crate::catalogs::find_provider(&wf_provider.value).is_none() {
            let suggestion = find_similar(&wf_provider.value, &all_provider_names, 0.7);
            let mut err = AnalyzeError::new(
                AnalyzeErrorKind::InvalidValue,
                wf_provider.span,
                format!("unknown provider '{}'", wf_provider.value),
            );
            if let Some(ref similar) = suggestion {
                err = err.with_suggestion(format!("did you mean '{}'?", similar));
            }
            ctx.errors.push(err);
        }
    }
    // 2c. Validate task-level fields (provider, model, invoke, for_each, misplaced LLM fields)
    let has_workflow_model = raw.model.is_some();
    let workflow_provider = raw
        .provider
        .as_ref()
        .map(|p| p.value.as_str())
        .unwrap_or("");
    for raw_task in &raw.tasks.value {
        let task = &raw_task.value;

        // Check task-level provider name is recognized
        if let Some(ref task_provider) = task.provider {
            if crate::catalogs::find_provider(&task_provider.value).is_none() {
                let suggestion = find_similar(&task_provider.value, &all_provider_names, 0.7);
                let mut err = AnalyzeError::new(
                    AnalyzeErrorKind::InvalidValue,
                    task_provider.span,
                    format!("unknown provider '{}'", task_provider.value),
                );
                if let Some(ref similar) = suggestion {
                    err = err.with_suggestion(format!("did you mean '{}'?", similar));
                }
                ctx.errors.push(err);
            }
        }

        let uses_llm = task
            .action
            .as_ref()
            .is_some_and(|a| matches!(&a, RawTaskAction::Infer(_) | RawTaskAction::Agent(_)));
        let has_task_model = task.model.is_some();
        let has_preset = task.preset.is_some();
        let provider_name = task
            .provider
            .as_ref()
            .map(|p| p.value.as_str())
            .unwrap_or(workflow_provider);
        let is_mock = provider_name == "mock";

        if uses_llm && !has_workflow_model && !has_task_model && !has_preset && !is_mock {
            let span = task.id.span;
            ctx.errors.push(AnalyzeError {
                kind: AnalyzeErrorKind::MissingModel,
                span,
                message: format!(
                    "Task '{}' uses {} verb but no model is specified. \
                     Add `model:` at workflow level or on this task. \
                     Example: model: gpt-4o-mini",
                    task.id.value,
                    if matches!(task.action.as_ref(), Some(RawTaskAction::Agent(_))) {
                        "agent:"
                    } else {
                        "infer:"
                    }
                ),
                suggestion: Some(
                    "Add `model: gpt-4o-mini` after `provider:` in your workflow".to_string(),
                ),
                note: None,
            });
        }

        // Check task-level model name is recognized
        // Only warn when no explicit provider on this task or inherited from workflow.
        if task.provider.is_none() && raw.provider.is_none() {
            if let Some(ref task_model) = task.model {
                check_model_name(task_model, &mut ctx);
            }
        }

        // Check invoke: nika:xxx tool names are valid builtins
        if let Some(RawTaskAction::Invoke(ref invoke)) = task.action {
            if let Some(ref tool) = invoke.value.tool {
                if let Some(suffix) = tool.value.strip_prefix("nika:") {
                    if !crate::catalogs::builtins::is_known_builtin(suffix) {
                        let builtins = crate::catalogs::builtins::KNOWN_BUILTIN_TOOLS;
                        let suggestion = find_similar(suffix, builtins, 0.7);
                        let mut err = AnalyzeError::new(
                            AnalyzeErrorKind::InvalidValue,
                            tool.span,
                            format!(
                                "unknown builtin tool 'nika:{}'. \
                                 There are {} registered nika:* tools.",
                                suffix,
                                builtins.len()
                            ),
                        );
                        if let Some(ref similar) = suggestion {
                            err = err.with_suggestion(format!("did you mean 'nika:{}'?", similar));
                        }
                        ctx.errors.push(err);
                    }
                }
            }
        }

        // Check for task-level LLM fields that are silently ignored with full-form infer
        if !task.misplaced_llm_fields.is_empty() {
            ctx.add_warning(AnalyzeError::new(
                AnalyzeErrorKind::InvalidValue,
                task.id.span,
                format!(
                    "task '{}': {} at task level {} ignored with full infer: block form. \
                     Move {} inside the infer: block.",
                    task.id.value,
                    task.misplaced_llm_fields.join(", "),
                    if task.misplaced_llm_fields.len() == 1 {
                        "is"
                    } else {
                        "are"
                    },
                    if task.misplaced_llm_fields.len() == 1 {
                        "it"
                    } else {
                        "them"
                    },
                ),
            ));
        }

        // for_each concurrency hints are handled by lint L031 — not duplicated here.

        // Validate infer-specific fields
        if let Some(RawTaskAction::Infer(ref infer)) = task.action {
            check_infer_fields(&infer.value, &mut ctx);
        }
        if let Some(RawTaskAction::Agent(ref agent)) = task.action {
            check_agent_infer_fields(&agent.value, &mut ctx);
        }

        // Validate fetch extract modes that require selector:
        if let Some(RawTaskAction::Fetch(ref fetch)) = task.action {
            if let Some(ref extract) = fetch.value.extract {
                let mode = extract.value.as_str();
                if (mode == "selector" || mode == "jsonpath") && fetch.value.selector.is_none() {
                    ctx.add_error(AnalyzeError::new(
                        AnalyzeErrorKind::MissingField,
                        extract.span,
                        format!(
                            "extract: {} requires a selector: field. {}",
                            mode,
                            if mode == "jsonpath" {
                                "Add selector: with a JSONPath expression (e.g. $.data[0].name)"
                            } else {
                                "Add selector: with a CSS selector (e.g. main article)"
                            }
                        ),
                    ));
                }
            }
        }
    }

    // 3. Build task table (detect duplicates)
    build_task_table(&raw.tasks.value, &mut ctx);

    // 4. Validate task references (unknown tasks in with:/depends_on:, invalid bindings)
    let task_table = ctx.task_table.clone();
    let task_names: Vec<String> = task_table.iter().map(|(_, n)| n.to_string()).collect();
    for raw_task in raw.tasks.value.iter() {
        validate_task_refs(&raw_task.value, &task_table, &task_names, &mut ctx);
    }

    // 4b. Validate preset references in validate() path
    let agent_names: Vec<String> = raw
        .agents
        .as_ref()
        .and_then(|a| {
            if let serde_json::Value::Object(map) = &a.value {
                Some(map.keys().cloned().collect())
            } else {
                None
            }
        })
        .unwrap_or_default();
    for raw_task in &raw.tasks.value {
        if let Some(ref preset) = raw_task.value.preset {
            if !agent_names.contains(&preset.value) {
                let hint = if agent_names.is_empty() {
                    "Add an `agents:` block to the workflow header".to_string()
                } else {
                    format!("Available presets: {}", agent_names.join(", "))
                };
                ctx.errors.push(AnalyzeError {
                    kind: AnalyzeErrorKind::InvalidValue,
                    span: preset.span,
                    message: format!(
                        "Task '{}' references preset '{}' which is not defined in the agents: block",
                        raw_task.value.id.value, preset.value
                    ),
                    suggestion: Some(hint),
                    note: None,
                });
            }
        }
    }

    // 4c. Validate template references (G4, G5, G6)
    validate_template_refs(raw, &mut ctx);

    // 5. Detect cyclic dependencies (requires building a lightweight dep graph)
    detect_cycles_from_raw(&raw.tasks.value, &task_table, &mut ctx);

    if ctx.errors.is_empty() {
        let mut result = AnalyzeResult::ok(());
        result.warnings = ctx.warnings;
        result
    } else {
        let mut result = AnalyzeResult::err(ctx.errors);
        result.warnings = ctx.warnings;
        result
    }
}

/// Analyze a raw workflow into a resolved, validated workflow.
///
/// This is the main entry point for Phase 2 analysis.
///
/// # Arguments
///
/// * `raw` - The raw workflow from Phase 1 parsing
///
/// # Returns
///
/// An AnalyzeResult containing either the analyzed workflow or collected errors.
///
/// # Example
///
/// ```ignore
/// let raw = raw::parse(&source, file_id)?;
/// let result = analyze(raw);
/// if result.is_ok() {
///     let workflow = result.value.unwrap();
///     // Use analyzed workflow
/// } else {
///     for error in &result.errors {
///         eprintln!("{}: {}", error.kind.code(), error);
///     }
/// }
/// ```
pub fn analyze(raw: RawWorkflow) -> AnalyzeResult<AnalyzedWorkflow> {
    let mut ctx = AnalyzerContext::new();

    // Collect include prefixes early (before raw fields are moved)
    collect_include_prefixes(&raw, &mut ctx);

    let mut workflow = AnalyzedWorkflow {
        span: raw.span,
        ..Default::default()
    };

    // 1. Validate schema version
    if let Some(version) = analyze_schema(&raw, &mut ctx) {
        workflow.schema_version = version;
    }

    // 2. Validate version-gated features
    validate_feature_gates(&raw, workflow.schema_version, &mut ctx);

    // 3. Extract metadata
    workflow.name = raw.workflow.as_ref().map(|s| s.value.clone());
    workflow.description = raw.description.map(|s| s.value);
    workflow.goal = raw.goal.map(|s| s.value);
    workflow.provider = raw.provider.map(|s| crate::ProviderName::parse(&s.value));
    workflow.model = raw.model.map(|s| s.value);
    // 3a. Parse orchestrate configuration (with bounds validation)
    workflow.orchestrate = raw.orchestrate.as_ref().and_then(|s| {
        match serde_json::from_value::<crate::ast::orchestrate::OrchestrateConfig>(s.value.clone())
        {
            Ok(mut config) => {
                config.validate();
                Some(config)
            }
            Err(e) => {
                tracing::warn!("Invalid orchestrate: config: {e}");
                None
            }
        }
    });

    // 3b. Parse routing configuration
    workflow.routing = raw.routing.as_ref().and_then(|s| {
        match serde_json::from_value::<crate::ast::routing::RoutingConfig>(s.value.clone()) {
            Ok(config) => Some(config),
            Err(e) => {
                tracing::warn!("Invalid routing: config: {e}");
                None
            }
        }
    });

    // 3c. Parse schedule configuration (recurring execution)
    workflow.schedule = raw.schedule.as_ref().and_then(|s| {
        match crate::ast::schedule::parse_schedule_value(&s.value, s.span) {
            Ok(config) => Some(config),
            Err(e) => {
                ctx.add_error(AnalyzeError::new(
                    AnalyzeErrorKind::InvalidValue,
                    s.span,
                    format!("invalid schedule: {e}"),
                ));
                None
            }
        }
    });

    // 3d. Global workflow timeout (default: 3600s = 1h)
    workflow.max_duration_secs = raw
        .max_duration_secs
        .as_ref()
        .map(|s| s.value.clone())
        .unwrap_or(Templatable::Value(3600));

    // 3b. Validate model is specified when LLM verbs are used
    let has_workflow_model = workflow.model.is_some();
    for raw_task in &raw.tasks.value {
        let task = &raw_task.value;
        let uses_llm = task
            .action
            .as_ref()
            .is_some_and(|a| matches!(a, RawTaskAction::Infer(_) | RawTaskAction::Agent(_)));
        let has_task_model = task.model.is_some();
        let has_preset = task.preset.is_some();

        // Provider 'mock' is exempt (no real API calls)
        let provider_name = task
            .provider
            .as_ref()
            .map(|p| p.value.as_str())
            .or(workflow.provider.as_ref().map(|p| p.as_str()))
            .unwrap_or("");
        let is_mock = provider_name == "mock";

        if uses_llm && !has_workflow_model && !has_task_model && !has_preset && !is_mock {
            let span = task.id.span;
            ctx.errors.push(AnalyzeError {
                kind: AnalyzeErrorKind::MissingModel,
                span,
                message: format!(
                    "Task '{}' uses {} verb but no model is specified. \
                     Add `model:` at workflow level or on this task. \
                     Example: model: gpt-4o-mini",
                    task.id.value,
                    if matches!(task.action.as_ref(), Some(RawTaskAction::Agent(_))) {
                        "agent:"
                    } else {
                        "infer:"
                    }
                ),
                suggestion: Some(
                    "Add `model: gpt-4o-mini` after `provider:` in your workflow".to_string(),
                ),
                note: None,
            });
        }
    }

    // 3c. Validate empty prompts (waste API credits)
    for raw_task in &raw.tasks.value {
        let task = &raw_task.value;
        if let Some(RawTaskAction::Infer(ref infer)) = task.action {
            if infer.value.prompt.value.trim().is_empty() && infer.value.content.is_none() {
                ctx.errors.push(AnalyzeError {
                    kind: AnalyzeErrorKind::MissingField,
                    span: task.id.span,
                    message: format!(
                        "Task '{}' has an empty prompt. Add a prompt or use content: for multimodal.",
                        task.id.value,
                    ),
                    suggestion: Some("Add a non-empty prompt".to_string()),
                    note: None,
                });
            }
        }
        if let Some(RawTaskAction::Agent(ref agent)) = task.action {
            if agent.value.prompt.value.trim().is_empty() {
                ctx.errors.push(AnalyzeError {
                    kind: AnalyzeErrorKind::MissingField,
                    span: task.id.span,
                    message: format!("Task '{}' has an empty agent prompt.", task.id.value,),
                    suggestion: Some(
                        "Add a non-empty prompt describing the agent's goal".to_string(),
                    ),
                    note: None,
                });
            }
        }
    }

    // 4. Analyze MCP server configurations
    if let Some(ref mcp) = raw.mcp {
        for (name_spanned, server_spanned) in &mcp.value.servers {
            let analyzed_server = analyze_mcp_server(
                &name_spanned.value,
                &server_spanned.value,
                server_spanned.span,
                &mut ctx,
            );
            workflow
                .mcp_servers
                .insert(name_spanned.value.clone(), analyzed_server);
        }
    }

    // 5. Analyze include
    if let Some(ref include) = raw.include {
        for include_spanned in &include.value {
            let spec = &include_spanned.value;
            workflow.include.push(AnalyzedIncludeSpec {
                path: spec.path.value.clone(),
                prefix: spec.prefix.as_ref().map(|s| s.value.clone()),
                span: include_spanned.span,
            });
        }
    }

    // 6. Analyze inputs
    if let Some(ref inputs) = raw.inputs {
        for (key_spanned, val_spanned) in &inputs.value {
            workflow
                .inputs
                .insert(key_spanned.value.clone(), val_spanned.value.clone());
        }
    }

    // 6a. Analyze context files
    if let Some(ref context) = raw.context {
        if let Some(ref files) = context.value.files {
            for (alias_spanned, path_spanned) in files {
                workflow.context_files.push(AnalyzedContextFile {
                    path: path_spanned.value.clone(),
                    alias: Some(alias_spanned.value.clone()),
                    max_bytes: None,
                    span: path_spanned.span,
                });
            }
        }
    }

    // 6b. Parse workflow-level artifacts config
    if let Some(ref artifacts_spanned) = raw.artifacts {
        if let Ok(config) = serde_json::from_value(artifacts_spanned.value.clone()) {
            workflow.artifacts = Some(config);
        }
    }

    // 6c. Parse workflow-level log config
    if let Some(ref log_spanned) = raw.log {
        match serde_json::from_value(log_spanned.value.clone()) {
            Ok(config) => workflow.log = Some(config),
            Err(e) => tracing::warn!(error = %e, "Failed to parse workflow log: config"),
        }
    }

    // 6d. Parse workflow-level agents config
    if let Some(ref agents_spanned) = raw.agents {
        if let serde_json::Value::Object(map) = &agents_spanned.value {
            let mut agents = indexmap::IndexMap::new();
            for (name, def_value) in map {
                match serde_json::from_value(def_value.clone()) {
                    Ok(def) => {
                        agents.insert(name.clone(), def);
                    }
                    Err(e) => tracing::warn!(
                        agent = %name,
                        error = %e,
                        "Failed to parse agent definition"
                    ),
                }
            }
            if !agents.is_empty() {
                workflow.agents = Some(agents);
            }
        }
    }

    // 6d-ii. Carry workflow-level skills mapping (alias -> path)
    if let Some(ref skills_spanned) = raw.skills {
        for (alias_spanned, path_spanned) in &skills_spanned.value {
            workflow
                .skills_map
                .insert(alias_spanned.value.clone(), path_spanned.value.clone());
        }
    }

    // 6e. Validate tasks array is non-empty
    if raw.tasks.value.is_empty() {
        ctx.errors.push(AnalyzeError::new(
            AnalyzeErrorKind::InvalidValue,
            raw.tasks.span,
            "tasks array must not be empty; workflow requires at least one task",
        ));
    }

    // 7. Build task table (first pass - collect all task IDs)
    build_task_table(&raw.tasks.value, &mut ctx);

    // 8. Analyze each task (second pass - resolve references)
    // Clone task_table to avoid borrow checker issues
    let task_table = ctx.task_table.clone();
    let task_names: Vec<String> = task_table.iter().map(|(_, n)| n.to_string()).collect();
    for raw_task in raw.tasks.value.iter() {
        if let Some(analyzed_task) =
            analyze_task(&raw_task.value, &task_table, &task_names, &mut ctx)
        {
            workflow.tasks.push(analyzed_task);
        }
    }

    // Copy task table to workflow BEFORE cycle detection so that
    // 8b. Validate preset references
    for task in &workflow.tasks {
        if let Some(ref preset_name) = task.preset {
            let agents_has_key = workflow
                .agents
                .as_ref()
                .is_some_and(|a| a.contains_key(preset_name));
            if !agents_has_key {
                let available: Vec<String> = workflow
                    .agents
                    .as_ref()
                    .map(|a| a.keys().cloned().collect())
                    .unwrap_or_default();
                let hint = if available.is_empty() {
                    "Add an `agents:` block to the workflow header".to_string()
                } else {
                    format!("Available presets: {}", available.join(", "))
                };
                ctx.errors.push(AnalyzeError {
                    kind: AnalyzeErrorKind::InvalidValue,
                    span: task.span,
                    message: format!(
                        "Task '{}' references preset '{}' which is not defined in the agents: block",
                        task.name, preset_name
                    ),
                    suggestion: Some(hint),
                    note: None,
                });
            }
        }
    }

    // detect_cycles_dfs can resolve TaskId → name via workflow.task_table.
    // Previously the table was copied AFTER detection, leaving it empty and
    // producing error messages like "cyclic dependency detected: " with no names.
    workflow.task_table = ctx.task_table.clone();

    // 9. Detect cyclic dependencies
    detect_cycles(&workflow, &mut ctx);

    // 10. Detect artifact path collisions (static paths only)
    detect_artifact_collisions(&workflow, &mut ctx);

    // Build result
    if ctx.errors.is_empty() {
        let mut result = AnalyzeResult::ok(workflow);
        result.warnings = ctx.warnings;
        result
    } else {
        let mut result = AnalyzeResult::err(ctx.errors);
        result.warnings = ctx.warnings;
        result
    }
}

/// Analyze a single task.
///
/// Parses `with:` binding expressions via `parse_with_entry()`, resolves `depends_on:`
/// task names to `TaskId`, and auto-extracts implicit dependencies from task bindings.
fn analyze_task(
    raw: &RawTask,
    task_table: &TaskTable,
    all_task_names: &[String],
    ctx: &mut AnalyzerContext,
) -> Option<AnalyzedTask> {
    let task_id = task_table.get_id(&raw.id.value)?;

    let mut task = AnalyzedTask {
        id: task_id,
        name: raw.id.value.clone(),
        description: raw.description.as_ref().map(|s| s.value.clone()),
        action: AnalyzedTaskAction::default(),
        provider: raw
            .provider
            .as_ref()
            .map(|s| crate::ProviderName::parse(&s.value)),
        model: raw.model.as_ref().map(|s| s.value.clone()),
        preset: raw.preset.as_ref().map(|s| s.value.clone()),
        with_spec: WithSpec::default(),
        depends_on: Vec::new(),
        implicit_deps: Vec::new(),
        output: None,
        for_each: raw
            .for_each
            .as_ref()
            .map(|f| analyze_for_each(&f.value, f.span)),
        retry: raw.retry.as_ref().map(|r| analyze_retry(&r.value, r.span)),
        on_error: None, // Resolved below after task_table lookups
        decompose: raw.decompose.as_ref().map(|d| d.value.clone()),
        concurrency: raw.concurrency.as_ref().map(|s| s.value.clone()),
        fail_fast: raw.fail_fast.as_ref().map(|s| s.value.clone()),
        artifact: raw.artifact.as_ref().and_then(|s| {
            match serde_json::from_value(s.value.clone()) {
                Ok(spec) => Some(spec),
                Err(e) => {
                    tracing::warn!(
                        task_id = %raw.id.value,
                        error = %e,
                        "Failed to parse artifact config, ignoring"
                    );
                    None
                }
            }
        }),
        log: raw
            .log
            .as_ref()
            .and_then(|s| match serde_json::from_value(s.value.clone()) {
                Ok(config) => Some(config),
                Err(e) => {
                    tracing::warn!(
                        task_id = %raw.id.value,
                        error = %e,
                        "Failed to parse log: config, ignoring"
                    );
                    None
                }
            }),
        structured: raw.structured.clone(),
        routing: raw.routing.as_ref().and_then(|s| {
            match serde_json::from_value::<crate::ast::routing::RoutingConfig>(s.value.clone()) {
                Ok(config) => Some(config),
                Err(e) => {
                    tracing::warn!(
                        task_id = %raw.id.value,
                        error = %e,
                        "Failed to parse routing: config, ignoring"
                    );
                    None
                }
            }
        }),
        record: raw.record.as_ref().and_then(|s| {
            // Shorthand: record: true → RecordSpec { compress: true, ..default }
            if let Some(b) = s.value.as_bool() {
                if b {
                    Some(crate::ast::record::RecordSpec::shorthand_true())
                } else {
                    None
                }
            } else {
                match serde_json::from_value::<crate::ast::record::RecordSpec>(s.value.clone()) {
                    Ok(spec) => {
                        if let Err(e) = spec.validate() {
                            ctx.errors.push(AnalyzeError {
                                kind: AnalyzeErrorKind::InvalidValue,
                                span: s.span,
                                message: format!("record: {e}"),
                                suggestion: None,
                                note: None,
                            });
                            None
                        } else {
                            Some(spec)
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            task_id = %raw.id.value,
                            error = %e,
                            "Failed to parse record: config, ignoring"
                        );
                        None
                    }
                }
            }
        }),
        context_budget: raw.context_budget.as_ref().and_then(|s| {
            // Only validate concrete values; templates are resolved at runtime
            if let Templatable::Value(val) = &s.value {
                if *val == 0 || *val > 200_000 {
                    ctx.errors.push(AnalyzeError {
                        kind: AnalyzeErrorKind::InvalidValue,
                        span: s.span,
                        message: format!("context_budget must be between 1 and 200000, got {val}"),
                        suggestion: Some("Use a value like 4000 (roughly 16K chars)".to_string()),
                        note: None,
                    });
                    return None;
                }
            }
            Some(s.value.clone())
        }),
        when: raw.when.as_ref().map(|s| s.value.clone()),
        trust_elevated: match raw.trust.as_ref() {
            Some(s) if s.value == "elevated" => true,
            Some(s) => {
                ctx.errors.push(AnalyzeError::new(
                    AnalyzeErrorKind::InvalidValue,
                    s.span,
                    format!("invalid trust value '{}', expected 'elevated'", s.value),
                ));
                false
            }
            None => false,
        },
        span: raw.span,
    };

    // Analyze action — reject tasks with no verb
    if let Some(ref action) = raw.action {
        task.action = analyze_action(action, ctx);

        // Merge agent-block provider/model into task if not set at task level
        if let RawTaskAction::Agent(ref agent_spanned) = action {
            let agent = &agent_spanned.value;
            if task.provider.is_none() {
                task.provider = agent
                    .provider
                    .as_ref()
                    .map(|s| crate::ProviderName::parse(&s.value));
            }
            if task.model.is_none() {
                task.model = agent.model.as_ref().map(|s| s.value.clone());
            }
        }
    } else {
        ctx.errors.push(AnalyzeError {
            kind: AnalyzeErrorKind::MissingField,
            span: raw.span,
            message: format!(
                "Task '{}' has no verb (infer:, exec:, fetch:, invoke:, or agent:)",
                raw.id.value
            ),
            note: None,
            suggestion: Some("Add one of: infer:, exec:, fetch:, invoke:, or agent:".to_string()),
        });
        return None;
    }

    // Parse with: bindings
    if let Some(ref with_refs) = raw.with_refs {
        for (alias_spanned, value_spanned) in with_refs.value.iter() {
            let alias = &alias_spanned.value;
            let expr = &value_spanned.value;

            match parse_with_entry(expr) {
                Ok(entry) => {
                    // Extract implicit dependency if this binding references a task
                    if let Some(dep_task_name) = entry.task_id() {
                        if let Some(dep_id) = task_table.get_id(dep_task_name) {
                            // Deduplicate implicit deps
                            if !task.implicit_deps.contains(&dep_id) {
                                task.implicit_deps.push(dep_id);
                            }
                        } else if !ctx.is_included_task(dep_task_name)
                            && dep_task_name != crate::ast::PARENT_CONTEXT_BINDING
                        {
                            // Unknown task reference in with: binding
                            // (skip include-prefixed tasks and the
                            //  runtime-injected __parent_context__ binding)
                            let all_names: Vec<&str> =
                                all_task_names.iter().map(|s| s.as_str()).collect();
                            let suggestion = find_similar(dep_task_name, &all_names, 0.6);
                            ctx.add_error(AnalyzeError::unknown_task(
                                value_spanned.span,
                                dep_task_name,
                                suggestion.as_deref(),
                            ));
                        }
                    }
                    task.with_spec.insert(alias.clone(), entry);
                }
                Err(parse_err) => {
                    ctx.add_error(AnalyzeError::invalid_binding(
                        value_spanned.span,
                        expr,
                        &parse_err.reason,
                    ));
                }
            }
        }
    }

    // Resolve depends_on: task names to TaskId
    if let Some(ref depends_on) = raw.depends_on {
        for dep_spanned in &depends_on.value {
            let dep_name = &dep_spanned.value;
            if let Some(dep_id) = task_table.get_id(dep_name) {
                task.depends_on.push(dep_id);
            } else if !ctx.is_included_task(dep_name) {
                // Unknown task in depends_on (skip for include-prefixed tasks)
                let all_names: Vec<&str> = all_task_names.iter().map(|s| s.as_str()).collect();
                let suggestion = find_similar(dep_name, &all_names, 0.6);
                ctx.add_error(AnalyzeError::unknown_task(
                    dep_spanned.span,
                    dep_name,
                    suggestion.as_deref(),
                ));
            }
        }
    }

    // Resolve on_error: fallback configuration
    if let Some(ref on_error_raw) = raw.on_error {
        let value = &on_error_raw.value;
        let span = on_error_raw.span;

        if let Some(serde_json::Value::Bool(true)) = value.get("ignore") {
            task.on_error = Some(AnalyzedOnError {
                action: OnErrorAction::Ignore,
                span,
            });
        } else if let Some(serde_json::Value::String(provider_str)) =
            value.get("retry_with_provider")
        {
            let provider = crate::ProviderName::parse(provider_str);
            task.on_error = Some(AnalyzedOnError {
                action: OnErrorAction::RetryWithProvider { provider },
                span,
            });
        } else if let Some(serde_json::Value::String(fallback_name)) = value.get("fallback") {
            if let Some(fallback_id) = task_table.get_id(fallback_name) {
                task.on_error = Some(AnalyzedOnError {
                    action: OnErrorAction::Fallback {
                        task_id: fallback_id,
                    },
                    span,
                });
            } else if !ctx.is_included_task(fallback_name) {
                let all_names: Vec<&str> = all_task_names.iter().map(|s| s.as_str()).collect();
                let suggestion = find_similar(fallback_name, &all_names, 0.6);
                ctx.add_error(AnalyzeError {
                    kind: AnalyzeErrorKind::UnknownOnErrorFallback,
                    span,
                    message: format!(
                        "on_error fallback references unknown task '{}'",
                        fallback_name
                    ),
                    suggestion: suggestion.map(|s| format!("did you mean '{}'?", s)),
                    note: Some(
                        "The fallback task must be defined in the same workflow".to_string(),
                    ),
                });
            }
        } else {
            ctx.add_error(AnalyzeError {
                kind: AnalyzeErrorKind::InvalidValue,
                span,
                message: "on_error: must contain exactly one of: ignore, retry_with_provider, \
                          or fallback"
                    .to_string(),
                suggestion: Some(
                    "Example: on_error: { ignore: true } or on_error: { fallback: task_id }"
                        .to_string(),
                ),
                note: None,
            });
        }
    }

    // Warn if for_each and decompose are both present
    if raw.for_each.is_some() && raw.decompose.is_some() {
        let span = raw
            .decompose
            .as_ref()
            .map(|d| d.span)
            .unwrap_or(raw.id.span);
        ctx.add_warning(AnalyzeError::new(
            AnalyzeErrorKind::InvalidValue,
            span,
            "for_each and decompose are both present — decompose takes priority, \
             for_each concurrency/fail_fast settings will be ignored"
                .to_string(),
        ));
    }

    // Error if concurrency: 0 (must be >= 1) — check both standalone and for_each
    let concurrency_check = raw.concurrency.as_ref().or_else(|| {
        raw.for_each
            .as_ref()
            .and_then(|fe| fe.value.concurrency.as_ref())
    });
    if let Some(concurrency) = concurrency_check {
        if let Templatable::Value(0) = &concurrency.value {
            ctx.add_error(AnalyzeError::new(
                AnalyzeErrorKind::InvalidValue,
                concurrency.span,
                "concurrency: 0 is invalid — must be >= 1 (use 1 for sequential execution)"
                    .to_string(),
            ));
        }
    }

    // Warn if timeout: 0 on any action (would cause immediate timeout)
    if let Some(ref action) = raw.action {
        let timeout_zero = match action {
            RawTaskAction::Exec(s) => s
                .value
                .timeout_ms
                .as_ref()
                .filter(|t| matches!(t.value, Templatable::Value(0))),
            RawTaskAction::Fetch(s) => s
                .value
                .timeout_ms
                .as_ref()
                .filter(|t| matches!(t.value, Templatable::Value(0))),
            RawTaskAction::Invoke(s) => s
                .value
                .timeout_ms
                .as_ref()
                .filter(|t| matches!(t.value, Templatable::Value(0))),
            _ => None,
        };
        if let Some(t) = timeout_zero {
            ctx.add_error(AnalyzeError::new(
                AnalyzeErrorKind::InvalidValue,
                t.span,
                "timeout: 0 will cause immediate timeout — use at least 1 second".to_string(),
            ));
        }
    }

    // Analyze output config
    if let Some(ref output) = raw.output {
        task.output = Some(analyze_output(&output.value, ctx));

        // Reject output: { format: json } without schema on LLM verbs — use structured: instead
        let is_llm_verb = raw
            .action
            .as_ref()
            .is_some_and(|a| matches!(a, RawTaskAction::Infer(_) | RawTaskAction::Agent(_)));
        let format_is_json = output
            .value
            .format
            .as_ref()
            .is_some_and(|f| f.value.eq_ignore_ascii_case("json"));
        let has_schema = output.value.schema.is_some() || output.value.schema_ref.is_some();
        let has_structured = raw.structured.is_some();
        if is_llm_verb && format_is_json && !has_schema && !has_structured {
            ctx.add_warning(AnalyzeError::new(
                AnalyzeErrorKind::InvalidValue,
                output.span,
                "output: { format: json } without a schema does not validate JSON — \
                 use structured: { schema: ... } for guaranteed valid JSON output"
                    .to_string(),
            ));
        }
    }

    Some(task)
}
