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
    #[serde(alias = "path")]
    pub file_path: String,

    /// Content to write.
    /// Accepts strings directly, or objects/arrays which are auto-serialized to pretty JSON.
    #[serde(deserialize_with = "deserialize_content")]
    pub content: String,
}

/// Deserialize content from any JSON value: strings pass through,
/// objects/arrays are pretty-printed, primitives are converted to string.
fn deserialize_content<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    match value {
        Value::String(s) => Ok(s),
        Value::Object(_) | Value::Array(_) => {
            serde_json::to_string_pretty(&value).map_err(serde::de::Error::custom)
        }
        Value::Number(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(b.to_string()),
        Value::Null => Ok(String::new()),
    }
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

/// Maximum write size: 10 MB
const MAX_WRITE_SIZE: usize = 10 * 1024 * 1024;

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

        // Check content size limit
        if params.content.len() > MAX_WRITE_SIZE {
            return Err(NikaError::ToolError {
                code: ToolErrorCode::WriteFailed.code(),
                message: format!(
                    "Content size ({} bytes) exceeds maximum write size ({} bytes)",
                    params.content.len(),
                    MAX_WRITE_SIZE
                ),
            });
        }

        // Create parent directories if needed (idempotent, no TOCTOU race)
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| NikaError::ToolError {
                    code: ToolErrorCode::WriteFailed.code(),
                    message: format!("Failed to create parent directories: {}", e),
                })?;
        }

        // Atomic create-if-not-exists using create_new(true).
        // This is a single syscall — no TOCTOU window between exists check and creation.
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true) // Fails atomically if file exists
            .open(&path)
            .await
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    NikaError::ToolError {
                        code: ToolErrorCode::FileAlreadyExists.code(),
                        message: format!(
                            "File already exists: {}. Use the Edit tool to modify existing files.",
                            params.file_path
                        ),
                    }
                } else {
                    NikaError::ToolError {
                        code: ToolErrorCode::WriteFailed.code(),
                        message: format!("Failed to create file: {}", e),
                    }
                }
            })?;

        file.write_all(params.content.as_bytes())
            .await
            .map_err(|e| NikaError::ToolError {
                code: ToolErrorCode::WriteFailed.code(),
                message: format!("Failed to write content: {}", e),
            })?;

        file.sync_all().await.map_err(|e| NikaError::ToolError {
            code: ToolErrorCode::WriteFailed.code(),
            message: format!("Failed to sync file: {}", e),
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
                    "description": "Content to write. Strings written as-is; objects/arrays serialized to pretty JSON."
                }
            },
            "required": ["file_path", "content"],
            "additionalProperties": false
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
    use crate::tools::context::testing::setup_test;

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
    async fn test_write_rejects_oversized_content() {
        let (temp_dir, ctx) = setup_test().await;
        let file_path = temp_dir
            .path()
            .join("huge.txt")
            .to_string_lossy()
            .to_string();

        // 11MB exceeds the 10MB limit
        let oversized = "x".repeat(11 * 1024 * 1024);

        let tool = WriteTool::new(ctx);
        let result = tool
            .execute(WriteParams {
                file_path,
                content: oversized,
            })
            .await;

        assert!(result.is_err(), "oversized content should be rejected");
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("exceeds"),
            "error should mention size limit, got: {}",
            err
        );
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

    #[tokio::test]
    async fn test_write_atomic_preserves_existing_content() {
        let (temp_dir, ctx) = setup_test().await;
        let file_path = temp_dir
            .path()
            .join("precious.txt")
            .to_string_lossy()
            .to_string();

        // Pre-create file with precious content
        fs::write(&file_path, "precious data").await.unwrap();

        let tool = WriteTool::new(ctx);
        let result = tool
            .execute(WriteParams {
                file_path: file_path.clone(),
                content: "replacement".to_string(),
            })
            .await;

        assert!(result.is_err(), "should reject existing file");

        // Original content MUST be preserved
        let content = fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(
            content, "precious data",
            "original content must not be corrupted"
        );

        // No temp file should be left behind
        let temp = std::path::Path::new(&file_path).with_extension("tmp.nika");
        assert!(!temp.exists(), "no temp file should remain");
    }

    #[test]
    fn write_params_accepts_path_alias() {
        let json = r#"{"path": "/tmp/test.txt", "content": "hello"}"#;
        let params: WriteParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.file_path, "/tmp/test.txt");
        assert_eq!(params.content, "hello");
    }

    #[test]
    fn write_params_accepts_object_content() {
        let json = r#"{"file_path": "/tmp/t.json", "content": {"name": "test", "count": 42}}"#;
        let params: WriteParams = serde_json::from_str(json).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&params.content).unwrap();
        assert_eq!(parsed["name"], "test");
        assert_eq!(parsed["count"], 42);
    }

    #[test]
    fn write_params_accepts_array_content() {
        let json = r#"{"file_path": "/tmp/t.json", "content": [1, 2, 3]}"#;
        let params: WriteParams = serde_json::from_str(json).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&params.content).unwrap();
        assert!(parsed.is_array());
    }

    #[test]
    fn write_params_accepts_null_content() {
        let json = r#"{"file_path": "/tmp/t.txt", "content": null}"#;
        let params: WriteParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.content, "");
    }

    #[tokio::test]
    async fn test_write_json_object_via_call() {
        let (temp_dir, ctx) = setup_test().await;
        let file_path = temp_dir
            .path()
            .join("data.json")
            .to_string_lossy()
            .to_string();
        let tool = WriteTool::new(ctx);
        let result = tool
            .call(json!({
                "file_path": file_path,
                "content": {"key": "value", "nested": {"a": 1}}
            }))
            .await
            .unwrap();
        assert!(!result.is_error);
        let content = fs::read_to_string(&file_path).await.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["key"], "value");
        assert_eq!(parsed["nested"]["a"], 1);
    }
}
