// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:glob — Find files matching a glob pattern.

use super::context::FileToolContext;
use super::shield::is_sensitive_file_name;
use crate::{BuiltinError, BuiltinTool, __sealed};
use nika_kernel::task_local::current_is_tainted;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

#[derive(Debug, Clone, Deserialize)]
pub struct GlobParams {
    /// Glob pattern (e.g. `*.rs`, `**/*.yaml`)
    pub pattern: String,
    /// Search root directory (defaults to working_dir)
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlobResponse {
    pub files: Vec<String>,
    pub count: usize,
}

pub struct GlobTool {
    ctx: Arc<FileToolContext>,
}

impl GlobTool {
    pub fn new(ctx: Arc<FileToolContext>) -> Self {
        Self { ctx }
    }
}

impl __sealed::Sealed for GlobTool {}

impl BuiltinTool for GlobTool {
    fn name(&self) -> &'static str {
        "glob"
    }

    fn description(&self) -> &'static str {
        "Find files matching a glob pattern"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "Glob pattern (e.g. *.rs)" },
                "path": { "type": "string", "description": "Search root (default: working dir)" }
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
            let params: GlobParams = serde_json::from_str(&args)
                .map_err(|e| BuiltinError::invalid_params("nika:glob", e))?;

            // S12.D2 — reject parent-traversal in the glob pattern itself.
            // validate_path() guards `params.path` but the glob pattern was
            // previously passed through untouched, so `../**/*` would escape
            // the working directory and expose sibling temp dirs (real
            // finding during TDD: vault.enc + audit.jsonl leaked in tests).
            // Segment equality only — substrings like `file..txt` are valid.
            if params.pattern.split('/').any(|seg| seg == "..") {
                return Err(BuiltinError::InvalidArgs {
                    tool: "nika:glob".into(),
                    reason: format!(
                        "[NIKA-204] Glob pattern contains parent traversal: {}",
                        params.pattern
                    ),
                });
            }

            let search_root = if let Some(p) = params.path.as_deref() {
                self.ctx.validate_path(p)?
            } else {
                self.ctx.working_dir.clone()
            };

            if !search_root.is_dir() {
                return Err(BuiltinError::InvalidArgs {
                    tool: "nika:glob".into(),
                    reason: format!(
                        "[NIKA-206] Search path is not a directory: {}",
                        search_root.display()
                    ),
                });
            }

            // Build full pattern: root + "/" + pattern
            let full_pattern = format!(
                "{}/{}",
                search_root.to_string_lossy(),
                params.pattern
            );

            // Shield Item 3b extension (S11.A2): capture trust state BEFORE
            // spawn_blocking. Skip sensitive files in the untrusted case so
            // they never appear in glob results.
            let untrusted = current_is_tainted();

            let files: Vec<String> = {
                let pattern = full_pattern.clone();
                tokio::task::spawn_blocking(move || -> Result<Vec<String>, BuiltinError> {
                    let paths: Vec<PathBuf> = glob::glob(&pattern)
                        .map_err(|e| BuiltinError::InvalidArgs {
                            tool: "nika:glob".into(),
                            reason: format!("[NIKA-206] Invalid glob pattern: {e}"),
                        })?
                        .filter_map(|r| r.ok())
                        .filter(|p| p.is_file())
                        .filter(|p| {
                            if !untrusted {
                                return true;
                            }
                            let name = p
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("");
                            !is_sensitive_file_name(name)
                        })
                        .collect();
                    Ok(paths
                        .into_iter()
                        .map(|p| p.to_string_lossy().into_owned())
                        .collect())
                })
                .await
                .map_err(|e| BuiltinError::Other {
                    tool: "nika:glob".into(),
                    reason: format!("Task join error: {e}"),
                })??
            };

            let count = files.len();
            serde_json::to_string(&GlobResponse { files, count })
                .map_err(|e| BuiltinError::tool_error("nika:glob", e))
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
    async fn test_glob_finds_matching_files() {
        let (d, ctx) = setup();
        std::fs::write(d.path().join("a.txt"), "a").unwrap();
        std::fs::write(d.path().join("b.txt"), "b").unwrap();
        std::fs::write(d.path().join("c.md"), "c").unwrap();

        let tool = GlobTool::new(ctx);
        let result = tool
            .call(
                serde_json::json!({
                    "pattern": "*.txt",
                    "path": d.path().to_string_lossy()
                })
                .to_string(),
            )
            .await;
        assert!(result.is_ok(), "{:?}", result);
        let v: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(v["count"], 2);
        let files = v["files"].as_array().unwrap();
        assert!(files.iter().any(|f| f.as_str().unwrap().ends_with("a.txt")));
        assert!(!files.iter().any(|f| f.as_str().unwrap().ends_with("c.md")));
    }

    #[tokio::test]
    async fn test_glob_no_matches_returns_empty() {
        let (d, ctx) = setup();
        std::fs::write(d.path().join("file.txt"), "content").unwrap();

        let tool = GlobTool::new(ctx);
        let result = tool
            .call(
                serde_json::json!({
                    "pattern": "*.rs",
                    "path": d.path().to_string_lossy()
                })
                .to_string(),
            )
            .await;
        assert!(result.is_ok());
        let v: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(v["count"], 0);
        assert_eq!(v["files"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_glob_outside_boundary_rejected() {
        let (_d, ctx) = setup();
        let tool = GlobTool::new(ctx);
        let result = tool
            .call(
                serde_json::json!({
                    "pattern": "*.conf",
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
    async fn test_glob_skips_sensitive_files_for_untrusted() {
        use nika_core::trust::TrustLevel;
        use nika_kernel::task_local::{CURRENT_TASK_ELEVATED, CURRENT_TASK_TRUST};

        let (d, ctx) = setup();
        std::fs::write(d.path().join(".env"), "SECRET=1").unwrap();
        std::fs::write(d.path().join(".env.local"), "SECRET=2").unwrap();
        std::fs::write(d.path().join("nika.toml"), "[project]").unwrap();
        std::fs::write(d.path().join("workflow.nika.yaml"), "schema: x").unwrap();
        std::fs::write(d.path().join("data.json"), "{}").unwrap();

        let tool = GlobTool::new(ctx);
        let args = serde_json::json!({
            "pattern": "*",
            "path": d.path().to_string_lossy()
        })
        .to_string();

        let result = CURRENT_TASK_TRUST
            .scope(TrustLevel::Untrusted, async {
                CURRENT_TASK_ELEVATED
                    .scope(false, async { tool.call(args).await })
                    .await
            })
            .await;

        assert!(result.is_ok(), "{:?}", result);
        let v: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        let files = v["files"].as_array().unwrap();
        let names: Vec<&str> = files
            .iter()
            .filter_map(|f| f.as_str())
            .map(|p| p.rsplit('/').next().unwrap_or(p))
            .collect();
        assert!(!names.contains(&".env"), "files: {names:?}");
        assert!(!names.contains(&".env.local"), "files: {names:?}");
        assert!(!names.contains(&"nika.toml"), "files: {names:?}");
        assert!(
            !names.contains(&"workflow.nika.yaml"),
            "files: {names:?}"
        );
        assert!(names.contains(&"data.json"), "files: {names:?}");
    }

    #[tokio::test]
    async fn test_glob_default_working_dir() {
        let (d, ctx) = setup();
        std::fs::write(d.path().join("test.txt"), "t").unwrap();

        let tool = GlobTool::new(ctx);
        let result = tool
            .call(serde_json::json!({"pattern": "*.txt"}).to_string())
            .await;
        assert!(result.is_ok());
        let v: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert!(v["count"].as_u64().unwrap() >= 1);
    }

    // ── S12.D2: reject `..` parent-traversal in glob pattern ──

    #[tokio::test]
    async fn test_glob_rejects_parent_traversal_in_pattern() {
        let (_d, ctx) = setup();
        let tool = GlobTool::new(ctx);

        for evil in [
            "../**/*",
            "../etc/passwd",
            "**/../secrets.json",
            "a/../../b",
        ] {
            let result = tool
                .call(serde_json::json!({"pattern": evil}).to_string())
                .await;
            assert!(
                result.is_err(),
                "pattern {evil:?} should have been rejected, got: {result:?}"
            );
            let msg = result.unwrap_err().to_string();
            assert!(
                msg.contains("NIKA-204") && msg.contains("parent traversal"),
                "pattern {evil:?}: unexpected error message: {msg}"
            );
        }
    }

    #[tokio::test]
    async fn test_glob_allows_double_dot_substring_in_segment() {
        // Defensive: `file..txt` is a legal name and must NOT be mistaken
        // for a parent-traversal component. Segment equality only.
        let (d, ctx) = setup();
        std::fs::write(d.path().join("file..txt"), "x").unwrap();

        let tool = GlobTool::new(ctx);
        let result = tool
            .call(
                serde_json::json!({
                    "pattern": "file..txt",
                    "path": d.path().to_string_lossy()
                })
                .to_string(),
            )
            .await;
        assert!(result.is_ok(), "{:?}", result);
    }
}
