//! File Tools Module - Claude Code-like filesystem operations
//!
//! Provides 5 tools for filesystem interaction:
//! - [`ReadTool`] - Read files with line numbers
//! - [`WriteTool`] - Create new files
//! - [`EditTool`] - Modify existing files (requires read-before-edit)
//! - [`GlobTool`] - Find files by pattern
//! - [`GrepTool`] - Search file contents with regex
//!
//! # Permission Model
//!
//! Inspired by Gemini CLI's Yolo Mode and Claude Code's permission levels:
//!
//! | Mode | Behavior |
//! |------|----------|
//! | `Deny` | All operations denied |
//! | `Plan` | Ask before each operation |
//! | `AcceptEdits` | Auto-approve edits, ask for others |
//! | `YoloMode` | Auto-approve all (Yolo mode) |
//!
//! # Security
//!
//! All paths are validated to be:
//! - Absolute paths only
//! - Within the working directory (security boundary)
//!
//! # Example
//!
//! ```ignore
//! use nika::tools::{ToolContext, ReadTool, PermissionMode};
//! use std::path::PathBuf;
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let ctx = Arc::new(ToolContext::new(
//!         PathBuf::from("/path/to/project"),
//!         PermissionMode::YoloMode,
//!     ));
//!
//!     let read_tool = ReadTool::new(ctx);
//!     let result = read_tool.execute(nika::tools::ReadParams {
//!         file_path: "/path/to/project/src/main.rs".to_string(),
//!         offset: None,
//!         limit: None,
//!     }).await?;
//!
//!     println!("{}", result.content);
//!     Ok(())
//! }
//! ```

mod context;
mod edit;
mod glob;
mod grep;
mod read;
mod rig_adapter;
mod submit_tool;
#[cfg(test)]
mod tests_shield_path_check;
mod write;

pub use context::{PermissionMode, ToolContext, ToolEvent, ToolOperation};
pub use edit::{EditParams, EditResult, EditTool};
pub use glob::{GlobParams, GlobResult, GlobTool};
pub use grep::{GrepOutputMode, GrepParams, GrepResult, GrepTool};
pub use read::{ReadParams, ReadResult, ReadTool};
pub use rig_adapter::{create_rig_file_tools, RigFileTool};
pub use submit_tool::{DynamicSubmitTool, ToolDefinition};
pub use write::{WriteParams, WriteResult, WriteTool};

use crate::error::NikaError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

// ═══════════════════════════════════════════════════════════════════════════
// SHIELD: PATH RECONNAISSANCE BLOCK
// ═══════════════════════════════════════════════════════════════════════════

/// Files that an untrusted agent must never read. Sprint 2 Item 3b.
///
/// Resolved by file name (after canonicalization, so symlink-bait fails).
pub const SENSITIVE_FILE_NAMES: &[&str] = &[".mcp.json", "nika.toml"];

/// Suffixes that an untrusted agent must never read. Sprint 2 Item 3b.
pub const SENSITIVE_FILE_SUFFIXES: &[&str] = &[".nika.yaml", ".nika.yml"];

/// File-name patterns matching dot-env families. We treat any file whose
/// name starts with `.env` (`.env`, `.env.local`, `.env.production`, …)
/// as sensitive.
#[inline]
fn is_dotenv_family(name: &str) -> bool {
    name == ".env" || name.starts_with(".env.")
}

/// Check whether a path is readable by the calling task. Sprint 2 Item 3b.
///
/// For tainted agents (untrusted upstream inputs, not `trust: elevated`),
/// blocks reads of `nika.toml`, `.mcp.json`, `.env*`, and any `*.nika.yaml`
/// workflow file. Resolves symlinks before checking so a symlink-bait
/// attack (`innocent.txt` → `nika.toml`) is defeated.
///
/// Returns `Ok(())` for trusted callers, elevated callers, and any path
/// that is not on the sensitive list.
pub fn check_path_readable(
    path: &Path,
    caller_trust: nika_core::trust::TrustLevel,
    caller_elevated: bool,
) -> Result<(), NikaError> {
    if !caller_trust.is_untrusted() || caller_elevated {
        return Ok(());
    }

    // Canonicalize first to defeat symlink-bait attacks. If canonicalization
    // fails (e.g. file doesn't exist yet), fall back to the literal path —
    // the caller's read will then fail with the underlying I/O error and
    // there's nothing to leak.
    let canonical = path.canonicalize().unwrap_or_else(|_| PathBuf::from(path));
    let file_name = canonical
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    let blocked = SENSITIVE_FILE_NAMES.contains(&file_name)
        || SENSITIVE_FILE_SUFFIXES
            .iter()
            .any(|suffix| file_name.ends_with(suffix))
        || is_dotenv_family(file_name);

    if blocked {
        let task_id = crate::runtime::builtin::run::current_task_id()
            .map(|id| id.to_string())
            .unwrap_or_else(|| "<unknown>".to_string());
        return Err(NikaError::CapabilityDenied {
            task_id,
            action: "nika:read".to_string(),
            reason: format!(
                "tainted agent cannot read sensitive file: {file_name} \
                 (canonical: {})",
                canonical.display()
            ),
        });
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// TOOL TRAIT
// ═══════════════════════════════════════════════════════════════════════════

/// Trait for file tools that can be called by agents
///
/// Implements the rig::ToolDyn pattern for integration with RigAgentLoop.
#[async_trait]
pub trait FileTool: Send + Sync {
    /// Tool name (e.g., "read", "edit", "glob")
    fn name(&self) -> &'static str;

    /// Tool description for LLM
    fn description(&self) -> &'static str;

    /// JSON Schema for parameters
    fn parameters_schema(&self) -> Value;

    /// Execute the tool with JSON parameters
    async fn call(&self, params: Value) -> Result<ToolOutput, NikaError>;
}

/// Output from a tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    /// Text content of the result
    pub content: String,
    /// Whether this is an error response
    pub is_error: bool,
    /// Optional structured data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl ToolOutput {
    /// Create a successful output
    pub fn success(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: false,
            data: None,
        }
    }

    /// Create a successful output with data
    pub fn success_with_data(content: impl Into<String>, data: Value) -> Self {
        Self {
            content: content.into(),
            is_error: false,
            data: Some(data),
        }
    }

    /// Create an error output
    pub fn error(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: true,
            data: None,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TOOL ERROR CODES
// ═══════════════════════════════════════════════════════════════════════════

/// Tool-specific error codes (NIKA-200 range)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolErrorCode {
    /// NIKA-200: Read operation failed
    ReadFailed = 200,
    /// NIKA-201: Write operation failed
    WriteFailed = 201,
    /// NIKA-202: Edit operation failed
    EditFailed = 202,
    /// NIKA-203: Must read file before editing
    MustReadFirst = 203,
    /// NIKA-204: Path is outside working directory
    PathOutOfBounds = 204,
    /// NIKA-205: Permission denied for this operation
    PermissionDenied = 205,
    /// NIKA-206: Invalid glob pattern
    InvalidGlobPattern = 206,
    /// NIKA-207: Invalid regex pattern
    InvalidRegex = 207,
    /// NIKA-208: File not found
    FileNotFound = 208,
    /// NIKA-209: old_string not unique in file
    OldStringNotUnique = 209,
    /// NIKA-215: File already exists (for write)
    FileAlreadyExists = 215,
    /// NIKA-211: Path must be absolute
    RelativePath = 211,
}

impl ToolErrorCode {
    /// Get the error code string
    pub fn code(&self) -> String {
        format!("NIKA-{}", *self as u16)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_output_success() {
        let output = ToolOutput::success("File read successfully");
        assert!(!output.is_error);
        assert_eq!(output.content, "File read successfully");
        assert!(output.data.is_none());
    }

    #[test]
    fn test_tool_output_error() {
        let output = ToolOutput::error("File not found");
        assert!(output.is_error);
        assert_eq!(output.content, "File not found");
    }

    #[test]
    fn test_tool_error_codes() {
        assert_eq!(ToolErrorCode::ReadFailed.code(), "NIKA-200");
        assert_eq!(ToolErrorCode::MustReadFirst.code(), "NIKA-203");
        assert_eq!(ToolErrorCode::PathOutOfBounds.code(), "NIKA-204");
    }
}
