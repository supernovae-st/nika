// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:read — Read files with optional line numbers, offset, and limit.

use super::limits::MAX_READ_BYTES;
use super::{context::FileToolContext, shield::check_path_readable};
use crate::{BuiltinError, BuiltinTool, __sealed};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

const DEFAULT_LIMIT: usize = 2000;
const MAX_LINE_LENGTH: usize = 2000;

#[derive(Debug, Clone, Deserialize)]
pub struct ReadParams {
    pub file_path: String,
    #[serde(default)]
    pub offset: Option<usize>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub raw: Option<bool>,
    #[serde(default)]
    pub optional: Option<bool>,
    #[serde(default)]
    pub fallback: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReadResponse {
    pub content: String,
    pub total_lines: usize,
    pub lines_returned: usize,
    pub truncated: bool,
}

pub struct ReadTool {
    ctx: Arc<FileToolContext>,
}

impl ReadTool {
    pub fn new(ctx: Arc<FileToolContext>) -> Self {
        Self { ctx }
    }
}

impl __sealed::Sealed for ReadTool {}

impl BuiltinTool for ReadTool {
    fn name(&self) -> &'static str {
        "read"
    }

    fn description(&self) -> &'static str {
        "Read a file with line numbers"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string", "description": "Absolute path to file" },
                "offset": { "type": "integer", "description": "Start from this line (1-indexed)" },
                "limit": { "type": "integer", "description": "Max lines to return (default 2000)" },
                "raw": { "type": "boolean", "description": "Return raw content without line numbers" },
                "optional": { "type": "boolean", "description": "Return fallback if file missing" },
                "fallback": { "type": "string", "description": "Fallback value when optional=true" }
            },
            "required": ["file_path"],
            "additionalProperties": false
        })
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, BuiltinError>> + Send + 'a>> {
        Box::pin(async move {
            let params: ReadParams = serde_json::from_str(&args)
                .map_err(|e| BuiltinError::invalid_params("nika:read", e))?;

            let path = self.ctx.validate_path(&params.file_path)?;
            check_path_readable(&path)?;

            // Handle optional reads
            if !path.exists() {
                if params.optional.unwrap_or(false) {
                    let fallback = params.fallback.unwrap_or_default();
                    return serde_json::to_string(&ReadResponse {
                        content: fallback,
                        total_lines: 0,
                        lines_returned: 0,
                        truncated: false,
                    })
                    .map_err(|e| BuiltinError::tool_error("nika:read", e));
                }
                return Err(BuiltinError::Io {
                    tool: "nika:read".into(),
                    reason: format!("[NIKA-208] File not found: {}", path.display()),
                });
            }

            // DoS pre-check (S11.A3): stat before read_to_string so a 2 GB file
            // cannot be loaded fully into memory just to be trimmed by offset/limit.
            let metadata = tokio::fs::metadata(&path).await.map_err(|e| BuiltinError::Io {
                tool: "nika:read".into(),
                reason: format!("[NIKA-200] Failed to stat {}: {e}", path.display()),
            })?;
            if metadata.len() > MAX_READ_BYTES {
                return Err(BuiltinError::InvalidArgs {
                    tool: "nika:read".into(),
                    reason: format!(
                        "[NIKA-200] File too large: {} bytes (max {} bytes / 50 MB). \
                         Use offset/limit on a narrower range or split the file.",
                        metadata.len(),
                        MAX_READ_BYTES
                    ),
                });
            }

            let raw_content = tokio::fs::read_to_string(&path).await.map_err(|e| {
                BuiltinError::Io {
                    tool: "nika:read".into(),
                    reason: format!("[NIKA-200] Failed to read {}: {e}", path.display()),
                }
            })?;

            // Track that we read this file (for edit-before-edit enforcement)
            self.ctx.mark_read(&path);

            let all_lines: Vec<&str> = raw_content.lines().collect();
            let total_lines = all_lines.len();

            let offset = params.offset.map(|o| o.saturating_sub(1)).unwrap_or(0);
            let limit = params.limit.unwrap_or(DEFAULT_LIMIT);

            let lines: Vec<&str> = all_lines
                .iter()
                .skip(offset)
                .take(limit)
                .copied()
                .collect();

            let lines_returned = lines.len();
            let truncated = (offset + limit) < total_lines;

            let content = if params.raw.unwrap_or(false) {
                lines.join("\n")
            } else {
                lines
                    .iter()
                    .enumerate()
                    .map(|(i, line)| {
                        let line_num = offset + i + 1;
                        let line = if line.len() > MAX_LINE_LENGTH {
                            &line[..MAX_LINE_LENGTH]
                        } else {
                            line
                        };
                        format!("{line_num}\t{line}")
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            };

            serde_json::to_string(&ReadResponse {
                content,
                total_lines,
                lines_returned,
                truncated,
            })
            .map_err(|e| BuiltinError::tool_error("nika:read", e))
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
    async fn test_read_basic() {
        let (d, ctx) = setup();
        let path = d.path().join("test.txt");
        std::fs::write(&path, "line1\nline2\nline3").unwrap();

        let tool = ReadTool::new(ctx);
        let result = tool
            .call(serde_json::json!({"file_path": path.to_string_lossy()}).to_string())
            .await;
        assert!(result.is_ok(), "{:?}", result);
        let v: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(v["total_lines"], 3);
        assert!(v["content"].as_str().unwrap().contains("1\tline1"));
    }

    #[tokio::test]
    async fn test_read_raw_mode() {
        let (d, ctx) = setup();
        let path = d.path().join("raw.txt");
        std::fs::write(&path, "hello\nworld").unwrap();

        let tool = ReadTool::new(ctx);
        let result = tool
            .call(
                serde_json::json!({"file_path": path.to_string_lossy(), "raw": true}).to_string(),
            )
            .await;
        assert!(result.is_ok());
        let v: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        // Raw mode — no line number prefix
        assert!(!v["content"].as_str().unwrap().contains('\t'));
    }

    #[tokio::test]
    async fn test_read_optional_missing_file() {
        let (d, ctx) = setup();
        let path = d.path().join("missing.txt");

        let tool = ReadTool::new(ctx);
        let result = tool
            .call(
                serde_json::json!({
                    "file_path": path.to_string_lossy(),
                    "optional": true,
                    "fallback": "default_content"
                })
                .to_string(),
            )
            .await;
        assert!(result.is_ok());
        let v: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(v["content"], "default_content");
    }

    #[tokio::test]
    async fn test_read_nonexistent_file_errors() {
        let (d, ctx) = setup();
        let path = d.path().join("missing.txt");

        let tool = ReadTool::new(ctx);
        let result = tool
            .call(serde_json::json!({"file_path": path.to_string_lossy()}).to_string())
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-208"));
    }

    #[tokio::test]
    async fn test_read_marks_file_as_read() {
        let (d, ctx) = setup();
        let path = d.path().join("check.txt");
        std::fs::write(&path, "content").unwrap();

        assert!(!ctx.has_been_read(&path));
        let tool = ReadTool::new(Arc::clone(&ctx));
        tool.call(serde_json::json!({"file_path": path.to_string_lossy()}).to_string())
            .await
            .unwrap();
        assert!(ctx.has_been_read(&path));
    }

    #[tokio::test]
    async fn test_read_outside_boundary_rejected() {
        let (_d, ctx) = setup();
        let tool = ReadTool::new(ctx);
        let result = tool
            .call(serde_json::json!({"file_path": "/etc/passwd"}).to_string())
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-204"));
    }

    // ── S11.A3 DoS pre-check: reject files >50MB before read_to_string ──

    #[tokio::test]
    async fn test_read_rejects_oversized_file() {
        let (d, ctx) = setup();
        let path = d.path().join("huge.bin");

        // Create a 51 MB sparse file via set_len (no actual write).
        let f = std::fs::File::create(&path).unwrap();
        f.set_len(51 * 1024 * 1024).unwrap();
        drop(f);

        let tool = ReadTool::new(ctx);
        let args = serde_json::json!({ "file_path": path.to_string_lossy() }).to_string();
        let result = tool.call(args).await;

        assert!(result.is_err(), "51 MB file must be rejected");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("50") || err.to_lowercase().contains("too large") || err.to_lowercase().contains("size"),
            "error should mention size limit: {err}"
        );
    }

    #[tokio::test]
    async fn test_read_accepts_small_file_after_size_check() {
        let (d, ctx) = setup();
        let path = d.path().join("tiny.txt");
        std::fs::write(&path, "hello").unwrap();

        let tool = ReadTool::new(ctx);
        let args = serde_json::json!({ "file_path": path.to_string_lossy() }).to_string();
        let result = tool.call(args).await;
        assert!(result.is_ok(), "{:?}", result);
    }

    #[tokio::test]
    async fn test_read_offset_and_limit() {
        let (d, ctx) = setup();
        let path = d.path().join("multi.txt");
        std::fs::write(&path, "l1\nl2\nl3\nl4\nl5").unwrap();

        let tool = ReadTool::new(ctx);
        let result = tool
            .call(
                serde_json::json!({
                    "file_path": path.to_string_lossy(),
                    "offset": 2,
                    "limit": 2
                })
                .to_string(),
            )
            .await;
        assert!(result.is_ok());
        let v: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(v["lines_returned"], 2);
        assert_eq!(v["total_lines"], 5);
        assert_eq!(v["truncated"], true);
    }
}
