// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:grep — Search file contents with a regex pattern.

use super::context::FileToolContext;
use super::shield::is_sensitive_file_name;
use crate::{BuiltinError, BuiltinTool, __sealed};
use nika_kernel::task_local::current_is_tainted;
use regex::RegexBuilder;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

#[derive(Debug, Clone, Deserialize)]
pub struct GrepParams {
    /// Regex pattern to search for
    pub pattern: String,
    /// Root path to search in (default: working_dir)
    #[serde(default)]
    pub path: Option<String>,
    /// Case-insensitive search (default: false)
    #[serde(default)]
    pub case_insensitive: Option<bool>,
    /// File extension filter (e.g. "rs" or "*.rs")
    #[serde(default)]
    pub file_filter: Option<String>,
    /// Context lines around each match (default: 0)
    #[serde(default)]
    pub context_lines: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GrepMatch {
    pub file: String,
    pub line: usize,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GrepResponse {
    pub matches: Vec<GrepMatch>,
    pub count: usize,
    pub files_searched: usize,
}

const MAX_MATCHES: usize = 1000;

pub struct GrepTool {
    ctx: Arc<FileToolContext>,
}

impl GrepTool {
    pub fn new(ctx: Arc<FileToolContext>) -> Self {
        Self { ctx }
    }
}

impl __sealed::Sealed for GrepTool {}

impl BuiltinTool for GrepTool {
    fn name(&self) -> &'static str {
        "grep"
    }

    fn description(&self) -> &'static str {
        "Search file contents with a regex pattern"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "Regex pattern" },
                "path": { "type": "string", "description": "Root path to search" },
                "case_insensitive": { "type": "boolean" },
                "file_filter": { "type": "string", "description": "File extension filter" },
                "context_lines": { "type": "integer", "description": "Context lines around matches" }
            },
            "required": ["pattern"],
            "additionalProperties": false
        })
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, BuiltinError>> + Send + 'a>> {
        Box::pin(async move {
            let params: GrepParams = serde_json::from_str(&args)
                .map_err(|e| BuiltinError::invalid_params("nika:grep", e))?;

            let search_root = if let Some(p) = params.path.as_deref() {
                self.ctx.validate_path(p)?
            } else {
                self.ctx.working_dir.clone()
            };

            let regex = RegexBuilder::new(&params.pattern)
                .case_insensitive(params.case_insensitive.unwrap_or(false))
                .build()
                .map_err(|e| BuiltinError::InvalidArgs {
                    tool: "nika:grep".into(),
                    reason: format!("[NIKA-207] Invalid regex pattern: {e}"),
                })?;

            let file_filter = params.file_filter.clone();
            let root = search_root.clone();
            // Shield Item 3b extension (S11.A2): capture trust state BEFORE
            // spawn_blocking. task_local does not propagate across thread
            // boundaries, so we snapshot it here and move the bool into the
            // closure to filter sensitive files inside the walker.
            let untrusted = current_is_tainted();

            let (grep_matches, files_searched) =
                tokio::task::spawn_blocking(move || -> Result<(Vec<GrepMatch>, usize), BuiltinError> {
                    let walker = ignore::WalkBuilder::new(&root)
                        .hidden(false)
                        .build();

                    let mut results: Vec<GrepMatch> = Vec::new();
                    let mut files_searched = 0usize;

                    'outer: for entry in walker {
                        let entry = match entry {
                            Ok(e) => e,
                            Err(_) => continue,
                        };

                        let path = entry.path();
                        if !path.is_file() {
                            continue;
                        }

                        // Shield Item 3b extension: skip sensitive files when
                        // caller is untrusted. Prevents .env / nika.toml / .mcp.json
                        // / *.nika.yaml contents from leaking through grep results.
                        if untrusted {
                            let name = path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("");
                            if is_sensitive_file_name(name) {
                                continue;
                            }
                        }

                        // Apply file filter if provided
                        if let Some(ref filter) = file_filter {
                            let ext_filter = filter.trim_start_matches("*.");
                            let name = path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("");
                            if !name.ends_with(ext_filter) {
                                continue;
                            }
                        }

                        // Read file content, skip on error
                        let content = match std::fs::read_to_string(path) {
                            Ok(c) => c,
                            Err(_) => continue,
                        };

                        files_searched += 1;

                        for (line_idx, line) in content.lines().enumerate() {
                            if regex.is_match(line) {
                                results.push(GrepMatch {
                                    file: path.to_string_lossy().into_owned(),
                                    line: line_idx + 1,
                                    content: line.to_string(),
                                });
                                if results.len() >= MAX_MATCHES {
                                    break 'outer;
                                }
                            }
                        }
                    }

                    Ok((results, files_searched))
                })
                .await
                .map_err(|e| BuiltinError::Other {
                    tool: "nika:grep".into(),
                    reason: format!("Task join error: {e}"),
                })??;

            let count = grep_matches.len();
            serde_json::to_string(&GrepResponse {
                matches: grep_matches,
                count,
                files_searched,
            })
            .map_err(|e| BuiltinError::tool_error("nika:grep", e))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Arc<FileToolContext>) {
        let d = TempDir::new().unwrap();
        let ctx = Arc::new(FileToolContext::new(d.path().to_path_buf()));
        (d, ctx)
    }

    #[tokio::test]
    async fn test_grep_finds_matches() {
        let (d, ctx) = setup();
        std::fs::write(
            d.path().join("search.txt"),
            "line 1: hello\nline 2: world\nline 3: hello world",
        )
        .unwrap();

        let tool = GrepTool::new(ctx);
        let result = tool
            .call(
                serde_json::json!({
                    "pattern": "hello",
                    "path": d.path().to_string_lossy()
                })
                .to_string(),
            )
            .await;
        assert!(result.is_ok(), "{:?}", result);
        let v: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(v["count"], 2);
    }

    #[tokio::test]
    async fn test_grep_no_matches_returns_empty() {
        let (d, ctx) = setup();
        std::fs::write(d.path().join("file.txt"), "Hello World").unwrap();

        let tool = GrepTool::new(ctx);
        let result = tool
            .call(
                serde_json::json!({
                    "pattern": "XYZNONEXISTENT",
                    "path": d.path().to_string_lossy()
                })
                .to_string(),
            )
            .await;
        assert!(result.is_ok());
        let v: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(v["count"], 0);
    }

    #[tokio::test]
    async fn test_grep_case_insensitive() {
        let (d, ctx) = setup();
        std::fs::write(d.path().join("ci.txt"), "Hello World\nhello world\nHELLO").unwrap();

        let tool = GrepTool::new(ctx);
        let result = tool
            .call(
                serde_json::json!({
                    "pattern": "hello",
                    "path": d.path().to_string_lossy(),
                    "case_insensitive": true
                })
                .to_string(),
            )
            .await;
        assert!(result.is_ok());
        let v: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(v["count"], 3);
    }

    #[tokio::test]
    async fn test_grep_invalid_regex_errors() {
        let (d, ctx) = setup();
        let tool = GrepTool::new(ctx);
        let result = tool
            .call(
                serde_json::json!({
                    "pattern": "[invalid",
                    "path": d.path().to_string_lossy()
                })
                .to_string(),
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-207"));
    }

    #[tokio::test]
    async fn test_grep_outside_boundary_rejected() {
        let (_d, ctx) = setup();
        let tool = GrepTool::new(ctx);
        let result = tool
            .call(
                serde_json::json!({
                    "pattern": "root",
                    "path": "/etc"
                })
                .to_string(),
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-204"));
    }

    // ── S11.A2 Shield 3b extension: skip sensitive files for untrusted ──

    #[tokio::test]
    async fn test_grep_skips_sensitive_files_for_untrusted() {
        use super::super::test_util::run_as;
        use nika_core::trust::TrustLevel;

        let (d, ctx) = setup();
        std::fs::write(d.path().join(".env"), "SECRET_KEY=leak_me").unwrap();
        std::fs::write(d.path().join("normal.txt"), "SECRET_KEY=this_is_fine").unwrap();

        let tool = GrepTool::new(ctx);
        let args = serde_json::json!({
            "pattern": "SECRET_KEY",
            "path": d.path().to_string_lossy()
        })
        .to_string();

        let result = run_as(TrustLevel::Untrusted, false, tool.call(args)).await;

        assert!(result.is_ok(), "{:?}", result);
        let body = result.unwrap();
        assert!(!body.contains("leak_me"), ".env contents must not leak: {body}");
        assert!(
            body.contains("this_is_fine"),
            "normal.txt must still be grepped: {body}"
        );
    }

    #[tokio::test]
    async fn test_grep_file_filter() {
        let (d, ctx) = setup();
        std::fs::write(d.path().join("code.rs"), "fn hello() {}").unwrap();
        std::fs::write(d.path().join("doc.txt"), "hello world").unwrap();

        let tool = GrepTool::new(ctx);
        let result = tool
            .call(
                serde_json::json!({
                    "pattern": "hello",
                    "path": d.path().to_string_lossy(),
                    "file_filter": "*.rs"
                })
                .to_string(),
            )
            .await;
        assert!(result.is_ok());
        let v: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        let matches = v["matches"].as_array().unwrap();
        // Only .rs file should be searched
        for m in matches {
            assert!(m["file"].as_str().unwrap().ends_with(".rs"));
        }
    }
}
