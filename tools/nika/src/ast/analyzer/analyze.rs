//! Analyzer: raw::Workflow → analyzed::Workflow
//!
//! This is the core Phase 2 transformation that:
//! 1. Validates schema version
//! 2. Builds task table (interned IDs)
//! 3. Resolves all task references
//! 4. Detects cyclic dependencies
//! 5. Collects all errors with precise spans

use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};

#[cfg(test)]
use super::errors::AnalyzeErrorKind;
use super::errors::{AnalyzeError, AnalyzeResult};
use super::suggestions::find_similar;
use crate::ast::analyzed::{
    AnalyzedAgentAction, AnalyzedExecAction, AnalyzedFetchAction, AnalyzedInferAction,
    AnalyzedInvokeAction, AnalyzedOutput, AnalyzedTask, AnalyzedTaskAction, AnalyzedUseRef,
    AnalyzedWorkflow, HttpMethod, OutputFormat, SchemaVersion, TaskId, TaskTable,
};
use crate::ast::raw::{
    RawAgentAction, RawExecAction, RawFetchAction, RawFlow, RawInferAction, RawInvokeAction,
    RawTask, RawTaskAction, RawWorkflow,
};
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

    // 2. Extract metadata
    workflow.name = raw.workflow.as_ref().map(|s| s.value.clone());
    workflow.description = raw.description.map(|s| s.value);
    workflow.provider = raw.provider.map(|s| s.value);
    workflow.model = raw.model.map(|s| s.value);

    // 3. Build task table (first pass - collect all task IDs)
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

    // 4. Analyze each task (second pass - resolve references)
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

    // 5. Detect cyclic dependencies
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

/// Analyze a single task.
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
        use_refs: IndexMap::new(),
        flow_deps: Vec::new(),
        output: None,
        span: raw.span,
    };

    // Analyze action
    if let Some(ref action) = raw.action {
        task.action = analyze_action(action);
    }

    // Resolve use: references
    if let Some(ref use_refs) = raw.use_refs {
        for (alias_spanned, target) in use_refs.value.iter() {
            let alias = &alias_spanned.value;
            let target_name = target.task_id();
            let target_span = target.task_span();

            if let Some(target_id) = task_table.get_id(target_name) {
                task.use_refs.insert(
                    alias.clone(),
                    AnalyzedUseRef {
                        alias: alias.clone(),
                        target: target_id,
                        path: target.path().map(|s| s.to_string()),
                        span: target_span,
                    },
                );
            } else {
                // Unknown task - suggest similar
                let all_names: Vec<&str> = all_task_names.iter().map(|s| s.as_str()).collect();
                let suggestion = find_similar(target_name, &all_names, 0.6);
                ctx.add_error(AnalyzeError::unknown_task(
                    target_span,
                    target_name,
                    suggestion.as_deref(),
                ));
            }
        }
    }

    // Resolve flow: dependencies
    if let Some(ref flow) = raw.flow {
        let flow_task_ids = flow.value.task_ids();
        for flow_task_name in flow_task_ids {
            if let Some(target_id) = task_table.get_id(flow_task_name) {
                task.flow_deps.push(target_id);
            } else {
                // Unknown task in flow
                let all_names: Vec<&str> = all_task_names.iter().map(|s| s.as_str()).collect();
                let suggestion = find_similar(flow_task_name, &all_names, 0.6);

                let span = match &flow.value {
                    RawFlow::Single(s) => s.span,
                    RawFlow::Multiple(_) => flow.span,
                    RawFlow::None => flow.span,
                };

                ctx.add_error(AnalyzeError::unknown_task(
                    span,
                    flow_task_name,
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

/// Detect cyclic dependencies using DFS.
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
        // Check flow dependencies
        for dep_id in &task.flow_deps {
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

        // Check use: dependencies
        for use_ref in task.use_refs.values() {
            let dep_id = use_ref.target;
            if !visited.contains(&dep_id) {
                detect_cycles_dfs(dep_id, workflow, visited, rec_stack, path, ctx);
            } else if rec_stack.contains(&dep_id) {
                // Found cycle via use:
                let cycle_start = path.iter().position(|&id| id == dep_id).unwrap();
                let cycle_path: Vec<&str> = path[cycle_start..]
                    .iter()
                    .filter_map(|id| workflow.task_table.get_name(*id))
                    .collect();
                let mut cycle_with_close = cycle_path.clone();
                if let Some(name) = workflow.task_table.get_name(dep_id) {
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
    use crate::ast::raw::{RawTask, RawUseTarget, RawWorkflow};
    use crate::source::{FileId, Spanned};

    fn make_span(start: u32, end: u32) -> Span {
        Span::new(FileId(0), start, end)
    }

    fn make_raw_workflow(schema: &str, tasks: Vec<RawTask>) -> RawWorkflow {
        let mut workflow = RawWorkflow::default();
        workflow.schema = Spanned::new(schema.to_string(), make_span(0, 20));
        workflow.tasks = Spanned::new(
            tasks
                .into_iter()
                .map(|t| Spanned::new(t, make_span(0, 50)))
                .collect(),
            make_span(0, 100),
        );
        workflow
    }

    fn make_raw_task(id: &str) -> RawTask {
        let mut task = RawTask::default();
        task.id = Spanned::new(id.to_string(), make_span(0, id.len() as u32));
        task
    }

    #[test]
    fn test_analyze_valid_workflow() {
        let raw = make_raw_workflow(
            "nika/workflow@0.10",
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
        // Typo in schema version
        let raw = make_raw_workflow("nika/workflow@0.9", vec![]);
        let result = analyze(raw);

        // Should succeed since 0.9 is valid
        assert!(result.is_ok());
    }

    #[test]
    fn test_analyze_duplicate_task() {
        let raw = make_raw_workflow(
            "nika/workflow@0.10",
            vec![make_raw_task("task1"), make_raw_task("task1")],
        );

        let result = analyze(raw);
        assert!(result.is_err());
        assert_eq!(result.errors[0].kind, AnalyzeErrorKind::DuplicateTask);
    }

    #[test]
    fn test_analyze_unknown_task_reference() {
        let mut task1 = make_raw_task("task1");

        // Add a use: reference to non-existent task
        let mut use_refs = IndexMap::new();
        use_refs.insert(
            Spanned::new("data".to_string(), make_span(0, 4)),
            RawUseTarget::TaskId(Spanned::new("unknown_task".to_string(), make_span(10, 22))),
        );
        task1.use_refs = Some(Spanned::new(use_refs, make_span(0, 30)));

        let raw = make_raw_workflow("nika/workflow@0.10", vec![task1]);
        let result = analyze(raw);

        assert!(result.is_err());
        assert_eq!(result.errors[0].kind, AnalyzeErrorKind::UnknownTask);
    }

    #[test]
    fn test_analyze_cyclic_dependency() {
        let mut task1 = make_raw_task("task1");
        let mut task2 = make_raw_task("task2");

        // task1 -> task2 via flow
        task1.flow = Some(Spanned::new(
            RawFlow::Single(Spanned::new("task2".to_string(), make_span(0, 5))),
            make_span(0, 10),
        ));

        // task2 -> task1 via flow (creates cycle)
        task2.flow = Some(Spanned::new(
            RawFlow::Single(Spanned::new("task1".to_string(), make_span(0, 5))),
            make_span(0, 10),
        ));

        let raw = make_raw_workflow("nika/workflow@0.10", vec![task1, task2]);
        let result = analyze(raw);

        assert!(result.is_err());
        assert!(result
            .errors
            .iter()
            .any(|e| e.kind == AnalyzeErrorKind::CyclicDependency));
    }
}
