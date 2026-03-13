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

#[cfg(test)]
use super::errors::AnalyzeErrorKind;
use super::errors::{AnalyzeError, AnalyzeResult};
use super::suggestions::find_similar;
use crate::ast::analyzed::{
    AnalyzedAgentAction, AnalyzedExecAction, AnalyzedFetchAction, AnalyzedForEach,
    AnalyzedImportSpec, AnalyzedInferAction, AnalyzedInvokeAction, AnalyzedMcpServer,
    AnalyzedOutput, AnalyzedRetry, AnalyzedTask, AnalyzedTaskAction, AnalyzedWorkflow, HttpMethod,
    McpTransport, OutputFormat, SchemaVersion, TaskId, TaskTable,
};
use crate::ast::raw::{
    RawAgentAction, RawExecAction, RawFetchAction, RawInferAction, RawInvokeAction, RawTask,
    RawTaskAction, RawWorkflow,
};
use crate::binding::{parse_with_entry, WithSpec};
use crate::source::Span;

/// Analyzer context - holds state during analysis.
struct AnalyzerContext {
    /// Task name to ID mapping
    task_table: TaskTable,
    /// Task name to span mapping (for duplicate detection)
    task_spans: HashMap<String, Span>,
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
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    fn add_error(&mut self, error: AnalyzeError) {
        self.errors.push(error);
    }

    #[allow(dead_code)]
    fn add_warning(&mut self, warning: AnalyzeError) {
        self.warnings.push(warning);
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
            );
            workflow
                .mcp_servers
                .insert(name_spanned.value.clone(), analyzed_server);
        }
    }

    // 5. Analyze imports (v0.28, replaces include: + skills:)
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

    // 7. Build task table (first pass - collect all task IDs)
    for task in raw.tasks.value.iter() {
        let task_name = &task.value.id.value;
        let task_span = task.value.id.span;

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

    // Check MCP servers (v0.2+)
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

    // Check context files (v0.9+)
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

    // Check imports (v0.12+, replaces include: + skills:)
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

    // Check inputs (v0.10+)
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
    // Check for_each (v0.3+)
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

    // Check retry (v0.3+)
    if let Some(ref retry) = task.retry {
        if !version.supports_retry() {
            ctx.add_error(AnalyzeError::unsupported_feature(
                retry.span,
                "retry",
                version_str,
                "nika/workflow@0.3",
            ));
        }
    }

    // Check with: bindings (v0.12+)
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

    // Check depends_on: (v0.12+)
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

    // Check invoke/agent verbs (v0.2+)
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
        span: raw.span,
    };

    // Analyze action
    if let Some(ref action) = raw.action {
        task.action = analyze_action(action);
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
                        } else {
                            // Unknown task reference in with: binding
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
            } else {
                // Unknown task in depends_on
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
    AnalyzedInferAction {
        prompt: raw.prompt.value.clone(),
        system: raw.system.as_ref().map(|s| s.value.clone()),
        temperature: raw.temperature.as_ref().map(|s| s.value),
        max_tokens: raw.max_tokens.as_ref().map(|s| s.value),
        stop: raw
            .stop
            .as_ref()
            .map(|s| s.value.iter().map(|v| v.value.clone()).collect())
            .unwrap_or_default(),
        thinking: raw.thinking.as_ref().map(|s| s.value),
        thinking_budget: raw.thinking_budget.as_ref().map(|s| s.value),
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
        capture_stdout: raw.capture_stdout.as_ref().map(|s| s.value).unwrap_or(true),
        capture_stderr: raw.capture_stderr.as_ref().map(|s| s.value).unwrap_or(true),
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
        goal: raw.goal.value.clone(),
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
        span: raw.goal.span,
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
) -> AnalyzedMcpServer {
    let transport = if raw.is_sse() {
        McpTransport::Sse
    } else {
        McpTransport::Stdio
    };

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
        parallel: raw.parallel.as_ref().map(|s| s.value),
        fail_fast: true, // Default to true (not yet configurable in raw)
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
    use crate::ast::raw::{RawTask, RawWorkflow};
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
        // v0.9 is valid
        let raw = make_raw_workflow("nika/workflow@0.9", vec![]);
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
                goal: Spanned::new("Do something".to_string(), make_span(0, 12)),
                tools: None,
                max_iterations: None,
                max_tokens: None,
                from: None,
                skills: None,
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

        // v0.11 doesn't support with:
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

        // v0.11 doesn't support depends_on:
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
                goal: Spanned::new("Goal".to_string(), make_span(0, 4)),
                tools: None,
                max_iterations: None,
                max_tokens: None,
                from: None,
                skills: None,
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
}
