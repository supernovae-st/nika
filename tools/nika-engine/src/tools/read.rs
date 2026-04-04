//! Read Tool - Read files with line numbers
//!
//! Provides Claude Code-like file reading with:
//! - Line number prefix (cat -n style)
//! - Offset and limit for large files
//! - Automatic read tracking for edit validation

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::fs;

use super::context::{ToolContext, ToolEvent};
use super::{FileTool, ToolErrorCode, ToolOutput};
use crate::error::NikaError;

// ═══════════════════════════════════════════════════════════════════════════
// PARAMETERS & RESULT
// ═══════════════════════════════════════════════════════════════════════════

/// Parameters for the Read tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadParams {
    /// Absolute path to the file to read
    pub file_path: String,

    /// Line offset (1-indexed, skip first N-1 lines)
    #[serde(default)]
    pub offset: Option<usize>,

    /// Maximum lines to read (default: 2000)
    #[serde(default)]
    pub limit: Option<usize>,

    /// When true, return raw content without line numbers.
    /// Saves ~15% tokens when feeding file content to LLM prompts.
    #[serde(default)]
    pub raw: Option<bool>,

    /// When true, missing files return fallback instead of error.
    #[serde(default)]
    pub optional: Option<bool>,

    /// Value to return when file is missing and optional=true (default: "").
    #[serde(default)]
    pub fallback: Option<String>,
}

/// Result from reading a file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadResult {
    /// File content with line numbers
    pub content: String,

    /// Total lines in file
    pub total_lines: usize,

    /// Lines actually returned
    pub lines_returned: usize,

    /// Whether output was truncated
    pub truncated: bool,
}

// ═══════════════════════════════════════════════════════════════════════════
// READ TOOL
// ═══════════════════════════════════════════════════════════════════════════

/// Read tool for reading files with line numbers
///
/// # Features
///
/// - Line numbers in output (cat -n style)
/// - Offset/limit for large files
/// - Tracks reads for edit validation
/// - Max line length truncation (2000 chars)
pub struct ReadTool {
    ctx: Arc<ToolContext>,
}

impl ReadTool {
    /// Default maximum lines to read
    pub const DEFAULT_LIMIT: usize = 2000;

    /// Maximum characters per line before truncation
    pub const MAX_LINE_LENGTH: usize = 2000;

    /// Create a new Read tool
    pub fn new(ctx: Arc<ToolContext>) -> Self {
        Self { ctx }
    }

    /// Execute the read operation
    pub async fn execute(&self, params: ReadParams) -> Result<ReadResult, NikaError> {
        // Validate path
        let path = self.ctx.validate_path(&params.file_path)?;

        // Check permission (reads are usually allowed, but respect Deny mode)
        if self.ctx.permission_mode() == super::context::PermissionMode::Deny {
            return Err(NikaError::ToolError {
                code: ToolErrorCode::PermissionDenied.code(),
                message: "Read operations are denied in current permission mode".to_string(),
            });
        }

        let is_optional = params.optional.unwrap_or(false);

        // SEC-4: Pre-check file size to prevent OOM from huge files
        const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024; // 50 MB
        if let Ok(metadata) = tokio::fs::metadata(&path).await {
            if metadata.len() > MAX_FILE_SIZE {
                return Err(NikaError::ToolError {
                    code: ToolErrorCode::ReadFailed.code(),
                    message: format!(
                        "File too large: {} bytes (max {} MB). Use offset/limit to read portions.",
                        metadata.len(),
                        MAX_FILE_SIZE / (1024 * 1024)
                    ),
                });
            }
        }

        // Check file exists
        if !path.exists() {
            if is_optional {
                let fallback_val = params.fallback.clone().unwrap_or_default();
                return Ok(ReadResult {
                    content: fallback_val,
                    total_lines: 0,
                    lines_returned: 0,
                    truncated: false,
                });
            }
            return Err(NikaError::ToolError {
                code: ToolErrorCode::FileNotFound.code(),
                message: format!("File not found: {}", params.file_path),
            });
        }

        // Read file content
        let content = match fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(_) if is_optional => {
                let fallback_val = params.fallback.clone().unwrap_or_default();
                return Ok(ReadResult {
                    content: fallback_val,
                    total_lines: 0,
                    lines_returned: 0,
                    truncated: false,
                });
            }
            Err(e) => {
                return Err(NikaError::ToolError {
                    code: ToolErrorCode::ReadFailed.code(),
                    message: format!("Failed to read file: {}", e),
                });
            }
        };

        // Process lines
        let all_lines: Vec<&str> = content.lines().collect();
        let total_lines = all_lines.len();

        // Apply offset (1-indexed)
        let offset = params.offset.unwrap_or(1).saturating_sub(1);
        let limit = params.limit.unwrap_or(Self::DEFAULT_LIMIT);

        let selected_lines: Vec<&str> = all_lines.into_iter().skip(offset).take(limit).collect();

        let lines_returned = selected_lines.len();
        let truncated = offset + lines_returned < total_lines;

        let is_raw = params.raw.unwrap_or(false);

        // Format output: raw mode returns plain text, default adds line numbers
        let formatted = if is_raw {
            selected_lines.join("\n")
        } else {
            // cat -n style: "    N\tline content"
            selected_lines
                .iter()
                .enumerate()
                .map(|(i, line)| {
                    let line_num = offset + i + 1;
                    let truncated_line = if line.len() > Self::MAX_LINE_LENGTH {
                        let mut end = Self::MAX_LINE_LENGTH;
                        while end > 0 && !line.is_char_boundary(end) {
                            end -= 1;
                        }
                        format!("{}...", &line[..end])
                    } else {
                        line.to_string()
                    };
                    format!("{:>6}\t{}", line_num, truncated_line)
                })
                .collect::<Vec<_>>()
                .join("\n")
        };

        // Mark as read for edit validation
        self.ctx.mark_as_read(&path);

        // Emit event
        self.ctx
            .emit(ToolEvent::FileRead {
                path: params.file_path,
                lines: lines_returned,
                truncated,
            })
            .await;

        Ok(ReadResult {
            content: formatted,
            total_lines,
            lines_returned,
            truncated,
        })
    }
}

#[async_trait]
impl FileTool for ReadTool {
    fn name(&self) -> &'static str {
        "read"
    }

    fn description(&self) -> &'static str {
        "Read a file from the filesystem. Returns content with line numbers. \
         Use offset and limit for large files. Must use absolute paths within \
         the working directory."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to the file to read"
                },
                "offset": {
                    "type": "integer",
                    "description": "Line number to start reading from (1-indexed)",
                    "minimum": 1
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of lines to read (default: 2000)",
                    "minimum": 1,
                    "maximum": 10000
                },
                "raw": {
                    "type": "boolean",
                    "description": "Return raw content without line numbers (default: false)"
                },
                "optional": {
                    "type": "boolean",
                    "description": "Return fallback instead of error when file is missing (default: false)"
                },
                "fallback": {
                    "type": "string",
                    "description": "Value to return when file is missing and optional=true (default: empty string)"
                }
            },
            "required": ["file_path"],
            "additionalProperties": false
        })
    }

    async fn call(&self, params: Value) -> Result<ToolOutput, NikaError> {
        let params: ReadParams =
            serde_json::from_value(params).map_err(|e| NikaError::ToolError {
                code: ToolErrorCode::ReadFailed.code(),
                message: format!("Invalid parameters: {}", e),
            })?;

        let is_raw = params.raw.unwrap_or(false);
        let result = self.execute(params).await?;

        if is_raw {
            Ok(ToolOutput::success(result.content))
        } else {
            Ok(ToolOutput::success_with_data(
                result.content.clone(),
                serde_json::to_value(&result).unwrap_or_default(),
            ))
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::context::testing::{create_test_file, setup_test};

    #[tokio::test]
    async fn test_read_simple_file() {
        let (temp_dir, ctx) = setup_test().await;
        let path = create_test_file(&temp_dir, "test.txt", "line 1\nline 2\nline 3").await;
        let file_path = path.to_string_lossy().to_string();

        let tool = ReadTool::new(ctx);
        let result = tool
            .execute(ReadParams {
                file_path,
                offset: None,
                limit: None,
                raw: None,
                optional: None,
                fallback: None,
            })
            .await
            .unwrap();

        assert_eq!(result.total_lines, 3);
        assert_eq!(result.lines_returned, 3);
        assert!(!result.truncated);
        assert!(result.content.contains("line 1"));
        assert!(result.content.contains("line 3"));
    }

    #[tokio::test]
    async fn test_read_with_offset() {
        let (temp_dir, ctx) = setup_test().await;
        let content = (1..=10)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let path = create_test_file(&temp_dir, "test.txt", &content).await;
        let file_path = path.to_string_lossy().to_string();

        let tool = ReadTool::new(ctx);
        let result = tool
            .execute(ReadParams {
                file_path,
                offset: Some(5),
                limit: Some(3),
                raw: None,
                optional: None,
                fallback: None,
            })
            .await
            .unwrap();

        assert_eq!(result.total_lines, 10);
        assert_eq!(result.lines_returned, 3);
        assert!(result.truncated);
        assert!(result.content.contains("line 5"));
        assert!(result.content.contains("line 7"));
        assert!(!result.content.contains("line 4"));
    }

    #[tokio::test]
    async fn test_read_line_numbers_format() {
        let (temp_dir, ctx) = setup_test().await;
        let path = create_test_file(&temp_dir, "test.txt", "hello\nworld").await;
        let file_path = path.to_string_lossy().to_string();

        let tool = ReadTool::new(ctx);
        let result = tool
            .execute(ReadParams {
                file_path,
                offset: None,
                limit: None,
                raw: None,
                optional: None,
                fallback: None,
            })
            .await
            .unwrap();

        // Check line number format (right-aligned with tab)
        assert!(result.content.contains("     1\thello"));
        assert!(result.content.contains("     2\tworld"));
    }

    #[tokio::test]
    async fn test_read_marks_file_as_read() {
        let (temp_dir, ctx) = setup_test().await;
        let path = create_test_file(&temp_dir, "test.txt", "content").await;
        let file_path = path.to_string_lossy().to_string();

        assert!(!ctx.was_read(&path));

        let tool = ReadTool::new(ctx.clone());
        tool.execute(ReadParams {
            file_path,
            offset: None,
            limit: None,
            raw: None,
            optional: None,
            fallback: None,
        })
        .await
        .unwrap();

        assert!(ctx.was_read(&path));
    }

    #[tokio::test]
    async fn test_read_file_not_found() {
        let (temp_dir, ctx) = setup_test().await;
        let file_path = temp_dir
            .path()
            .join("nonexistent.txt")
            .to_string_lossy()
            .to_string();

        let tool = ReadTool::new(ctx);
        let result = tool
            .execute(ReadParams {
                file_path,
                offset: None,
                limit: None,
                raw: None,
                optional: None,
                fallback: None,
            })
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_read_optional_missing_file() {
        let (temp_dir, ctx) = setup_test().await;
        let file_path = temp_dir
            .path()
            .join("missing.txt")
            .to_string_lossy()
            .to_string();

        let tool = ReadTool::new(ctx);
        let result = tool
            .execute(ReadParams {
                file_path,
                offset: None,
                limit: None,
                raw: None,
                optional: Some(true),
                fallback: Some("default_value".to_string()),
            })
            .await
            .unwrap();

        assert_eq!(result.content, "default_value");
        assert_eq!(result.total_lines, 0);
        assert_eq!(result.lines_returned, 0);
    }

    #[tokio::test]
    async fn test_read_optional_without_fallback() {
        let (temp_dir, ctx) = setup_test().await;
        let file_path = temp_dir
            .path()
            .join("missing.txt")
            .to_string_lossy()
            .to_string();

        let tool = ReadTool::new(ctx);
        let result = tool
            .execute(ReadParams {
                file_path,
                offset: None,
                limit: None,
                raw: None,
                optional: Some(true),
                fallback: None,
            })
            .await
            .unwrap();

        assert_eq!(result.content, "");
    }

    #[tokio::test]
    async fn test_read_optional_existing_file_reads_normally() {
        let (temp_dir, ctx) = setup_test().await;
        let path = create_test_file(&temp_dir, "exists.txt", "real content").await;
        let file_path = path.to_string_lossy().to_string();

        let tool = ReadTool::new(ctx);
        let result = tool
            .execute(ReadParams {
                file_path,
                offset: None,
                limit: None,
                raw: Some(true),
                optional: Some(true),
                fallback: Some("fallback".to_string()),
            })
            .await
            .unwrap();

        assert!(result.content.contains("real content"));
        assert_eq!(result.total_lines, 1);
    }

    #[tokio::test]
    async fn test_read_outside_working_dir() {
        let (_temp_dir, ctx) = setup_test().await;

        let tool = ReadTool::new(ctx);
        let result = tool
            .execute(ReadParams {
                file_path: "/etc/passwd".to_string(),
                offset: None,
                limit: None,
                raw: None,
                optional: None,
                fallback: None,
            })
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("outside"));
    }

    #[tokio::test]
    async fn test_read_relative_path_resolved() {
        let (temp_dir, ctx) = setup_test().await;

        // Create a file at a relative path within the working dir
        let file_path = temp_dir.path().join("relative.txt");
        tokio::fs::write(&file_path, "relative content")
            .await
            .unwrap();

        let tool = ReadTool::new(ctx);
        let result = tool
            .execute(ReadParams {
                file_path: "relative.txt".to_string(),
                offset: None,
                limit: None,
                raw: None,
                optional: None,
                fallback: None,
            })
            .await;

        assert!(
            result.is_ok(),
            "relative path should resolve: {:?}",
            result.err()
        );
        assert!(result.unwrap().content.contains("relative content"));
    }

    #[tokio::test]
    async fn test_file_tool_trait() {
        let (temp_dir, ctx) = setup_test().await;
        let path = create_test_file(&temp_dir, "test.txt", "content").await;
        let file_path = path.to_string_lossy().to_string();

        let tool = ReadTool::new(ctx);

        assert_eq!(tool.name(), "read");
        assert!(tool.description().contains("Read a file"));

        let schema = tool.parameters_schema();
        assert!(schema["properties"]["file_path"].is_object());

        let result = tool.call(json!({ "file_path": file_path })).await.unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("content"));
    }

    #[tokio::test]
    async fn sec4_rejects_file_exceeding_50mb() {
        let (temp_dir, ctx) = setup_test().await;
        // Create a file slightly over 50MB
        let large_content = "x".repeat(51 * 1024 * 1024);
        let path = create_test_file(&temp_dir, "huge.txt", &large_content).await;
        let file_path = path.to_string_lossy().to_string();

        let tool = ReadTool::new(ctx);
        let result = tool
            .execute(ReadParams {
                file_path,
                offset: None,
                limit: None,
                raw: None,
                optional: None,
                fallback: None,
            })
            .await;

        assert!(result.is_err(), "Should reject files > 50MB");
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("File too large"),
            "Error should mention file too large, got: {err}"
        );
    }
}
