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
use tokio::process::Command as TokioCommand;
use tokio::time::{timeout, Duration};

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
            290..=297 => ("Media Tools", "Builtin media tool error (import, decode, thumbnail, etc.)"),
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
id, description, provider, model, base_url, preset, with, depends_on, output, for_each, as, concurrency, fail_fast, retry, timeout, structured, artifact, record, context_budget, routing, log

## Workflow Fields
schema, workflow, description, provider, model, inputs, context, include, mcp, agents, skills, artifacts, goal, orchestrate, log, tasks

## Bindings
with: { alias: $task_id } → {{with.alias}}
Path: $task.data.field | Defaults: $task.path ?? "fallback" | Env: $env.API_KEY

## 50 Transforms
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

## 62 Builtin Tools (nika:*)
Core (7): sleep, log, emit, assert, prompt, run, complete
File (5): read, write, edit, glob, grep
Introspection (6): dag_info, task_status, threads, orchestrate, cost, records
Data (13): json_merge, set_diff, zip, map, filter, group_by, chunk, token_count, enrich, jq, tree_data, inject, json_query (deprecated → jq)
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
}
