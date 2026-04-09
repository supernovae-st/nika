// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:edit — Modify files via exact string replacement.
//!
//! Enforces read-before-edit: the file must have been read via `nika:read`
//! in the same context before an edit is permitted.

use super::context::FileToolContext;
use crate::{BuiltinError, BuiltinTool, __sealed};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

#[derive(Debug, Clone, Deserialize)]
pub struct EditParams {
    pub file_path: String,
    pub old_string: String,
    pub new_string: String,
    /// Replace ALL occurrences (default: false — errors if not unique).
    #[serde(default)]
    pub replace_all: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EditResponse {
    pub path: String,
    pub replacements: usize,
}

pub struct EditTool {
    ctx: Arc<FileToolContext>,
}

impl EditTool {
    pub fn new(ctx: Arc<FileToolContext>) -> Self {
        Self { ctx }
    }
}

impl __sealed::Sealed for EditTool {}

impl BuiltinTool for EditTool {
    fn name(&self) -> &'static str {
        "edit"
    }

    fn description(&self) -> &'static str {
        "Modify a file by replacing an exact string"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string" },
                "old_string": { "type": "string", "description": "String to replace (must be unique)" },
                "new_string": { "type": "string", "description": "Replacement string" },
                "replace_all": { "type": "boolean", "description": "Replace all occurrences" }
            },
            "required": ["file_path", "old_string", "new_string"],
            "additionalProperties": false
        })
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, BuiltinError>> + Send + 'a>> {
        Box::pin(async move {
            let params: EditParams = serde_json::from_str(&args)
                .map_err(|e| BuiltinError::invalid_params("nika:edit", e))?;

            let path = self.ctx.validate_path(&params.file_path)?;

            // Enforce read-before-edit
            if !self.ctx.has_been_read(&path) {
                return Err(BuiltinError::InvalidArgs {
                    tool: "nika:edit".into(),
                    reason: format!(
                        "[NIKA-203] Must read file before editing: {}. \
                         Call nika:read first.",
                        path.display()
                    ),
                });
            }

            if !path.exists() {
                return Err(BuiltinError::Io {
                    tool: "nika:edit".into(),
                    reason: format!("[NIKA-208] File not found: {}", path.display()),
                });
            }

            let content = tokio::fs::read_to_string(&path).await.map_err(|e| {
                BuiltinError::Io {
                    tool: "nika:edit".into(),
                    reason: format!("[NIKA-202] Failed to read {}: {e}", path.display()),
                }
            })?;

            let replace_all = params.replace_all.unwrap_or(false);

            let occurrences = content.matches(&*params.old_string).count();
            if occurrences == 0 {
                return Err(BuiltinError::InvalidArgs {
                    tool: "nika:edit".into(),
                    reason: format!(
                        "[NIKA-208] old_string not found in {}: {:?}",
                        path.display(),
                        params.old_string
                    ),
                });
            }

            if !replace_all && occurrences > 1 {
                return Err(BuiltinError::InvalidArgs {
                    tool: "nika:edit".into(),
                    reason: format!(
                        "[NIKA-209] old_string not unique in {} ({occurrences} occurrences). \
                         Use replace_all: true or provide more context.",
                        path.display()
                    ),
                });
            }

            let new_content = if replace_all {
                content.replace(&*params.old_string, &params.new_string)
            } else {
                content.replacen(&*params.old_string, &params.new_string, 1)
            };

            tokio::fs::write(&path, &new_content).await.map_err(|e| BuiltinError::Io {
                tool: "nika:edit".into(),
                reason: format!("[NIKA-202] Failed to write {}: {e}", path.display()),
            })?;

            serde_json::to_string(&EditResponse {
                path: path.to_string_lossy().into_owned(),
                replacements: occurrences,
            })
            .map_err(|e| BuiltinError::tool_error("nika:edit", e))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file::read::ReadTool;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Arc<FileToolContext>) {
        let d = TempDir::new().unwrap();
        let ctx = Arc::new(FileToolContext::new(d.path().to_path_buf()));
        (d, ctx)
    }

    async fn read_file(ctx: Arc<FileToolContext>, path: &std::path::Path) {
        let tool = ReadTool::new(ctx);
        tool.call(serde_json::json!({"file_path": path.to_string_lossy()}).to_string())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_edit_replaces_string() {
        let (d, ctx) = setup();
        let path = d.path().join("hello.txt");
        std::fs::write(&path, "Hello World").unwrap();

        read_file(Arc::clone(&ctx), &path).await;

        let tool = EditTool::new(Arc::clone(&ctx));
        let result = tool
            .call(
                serde_json::json!({
                    "file_path": path.to_string_lossy(),
                    "old_string": "World",
                    "new_string": "Nika"
                })
                .to_string(),
            )
            .await;
        assert!(result.is_ok(), "{:?}", result);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "Hello Nika");
    }

    #[tokio::test]
    async fn test_edit_without_read_rejected() {
        let (d, ctx) = setup();
        let path = d.path().join("noread.txt");
        std::fs::write(&path, "content").unwrap();

        let tool = EditTool::new(ctx);
        let result = tool
            .call(
                serde_json::json!({
                    "file_path": path.to_string_lossy(),
                    "old_string": "content",
                    "new_string": "replaced"
                })
                .to_string(),
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-203"));
    }

    #[tokio::test]
    async fn test_edit_old_string_not_found() {
        let (d, ctx) = setup();
        let path = d.path().join("text.txt");
        std::fs::write(&path, "Hello World").unwrap();
        read_file(Arc::clone(&ctx), &path).await;

        let tool = EditTool::new(Arc::clone(&ctx));
        let result = tool
            .call(
                serde_json::json!({
                    "file_path": path.to_string_lossy(),
                    "old_string": "NONEXISTENT",
                    "new_string": "x"
                })
                .to_string(),
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-208"));
    }

    #[tokio::test]
    async fn test_edit_not_unique_rejected() {
        let (d, ctx) = setup();
        let path = d.path().join("dup.txt");
        std::fs::write(&path, "foo foo foo").unwrap();
        read_file(Arc::clone(&ctx), &path).await;

        let tool = EditTool::new(Arc::clone(&ctx));
        let result = tool
            .call(
                serde_json::json!({
                    "file_path": path.to_string_lossy(),
                    "old_string": "foo",
                    "new_string": "bar"
                })
                .to_string(),
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-209"));
    }

    #[tokio::test]
    async fn test_edit_replace_all() {
        let (d, ctx) = setup();
        let path = d.path().join("all.txt");
        std::fs::write(&path, "foo bar foo").unwrap();
        read_file(Arc::clone(&ctx), &path).await;

        let tool = EditTool::new(Arc::clone(&ctx));
        let result = tool
            .call(
                serde_json::json!({
                    "file_path": path.to_string_lossy(),
                    "old_string": "foo",
                    "new_string": "baz",
                    "replace_all": true
                })
                .to_string(),
            )
            .await;
        assert!(result.is_ok());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "baz bar baz");
    }

    #[tokio::test]
    async fn test_edit_outside_boundary_rejected() {
        let (_d, ctx) = setup();
        let tool = EditTool::new(ctx);
        let result = tool
            .call(
                serde_json::json!({
                    "file_path": "/etc/hosts",
                    "old_string": "localhost",
                    "new_string": "evil"
                })
                .to_string(),
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-204"));
    }
}
