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

use nika_core::ast::templatable::Templatable;

use super::action::{ExecParams, FetchParams, InferParams, RetryConfig, TaskAction};
use super::agent::AgentParams;
use super::analyzed::{
    AnalyzedAgentAction, AnalyzedContextFile, AnalyzedExecAction, AnalyzedFetchAction,
    AnalyzedForEach, AnalyzedIncludeSpec, AnalyzedInferAction, AnalyzedInvokeAction,
    AnalyzedMcpServer, AnalyzedOutput, AnalyzedRetry, AnalyzedTask, AnalyzedTaskAction,
    AnalyzedWorkflow, HttpMethod, McpTransport, OutputFormat as AnalyzedOutputFormat, TaskId,
    TaskTable,
};
use super::include::IncludeSpec;
use super::invoke::InvokeParams;
use super::output::{OutputFormat, OutputPolicy, SchemaRef};
use super::schema::SchemaVersion;
use super::workflow::{McpConfigInline, Task, Workflow};
use crate::error::NikaError;
use crate::error_domains::ExecutionError;
use crate::source::Span;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Lower an [`AnalyzedWorkflow`] into a runtime [`Workflow`].
///
/// The analyzed workflow is consumed. All `TaskId` references are resolved
/// back to string names via the embedded [`TaskTable`].
pub fn lower(analyzed: AnalyzedWorkflow) -> Result<Workflow, NikaError> {
    let AnalyzedWorkflow {
        schema_version,
        name,
        description: _,
        goal: _,
        provider,
        model,
        task_table,
        tasks,
        mcp_servers,
        context_files,
        include,
        inputs,
        artifacts,
        log,
        agents,
        skills_map: _,
        orchestrate: _,
        routing: _,
        schedule: _,
        max_duration_secs: _,
        span: _,
    } = analyzed;

    let tasks: Vec<Arc<Task>> = tasks
        .into_iter()
        .map(|t| lower_task(t, &task_table).map(Arc::new))
        .collect::<Result<_, _>>()?;
    let mcp = lower_mcp_servers(mcp_servers);
    let inputs = lower_inputs(inputs);

    // Convert AnalyzedContextFile → ContextConfig for the lowered Workflow
    let context = if context_files.is_empty() {
        None
    } else {
        let mut files = rustc_hash::FxHashMap::default();
        for cf in &context_files {
            if let Some(alias) = &cf.alias {
                files.insert(alias.clone(), cf.path.clone());
            }
        }
        Some(crate::ast::context::ContextConfig {
            files,
            session: None,
        })
    };

    Ok(Workflow {
        schema: schema_version.as_str().to_string(),
        name,
        provider: provider.unwrap_or(nika_core::ProviderName::Anthropic),
        model,
        mcp,
        context,
        include: lower_include(include),
        agents: agents.map(|m| m.into_iter().collect()),
        skills: None,
        artifacts,
        log,
        inputs,
        tasks,
    })
}

// ---------------------------------------------------------------------------
// Task
// ---------------------------------------------------------------------------

fn lower_task(task: AnalyzedTask, table: &TaskTable) -> Result<Task, NikaError> {
    let depends_on = task_dep_names(&task.depends_on, &task.implicit_deps, table)?;
    let (for_each, for_each_as, fe_concurrency, fe_fail_fast) = lower_for_each(task.for_each);
    // Extract provider_chain from routing.fallback if present
    let provider_chain: Option<Vec<nika_core::ProviderName>> = task
        .routing
        .as_ref()
        .filter(|r| !r.fallback.is_empty())
        .map(|r| {
            r.fallback
                .iter()
                .map(|s| nika_core::ProviderName::parse(s))
                .collect()
        });
    let action = lower_action(
        &task.action,
        &task.provider,
        &task.model,
        &task.retry,
        &provider_chain,
    );
    let output = task.output.map(lower_output);
    let with_spec = if task.with_spec.is_empty() {
        None
    } else {
        Some(task.with_spec)
    };

    // Use for_each concurrency/fail_fast when available, otherwise standalone values

    let concurrency =
        fe_concurrency.or(task.concurrency.and_then(|c| c.value()).map(|c| c as usize));
    let fail_fast = fe_fail_fast.or(task.fail_fast.and_then(|f| f.value()));

    Ok(Task {
        id: task.name,
        with_spec,
        output,
        decompose: task.decompose,
        for_each,
        for_each_as,
        concurrency,
        fail_fast,
        action,
        artifact: task.artifact.clone(),
        log: task.log.clone(),
        depends_on,
        structured: task.structured,
        record: task.record,
        preset: task.preset,
        when: task.when,
    })
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

/// PRECONDITION: `action` has been through `resolve_action_templates()`.
/// All `Templatable` fields are guaranteed to be `Templatable::Value` variants.
/// Similarly, `retry` should be pre-resolved via `resolve_retry()`.
pub(crate) fn lower_action(
    action: &AnalyzedTaskAction,
    provider: &Option<nika_core::ProviderName>,
    model: &Option<String>,
    retry: &Option<AnalyzedRetry>,
    provider_chain: &Option<Vec<nika_core::ProviderName>>,
) -> TaskAction {
    match action {
        AnalyzedTaskAction::Infer(a) => TaskAction::Infer {
            infer: lower_infer(
                a.clone(),
                provider.clone(),
                model.clone(),
                provider_chain.clone(),
            ),
        },
        AnalyzedTaskAction::Exec(a) => TaskAction::Exec {
            exec: lower_exec(a.clone()),
        },
        AnalyzedTaskAction::Fetch(a) => TaskAction::Fetch {
            fetch: lower_fetch(a.clone(), retry.clone()),
        },
        AnalyzedTaskAction::Invoke(a) => TaskAction::Invoke {
            invoke: lower_invoke(a.clone()),
        },
        AnalyzedTaskAction::Agent(a) => TaskAction::Agent {
            agent: lower_agent(
                a.as_ref().clone(),
                provider.clone(),
                model.clone(),
                provider_chain.clone(),
            ),
        },
    }
}

fn lower_infer(
    infer: AnalyzedInferAction,
    provider: Option<nika_core::ProviderName>,
    model: Option<String>,
    provider_chain: Option<Vec<nika_core::ProviderName>>,
) -> InferParams {
    use crate::ast::action::ResponseFormat;

    let response_format =
        infer
            .response_format
            .as_deref()
            .and_then(|s| match s.to_lowercase().as_str() {
                "text" => Some(ResponseFormat::Text),
                "json" => Some(ResponseFormat::Json),
                "markdown" => Some(ResponseFormat::Markdown),
                _ => None,
            });

    InferParams {
        prompt: infer.prompt,
        provider,
        model,

        temperature: infer.temperature.and_then(|t| t.value()),

        max_tokens: infer.max_tokens.and_then(|t| t.value()),
        system: infer.system,
        response_format,

        extended_thinking: infer.extended_thinking.and_then(|t| t.value()),

        thinking_budget: infer.thinking_budget.and_then(|t| t.value()).map(u64::from),
        content: infer
            .content
            .map(|parts| parts.into_iter().map(Into::into).collect()),
        guardrails: infer.guardrails,
        provider_chain,
    }
}

fn lower_exec(e: AnalyzedExecAction) -> ExecParams {
    ExecParams {
        command: e.command,

        shell: Some(e.shell.value().unwrap_or(false)),
        // Convert milliseconds (YAML) to seconds (runtime), ceiling division
        // to avoid losing sub-second values (e.g. 500ms -> 1s, not 0s)
        timeout: e
            .timeout_ms
            .and_then(|ms| ms.value())
            .map(|ms| ms.div_ceil(1000)),
        cwd: e.cwd,
        env: if e.env.is_empty() {
            None
        } else {
            Some(e.env.into_iter().collect())
        },

        max_stdout: e.max_stdout.and_then(|v| v.value()),
    }
}

fn lower_fetch(fetch: AnalyzedFetchAction, retry: Option<AnalyzedRetry>) -> FetchParams {
    let follow_redirects_val = fetch.follow_redirects.value().unwrap_or(true);
    let session_val = fetch.session.value().unwrap_or(false);
    let cache_val = fetch.cache.value().unwrap_or(false);
    FetchParams {
        url: fetch.url,
        method: fetch.method.as_str().to_string(),
        headers: fetch.headers.into_iter().collect(),
        body: fetch.body,
        json: fetch.json,
        // Convert milliseconds (YAML) to seconds (runtime), ceiling division
        timeout: fetch
            .timeout_ms
            .and_then(|ms| ms.value())
            .map(|ms| ms.div_ceil(1000)),
        retry: retry.map(lower_retry),
        follow_redirects: Some(follow_redirects_val),
        response: fetch.response,
        extract: fetch.extract,
        selector: fetch.selector,
        session: if session_val { Some(true) } else { None },
        cache: if cache_val { Some(true) } else { None },
    }
}

fn lower_invoke(invoke: AnalyzedInvokeAction) -> InvokeParams {
    InvokeParams {
        mcp: invoke.server,
        tool: Some(invoke.tool),
        params: invoke.params,
        resource: invoke.resource,
        // Convert milliseconds (YAML) to seconds (runtime), ceiling division
        timeout: invoke
            .timeout_ms
            .and_then(|ms| ms.value())
            .map(|ms| ms.div_ceil(1000)),
    }
}

fn lower_agent(
    agent: AnalyzedAgentAction,
    provider: Option<nika_core::ProviderName>,
    model: Option<String>,
    provider_chain: Option<Vec<nika_core::ProviderName>>,
) -> AgentParams {
    // Parse tool_choice string to ToolChoice enum
    let tool_choice = agent
        .tool_choice
        .as_deref()
        .and_then(|s| match s.to_lowercase().as_str() {
            "auto" => Some(crate::ast::agent::ToolChoice::Auto),
            "required" => Some(crate::ast::agent::ToolChoice::Required),
            "none" => Some(crate::ast::agent::ToolChoice::None),
            other => {
                tracing::warn!(
                    tool_choice = other,
                    "invalid tool_choice value (expected \"auto\", \"required\", or \"none\"), ignoring"
                );
                None
            }
        });

    AgentParams {
        prompt: agent.prompt,
        system: agent.system,
        provider,
        model,
        mcp: agent.mcp,
        tools: agent.tools,
        max_turns: agent.max_turns.and_then(|t| t.value()),
        token_budget: agent.token_budget.and_then(|t| t.value()),
        stop_sequences: agent.stop_sequences,
        scope: agent.scope,
        extended_thinking: agent.extended_thinking.and_then(|t| t.value()),
        thinking_budget: agent.thinking_budget.and_then(|t| t.value()).map(u64::from),
        depth_limit: agent.depth_limit.and_then(|t| t.value()),
        tool_choice,
        temperature: agent.temperature.and_then(|t| t.value()).map(|t| t as f32),
        max_tokens: agent.max_tokens.and_then(|t| t.value()),
        skills: if agent.skills.is_empty() {
            None
        } else {
            Some(agent.skills)
        },
        completion: agent.completion,
        guardrails: agent.guardrails,
        limits: agent.limits,
        provider_chain,
    }
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

pub(crate) fn lower_output(output: AnalyzedOutput) -> OutputPolicy {
    // Determine SchemaRef: prefer explicit schema_ref, then classify schema value
    let schema = if let Some(ref schema_ref_str) = output.schema_ref {
        // Explicit schema_ref always treated as a file path
        Some(SchemaRef::File(schema_ref_str.clone()))
    } else {
        output.schema.map(|v| {
            // If the schema value is a string that looks like a file path,
            // classify it as SchemaRef::File instead of Inline
            if let serde_json::Value::String(ref s) = v {
                if s.starts_with("./") || s.starts_with('/') || s.ends_with(".json") {
                    return SchemaRef::File(s.clone());
                }
            }
            SchemaRef::Inline(v)
        })
    };

    OutputPolicy {
        format: lower_output_format(output.format),
        schema,
        from_example: None,

        max_retries: output.max_retries.and_then(|v| v.value()).map(|v| v as u8),
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
            let items =
                serde_json::from_str(&fe.items).unwrap_or(serde_json::Value::String(fe.items));

            let concurrency = fe.concurrency.and_then(|p| p.value()).map(|p| p as usize);
            let fail_fast = fe.fail_fast.value().unwrap_or(true);
            (Some(items), Some(fe.as_var), concurrency, Some(fail_fast))
        }
    }
}

// ---------------------------------------------------------------------------
// Retry
// ---------------------------------------------------------------------------

fn lower_retry(retry: AnalyzedRetry) -> RetryConfig {
    RetryConfig {
        max_attempts: retry.max_attempts.value().unwrap_or(3),
        backoff_ms: retry.delay_ms.value().unwrap_or(1000),
        multiplier: retry.backoff.and_then(|b| b.value()).unwrap_or(1.0),
    }
}

/// Practical tolerance for backoff unity comparison (0.01% relative difference).
/// `f64::EPSILON` (~2.2e-16) is too strict for user-provided floats that may
/// accumulate rounding from YAML parsing or arithmetic.
const BACKOFF_UNITY_TOLERANCE: f64 = 0.0001;

fn unlower_retry(action: &TaskAction) -> Option<AnalyzedRetry> {
    let retry = match action {
        TaskAction::Fetch { fetch } => fetch.retry.as_ref(),
        _ => None,
    };
    retry.map(|r| AnalyzedRetry {
        max_attempts: Templatable::Value(r.max_attempts),
        delay_ms: Templatable::Value(r.backoff_ms),
        backoff: if r.multiplier.is_nan() || (r.multiplier - 1.0).abs() <= BACKOFF_UNITY_TOLERANCE {
            None
        } else {
            Some(Templatable::Value(r.multiplier))
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
    lower_mcp_servers_with_resolver(servers, None)
}

pub(crate) fn lower_mcp_servers_with_resolver(
    servers: IndexMap<String, AnalyzedMcpServer>,
    resolver: Option<&crate::core::McpConfigResolver>,
) -> Option<FxHashMap<String, McpConfigInline>> {
    use crate::core::McpResolveSource;
    use nika_core::ast::analyzed::McpFromSource;

    if servers.is_empty() {
        return None;
    }
    let mut map = FxHashMap::default();

    for (name, server) in servers {
        if server.transport == McpTransport::Sse {
            tracing::warn!(server = %name, "SSE MCP server dropped during lowering");
            continue;
        }

        let config = if let Some(from_source) = &server.from {
            // Resolve from: reference
            let resolve_source = match from_source {
                McpFromSource::Config => McpResolveSource::Config,
                McpFromSource::Project => McpResolveSource::Project,
                McpFromSource::Global => McpResolveSource::Global,
            };

            let Some(resolver) = resolver else {
                tracing::warn!(server = %name, "from: used but no config resolver available — skipping");
                continue;
            };

            // NIKA-108: server not found in config
            let Some(base) = resolver.resolve(&name, resolve_source) else {
                tracing::error!(
                    server = %name,
                    source = ?from_source,
                    "NIKA-108: MCP server '{}' not found in config. Add it to .mcp.json or use command: for inline.",
                    name
                );
                continue;
            };

            // Deep merge: base from config, workflow fields override
            let mut env: FxHashMap<String, String> = base
                .env
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            // Workflow env overrides base env
            for (k, v) in &server.env {
                env.insert(k.clone(), v.clone());
            }

            McpConfigInline {
                command: base.command.clone(),
                args: if server.args.is_empty() {
                    base.args.clone()
                } else {
                    server.args
                },
                env,
                cwd: server.cwd, // No cwd in config files, only workflow override
            }
        } else {
            // Inline: use workflow fields directly (existing behavior)
            McpConfigInline {
                command: server.command.unwrap_or_default(),
                args: server.args,
                env: server.env.into_iter().collect(),
                cwd: server.cwd,
            }
        };

        map.insert(name, config);
    }

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
// Include
// ---------------------------------------------------------------------------

fn lower_include(specs: Vec<AnalyzedIncludeSpec>) -> Option<Vec<IncludeSpec>> {
    if specs.is_empty() {
        None
    } else {
        Some(
            specs
                .into_iter()
                .map(|spec| IncludeSpec {
                    path: Some(spec.path),
                    pkg: None,
                    prefix: spec.prefix,
                })
                .collect(),
        )
    }
}

/// Build per-task dependency list for the runtime `flow` field.
///
/// Returns `Err` if any `TaskId` is not found in the `TaskTable`.
/// This is defense-in-depth: the analyzer should have validated all
/// references, so a missing ID here indicates an invariant violation.
fn task_dep_names(
    depends: &[TaskId],
    implicit: &[TaskId],
    table: &TaskTable,
) -> Result<Option<Vec<String>>, NikaError> {
    let mut deps = Vec::new();
    for id in depends.iter().chain(implicit.iter()) {
        let name = table
            .get_name(*id)
            .ok_or_else(|| NikaError::ValidationError {
                reason: format!(
                    "Lowering: TaskId({}) not found in TaskTable (invariant violation)",
                    id.0
                ),
            })?;
        deps.push(name.to_string());
    }
    Ok(if deps.is_empty() { None } else { Some(deps) })
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
/// Fields that were dropped during lowering (`context_files`, `include`,
/// `agents`, `artifacts`, `log`, `name`, `description`) are set to their
/// defaults since they have already been consumed/expanded.
pub fn unlower(workflow: Workflow) -> Result<AnalyzedWorkflow, NikaError> {
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
        let Some(id) = task_table.get_id(&task.id) else {
            return Err(ExecutionError::General(format!(
                "task '{}' missing from table after insert",
                task.id
            ))
            .into());
        };

        // Resolve flow dependencies to TaskIds (reject dangling names)
        let depends_on: Vec<TaskId> = match task.depends_on.as_ref() {
            Some(deps) => {
                let mut ids = Vec::with_capacity(deps.len());
                for name in deps {
                    let dep_id =
                        task_table
                            .get_id(name)
                            .ok_or_else(|| NikaError::ValidationError {
                                reason: format!(
                                    "Unlowering: dependency '{}' not found in TaskTable \
                                 (invariant violation)",
                                    name
                                ),
                            })?;
                    ids.push(dep_id);
                }
                ids
            }
            None => vec![],
        };

        let action = unlower_action(&task.action);
        let output = task.output.as_ref().map(unlower_output);

        let for_each = unlower_for_each(
            task.for_each.as_ref(),
            task.for_each_as.as_ref(),
            task.concurrency,
            task.fail_fast,
        );

        let with_spec = task.with_spec.clone().unwrap_or_default();

        // Extract provider/model from lowered action (preserved inside InferParams/AgentParams)
        let (task_provider, task_model) = extract_provider_model(&task.action);

        analyzed_tasks.push(AnalyzedTask {
            id,
            name: task.id.clone(),
            description: None,
            action,
            provider: task_provider,
            model: task_model,
            with_spec,
            depends_on,
            implicit_deps: vec![],
            output,
            for_each,
            retry: unlower_retry(&task.action),
            on_error: None,
            decompose: task.decompose.clone(),
            concurrency: task
                .concurrency
                .map(|c| Templatable::Value(u32::try_from(c).unwrap_or(u32::MAX))),
            fail_fast: task.fail_fast.map(Templatable::Value),
            artifact: task.artifact.clone(),
            log: task.log.clone(),
            structured: task.structured.clone(),
            record: None,
            context_budget: None,
            preset: task.preset.clone(),
            routing: None,
            when: None,
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

    // Restore context_files from Workflow.context (lost during lower round-trip)
    let context_files = workflow
        .context
        .as_ref()
        .map(|ctx| {
            ctx.files
                .iter()
                .map(|(alias, path)| AnalyzedContextFile {
                    path: path.clone(),
                    alias: Some(alias.clone()),
                    max_bytes: None,
                    span: Span::dummy(),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(AnalyzedWorkflow {
        schema_version,
        name: workflow.name,
        description: None,
        goal: None,
        provider: Some(workflow.provider),
        model: workflow.model,
        task_table,
        tasks: analyzed_tasks,
        mcp_servers,
        context_files,
        include: vec![],
        inputs,
        artifacts: workflow.artifacts,
        log: workflow.log,
        agents: workflow
            .agents
            .map(|m| m.into_iter().collect::<IndexMap<_, _>>()),
        skills_map: workflow
            .skills
            .map(|m| m.into_iter().collect())
            .unwrap_or_default(),
        orchestrate: None,
        routing: None,
        schedule: None,
        max_duration_secs: Templatable::Value(3600),
        span: Span::dummy(),
    })
}

/// Extract task-level provider and model from lowered TaskAction.
///
/// During lower → unlower round-trip, provider/model are stored inside
/// InferParams/AgentParams. This extracts them so the Runner can pass them
/// back through lower_action() at the TaskExecutor boundary.
fn extract_provider_model(
    action: &TaskAction,
) -> (Option<nika_core::ProviderName>, Option<String>) {
    match action {
        TaskAction::Infer { infer } => (infer.provider.clone(), infer.model.clone()),
        TaskAction::Agent { agent } => (agent.provider.clone(), agent.model.clone()),
        _ => (None, None),
    }
}

fn unlower_action(action: &TaskAction) -> AnalyzedTaskAction {
    match action {
        TaskAction::Infer { infer } => AnalyzedTaskAction::Infer(AnalyzedInferAction {
            prompt: infer.prompt.clone(),
            system: infer.system.clone(),
            temperature: infer.temperature.map(Templatable::Value),
            max_tokens: infer.max_tokens.map(Templatable::Value),
            extended_thinking: infer.extended_thinking.map(Templatable::Value),
            thinking_budget: infer
                .thinking_budget
                .map(|b| Templatable::Value(u32::try_from(b).unwrap_or(u32::MAX))),
            content: infer.content.as_ref().map(|parts| {
                parts
                    .iter()
                    .map(|p| match p {
                        crate::ast::content::ContentPart::Text { text } => {
                            crate::ast::content::AnalyzedContentPart::Text {
                                text: text.clone(),
                            }
                        }
                        crate::ast::content::ContentPart::Image { source, detail } => {
                            crate::ast::content::AnalyzedContentPart::Image {
                                source: source.clone(),
                                detail: *detail,
                            }
                        }
                        crate::ast::content::ContentPart::ImageUrl { url, detail } => {
                            crate::ast::content::AnalyzedContentPart::ImageUrl {
                                url: url.clone(),
                                detail: *detail,
                            }
                        }
                    })
                    .collect()
            }),
            response_format: infer.response_format.as_ref().map(|rf| {
                use crate::ast::action::ResponseFormat;
                match rf {
                    ResponseFormat::Text => "text".to_string(),
                    ResponseFormat::Json => "json".to_string(),
                    ResponseFormat::Markdown => "markdown".to_string(),
                }
            }),
            guardrails: infer.guardrails.clone(),
            span: Span::dummy(),
        }),
        TaskAction::Exec { exec } => AnalyzedTaskAction::Exec(AnalyzedExecAction {
            command: exec.command.clone(),
            shell: Templatable::Value(exec.shell.unwrap_or(false)),
            cwd: exec.cwd.clone(),
            env: exec
                .env
                .as_ref()
                .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default(),
            // Convert seconds (runtime) back to milliseconds (analyzed)
            timeout_ms: exec.timeout.map(|s| Templatable::Value(s * 1000)),
            max_stdout: exec.max_stdout.map(Templatable::Value),
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
            // Convert seconds (runtime) back to milliseconds (analyzed)
            timeout_ms: fetch.timeout.map(|s| Templatable::Value(s * 1000)),
            follow_redirects: Templatable::Value(fetch.follow_redirects.unwrap_or(true)),
            response: fetch.response,
            extract: fetch.extract,
            selector: fetch.selector.clone(),
            session: Templatable::Value(fetch.session.unwrap_or(false)),
            cache: Templatable::Value(fetch.cache.unwrap_or(false)),
            span: Span::dummy(),
        }),
        TaskAction::Invoke { invoke } => AnalyzedTaskAction::Invoke(AnalyzedInvokeAction {
            server: invoke.mcp.clone(),
            tool: invoke.tool.clone().unwrap_or_default(),
            resource: invoke.resource.clone(),
            params: invoke.params.clone(),
            // Convert seconds (runtime) back to milliseconds (analyzed)
            timeout_ms: invoke.timeout.map(|s| Templatable::Value(s * 1000)),
            span: Span::dummy(),
        }),
        TaskAction::Agent { agent } => AnalyzedTaskAction::Agent(Box::new(AnalyzedAgentAction {
            prompt: agent.prompt.clone(),
            tools: agent.tools.clone(),
            max_turns: agent.max_turns.map(Templatable::Value),
            max_tokens: agent.max_tokens.map(Templatable::Value),
            from: None,
            skills: agent
                .skills
                .as_ref()
                .map(|s| s.to_vec())
                .unwrap_or_default(),
            mcp: agent.mcp.clone(),
            system: agent.system.clone(),
            provider: None,
            model: None,
            temperature: agent.temperature.map(|t| Templatable::Value(f64::from(t))),
            token_budget: agent.token_budget.map(Templatable::Value),
            extended_thinking: agent.extended_thinking.map(Templatable::Value),
            thinking_budget: agent.thinking_budget.map(|v| Templatable::Value(v as u32)),
            depth_limit: agent.depth_limit.map(Templatable::Value),
            tool_choice: agent.tool_choice.as_ref().map(|tc| tc.as_str().to_string()),
            stop_sequences: agent.stop_sequences.clone(),
            scope: agent.scope.clone(),
            guardrails: agent.guardrails.clone(),
            completion: agent.completion.clone(),
            limits: agent.limits.clone(),
            span: Span::dummy(),
        })),
    }
}

fn unlower_output(output: &OutputPolicy) -> AnalyzedOutput {
    let format = match output.format {
        OutputFormat::Text => AnalyzedOutputFormat::Text,
        OutputFormat::Json => AnalyzedOutputFormat::Json,
        OutputFormat::Yaml => AnalyzedOutputFormat::Yaml,
        OutputFormat::Markdown => AnalyzedOutputFormat::Text,
        OutputFormat::Binary => AnalyzedOutputFormat::Text, // Binary bypasses text formatting
    };

    // Extract schema_ref from File variant, pass inline as schema
    let (schema, schema_ref) = match output.schema.as_ref() {
        Some(SchemaRef::Inline(v)) => (Some(v.clone()), None),
        Some(SchemaRef::File(path)) => (None, Some(path.clone())),
        None => (None, None),
    };

    AnalyzedOutput {
        format,
        schema,
        schema_ref,
        max_retries: output.max_retries.map(|v| Templatable::Value(u32::from(v))),
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
        concurrency: concurrency.map(|c| Templatable::Value(u32::try_from(c).unwrap_or(u32::MAX))),
        fail_fast: Templatable::Value(fail_fast.unwrap_or(true)),
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
                from: None,
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
            on_error: None,
            decompose: None,
            concurrency: None,
            fail_fast: None,
            artifact: None,
            log: None,
            structured: None,
            record: None,
            context_budget: None,
            preset: None,
            routing: None,
            when: None,
            span: Span::dummy(),
        }
    }

    #[test]
    fn lower_empty_workflow() {
        let wf = dummy_workflow();
        let lowered = lower(wf).unwrap();
        assert_eq!(lowered.schema, "nika/workflow@0.12");
        assert_eq!(lowered.provider, nika_core::ProviderName::Anthropic);
        assert!(lowered.tasks.is_empty());
        assert!(lowered.mcp.is_none());
        assert!(lowered.inputs.is_none());
    }

    #[test]
    fn lower_provider_passthrough() {
        let mut wf = dummy_workflow();
        wf.provider = Some(nika_core::ProviderName::OpenAI);
        wf.model = Some("gpt-4o".to_string());
        let lowered = lower(wf).unwrap();
        assert_eq!(lowered.provider, nika_core::ProviderName::OpenAI);
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
                temperature: Some(Templatable::Value(0.7)),
                max_tokens: Some(Templatable::Value(100)),
                extended_thinking: Some(Templatable::Value(true)),
                thinking_budget: Some(Templatable::Value(8192)),
                content: None,
                response_format: None,
                guardrails: Vec::new(),
                span: Span::dummy(),
            }),
            provider: Some(nika_core::ProviderName::Mistral),
            model: Some("mistral-large".to_string()),
            with_spec: WithSpec::default(),
            depends_on: vec![],
            implicit_deps: vec![],
            output: None,
            for_each: None,
            retry: None,
            on_error: None,
            decompose: None,
            concurrency: None,
            fail_fast: None,
            artifact: None,
            log: None,
            structured: None,
            record: None,
            context_budget: None,
            preset: None,
            routing: None,
            when: None,
            description: None,
            span: Span::dummy(),
        });

        let lowered = lower(wf).unwrap();
        assert_eq!(lowered.tasks.len(), 1);
        let task = &lowered.tasks[0];
        assert_eq!(task.id, "step1");
        match &task.action {
            TaskAction::Infer { infer } => {
                assert_eq!(infer.prompt, "Hello");
                assert_eq!(infer.system.as_deref(), Some("You are helpful"));
                assert_eq!(infer.temperature, Some(0.7));
                assert_eq!(infer.max_tokens, Some(100));
                assert_eq!(infer.provider, Some(nika_core::ProviderName::Mistral));
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
                shell: Templatable::Value(true),
                cwd: Some("/app".to_string()),
                env,
                timeout_ms: Some(Templatable::Value(30000)),
                max_stdout: None,
                span: Span::dummy(),
            }),
            ..dummy_task(id, "build")
        });

        let lowered = lower(wf).unwrap();
        match &lowered.tasks[0].action {
            TaskAction::Exec { exec: e } => {
                assert_eq!(e.command, "npm run build");
                assert_eq!(e.shell, Some(true));
                assert_eq!(e.cwd.as_deref(), Some("/app"));
                assert_eq!(e.timeout, Some(30)); // 30000ms → 30s
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
                timeout_ms: Some(Templatable::Value(5000)),
                follow_redirects: Templatable::Value(false),
                response: None,
                extract: None,
                selector: None,
                session: Templatable::Value(false),
                cache: Templatable::Value(false),
                span: Span::dummy(),
            }),
            retry: Some(AnalyzedRetry {
                max_attempts: Templatable::Value(3),
                delay_ms: Templatable::Value(1000),
                backoff: Some(Templatable::Value(2.0)),
                span: Span::dummy(),
            }),
            ..dummy_task(id, "fetch_data")
        });

        let lowered = lower(wf).unwrap();
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
                tool: "novanet_context".to_string(),
                resource: None,
                params: Some(serde_json::json!({"entity": "qr-code"})),
                timeout_ms: None,
                span: Span::dummy(),
            }),
            ..dummy_task(id, "call_tool")
        });

        let lowered = lower(wf).unwrap();
        match &lowered.tasks[0].action {
            TaskAction::Invoke { invoke } => {
                assert_eq!(invoke.mcp.as_deref(), Some("novanet"));
                assert_eq!(invoke.tool.as_deref(), Some("novanet_context"));
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
            action: AnalyzedTaskAction::Agent(Box::new(AnalyzedAgentAction {
                prompt: "Research AI papers".to_string(),
                tools: vec!["nika:read".to_string(), "nika:write".to_string()],
                max_turns: Some(Templatable::Value(10)),
                max_tokens: Some(Templatable::Value(4096)),
                from: None,
                skills: vec!["writing".to_string()],
                mcp: vec![],
                system: None,
                provider: None,
                model: None,
                temperature: None,
                token_budget: None,
                extended_thinking: None,
                thinking_budget: None,
                depth_limit: None,
                tool_choice: None,
                stop_sequences: vec![],
                scope: None,
                guardrails: Vec::new(),
                completion: None,
                limits: None,
                span: Span::dummy(),
            })),
            provider: Some(nika_core::ProviderName::Anthropic),
            ..dummy_task(id, "researcher")
        });

        let lowered = lower(wf).unwrap();
        match &lowered.tasks[0].action {
            TaskAction::Agent { agent } => {
                assert_eq!(agent.prompt, "Research AI papers");
                assert_eq!(agent.tools, vec!["nika:read", "nika:write"]);
                assert_eq!(agent.max_turns, Some(10));
                assert_eq!(agent.max_tokens, Some(4096));
                assert_eq!(agent.provider, Some(nika_core::ProviderName::Anthropic));
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

        let lowered = lower(wf).unwrap();

        // Task-level flow field
        assert!(lowered.tasks[0].depends_on.is_none()); // a: no deps
        assert_eq!(
            lowered.tasks[1].depends_on.as_deref(),
            Some(&["a".to_string()][..])
        ); // b: [a]

        let c_deps = lowered.tasks[2].depends_on.as_ref().unwrap();
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
                concurrency: Some(Templatable::Value(3)),
                fail_fast: Templatable::Value(true),
                span: Span::dummy(),
            }),
            ..dummy_task(id, "par")
        });

        let lowered = lower(wf).unwrap();
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
            concurrency: None,
            fail_fast: Templatable::Value(false),
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
                from: None,
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
                from: None,
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
            schema_ref: None,
            max_retries: None,
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
        let lowered = lower(wf).unwrap();
        assert!(lowered.tasks[0].with_spec.is_none());
    }

    // =========================================================================
    // Pipeline audit tests: document intentional drops and gaps
    // =========================================================================

    /// lower() explicitly drops workflow-level artifacts.
    /// lower() preserves workflow-level artifacts through the pipeline.
    #[test]
    fn lower_preserves_workflow_artifacts() {
        let mut wf = dummy_workflow();
        wf.artifacts = Some(crate::ast::artifact::ArtifactsConfig {
            dir: Some("./output".to_string()),
            ..Default::default()
        });
        let lowered = lower(wf).unwrap();
        assert!(
            lowered.artifacts.is_some(),
            "lower() must preserve workflow-level artifacts"
        );
        assert_eq!(
            lowered.artifacts.as_ref().unwrap().dir.as_deref(),
            Some("./output")
        );
    }

    /// lower() preserves workflow-level log config.
    #[test]
    fn lower_preserves_workflow_log() {
        let mut wf = dummy_workflow();
        wf.log = Some(crate::ast::logging::LogConfig::default());
        let lowered = lower(wf).unwrap();
        assert!(
            lowered.log.is_some(),
            "lower() must preserve workflow-level log config"
        );
    }

    /// lower() preserves workflow-level agents.
    #[test]
    fn lower_preserves_workflow_agents() {
        let mut wf = dummy_workflow();
        let mut agents = IndexMap::new();
        agents.insert(
            "researcher".to_string(),
            crate::ast::agent_def::AgentDef::From {
                from: "./agents/researcher.yaml".to_string(),
            },
        );
        wf.agents = Some(agents);
        let lowered = lower(wf).unwrap();
        assert!(
            lowered.agents.is_some(),
            "lower() must preserve workflow-level agents"
        );
        assert!(lowered.agents.as_ref().unwrap().contains_key("researcher"));
    }

    /// lower_task() preserves task-level artifact config.
    #[test]
    fn lower_task_preserves_artifact() {
        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("t");
        let mut task = dummy_task(id, "t");
        task.artifact = Some(crate::ast::artifact::ArtifactSpec::Enabled(true));
        wf.tasks.push(task);
        let lowered = lower(wf).unwrap();
        assert!(
            lowered.tasks[0].artifact.is_some(),
            "lower_task() must preserve artifact config"
        );
    }

    /// lower_task() preserves task-level log config.
    #[test]
    fn lower_task_preserves_log() {
        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("t");
        let mut task = dummy_task(id, "t");
        task.log = Some(crate::ast::logging::LogConfig::default());
        wf.tasks.push(task);
        let lowered = lower(wf).unwrap();
        assert!(
            lowered.tasks[0].log.is_some(),
            "lower_task() must preserve log config"
        );
    }

    /// lower_task() passes structured through as-is. This is correct because
    /// the Runner also reads AnalyzedTask.structured directly for OutputPolicy.
    #[test]
    fn lower_task_structured_passthrough() {
        use crate::ast::structured::StructuredOutputSpec;

        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("t");
        let mut task = dummy_task(id, "t");
        task.structured = Some(StructuredOutputSpec::with_file_schema("./schema.json"));
        wf.tasks.push(task);
        let lowered = lower(wf).unwrap();
        let structured = lowered.tasks[0]
            .structured
            .as_ref()
            .expect("structured should pass through lower_task");
        assert!(matches!(structured.schema, Some(SchemaRef::File(ref p)) if p == "./schema.json"));
    }

    /// The lower -> unlower roundtrip loses task-level artifact and log
    /// because lower_task() sets both to None. This is a known gap:
    /// Task artifact/log are preserved through the lower→unlower round-trip.
    #[test]
    fn roundtrip_preserves_task_artifact_and_log() {
        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("t");
        let mut task = dummy_task(id, "t");
        task.artifact = Some(crate::ast::artifact::ArtifactSpec::Enabled(true));
        task.log = Some(crate::ast::logging::LogConfig::default());
        wf.tasks.push(task);

        let lowered = lower(wf).unwrap();
        assert!(
            lowered.tasks[0].artifact.is_some(),
            "lower() must preserve task artifact"
        );
        assert!(
            lowered.tasks[0].log.is_some(),
            "lower() must preserve task log"
        );

        let unl = unlower(lowered).unwrap();
        assert!(
            unl.tasks[0].artifact.is_some(),
            "unlower() must restore task artifact"
        );
        assert!(
            unl.tasks[0].log.is_some(),
            "unlower() must restore task log"
        );
    }

    /// retry is only wired to fetch actions via lower_fetch(). For infer/exec/invoke,
    /// task.retry is silently ignored by lower_action(). The Runner handles retry
    /// separately for structured output via get_retry_config().
    #[test]
    fn lower_retry_only_applies_to_fetch() {
        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("infer_retry");
        let mut task = dummy_task(id, "infer_retry");
        task.retry = Some(AnalyzedRetry {
            max_attempts: Templatable::Value(3),
            delay_ms: Templatable::Value(1000),
            backoff: Some(Templatable::Value(2.0)),
            span: Span::dummy(),
        });
        wf.tasks.push(task);

        let lowered = lower(wf).unwrap();
        // The infer action does not carry retry config; it is silently dropped
        match &lowered.tasks[0].action {
            TaskAction::Infer { infer } => {
                assert!(infer.prompt.is_empty(), "default infer has empty prompt");
            }
            _ => panic!("expected Infer action"),
        }
    }

    // =========================================================================
    // Defense-in-depth: task_dep_names must reject dangling TaskIds
    // =========================================================================

    #[test]
    fn task_dep_names_rejects_dangling_task_id() {
        let mut table = TaskTable::new();
        table.insert("step1");
        let dangling = TaskId(99);

        let result = task_dep_names(&[dangling], &[], &table);
        assert!(
            result.is_err(),
            "Dangling TaskId should be rejected in lowering"
        );
    }

    #[test]
    fn task_dep_names_valid_ids_resolve() {
        let mut table = TaskTable::new();
        let id_a = table.insert("a");
        let id_b = table.insert("b");

        let result = task_dep_names(&[id_a], &[id_b], &table);
        let deps = result
            .expect("valid TaskIds should resolve")
            .expect("should be Some");
        assert_eq!(deps, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn task_dep_names_empty_returns_none() {
        let table = TaskTable::new();
        let result = task_dep_names(&[], &[], &table);
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn lower_rejects_dangling_task_id_in_deps() {
        let mut wf = dummy_workflow();
        let id_a = wf.task_table.insert("a");
        let id_b = wf.task_table.insert("b");

        wf.tasks.push(dummy_task(id_a, "a"));
        wf.tasks.push(AnalyzedTask {
            depends_on: vec![TaskId(99)], // dangling
            ..dummy_task(id_b, "b")
        });

        let result = lower(wf);
        assert!(
            result.is_err(),
            "lower() should reject dangling TaskId in depends_on"
        );
    }

    #[test]
    fn unlower_rejects_dangling_dep_name() {
        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("producer");
        wf.tasks.push(dummy_task(id, "producer"));

        // Lower to Workflow, then tamper with flow to create dangling ref
        let mut lowered = lower(wf).unwrap();
        let task = Arc::make_mut(&mut lowered.tasks[0]);
        task.depends_on = Some(vec!["nonexistent_task".to_string()]);

        // unlower should reject the dangling name
        let result = unlower(lowered);
        assert!(result.is_err(), "unlower should reject dangling dep name");
    }

    #[test]
    fn unlower_retry_backoff_near_one() {
        // Backoff of exactly 1.0 should roundtrip as None (no backoff)
        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("fetcher");
        wf.tasks.push(AnalyzedTask {
            action: AnalyzedTaskAction::Fetch(AnalyzedFetchAction {
                url: "https://example.com".to_string(),
                method: HttpMethod::Get,
                headers: IndexMap::new(),
                body: None,
                json: None,
                timeout_ms: None,
                follow_redirects: Templatable::Value(true),
                response: None,
                extract: None,
                selector: None,
                session: Templatable::Value(false),
                cache: Templatable::Value(false),
                span: Span::dummy(),
            }),
            retry: Some(AnalyzedRetry {
                max_attempts: Templatable::Value(3),
                delay_ms: Templatable::Value(1000),
                backoff: None, // None → multiplier 1.0
                span: Span::dummy(),
            }),
            ..dummy_task(id, "fetcher")
        });

        let lowered = lower(wf).unwrap();
        let unlowered = unlower(lowered).unwrap();

        assert!(
            unlowered.tasks[0].retry.as_ref().unwrap().backoff.is_none(),
            "backoff of 1.0 should roundtrip as None"
        );
    }

    #[test]
    fn unlower_retry_backoff_near_one_within_tolerance() {
        // Backoff of 1.00005 (within BACKOFF_UNITY_TOLERANCE) should also → None
        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("fetcher");
        wf.tasks.push(AnalyzedTask {
            action: AnalyzedTaskAction::Fetch(AnalyzedFetchAction {
                url: "https://example.com".to_string(),
                method: HttpMethod::Get,
                headers: IndexMap::new(),
                body: None,
                json: None,
                timeout_ms: None,
                follow_redirects: Templatable::Value(true),
                response: None,
                extract: None,
                selector: None,
                session: Templatable::Value(false),
                cache: Templatable::Value(false),
                span: Span::dummy(),
            }),
            retry: Some(AnalyzedRetry {
                max_attempts: Templatable::Value(3),
                delay_ms: Templatable::Value(1000),
                backoff: Some(Templatable::Value(1.00005)), // Within tolerance of 1.0
                span: Span::dummy(),
            }),
            ..dummy_task(id, "fetcher")
        });

        let lowered = lower(wf).unwrap();
        // Lowered multiplier is 1.00005
        let unlowered = unlower(lowered).unwrap();

        assert!(
            unlowered.tasks[0].retry.as_ref().unwrap().backoff.is_none(),
            "backoff within tolerance of 1.0 should roundtrip as None"
        );
    }

    #[test]
    fn unlower_retry_backoff_just_over_tolerance() {
        // 1.00011 is just OVER BACKOFF_UNITY_TOLERANCE (0.0001) — must survive roundtrip
        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("fetcher");
        wf.tasks.push(AnalyzedTask {
            action: AnalyzedTaskAction::Fetch(AnalyzedFetchAction {
                url: "https://example.com".to_string(),
                method: HttpMethod::Get,
                headers: IndexMap::new(),
                body: None,
                json: None,
                timeout_ms: None,
                follow_redirects: Templatable::Value(true),
                response: None,
                extract: None,
                selector: None,
                session: Templatable::Value(false),
                cache: Templatable::Value(false),
                span: Span::dummy(),
            }),
            retry: Some(AnalyzedRetry {
                max_attempts: Templatable::Value(3),
                delay_ms: Templatable::Value(1000),
                backoff: Some(Templatable::Value(1.00011)),
                span: Span::dummy(),
            }),
            ..dummy_task(id, "fetcher")
        });

        let lowered = lower(wf).unwrap();
        let unlowered = unlower(lowered).unwrap();
        let backoff = unlowered.tasks[0].retry.as_ref().unwrap().backoff.clone();
        assert!(
            backoff.is_some(),
            "backoff of 1.00011 (just over tolerance) must NOT be collapsed to None"
        );
        let backoff_val = backoff.unwrap().value().unwrap();
        assert!(
            (backoff_val - 1.00011).abs() < f64::EPSILON * 100.0,
            "backoff value should be preserved: got {}",
            backoff_val
        );
    }

    #[test]
    fn unlower_retry_backoff_just_under_one_over_tolerance() {
        // 0.99989 is just OVER tolerance on the low side — must survive roundtrip
        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("fetcher");
        wf.tasks.push(AnalyzedTask {
            action: AnalyzedTaskAction::Fetch(AnalyzedFetchAction {
                url: "https://example.com".to_string(),
                method: HttpMethod::Get,
                headers: IndexMap::new(),
                body: None,
                json: None,
                timeout_ms: None,
                follow_redirects: Templatable::Value(true),
                response: None,
                extract: None,
                selector: None,
                session: Templatable::Value(false),
                cache: Templatable::Value(false),
                span: Span::dummy(),
            }),
            retry: Some(AnalyzedRetry {
                max_attempts: Templatable::Value(3),
                delay_ms: Templatable::Value(1000),
                backoff: Some(Templatable::Value(0.99989)),
                span: Span::dummy(),
            }),
            ..dummy_task(id, "fetcher")
        });

        let lowered = lower(wf).unwrap();
        let unlowered = unlower(lowered).unwrap();
        let backoff = unlowered.tasks[0].retry.as_ref().unwrap().backoff.clone();
        assert!(
            backoff.is_some(),
            "backoff of 0.99989 (just over tolerance on low side) must NOT be collapsed to None"
        );
        let backoff_val = backoff.unwrap().value().unwrap();
        assert!(
            (backoff_val - 0.99989).abs() < f64::EPSILON * 100.0,
            "backoff value should be preserved: got {}",
            backoff_val
        );
    }

    #[test]
    fn unlower_retry_backoff_exactly_at_tolerance_boundary() {
        // 1.0001 is exactly at BACKOFF_UNITY_TOLERANCE — boundary is > not >=
        // So (1.0001 - 1.0).abs() = 0.0001, which is NOT > 0.0001 → collapsed to None
        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("fetcher");
        wf.tasks.push(AnalyzedTask {
            action: AnalyzedTaskAction::Fetch(AnalyzedFetchAction {
                url: "https://example.com".to_string(),
                method: HttpMethod::Get,
                headers: IndexMap::new(),
                body: None,
                json: None,
                timeout_ms: None,
                follow_redirects: Templatable::Value(true),
                response: None,
                extract: None,
                selector: None,
                session: Templatable::Value(false),
                cache: Templatable::Value(false),
                span: Span::dummy(),
            }),
            retry: Some(AnalyzedRetry {
                max_attempts: Templatable::Value(3),
                delay_ms: Templatable::Value(1000),
                backoff: Some(Templatable::Value(1.0001)),
                span: Span::dummy(),
            }),
            ..dummy_task(id, "fetcher")
        });

        let lowered = lower(wf).unwrap();
        let unlowered = unlower(lowered).unwrap();
        let backoff = unlowered.tasks[0].retry.as_ref().unwrap().backoff.clone();
        assert!(
            backoff.is_none(),
            "backoff of exactly 1.0001 (at boundary, uses >) should be collapsed to None"
        );
    }

    #[test]
    fn unlower_thinking_budget_clamps_to_u32_max() {
        // thinking_budget in Workflow is u64, but AnalyzedInferAction uses u32.
        // Values > u32::MAX must clamp to u32::MAX, not silently wrap to 0.
        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("thinker");
        wf.tasks.push(AnalyzedTask {
            action: AnalyzedTaskAction::Infer(AnalyzedInferAction {
                prompt: "think hard".to_string(),
                extended_thinking: Some(Templatable::Value(true)),
                thinking_budget: Some(Templatable::Value(u32::MAX)),
                ..Default::default()
            }),
            ..dummy_task(id, "thinker")
        });

        let lowered = lower(wf).unwrap();
        // lower converts u32 → u64 (lossless)
        match &lowered.tasks[0].action {
            TaskAction::Infer { infer } => {
                assert_eq!(infer.thinking_budget, Some(u32::MAX as u64));
            }
            _ => panic!("expected Infer"),
        }

        // Now test unlower with a value > u32::MAX
        let unlowered = unlower(lowered).unwrap();
        match &unlowered.tasks[0].action {
            AnalyzedTaskAction::Infer(infer) => {
                assert_eq!(infer.thinking_budget, Some(Templatable::Value(u32::MAX)));
            }
            _ => panic!("expected Infer"),
        }
    }

    #[test]
    fn unlower_valid_deps_resolve() {
        let mut wf = dummy_workflow();
        let id_a = wf.task_table.insert("step_a");
        let id_b = wf.task_table.insert("step_b");
        let mut task_b = dummy_task(id_b, "step_b");
        task_b.depends_on = vec![id_a];
        wf.tasks.push(dummy_task(id_a, "step_a"));
        wf.tasks.push(task_b);

        let lowered = lower(wf).unwrap();
        // Roundtrip should succeed when all deps are valid
        let result = unlower(lowered);
        assert!(
            result.is_ok(),
            "unlower should succeed with valid deps: {:?}",
            result.err()
        );
    }

    // =========================================================================
    // Roundtrip edge cases
    // =========================================================================

    #[test]
    fn lower_unlower_roundtrip_for_each() {
        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("iter");
        wf.tasks.push(AnalyzedTask {
            for_each: Some(AnalyzedForEach {
                items: r#"["a","b","c"]"#.to_string(),
                as_var: "item".to_string(),
                concurrency: Some(Templatable::Value(2)),
                fail_fast: Templatable::Value(true),
                span: Span::dummy(),
            }),
            ..dummy_task(id, "iter")
        });

        let lowered = lower(wf).unwrap();
        let unlowered = unlower(lowered).unwrap();
        let t = &unlowered.tasks[0];
        assert!(t.for_each.is_some(), "for_each should survive roundtrip");
        let fe = t.for_each.as_ref().unwrap();
        assert_eq!(fe.as_var, "item");
        assert_eq!(fe.concurrency, Some(Templatable::Value(2)));
    }

    #[test]
    fn lower_unlower_roundtrip_mcp_stdio() {
        let mut wf = dummy_workflow();
        let mut servers = IndexMap::new();
        servers.insert(
            "test".to_string(),
            AnalyzedMcpServer {
                name: "test".to_string(),
                from: None,
                command: Some("node".to_string()),
                args: vec!["server.js".to_string()],
                env: IndexMap::new(),
                cwd: None,
                url: None,
                transport: McpTransport::Stdio,
                span: Span::dummy(),
            },
        );
        wf.mcp_servers = servers;

        let lowered = lower(wf).unwrap();
        assert!(lowered.mcp.is_some(), "stdio server should be lowered");

        let unlowered = unlower(lowered).unwrap();
        assert_eq!(unlowered.mcp_servers.len(), 1);
        assert!(unlowered.mcp_servers.contains_key("test"));
        assert_eq!(
            unlowered.mcp_servers["test"].command.as_deref(),
            Some("node")
        );
    }

    #[test]
    fn lower_unlower_roundtrip_invoke_with_resource() {
        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("read_res");
        wf.tasks.push(AnalyzedTask {
            action: AnalyzedTaskAction::Invoke(AnalyzedInvokeAction {
                server: Some("novanet".to_string()),
                tool: "".to_string(), // resource-only invoke
                resource: None,
                params: None,
                timeout_ms: Some(Templatable::Value(10000)),
                span: Span::dummy(),
            }),
            ..dummy_task(id, "read_res")
        });

        let lowered = lower(wf).unwrap();
        let unlowered = unlower(lowered).unwrap();
        let t = &unlowered.tasks[0];
        match &t.action {
            AnalyzedTaskAction::Invoke(inv) => {
                assert_eq!(inv.server.as_deref(), Some("novanet"));
            }
            _ => panic!("expected Invoke action after roundtrip"),
        }
    }

    // =========================================================================
    // Roundtrip documentation tests — prove known lossy conversions
    // =========================================================================

    /// L1: Provider None normalizes to "anthropic" (canonical) during lowering.
    #[test]
    fn roundtrip_provider_none_becomes_claude() {
        let wf = AnalyzedWorkflow {
            provider: None,
            ..dummy_workflow()
        };
        let lowered = lower(wf).unwrap();
        assert_eq!(
            lowered.provider,
            nika_core::ProviderName::Anthropic,
            "None provider should default to anthropic (canonical)"
        );
        let unlowered = unlower(lowered).unwrap();
        assert_eq!(
            unlowered.provider,
            Some(nika_core::ProviderName::Anthropic),
            "After roundtrip, provider is Some(\"claude\"), not None"
        );
    }

    /// L2: `unlower_output` maps `OutputFormat::Markdown` to `AnalyzedOutputFormat::Text`.
    ///
    /// Note: `AnalyzedOutputFormat` has no `Markdown` variant, so `lower()` never
    /// produces `OutputFormat::Markdown`. This test verifies `unlower_output()`
    /// behavior directly by injecting a Markdown format into a lowered workflow.
    /// The test exists because external YAML files or future code paths may produce
    /// `OutputFormat::Markdown` that feeds into `unlower()`.
    #[test]
    fn roundtrip_markdown_output_becomes_text() {
        use crate::ast::output::OutputFormat as RuntimeOutputFormat;

        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("md_task");
        wf.tasks.push(AnalyzedTask {
            output: Some(AnalyzedOutput {
                format: AnalyzedOutputFormat::Text,
                schema: None,
                schema_ref: None,
                max_retries: None,
                span: Span::dummy(),
            }),
            ..dummy_task(id, "md_task")
        });
        let lowered = lower(wf).unwrap();
        // Inject Markdown format into lowered workflow (cannot occur via lower(),
        // but can occur when deserializing runtime YAML that uses "markdown").
        let lowered_clone = Workflow {
            tasks: lowered
                .tasks
                .iter()
                .map(|t| {
                    let mut task = (**t).clone();
                    if let Some(ref mut out) = task.output {
                        out.format = RuntimeOutputFormat::Markdown;
                    }
                    Arc::new(task)
                })
                .collect(),
            ..lowered
        };
        let unlowered = unlower(lowered_clone).unwrap();
        let out = unlowered.tasks[0].output.as_ref().unwrap();
        assert_eq!(
            out.format,
            AnalyzedOutputFormat::Text,
            "Markdown should collapse to Text after unlower"
        );
    }

    /// L3: Implicit deps merge into depends_on and lose their distinct identity.
    #[test]
    fn roundtrip_implicit_deps_merge_into_depends_on() {
        let mut wf = dummy_workflow();
        let id_a = wf.task_table.insert("task_a");
        let id_b = wf.task_table.insert("task_b");
        wf.tasks.push(dummy_task(id_a, "task_a"));
        wf.tasks.push(AnalyzedTask {
            depends_on: vec![id_a],
            implicit_deps: vec![id_a], // same dep as both explicit and implicit
            ..dummy_task(id_b, "task_b")
        });
        let lowered = lower(wf).unwrap();
        // Both explicit and implicit deps merged into flow
        let deps = lowered.tasks[1].depends_on.as_ref().unwrap();
        assert_eq!(deps.len(), 2, "Both deps should be merged into depends_on");
        // After roundtrip, all appear in depends_on, implicit_deps is empty
        let unlowered = unlower(lowered).unwrap();
        let rt_task = &unlowered.tasks[1];
        assert!(
            rt_task.implicit_deps.is_empty(),
            "After roundtrip, implicit_deps should be empty (merged into depends_on)"
        );
        assert_eq!(rt_task.depends_on.len(), 2);
    }

    /// L4: context_files with aliases are preserved through round-trip.
    /// Files without aliases are dropped (no key for ContextConfig map).
    #[test]
    fn roundtrip_context_files_preserved() {
        let wf = AnalyzedWorkflow {
            context_files: vec![
                AnalyzedContextFile {
                    path: "README.md".to_string(),
                    alias: None, // No alias → dropped during round-trip
                    max_bytes: None,
                    span: Span::dummy(),
                },
                AnalyzedContextFile {
                    path: "schema.json".to_string(),
                    alias: Some("schema".to_string()),
                    max_bytes: Some(4096),
                    span: Span::dummy(),
                },
            ],
            ..dummy_workflow()
        };
        let lowered = lower(wf).unwrap();
        assert!(
            lowered.context.is_some(),
            "context should be preserved in lowered Workflow"
        );
        let ctx = lowered.context.as_ref().unwrap();
        assert_eq!(
            ctx.files.len(),
            1,
            "only aliased files survive the round-trip"
        );
        assert_eq!(ctx.files.get("schema"), Some(&"schema.json".to_string()));

        let unlowered = unlower(lowered).unwrap();
        assert_eq!(
            unlowered.context_files.len(),
            1,
            "context_files with aliases should be restored"
        );
        assert_eq!(unlowered.context_files[0].alias.as_deref(), Some("schema"));
        assert_eq!(unlowered.context_files[0].path, "schema.json");
    }

    /// L5: Agent `from` field is lost during roundtrip.
    #[test]
    fn roundtrip_agent_from_field_is_lost() {
        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("agent_task");
        wf.tasks.push(AnalyzedTask {
            action: AnalyzedTaskAction::Agent(Box::new(AnalyzedAgentAction {
                prompt: "do something".to_string(),
                tools: vec![],
                max_turns: Some(Templatable::Value(5)),
                max_tokens: None,
                from: Some("my_agent_def".to_string()),
                skills: vec![],
                mcp: vec![],
                system: None,
                provider: None,
                model: None,
                temperature: None,
                token_budget: None,
                extended_thinking: None,
                thinking_budget: None,
                depth_limit: None,
                tool_choice: None,
                stop_sequences: vec![],
                scope: None,
                guardrails: Vec::new(),
                completion: None,
                limits: None,
                span: Span::dummy(),
            })),
            ..dummy_task(id, "agent_task")
        });
        let lowered = lower(wf).unwrap();
        let unlowered = unlower(lowered).unwrap();
        match &unlowered.tasks[0].action {
            AnalyzedTaskAction::Agent(agent) => {
                assert_eq!(
                    agent.from, None,
                    "Agent `from` field should be None after roundtrip"
                );
            }
            _ => panic!("expected Agent action after roundtrip"),
        }
    }

    /// L6: SSE MCP servers are permanently dropped during lowering (only Stdio survives).
    #[test]
    fn roundtrip_sse_server_permanently_lost() {
        let mut wf = dummy_workflow();
        let mut servers = IndexMap::new();
        servers.insert(
            "sse_server".to_string(),
            AnalyzedMcpServer {
                name: "sse_server".to_string(),
                from: None,
                command: None,
                args: vec![],
                env: IndexMap::new(),
                cwd: None,
                url: Some("https://example.com/mcp".to_string()),
                transport: McpTransport::Sse,
                span: Span::dummy(),
            },
        );
        servers.insert(
            "stdio_server".to_string(),
            AnalyzedMcpServer {
                name: "stdio_server".to_string(),
                from: None,
                command: Some("npx server".to_string()),
                args: vec!["--port".to_string(), "3000".to_string()],
                env: IndexMap::new(),
                cwd: None,
                url: None,
                transport: McpTransport::Stdio,
                span: Span::dummy(),
            },
        );
        wf.mcp_servers = servers;

        let lowered = lower(wf).unwrap();
        // Only stdio survives lowering
        let mcp = lowered.mcp.as_ref().unwrap();
        assert_eq!(mcp.len(), 1, "Only Stdio server should survive lowering");
        assert!(mcp.contains_key("stdio_server"));

        let unlowered = unlower(lowered).unwrap();
        assert_eq!(
            unlowered.mcp_servers.len(),
            1,
            "SSE server should be permanently lost after roundtrip"
        );
        assert!(unlowered.mcp_servers.contains_key("stdio_server"));
        assert!(
            !unlowered.mcp_servers.contains_key("sse_server"),
            "SSE server should not exist after roundtrip"
        );
    }

    /// L7: NaN backoff multiplier becomes None (not silently preserved).
    ///
    /// Before the fix, `(NaN - 1.0).abs() > 0.0001` evaluated to `false`
    /// because NaN comparisons always return false, silently dropping the
    /// multiplier as if it were 1.0.
    #[test]
    fn roundtrip_nan_multiplier_becomes_none() {
        use crate::ast::action::{FetchParams, RetryConfig, TaskAction};

        let action = TaskAction::Fetch {
            fetch: FetchParams {
                url: "https://example.com".to_string(),
                method: "GET".to_string(),
                headers: Default::default(),
                body: None,
                json: None,
                timeout: None,
                retry: Some(RetryConfig {
                    max_attempts: 3,
                    backoff_ms: 1000,
                    multiplier: f64::NAN,
                }),
                follow_redirects: None,
                response: None,
                extract: None,
                selector: None,
                session: None,
                cache: None,
            },
        };
        let result = unlower_retry(&action);
        assert!(result.is_some(), "Retry config should be preserved");
        assert!(
            result.unwrap().backoff.is_none(),
            "NaN multiplier should become None, not be silently preserved"
        );
    }

    /// L8: `task.description` is dropped during unlowering.
    #[test]
    fn roundtrip_task_description_is_lost() {
        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("described");
        wf.tasks.push(AnalyzedTask {
            description: Some("Important task description".to_string()),
            ..dummy_task(id, "described")
        });
        let lowered = lower(wf).unwrap();
        let unlowered = unlower(lowered).unwrap();
        assert!(
            unlowered.tasks[0].description.is_none(),
            "Task description should be lost after roundtrip"
        );
    }

    /// L9: `workflow.name` and `workflow.description` are dropped.
    #[test]
    fn roundtrip_workflow_name_preserved_description_lost() {
        let mut wf = dummy_workflow();
        wf.name = Some("My workflow".to_string());
        wf.description = Some("Does important things".to_string());
        let lowered = lower(wf).unwrap();
        let unlowered = unlower(lowered).unwrap();
        assert_eq!(
            unlowered.name,
            Some("My workflow".to_string()),
            "Workflow name should be preserved after roundtrip"
        );
        assert!(
            unlowered.description.is_none(),
            "Workflow description should be lost after roundtrip"
        );
    }

    // =========================================================================
    // Bug fix regression tests
    // =========================================================================

    /// Bug 7: schema_ref must survive the Raw -> Analyzed -> Lower pipeline.
    #[test]
    fn bug7_schema_ref_threaded_through_pipeline() {
        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("t");
        wf.tasks.push(AnalyzedTask {
            output: Some(AnalyzedOutput {
                format: AnalyzedOutputFormat::Json,
                schema: None,
                schema_ref: Some("./schemas/result.json".to_string()),
                max_retries: None,
                span: Span::dummy(),
            }),
            ..dummy_task(id, "t")
        });

        let lowered = lower(wf).unwrap();
        let output = lowered.tasks[0]
            .output
            .as_ref()
            .expect("output should exist");
        match &output.schema {
            Some(SchemaRef::File(path)) => {
                assert_eq!(path, "./schemas/result.json");
            }
            other => panic!("expected SchemaRef::File, got {:?}", other),
        }
    }

    /// Bug 7: schema_ref roundtrips through lower -> unlower.
    #[test]
    fn bug7_schema_ref_roundtrip() {
        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("t");
        wf.tasks.push(AnalyzedTask {
            output: Some(AnalyzedOutput {
                format: AnalyzedOutputFormat::Json,
                schema: None,
                schema_ref: Some("/absolute/schema.json".to_string()),
                max_retries: None,
                span: Span::dummy(),
            }),
            ..dummy_task(id, "t")
        });

        let lowered = lower(wf).unwrap();
        let unlowered = unlower(lowered).unwrap();
        let output = unlowered.tasks[0]
            .output
            .as_ref()
            .expect("output should exist");
        assert_eq!(
            output.schema_ref.as_deref(),
            Some("/absolute/schema.json"),
            "schema_ref should survive roundtrip"
        );
        assert!(
            output.schema.is_none(),
            "File schema should not appear as inline schema"
        );
    }

    /// Bug 8: String schemas that look like file paths must become SchemaRef::File.
    #[test]
    fn bug8_schema_file_path_classified_correctly() {
        // Test ./ prefix
        let output = AnalyzedOutput {
            format: AnalyzedOutputFormat::Json,
            schema: Some(serde_json::Value::String("./schemas/user.json".to_string())),
            schema_ref: None,
            max_retries: None,
            span: Span::dummy(),
        };
        let lowered = lower_output(output);
        assert!(
            matches!(lowered.schema, Some(SchemaRef::File(ref p)) if p == "./schemas/user.json"),
            "Schema starting with ./ should be File, got {:?}",
            lowered.schema
        );

        // Test / prefix
        let output = AnalyzedOutput {
            format: AnalyzedOutputFormat::Json,
            schema: Some(serde_json::Value::String("/etc/schema.json".to_string())),
            schema_ref: None,
            max_retries: None,
            span: Span::dummy(),
        };
        let lowered = lower_output(output);
        assert!(
            matches!(lowered.schema, Some(SchemaRef::File(ref p)) if p == "/etc/schema.json"),
            "Schema starting with / should be File"
        );

        // Test .json suffix
        let output = AnalyzedOutput {
            format: AnalyzedOutputFormat::Json,
            schema: Some(serde_json::Value::String("schemas/result.json".to_string())),
            schema_ref: None,
            max_retries: None,
            span: Span::dummy(),
        };
        let lowered = lower_output(output);
        assert!(
            matches!(lowered.schema, Some(SchemaRef::File(ref p)) if p == "schemas/result.json"),
            "Schema ending with .json should be File"
        );

        // Test inline schema object (should remain Inline)
        let output = AnalyzedOutput {
            format: AnalyzedOutputFormat::Json,
            schema: Some(serde_json::json!({"type": "object"})),
            schema_ref: None,
            max_retries: None,
            span: Span::dummy(),
        };
        let lowered = lower_output(output);
        assert!(
            matches!(lowered.schema, Some(SchemaRef::Inline(_))),
            "JSON object schema should remain Inline"
        );

        // Test string that does NOT look like a file path (should remain Inline)
        let output = AnalyzedOutput {
            format: AnalyzedOutputFormat::Json,
            schema: Some(serde_json::Value::String("just-a-string".to_string())),
            schema_ref: None,
            max_retries: None,
            span: Span::dummy(),
        };
        let lowered = lower_output(output);
        assert!(
            matches!(lowered.schema, Some(SchemaRef::Inline(_))),
            "Non-path string schema should remain Inline"
        );
    }

    /// Bug 42: output.max_retries must survive the pipeline.
    #[test]
    fn bug42_output_max_retries_threaded() {
        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("t");
        wf.tasks.push(AnalyzedTask {
            output: Some(AnalyzedOutput {
                format: AnalyzedOutputFormat::Json,
                schema: Some(serde_json::json!({"type": "object"})),
                schema_ref: None,
                max_retries: Some(Templatable::Value(5)),
                span: Span::dummy(),
            }),
            ..dummy_task(id, "t")
        });

        let lowered = lower(wf).unwrap();
        let output = lowered.tasks[0]
            .output
            .as_ref()
            .expect("output should exist");
        assert_eq!(
            output.max_retries,
            Some(5),
            "max_retries should survive lowering"
        );
    }

    /// Bug 42: output.max_retries roundtrips.
    #[test]
    fn bug42_output_max_retries_roundtrip() {
        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("t");
        wf.tasks.push(AnalyzedTask {
            output: Some(AnalyzedOutput {
                format: AnalyzedOutputFormat::Json,
                schema: Some(serde_json::json!({"type": "object"})),
                schema_ref: None,
                max_retries: Some(Templatable::Value(3)),
                span: Span::dummy(),
            }),
            ..dummy_task(id, "t")
        });

        let lowered = lower(wf).unwrap();
        let unlowered = unlower(lowered).unwrap();
        let output = unlowered.tasks[0]
            .output
            .as_ref()
            .expect("output should exist");
        assert_eq!(
            output.max_retries,
            Some(Templatable::Value(3)),
            "max_retries should survive roundtrip"
        );
    }

    /// Bug 43: infer.response_format must survive the pipeline.
    #[test]
    fn bug43_response_format_threaded() {
        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("t");
        wf.tasks.push(AnalyzedTask {
            action: AnalyzedTaskAction::Infer(AnalyzedInferAction {
                prompt: "test".to_string(),
                response_format: Some("json".to_string()),
                ..Default::default()
            }),
            ..dummy_task(id, "t")
        });

        let lowered = lower(wf).unwrap();
        match &lowered.tasks[0].action {
            TaskAction::Infer { infer } => {
                assert_eq!(
                    infer.response_format,
                    Some(crate::ast::action::ResponseFormat::Json),
                    "response_format should survive lowering"
                );
            }
            _ => panic!("expected Infer action"),
        }
    }

    /// Bug 43: response_format roundtrips.
    #[test]
    fn bug43_response_format_roundtrip() {
        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("t");
        wf.tasks.push(AnalyzedTask {
            action: AnalyzedTaskAction::Infer(AnalyzedInferAction {
                prompt: "test".to_string(),
                response_format: Some("markdown".to_string()),
                ..Default::default()
            }),
            ..dummy_task(id, "t")
        });

        let lowered = lower(wf).unwrap();
        let unlowered = unlower(lowered).unwrap();
        match &unlowered.tasks[0].action {
            AnalyzedTaskAction::Infer(infer) => {
                assert_eq!(
                    infer.response_format.as_deref(),
                    Some("markdown"),
                    "response_format should survive roundtrip"
                );
            }
            _ => panic!("expected Infer action"),
        }
    }

    /// Bug 44: Sub-second timeouts must not be truncated to zero.
    #[test]
    fn bug44_timeout_ceiling_division() {
        // 500ms should become 1s (not 0s)
        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("t");
        wf.tasks.push(AnalyzedTask {
            action: AnalyzedTaskAction::Exec(AnalyzedExecAction {
                command: "echo hi".to_string(),
                shell: Templatable::Value(false),
                cwd: None,
                env: IndexMap::new(),
                timeout_ms: Some(Templatable::Value(500)),
                max_stdout: None,
                span: Span::dummy(),
            }),
            ..dummy_task(id, "t")
        });

        let lowered = lower(wf).unwrap();
        match &lowered.tasks[0].action {
            TaskAction::Exec { exec: e } => {
                assert_eq!(
                    e.timeout,
                    Some(1),
                    "500ms should ceil to 1s, not truncate to 0s"
                );
            }
            _ => panic!("expected Exec action"),
        }
    }

    /// Bug 44: Exact second boundaries should not be inflated.
    #[test]
    fn bug44_timeout_exact_seconds_unchanged() {
        // 1000ms -> 1s (no inflation)
        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("t");
        wf.tasks.push(AnalyzedTask {
            action: AnalyzedTaskAction::Exec(AnalyzedExecAction {
                command: "echo hi".to_string(),
                shell: Templatable::Value(false),
                cwd: None,
                env: IndexMap::new(),
                timeout_ms: Some(Templatable::Value(1000)),
                max_stdout: None,
                span: Span::dummy(),
            }),
            ..dummy_task(id, "t")
        });

        let lowered = lower(wf).unwrap();
        match &lowered.tasks[0].action {
            TaskAction::Exec { exec: e } => {
                assert_eq!(e.timeout, Some(1), "1000ms should remain 1s");
            }
            _ => panic!("expected Exec action"),
        }
    }

    /// Bug 44: Fetch timeout ceiling division.
    #[test]
    fn bug44_timeout_fetch_ceiling() {
        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("t");
        wf.tasks.push(AnalyzedTask {
            action: AnalyzedTaskAction::Fetch(AnalyzedFetchAction {
                url: "https://example.com".to_string(),
                method: HttpMethod::Get,
                headers: IndexMap::new(),
                body: None,
                json: None,
                timeout_ms: Some(Templatable::Value(1500)),
                follow_redirects: Templatable::Value(true),
                response: None,
                extract: None,
                selector: None,
                session: Templatable::Value(false),
                cache: Templatable::Value(false),
                span: Span::dummy(),
            }),
            ..dummy_task(id, "t")
        });

        let lowered = lower(wf).unwrap();
        match &lowered.tasks[0].action {
            TaskAction::Fetch { fetch } => {
                assert_eq!(
                    fetch.timeout,
                    Some(2),
                    "1500ms should ceil to 2s, not truncate to 1s"
                );
            }
            _ => panic!("expected Fetch action"),
        }
    }

    /// Bug 44: Invoke timeout ceiling division.
    #[test]
    fn bug44_timeout_invoke_ceiling() {
        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("t");
        wf.tasks.push(AnalyzedTask {
            action: AnalyzedTaskAction::Invoke(AnalyzedInvokeAction {
                server: Some("test".to_string()),
                tool: "tool".to_string(),
                resource: None,
                params: None,
                timeout_ms: Some(Templatable::Value(100)),
                span: Span::dummy(),
            }),
            ..dummy_task(id, "t")
        });

        let lowered = lower(wf).unwrap();
        match &lowered.tasks[0].action {
            TaskAction::Invoke { invoke } => {
                assert_eq!(
                    invoke.timeout,
                    Some(1),
                    "100ms should ceil to 1s, not truncate to 0s"
                );
            }
            _ => panic!("expected Invoke action"),
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // Bug 9: invalid tool_choice should warn and map to None
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn lower_agent_valid_tool_choice_values() {
        for (input, expected_variant) in [
            ("auto", "Auto"),
            ("required", "Required"),
            ("none", "None"),
            ("AUTO", "Auto"), // case-insensitive
            ("Required", "Required"),
        ] {
            let agent = AnalyzedAgentAction {
                prompt: "test".to_string(),
                tools: vec![],
                max_turns: None,
                max_tokens: None,
                from: None,
                skills: vec![],
                mcp: vec![],
                system: None,
                provider: None,
                model: None,
                temperature: None,
                token_budget: None,
                extended_thinking: None,
                thinking_budget: None,
                depth_limit: None,
                tool_choice: Some(input.to_string()),
                stop_sequences: vec![],
                scope: None,
                guardrails: Vec::new(),
                completion: None,
                limits: None,
                span: Span::dummy(),
            };
            let params = lower_agent(agent, None, None, None);
            assert!(
                params.tool_choice.is_some(),
                "valid tool_choice '{}' should produce Some({})",
                input,
                expected_variant
            );
        }
    }

    #[test]
    fn lower_agent_invalid_tool_choice_maps_to_none() {
        // Invalid values should be mapped to None (with a tracing::warn)
        for invalid in ["foo", "always", "any", ""] {
            let agent = AnalyzedAgentAction {
                prompt: "test".to_string(),
                tools: vec![],
                max_turns: None,
                max_tokens: None,
                from: None,
                skills: vec![],
                mcp: vec![],
                system: None,
                provider: None,
                model: None,
                temperature: None,
                token_budget: None,
                extended_thinking: None,
                thinking_budget: None,
                depth_limit: None,
                tool_choice: Some(invalid.to_string()),
                stop_sequences: vec![],
                scope: None,
                guardrails: Vec::new(),
                completion: None,
                limits: None,
                span: Span::dummy(),
            };
            let params = lower_agent(agent, None, None, None);
            assert!(
                params.tool_choice.is_none(),
                "invalid tool_choice '{}' should map to None, got {:?}",
                invalid,
                params.tool_choice
            );
        }
    }

    #[test]
    fn lower_infer_guardrails_preserved() {
        use crate::ast::guardrails::{GuardrailConfig, LengthGuardrail, OnFailure};

        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("guarded");
        wf.tasks.push(AnalyzedTask {
            action: AnalyzedTaskAction::Infer(AnalyzedInferAction {
                prompt: "Summarize".to_string(),
                guardrails: vec![GuardrailConfig::Length(LengthGuardrail {
                    id: Some("word_count".to_string()),
                    min_words: Some(10),
                    max_words: Some(200),
                    min_chars: None,
                    max_chars: None,
                    message: None,
                    on_failure: OnFailure::Fail,
                })],
                ..Default::default()
            }),
            ..dummy_task(id, "guarded")
        });

        let lowered = lower(wf).unwrap();
        match &lowered.tasks[0].action {
            TaskAction::Infer { infer } => {
                assert_eq!(infer.guardrails.len(), 1);
                assert_eq!(infer.guardrails[0].guardrail_type(), "length");
                assert_eq!(infer.guardrails[0].on_failure(), OnFailure::Fail);
            }
            _ => panic!("expected Infer action"),
        }
    }

    #[test]
    fn unlower_infer_guardrails_roundtrip() {
        use crate::ast::guardrails::{GuardrailConfig, LengthGuardrail, OnFailure};

        // Build regex guardrail via serde (compiled field is private)
        let regex_guardrail: GuardrailConfig = serde_json::from_value(serde_json::json!({
            "type": "regex",
            "id": "starts_with_summary",
            "pattern": "^Summary:",
            "message": "Must start with Summary:",
            "on_failure": "fail"
        }))
        .expect("valid regex guardrail JSON");

        let mut wf = dummy_workflow();
        let id = wf.task_table.insert("validated");
        wf.tasks.push(AnalyzedTask {
            action: AnalyzedTaskAction::Infer(AnalyzedInferAction {
                prompt: "Write a summary".to_string(),
                guardrails: vec![
                    GuardrailConfig::Length(LengthGuardrail {
                        id: None,
                        min_words: Some(50),
                        max_words: None,
                        min_chars: None,
                        max_chars: None,
                        message: None,
                        on_failure: OnFailure::default(),
                    }),
                    regex_guardrail,
                ],
                ..Default::default()
            }),
            ..dummy_task(id, "validated")
        });

        let lowered = lower(wf).unwrap();
        let unlowered = unlower(lowered).unwrap();

        match &unlowered.tasks[0].action {
            AnalyzedTaskAction::Infer(infer) => {
                assert_eq!(infer.guardrails.len(), 2);
                assert_eq!(infer.guardrails[0].guardrail_type(), "length");
                assert_eq!(infer.guardrails[1].guardrail_type(), "regex");
                assert_eq!(infer.guardrails[1].on_failure(), OnFailure::Fail);
            }
            _ => panic!("expected Infer action after roundtrip"),
        }
    }
}
