//! MCP Server for Nika — exposes workflow tools via Model Context Protocol
//!
//! Allows AI coding tools (Claude Code, Cursor, Copilot, etc.) to validate,
//! run, and explore Nika workflows through MCP.

use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content, ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ServerHandler};
use schemars::JsonSchema;
use serde::Deserialize;

/// Nika MCP Server handler
#[derive(Clone)]
pub struct NikaMcpServer {
    tool_router: ToolRouter<Self>,
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

#[tool_router]
impl NikaMcpServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
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
        let path = std::path::Path::new(&params.path);
        if !path.exists() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "File not found: {}",
                params.path
            ))]));
        }

        match std::process::Command::new("nika")
            .args(["check", &params.path])
            .output()
        {
            Ok(output) => {
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
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to run nika check: {}",
                e
            ))])),
        }
    }

    /// List all .nika.yaml workflow files in the current directory and subdirectories.
    #[tool(
        name = "nika_list_workflows",
        description = "List all .nika.yaml workflow files in the project. Use to discover available workflows."
    )]
    async fn list_workflows(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let mut workflows = Vec::new();
        collect_workflows(std::path::Path::new("."), &mut workflows, 0);

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
            10..=19 => ("Schema/Validation", "Schema validation error (task IDs, fields)"),
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
            300..=309 => ("Structured Output", "Structured output error"),
            _ => ("Unknown", "Unknown error code"),
        };

        Ok(CallToolResult::success(vec![Content::text(format!(
            "NIKA-{:03}: {}\nCategory: {}\nFix: Run `nika check <file>` for detailed diagnostics.",
            num, description, category
        ))]))
    }
}

#[tool_handler]
impl ServerHandler for NikaMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Nika workflow engine MCP server. Validate, list, and explore .nika.yaml workflows."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            ..Default::default()
        }
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
    if depth > 5 {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
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

const SCHEMA_REF: &str = r#"# Nika Workflow Schema (v0.12)

## 5 Verbs
- infer: { prompt, system, temperature, max_tokens, content, extended_thinking }
- exec: { command, shell, cwd, env, timeout_ms }
- fetch: { url, method, headers, body/json, extract, selector, response }
- invoke: { tool, mcp, params, resource }
- agent: { prompt, tools, max_turns, system, mcp, guardrails, completion }

## Task Fields
id, description, provider, model, with, depends_on, output, for_each, as, concurrency, fail_fast, retry, timeout, structured, artifact, log

## Bindings
with: { alias: $task_id } → {{with.alias}}
Transforms: upper, lower, trim, length, first, last, keys, values, flatten, sort, unique, to_json, parse_json, join(sep), split(sep), default(val)

## Extract Modes (fetch:)
markdown, article, text, selector, metadata, links, jsonpath, feed, llm_txt
"#;
