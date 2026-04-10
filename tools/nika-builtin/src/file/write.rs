// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:write — Create or overwrite files.

use super::context::FileToolContext;
use super::limits::MAX_WRITE_BYTES;
use crate::{BuiltinError, BuiltinTool, __sealed};
use nika_kernel::task_local::current_is_tainted;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

#[derive(Debug, Clone, Deserialize)]
pub struct WriteParams {
    #[serde(alias = "path")]
    pub file_path: String,
    /// Content to write. Strings pass through; objects/arrays are pretty-printed JSON.
    #[serde(deserialize_with = "deserialize_content")]
    pub content: String,
    /// When true, overwrite existing files (default: false — fail with NIKA-215).
    #[serde(default)]
    pub overwrite: Option<bool>,
}

fn deserialize_content<'de, D>(de: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = serde_json::Value::deserialize(de)?;
    match v {
        serde_json::Value::String(s) => Ok(s),
        serde_json::Value::Object(_) | serde_json::Value::Array(_) => {
            serde_json::to_string_pretty(&v).map_err(serde::de::Error::custom)
        }
        serde_json::Value::Number(n) => Ok(n.to_string()),
        serde_json::Value::Bool(b) => Ok(b.to_string()),
        serde_json::Value::Null => Ok(String::new()),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct WriteResponse {
    pub path: String,
    pub bytes_written: usize,
    pub lines_written: usize,
}

pub struct WriteTool {
    ctx: Arc<FileToolContext>,
}

impl WriteTool {
    pub fn new(ctx: Arc<FileToolContext>) -> Self {
        Self { ctx }
    }
}

impl __sealed::Sealed for WriteTool {}

impl BuiltinTool for WriteTool {
    fn name(&self) -> &'static str {
        "write"
    }

    fn description(&self) -> &'static str {
        "Create a new file or overwrite an existing one"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string" },
                "content": { "description": "File content (string or JSON object/array)" },
                "overwrite": { "type": "boolean", "description": "Overwrite if exists (default false)" }
            },
            "required": ["file_path", "content"],
            "additionalProperties": false
        })
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, BuiltinError>> + Send + 'a>> {
        Box::pin(async move {
            let params: WriteParams = serde_json::from_str(&args)
                .map_err(|e| BuiltinError::invalid_params("nika:write", e))?;

            // Shield Item 3a restoration: untrusted tasks cannot write files.
            // Preserves the pre-S10 PermissionMode::Plan behavior via the Shield
            // trust system. See S11.A1 / project_s10_security_findings.md.
            if current_is_tainted() {
                return Err(BuiltinError::denied(
                    "nika:write",
                    format!(
                        "tainted agent cannot write files. Elevate the task with \
                         `trust: elevated` if this is intentional. Requested path: {}",
                        params.file_path
                    ),
                ));
            }

            let path = self.ctx.validate_path(&params.file_path)?;

            if (params.content.len() as u64) > MAX_WRITE_BYTES {
                return Err(BuiltinError::InvalidArgs {
                    tool: "nika:write".into(),
                    reason: format!(
                        "[NIKA-201] Content exceeds 10 MB limit ({} bytes)",
                        params.content.len()
                    ),
                });
            }

            let overwrite = params.overwrite.unwrap_or(false);
            if path.exists() && !overwrite {
                return Err(BuiltinError::InvalidArgs {
                    tool: "nika:write".into(),
                    reason: format!(
                        "[NIKA-215] File already exists: {}. Use overwrite: true to replace it.",
                        path.display()
                    ),
                });
            }

            // Ensure parent directory exists
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await.map_err(|e| {
                    BuiltinError::Io {
                        tool: "nika:write".into(),
                        reason: format!("Failed to create parent directory: {e}"),
                    }
                })?;
            }

            let bytes = params.content.as_bytes();
            let bytes_written = bytes.len();
            let lines_written = params.content.lines().count();

            tokio::fs::write(&path, bytes).await.map_err(|e| BuiltinError::Io {
                tool: "nika:write".into(),
                reason: format!("[NIKA-201] Failed to write {}: {e}", path.display()),
            })?;

            serde_json::to_string(&WriteResponse {
                path: path.to_string_lossy().into_owned(),
                bytes_written,
                lines_written,
            })
            .map_err(|e| BuiltinError::tool_error("nika:write", e))
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
    async fn test_write_creates_file() {
        let (d, ctx) = setup();
        let path = d.path().join("new.txt");

        let tool = WriteTool::new(ctx);
        let result = tool
            .call(
                serde_json::json!({
                    "file_path": path.to_string_lossy(),
                    "content": "Hello Nika!"
                })
                .to_string(),
            )
            .await;
        assert!(result.is_ok(), "{:?}", result);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "Hello Nika!");
    }

    #[tokio::test]
    async fn test_write_fails_if_exists_without_overwrite() {
        let (d, ctx) = setup();
        let path = d.path().join("existing.txt");
        std::fs::write(&path, "original").unwrap();

        let tool = WriteTool::new(ctx);
        let result = tool
            .call(
                serde_json::json!({
                    "file_path": path.to_string_lossy(),
                    "content": "new content"
                })
                .to_string(),
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-215"));
        // Original content preserved
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "original");
    }

    #[tokio::test]
    async fn test_write_with_overwrite() {
        let (d, ctx) = setup();
        let path = d.path().join("over.txt");
        std::fs::write(&path, "original").unwrap();

        let tool = WriteTool::new(ctx);
        let result = tool
            .call(
                serde_json::json!({
                    "file_path": path.to_string_lossy(),
                    "content": "replaced",
                    "overwrite": true
                })
                .to_string(),
            )
            .await;
        assert!(result.is_ok());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "replaced");
    }

    #[tokio::test]
    async fn test_write_creates_parent_dirs() {
        let (d, ctx) = setup();
        let path = d.path().join("sub/dir/file.txt");

        let tool = WriteTool::new(ctx);
        let result = tool
            .call(
                serde_json::json!({
                    "file_path": path.to_string_lossy(),
                    "content": "nested"
                })
                .to_string(),
            )
            .await;
        assert!(result.is_ok());
        assert!(path.exists());
    }

    #[tokio::test]
    async fn test_write_outside_boundary_rejected() {
        let (_d, ctx) = setup();
        let tool = WriteTool::new(ctx);
        let result = tool
            .call(
                serde_json::json!({
                    "file_path": "/tmp/evil.txt",
                    "content": "evil"
                })
                .to_string(),
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-204"));
    }

    // ── S11.A1 Shield trust gate ──

    #[tokio::test]
    async fn test_write_blocked_for_untrusted_task() {
        use super::super::test_util::run_as;
        use nika_core::trust::TrustLevel;

        let (d, ctx) = setup();
        let path = d.path().join("target.txt");

        let tool = WriteTool::new(ctx);
        let args = serde_json::json!({
            "file_path": path.to_string_lossy(),
            "content": "should be blocked"
        })
        .to_string();

        let result = run_as(TrustLevel::Untrusted, false, tool.call(args)).await;

        assert!(result.is_err(), "untrusted write must be denied");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("NIKA-380"), "expected NIKA-380, got: {err}");
        assert!(!path.exists(), "no file must be created when write is denied");
    }

    #[tokio::test]
    async fn test_write_allowed_for_elevated_untrusted() {
        use super::super::test_util::run_as;
        use nika_core::trust::TrustLevel;

        let (d, ctx) = setup();
        let path = d.path().join("elevated.txt");
        let tool = WriteTool::new(ctx);
        let args = serde_json::json!({
            "file_path": path.to_string_lossy(),
            "content": "allowed because elevated"
        })
        .to_string();

        let result = run_as(TrustLevel::Untrusted, true, tool.call(args)).await;
        assert!(result.is_ok(), "elevated untrusted must pass: {:?}", result);
        assert!(path.exists());
    }

    #[tokio::test]
    async fn test_write_allowed_for_trusted_task() {
        use super::super::test_util::run_as;
        use nika_core::trust::TrustLevel;

        let (d, ctx) = setup();
        let path = d.path().join("trusted.txt");
        let tool = WriteTool::new(ctx);
        let args = serde_json::json!({
            "file_path": path.to_string_lossy(),
            "content": "trusted caller ok"
        })
        .to_string();

        let result = run_as(TrustLevel::Trusted, false, tool.call(args)).await;
        assert!(result.is_ok(), "trusted caller must pass: {:?}", result);
        assert!(path.exists());
    }

    #[tokio::test]
    async fn test_write_json_object_is_pretty_printed() {
        let (d, ctx) = setup();
        let path = d.path().join("data.json");

        let tool = WriteTool::new(ctx);
        let result = tool
            .call(
                serde_json::json!({
                    "file_path": path.to_string_lossy(),
                    "content": {"key": "value", "num": 42}
                })
                .to_string(),
            )
            .await;
        assert!(result.is_ok());
        let content = std::fs::read_to_string(&path).unwrap();
        // Should be pretty-printed JSON
        assert!(content.contains("\"key\""));
        assert!(content.contains('\n'));
    }
}
