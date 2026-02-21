//! Glob Tool - Find files by pattern
//!
//! Fast pattern matching using the `ignore` crate:
//! - Supports `**/*.rs`, `src/**/*.ts` patterns
//! - Respects .gitignore automatically
//! - Sorted by modification time (deterministic)

use std::sync::Arc;
use std::time::SystemTime;

use async_trait::async_trait;
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::context::{ToolContext, ToolEvent};
use super::{FileTool, ToolErrorCode, ToolOutput};
use crate::error::NikaError;

// ═══════════════════════════════════════════════════════════════════════════
// PARAMETERS & RESULT
// ═══════════════════════════════════════════════════════════════════════════

/// Parameters for the Glob tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobParams {
    /// Glob pattern (e.g., "**/*.rs", "src/**/*.ts")
    pub pattern: String,

    /// Base path to search in (default: working directory)
    #[serde(default)]
    pub path: Option<String>,
}

/// Result from glob search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobResult {
    /// Matching file paths (absolute)
    pub matches: Vec<String>,

    /// Number of matches
    pub count: usize,

    /// Base path searched
    pub base_path: String,
}

// ═══════════════════════════════════════════════════════════════════════════
// GLOB TOOL
// ═══════════════════════════════════════════════════════════════════════════

/// Glob tool for finding files by pattern
///
/// # Features
///
/// - Fast pattern matching via `ignore` crate
/// - Respects .gitignore automatically
/// - Results sorted by modification time
/// - Supports recursive patterns (`**`)
pub struct GlobTool {
    ctx: Arc<ToolContext>,
}

impl GlobTool {
    /// Maximum files to return (prevent memory issues)
    pub const MAX_RESULTS: usize = 10000;

    /// Create a new Glob tool
    pub fn new(ctx: Arc<ToolContext>) -> Self {
        Self { ctx }
    }

    /// Execute the glob search
    pub async fn execute(&self, params: GlobParams) -> Result<GlobResult, NikaError> {
        // Determine base path
        let base_path = match params.path {
            Some(ref p) => self.ctx.validate_path(p)?,
            None => self.ctx.working_dir().to_path_buf(),
        };

        // Build glob matcher
        let glob = globset::GlobBuilder::new(&params.pattern)
            .literal_separator(true)
            .build()
            .map_err(|e| NikaError::ToolError {
                code: ToolErrorCode::InvalidGlobPattern.code(),
                message: format!("Invalid glob pattern '{}': {}", params.pattern, e),
            })?
            .compile_matcher();

        // Walk directory and collect matches
        let mut matches: Vec<(String, SystemTime)> = Vec::new();

        let walker = WalkBuilder::new(&base_path)
            .hidden(false) // Include hidden files
            .git_ignore(true) // Respect .gitignore
            .git_global(true)
            .git_exclude(true)
            .build();

        for entry in walker.filter_map(Result::ok) {
            let path = entry.path();

            // Skip directories
            if path.is_dir() {
                continue;
            }

            // Check if path matches the pattern
            // We need to match against the relative path from base
            let relative = path.strip_prefix(&base_path).unwrap_or(path);

            if glob.is_match(relative) || glob.is_match(path) {
                let modified = path
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .unwrap_or(SystemTime::UNIX_EPOCH);

                matches.push((path.to_string_lossy().to_string(), modified));

                // Limit results
                if matches.len() >= Self::MAX_RESULTS {
                    break;
                }
            }
        }

        // Sort by modification time (newest first for determinism)
        matches.sort_by(|a, b| b.1.cmp(&a.1));

        let count = matches.len();
        let match_paths: Vec<String> = matches.into_iter().map(|(p, _)| p).collect();

        // Emit event
        self.ctx
            .emit(ToolEvent::GlobSearch {
                pattern: params.pattern,
                matches: count,
                base_path: base_path.to_string_lossy().to_string(),
            })
            .await;

        Ok(GlobResult {
            matches: match_paths,
            count,
            base_path: base_path.to_string_lossy().to_string(),
        })
    }
}

#[async_trait]
impl FileTool for GlobTool {
    fn name(&self) -> &'static str {
        "glob"
    }

    fn description(&self) -> &'static str {
        "Find files matching a glob pattern. Supports recursive patterns like '**/*.rs'. \
         Respects .gitignore automatically. Results are sorted by modification time. \
         Use this to discover files before reading them."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern (e.g., '**/*.rs', 'src/**/*.ts', '*.json')"
                },
                "path": {
                    "type": "string",
                    "description": "Base path to search in (default: working directory)"
                }
            },
            "required": ["pattern"]
        })
    }

    async fn call(&self, params: Value) -> Result<ToolOutput, NikaError> {
        let params: GlobParams =
            serde_json::from_value(params).map_err(|e| NikaError::ToolError {
                code: ToolErrorCode::InvalidGlobPattern.code(),
                message: format!("Invalid parameters: {}", e),
            })?;

        let result = self.execute(params).await?;

        // Format output as newline-separated paths
        let content = if result.matches.is_empty() {
            "No matching files found".to_string()
        } else {
            format!(
                "Found {} files:\n{}",
                result.count,
                result.matches.join("\n")
            )
        };

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
    use tokio::fs;

    async fn setup_test() -> (TempDir, Arc<ToolContext>) {
        let temp_dir = TempDir::new().unwrap();
        let ctx = Arc::new(ToolContext::new(
            temp_dir.path().to_path_buf(),
            super::super::context::PermissionMode::YoloMode,
        ));
        (temp_dir, ctx)
    }

    async fn create_test_files(temp_dir: &TempDir) {
        // Create directory structure
        fs::create_dir_all(temp_dir.path().join("src"))
            .await
            .unwrap();
        fs::create_dir_all(temp_dir.path().join("tests"))
            .await
            .unwrap();

        // Create files
        fs::write(temp_dir.path().join("src/main.rs"), "fn main() {}")
            .await
            .unwrap();
        fs::write(temp_dir.path().join("src/lib.rs"), "pub fn lib() {}")
            .await
            .unwrap();
        fs::write(temp_dir.path().join("tests/test.rs"), "#[test]")
            .await
            .unwrap();
        fs::write(temp_dir.path().join("Cargo.toml"), "[package]")
            .await
            .unwrap();
        fs::write(temp_dir.path().join("README.md"), "# Readme")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_glob_all_rs_files() {
        let (temp_dir, ctx) = setup_test().await;
        create_test_files(&temp_dir).await;

        let tool = GlobTool::new(ctx);
        let result = tool
            .execute(GlobParams {
                pattern: "**/*.rs".to_string(),
                path: None,
            })
            .await
            .unwrap();

        assert_eq!(result.count, 3);
        assert!(result.matches.iter().any(|p| p.contains("main.rs")));
        assert!(result.matches.iter().any(|p| p.contains("lib.rs")));
        assert!(result.matches.iter().any(|p| p.contains("test.rs")));
    }

    #[tokio::test]
    async fn test_glob_src_only() {
        let (temp_dir, ctx) = setup_test().await;
        create_test_files(&temp_dir).await;

        let tool = GlobTool::new(ctx);
        let result = tool
            .execute(GlobParams {
                pattern: "*.rs".to_string(),
                path: Some(temp_dir.path().join("src").to_string_lossy().to_string()),
            })
            .await
            .unwrap();

        assert_eq!(result.count, 2);
        assert!(result.matches.iter().all(|p| p.contains("src")));
    }

    #[tokio::test]
    async fn test_glob_specific_extension() {
        let (temp_dir, ctx) = setup_test().await;
        create_test_files(&temp_dir).await;

        let tool = GlobTool::new(ctx);
        let result = tool
            .execute(GlobParams {
                pattern: "*.toml".to_string(),
                path: None,
            })
            .await
            .unwrap();

        assert_eq!(result.count, 1);
        assert!(result.matches[0].contains("Cargo.toml"));
    }

    #[tokio::test]
    async fn test_glob_no_matches() {
        let (temp_dir, ctx) = setup_test().await;
        create_test_files(&temp_dir).await;

        let tool = GlobTool::new(ctx);
        let result = tool
            .execute(GlobParams {
                pattern: "**/*.xyz".to_string(),
                path: None,
            })
            .await
            .unwrap();

        assert_eq!(result.count, 0);
        assert!(result.matches.is_empty());
    }

    #[tokio::test]
    async fn test_glob_invalid_pattern() {
        let (_temp_dir, ctx) = setup_test().await;

        let tool = GlobTool::new(ctx);
        let result = tool
            .execute(GlobParams {
                pattern: "[invalid".to_string(),
                path: None,
            })
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid glob"));
    }

    #[tokio::test]
    async fn test_file_tool_trait() {
        let (temp_dir, ctx) = setup_test().await;
        create_test_files(&temp_dir).await;

        let tool = GlobTool::new(ctx);

        assert_eq!(tool.name(), "glob");
        assert!(tool.description().contains("Find files"));

        let result = tool.call(json!({ "pattern": "**/*.rs" })).await.unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("Found"));
    }
}
