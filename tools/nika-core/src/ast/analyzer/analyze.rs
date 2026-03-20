//! Analyzer: raw::Workflow → analyzed::Workflow
//!
//! This is the core Phase 2 transformation that:
//! 1. Validates schema version
//! 2. Builds task table (interned IDs)
//! 3. Resolves all task references (`with:` bindings → WithSpec, `depends_on:` → Vec<TaskId>)
//! 4. Extracts implicit dependencies from WithEntry task references
//! 5. Resolves `imports:` specifications
//! 6. Detects cyclic dependencies
//! 7. Collects all errors with precise spans

use std::collections::{HashMap, HashSet};

use super::errors::{AnalyzeError, AnalyzeErrorKind, AnalyzeResult};
use super::suggestions::find_similar;
use crate::ast::analyzed::{
    AnalyzedAgentAction, AnalyzedContextFile, AnalyzedExecAction, AnalyzedFetchAction,
    AnalyzedForEach, AnalyzedImportSpec, AnalyzedInferAction, AnalyzedInvokeAction,
    AnalyzedMcpServer, AnalyzedOutput, AnalyzedRetry, AnalyzedTask, AnalyzedTaskAction,
    AnalyzedWorkflow, HttpMethod, McpTransport, OutputFormat, TaskId, TaskTable,
};
use crate::ast::raw::{
    RawAgentAction, RawExecAction, RawFetchAction, RawInferAction, RawInvokeAction, RawTask,
    RawTaskAction, RawWorkflow,
};
use crate::ast::schema::SchemaVersion;
use crate::binding::{parse_with_entry, WithSpec};
use crate::source::{Span, Spanned};

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

    // 3. Build task table (detect duplicates)
    build_task_table(&raw.tasks.value, &mut ctx);

    // 4. Validate task references (unknown tasks in with:/depends_on:, invalid bindings)
    let task_table = ctx.task_table.clone();
    let task_names: Vec<String> = task_table.iter().map(|(_, n)| n.to_string()).collect();
    for raw_task in raw.tasks.value.iter() {
        validate_task_refs(&raw_task.value, &task_table, &task_names, &mut ctx);
    }

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
    workflow.provider = raw.provider.map(|s| s.value);
    workflow.model = raw.model.map(|s| s.value);

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

    // 5. Analyze imports
    if let Some(ref imports) = raw.imports {
        for import_spanned in &imports.value {
            let import = &import_spanned.value;
            workflow.imports.push(AnalyzedImportSpec {
                path: import.path.value.clone(),
                prefix: import.prefix.as_ref().map(|s| s.value.clone()),
                span: import_spanned.span,
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

    // 9. Detect cyclic dependencies
    detect_cycles(&workflow, &mut ctx);

    // Copy task table to workflow
    workflow.task_table = ctx.task_table;

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

/// Analyze and validate the schema version.
fn analyze_schema(raw: &RawWorkflow, ctx: &mut AnalyzerContext) -> Option<SchemaVersion> {
    let schema_str = &raw.schema.value;

    if let Some(version) = SchemaVersion::parse(schema_str) {
        Some(version)
    } else {
        // Find similar schema version
        let all_versions: Vec<&str> = SchemaVersion::all().iter().map(|v| v.as_str()).collect();
        let suggestion = find_similar(schema_str, &all_versions, 0.6);

        ctx.add_error(AnalyzeError::invalid_schema(
            raw.schema.span,
            schema_str,
            suggestion.as_deref(),
        ));
        None
    }
}

/// Validate version-gated features.
///
/// Checks that features used in the workflow are available in the declared schema version.
fn validate_feature_gates(raw: &RawWorkflow, version: SchemaVersion, ctx: &mut AnalyzerContext) {
    let version_str = version.as_str();

    // Check MCP servers
    if let Some(ref mcp) = raw.mcp {
        if !version.supports_mcp() {
            ctx.add_error(AnalyzeError::unsupported_feature(
                mcp.span,
                "mcp",
                version_str,
                "nika/workflow@0.2",
            ));
        }
    }

    // Check context files
    if let Some(ref context) = raw.context {
        if !version.supports_context() {
            ctx.add_error(AnalyzeError::unsupported_feature(
                context.span,
                "context",
                version_str,
                "nika/workflow@0.9",
            ));
        }
    }

    // Check imports
    if let Some(ref imports) = raw.imports {
        if !version.supports_imports() {
            ctx.add_error(AnalyzeError::unsupported_feature(
                imports.span,
                "imports",
                version_str,
                "nika/workflow@0.12",
            ));
        }
    }

    // Check inputs
    if let Some(ref inputs) = raw.inputs {
        if !version.supports_inputs() {
            ctx.add_error(AnalyzeError::unsupported_feature(
                inputs.span,
                "inputs",
                version_str,
                "nika/workflow@0.10",
            ));
        }
    }

    // Check task-level features
    for task in raw.tasks.value.iter() {
        validate_task_feature_gates(&task.value, version, version_str, ctx);
    }
}

/// Validate version-gated features in a task.
fn validate_task_feature_gates(
    task: &RawTask,
    version: SchemaVersion,
    version_str: &str,
    ctx: &mut AnalyzerContext,
) {
    // Check for_each
    if let Some(ref for_each) = task.for_each {
        if !version.supports_for_each() {
            ctx.add_error(AnalyzeError::unsupported_feature(
                for_each.span,
                "for_each",
                version_str,
                "nika/workflow@0.3",
            ));
        }
    }

    // Check retry
    if let Some(ref retry) = task.retry {
        if !version.supports_retry() {
            ctx.add_error(AnalyzeError::unsupported_feature(
                retry.span,
                "retry",
                version_str,
                "nika/workflow@0.3",
            ));
        }

        // retry: is only effective on fetch: tasks — warn on other verbs
        if let Some(ref action) = task.action {
            let verb_name = match action {
                RawTaskAction::Fetch(_) => None,
                RawTaskAction::Infer(_) => Some("infer"),
                RawTaskAction::Exec(_) => Some("exec"),
                RawTaskAction::Invoke(_) => Some("invoke"),
                RawTaskAction::Agent(_) => Some("agent"),
            };
            if let Some(verb_name) = verb_name {
                ctx.add_warning(
                    AnalyzeError::new(
                        AnalyzeErrorKind::InvalidValue,
                        retry.span,
                        format!(
                            "'retry' has no effect on '{}' tasks (only 'fetch' supports retry)",
                            verb_name
                        ),
                    )
                    .with_suggestion("move retry to a fetch task, or remove it"),
                );
            }
        }
    }

    // Check with: bindings
    if let Some(ref with_refs) = task.with_refs {
        if !version.supports_with() {
            ctx.add_error(AnalyzeError::unsupported_feature(
                with_refs.span,
                "with",
                version_str,
                "nika/workflow@0.12",
            ));
        }
    }

    // Check depends_on:
    if let Some(ref depends_on) = task.depends_on {
        if !version.supports_depends_on() {
            ctx.add_error(AnalyzeError::unsupported_feature(
                depends_on.span,
                "depends_on",
                version_str,
                "nika/workflow@0.12",
            ));
        }
    }

    // Check invoke/agent verbs
    if let Some(ref action) = task.action {
        match action {
            RawTaskAction::Invoke(invoke) => {
                if !version.supports_invoke_agent() {
                    ctx.add_error(AnalyzeError::unsupported_feature(
                        invoke.span,
                        "invoke verb",
                        version_str,
                        "nika/workflow@0.2",
                    ));
                }
            }
            RawTaskAction::Agent(agent) => {
                if !version.supports_invoke_agent() {
                    ctx.add_error(AnalyzeError::unsupported_feature(
                        agent.span,
                        "agent verb",
                        version_str,
                        "nika/workflow@0.2",
                    ));
                }
            }
            _ => {}
        }
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
        provider: raw.provider.as_ref().map(|s| s.value.clone()),
        model: raw.model.as_ref().map(|s| s.value.clone()),
        with_spec: WithSpec::default(),
        depends_on: Vec::new(),
        implicit_deps: Vec::new(),
        output: None,
        for_each: raw
            .for_each
            .as_ref()
            .map(|f| analyze_for_each(&f.value, f.span)),
        retry: raw.retry.as_ref().map(|r| analyze_retry(&r.value, r.span)),
        decompose: raw.decompose.as_ref().map(|d| d.value.clone()),
        concurrency: raw.concurrency.as_ref().map(|s| s.value),
        fail_fast: raw.fail_fast.as_ref().map(|s| s.value),
        artifact: raw.artifact.as_ref().and_then(|s| {
            match serde_json::from_value(s.value.clone()) {
                Ok(spec) => Some(spec),
                Err(e) => {
                    tracing::warn!(
                        task_id = %raw.id.value,
                        error = %e,
                        "Failed to parse artifact: config, ignoring"
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
        span: raw.span,
    };

    // Analyze action
    if let Some(ref action) = raw.action {
        task.action = analyze_action(action);

        // Merge agent-block provider/model into task if not set at task level
        if let RawTaskAction::Agent(ref agent_spanned) = action {
            let agent = &agent_spanned.value;
            if task.provider.is_none() {
                task.provider = agent.provider.as_ref().map(|s| s.value.clone());
            }
            if task.model.is_none() {
                task.model = agent.model.as_ref().map(|s| s.value.clone());
            }
        }
    }

    // Warn if retry: is used on non-fetch verbs (it only applies to fetch:)
    if task.retry.is_some() {
        if let Some(ref action) = raw.action {
            let is_fetch = matches!(action, RawTaskAction::Fetch(_));
            if !is_fetch {
                tracing::warn!(
                    task_id = %raw.id.value,
                    "retry: config is only supported on fetch: tasks — ignored for this verb"
                );
            }
        }
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
                        } else if !ctx.is_included_task(dep_task_name) {
                            // Unknown task reference in with: binding
                            // (skip check for tasks matching include prefixes)
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

    // Analyze output config
    if let Some(ref output) = raw.output {
        task.output = Some(analyze_output(&output.value, ctx));
    }

    Some(task)
}

/// Analyze task action.
fn analyze_action(raw: &RawTaskAction) -> AnalyzedTaskAction {
    match raw {
        RawTaskAction::Infer(s) => AnalyzedTaskAction::Infer(analyze_infer(&s.value)),
        RawTaskAction::Exec(s) => AnalyzedTaskAction::Exec(analyze_shell_cmd(&s.value)),
        RawTaskAction::Fetch(s) => AnalyzedTaskAction::Fetch(analyze_fetch(&s.value)),
        RawTaskAction::Invoke(s) => AnalyzedTaskAction::Invoke(analyze_invoke(&s.value)),
        RawTaskAction::Agent(s) => AnalyzedTaskAction::Agent(analyze_agent(&s.value)),
    }
}

fn analyze_infer(raw: &RawInferAction) -> AnalyzedInferAction {
    use crate::ast::content::analyze_content_part;

    AnalyzedInferAction {
        prompt: raw.prompt.value.clone(),
        system: raw.system.as_ref().map(|s| s.value.clone()),
        temperature: raw.temperature.as_ref().map(|s| s.value),
        max_tokens: raw.max_tokens.as_ref().map(|s| s.value),
        thinking: raw.thinking.as_ref().map(|s| s.value),
        thinking_budget: raw.thinking_budget.as_ref().map(|s| s.value),
        content: raw
            .content
            .as_ref()
            .map(|spanned| spanned.value.iter().map(analyze_content_part).collect()),
        span: raw.prompt.span,
    }
}

fn analyze_shell_cmd(raw: &RawExecAction) -> AnalyzedExecAction {
    AnalyzedExecAction {
        command: raw.command.value.clone(),
        shell: raw.shell.as_ref().map(|s| s.value).unwrap_or(false),
        working_dir: raw.working_dir.as_ref().map(|s| s.value.clone()),
        env: raw
            .env
            .as_ref()
            .map(|s| {
                s.value
                    .iter()
                    .map(|(k, v)| (k.value.clone(), v.value.clone()))
                    .collect()
            })
            .unwrap_or_default(),
        timeout_ms: raw.timeout_ms.as_ref().map(|s| s.value),
        span: raw.command.span,
    }
}

fn analyze_fetch(raw: &RawFetchAction) -> AnalyzedFetchAction {
    let method = raw
        .method
        .as_ref()
        .and_then(|s| HttpMethod::parse(&s.value))
        .unwrap_or(HttpMethod::Get);

    AnalyzedFetchAction {
        url: raw.url.value.clone(),
        method,
        headers: raw
            .headers
            .as_ref()
            .map(|s| {
                s.value
                    .iter()
                    .map(|(k, v)| (k.value.clone(), v.value.clone()))
                    .collect()
            })
            .unwrap_or_default(),
        body: raw.body.as_ref().map(|s| s.value.clone()),
        json: raw.json.as_ref().map(|s| s.value.clone()),
        timeout_ms: raw.timeout_ms.as_ref().map(|s| s.value),
        follow_redirects: raw
            .follow_redirects
            .as_ref()
            .map(|s| s.value)
            .unwrap_or(true),
        response: raw.response.as_ref().map(|s| s.value.clone()),
        extract: raw.extract.as_ref().map(|s| s.value.clone()),
        selector: raw.selector.as_ref().map(|s| s.value.clone()),
        span: raw.url.span,
    }
}

fn analyze_invoke(raw: &RawInvokeAction) -> AnalyzedInvokeAction {
    let (server, tool) = raw.parse_tool_name();

    AnalyzedInvokeAction {
        server: server
            .map(|s| s.to_string())
            .or_else(|| raw.mcp.as_ref().map(|s| s.value.clone())),
        tool: tool.to_string(),
        params: raw.params.as_ref().map(|s| s.value.clone()),
        timeout_ms: raw.timeout_ms.as_ref().map(|s| s.value),
        span: raw.tool.span,
    }
}

fn analyze_agent(raw: &RawAgentAction) -> AnalyzedAgentAction {
    AnalyzedAgentAction {
        prompt: raw.prompt.value.clone(),
        tools: raw
            .tools
            .as_ref()
            .map(|s| s.value.iter().map(|v| v.value.clone()).collect())
            .unwrap_or_default(),
        max_iterations: raw.max_iterations.as_ref().map(|s| s.value),
        max_tokens: raw.max_tokens.as_ref().map(|s| s.value),
        from: raw.from.as_ref().map(|s| s.value.clone()),
        skills: raw
            .skills
            .as_ref()
            .map(|s| s.value.iter().map(|v| v.value.clone()).collect())
            .unwrap_or_default(),
        mcp: raw
            .mcp
            .as_ref()
            .map(|s| s.value.iter().map(|v| v.value.clone()).collect())
            .unwrap_or_default(),
        system: raw.system.as_ref().map(|s| s.value.clone()),
        temperature: raw.temperature.as_ref().map(|s| s.value),
        token_budget: raw.token_budget.as_ref().map(|s| s.value),
        extended_thinking: raw.extended_thinking.as_ref().map(|s| s.value),
        thinking_budget: raw.thinking_budget.as_ref().map(|s| s.value),
        depth_limit: raw.depth_limit.as_ref().map(|s| s.value),
        span: raw.prompt.span,
    }
}

fn analyze_output(
    raw: &crate::ast::raw::RawOutputConfig,
    _ctx: &mut AnalyzerContext,
) -> AnalyzedOutput {
    let format = raw
        .format
        .as_ref()
        .and_then(|s| OutputFormat::parse(&s.value))
        .unwrap_or(OutputFormat::Text);

    AnalyzedOutput {
        format,
        schema: raw.schema.as_ref().map(|s| s.value.clone()),
        span: raw.format.as_ref().map(|s| s.span).unwrap_or(Span::dummy()),
    }
}

/// Analyze MCP server configuration.
fn analyze_mcp_server(
    name: &str,
    raw: &crate::ast::raw::RawMcpServer,
    span: Span,
    ctx: &mut AnalyzerContext,
) -> AnalyzedMcpServer {
    let transport = if raw.is_sse() {
        McpTransport::Sse
    } else {
        McpTransport::Stdio
    };

    // SSE servers are accepted by the analyzer but dropped during lowering
    if transport == McpTransport::Sse {
        ctx.add_warning(
            AnalyzeError::new(
                AnalyzeErrorKind::UnsupportedFeature,
                span,
                format!(
                    "SSE MCP server '{}' will be dropped during execution (no runtime support)",
                    name
                ),
            )
            .with_suggestion("use a stdio-based MCP server instead"),
        );
    }

    // Stdio servers require a non-empty command field
    if transport == McpTransport::Stdio {
        let has_command = raw
            .command
            .as_ref()
            .map(|s| !s.value.trim().is_empty())
            .unwrap_or(false);
        if !has_command {
            let error_span = raw.command.as_ref().map(|s| s.span).unwrap_or(span);
            ctx.add_error(AnalyzeError::new(
                AnalyzeErrorKind::MissingField,
                error_span,
                format!(
                    "MCP server '{}' uses stdio transport but has no 'command' field",
                    name
                ),
            ));
        }
    }

    AnalyzedMcpServer {
        name: name.to_string(),
        command: raw.command.as_ref().map(|s| s.value.clone()),
        args: raw
            .args
            .as_ref()
            .map(|s| s.value.iter().map(|v| v.value.clone()).collect())
            .unwrap_or_default(),
        env: raw
            .env
            .as_ref()
            .map(|s| {
                s.value
                    .iter()
                    .map(|(k, v)| (k.value.clone(), v.value.clone()))
                    .collect()
            })
            .unwrap_or_default(),
        cwd: raw.cwd.as_ref().map(|s| s.value.clone()),
        url: raw.url.as_ref().map(|s| s.value.clone()),
        transport,
        span,
    }
}

/// Analyze for_each iteration configuration.
fn analyze_for_each(raw: &crate::ast::raw::RawForEach, span: Span) -> AnalyzedForEach {
    AnalyzedForEach {
        items: raw.items.value.clone(),
        as_var: raw
            .as_var
            .as_ref()
            .map(|s| s.value.clone())
            .unwrap_or_else(|| "item".to_string()),
        parallel: Some(raw.parallel.as_ref().map(|s| s.value).unwrap_or(1)),
        fail_fast: raw.fail_fast.as_ref().map(|s| s.value).unwrap_or(true),
        span,
    }
}

/// Analyze retry configuration.
fn analyze_retry(raw: &crate::ast::raw::RawRetryConfig, span: Span) -> AnalyzedRetry {
    AnalyzedRetry {
        max_attempts: raw.max_attempts.as_ref().map(|s| s.value).unwrap_or(3),
        delay_ms: raw.delay_ms.as_ref().map(|s| s.value).unwrap_or(1000),
        backoff: raw.backoff.as_ref().map(|s| s.value),
        span,
    }
}

/// Build the task table from raw tasks, detecting duplicates.
///
/// Collect prefixes declared in `include:` entries.
///
/// When a workflow uses `include: [{ path: ./lib/seo.nika.yaml, prefix: seo_ }]`,
/// task references like `$seo_generate_title` should not be flagged as unknown
/// during analysis — they'll be resolved after `expand_includes()` merges the
/// included tasks into the DAG.
fn collect_include_prefixes(raw: &RawWorkflow, ctx: &mut AnalyzerContext) {
    if let Some(ref imports) = raw.imports {
        let mut seen = HashSet::new();
        for import in &imports.value {
            if let Some(ref prefix) = import.value.prefix {
                if !seen.insert(prefix.value.clone()) {
                    ctx.add_error(AnalyzeError::new(
                        AnalyzeErrorKind::InvalidValue,
                        prefix.span,
                        format!("duplicate import prefix '{}'", prefix.value),
                    ));
                }
                ctx.include_prefixes.push(prefix.value.clone());
            }
        }
    }
}

/// Shared between `validate()` and `analyze()`.
fn build_task_table(tasks: &[Spanned<RawTask>], ctx: &mut AnalyzerContext) {
    for task in tasks.iter() {
        let task_name = &task.value.id.value;
        let task_span = task.value.id.span;

        // Validate task ID format before inserting
        if !validate_task_id(task_name, task_span, ctx) {
            continue;
        }

        if let Some(first_span) = ctx.task_spans.get(task_name) {
            ctx.add_error(AnalyzeError::duplicate_task(
                task_span,
                task_name,
                *first_span,
            ));
        } else {
            ctx.task_table.insert(task_name);
            ctx.task_spans.insert(task_name.clone(), task_span);
        }
    }
}

/// Validate task ID format: non-empty, alphanumeric with hyphens/underscores/dots,
/// must not start with `$` (reserved for binding references).
fn validate_task_id(name: &str, span: Span, ctx: &mut AnalyzerContext) -> bool {
    if name.is_empty() {
        ctx.add_error(AnalyzeError::new(
            AnalyzeErrorKind::InvalidValue,
            span,
            "task ID must not be empty",
        ));
        return false;
    }
    if name.starts_with('$') {
        ctx.add_error(
            AnalyzeError::new(
                AnalyzeErrorKind::InvalidValue,
                span,
                format!(
                    "task ID '{}' must not start with '$' (reserved for binding references)",
                    name
                ),
            )
            .with_suggestion("remove the leading '$' from the task ID"),
        );
        return false;
    }
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.')
    {
        ctx.add_error(
            AnalyzeError::new(
                AnalyzeErrorKind::InvalidValue,
                span,
                format!("task ID '{}' contains invalid characters", name),
            )
            .with_suggestion("use only alphanumeric characters, hyphens, underscores, and dots"),
        );
        return false;
    }
    true
}

/// Validate task references without building analyzed tasks.
///
/// Checks `with:` bindings for unknown task references and invalid expressions,
/// and `depends_on:` for unknown task references. Used by `validate()`.
fn validate_task_refs(
    raw: &RawTask,
    task_table: &TaskTable,
    all_task_names: &[String],
    ctx: &mut AnalyzerContext,
) {
    // Validate with: bindings
    if let Some(ref with_refs) = raw.with_refs {
        for (_alias_spanned, value_spanned) in with_refs.value.iter() {
            let expr = &value_spanned.value;

            match parse_with_entry(expr) {
                Ok(entry) => {
                    // Check for unknown task references
                    if let Some(dep_task_name) = entry.task_id() {
                        if task_table.get_id(dep_task_name).is_none()
                            && !ctx.is_included_task(dep_task_name)
                        {
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

    // Validate depends_on: references
    if let Some(ref depends_on) = raw.depends_on {
        for dep_spanned in &depends_on.value {
            let dep_name = &dep_spanned.value;
            if task_table.get_id(dep_name).is_none() && !ctx.is_included_task(dep_name) {
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
}

/// Detect cyclic dependencies from raw workflow (without AnalyzedWorkflow).
///
/// Builds a lightweight dependency graph from raw task data and runs DFS cycle detection.
/// Used by `validate()`.
fn detect_cycles_from_raw(
    tasks: &[Spanned<RawTask>],
    task_table: &TaskTable,
    ctx: &mut AnalyzerContext,
) {
    // Build adjacency list: TaskId → Vec<TaskId>
    let mut adjacency: HashMap<TaskId, Vec<TaskId>> = HashMap::new();
    let mut task_spans: HashMap<TaskId, Span> = HashMap::new();

    for raw_task in tasks.iter() {
        let task_name = &raw_task.value.id.value;
        let Some(task_id) = task_table.get_id(task_name) else {
            continue; // Skip tasks that failed duplicate detection
        };
        task_spans.insert(task_id, raw_task.value.span);
        let deps = adjacency.entry(task_id).or_default();

        // Collect depends_on edges
        if let Some(ref depends_on) = raw_task.value.depends_on {
            for dep_spanned in &depends_on.value {
                if let Some(dep_id) = task_table.get_id(&dep_spanned.value) {
                    deps.push(dep_id);
                }
            }
        }

        // Collect implicit deps from with: bindings
        if let Some(ref with_refs) = raw_task.value.with_refs {
            for (_alias, value_spanned) in with_refs.value.iter() {
                if let Ok(entry) = parse_with_entry(&value_spanned.value) {
                    if let Some(dep_task_name) = entry.task_id() {
                        if let Some(dep_id) = task_table.get_id(dep_task_name) {
                            if !deps.contains(&dep_id) {
                                deps.push(dep_id);
                            }
                        }
                    }
                }
            }
        }
    }

    // DFS cycle detection
    let graph = RawDepGraph {
        adjacency,
        task_table,
        task_spans,
    };
    let mut visited = HashSet::new();
    let mut rec_stack = HashSet::new();
    let mut path = Vec::new();

    for &task_id in graph.adjacency.keys() {
        if !visited.contains(&task_id) {
            detect_cycles_raw_dfs(
                task_id,
                &graph,
                &mut visited,
                &mut rec_stack,
                &mut path,
                ctx,
            );
        }
    }
}

/// Read-only dependency graph context for raw cycle detection.
struct RawDepGraph<'a> {
    adjacency: HashMap<TaskId, Vec<TaskId>>,
    task_table: &'a TaskTable,
    task_spans: HashMap<TaskId, Span>,
}

/// DFS helper for raw cycle detection.
fn detect_cycles_raw_dfs(
    task_id: TaskId,
    graph: &RawDepGraph<'_>,
    visited: &mut HashSet<TaskId>,
    rec_stack: &mut HashSet<TaskId>,
    path: &mut Vec<TaskId>,
    ctx: &mut AnalyzerContext,
) {
    visited.insert(task_id);
    rec_stack.insert(task_id);
    path.push(task_id);

    if let Some(deps) = graph.adjacency.get(&task_id) {
        for dep_id in deps {
            if !visited.contains(dep_id) {
                detect_cycles_raw_dfs(*dep_id, graph, visited, rec_stack, path, ctx);
            } else if rec_stack.contains(dep_id) {
                // Found cycle
                let cycle_start = path.iter().position(|&id| id == *dep_id).unwrap();
                let cycle_path: Vec<&str> = path[cycle_start..]
                    .iter()
                    .filter_map(|id| graph.task_table.get_name(*id))
                    .collect();
                let mut cycle_with_close = cycle_path.clone();
                if let Some(name) = graph.task_table.get_name(*dep_id) {
                    cycle_with_close.push(name);
                }
                let span = graph
                    .task_spans
                    .get(&task_id)
                    .copied()
                    .unwrap_or(Span::dummy());
                ctx.add_error(AnalyzeError::cyclic_dependency(span, &cycle_with_close));
            }
        }
    }

    path.pop();
    rec_stack.remove(&task_id);
}

/// Detect cyclic dependencies using DFS.
///
/// Checks both `depends_on` (explicit ordering) and `implicit_deps` (from with: bindings).
fn detect_cycles(workflow: &AnalyzedWorkflow, ctx: &mut AnalyzerContext) {
    let mut visited = HashSet::new();
    let mut rec_stack = HashSet::new();
    let mut path = Vec::new();

    for task in &workflow.tasks {
        if !visited.contains(&task.id) {
            detect_cycles_dfs(
                task.id,
                workflow,
                &mut visited,
                &mut rec_stack,
                &mut path,
                ctx,
            );
        }
    }
}

fn detect_cycles_dfs(
    task_id: TaskId,
    workflow: &AnalyzedWorkflow,
    visited: &mut HashSet<TaskId>,
    rec_stack: &mut HashSet<TaskId>,
    path: &mut Vec<TaskId>,
    ctx: &mut AnalyzerContext,
) {
    visited.insert(task_id);
    rec_stack.insert(task_id);
    path.push(task_id);

    if let Some(task) = workflow.get_task(task_id) {
        // Check explicit depends_on dependencies
        for dep_id in &task.depends_on {
            if !visited.contains(dep_id) {
                detect_cycles_dfs(*dep_id, workflow, visited, rec_stack, path, ctx);
            } else if rec_stack.contains(dep_id) {
                // Found cycle
                let cycle_start = path.iter().position(|&id| id == *dep_id).unwrap();
                let cycle_path: Vec<&str> = path[cycle_start..]
                    .iter()
                    .filter_map(|id| workflow.task_table.get_name(*id))
                    .collect();
                let mut cycle_with_close = cycle_path.clone();
                if let Some(name) = workflow.task_table.get_name(*dep_id) {
                    cycle_with_close.push(name);
                }
                ctx.add_error(AnalyzeError::cyclic_dependency(
                    task.span,
                    &cycle_with_close,
                ));
            }
        }

        // Check implicit dependencies (from with: bindings)
        for dep_id in &task.implicit_deps {
            if !visited.contains(dep_id) {
                detect_cycles_dfs(*dep_id, workflow, visited, rec_stack, path, ctx);
            } else if rec_stack.contains(dep_id) {
                // Found cycle via with: binding
                let cycle_start = path.iter().position(|&id| id == *dep_id).unwrap();
                let cycle_path: Vec<&str> = path[cycle_start..]
                    .iter()
                    .filter_map(|id| workflow.task_table.get_name(*id))
                    .collect();
                let mut cycle_with_close = cycle_path.clone();
                if let Some(name) = workflow.task_table.get_name(*dep_id) {
                    cycle_with_close.push(name);
                }
                ctx.add_error(AnalyzeError::cyclic_dependency(
                    task.span,
                    &cycle_with_close,
                ));
            }
        }
    }

    path.pop();
    rec_stack.remove(&task_id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::raw::{
        RawContextConfig, RawImportSpec, RawMcpConfig, RawMcpServer, RawTask, RawWorkflow,
    };
    use crate::source::{FileId, Spanned};
    use indexmap::IndexMap;

    fn make_span(start: u32, end: u32) -> Span {
        Span::new(FileId(0), start, end)
    }

    fn make_raw_workflow(schema: &str, tasks: Vec<RawTask>) -> RawWorkflow {
        RawWorkflow {
            schema: Spanned::new(schema.to_string(), make_span(0, 20)),
            tasks: Spanned::new(
                tasks
                    .into_iter()
                    .map(|t| Spanned::new(t, make_span(0, 50)))
                    .collect(),
                make_span(0, 100),
            ),
            ..Default::default()
        }
    }

    fn make_raw_task(id: &str) -> RawTask {
        RawTask {
            id: Spanned::new(id.to_string(), make_span(0, id.len() as u32)),
            ..Default::default()
        }
    }

    /// Helper: add a `with:` binding to a raw task.
    fn add_with_ref(task: &mut RawTask, alias: &str, expr: &str) {
        let with_refs = task
            .with_refs
            .get_or_insert_with(|| Spanned::new(IndexMap::new(), make_span(0, 50)));
        with_refs.value.insert(
            Spanned::new(alias.to_string(), make_span(0, alias.len() as u32)),
            Spanned::new(expr.to_string(), make_span(0, expr.len() as u32)),
        );
    }

    /// Helper: add `depends_on:` entries to a raw task.
    fn add_depends_on(task: &mut RawTask, deps: &[&str]) {
        let spanned_deps: Vec<Spanned<String>> = deps
            .iter()
            .map(|d| Spanned::new(d.to_string(), make_span(0, d.len() as u32)))
            .collect();
        task.depends_on = Some(Spanned::new(spanned_deps, make_span(0, 50)));
    }

    // ====================================================================
    // Basic workflow analysis
    // ====================================================================

    #[test]
    fn test_analyze_valid_workflow() {
        let raw = make_raw_workflow(
            "nika/workflow@0.12",
            vec![make_raw_task("task1"), make_raw_task("task2")],
        );

        let result = analyze(raw);
        assert!(result.is_ok());

        let workflow = result.value.unwrap();
        assert_eq!(workflow.task_count(), 2);
        assert!(workflow.has_task("task1"));
        assert!(workflow.has_task("task2"));
    }

    #[test]
    fn test_analyze_invalid_schema() {
        let raw = make_raw_workflow("invalid", vec![]);
        let result = analyze(raw);
        assert!(result.is_err());
        assert_eq!(result.errors[0].kind, AnalyzeErrorKind::InvalidSchema);
    }

    #[test]
    fn test_analyze_schema_suggestion() {
        // Valid schema with at least one task
        let raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("step1")]);
        let result = analyze(raw);
        assert!(result.is_ok());
    }

    #[test]
    fn test_analyze_duplicate_task() {
        let raw = make_raw_workflow(
            "nika/workflow@0.12",
            vec![make_raw_task("task1"), make_raw_task("task1")],
        );

        let result = analyze(raw);
        assert!(result.is_err());
        assert_eq!(result.errors[0].kind, AnalyzeErrorKind::DuplicateTask);
    }

    // ====================================================================
    // with: binding analysis
    // ====================================================================

    #[test]
    fn test_analyze_with_binding_simple() {
        let mut task2 = make_raw_task("task2");
        add_with_ref(&mut task2, "data", "$task1");

        let raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("task1"), task2]);
        let result = analyze(raw);
        assert!(result.is_ok());

        let workflow = result.value.unwrap();
        let t2 = workflow.get_task_by_name("task2").unwrap();
        assert_eq!(t2.with_spec.len(), 1);
        assert!(t2.with_spec.contains_key("data"));
        assert_eq!(t2.implicit_deps.len(), 1);
    }

    #[test]
    fn test_analyze_with_binding_unknown_task() {
        let mut task1 = make_raw_task("task1");
        add_with_ref(&mut task1, "data", "$unknown_task");

        let raw = make_raw_workflow("nika/workflow@0.12", vec![task1]);
        let result = analyze(raw);

        assert!(result.is_err());
        assert_eq!(result.errors[0].kind, AnalyzeErrorKind::UnknownTask);
    }

    #[test]
    fn test_analyze_with_binding_invalid_expr() {
        let mut task1 = make_raw_task("task1");
        // Empty expression should fail parse_with_entry
        add_with_ref(&mut task1, "data", "");

        let raw = make_raw_workflow("nika/workflow@0.12", vec![task1]);
        let result = analyze(raw);

        assert!(result.is_err());
        assert_eq!(result.errors[0].kind, AnalyzeErrorKind::InvalidBinding);
    }

    #[test]
    fn test_analyze_with_binding_env_source() {
        // $env.API_KEY should NOT create an implicit dependency
        let mut task1 = make_raw_task("task1");
        add_with_ref(&mut task1, "key", "$env.API_KEY");

        let raw = make_raw_workflow("nika/workflow@0.12", vec![task1]);
        let result = analyze(raw);
        assert!(result.is_ok());

        let workflow = result.value.unwrap();
        let t1 = workflow.get_task_by_name("task1").unwrap();
        assert_eq!(t1.with_spec.len(), 1);
        assert!(t1.implicit_deps.is_empty()); // Env source, no task dep
    }

    #[test]
    fn test_analyze_with_binding_deduplicates_implicit_deps() {
        let mut task2 = make_raw_task("task2");
        add_with_ref(&mut task2, "a", "$task1.field_a");
        add_with_ref(&mut task2, "b", "$task1.field_b");

        let raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("task1"), task2]);
        let result = analyze(raw);
        assert!(result.is_ok());

        let workflow = result.value.unwrap();
        let t2 = workflow.get_task_by_name("task2").unwrap();
        // Both bindings reference task1, but implicit_deps should deduplicate
        assert_eq!(t2.implicit_deps.len(), 1);
    }

    #[test]
    fn test_analyze_with_binding_with_transforms() {
        let mut task2 = make_raw_task("task2");
        add_with_ref(&mut task2, "result", "$task1.data | upper | trim");

        let raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("task1"), task2]);
        let result = analyze(raw);
        assert!(result.is_ok());

        let workflow = result.value.unwrap();
        let t2 = workflow.get_task_by_name("task2").unwrap();
        let entry = t2.with_spec.get("result").unwrap();
        assert!(entry.transform.is_some());
        assert_eq!(entry.task_id(), Some("task1"));
    }

    #[test]
    fn test_analyze_with_binding_with_default() {
        let mut task2 = make_raw_task("task2");
        add_with_ref(&mut task2, "val", "$task1.count ?? 0");

        let raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("task1"), task2]);
        let result = analyze(raw);
        assert!(result.is_ok());

        let workflow = result.value.unwrap();
        let t2 = workflow.get_task_by_name("task2").unwrap();
        let entry = t2.with_spec.get("val").unwrap();
        assert!(entry.default.is_some());
    }

    // ====================================================================
    // depends_on: analysis
    // ====================================================================

    #[test]
    fn test_analyze_depends_on_valid() {
        let mut task2 = make_raw_task("task2");
        add_depends_on(&mut task2, &["task1"]);

        let raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("task1"), task2]);
        let result = analyze(raw);
        assert!(result.is_ok());

        let workflow = result.value.unwrap();
        let t2 = workflow.get_task_by_name("task2").unwrap();
        assert_eq!(t2.depends_on.len(), 1);
    }

    #[test]
    fn test_analyze_depends_on_unknown_task() {
        let mut task1 = make_raw_task("task1");
        add_depends_on(&mut task1, &["nonexistent"]);

        let raw = make_raw_workflow("nika/workflow@0.12", vec![task1]);
        let result = analyze(raw);

        assert!(result.is_err());
        assert_eq!(result.errors[0].kind, AnalyzeErrorKind::UnknownTask);
    }

    #[test]
    fn test_analyze_depends_on_multiple() {
        let mut task3 = make_raw_task("task3");
        add_depends_on(&mut task3, &["task1", "task2"]);

        let raw = make_raw_workflow(
            "nika/workflow@0.12",
            vec![make_raw_task("task1"), make_raw_task("task2"), task3],
        );
        let result = analyze(raw);
        assert!(result.is_ok());

        let workflow = result.value.unwrap();
        let t3 = workflow.get_task_by_name("task3").unwrap();
        assert_eq!(t3.depends_on.len(), 2);
    }

    // ====================================================================
    // Cycle detection
    // ====================================================================

    #[test]
    fn test_analyze_cyclic_dependency_depends_on() {
        let mut task1 = make_raw_task("task1");
        let mut task2 = make_raw_task("task2");

        add_depends_on(&mut task1, &["task2"]);
        add_depends_on(&mut task2, &["task1"]);

        let raw = make_raw_workflow("nika/workflow@0.12", vec![task1, task2]);
        let result = analyze(raw);

        assert!(result.is_err());
        assert!(result
            .errors
            .iter()
            .any(|e| e.kind == AnalyzeErrorKind::CyclicDependency));
    }

    #[test]
    fn test_analyze_cyclic_dependency_via_with() {
        let mut task1 = make_raw_task("task1");
        let mut task2 = make_raw_task("task2");

        add_with_ref(&mut task1, "data", "$task2");
        add_with_ref(&mut task2, "data", "$task1");

        let raw = make_raw_workflow("nika/workflow@0.12", vec![task1, task2]);
        let result = analyze(raw);

        assert!(result.is_err());
        assert!(result
            .errors
            .iter()
            .any(|e| e.kind == AnalyzeErrorKind::CyclicDependency));
    }

    #[test]
    fn test_analyze_cyclic_dependency_mixed() {
        // task1 depends_on task2 explicitly, task2 references task1 via with:
        let mut task1 = make_raw_task("task1");
        let mut task2 = make_raw_task("task2");

        add_depends_on(&mut task1, &["task2"]);
        add_with_ref(&mut task2, "data", "$task1");

        let raw = make_raw_workflow("nika/workflow@0.12", vec![task1, task2]);
        let result = analyze(raw);

        assert!(result.is_err());
        assert!(result
            .errors
            .iter()
            .any(|e| e.kind == AnalyzeErrorKind::CyclicDependency));
    }

    #[test]
    fn test_analyze_cyclic_dependency_three_tasks_via_with() {
        // A → B → C → A via with: bindings (transitive implicit cycle)
        let mut a = make_raw_task("a");
        add_with_ref(&mut a, "x", "$c.out");

        let mut b = make_raw_task("b");
        add_with_ref(&mut b, "x", "$a.out");

        let mut c = make_raw_task("c");
        add_with_ref(&mut c, "x", "$b.out");

        let raw = make_raw_workflow("nika/workflow@0.12", vec![a, b, c]);
        let result = analyze(raw);

        assert!(result.is_err(), "3-task implicit cycle should be detected");
        assert!(result
            .errors
            .iter()
            .any(|e| e.kind == AnalyzeErrorKind::CyclicDependency));
    }

    #[test]
    fn test_analyze_complex_jsonpath_extracts_dep() {
        // Deep JSONPath with transforms and default: $task1.data.items | sort | first(3) ?? []
        let mut task2 = make_raw_task("task2");
        add_with_ref(
            &mut task2,
            "items",
            "$task1.data.items | sort | first(3) ?? []",
        );

        let raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("task1"), task2]);
        let result = analyze(raw);
        assert!(result.is_ok());

        let wf = result.value.unwrap();
        let t2 = wf.get_task_by_name("task2").unwrap();
        assert_eq!(
            t2.implicit_deps.len(),
            1,
            "should extract dep from complex JSONPath expression"
        );
    }

    // ====================================================================
    // imports: analysis
    // ====================================================================

    #[test]
    fn test_analyze_imports() {
        use crate::ast::raw::RawImportSpec;

        let mut raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("task1")]);
        raw.imports = Some(Spanned::new(
            vec![
                Spanned::new(
                    RawImportSpec {
                        path: Spanned::new(
                            "./partials/setup.nika.yaml".to_string(),
                            make_span(0, 25),
                        ),
                        prefix: Some(Spanned::new("setup_".to_string(), make_span(30, 36))),
                        span: make_span(0, 40),
                    },
                    make_span(0, 40),
                ),
                Spanned::new(
                    RawImportSpec {
                        path: Spanned::new("./tools.nika.yaml".to_string(), make_span(50, 67)),
                        prefix: None,
                        span: make_span(50, 70),
                    },
                    make_span(50, 70),
                ),
            ],
            make_span(0, 80),
        ));

        let result = analyze(raw);
        assert!(result.is_ok());

        let workflow = result.value.unwrap();
        assert_eq!(workflow.imports.len(), 2);
        assert_eq!(workflow.imports[0].path, "./partials/setup.nika.yaml");
        assert_eq!(workflow.imports[0].prefix.as_deref(), Some("setup_"));
        assert_eq!(workflow.imports[1].path, "./tools.nika.yaml");
        assert!(workflow.imports[1].prefix.is_none());
    }

    // ====================================================================
    // Feature gating tests
    // ====================================================================

    #[test]
    fn test_feature_gate_for_each_v01_fails() {
        use crate::ast::raw::RawForEach;

        let mut task = make_raw_task("task1");
        task.for_each = Some(Spanned::new(
            RawForEach {
                items: Spanned::new("[\"a\", \"b\"]".to_string(), make_span(0, 10)),
                as_var: None,
                parallel: None,
                fail_fast: None,
            },
            make_span(0, 50),
        ));

        // Using v0.1 which doesn't support for_each
        let raw = make_raw_workflow("nika/workflow@0.1", vec![task]);
        let result = analyze(raw);

        assert!(result.is_err());
        assert!(result
            .errors
            .iter()
            .any(|e| e.kind == AnalyzeErrorKind::UnsupportedFeature));
        assert!(result.errors[0].message.contains("for_each"));
    }

    #[test]
    fn test_feature_gate_for_each_v03_succeeds() {
        use crate::ast::raw::RawForEach;

        let mut task = make_raw_task("task1");
        task.for_each = Some(Spanned::new(
            RawForEach {
                items: Spanned::new("[\"a\", \"b\"]".to_string(), make_span(0, 10)),
                as_var: None,
                parallel: None,
                fail_fast: None,
            },
            make_span(0, 50),
        ));

        // Using v0.3 which supports for_each
        let raw = make_raw_workflow("nika/workflow@0.3", vec![task]);
        let result = analyze(raw);

        // Should not have UnsupportedFeature errors
        assert!(!result
            .errors
            .iter()
            .any(|e| e.kind == AnalyzeErrorKind::UnsupportedFeature));
    }

    #[test]
    fn test_feature_gate_retry_v02_fails() {
        use crate::ast::raw::RawRetryConfig;

        let mut task = make_raw_task("task1");
        task.retry = Some(Spanned::new(
            RawRetryConfig {
                max_attempts: Some(Spanned::new(3, make_span(0, 1))),
                delay_ms: None,
                backoff: None,
            },
            make_span(0, 30),
        ));

        // Using v0.2 which doesn't support retry
        let raw = make_raw_workflow("nika/workflow@0.2", vec![task]);
        let result = analyze(raw);

        assert!(result.is_err());
        assert!(result
            .errors
            .iter()
            .any(|e| e.kind == AnalyzeErrorKind::UnsupportedFeature));
        assert!(result.errors[0].message.contains("retry"));
    }

    #[test]
    fn test_feature_gate_invoke_v01_fails() {
        use crate::ast::raw::RawInvokeAction;

        let mut task = make_raw_task("task1");
        task.action = Some(RawTaskAction::Invoke(Spanned::new(
            RawInvokeAction {
                tool: Spanned::new("novanet:search".to_string(), make_span(0, 14)),
                mcp: None,
                params: None,
                timeout_ms: None,
            },
            make_span(0, 50),
        )));

        // Using v0.1 which doesn't support invoke
        let raw = make_raw_workflow("nika/workflow@0.1", vec![task]);
        let result = analyze(raw);

        assert!(result.is_err());
        assert!(result
            .errors
            .iter()
            .any(|e| e.kind == AnalyzeErrorKind::UnsupportedFeature));
        assert!(result.errors[0].message.contains("invoke"));
    }

    #[test]
    fn test_feature_gate_agent_v01_fails() {
        use crate::ast::raw::RawAgentAction;

        let mut task = make_raw_task("task1");
        task.action = Some(RawTaskAction::Agent(Spanned::new(
            RawAgentAction {
                prompt: Spanned::new("Do something".to_string(), make_span(0, 12)),
                tools: None,
                max_iterations: None,
                max_tokens: None,
                from: None,
                skills: None,
                provider: None,
                model: None,
                mcp: None,
                system: None,
                temperature: None,
                token_budget: None,
                extended_thinking: None,
                thinking_budget: None,
                depth_limit: None,
            },
            make_span(0, 50),
        )));

        // Using v0.1 which doesn't support agent
        let raw = make_raw_workflow("nika/workflow@0.1", vec![task]);
        let result = analyze(raw);

        assert!(result.is_err());
        assert!(result
            .errors
            .iter()
            .any(|e| e.kind == AnalyzeErrorKind::UnsupportedFeature));
        assert!(result.errors[0].message.contains("agent"));
    }

    #[test]
    fn test_feature_gate_with_v11_fails() {
        let mut task = make_raw_task("task1");
        add_with_ref(&mut task, "data", "$other");

        // Doesn't support with:
        let raw = make_raw_workflow("nika/workflow@0.11", vec![task]);
        let result = analyze(raw);

        assert!(result.is_err());
        assert!(result
            .errors
            .iter()
            .any(|e| e.kind == AnalyzeErrorKind::UnsupportedFeature));
    }

    #[test]
    fn test_feature_gate_depends_on_v11_fails() {
        let mut task = make_raw_task("task1");
        add_depends_on(&mut task, &["other"]);

        // Doesn't support depends_on:
        let raw = make_raw_workflow("nika/workflow@0.11", vec![task]);
        let result = analyze(raw);

        assert!(result.is_err());
        assert!(result
            .errors
            .iter()
            .any(|e| e.kind == AnalyzeErrorKind::UnsupportedFeature));
    }

    #[test]
    fn test_feature_gate_imports_v11_fails() {
        use crate::ast::raw::RawImportSpec;

        let mut raw = make_raw_workflow("nika/workflow@0.11", vec![make_raw_task("task1")]);
        raw.imports = Some(Spanned::new(
            vec![Spanned::new(
                RawImportSpec {
                    path: Spanned::new("./setup.nika.yaml".to_string(), make_span(0, 17)),
                    prefix: None,
                    span: make_span(0, 20),
                },
                make_span(0, 20),
            )],
            make_span(0, 30),
        ));

        let result = analyze(raw);

        assert!(result.is_err());
        assert!(result
            .errors
            .iter()
            .any(|e| e.kind == AnalyzeErrorKind::UnsupportedFeature));
    }

    #[test]
    fn test_feature_gate_multiple_errors() {
        use crate::ast::raw::{RawAgentAction, RawForEach};

        let mut task = make_raw_task("task1");

        // Add both for_each and agent action
        task.for_each = Some(Spanned::new(
            RawForEach {
                items: Spanned::new("[\"a\"]".to_string(), make_span(0, 5)),
                as_var: None,
                parallel: None,
                fail_fast: None,
            },
            make_span(0, 30),
        ));
        task.action = Some(RawTaskAction::Agent(Spanned::new(
            RawAgentAction {
                prompt: Spanned::new("Goal".to_string(), make_span(0, 4)),
                tools: None,
                max_iterations: None,
                max_tokens: None,
                from: None,
                skills: None,
                provider: None,
                model: None,
                mcp: None,
                system: None,
                temperature: None,
                token_budget: None,
                extended_thinking: None,
                thinking_budget: None,
                depth_limit: None,
            },
            make_span(0, 50),
        )));

        // Using v0.1 which doesn't support either
        let raw = make_raw_workflow("nika/workflow@0.1", vec![task]);
        let result = analyze(raw);

        // Should have multiple UnsupportedFeature errors
        let feature_errors: Vec<_> = result
            .errors
            .iter()
            .filter(|e| e.kind == AnalyzeErrorKind::UnsupportedFeature)
            .collect();
        assert_eq!(feature_errors.len(), 2);
    }

    #[test]
    fn test_feature_gate_error_message_format() {
        use crate::ast::raw::RawForEach;

        let mut task = make_raw_task("task1");
        task.for_each = Some(Spanned::new(
            RawForEach {
                items: Spanned::new("[\"x\"]".to_string(), make_span(0, 5)),
                as_var: None,
                parallel: None,
                fail_fast: None,
            },
            make_span(0, 30),
        ));

        let raw = make_raw_workflow("nika/workflow@0.1", vec![task]);
        let result = analyze(raw);

        assert!(result.is_err());
        let err = &result.errors[0];
        assert!(err.message.contains("requires schema version"));
        assert!(err.message.contains("nika/workflow@0.3"));
        assert!(err.message.contains("nika/workflow@0.1"));
        assert!(err.suggestion.as_ref().unwrap().contains("upgrade"));
    }

    // ====================================================================
    // Metadata extraction
    // ====================================================================

    #[test]
    fn test_analyze_metadata() {
        let mut raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("task1")]);
        raw.workflow = Some(Spanned::new("my-workflow".to_string(), make_span(0, 11)));
        raw.description = Some(Spanned::new(
            "A test workflow".to_string(),
            make_span(0, 15),
        ));
        raw.provider = Some(Spanned::new("claude".to_string(), make_span(0, 6)));
        raw.model = Some(Spanned::new(
            "claude-sonnet-4-6".to_string(),
            make_span(0, 15),
        ));

        let result = analyze(raw);
        assert!(result.is_ok());

        let workflow = result.value.unwrap();
        assert_eq!(workflow.name.as_deref(), Some("my-workflow"));
        assert_eq!(workflow.description.as_deref(), Some("A test workflow"));
        assert_eq!(workflow.provider.as_deref(), Some("claude"));
        assert_eq!(workflow.model.as_deref(), Some("claude-sonnet-4-6"));
    }

    // ====================================================================
    // Inputs analysis
    // ====================================================================

    #[test]
    fn test_analyze_inputs() {
        let mut raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("task1")]);

        let mut inputs = IndexMap::new();
        inputs.insert(
            Spanned::new("topic".to_string(), make_span(0, 5)),
            Spanned::new(serde_json::Value::String("AI".to_string()), make_span(0, 4)),
        );
        inputs.insert(
            Spanned::new("count".to_string(), make_span(0, 5)),
            Spanned::new(serde_json::json!(3), make_span(0, 1)),
        );
        raw.inputs = Some(Spanned::new(inputs, make_span(0, 50)));

        let result = analyze(raw);
        assert!(result.is_ok());

        let workflow = result.value.unwrap();
        assert_eq!(workflow.inputs.len(), 2);
        assert_eq!(
            workflow.inputs.get("topic"),
            Some(&serde_json::Value::String("AI".to_string()))
        );
    }

    // ====================================================================
    // validate() tests
    // ====================================================================

    #[test]
    fn test_validate_valid_workflow() {
        let raw = make_raw_workflow(
            "nika/workflow@0.12",
            vec![make_raw_task("task1"), make_raw_task("task2")],
        );

        let result = validate(&raw);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_invalid_schema() {
        let raw = make_raw_workflow("invalid", vec![]);
        let result = validate(&raw);
        assert!(result.is_err());
        assert_eq!(result.errors[0].kind, AnalyzeErrorKind::InvalidSchema);
    }

    #[test]
    fn test_validate_duplicate_task() {
        let raw = make_raw_workflow(
            "nika/workflow@0.12",
            vec![make_raw_task("task1"), make_raw_task("task1")],
        );

        let result = validate(&raw);
        assert!(result.is_err());
        assert_eq!(result.errors[0].kind, AnalyzeErrorKind::DuplicateTask);
    }

    #[test]
    fn test_validate_unknown_task_in_with() {
        let mut task1 = make_raw_task("task1");
        add_with_ref(&mut task1, "data", "$unknown_task");

        let raw = make_raw_workflow("nika/workflow@0.12", vec![task1]);
        let result = validate(&raw);

        assert!(result.is_err());
        assert_eq!(result.errors[0].kind, AnalyzeErrorKind::UnknownTask);
    }

    #[test]
    fn test_validate_unknown_task_in_depends_on() {
        let mut task1 = make_raw_task("task1");
        add_depends_on(&mut task1, &["nonexistent"]);

        let raw = make_raw_workflow("nika/workflow@0.12", vec![task1]);
        let result = validate(&raw);

        assert!(result.is_err());
        assert_eq!(result.errors[0].kind, AnalyzeErrorKind::UnknownTask);
    }

    #[test]
    fn test_validate_invalid_binding() {
        let mut task1 = make_raw_task("task1");
        add_with_ref(&mut task1, "data", "");

        let raw = make_raw_workflow("nika/workflow@0.12", vec![task1]);
        let result = validate(&raw);

        assert!(result.is_err());
        assert_eq!(result.errors[0].kind, AnalyzeErrorKind::InvalidBinding);
    }

    #[test]
    fn test_validate_cyclic_dependency_depends_on() {
        let mut task1 = make_raw_task("task1");
        let mut task2 = make_raw_task("task2");

        add_depends_on(&mut task1, &["task2"]);
        add_depends_on(&mut task2, &["task1"]);

        let raw = make_raw_workflow("nika/workflow@0.12", vec![task1, task2]);
        let result = validate(&raw);

        assert!(result.is_err());
        assert!(result
            .errors
            .iter()
            .any(|e| e.kind == AnalyzeErrorKind::CyclicDependency));
    }

    #[test]
    fn test_validate_cyclic_dependency_via_with() {
        let mut task1 = make_raw_task("task1");
        let mut task2 = make_raw_task("task2");

        add_with_ref(&mut task1, "data", "$task2");
        add_with_ref(&mut task2, "data", "$task1");

        let raw = make_raw_workflow("nika/workflow@0.12", vec![task1, task2]);
        let result = validate(&raw);

        assert!(result.is_err());
        assert!(result
            .errors
            .iter()
            .any(|e| e.kind == AnalyzeErrorKind::CyclicDependency));
    }

    #[test]
    fn test_validate_feature_gate() {
        use crate::ast::raw::RawForEach;

        let mut task = make_raw_task("task1");
        task.for_each = Some(Spanned::new(
            RawForEach {
                items: Spanned::new("[\"a\"]".to_string(), make_span(0, 5)),
                as_var: None,
                parallel: None,
                fail_fast: None,
            },
            make_span(0, 30),
        ));

        let raw = make_raw_workflow("nika/workflow@0.1", vec![task]);
        let result = validate(&raw);

        assert!(result.is_err());
        assert!(result
            .errors
            .iter()
            .any(|e| e.kind == AnalyzeErrorKind::UnsupportedFeature));
    }

    #[test]
    fn test_validate_agrees_with_analyze() {
        // Ensure validate() and analyze() produce the same errors for invalid workflows
        let mut task1 = make_raw_task("task1");
        add_with_ref(&mut task1, "data", "$nonexistent");
        add_depends_on(&mut task1, &["also_missing"]);

        let raw = make_raw_workflow("nika/workflow@0.12", vec![task1]);

        let validate_result = validate(&raw);
        let analyze_result = analyze(raw.clone());

        // Both should fail
        assert!(validate_result.is_err());
        assert!(analyze_result.is_err());

        // Both should report the same error kinds
        let validate_kinds: Vec<_> = validate_result.errors.iter().map(|e| &e.kind).collect();
        let analyze_kinds: Vec<_> = analyze_result.errors.iter().map(|e| &e.kind).collect();
        assert_eq!(validate_kinds, analyze_kinds);
    }

    #[test]
    fn test_validate_valid_with_bindings() {
        let mut task2 = make_raw_task("task2");
        add_with_ref(&mut task2, "data", "$task1");

        let raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("task1"), task2]);
        let result = validate(&raw);
        assert!(result.is_ok());
    }

    // ====================================================================
    // MCP server validation
    // ====================================================================

    #[test]
    fn test_analyze_mcp_stdio_server_requires_command() {
        let mut raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("task1")]);

        // Create an MCP server with no command (stdio transport is default)
        let mut mcp_config = RawMcpConfig::new();
        mcp_config.servers.insert(
            Spanned::new("broken".to_string(), make_span(10, 16)),
            Spanned::new(RawMcpServer::default(), make_span(20, 30)),
        );
        raw.mcp = Some(Spanned::new(mcp_config, make_span(5, 35)));

        let result = analyze(raw);
        assert!(result.is_err());
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].kind, AnalyzeErrorKind::MissingField);
        let msg = result.errors[0].message.to_lowercase();
        assert!(
            msg.contains("command"),
            "error message should mention command: {}",
            msg
        );
        assert!(
            msg.contains("broken"),
            "error message should mention server name: {}",
            msg
        );
    }

    #[test]
    fn test_analyze_mcp_stdio_server_with_command_ok() {
        let mut raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("task1")]);

        let mut mcp_config = RawMcpConfig::new();
        mcp_config.servers.insert(
            Spanned::new("novanet".to_string(), make_span(10, 17)),
            Spanned::new(
                RawMcpServer::with_command("cargo run -p novanet-mcp"),
                make_span(20, 60),
            ),
        );
        raw.mcp = Some(Spanned::new(mcp_config, make_span(5, 65)));

        let result = analyze(raw);
        assert!(result.is_ok());
    }

    #[test]
    fn test_analyze_mcp_sse_server_without_command_ok() {
        let mut raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("task1")]);

        // SSE server does not need a command — it uses a URL
        let mut mcp_config = RawMcpConfig::new();
        mcp_config.servers.insert(
            Spanned::new("remote".to_string(), make_span(10, 16)),
            Spanned::new(
                RawMcpServer::with_url("http://localhost:8080"),
                make_span(20, 60),
            ),
        );
        raw.mcp = Some(Spanned::new(mcp_config, make_span(5, 65)));

        let result = analyze(raw);
        assert!(result.is_ok(), "SSE server should not cause error");
        // SSE server should produce a warning about being dropped
        assert!(
            !result.warnings.is_empty(),
            "SSE server should produce a warning"
        );
        assert!(
            result.warnings[0].message.contains("SSE"),
            "warning should mention SSE, got: {}",
            result.warnings[0].message
        );
    }

    #[test]
    fn test_analyze_mcp_stdio_server_empty_command() {
        let mut raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("task1")]);

        // A server with an empty command string should also fail
        let mut mcp_config = RawMcpConfig::new();
        let server = RawMcpServer {
            command: Some(Spanned::new(String::new(), make_span(25, 25))),
            ..Default::default()
        };
        mcp_config.servers.insert(
            Spanned::new("empty_cmd".to_string(), make_span(10, 19)),
            Spanned::new(server, make_span(20, 30)),
        );
        raw.mcp = Some(Spanned::new(mcp_config, make_span(5, 35)));

        let result = analyze(raw);
        assert!(result.is_err());
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].kind, AnalyzeErrorKind::MissingField);
    }

    #[test]
    fn test_analyze_rejects_empty_tasks_array() {
        let raw = make_raw_workflow("nika/workflow@0.12", vec![]);
        let result = analyze(raw);
        assert!(result.is_err(), "empty tasks array should be rejected");
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].kind, AnalyzeErrorKind::InvalidValue);
        assert!(
            result.errors[0].message.contains("empty"),
            "error should mention empty, got: {}",
            result.errors[0].message
        );
    }

    #[test]
    fn test_retry_on_infer_emits_warning() {
        use crate::ast::raw::RawRetryConfig;

        let mut task = make_raw_task("my_task");
        task.action = Some(RawTaskAction::Infer(Spanned::new(
            RawInferAction {
                prompt: Spanned::new("Generate something".to_string(), make_span(0, 18)),
                ..Default::default()
            },
            make_span(0, 50),
        )));
        task.retry = Some(Spanned::new(
            RawRetryConfig {
                max_attempts: Some(Spanned::new(3, make_span(0, 1))),
                delay_ms: Some(Spanned::new(1000, make_span(0, 4))),
                ..Default::default()
            },
            make_span(0, 30),
        ));

        let raw = make_raw_workflow("nika/workflow@0.12", vec![task]);
        let result = analyze(raw);

        // Should succeed (retry on infer is not an error)
        assert!(
            result.is_ok(),
            "retry on infer should not be an error: {:?}",
            result.errors
        );
        // But should emit a warning
        assert!(
            !result.warnings.is_empty(),
            "retry on non-fetch verb should emit a warning"
        );
        assert!(
            result.warnings[0].message.contains("retry"),
            "Warning should mention retry, got: {}",
            result.warnings[0].message
        );
    }

    #[test]
    fn test_retry_on_fetch_no_warning() {
        use crate::ast::raw::RawRetryConfig;

        let mut task = make_raw_task("my_fetch");
        task.action = Some(RawTaskAction::Fetch(Spanned::new(
            RawFetchAction {
                url: Spanned::new("https://example.com".to_string(), make_span(0, 20)),
                ..Default::default()
            },
            make_span(0, 50),
        )));
        task.retry = Some(Spanned::new(
            RawRetryConfig {
                max_attempts: Some(Spanned::new(3, make_span(0, 1))),
                ..Default::default()
            },
            make_span(0, 30),
        ));

        let raw = make_raw_workflow("nika/workflow@0.12", vec![task]);
        let result = analyze(raw);

        // Should succeed with no warnings
        assert!(
            result.is_ok(),
            "retry on fetch should succeed: {:?}",
            result.errors
        );
        assert!(
            result.warnings.is_empty(),
            "retry on fetch should NOT emit a warning, got: {:?}",
            result.warnings
        );
    }

    // ====================================================================
    // Task ID format validation
    // ====================================================================

    #[test]
    fn test_analyze_empty_task_id() {
        let raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("")]);
        let result = analyze(raw);
        assert!(result.is_err(), "empty task ID should be rejected");
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.kind == AnalyzeErrorKind::InvalidValue),
            "expected InvalidValue for empty task ID, got: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_analyze_task_id_with_spaces() {
        let raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("my task")]);
        let result = analyze(raw);
        assert!(result.is_err(), "task ID with spaces should be rejected");
    }

    #[test]
    fn test_analyze_task_id_dollar_prefix() {
        let raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("$reserved")]);
        let result = analyze(raw);
        assert!(
            result.is_err(),
            "task ID starting with $ should be rejected"
        );
    }

    #[test]
    fn test_analyze_valid_task_id_with_hyphens_underscores() {
        let raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("my-task_v2")]);
        let result = analyze(raw);
        assert!(
            result.is_ok(),
            "task ID with hyphens/underscores is valid: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_analyze_valid_task_id_with_dots() {
        let raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("seo.keyword")]);
        let result = analyze(raw);
        assert!(
            result.is_ok(),
            "task ID with dots is valid: {:?}",
            result.errors
        );
    }

    // ====================================================================
    // Duplicate import prefix detection
    // ====================================================================

    #[test]
    fn test_analyze_duplicate_import_prefix() {
        let mut raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("main")]);
        let imports = vec![
            Spanned::new(
                RawImportSpec {
                    path: Spanned::new("./lib1.nika.yaml".to_string(), make_span(0, 20)),
                    prefix: Some(Spanned::new("seo_".to_string(), make_span(0, 4))),
                    span: make_span(0, 30),
                },
                make_span(0, 30),
            ),
            Spanned::new(
                RawImportSpec {
                    path: Spanned::new("./lib2.nika.yaml".to_string(), make_span(0, 20)),
                    prefix: Some(Spanned::new("seo_".to_string(), make_span(0, 4))),
                    span: make_span(0, 30),
                },
                make_span(0, 30),
            ),
        ];
        raw.imports = Some(Spanned::new(imports, make_span(0, 100)));

        let result = analyze(raw);
        assert!(
            result.is_err(),
            "duplicate import prefix should be rejected"
        );
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.message.contains("duplicate import prefix")),
            "error should mention duplicate prefix: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_analyze_distinct_import_prefixes_ok() {
        let mut raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("main")]);
        let imports = vec![
            Spanned::new(
                RawImportSpec {
                    path: Spanned::new("./lib1.nika.yaml".to_string(), make_span(0, 20)),
                    prefix: Some(Spanned::new("seo_".to_string(), make_span(0, 4))),
                    span: make_span(0, 30),
                },
                make_span(0, 30),
            ),
            Spanned::new(
                RawImportSpec {
                    path: Spanned::new("./lib2.nika.yaml".to_string(), make_span(0, 20)),
                    prefix: Some(Spanned::new("content_".to_string(), make_span(0, 8))),
                    span: make_span(0, 30),
                },
                make_span(0, 30),
            ),
        ];
        raw.imports = Some(Spanned::new(imports, make_span(0, 100)));

        let result = analyze(raw);
        assert!(
            result.is_ok(),
            "distinct prefixes should be accepted: {:?}",
            result.errors
        );
    }

    // ====================================================================
    // Multi-error collection and self-cycle
    // ====================================================================

    #[test]
    fn test_analyze_collects_all_errors() {
        // Workflow with 2 problems: duplicate task ID and unknown with: ref
        let mut task1a = make_raw_task("task1");
        add_with_ref(&mut task1a, "x", "$nonexistent");
        let task1b = make_raw_task("task1"); // duplicate

        let raw = make_raw_workflow("nika/workflow@0.12", vec![task1a, task1b]);
        let result = analyze(raw);
        assert!(result.is_err());
        assert!(
            result.errors.len() >= 2,
            "analyzer should collect all errors, got {}: {:?}",
            result.errors.len(),
            result.errors
        );
    }

    #[test]
    fn test_analyze_self_cycle() {
        let mut task = make_raw_task("loop_task");
        add_depends_on(&mut task, &["loop_task"]);

        let raw = make_raw_workflow("nika/workflow@0.12", vec![task]);
        let result = analyze(raw);
        assert!(result.is_err());
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.kind == AnalyzeErrorKind::CyclicDependency),
            "self-cycle should be detected: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_analyze_self_cycle_via_with() {
        // A task referencing itself via with: binding must be caught as a cycle
        let mut task = make_raw_task("self_ref");
        add_with_ref(&mut task, "data", "$self_ref");

        let raw = make_raw_workflow("nika/workflow@0.12", vec![task]);
        let result = analyze(raw);
        assert!(result.is_err());
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.kind == AnalyzeErrorKind::CyclicDependency),
            "self-cycle via with: should be detected: {:?}",
            result.errors
        );
    }

    // ── BUG-1: context.files must be transferred from raw to analyzed ──

    #[test]
    fn test_context_files_transferred_to_analyzed() {
        let task = make_raw_task("greet");
        let mut raw = make_raw_workflow("nika/workflow@0.9", vec![task]);

        // Add context with two files
        let mut files = IndexMap::new();
        files.insert(
            Spanned::new("brand".to_string(), make_span(0, 5)),
            Spanned::new("./context/brand.md".to_string(), make_span(0, 18)),
        );
        files.insert(
            Spanned::new("persona".to_string(), make_span(0, 7)),
            Spanned::new("./context/persona.json".to_string(), make_span(0, 22)),
        );
        raw.context = Some(Spanned::new(
            RawContextConfig { files: Some(files) },
            make_span(0, 50),
        ));

        let result = analyze(raw);
        assert!(result.is_ok(), "analyze failed: {:?}", result.errors);

        let workflow = result.value.unwrap();
        assert_eq!(
            workflow.context_files.len(),
            2,
            "expected 2 context files, got {}",
            workflow.context_files.len()
        );

        // Verify aliases and paths are preserved
        let aliases: Vec<&str> = workflow
            .context_files
            .iter()
            .filter_map(|cf| cf.alias.as_deref())
            .collect();
        assert!(aliases.contains(&"brand"), "missing 'brand' alias");
        assert!(aliases.contains(&"persona"), "missing 'persona' alias");

        let paths: Vec<&str> = workflow
            .context_files
            .iter()
            .map(|cf| cf.path.as_str())
            .collect();
        assert!(paths.contains(&"./context/brand.md"));
        assert!(paths.contains(&"./context/persona.json"));
    }

    #[test]
    fn test_context_files_empty_when_no_context_block() {
        let task = make_raw_task("greet");
        let raw = make_raw_workflow("nika/workflow@0.12", vec![task]);

        let result = analyze(raw);
        assert!(result.is_ok());

        let workflow = result.value.unwrap();
        assert!(
            workflow.context_files.is_empty(),
            "context_files should be empty when no context: block"
        );
    }
}
