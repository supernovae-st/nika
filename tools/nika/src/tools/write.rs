//! Write Tool - Create new files
//!
//! Atomic file creation with:
//! - Permission checking
//! - Fail if file exists (use Edit for modifications)
//! - Temp file + rename pattern for safety

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::fs;
use tokio::io::AsyncWriteExt;

use super::context::{ToolContext, ToolEvent, ToolOperation};
use super::{FileTool, ToolErrorCode, ToolOutput};
use crate::error::NikaError;

// ═══════════════════════════════════════════════════════════════════════════
// PARAMETERS & RESULT
// ═══════════════════════════════════════════════════════════════════════════

/// Parameters for the Write tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteParams {
    /// Absolute path for the new file
    pub file_path: String,

    /// Content to write
    pub content: String,
}

/// Result from writing a file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteResult {
    /// Path of the created file
    pub path: String,

    /// Bytes written
    pub bytes_written: usize,

    /// Lines written
    pub lines_written: usize,
}

// ═══════════════════════════════════════════════════════════════════════════
// WRITE TOOL
// ═══════════════════════════════════════════════════════════════════════════

/// Write tool for creating new files
///
/// # Features
///
/// - Atomic write (temp file + rename)
/// - Fails if file already exists
/// - Creates parent directories if needed
/// - Permission checking
pub struct WriteTool {
    ctx: Arc<ToolContext>,
}

impl WriteTool {
    /// Create a new Write tool
    pub fn new(ctx: Arc<ToolContext>) -> Self {
        Self { ctx }
    }

    /// Execute the write operation
    pub async fn execute(&self, params: WriteParams) -> Result<WriteResult, NikaError> {
        // Validate path
        let path = self.ctx.validate_path(&params.file_path)?;

        // Check permission
        self.ctx.check_permission(ToolOperation::Write)?;

        // Fail if file already exists (use Edit for modifications)
        if path.exists() {
            return Err(NikaError::ToolError {
                code: ToolErrorCode::FileAlreadyExists.code(),
                message: format!(
                    "File already exists: {}. Use the Edit tool to modify existing files.",
                    params.file_path
                ),
            });
        }

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)
                    .await
                    .map_err(|e| NikaError::ToolError {
                        code: ToolErrorCode::WriteFailed.code(),
                        message: format!("Failed to create parent directories: {}", e),
                    })?;
            }
        }

        // Atomic write: temp file + rename
        let temp_path = path.with_extension("tmp.nika");

        // Write to temp file
        let mut file = fs::File::create(&temp_path)
            .await
            .map_err(|e| NikaError::ToolError {
                code: ToolErrorCode::WriteFailed.code(),
                message: format!("Failed to create temp file: {}", e),
            })?;

        file.write_all(params.content.as_bytes())
            .await
            .map_err(|e| NikaError::ToolError {
                code: ToolErrorCode::WriteFailed.code(),
                message: format!("Failed to write content: {}", e),
            })?;

        file.flush().await.map_err(|e| NikaError::ToolError {
            code: ToolErrorCode::WriteFailed.code(),
            message: format!("Failed to flush file: {}", e),
        })?;

        // Atomic rename
        fs::rename(&temp_path, &path).await.map_err(|e| {
            // Clean up temp file on error
            let _ = std::fs::remove_file(&temp_path);
            NikaError::ToolError {
                code: ToolErrorCode::WriteFailed.code(),
                message: format!("Failed to finalize file: {}", e),
            }
        })?;

        let bytes_written = params.content.len();
        let lines_written = params.content.lines().count();

        // Emit event
        self.ctx
            .emit(ToolEvent::FileWritten {
                path: params.file_path.clone(),
                bytes: bytes_written,
            })
            .await;

        Ok(WriteResult {
            path: params.file_path,
            bytes_written,
            lines_written,
        })
    }
}

#[async_trait]
impl FileTool for WriteTool {
    fn name(&self) -> &'static str {
        "write"
    }

    fn description(&self) -> &'static str {
        "Create a new file with the specified content. Fails if the file already exists \
         (use Edit for modifications). Creates parent directories if needed. \
         Must use absolute paths within the working directory."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path for the new file"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write to the file"
                }
            },
            "required": ["file_path", "content"]
        })
    }

    async fn call(&self, params: Value) -> Result<ToolOutput, NikaError> {
        let params: WriteParams =
            serde_json::from_value(params).map_err(|e| NikaError::ToolError {
                code: ToolErrorCode::WriteFailed.code(),
                message: format!("Invalid parameters: {}", e),
            })?;

        let result = self.execute(params).await?;

        Ok(ToolOutput::success_with_data(
            format!(
                "Created file: {} ({} bytes, {} lines)",
                result.path, result.bytes_written, result.lines_written
            ),
            serde_json::to_value(&result).unwrap_or_default(),
        ))
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn setup_test() -> (TempDir, Arc<ToolContext>) {
        let temp_dir = TempDir::new().unwrap();
        let ctx = Arc::new(ToolContext::new(
            temp_dir.path().to_path_buf(),
            super::super::context::PermissionMode::YoloMode,
        ));
        (temp_dir, ctx)
    }

    #[tokio::test]
    async fn test_write_new_file() {
        let (temp_dir, ctx) = setup_test().await;
        let file_path = temp_dir
            .path()
            .join("new_file.txt")
            .to_string_lossy()
            .to_string();

        let tool = WriteTool::new(ctx);
        let result = tool
            .execute(WriteParams {
                file_path: file_path.clone(),
                content: "Hello, World!\nLine 2".to_string(),
            })
            .await
            .unwrap();

        assert_eq!(result.bytes_written, 20);
        assert_eq!(result.lines_written, 2);

        // Verify file was created
        let content = fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(content, "Hello, World!\nLine 2");
    }

    #[tokio::test]
    async fn test_write_creates_parent_dirs() {
        let (temp_dir, ctx) = setup_test().await;
        let file_path = temp_dir
            .path()
            .join("nested/deep/dir/file.txt")
            .to_string_lossy()
            .to_string();

        let tool = WriteTool::new(ctx);
        let result = tool
            .execute(WriteParams {
                file_path: file_path.clone(),
                content: "content".to_string(),
            })
            .await;

        assert!(result.is_ok());
        assert!(std::path::Path::new(&file_path).exists());
    }

    #[tokio::test]
    async fn test_write_fails_if_exists() {
        let (temp_dir, ctx) = setup_test().await;
        let file_path = temp_dir
            .path()
            .join("existing.txt")
            .to_string_lossy()
            .to_string();

        // Create the file first
        fs::write(&file_path, "existing content").await.unwrap();

        let tool = WriteTool::new(ctx);
        let result = tool
            .execute(WriteParams {
                file_path,
                content: "new content".to_string(),
            })
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[tokio::test]
    async fn test_write_permission_denied() {
        let (temp_dir, _) = setup_test().await;
        let ctx = Arc::new(ToolContext::new(
            temp_dir.path().to_path_buf(),
            super::super::context::PermissionMode::Plan,
        ));
        let file_path = temp_dir
            .path()
            .join("test.txt")
            .to_string_lossy()
            .to_string();

        let tool = WriteTool::new(ctx);
        let result = tool
            .execute(WriteParams {
                file_path,
                content: "content".to_string(),
            })
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Permission"));
    }

    #[tokio::test]
    async fn test_write_outside_working_dir() {
        let (_temp_dir, ctx) = setup_test().await;

        let tool = WriteTool::new(ctx);
        let result = tool
            .execute(WriteParams {
                file_path: "/tmp/outside.txt".to_string(),
                content: "content".to_string(),
            })
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("outside"));
    }

    #[tokio::test]
    async fn test_file_tool_trait() {
        let (temp_dir, ctx) = setup_test().await;
        let file_path = temp_dir
            .path()
            .join("test.txt")
            .to_string_lossy()
            .to_string();

        let tool = WriteTool::new(ctx);

        assert_eq!(tool.name(), "write");
        assert!(tool.description().contains("Create a new file"));

        let result = tool
            .call(json!({
                "file_path": file_path,
                "content": "test content"
            }))
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("Created file"));
    }
}
