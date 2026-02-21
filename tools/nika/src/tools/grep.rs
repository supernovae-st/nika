//! Grep Tool - Search file contents with regex
//!
//! Fast content search with:
//! - Full regex support
//! - Multiple output modes (content, files, count)
//! - Context lines (-B/-A/-C)
//! - File type filtering

use std::sync::Arc;

use async_trait::async_trait;
use ignore::WalkBuilder;
use regex::RegexBuilder;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::fs;

use super::context::{ToolContext, ToolEvent};
use super::{FileTool, ToolErrorCode, ToolOutput};
use crate::error::NikaError;

// ═══════════════════════════════════════════════════════════════════════════
// PARAMETERS & RESULT
// ═══════════════════════════════════════════════════════════════════════════

/// Output mode for grep results
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrepOutputMode {
    /// Show matching lines with content
    Content,
    /// Show only file paths (default)
    #[default]
    FilesWithMatches,
    /// Show match count per file
    Count,
}

/// Parameters for the Grep tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrepParams {
    /// Regex pattern to search for
    pub pattern: String,

    /// Base path to search in (default: working directory)
    #[serde(default)]
    pub path: Option<String>,

    /// Glob pattern to filter files (e.g., "*.rs")
    #[serde(default)]
    pub glob: Option<String>,

    /// Output mode
    #[serde(default)]
    pub output_mode: GrepOutputMode,

    /// Case-insensitive search
    #[serde(default)]
    pub case_insensitive: bool,

    /// Lines of context before match
    #[serde(default, rename = "context_before")]
    pub context_before: Option<usize>,

    /// Lines of context after match
    #[serde(default, rename = "context_after")]
    pub context_after: Option<usize>,

    /// Lines of context before and after
    #[serde(default, rename = "context")]
    pub context: Option<usize>,

    /// Limit results
    #[serde(default)]
    pub limit: Option<usize>,
}

/// A single match in a file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrepMatch {
    /// File path
    pub file: String,
    /// Line number (1-indexed)
    pub line_number: usize,
    /// Line content
    pub content: String,
    /// Context lines before (if requested)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub context_before: Vec<String>,
    /// Context lines after (if requested)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub context_after: Vec<String>,
}

/// Result from grep search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrepResult {
    /// Total matches found
    pub total_matches: usize,

    /// Files searched
    pub files_searched: usize,

    /// Files with matches
    pub files_with_matches: usize,

    /// Matches (for Content mode) or file paths (for FilesWithMatches mode)
    pub matches: Vec<GrepMatch>,

    /// Match counts per file (for Count mode)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub counts: Vec<(String, usize)>,
}

// ═══════════════════════════════════════════════════════════════════════════
// GREP TOOL
// ═══════════════════════════════════════════════════════════════════════════

/// Grep tool for searching file contents
///
/// # Features
///
/// - Full regex support via `regex` crate
/// - Multiple output modes
/// - Context lines for surrounding content
/// - File type filtering via glob
/// - Respects .gitignore
pub struct GrepTool {
    ctx: Arc<ToolContext>,
}

impl GrepTool {
    /// Maximum files to search
    pub const MAX_FILES: usize = 10000;

    /// Maximum matches to return
    pub const MAX_MATCHES: usize = 1000;

    /// Create a new Grep tool
    pub fn new(ctx: Arc<ToolContext>) -> Self {
        Self { ctx }
    }

    /// Execute the grep search
    pub async fn execute(&self, params: GrepParams) -> Result<GrepResult, NikaError> {
        // Determine base path
        let base_path = match params.path {
            Some(ref p) => self.ctx.validate_path(p)?,
            None => self.ctx.working_dir().to_path_buf(),
        };

        // Build regex
        let regex = RegexBuilder::new(&params.pattern)
            .case_insensitive(params.case_insensitive)
            .multi_line(true)
            .build()
            .map_err(|e| NikaError::ToolError {
                code: ToolErrorCode::InvalidRegex.code(),
                message: format!("Invalid regex pattern '{}': {}", params.pattern, e),
            })?;

        // Build glob filter if provided
        let glob_filter = if let Some(ref glob_pattern) = params.glob {
            Some(
                globset::GlobBuilder::new(glob_pattern)
                    .literal_separator(true)
                    .build()
                    .map_err(|e| NikaError::ToolError {
                        code: ToolErrorCode::InvalidGlobPattern.code(),
                        message: format!("Invalid glob pattern '{}': {}", glob_pattern, e),
                    })?
                    .compile_matcher(),
            )
        } else {
            None
        };

        // Context lines
        let context_before = params.context_before.or(params.context).unwrap_or(0);
        let context_after = params.context_after.or(params.context).unwrap_or(0);

        // Walk files and search
        let mut matches: Vec<GrepMatch> = Vec::new();
        let mut counts: Vec<(String, usize)> = Vec::new();
        let mut files_searched = 0;
        let mut files_with_matches = 0;
        let mut total_matches = 0;

        let limit = params.limit.unwrap_or(Self::MAX_MATCHES);

        let walker = WalkBuilder::new(&base_path)
            .hidden(false)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .build();

        for entry in walker.filter_map(Result::ok) {
            let path = entry.path();

            // Skip directories
            if path.is_dir() {
                continue;
            }

            // Apply glob filter
            if let Some(ref glob) = glob_filter {
                let relative = path.strip_prefix(&base_path).unwrap_or(path);
                if !glob.is_match(relative) && !glob.is_match(path) {
                    continue;
                }
            }

            // Read file
            let content = match fs::read_to_string(path).await {
                Ok(c) => c,
                Err(_) => continue, // Skip unreadable files
            };

            files_searched += 1;

            if files_searched > Self::MAX_FILES {
                break;
            }

            // Search for matches
            let lines: Vec<&str> = content.lines().collect();
            let mut file_matches = 0;

            for (line_idx, line) in lines.iter().enumerate() {
                if regex.is_match(line) {
                    file_matches += 1;
                    total_matches += 1;

                    if total_matches > limit {
                        continue; // Still count but don't store
                    }

                    // Build context
                    let ctx_before: Vec<String> = if context_before > 0 {
                        let start = line_idx.saturating_sub(context_before);
                        lines[start..line_idx]
                            .iter()
                            .map(|s| s.to_string())
                            .collect()
                    } else {
                        Vec::new()
                    };

                    let ctx_after: Vec<String> = if context_after > 0 {
                        let end = (line_idx + 1 + context_after).min(lines.len());
                        lines[line_idx + 1..end]
                            .iter()
                            .map(|s| s.to_string())
                            .collect()
                    } else {
                        Vec::new()
                    };

                    matches.push(GrepMatch {
                        file: path.to_string_lossy().to_string(),
                        line_number: line_idx + 1,
                        content: line.to_string(),
                        context_before: ctx_before,
                        context_after: ctx_after,
                    });
                }
            }

            if file_matches > 0 {
                files_with_matches += 1;
                counts.push((path.to_string_lossy().to_string(), file_matches));
            }
        }

        // Emit event
        self.ctx
            .emit(ToolEvent::GrepSearch {
                pattern: params.pattern,
                files_searched,
                matches: total_matches,
            })
            .await;

        Ok(GrepResult {
            total_matches,
            files_searched,
            files_with_matches,
            matches,
            counts,
        })
    }

    /// Format output based on mode
    fn format_output(&self, result: &GrepResult, mode: GrepOutputMode) -> String {
        match mode {
            GrepOutputMode::Content => {
                if result.matches.is_empty() {
                    return "No matches found".to_string();
                }

                result
                    .matches
                    .iter()
                    .map(|m| {
                        let mut output = String::new();

                        // Context before
                        for (i, ctx) in m.context_before.iter().enumerate() {
                            let line_num = m.line_number - m.context_before.len() + i;
                            output.push_str(&format!("{}:{}: {}\n", m.file, line_num, ctx));
                        }

                        // Matching line
                        output.push_str(&format!("{}:{}> {}\n", m.file, m.line_number, m.content));

                        // Context after
                        for (i, ctx) in m.context_after.iter().enumerate() {
                            let line_num = m.line_number + 1 + i;
                            output.push_str(&format!("{}:{}: {}\n", m.file, line_num, ctx));
                        }

                        output
                    })
                    .collect::<Vec<_>>()
                    .join("--\n")
            }
            GrepOutputMode::FilesWithMatches => {
                if result.files_with_matches == 0 {
                    return "No matching files found".to_string();
                }

                // Deduplicate file paths
                let mut files: Vec<&str> = result.matches.iter().map(|m| m.file.as_str()).collect();
                files.sort();
                files.dedup();

                format!("Found {} files:\n{}", files.len(), files.join("\n"))
            }
            GrepOutputMode::Count => {
                if result.counts.is_empty() {
                    return "No matches found".to_string();
                }

                let counts_str = result
                    .counts
                    .iter()
                    .map(|(file, count)| format!("{}: {}", file, count))
                    .collect::<Vec<_>>()
                    .join("\n");

                format!(
                    "Total: {} matches in {} files\n{}",
                    result.total_matches, result.files_with_matches, counts_str
                )
            }
        }
    }
}

#[async_trait]
impl FileTool for GrepTool {
    fn name(&self) -> &'static str {
        "grep"
    }

    fn description(&self) -> &'static str {
        "Search file contents with regex patterns. Supports multiple output modes: \
         'content' shows matching lines, 'files_with_matches' shows file paths, \
         'count' shows match counts. Use context_before/context_after for surrounding lines. \
         Use glob parameter to filter by file pattern."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Regex pattern to search for"
                },
                "path": {
                    "type": "string",
                    "description": "Base path to search in (default: working directory)"
                },
                "glob": {
                    "type": "string",
                    "description": "Glob pattern to filter files (e.g., '*.rs', '**/*.ts')"
                },
                "output_mode": {
                    "type": "string",
                    "enum": ["content", "files_with_matches", "count"],
                    "description": "Output format (default: files_with_matches)"
                },
                "case_insensitive": {
                    "type": "boolean",
                    "description": "Case-insensitive search (default: false)"
                },
                "context_before": {
                    "type": "integer",
                    "description": "Lines of context before match"
                },
                "context_after": {
                    "type": "integer",
                    "description": "Lines of context after match"
                },
                "context": {
                    "type": "integer",
                    "description": "Lines of context before and after (shorthand)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum matches to return"
                }
            },
            "required": ["pattern"]
        })
    }

    async fn call(&self, params: Value) -> Result<ToolOutput, NikaError> {
        let params: GrepParams =
            serde_json::from_value(params.clone()).map_err(|e| NikaError::ToolError {
                code: ToolErrorCode::InvalidRegex.code(),
                message: format!("Invalid parameters: {}", e),
            })?;

        let output_mode = params.output_mode;
        let result = self.execute(params).await?;
        let content = self.format_output(&result, output_mode);

        Ok(ToolOutput::success_with_data(
            content,
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

    async fn create_test_files(temp_dir: &TempDir) {
        fs::create_dir_all(temp_dir.path().join("src"))
            .await
            .unwrap();

        fs::write(
            temp_dir.path().join("src/main.rs"),
            "fn main() {\n    println!(\"Hello\");\n    println!(\"World\");\n}",
        )
        .await
        .unwrap();

        fs::write(
            temp_dir.path().join("src/lib.rs"),
            "pub fn hello() {\n    // Hello function\n}",
        )
        .await
        .unwrap();

        fs::write(
            temp_dir.path().join("README.md"),
            "# Hello World\n\nThis is a test.",
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_grep_simple_pattern() {
        let (temp_dir, ctx) = setup_test().await;
        create_test_files(&temp_dir).await;

        let tool = GrepTool::new(ctx);
        let result = tool
            .execute(GrepParams {
                pattern: "Hello".to_string(),
                path: None,
                glob: None,
                output_mode: GrepOutputMode::Content,
                case_insensitive: false,
                context_before: None,
                context_after: None,
                context: None,
                limit: None,
            })
            .await
            .unwrap();

        assert!(result.total_matches >= 2);
        assert!(result.files_with_matches >= 2);
    }

    #[tokio::test]
    async fn test_grep_with_glob_filter() {
        let (temp_dir, ctx) = setup_test().await;
        create_test_files(&temp_dir).await;

        let tool = GrepTool::new(ctx);
        let result = tool
            .execute(GrepParams {
                pattern: "fn".to_string(),
                path: None,
                glob: Some("**/*.rs".to_string()),
                output_mode: GrepOutputMode::FilesWithMatches,
                case_insensitive: false,
                context_before: None,
                context_after: None,
                context: None,
                limit: None,
            })
            .await
            .unwrap();

        assert_eq!(result.files_with_matches, 2);
        assert!(result.matches.iter().all(|m| m.file.ends_with(".rs")));
    }

    #[tokio::test]
    async fn test_grep_case_insensitive() {
        let (temp_dir, ctx) = setup_test().await;
        create_test_files(&temp_dir).await;

        let tool = GrepTool::new(ctx);
        let result = tool
            .execute(GrepParams {
                pattern: "hello".to_string(),
                path: None,
                glob: None,
                output_mode: GrepOutputMode::Content,
                case_insensitive: true,
                context_before: None,
                context_after: None,
                context: None,
                limit: None,
            })
            .await
            .unwrap();

        // Should match "Hello" in multiple files
        assert!(result.total_matches >= 2);
    }

    #[tokio::test]
    async fn test_grep_with_context() {
        let (temp_dir, ctx) = setup_test().await;
        create_test_files(&temp_dir).await;

        let tool = GrepTool::new(ctx);
        let result = tool
            .execute(GrepParams {
                pattern: "println".to_string(),
                path: None,
                glob: Some("*.rs".to_string()),
                output_mode: GrepOutputMode::Content,
                case_insensitive: false,
                context_before: Some(1),
                context_after: Some(1),
                context: None,
                limit: None,
            })
            .await
            .unwrap();

        // Matches should have context
        for m in &result.matches {
            // Depending on position, there should be context
            assert!(m.context_before.len() <= 1);
            assert!(m.context_after.len() <= 1);
        }
    }

    #[tokio::test]
    async fn test_grep_count_mode() {
        let (temp_dir, ctx) = setup_test().await;
        create_test_files(&temp_dir).await;

        let tool = GrepTool::new(ctx);
        let result = tool
            .execute(GrepParams {
                pattern: "println".to_string(),
                path: None,
                glob: None,
                output_mode: GrepOutputMode::Count,
                case_insensitive: false,
                context_before: None,
                context_after: None,
                context: None,
                limit: None,
            })
            .await
            .unwrap();

        // main.rs has 2 println calls
        assert!(result
            .counts
            .iter()
            .any(|(f, c)| f.contains("main.rs") && *c == 2));
    }

    #[tokio::test]
    async fn test_grep_invalid_regex() {
        let (_temp_dir, ctx) = setup_test().await;

        let tool = GrepTool::new(ctx);
        let result = tool
            .execute(GrepParams {
                pattern: "[invalid".to_string(),
                path: None,
                glob: None,
                output_mode: GrepOutputMode::Content,
                case_insensitive: false,
                context_before: None,
                context_after: None,
                context: None,
                limit: None,
            })
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid regex"));
    }

    #[tokio::test]
    async fn test_file_tool_trait() {
        let (temp_dir, ctx) = setup_test().await;
        create_test_files(&temp_dir).await;

        let tool = GrepTool::new(ctx);

        assert_eq!(tool.name(), "grep");
        assert!(tool.description().contains("Search file contents"));

        let result = tool
            .call(json!({
                "pattern": "fn",
                "glob": "**/*.rs"
            }))
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("Found"));
    }
}
