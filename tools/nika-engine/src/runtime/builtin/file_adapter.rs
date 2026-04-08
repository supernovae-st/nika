// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! File Adapter - Bridges FileTool to BuiltinTool for nika:* integration
//!
//! This module adapts the file tools from `src/tools/` (ReadTool, WriteTool, etc.)
//! to work with the BuiltinToolRouter as `nika:read`, `nika:write`, etc.

use super::BuiltinTool;
use crate::error::NikaError;
use crate::tools::{EditTool, FileTool, GlobTool, GrepTool, ReadTool, ToolContext, WriteTool};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Adapter that bridges a FileTool to BuiltinTool trait.
///
/// This allows file tools to be registered in the BuiltinToolRouter
/// and called via `nika:read`, `nika:write`, etc.
pub struct FileToolAdapter<T: FileTool + Send + Sync + 'static> {
    tool: Arc<T>,
}

impl<T: FileTool + Send + Sync + 'static> FileToolAdapter<T> {
    /// Create a new adapter wrapping a FileTool.
    pub fn new(tool: T) -> Self {
        Self {
            tool: Arc::new(tool),
        }
    }

    /// Create from an existing Arc.
    pub fn from_arc(tool: Arc<T>) -> Self {
        Self { tool }
    }
}

impl<T: FileTool + Send + Sync + 'static> BuiltinTool for FileToolAdapter<T> {
    fn name(&self) -> &'static str {
        self.tool.name()
    }

    fn description(&self) -> &'static str {
        self.tool.description()
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.tool.parameters_schema()
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, NikaError>> + Send + 'a>> {
        let tool = Arc::clone(&self.tool);

        Box::pin(async move {
            // Parse JSON string to Value
            let params: serde_json::Value =
                serde_json::from_str(&args).map_err(|e| NikaError::BuiltinToolError {
                    tool: tool.name().into(),
                    reason: format!("Invalid JSON parameters: {e}"),
                })?;

            // Call the underlying FileTool
            let output = tool.call(params).await?;

            // Convert ToolOutput to JSON string
            if output.is_error {
                Err(NikaError::BuiltinToolError {
                    tool: tool.name().into(),
                    reason: output.content,
                })
            } else {
                // Return content with optional data
                if let Some(data) = output.data {
                    serde_json::to_string(&serde_json::json!({
                        "content": output.content,
                        "data": data
                    }))
                    .map_err(|e| NikaError::BuiltinToolError {
                        tool: tool.name().into(),
                        reason: format!("Failed to serialize result: {e}"),
                    })
                } else {
                    Ok(output.content)
                }
            }
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CONVENIENCE CONSTRUCTORS
// ═══════════════════════════════════════════════════════════════════════════

/// Create all 5 file tool adapters for the BuiltinToolRouter.
///
/// Returns adapters for: read, write, edit, glob, grep
pub fn create_file_tool_adapters(ctx: Arc<ToolContext>) -> Vec<Box<dyn BuiltinTool>> {
    vec![
        Box::new(FileToolAdapter::new(ReadTool::new(Arc::clone(&ctx)))),
        Box::new(FileToolAdapter::new(WriteTool::new(Arc::clone(&ctx)))),
        Box::new(FileToolAdapter::new(EditTool::new(Arc::clone(&ctx)))),
        Box::new(FileToolAdapter::new(GlobTool::new(Arc::clone(&ctx)))),
        Box::new(FileToolAdapter::new(GrepTool::new(ctx))),
    ]
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::PermissionMode;
    use tempfile::TempDir;

    async fn setup_test() -> (TempDir, Arc<ToolContext>) {
        let temp_dir = TempDir::new().unwrap();
        let ctx = Arc::new(ToolContext::new(
            temp_dir.path().to_path_buf(),
            PermissionMode::YoloMode,
        ));
        (temp_dir, ctx)
    }

    #[tokio::test]
    async fn test_read_adapter_name() {
        let (_temp, ctx) = setup_test().await;
        let adapter = FileToolAdapter::new(ReadTool::new(ctx));
        assert_eq!(adapter.name(), "read");
    }

    #[tokio::test]
    async fn test_write_adapter_name() {
        let (_temp, ctx) = setup_test().await;
        let adapter = FileToolAdapter::new(WriteTool::new(ctx));
        assert_eq!(adapter.name(), "write");
    }

    #[tokio::test]
    async fn test_edit_adapter_name() {
        let (_temp, ctx) = setup_test().await;
        let adapter = FileToolAdapter::new(EditTool::new(ctx));
        assert_eq!(adapter.name(), "edit");
    }

    #[tokio::test]
    async fn test_glob_adapter_name() {
        let (_temp, ctx) = setup_test().await;
        let adapter = FileToolAdapter::new(GlobTool::new(ctx));
        assert_eq!(adapter.name(), "glob");
    }

    #[tokio::test]
    async fn test_grep_adapter_name() {
        let (_temp, ctx) = setup_test().await;
        let adapter = FileToolAdapter::new(GrepTool::new(ctx));
        assert_eq!(adapter.name(), "grep");
    }

    #[tokio::test]
    async fn test_create_file_tool_adapters() {
        let (_temp, ctx) = setup_test().await;
        let adapters = create_file_tool_adapters(ctx);

        assert_eq!(adapters.len(), 5);

        let names: Vec<&str> = adapters.iter().map(|a| a.name()).collect();
        assert!(names.contains(&"read"));
        assert!(names.contains(&"write"));
        assert!(names.contains(&"edit"));
        assert!(names.contains(&"glob"));
        assert!(names.contains(&"grep"));
    }

    #[tokio::test]
    async fn test_write_then_read() {
        let (temp_dir, ctx) = setup_test().await;
        let file_path = temp_dir.path().join("test.txt");

        // Write file
        let write_adapter = FileToolAdapter::new(WriteTool::new(Arc::clone(&ctx)));
        let write_args = serde_json::json!({
            "file_path": file_path.to_string_lossy(),
            "content": "Hello, Nika!"
        })
        .to_string();

        let result = write_adapter.call(write_args).await;
        assert!(result.is_ok(), "Write failed: {:?}", result);

        // Read file
        let read_adapter = FileToolAdapter::new(ReadTool::new(ctx));
        let read_args = serde_json::json!({
            "file_path": file_path.to_string_lossy()
        })
        .to_string();

        let result = read_adapter.call(read_args).await;
        assert!(result.is_ok());
        let content = result.unwrap();
        assert!(content.contains("Hello, Nika!"));
    }

    #[tokio::test]
    async fn test_edit_file() {
        let (temp_dir, ctx) = setup_test().await;
        let file_path = temp_dir.path().join("edit-test.txt");

        // Write initial file
        std::fs::write(&file_path, "Hello World").unwrap();

        // Read first (required by EditTool)
        let read_adapter = FileToolAdapter::new(ReadTool::new(Arc::clone(&ctx)));
        let read_args = serde_json::json!({
            "file_path": file_path.to_string_lossy()
        })
        .to_string();
        read_adapter.call(read_args).await.unwrap();

        // Edit file
        let edit_adapter = FileToolAdapter::new(EditTool::new(ctx));
        let edit_args = serde_json::json!({
            "file_path": file_path.to_string_lossy(),
            "old_string": "World",
            "new_string": "Nika"
        })
        .to_string();

        let result = edit_adapter.call(edit_args).await;
        assert!(result.is_ok(), "Edit failed: {:?}", result);

        // Verify
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Hello Nika");
    }

    #[tokio::test]
    async fn test_glob_find_files() {
        let (temp_dir, ctx) = setup_test().await;

        // Create test files
        std::fs::write(temp_dir.path().join("file1.txt"), "content1").unwrap();
        std::fs::write(temp_dir.path().join("file2.txt"), "content2").unwrap();
        std::fs::write(temp_dir.path().join("other.md"), "markdown").unwrap();

        let glob_adapter = FileToolAdapter::new(GlobTool::new(ctx));
        let glob_args = serde_json::json!({
            "pattern": "*.txt",
            "path": temp_dir.path().to_string_lossy()
        })
        .to_string();

        let result = glob_adapter.call(glob_args).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("file1.txt"));
        assert!(output.contains("file2.txt"));
        assert!(!output.contains("other.md"));
    }

    #[tokio::test]
    async fn test_grep_search_content() {
        let (temp_dir, ctx) = setup_test().await;

        // Create test file
        std::fs::write(
            temp_dir.path().join("search.txt"),
            "Line 1: Hello\nLine 2: World\nLine 3: Hello World",
        )
        .unwrap();

        let grep_adapter = FileToolAdapter::new(GrepTool::new(ctx));
        let grep_args = serde_json::json!({
            "pattern": "Hello",
            "path": temp_dir.path().to_string_lossy()
        })
        .to_string();

        let result = grep_adapter.call(grep_args).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        // Should find matches
        assert!(output.contains("search.txt"));
    }

    #[tokio::test]
    async fn test_invalid_json_params() {
        let (_temp, ctx) = setup_test().await;
        let adapter = FileToolAdapter::new(ReadTool::new(ctx));

        let result = adapter.call("not valid json".to_string()).await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid JSON parameters"));
    }

    #[tokio::test]
    async fn test_path_outside_boundary() {
        let (_temp, ctx) = setup_test().await;
        let adapter = FileToolAdapter::new(ReadTool::new(ctx));

        // Try to read outside working directory
        let args = serde_json::json!({
            "file_path": "/etc/passwd"
        })
        .to_string();

        let result = adapter.call(args).await;
        assert!(result.is_err());
    }

    // =========================================================================
    // Additional Edge Case Tests
    // =========================================================================

    #[tokio::test]
    async fn test_read_nonexistent_file() {
        let (temp_dir, ctx) = setup_test().await;
        let adapter = FileToolAdapter::new(ReadTool::new(ctx));

        let args = serde_json::json!({
            "file_path": temp_dir.path().join("does_not_exist.txt").to_string_lossy()
        })
        .to_string();

        let result = adapter.call(args).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("read") || err.to_string().contains("File"));
    }

    #[tokio::test]
    async fn test_edit_nonexistent_file() {
        let (temp_dir, ctx) = setup_test().await;
        let adapter = FileToolAdapter::new(EditTool::new(ctx));

        let args = serde_json::json!({
            "file_path": temp_dir.path().join("nonexistent.txt").to_string_lossy(),
            "old_string": "foo",
            "new_string": "bar"
        })
        .to_string();

        let result = adapter.call(args).await;
        // Should error because file doesn't exist or wasn't read first
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_edit_old_string_not_found() {
        let (temp_dir, ctx) = setup_test().await;
        let file_path = temp_dir.path().join("edit-miss.txt");

        // Create file with known content
        std::fs::write(&file_path, "Hello World").unwrap();

        // Read first (required by EditTool)
        let read_adapter = FileToolAdapter::new(ReadTool::new(Arc::clone(&ctx)));
        let read_args = serde_json::json!({
            "file_path": file_path.to_string_lossy()
        })
        .to_string();
        read_adapter.call(read_args).await.unwrap();

        // Try to edit with non-matching old_string
        let edit_adapter = FileToolAdapter::new(EditTool::new(ctx));
        let edit_args = serde_json::json!({
            "file_path": file_path.to_string_lossy(),
            "old_string": "NONEXISTENT_STRING",
            "new_string": "replaced"
        })
        .to_string();

        let result = edit_adapter.call(edit_args).await;
        // Should error because old_string not found
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("not found")
                || err.to_string().contains("does not exist")
                || err.to_string().contains("old_string")
        );
    }

    #[tokio::test]
    async fn test_write_missing_content_param() {
        let (temp_dir, ctx) = setup_test().await;
        let adapter = FileToolAdapter::new(WriteTool::new(ctx));

        // Missing "content" field
        let args = serde_json::json!({
            "file_path": temp_dir.path().join("test.txt").to_string_lossy()
        })
        .to_string();

        let result = adapter.call(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_glob_no_matches() {
        let (temp_dir, ctx) = setup_test().await;

        // Create only .txt files
        std::fs::write(temp_dir.path().join("file.txt"), "content").unwrap();

        let adapter = FileToolAdapter::new(GlobTool::new(ctx));
        let args = serde_json::json!({
            "pattern": "*.rs",  // No .rs files exist
            "path": temp_dir.path().to_string_lossy()
        })
        .to_string();

        let result = adapter.call(args).await;
        // Should succeed but return empty or "no matches" message
        assert!(result.is_ok());
        let output = result.unwrap();
        // Either empty result or message indicating no matches
        assert!(
            output.is_empty()
                || output.contains("[]")
                || output.contains("No files")
                || !output.contains(".rs")
        );
    }

    #[tokio::test]
    async fn test_grep_no_matches() {
        let (temp_dir, ctx) = setup_test().await;

        // Create file without the search pattern
        std::fs::write(temp_dir.path().join("search.txt"), "Hello World").unwrap();

        let adapter = FileToolAdapter::new(GrepTool::new(ctx));
        let args = serde_json::json!({
            "pattern": "NONEXISTENT_PATTERN_12345",
            "path": temp_dir.path().to_string_lossy()
        })
        .to_string();

        let result = adapter.call(args).await;
        // Should succeed but return no matches
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(!output.contains("Hello"));
    }

    #[tokio::test]
    async fn test_read_missing_file_path_param() {
        let (_temp, ctx) = setup_test().await;
        let adapter = FileToolAdapter::new(ReadTool::new(ctx));

        // Missing file_path field
        let args = serde_json::json!({}).to_string();

        let result = adapter.call(args).await;
        assert!(result.is_err());
    }
}
