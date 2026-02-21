//! Edit Tool - Modify existing files
//!
//! Claude Code-like file editing with:
//! - Read-before-edit validation
//! - Unique string matching
//! - Atomic updates (temp file + rename)
//! - Diff preview

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

/// Parameters for the Edit tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditParams {
    /// Absolute path to the file to edit
    pub file_path: String,

    /// Text to find and replace
    pub old_string: String,

    /// Replacement text
    pub new_string: String,

    /// Replace all occurrences (default: false)
    #[serde(default)]
    pub replace_all: bool,
}

/// Result from editing a file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditResult {
    /// Path of the edited file
    pub path: String,

    /// Number of replacements made
    pub replacements: usize,

    /// Diff preview (unified format)
    pub diff_preview: String,
}

// ═══════════════════════════════════════════════════════════════════════════
// EDIT TOOL
// ═══════════════════════════════════════════════════════════════════════════

/// Edit tool for modifying existing files
///
/// # Features
///
/// - Read-before-edit validation (must read file first)
/// - Unique string matching (fails if old_string appears multiple times)
/// - replace_all mode for bulk replacements
/// - Atomic write (temp file + rename)
/// - Diff preview for verification
pub struct EditTool {
    ctx: Arc<ToolContext>,
}

impl EditTool {
    /// Create a new Edit tool
    pub fn new(ctx: Arc<ToolContext>) -> Self {
        Self { ctx }
    }

    /// Execute the edit operation
    pub async fn execute(&self, params: EditParams) -> Result<EditResult, NikaError> {
        // Validate path
        let path = self.ctx.validate_path(&params.file_path)?;

        // Check permission
        self.ctx.check_permission(ToolOperation::Edit)?;

        // Validate read-before-edit
        self.ctx.validate_read_before_edit(&path)?;

        // Check file exists
        if !path.exists() {
            return Err(NikaError::ToolError {
                code: ToolErrorCode::FileNotFound.code(),
                message: format!("File not found: {}", params.file_path),
            });
        }

        // Read current content
        let content = fs::read_to_string(&path)
            .await
            .map_err(|e| NikaError::ToolError {
                code: ToolErrorCode::EditFailed.code(),
                message: format!("Failed to read file: {}", e),
            })?;

        // Count occurrences
        let occurrences = content.matches(&params.old_string).count();

        if occurrences == 0 {
            return Err(NikaError::ToolError {
                code: ToolErrorCode::EditFailed.code(),
                message: "old_string not found in file. Make sure the string matches exactly, including whitespace and indentation.".to_string(),
            });
        }

        if occurrences > 1 && !params.replace_all {
            return Err(NikaError::ToolError {
                code: ToolErrorCode::OldStringNotUnique.code(),
                message: format!(
                    "old_string appears {} times in file. Use replace_all: true to replace all occurrences, \
                     or provide a more specific string that appears only once.",
                    occurrences
                ),
            });
        }

        // Perform replacement
        let new_content = if params.replace_all {
            content.replace(&params.old_string, &params.new_string)
        } else {
            content.replacen(&params.old_string, &params.new_string, 1)
        };

        let replacements = if params.replace_all { occurrences } else { 1 };

        // Generate diff preview
        let diff_preview = generate_diff(&content, &new_content, &params.file_path);

        // Atomic write: temp file + rename
        let temp_path = path.with_extension("tmp.nika.edit");

        let mut file = fs::File::create(&temp_path)
            .await
            .map_err(|e| NikaError::ToolError {
                code: ToolErrorCode::EditFailed.code(),
                message: format!("Failed to create temp file: {}", e),
            })?;

        file.write_all(new_content.as_bytes())
            .await
            .map_err(|e| NikaError::ToolError {
                code: ToolErrorCode::EditFailed.code(),
                message: format!("Failed to write content: {}", e),
            })?;

        file.flush().await.map_err(|e| NikaError::ToolError {
            code: ToolErrorCode::EditFailed.code(),
            message: format!("Failed to flush file: {}", e),
        })?;

        // Atomic rename
        fs::rename(&temp_path, &path).await.map_err(|e| {
            let _ = std::fs::remove_file(&temp_path);
            NikaError::ToolError {
                code: ToolErrorCode::EditFailed.code(),
                message: format!("Failed to finalize edit: {}", e),
            }
        })?;

        // Emit event
        self.ctx
            .emit(ToolEvent::FileEdited {
                path: params.file_path.clone(),
                replacements,
                diff_preview: diff_preview.clone(),
            })
            .await;

        Ok(EditResult {
            path: params.file_path,
            replacements,
            diff_preview,
        })
    }
}

/// Generate a simple unified diff preview
fn generate_diff(old: &str, new: &str, file_path: &str) -> String {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    let mut diff = format!("--- {}\n+++ {}\n", file_path, file_path);

    // Find changed regions (simple approach)
    let mut i = 0;
    let mut j = 0;

    while i < old_lines.len() || j < new_lines.len() {
        if i < old_lines.len() && j < new_lines.len() && old_lines[i] == new_lines[j] {
            i += 1;
            j += 1;
        } else {
            // Found a difference
            let start_i = i;
            let start_j = j;

            // Find where they converge again
            while i < old_lines.len() && !new_lines[start_j..].contains(&old_lines[i]) {
                i += 1;
            }
            while j < new_lines.len()
                && (i >= old_lines.len() || new_lines[j] != old_lines.get(i).copied().unwrap_or(""))
            {
                j += 1;
            }

            // Output the hunk
            diff.push_str(&format!(
                "@@ -{},{} +{},{} @@\n",
                start_i + 1,
                i - start_i,
                start_j + 1,
                j - start_j
            ));

            for line in &old_lines[start_i..i] {
                diff.push_str(&format!("-{}\n", line));
            }
            for line in &new_lines[start_j..j] {
                diff.push_str(&format!("+{}\n", line));
            }
        }
    }

    if diff.ends_with(&format!("--- {}\n+++ {}\n", file_path, file_path)) {
        "No changes".to_string()
    } else {
        diff
    }
}

#[async_trait]
impl FileTool for EditTool {
    fn name(&self) -> &'static str {
        "edit"
    }

    fn description(&self) -> &'static str {
        "Edit an existing file by replacing text. IMPORTANT: You must read the file first using \
         the Read tool before editing. The old_string must be unique in the file unless \
         replace_all is true. Preserves exact indentation and whitespace."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to the file to edit"
                },
                "old_string": {
                    "type": "string",
                    "description": "Exact text to find and replace (must be unique unless replace_all is true)"
                },
                "new_string": {
                    "type": "string",
                    "description": "Replacement text"
                },
                "replace_all": {
                    "type": "boolean",
                    "description": "Replace all occurrences (default: false)",
                    "default": false
                }
            },
            "required": ["file_path", "old_string", "new_string"]
        })
    }

    async fn call(&self, params: Value) -> Result<ToolOutput, NikaError> {
        let params: EditParams =
            serde_json::from_value(params).map_err(|e| NikaError::ToolError {
                code: ToolErrorCode::EditFailed.code(),
                message: format!("Invalid parameters: {}", e),
            })?;

        let result = self.execute(params).await?;

        Ok(ToolOutput::success_with_data(
            format!(
                "Edited file: {} ({} replacement{})\n\n{}",
                result.path,
                result.replacements,
                if result.replacements == 1 { "" } else { "s" },
                result.diff_preview
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

    async fn create_and_read_file(
        temp_dir: &TempDir,
        ctx: &Arc<ToolContext>,
        name: &str,
        content: &str,
    ) -> String {
        let path = temp_dir.path().join(name);
        fs::write(&path, content).await.unwrap();

        // Mark as read (simulating Read tool usage)
        ctx.mark_as_read(&path);

        path.to_string_lossy().to_string()
    }

    #[tokio::test]
    async fn test_edit_simple_replacement() {
        let (temp_dir, ctx) = setup_test().await;
        let file_path = create_and_read_file(&temp_dir, &ctx, "test.txt", "Hello, World!").await;

        let tool = EditTool::new(ctx);
        let result = tool
            .execute(EditParams {
                file_path: file_path.clone(),
                old_string: "World".to_string(),
                new_string: "Rust".to_string(),
                replace_all: false,
            })
            .await
            .unwrap();

        assert_eq!(result.replacements, 1);

        let content = fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(content, "Hello, Rust!");
    }

    #[tokio::test]
    async fn test_edit_replace_all() {
        let (temp_dir, ctx) = setup_test().await;
        let file_path =
            create_and_read_file(&temp_dir, &ctx, "test.txt", "foo bar foo baz foo").await;

        let tool = EditTool::new(ctx);
        let result = tool
            .execute(EditParams {
                file_path: file_path.clone(),
                old_string: "foo".to_string(),
                new_string: "qux".to_string(),
                replace_all: true,
            })
            .await
            .unwrap();

        assert_eq!(result.replacements, 3);

        let content = fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(content, "qux bar qux baz qux");
    }

    #[tokio::test]
    async fn test_edit_fails_without_read() {
        let (temp_dir, ctx) = setup_test().await;
        let file_path = temp_dir
            .path()
            .join("test.txt")
            .to_string_lossy()
            .to_string();
        fs::write(&file_path, "content").await.unwrap();

        // Don't mark as read

        let tool = EditTool::new(ctx);
        let result = tool
            .execute(EditParams {
                file_path,
                old_string: "content".to_string(),
                new_string: "new".to_string(),
                replace_all: false,
            })
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Must read file"));
    }

    #[tokio::test]
    async fn test_edit_fails_not_unique() {
        let (temp_dir, ctx) = setup_test().await;
        let file_path = create_and_read_file(&temp_dir, &ctx, "test.txt", "foo foo foo").await;

        let tool = EditTool::new(ctx);
        let result = tool
            .execute(EditParams {
                file_path,
                old_string: "foo".to_string(),
                new_string: "bar".to_string(),
                replace_all: false,
            })
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("3 times"));
    }

    #[tokio::test]
    async fn test_edit_not_found() {
        let (temp_dir, ctx) = setup_test().await;
        let file_path = create_and_read_file(&temp_dir, &ctx, "test.txt", "Hello World").await;

        let tool = EditTool::new(ctx);
        let result = tool
            .execute(EditParams {
                file_path,
                old_string: "Goodbye".to_string(),
                new_string: "Hi".to_string(),
                replace_all: false,
            })
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_edit_preserves_whitespace() {
        let (temp_dir, ctx) = setup_test().await;
        let file_path = create_and_read_file(
            &temp_dir,
            &ctx,
            "test.txt",
            "fn main() {\n    let x = 1;\n}",
        )
        .await;

        let tool = EditTool::new(ctx);
        let result = tool
            .execute(EditParams {
                file_path: file_path.clone(),
                old_string: "    let x = 1;".to_string(),
                new_string: "    let x = 42;".to_string(),
                replace_all: false,
            })
            .await
            .unwrap();

        assert_eq!(result.replacements, 1);

        let content = fs::read_to_string(&file_path).await.unwrap();
        assert!(content.contains("    let x = 42;"));
    }

    #[tokio::test]
    async fn test_edit_permission_accept_edits() {
        let (temp_dir, _) = setup_test().await;
        let ctx = Arc::new(ToolContext::new(
            temp_dir.path().to_path_buf(),
            super::super::context::PermissionMode::AcceptEdits,
        ));
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "content").await.unwrap();
        ctx.mark_as_read(&file_path);

        let tool = EditTool::new(ctx);
        let result = tool
            .execute(EditParams {
                file_path: file_path.to_string_lossy().to_string(),
                old_string: "content".to_string(),
                new_string: "new".to_string(),
                replace_all: false,
            })
            .await;

        // AcceptEdits mode allows edits
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_file_tool_trait() {
        let (temp_dir, ctx) = setup_test().await;
        let file_path = create_and_read_file(&temp_dir, &ctx, "test.txt", "hello").await;

        let tool = EditTool::new(ctx);

        assert_eq!(tool.name(), "edit");
        assert!(tool.description().contains("Edit"));
        assert!(tool.description().contains("read the file first"));

        let result = tool
            .call(json!({
                "file_path": file_path,
                "old_string": "hello",
                "new_string": "world"
            }))
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("Edited file"));
    }
}
