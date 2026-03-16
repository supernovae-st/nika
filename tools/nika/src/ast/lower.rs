//! Lowering: Analyzed AST -> Runtime Types.
//!
//! Converts the validated [`AnalyzedWorkflow`] into the runtime [`Workflow`]
//! structure consumed by the execution engine.
//!
//! Phase 3 of the three-phase pipeline:
//! `YAML -> Raw -> Analyzed -> lower() -> Workflow -> Runtime`

use std::sync::Arc;

use indexmap::IndexMap;
use rustc_hash::FxHashMap;

use super::action::{ExecParams, FetchParams, InferParams, RetryConfig, TaskAction};
use super::agent::AgentParams;
use super::analyzed::{
    AnalyzedAgentAction, AnalyzedExecAction, AnalyzedFetchAction, AnalyzedForEach,
    AnalyzedImportSpec, AnalyzedInferAction, AnalyzedInvokeAction, AnalyzedMcpServer,
    AnalyzedOutput, AnalyzedRetry, AnalyzedTask, AnalyzedTaskAction, AnalyzedWorkflow, HttpMethod,
    McpTransport, OutputFormat as AnalyzedOutputFormat, TaskId, TaskTable,
};
use super::include::IncludeSpec;
use super::invoke::InvokeParams;
use super::output::{OutputFormat, OutputPolicy, SchemaRef};
use super::schema::SchemaVersion;
use super::workflow::{McpConfigInline, Task, Workflow};
use crate::source::Span;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Lower an [`AnalyzedWorkflow`] into a runtime [`Workflow`].
///
/// The analyzed workflow is consumed. All `TaskId` references are resolved
/// back to string names via the embedded [`TaskTable`].
pub fn lower(analyzed: AnalyzedWorkflow) -> Workflow {
    let AnalyzedWorkflow {
        schema_version,
        name: _,
        description: _,
        provider,
        model,
        task_table,
        tasks,
        mcp_servers,
        context_files: _,
        imports,
        inputs,
        artifacts: _,
        log: _,
        agents: _,
        span: _,
    } = analyzed;

    let tasks: Vec<Arc<Task>> = tasks
        .into_iter()
        .map(|t| Arc::new(lower_task(t, &task_table)))
        .collect();
    let mcp = lower_mcp_servers(mcp_servers);
    let inputs = lower_inputs(inputs);

    Workflow {
        schema: schema_version.as_str().to_string(),
        provider: provider.unwrap_or_else(|| "claude".to_string()),
        model,
        mcp,
        context: None,
        include: lower_imports(imports),
        agents: None,
        skills: None,
        artifacts: None,
        log: None,
        inputs,
        tasks,
    }
}

// ---------------------------------------------------------------------------
// Task
// ---------------------------------------------------------------------------

fn lower_task(task: AnalyzedTask, table: &TaskTable) -> Task {
    let flow = task_dep_names(&task.depends_on, &task.implicit_deps, table);
    let (for_each, for_each_as, fe_concurrency, fe_fail_fast) = lower_for_each(task.for_each);
    let action = lower_action(task.action, task.provider, task.model, task.retry);
    let output = task.output.map(lower_output);
    let with_spec = if task.with_spec.is_empty() {
        None
    } else {
        Some(task.with_spec)
    };

    // Use for_each concurrency/fail_fast when available, otherwise standalone values
    let concurrency = fe_concurrency.or(task.concurrency.map(|c| c as usize));
    let fail_fast = fe_fail_fast.or(task.fail_fast);

    Task {
        id: task.name,
        with_spec,
        output,
        decompose: task.decompose,
        for_each,
        for_each_as,
        concurrency,
        fail_fast,
        action,
        artifact: None,
        log: None,
        flow,
        structured: task.structured,
    }
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

pub(crate) fn lower_action(
    action: AnalyzedTaskAction,
    provider: Option<String>,
    model: Option<String>,
    retry: Option<AnalyzedRetry>,
) -> TaskAction {
    match action {
        AnalyzedTaskAction::Infer(a) => TaskAction::Infer {
            infer: lower_infer(a, provider, model),
        },
        AnalyzedTaskAction::Exec(a) => TaskAction::Exec {
            exec: lower_exec(a),
        },
        AnalyzedTaskAction::Fetch(a) => TaskAction::Fetch {
            fetch: lower_fetch(a, retry),
        },
        AnalyzedTaskAction::Invoke(a) => TaskAction::Invoke {
            invoke: lower_invoke(a),
        },
        AnalyzedTaskAction::Agent(a) => TaskAction::Agent {
            agent: lower_agent(a, provider, model),
        },
    }
}

fn lower_infer(
    infer: AnalyzedInferAction,
    provider: Option<String>,
    model: Option<String>,
) -> InferParams {
    InferParams {
        prompt: infer.prompt,
        provider,
        model,
        temperature: infer.temperature,
        max_tokens: infer.max_tokens,
        system: infer.system,
        response_format: None,
        extended_thinking: infer.thinking,
        thinking_budget: infer.thinking_budget.map(u64::from),
    }
}

fn lower_exec(e: AnalyzedExecAction) -> ExecParams {
    ExecParams {
        command: e.command,
        shell: Some(e.shell),
        timeout: e.timeout_ms,
        cwd: e.working_dir,
        env: if e.env.is_empty() {
            None
        } else {
            Some(e.env.into_iter().collect())
        },
    }
}

fn lower_fetch(fetch: AnalyzedFetchAction, retry: Option<AnalyzedRetry>) -> FetchParams {
    FetchParams {
        url: fetch.url,
        method: fetch.method.as_str().to_string(),
        headers: fetch.headers.into_iter().collect(),
        body: fetch.body,
        json: fetch.json,
        timeout: fetch.timeout_ms,
        retry: retry.map(lower_retry),
        follow_redirects: Some(fetch.follow_redirects),
    }
}

fn lower_invoke(invoke: AnalyzedInvokeAction) -> InvokeParams {
    InvokeParams {
        mcp: invoke.server,
        tool: Some(invoke.tool),
        params: invoke.params,
        resource: None,
        timeout: invoke.timeout_ms,
    }
}

fn lower_agent(
    agent: AnalyzedAgentAction,
    provider: Option<String>,
    model: Option<String>,
) -> AgentParams {
    AgentParams {
        prompt: agent.goal,
        system: None,
        provider,
        model,
        mcp: Vec::new(),
        tools: agent.tools,
        max_turns: agent.max_iterations,
        token_budget: None,
        stop_sequences: Vec::new(),
        scope: None,
        extended_thinking: None,
        thinking_budget: None,
        depth_limit: None,
        tool_choice: None,
        temperature: None,
        max_tokens: agent.max_tokens,
        skills: if agent.skills.is_empty() {
            None
        } else {
            Some(agent.skills)
        },
        completion: None,
        guardrails: Vec::new(),
        limits: None,
    }
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

pub(crate) fn lower_output(output: AnalyzedOutput) -> OutputPolicy {
    OutputPolicy {
        format: lower_output_format(output.format),
        schema: output.schema.map(SchemaRef::Inline),
        max_retries: None,
        source_structured_spec: None,
    }
}

fn lower_output_format(fmt: AnalyzedOutputFormat) -> OutputFormat {
    match fmt {
        AnalyzedOutputFormat::Text => OutputFormat::Text,
        AnalyzedOutputFormat::Json => OutputFormat::Json,
        AnalyzedOutputFormat::Yaml => OutputFormat::Yaml,
    }
}

// ---------------------------------------------------------------------------
// for_each
// ---------------------------------------------------------------------------

pub(crate) fn lower_for_each(
    fe: Option<AnalyzedForEach>,
) -> (
    Option<serde_json::Value>,
    Option<String>,
    Option<usize>,
    Option<bool>,
) {
    match fe {
        None => (None, None, None, None),
        Some(fe) => {
            // Try parsing as JSON (e.g. `["a","b"]`); fall back to string binding.
            let items = serde_json::from_str(&fe.items)
                .unwrap_or_else(|_| serde_json::Value::String(fe.items));
            let concurrency = fe.parallel.map(|p| p as usize);
            (
                Some(items),
                Some(fe.as_var),
                concurrency,
                Some(fe.fail_fast),
            )
        }
    }
}

// ---------------------------------------------------------------------------
// Retry
// ---------------------------------------------------------------------------

fn lower_retry(retry: AnalyzedRetry) -> RetryConfig {
    RetryConfig {
        max_attempts: retry.max_attempts,
        backoff_ms: retry.delay_ms,
        multiplier: retry.backoff.unwrap_or(1.0),
    }
}

fn unlower_retry(action: &TaskAction) -> Option<AnalyzedRetry> {
    let retry = match action {
        TaskAction::Fetch { fetch } => fetch.retry.as_ref(),
        _ => None,
    };
    retry.map(|r| AnalyzedRetry {
        max_attempts: r.max_attempts,
        delay_ms: r.backoff_ms,
        backoff: if (r.multiplier - 1.0).abs() > f64::EPSILON {
            Some(r.multiplier)
        } else {
            None
        },
        span: Span::dummy(),
    })
}

// ---------------------------------------------------------------------------
// MCP servers
// ---------------------------------------------------------------------------

pub(crate) fn lower_mcp_servers(
    servers: IndexMap<String, AnalyzedMcpServer>,
) -> Option<FxHashMap<String, McpConfigInline>> {
    if servers.is_empty() {
        return None;
    }
    let map: FxHashMap<String, McpConfigInline> = servers
        .into_iter()
        .filter_map(|(name, server)| match server.transport {
            McpTransport::Stdio => Some((
                name,
                McpConfigInline {
                    command: server.command.unwrap_or_default(),
                    args: server.args,
                    env: server.env.into_iter().collect(),
                    cwd: server.cwd,
                },
            )),
            McpTransport::Sse => {
                tracing::warn!(server = %name, "SSE MCP server has no runtime equivalent and will be dropped during lowering");
                None
            }
        })
        .collect();
    if map.is_empty() {
        None
    } else {
        Some(map)
    }
}

// ---------------------------------------------------------------------------
// Inputs
// ---------------------------------------------------------------------------

fn lower_inputs(
    inputs: IndexMap<String, serde_json::Value>,
) -> Option<FxHashMap<String, serde_json::Value>> {
    if inputs.is_empty() {
        None
    } else {
        Some(inputs.into_iter().collect())
    }
}

// ---------------------------------------------------------------------------
// Imports → Include
// ---------------------------------------------------------------------------

fn lower_imports(imports: Vec<AnalyzedImportSpec>) -> Option<Vec<IncludeSpec>> {
    if imports.is_empty() {
        None
    } else {
        Some(
            imports
                .into_iter()
                .map(|imp| IncludeSpec {
                    path: Some(imp.path),
                    pkg: None,
                    prefix: imp.prefix,
                })
                .collect(),
        )
    }
}

/// Build per-task dependency list for the runtime `flow` field.
fn task_dep_names(
    depends: &[TaskId],
    implicit: &[TaskId],
    table: &TaskTable,
) -> Option<Vec<String>> {
    let deps: Vec<String> = depends
        .iter()
        .chain(implicit.iter())
        .filter_map(|id| table.get_name(*id).map(String::from))
        .collect();
    if deps.is_empty() {
        None
    } else {
        Some(deps)
    }
}

// ---------------------------------------------------------------------------
// Unlower: Workflow → AnalyzedWorkflow (bridge for expand_includes)
// ---------------------------------------------------------------------------

/// Convert a lowered [`Workflow`] back into an [`AnalyzedWorkflow`].
///
/// This is the reverse of [`lower()`] and exists as a temporary bridge
/// for call sites that use `expand_includes` (which operates on the old
/// `Workflow` type) before passing to `Runner` (which expects `AnalyzedWorkflow`).
///
/// Fields that were dropped during lowering (`context_files`, `imports`,
/// `agents`, `artifacts`, `log`, `name`, `description`) are set to their
/// defaults since they have already been consumed/expanded.
pub fn unlower(workflow: Workflow) -> AnalyzedWorkflow {
    let schema_version = SchemaVersion::parse(&workflow.schema).unwrap_or(SchemaVersion::V12);

    // Build TaskTable and convert tasks
    let mut task_table = TaskTable::new();
    let mut analyzed_tasks = Vec::with_capacity(workflow.tasks.len());

    // First pass: register all task names in the table
    for task in &workflow.tasks {
        task_table.insert(&task.id);
    }

    // Second pass: convert tasks with resolved dependencies
    for task in workflow.tasks {
        let task = Arc::try_unwrap(task).unwrap_or_else(|arc| (*arc).clone());
        let id = task_table.get_id(&task.id).expect("task just inserted");

        // Resolve flow dependencies to TaskIds
        let depends_on: Vec<TaskId> = task
            .flow
            .as_ref()
            .map(|deps| {
                deps.iter()
                    .filter_map(|name| task_table.get_id(name))
                    .collect()
            })
            .unwrap_or_default();

        let action = unlower_action(&task.action);
        let output = task.output.as_ref().map(unlower_output);

        let for_each = unlower_for_each(
            task.for_each.as_ref(),
            task.for_each_as.as_ref(),
            task.concurrency,
            task.fail_fast,
        );

        let with_spec = task.with_spec.clone().unwrap_or_default();

        analyzed_tasks.push(AnalyzedTask {
            id,
            name: task.id.clone(),
            description: None,
            action,
            provider: None, // Provider is at workflow level
            model: None,    // Model is at workflow level
            with_spec,
            depends_on,
            implicit_deps: vec![],
            output,
            for_each,
            retry: unlower_retry(&task.action),
            decompose: task.decompose.clone(),
            concurrency: task.concurrency.map(|c| c as u32),
            fail_fast: task.fail_fast,
            artifact: task.artifact.clone(),
            log: task.log.clone(),
            structured: task.structured.clone(),
            span: Span::dummy(),
        });
    }

    // Convert MCP servers back to IndexMap<String, AnalyzedMcpServer>
    let mcp_servers = unlower_mcp_servers(workflow.mcp);

    // Convert inputs back to IndexMap
    let inputs: IndexMap<String, serde_json::Value> = workflow
        .inputs
        .map(|m| m.into_iter().collect())
        .unwrap_or_default();

    AnalyzedWorkflow {
        schema_version,
        name: None,
        description: None,
        provider: Some(workflow.provider),
        model: workflow.model,
        task_table,
        tasks: analyzed_tasks,
        mcp_servers,
        context_files: vec![],
        imports: vec![],
        inputs,
        artifacts: workflow.artifacts,
        log: workflow.log,
        agents: workflow
            .agents
            .map(|m| m.into_iter().collect::<IndexMap<_, _>>()),
        span: Span::dummy(),
    }
}

fn unlower_action(action: &TaskAction) -> AnalyzedTaskAction {
    match action {
        TaskAction::Infer { infer } => AnalyzedTaskAction::Infer(AnalyzedInferAction {
            prompt: infer.prompt.clone(),
            system: infer.system.clone(),
            temperature: infer.temperature,
            max_tokens: infer.max_tokens,
            stop: vec![],
            thinking: infer.extended_thinking,
            thinking_budget: infer.thinking_budget.map(|b| b as u32),
            span: Span::dummy(),
        }),
        TaskAction::Exec { exec } => AnalyzedTaskAction::Exec(AnalyzedExecAction {
            command: exec.command.clone(),
            shell: exec.shell.unwrap_or(false),
            working_dir: exec.cwd.clone(),
            env: exec
                .env
                .as_ref()
                .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default(),
            timeout_ms: exec.timeout,
            capture_stdout: true,
            capture_stderr: false,
            span: Span::dummy(),
        }),
        TaskAction::Fetch { fetch } => AnalyzedTaskAction::Fetch(AnalyzedFetchAction {
            url: fetch.url.clone(),
            method: HttpMethod::parse(&fetch.method).unwrap_or_else(|| {
                tracing::warn!(method = %fetch.method, "Unknown HTTP method in lower, defaulting to GET");
                HttpMethod::Get
            }),
            headers: fetch
                .headers
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            body: fetch.body.clone(),
            json: fetch.json.clone(),
            timeout_ms: fetch.timeout,
            follow_redirects: fetch.follow_redirects.unwrap_or(true),
            span: Span::dummy(),
        }),
        TaskAction::Invoke { invoke } => AnalyzedTaskAction::Invoke(AnalyzedInvokeAction {
            server: invoke.mcp.clone(),
            tool: invoke.tool.clone().unwrap_or_default(),
            params: invoke.params.clone(),
            timeout_ms: invoke.timeout,
            span: Span::dummy(),
        }),
        TaskAction::Agent { agent } => AnalyzedTaskAction::Agent(AnalyzedAgentAction {
            goal: agent.prompt.clone(),
            tools: agent.tools.clone(),
            max_iterations: agent.max_turns,
            max_tokens: agent.max_tokens,
            from: None,
            skills: agent
                .skills
                .as_ref()
                .map(|s| s.to_vec())
                .unwrap_or_default(),
            span: Span::dummy(),
        }),
    }
}

fn unlower_output(output: &OutputPolicy) -> AnalyzedOutput {
    let format = match output.format {
        OutputFormat::Text => AnalyzedOutputFormat::Text,
        OutputFormat::Json => AnalyzedOutputFormat::Json,
        OutputFormat::Yaml => AnalyzedOutputFormat::Yaml,
        OutputFormat::Markdown => AnalyzedOutputFormat::Text,
    };
    AnalyzedOutput {
        format,
        schema: output.schema.as_ref().map(|s| match s {
            SchemaRef::Inline(v) => v.clone(),
            SchemaRef::File(path) => serde_json::Value::String(path.clone()),
        }),
        span: Span::dummy(),
    }
}

fn unlower_for_each(
    items: Option<&serde_json::Value>,
    as_var: Option<&String>,
    concurrency: Option<usize>,
    fail_fast: Option<bool>,
) -> Option<AnalyzedForEach> {
    let items = items?;
    let items_str = if items.is_string() {
        items.as_str().unwrap().to_string()
    } else {
        serde_json::to_string(items).unwrap_or_default()
    };
    Some(AnalyzedForEach {
        items: items_str,
        as_var: as_var.cloned().unwrap_or_else(|| "item".to_string()),
        parallel: concurrency.map(|c| c as u32),
        fail_fast: fail_fast.unwrap_or(true),
        span: Span::dummy(),
    })
}

fn unlower_mcp_servers(
    mcp: Option<FxHashMap<String, McpConfigInline>>,
) -> IndexMap<String, AnalyzedMcpServer> {
    let Some(mcp) = mcp else {
        return IndexMap::new();
    };
    mcp.into_iter()
        .map(|(name, config)| {
            let server = AnalyzedMcpServer {
                name: name.clone(),
                command: Some(config.command),
                args: config.args,
                env: config.env.into_iter().collect(),
                cwd: config.cwd,
                url: None,
                transport: McpTransport::Stdio,
                span: Span::dummy(),
            };
            (name, server)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::analyzed::*;
    use crate::ast::schema::SchemaVersion;
    use crate::binding::WithSpec;
    use crate::source::Span;

    fn dummy_workflow() -> AnalyzedWorkflow {
        AnalyzedWorkflow {
            schema_version: SchemaVersion::V12,
            ..Default::default()
        }
    }

    fn dummy_task(id: TaskId, name: &str) -> AnalyzedTask {
        AnalyzedTask {
            id,
            name: name.to_string(),
            description: None,
            action: AnalyzedTaskAction::default(),
            provider: None,
            model: None,
            with_spec: WithSpec::default(),
            depends_on: vec![],
            implicit_deps: vec![],
            output: None,
            for_each: None,
            retry: None,
            decompose: None,
            concurrency: None,
            fail_fast: None,
            artifact: None,
            log: None,
            structured: None,
            span: Span::dummy(),
        }
    }

    #[test]
    fn lower_empty_workflow() {
        let wf = dummy_workflow();
        let lowered = lower(wf);
        assert_eq!(lowered.schema, "nika/workflow@0.12");
        assert_eq!(lowered.provider, "claude");
        assert!(lowered.tasks.is_empty());
        assert!(lowered.mcp.is_none());
        assert!(lowered.inputs.is_none());
    }

    #[test]
    fn lower_provider_passthrough() {
        let mut wf = dummy_workflow();
        wf.provider = Some("openai".to_string());
        wf.model = Some("gpt-4o".to_string());
        let lowered = lower(wf);
        assert_eq!(lowered.provider, "openai");
        assert_eq!(lowered.model.as_deref(), Some("gpt-4o"));
    }

    #[test]
    fn lower_infer_task() {
        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("step1");
        wf.tasks.push(AnalyzedTask {
            id,
            name: "step1".to_string(),
            action: AnalyzedTaskAction::Infer(AnalyzedInferAction {
                prompt: "Hello".to_string(),
                system: Some("You are helpful".to_string()),
                temperature: Some(0.7),
                max_tokens: Some(100),
                stop: vec![],
                thinking: Some(true),
                thinking_budget: Some(8192),
                span: Span::dummy(),
            }),
            provider: Some("mistral".to_string()),
            model: Some("mistral-large".to_string()),
            with_spec: WithSpec::default(),
            depends_on: vec![],
            implicit_deps: vec![],
            output: None,
            for_each: None,
            retry: None,
            decompose: None,
            concurrency: None,
            fail_fast: None,
            artifact: None,
            log: None,
            structured: None,
            description: None,
            span: Span::dummy(),
        });

        let lowered = lower(wf);
        assert_eq!(lowered.tasks.len(), 1);
        let task = &lowered.tasks[0];
        assert_eq!(task.id, "step1");
        match &task.action {
            TaskAction::Infer { infer } => {
                assert_eq!(infer.prompt, "Hello");
                assert_eq!(infer.system.as_deref(), Some("You are helpful"));
                assert_eq!(infer.temperature, Some(0.7));
                assert_eq!(infer.max_tokens, Some(100));
                assert_eq!(infer.provider.as_deref(), Some("mistral"));
                assert_eq!(infer.model.as_deref(), Some("mistral-large"));
                assert_eq!(infer.extended_thinking, Some(true));
                assert_eq!(infer.thinking_budget, Some(8192));
            }
            _ => panic!("expected Infer action"),
        }
    }

    #[test]
    fn lower_exec_task() {
        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("build");
        let mut env = IndexMap::new();
        env.insert("NODE_ENV".to_string(), "production".to_string());
        wf.tasks.push(AnalyzedTask {
            action: AnalyzedTaskAction::Exec(AnalyzedExecAction {
                command: "npm run build".to_string(),
                shell: true,
                working_dir: Some("/app".to_string()),
                env,
                timeout_ms: Some(30000),
                capture_stdout: true,
                capture_stderr: false,
                span: Span::dummy(),
            }),
            ..dummy_task(id, "build")
        });

        let lowered = lower(wf);
        match &lowered.tasks[0].action {
            TaskAction::Exec { exec: e } => {
                assert_eq!(e.command, "npm run build");
                assert_eq!(e.shell, Some(true));
                assert_eq!(e.cwd.as_deref(), Some("/app"));
                assert_eq!(e.timeout, Some(30000));
                let env = e.env.as_ref().unwrap();
                assert_eq!(env.get("NODE_ENV").map(String::as_str), Some("production"));
            }
            _ => panic!("expected Exec action"),
        }
    }

    #[test]
    fn lower_fetch_with_retry() {
        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("fetch_data");
        wf.tasks.push(AnalyzedTask {
            action: AnalyzedTaskAction::Fetch(AnalyzedFetchAction {
                url: "https://api.example.com".to_string(),
                method: HttpMethod::Post,
                headers: IndexMap::new(),
                body: None,
                json: Some(serde_json::json!({"key": "value"})),
                timeout_ms: Some(5000),
                follow_redirects: false,
                span: Span::dummy(),
            }),
            retry: Some(AnalyzedRetry {
                max_attempts: 3,
                delay_ms: 1000,
                backoff: Some(2.0),
                span: Span::dummy(),
            }),
            ..dummy_task(id, "fetch_data")
        });

        let lowered = lower(wf);
        match &lowered.tasks[0].action {
            TaskAction::Fetch { fetch } => {
                assert_eq!(fetch.url, "https://api.example.com");
                assert_eq!(fetch.method, "POST");
                assert_eq!(fetch.follow_redirects, Some(false));
                assert!(fetch.json.is_some());
                let retry = fetch.retry.as_ref().expect("retry should be set");
                assert_eq!(retry.max_attempts, 3);
                assert_eq!(retry.backoff_ms, 1000);
                assert_eq!(retry.multiplier, 2.0);
            }
            _ => panic!("expected Fetch action"),
        }
    }

    #[test]
    fn lower_invoke_task() {
        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("call_tool");
        wf.tasks.push(AnalyzedTask {
            action: AnalyzedTaskAction::Invoke(AnalyzedInvokeAction {
                server: Some("novanet".to_string()),
                tool: "novanet_generate".to_string(),
                params: Some(serde_json::json!({"entity": "qr-code"})),
                timeout_ms: None,
                span: Span::dummy(),
            }),
            ..dummy_task(id, "call_tool")
        });

        let lowered = lower(wf);
        match &lowered.tasks[0].action {
            TaskAction::Invoke { invoke } => {
                assert_eq!(invoke.mcp.as_deref(), Some("novanet"));
                assert_eq!(invoke.tool.as_deref(), Some("novanet_generate"));
                assert!(invoke.params.is_some());
                assert!(invoke.resource.is_none());
            }
            _ => panic!("expected Invoke action"),
        }
    }

    #[test]
    fn lower_agent_task() {
        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("researcher");
        wf.tasks.push(AnalyzedTask {
            action: AnalyzedTaskAction::Agent(AnalyzedAgentAction {
                goal: "Research AI papers".to_string(),
                tools: vec!["nika:read".to_string(), "nika:write".to_string()],
                max_iterations: Some(10),
                max_tokens: Some(4096),
                from: None,
                skills: vec!["writing".to_string()],
                span: Span::dummy(),
            }),
            provider: Some("claude".to_string()),
            ..dummy_task(id, "researcher")
        });

        let lowered = lower(wf);
        match &lowered.tasks[0].action {
            TaskAction::Agent { agent } => {
                assert_eq!(agent.prompt, "Research AI papers");
                assert_eq!(agent.tools, vec!["nika:read", "nika:write"]);
                assert_eq!(agent.max_turns, Some(10));
                assert_eq!(agent.max_tokens, Some(4096));
                assert_eq!(agent.provider.as_deref(), Some("claude"));
                assert_eq!(agent.skills.as_deref(), Some(&["writing".to_string()][..]));
            }
            _ => panic!("expected Agent action"),
        }
    }

    #[test]
    fn lower_flows_from_deps() {
        let mut wf = dummy_workflow();
        let id_a = wf.task_table.insert("a");
        let id_b = wf.task_table.insert("b");
        let id_c = wf.task_table.insert("c");

        wf.tasks.push(dummy_task(id_a, "a"));
        wf.tasks.push(AnalyzedTask {
            depends_on: vec![id_a],
            ..dummy_task(id_b, "b")
        });
        wf.tasks.push(AnalyzedTask {
            depends_on: vec![id_a],
            implicit_deps: vec![id_b],
            ..dummy_task(id_c, "c")
        });

        let lowered = lower(wf);

        // Task-level flow field
        assert!(lowered.tasks[0].flow.is_none()); // a: no deps
        assert_eq!(
            lowered.tasks[1].flow.as_deref(),
            Some(&["a".to_string()][..])
        ); // b: [a]

        let c_deps = lowered.tasks[2].flow.as_ref().unwrap();
        assert_eq!(c_deps.len(), 2);
        assert!(c_deps.contains(&"a".to_string()));
        assert!(c_deps.contains(&"b".to_string()));
    }

    #[test]
    fn lower_for_each_array() {
        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("par");
        wf.tasks.push(AnalyzedTask {
            for_each: Some(AnalyzedForEach {
                items: r#"["a","b","c"]"#.to_string(),
                as_var: "item".to_string(),
                parallel: Some(3),
                fail_fast: true,
                span: Span::dummy(),
            }),
            ..dummy_task(id, "par")
        });

        let lowered = lower(wf);
        let task = &lowered.tasks[0];
        let items = task.for_each.as_ref().unwrap();
        assert!(items.is_array());
        assert_eq!(items.as_array().unwrap().len(), 3);
        assert_eq!(task.for_each_as.as_deref(), Some("item"));
        assert_eq!(task.concurrency, Some(3));
        assert_eq!(task.fail_fast, Some(true));
    }

    #[test]
    fn lower_for_each_binding() {
        let (items, as_var, conc, _) = lower_for_each(Some(AnalyzedForEach {
            items: "$my_list".to_string(),
            as_var: "x".to_string(),
            parallel: None,
            fail_fast: false,
            span: Span::dummy(),
        }));
        assert_eq!(
            items.unwrap(),
            serde_json::Value::String("$my_list".to_string())
        );
        assert_eq!(as_var.unwrap(), "x");
        assert!(conc.is_none());
    }

    #[test]
    fn lower_mcp_stdio_only() {
        let mut servers = IndexMap::new();
        servers.insert(
            "novanet".to_string(),
            AnalyzedMcpServer {
                name: "novanet".to_string(),
                command: Some("npx".to_string()),
                args: vec!["-y".to_string(), "@novanet/mcp".to_string()],
                env: IndexMap::new(),
                cwd: None,
                url: None,
                transport: McpTransport::Stdio,
                span: Span::dummy(),
            },
        );
        servers.insert(
            "sse_only".to_string(),
            AnalyzedMcpServer {
                name: "sse_only".to_string(),
                command: None,
                args: vec![],
                env: IndexMap::new(),
                cwd: None,
                url: Some("https://mcp.example.com".to_string()),
                transport: McpTransport::Sse,
                span: Span::dummy(),
            },
        );

        let result = lower_mcp_servers(servers);
        let map = result.expect("should have stdio server");
        assert_eq!(map.len(), 1);
        assert!(map.contains_key("novanet"));
        assert!(!map.contains_key("sse_only"));
        assert_eq!(map["novanet"].command, "npx");
    }

    #[test]
    fn lower_output_json_with_schema() {
        let output = AnalyzedOutput {
            format: AnalyzedOutputFormat::Json,
            schema: Some(serde_json::json!({"type": "object"})),
            span: Span::dummy(),
        };
        let lowered = lower_output(output);
        assert!(matches!(
            lowered.format,
            crate::ast::output::OutputFormat::Json
        ));
        match lowered.schema {
            Some(SchemaRef::Inline(v)) => {
                assert_eq!(v, serde_json::json!({"type": "object"}))
            }
            _ => panic!("expected Inline schema"),
        }
    }

    #[test]
    fn lower_inputs_map() {
        let mut inputs = IndexMap::new();
        inputs.insert("name".to_string(), serde_json::json!("test"));
        inputs.insert("count".to_string(), serde_json::json!(42));
        let result = lower_inputs(inputs);
        let map = result.expect("should have inputs");
        assert_eq!(map.len(), 2);
        assert_eq!(map["name"], serde_json::json!("test"));
    }

    #[test]
    fn lower_with_spec_empty_becomes_none() {
        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("t");
        wf.tasks.push(dummy_task(id, "t"));
        let lowered = lower(wf);
        assert!(lowered.tasks[0].with_spec.is_none());
    }
}
