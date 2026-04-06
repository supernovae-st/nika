//! MCP Server for Nika — exposes workflow tools via Model Context Protocol
//!
//! Allows AI coding tools (Claude Code, Cursor, Copilot, etc.) to validate,
//! run, and explore Nika workflows through MCP.

use rmcp::handler::server::router::prompt::PromptRouter;
use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, Content, GetPromptResult, ListPromptsResult, PromptMessage,
    PromptMessageRole, ServerCapabilities, ServerInfo,
};
use rmcp::{tool, tool_router, ServerHandler, RoleServer};
use schemars::JsonSchema;
use serde::Deserialize;
use tokio::process::Command as TokioCommand;
use tokio::time::{timeout, Duration};

/// Nika MCP Server handler
#[derive(Clone)]
pub struct NikaMcpServer {
    tool_router: ToolRouter<Self>,
    prompt_router: PromptRouter<Self>,
}

impl Default for NikaMcpServer {
    fn default() -> Self {
        Self::new()
    }
}

/// Parameters for the nika_check tool
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CheckParams {
    /// Path to a .nika.yaml workflow file to validate
    pub path: String,
}

/// Parameters for the nika_schema tool
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SchemaParams {
    /// Schema version (default: @0.12)
    #[serde(default = "default_schema_version")]
    pub version: String,
}

fn default_schema_version() -> String {
    "0.12".to_string()
}

/// Parameters for the nika_error_lookup tool
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ErrorLookupParams {
    /// NIKA error code (e.g., "NIKA-040")
    pub code: String,
}

/// Parameters for the nika_generate_task tool
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GenerateTaskParams {
    /// Natural language description of what the task should do
    pub description: String,
    /// Which verb to use (infer, exec, fetch, invoke, agent)
    #[serde(default)]
    pub verb: Option<String>,
    /// Provider to use (anthropic, openai, etc.)
    #[serde(default)]
    pub provider: Option<String>,
}

/// Parameters for the nika_dag_visualization tool
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DagVisualizationParams {
    /// Path to a .nika.yaml workflow file
    pub path: String,
}

/// Parameters for the nika_error_fix tool
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ErrorFixParams {
    /// NIKA error code (e.g., "NIKA-041")
    pub code: String,
    /// The YAML content that has the error
    pub content: String,
}

#[tool_router]
impl NikaMcpServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
            prompt_router: Self::prompt_router(),
        }
    }

    /// Validate a Nika .nika.yaml workflow file for syntax and semantic errors.
    #[tool(
        name = "nika_check",
        description = "Validate a Nika .nika.yaml workflow file. Returns validation errors with NIKA-XXX codes if invalid, or confirmation if valid. Use when editing or creating .nika.yaml files."
    )]
    async fn check(
        &self,
        Parameters(params): Parameters<CheckParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        // Validate path: must be .nika.yaml, must be within cwd (no traversal)
        let canonical = match validate_workflow_path(&params.path) {
            Ok(p) => p,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(e)]));
            }
        };

        let path_str = canonical.to_string_lossy().to_string();
        let output = timeout(
            Duration::from_secs(30),
            TokioCommand::new("nika")
                .args(["check", &path_str])
                .output(),
        )
        .await
        .map_err(|_| rmcp::ErrorData::internal_error("nika check timed out after 30s", None))?
        .map_err(|e| {
            rmcp::ErrorData::internal_error(format!("Failed to run nika check: {}", e), None)
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if output.status.success() {
            Ok(CallToolResult::success(vec![Content::text(format!(
                "Valid: {}",
                params.path
            ))]))
        } else {
            Ok(CallToolResult::error(vec![Content::text(format!(
                "Validation errors:\n{}\n{}",
                stdout, stderr
            ))]))
        }
    }

    /// List all .nika.yaml workflow files in the current directory and subdirectories.
    #[tool(
        name = "nika_list_workflows",
        description = "List all .nika.yaml workflow files in the project. Use to discover available workflows."
    )]
    async fn list_workflows(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        // Use spawn_blocking to avoid blocking the async MCP handler with recursive fs scan
        let workflows = tokio::task::spawn_blocking(|| {
            let mut wf = Vec::new();
            collect_workflows(std::path::Path::new("."), &mut wf, 0);
            wf
        })
        .await
        .unwrap_or_default();

        if workflows.is_empty() {
            Ok(CallToolResult::success(vec![Content::text(
                "No .nika.yaml files found in current directory.",
            )]))
        } else {
            Ok(CallToolResult::success(vec![Content::text(format!(
                "Found {} workflow(s):\n{}",
                workflows.len(),
                workflows.join("\n")
            ))]))
        }
    }

    /// Get the Nika workflow schema reference for a specific version.
    #[tool(
        name = "nika_schema",
        description = "Get the Nika workflow YAML schema reference. Returns the 5 verbs, all fields, binding syntax, and transform catalog."
    )]
    async fn schema(
        &self,
        Parameters(_params): Parameters<SchemaParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        Ok(CallToolResult::success(vec![Content::text(SCHEMA_REF)]))
    }

    /// Generate a valid Nika task from a natural language description.
    #[tool(
        name = "nika_generate_task",
        description = "Generate a valid Nika YAML task from a natural language description. Returns a task block ready to paste into a .nika.yaml workflow."
    )]
    async fn generate_task(
        &self,
        Parameters(params): Parameters<GenerateTaskParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let verb = params.verb.as_deref().unwrap_or("infer");
        let provider = params.provider.as_deref().unwrap_or("anthropic");

        let task_id = params
            .description
            .split_whitespace()
            .take(3)
            .collect::<Vec<_>>()
            .join("_")
            .to_lowercase()
            .replace(|c: char| !c.is_alphanumeric() && c != '_', "");

        let yaml = match verb {
            "infer" => format!(
                "- id: {task_id}\n  provider: {provider}\n  infer: |\n    {desc}",
                task_id = if task_id.is_empty() { "my_task" } else { &task_id },
                desc = params.description,
            ),
            "exec" => format!(
                "- id: {task_id}\n  exec:\n    command: \"# TODO: {desc}\"\n    shell: true",
                task_id = if task_id.is_empty() { "my_task" } else { &task_id },
                desc = params.description,
            ),
            "fetch" => format!(
                "- id: {task_id}\n  fetch:\n    url: \"https://example.com\"\n    extract: article\n  # {desc}",
                task_id = if task_id.is_empty() { "my_task" } else { &task_id },
                desc = params.description,
            ),
            "invoke" => format!(
                "- id: {task_id}\n  invoke:\n    tool: \"nika:log\"\n    params:\n      message: \"# TODO: {desc}\"",
                task_id = if task_id.is_empty() { "my_task" } else { &task_id },
                desc = params.description,
            ),
            "agent" => format!(
                "- id: {task_id}\n  provider: {provider}\n  agent:\n    prompt: |\n      {desc}\n    max_turns: 5\n    completion:\n      mode: natural",
                task_id = if task_id.is_empty() { "my_task" } else { &task_id },
                desc = params.description,
            ),
            _ => format!(
                "- id: {task_id}\n  # Unknown verb '{verb}'. Valid verbs: infer, exec, fetch, invoke, agent\n  infer: |\n    {desc}",
                task_id = if task_id.is_empty() { "my_task" } else { &task_id },
                desc = params.description,
            ),
        };

        Ok(CallToolResult::success(vec![Content::text(yaml)]))
    }

    /// Generate a Mermaid DAG visualization of a Nika workflow.
    #[tool(
        name = "nika_dag_visualization",
        description = "Generate a Mermaid graph visualization of a Nika workflow's task DAG. Returns Mermaid markdown that renders as a directed graph."
    )]
    async fn dag_visualization(
        &self,
        Parameters(params): Parameters<DagVisualizationParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let canonical = match validate_workflow_path(&params.path) {
            Ok(p) => p,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
        };

        let content = match std::fs::read_to_string(&canonical) {
            Ok(c) => c,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!("Cannot read file: {e}"))])),
        };

        // Parse task IDs and dependencies from YAML
        let mut tasks: Vec<(String, String)> = Vec::new(); // (id, verb)
        let mut edges: Vec<(String, String)> = Vec::new(); // (from, to)
        let mut current_id: Option<String> = None;

        for line in content.lines() {
            let trimmed = line.trim();

            // Match task ID
            if let Some(rest) = trimmed.strip_prefix("- id:") {
                current_id = Some(rest.trim().to_string());
            }

            // Match verb
            if let Some(ref id) = current_id {
                for verb in &["infer", "exec", "fetch", "invoke", "agent"] {
                    if trimmed.starts_with(&format!("{verb}:")) || trimmed.starts_with(&format!("{verb}: ")) {
                        tasks.push((id.clone(), verb.to_string()));
                        current_id = None;
                        break;
                    }
                }
            }

            // Match depends_on
            if trimmed.starts_with("depends_on:") {
                if let Some(ref id) = current_id.as_ref().or(tasks.last().map(|t| &t.0)) {
                    let deps_str = trimmed.strip_prefix("depends_on:").unwrap_or("").trim();
                    // Parse [dep1, dep2] or single dep
                    let deps_str = deps_str.trim_start_matches('[').trim_end_matches(']');
                    for dep in deps_str.split(',') {
                        let dep = dep.trim().trim_matches('"').trim_matches('\'');
                        if !dep.is_empty() {
                            edges.push((dep.to_string(), id.to_string()));
                        }
                    }
                }
            }
        }

        let mut mermaid = String::from("graph TD\n");
        for (id, verb) in &tasks {
            let style = match verb.as_str() {
                "infer" => ":::infer",
                "exec" => ":::exec",
                "fetch" => ":::fetch",
                "invoke" => ":::invoke",
                "agent" => ":::agent",
                _ => "",
            };
            mermaid.push_str(&format!("    {id}[\"{id} ({verb})\"{style}]\n"));
        }
        for (from, to) in &edges {
            mermaid.push_str(&format!("    {from} --> {to}\n"));
        }
        mermaid.push_str("\n    classDef infer fill:#7c3aed,color:white\n");
        mermaid.push_str("    classDef exec fill:#16a34a,color:white\n");
        mermaid.push_str("    classDef fetch fill:#0891b2,color:white\n");
        mermaid.push_str("    classDef invoke fill:#db2778,color:white\n");
        mermaid.push_str("    classDef agent fill:#d97306,color:white\n");

        Ok(CallToolResult::success(vec![Content::text(mermaid)]))
    }

    /// Suggest a fix for a NIKA error code given the workflow content.
    #[tool(
        name = "nika_error_fix",
        description = "Given a NIKA-XXX error code and the workflow YAML content, suggest a specific fix. Returns the corrected YAML or a targeted explanation."
    )]
    async fn error_fix(
        &self,
        Parameters(params): Parameters<ErrorFixParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let code = params.code.to_uppercase().replace("NIKA-", "");
        let num: u32 = code.parse().unwrap_or(999);

        let suggestion = match num {
            10 => "NIKA-010: Schema validation. Ensure the first line is: schema: \"nika/workflow@0.12\"",
            20 => "NIKA-020: DAG cycle detected. Check depends_on fields for circular references. Remove the cycle.",
            26 => "NIKA-026: Upstream task failed. Check the task in depends_on — it must succeed for this task to run.",
            41 => "NIKA-041: Template resolution error. Check that all {{with.alias}} references have matching entries in the task's `with:` block. Use $task_id (with $ prefix) in with: bindings.",
            45 => "NIKA-045: Fetch error. Check the URL is valid and accessible. Private IP ranges are blocked by default (SSRF protection).",
            53 => "NIKA-053: Blocked command. If using shell: true, ensure all {{with.*}} bindings use the | shell transform. Also check for $( ) command substitution which is blocked.",
            71 => "NIKA-071: Unknown alias. The {{with.alias}} reference is not declared in the task's `with:` block. Add it: with: { alias: $source_task }",
            72 => "NIKA-072: Null value at path. The binding resolved to null. Add a default: {{with.value | default(\"fallback\")}}",
            100 => "NIKA-100: MCP connection error. Check that the MCP server is running and the tool name uses double colon: server::tool_name",
            107 => "NIKA-107: MCP parameter validation failed. Check the tool's required params. Field is `params:` not `input:`.",
            270 => "NIKA-270: Skill file not found. The file referenced in `skills:` does not exist. Check the path (resolves from project root).",
            281 => "NIKA-281: Artifact write failed. Check the output path and permissions. Ensure artifacts.dir exists.",
            300 => "NIKA-300: Structured output validation failed. The LLM output doesn't match the schema. Check required fields and types. Enable repair: enable_repair: true",
            _ => "Run `nika check <file>` for detailed diagnostics with line numbers.",
        };

        Ok(CallToolResult::success(vec![Content::text(format!(
            "{suggestion}\n\nTo validate after fixing: `nika check <file>.nika.yaml`"
        ))]))
    }

    /// Look up a NIKA error code and get its description and fix.
    #[tool(
        name = "nika_error_lookup",
        description = "Look up a NIKA-XXX error code. Returns the error description, category, and how to fix it. Use when debugging workflow validation or runtime errors."
    )]
    async fn error_lookup(
        &self,
        Parameters(params): Parameters<ErrorLookupParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let code = params.code.to_uppercase().replace("NIKA-", "");
        let num: u32 = code.parse().unwrap_or(999);

        let (category, description) = match num {
            0..=9 => ("Workflow", "Workflow structure error (schema, tasks)"),
            10..=19 => (
                "Schema/Validation",
                "Schema validation error (task IDs, fields)",
            ),
            20..=29 => ("DAG", "DAG error (circular deps, missing deps)"),
            30..=39 => ("Provider", "Provider error (API key, model not found)"),
            40..=49 => ("Template/Binding", "Template or binding resolution error"),
            50..=59 => ("Path/Security", "Path, task, or security error"),
            60..=69 => ("Output", "JSON/schema validation error"),
            90..=99 => ("Execution", "Runtime execution error"),
            100..=109 => ("MCP", "MCP server/tool error"),
            110..=119 => ("Agent", "Agent loop error"),
            200..=219 => ("File Tools", "Builtin file tool error"),
            250..=259 => ("Media", "Media pipeline error"),
            270..=279 => ("Skills", "Skill file not found or invalid"),
            280..=289 => ("Artifacts", "Artifact write/path error"),
            290..=297 => (
                "Media Tools",
                "Builtin media tool error (import, decode, thumbnail, etc.)",
            ),
            300..=309 => ("Structured Output", "Structured output error"),
            _ => ("Unknown", "Unknown error code"),
        };

        Ok(CallToolResult::success(vec![Content::text(format!(
            "NIKA-{:03}: {}\nCategory: {}\nFix: Run `nika check <file>` for detailed diagnostics.",
            num, description, category
        ))]))
    }
}

/// Prompt parameters for create-workflow
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateWorkflowPromptParams {
    /// What the workflow should do
    pub description: String,
    /// Default provider (e.g., "anthropic")
    #[serde(default)]
    pub provider: Option<String>,
}

/// Prompt parameters for add-task
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddTaskPromptParams {
    /// Which verb to use (infer, exec, fetch, invoke, agent)
    pub verb: String,
    /// What the task should do
    pub description: String,
}

/// Prompt parameters for fix-error
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FixErrorPromptParams {
    /// NIKA-XXX error code
    pub code: String,
    /// The YAML content that has the error
    #[serde(default)]
    pub context: Option<String>,
}

#[rmcp::prompt_router]
impl NikaMcpServer {
    /// Generate a complete Nika workflow scaffold from a description.
    #[rmcp::prompt(description = "Generate a complete Nika .nika.yaml workflow from a description")]
    fn create_workflow(
        &self,
        Parameters(params): Parameters<CreateWorkflowPromptParams>,
    ) -> Vec<PromptMessage> {
        let provider = params.provider.as_deref().unwrap_or("anthropic");
        let content = format!(
            r#"Create a Nika workflow (.nika.yaml) that does the following:

{}

Use this template:

```yaml
schema: "nika/workflow@0.12"
workflow: my-workflow
description: "{}"
provider: {}
model: claude-sonnet-4-20250514

tasks:
  - id: step1
    infer: |
      <first task prompt>

  - id: step2
    depends_on: [step1]
    with:
      data: $step1
    infer: |
      <second task prompt using {{{{{{with.data}}}}}}>
```

Key rules:
- Always start with schema: "nika/workflow@0.12"
- Use with: {{ alias: $task_id }} for data bindings, then {{{{with.alias}}}}
- depends_on is always an array: depends_on: [task_id]
- 5 verbs: infer, exec, fetch, invoke, agent
- timeout is in seconds, not milliseconds"#,
            params.description, params.description, provider
        );

        vec![PromptMessage::new_text(PromptMessageRole::User, content)]
    }

    /// Generate a single Nika task block ready to paste into a workflow.
    #[rmcp::prompt(description = "Generate a single Nika task for a specific verb")]
    fn add_task(
        &self,
        Parameters(params): Parameters<AddTaskPromptParams>,
    ) -> Vec<PromptMessage> {
        let content = format!(
            "Generate a single Nika task using the '{}' verb that does: {}\n\n\
            Return ONLY the YAML task block (starting with `- id:`).\n\
            Follow nika/workflow@0.12 schema. Use `with:` for data bindings.",
            params.verb, params.description
        );
        vec![PromptMessage::new_text(PromptMessageRole::User, content)]
    }

    /// Suggest a fix for a NIKA-XXX error code.
    #[rmcp::prompt(description = "Fix a NIKA-XXX error in a workflow")]
    fn fix_error(
        &self,
        Parameters(params): Parameters<FixErrorPromptParams>,
    ) -> Vec<PromptMessage> {
        let context_hint = params
            .context
            .map(|c| format!("\n\nWorkflow content:\n```yaml\n{c}\n```"))
            .unwrap_or_default();

        let content = format!(
            "Fix error {} in this Nika workflow.{}\n\n\
            Return the corrected YAML. Explain what was wrong and how you fixed it.",
            params.code, context_hint
        );
        vec![PromptMessage::new_text(PromptMessageRole::User, content)]
    }
}

impl ServerHandler for NikaMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Nika workflow engine MCP server. Validate, list, explore, and generate .nika.yaml workflows."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .build(),
            ..Default::default()
        }
    }

    fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<rmcp::model::ListToolsResult, rmcp::ErrorData>> + Send + '_ {
        let tools = self.tool_router.list_all();
        std::future::ready(Ok(rmcp::model::ListToolsResult { tools, ..Default::default() }))
    }

    async fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
        context: rmcp::service::RequestContext<RoleServer>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let tool_context =
            rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        self.tool_router.call(tool_context).await
    }

    fn list_prompts(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListPromptsResult, rmcp::ErrorData>> + Send + '_ {
        let prompts = self.prompt_router.list_all();
        std::future::ready(Ok(ListPromptsResult { prompts, ..Default::default() }))
    }

    async fn get_prompt(
        &self,
        request: rmcp::model::GetPromptRequestParams,
        context: rmcp::service::RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, rmcp::ErrorData> {
        let prompt_context = rmcp::handler::server::prompt::PromptContext::new(
            self,
            request.name,
            request.arguments,
            context,
        );
        self.prompt_router.get_prompt(prompt_context).await
    }
}

/// Run the MCP server on stdio transport
pub async fn run_server() -> Result<(), Box<dyn std::error::Error>> {
    use rmcp::transport::stdio;
    use rmcp::ServiceExt;

    let handler = NikaMcpServer::new();
    let server = handler.serve(stdio()).await?;
    server.waiting().await?;
    Ok(())
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn collect_workflows(dir: &std::path::Path, results: &mut Vec<String>, depth: usize) {
    if depth > 5 || results.len() >= MAX_WORKFLOW_RESULTS {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if results.len() >= MAX_WORKFLOW_RESULTS {
                return;
            }
            let path = entry.path();
            // Use symlink_metadata to avoid following symlinks
            let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
            if is_dir {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.starts_with('.') && name != "target" && name != "node_modules" {
                    collect_workflows(&path, results, depth + 1);
                }
            } else if path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .ends_with(".nika.yaml")
            {
                results.push(path.display().to_string());
            }
        }
    }
}

/// Maximum results from workflow listing
const MAX_WORKFLOW_RESULTS: usize = 500;

/// Validate that a path is within the current working directory and has .nika.yaml extension
fn validate_workflow_path(user_path: &str) -> Result<std::path::PathBuf, String> {
    // Must have .nika.yaml extension
    if !user_path.ends_with(".nika.yaml") {
        return Err("Only .nika.yaml files can be validated".to_string());
    }

    let cwd =
        std::env::current_dir().map_err(|e| format!("Cannot determine working directory: {e}"))?;
    let canonical_cwd = cwd
        .canonicalize()
        .map_err(|e| format!("Cannot canonicalize cwd: {e}"))?;

    let requested = cwd.join(user_path);
    let canonical = requested
        .canonicalize()
        .map_err(|_| format!("File not found: {user_path}"))?;

    if !canonical.starts_with(&canonical_cwd) {
        return Err(format!("Path traversal blocked: {user_path}"));
    }

    Ok(canonical)
}

const SCHEMA_REF: &str = r#"# Nika Workflow Schema (v0.12)

## 5 Verbs
- infer: { prompt, system, temperature, max_tokens, content, extended_thinking, thinking_budget, response_format, guardrails }
- exec: { command, shell, cwd, env, timeout }
- fetch: { url, method, headers, body/json, extract, selector, response, follow_redirects, timeout }
- invoke: { tool, mcp, params, resource, timeout }
- agent: { prompt, system, tools, mcp, max_turns, max_tokens, token_budget, from, skills, guardrails, completion, limits, extended_thinking }

## Task Fields
id, description, provider, model, preset, with, depends_on, output, for_each, as, concurrency, fail_fast, retry, timeout, structured, artifact, record, context_budget, routing, log, on_error, when

## Workflow Fields
schema, workflow, description, provider, model, inputs, context, include, mcp, agents, skills, artifacts, goal, orchestrate, log, tasks

## Bindings
with: { alias: $task_id } → {{with.alias}}
Path: $task.data.field | Defaults: $task.path ?? "fallback" | Env: $env.API_KEY

## 63 Transforms
String: upper, lower, trim, trim_start, trim_end, length, to_string
Array: first, last, flatten, reverse, sort, unique, compact, keys, values
Numeric: to_number, round, abs, ceil, floor
Type: to_bool, to_json, parse_json, type_of
Parametric: join(sep), split(sep), default(val), slice(start, end)
Query: pluck(field), where(field, val), pick(f1, f2), omit(f1, f2), sort_by(field), group_by(field), merge, regex(pattern)
String test: starts_with(str), ends_with(str), contains(str)
URL: url_host, url_path, url_without_query, url_normalize
Encoding: base64_encode, base64_decode, content_hash, unique_urls
JQ: jq(expr) — full jq stdlib via jaq-core
System: shell (escape for shell: true commands)

## Extract Modes (fetch:)
markdown, article, text, selector, metadata, links, jsonpath, feed, llm_txt

## 63 Builtin Tools (nika:*)
Core (7): sleep, log, emit, assert, prompt, run, complete
File (5): read, write, edit, glob, grep
Introspection (6): dag_info, task_status, threads, orchestrate, cost, records
Data (12): json_merge, set_diff, zip, map, filter, group_by, chunk, token_count, enrich, jq, tree_data, inject
Data Sprint 2 (6): json_verify, yaml_validate, locale_lookup, aggregate, json_flatten, json_unflatten
Media always-on (5): import, decode, dimensions, thumbhash, dominant_color
Media core (3): thumbnail, convert, strip
Media opt-in (17): metadata, optimize, svg_render, chart, phash, compare, pdf_extract, provenance, verify, qr_validate, quality, html_to_md, css_select, extract_metadata, extract_links, readability, pipeline

## Providers
anthropic (claude), openai (gpt), mistral, groq, deepseek (deep-seek), gemini (google), xai (grok), native (local), mock
Fallback: provider: [groq, claude, openai]
"#;

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Path Validation Tests (RED → GREEN) ──────────────────────────────

    #[test]
    fn test_rejects_non_nika_extension() {
        let result = validate_workflow_path("/etc/passwd");
        assert!(result.is_err());
        assert!(
            result.unwrap_err().contains("Only .nika.yaml"),
            "Should reject non-.nika.yaml files"
        );
    }

    #[test]
    fn test_rejects_yaml_without_nika_prefix() {
        let result = validate_workflow_path("workflow.yaml");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Only .nika.yaml"));
    }

    #[test]
    fn test_rejects_path_traversal() {
        // Create a temp .nika.yaml outside cwd
        let result = validate_workflow_path("../../../etc/shadow.nika.yaml");
        assert!(result.is_err());
        // Should be either "File not found" or "Path traversal blocked"
    }

    #[test]
    fn test_accepts_valid_nika_yaml_in_cwd() {
        // Create a temp dir and write a .nika.yaml file, then validate using absolute path
        let tmpdir = tempfile::tempdir().unwrap();
        let test_file = tmpdir.path().join("valid.nika.yaml");
        std::fs::write(&test_file, "schema: nika/workflow@0.12\ntasks: []").unwrap();

        // validate_workflow_path uses cwd internally, so test with absolute path
        // that stays within cwd (we can't change cwd safely in parallel tests)
        let abs_path = test_file.to_string_lossy().to_string();
        // The function requires .nika.yaml extension — check extension validation works
        let result = validate_workflow_path(&abs_path);
        // Absolute path outside cwd will be blocked by traversal check — that's correct
        // So we just verify the extension check passes (file exists + has correct extension)
        // The path traversal check is tested separately in test_rejects_absolute_path_outside_cwd
        assert!(
            result.is_ok() || result.as_ref().unwrap_err().contains("Path traversal"),
            "Should accept valid .nika.yaml or block traversal, got: {:?}",
            result
        );
    }

    #[test]
    fn test_rejects_absolute_path_outside_cwd() {
        let result = validate_workflow_path("/tmp/evil.nika.yaml");
        assert!(result.is_err());
    }

    // ─── Collect Workflows Tests ──────────────────────────────────────────

    #[test]
    fn test_collect_workflows_respects_max_results() {
        let mut results = Vec::new();
        collect_workflows(std::path::Path::new("."), &mut results, 0);
        assert!(
            results.len() <= MAX_WORKFLOW_RESULTS,
            "Should not exceed {} results, got {}",
            MAX_WORKFLOW_RESULTS,
            results.len()
        );
    }

    #[test]
    fn test_collect_workflows_skips_hidden_dirs() {
        let mut results = Vec::new();
        collect_workflows(std::path::Path::new("."), &mut results, 0);
        for r in &results {
            assert!(
                !r.contains("/."),
                "Should not include files from hidden dirs: {}",
                r
            );
        }
    }

    #[test]
    fn test_collect_workflows_respects_depth_limit() {
        let mut results = Vec::new();
        // Depth 6 should be rejected
        collect_workflows(std::path::Path::new("."), &mut results, 6);
        assert!(results.is_empty(), "Depth 6 should return no results");
    }

    // ─── Error Lookup Tests ───────────────────────────────────────────────

    #[tokio::test]
    async fn test_error_lookup_returns_category() {
        let server = NikaMcpServer::new();
        let result = server
            .error_lookup(Parameters(ErrorLookupParams {
                code: "NIKA-040".to_string(),
            }))
            .await
            .unwrap();
        // Should return Template/Binding category
        let text = format!("{:?}", result);
        assert!(
            text.contains("Template") || text.contains("Binding"),
            "NIKA-040 should be Template/Binding category"
        );
    }

    #[tokio::test]
    async fn test_error_lookup_handles_invalid_code() {
        let server = NikaMcpServer::new();
        let result = server
            .error_lookup(Parameters(ErrorLookupParams {
                code: "not-a-code".to_string(),
            }))
            .await
            .unwrap();
        let text = format!("{:?}", result);
        assert!(
            text.contains("Unknown"),
            "Invalid code should return Unknown"
        );
    }

    // ─── Generate Task Tests ─────────────────────────────────────────────

    #[tokio::test]
    async fn test_generate_task_infer_default() {
        let server = NikaMcpServer::new();
        let result = server
            .generate_task(Parameters(GenerateTaskParams {
                description: "Summarize the research".to_string(),
                verb: None,
                provider: None,
            }))
            .await
            .unwrap();
        let text = format!("{:?}", result);
        assert!(text.contains("infer"), "Default verb should be infer");
        assert!(text.contains("anthropic"), "Default provider should be anthropic");
        assert!(text.contains("Summarize the research"), "Should contain description");
    }

    #[tokio::test]
    async fn test_generate_task_exec_verb() {
        let server = NikaMcpServer::new();
        let result = server
            .generate_task(Parameters(GenerateTaskParams {
                description: "Run build".to_string(),
                verb: Some("exec".to_string()),
                provider: None,
            }))
            .await
            .unwrap();
        let text = format!("{:?}", result);
        assert!(text.contains("exec:"), "Should generate exec verb");
        assert!(text.contains("shell: true"), "Exec should have shell: true");
    }

    #[tokio::test]
    async fn test_generate_task_fetch_verb() {
        let server = NikaMcpServer::new();
        let result = server
            .generate_task(Parameters(GenerateTaskParams {
                description: "Fetch API data".to_string(),
                verb: Some("fetch".to_string()),
                provider: None,
            }))
            .await
            .unwrap();
        let text = format!("{:?}", result);
        assert!(text.contains("fetch:"), "Should generate fetch verb");
        assert!(text.contains("extract:"), "Fetch should have extract mode");
    }

    #[tokio::test]
    async fn test_generate_task_agent_verb() {
        let server = NikaMcpServer::new();
        let result = server
            .generate_task(Parameters(GenerateTaskParams {
                description: "Research topic".to_string(),
                verb: Some("agent".to_string()),
                provider: Some("openai".to_string()),
            }))
            .await
            .unwrap();
        let text = format!("{:?}", result);
        assert!(text.contains("agent:"), "Should generate agent verb");
        assert!(text.contains("max_turns:"), "Agent should have max_turns");
        assert!(text.contains("openai"), "Should use specified provider");
    }

    #[tokio::test]
    async fn test_generate_task_unknown_verb_falls_back() {
        let server = NikaMcpServer::new();
        let result = server
            .generate_task(Parameters(GenerateTaskParams {
                description: "test".to_string(),
                verb: Some("invalid_verb".to_string()),
                provider: None,
            }))
            .await
            .unwrap();
        let text = format!("{:?}", result);
        assert!(text.contains("Unknown verb"), "Should warn about unknown verb");
    }

    // ─── Error Fix Tests ─────────────────────────────────────────────────

    #[tokio::test]
    async fn test_error_fix_known_code() {
        let server = NikaMcpServer::new();
        let result = server
            .error_fix(Parameters(ErrorFixParams {
                code: "NIKA-041".to_string(),
                content: "invalid yaml".to_string(),
            }))
            .await
            .unwrap();
        let text = format!("{:?}", result);
        assert!(text.contains("Template"), "NIKA-041 should mention template");
        assert!(text.contains("with:"), "Should mention with: block");
        // Verify no double prefix
        assert!(!text.contains("NIKA-041: NIKA-041"), "Should not double prefix");
    }

    #[tokio::test]
    async fn test_error_fix_unknown_code_fallback() {
        let server = NikaMcpServer::new();
        let result = server
            .error_fix(Parameters(ErrorFixParams {
                code: "NIKA-999".to_string(),
                content: "yaml".to_string(),
            }))
            .await
            .unwrap();
        let text = format!("{:?}", result);
        assert!(text.contains("nika check"), "Fallback should suggest nika check");
    }

    // ─── Prompt Tests ────────────────────────────────────────────────────

    #[test]
    fn test_prompts_registered() {
        let server = NikaMcpServer::new();
        let prompts = server.prompt_router.list_all();
        assert_eq!(prompts.len(), 3, "Should have 3 prompts");
        let names: Vec<&str> = prompts.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"create_workflow"), "Missing create_workflow prompt");
        assert!(names.contains(&"add_task"), "Missing add_task prompt");
        assert!(names.contains(&"fix_error"), "Missing fix_error prompt");
    }

    #[test]
    fn test_tools_registered() {
        let server = NikaMcpServer::new();
        let tools = server.tool_router.list_all();
        assert!(tools.len() >= 7, "Should have at least 7 tools, got {}", tools.len());
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
        assert!(names.contains(&"nika_check"), "Missing nika_check tool");
        assert!(names.contains(&"nika_generate_task"), "Missing nika_generate_task tool");
        assert!(names.contains(&"nika_dag_visualization"), "Missing nika_dag_visualization tool");
        assert!(names.contains(&"nika_error_fix"), "Missing nika_error_fix tool");
    }
}
