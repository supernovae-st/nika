//! Rig Adapter - Bridges FileTool to rig::ToolDyn
//!
//! This module provides the `RigFileTool` wrapper that adapts any `FileTool`
//! implementation to work with rig-core's `ToolDyn` trait, enabling file tools
//! to be used in `RigAgentLoop`.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use nika::tools::{ReadTool, ToolContext, PermissionMode, RigFileTool};
//! use std::sync::Arc;
//!
//! let ctx = Arc::new(ToolContext::new(
//!     std::env::current_dir().unwrap(),
//!     PermissionMode::YoloMode,
//! ));
//!
//! // Wrap a FileTool for use with rig
//! let read_tool = RigFileTool::new(ReadTool::new(ctx));
//!
//! // Now `read_tool` implements rig::ToolDyn
//! ```

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use rig::completion::ToolDefinition;
use rig::tool::{ToolDyn, ToolError};
use serde_json::Value;

use super::FileTool;

/// Type alias for boxed future (required by ToolDyn)
type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Wrapper that adapts a `FileTool` to rig's `ToolDyn` trait
///
/// This enables file tools to be used directly in `RigAgentLoop` alongside
/// MCP tools and spawn_agent.
pub struct RigFileTool<T: FileTool + Send + Sync + 'static> {
    inner: Arc<T>,
}

impl<T: FileTool + Send + Sync + 'static> RigFileTool<T> {
    /// Create a new RigFileTool wrapper
    pub fn new(tool: T) -> Self {
        Self {
            inner: Arc::new(tool),
        }
    }

    /// Create from an existing Arc
    pub fn from_arc(tool: Arc<T>) -> Self {
        Self { inner: tool }
    }
}

impl<T: FileTool + Send + Sync + 'static> Clone for RigFileTool<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T: FileTool + Send + Sync + 'static> std::fmt::Debug for RigFileTool<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RigFileTool")
            .field("name", &self.inner.name())
            .finish()
    }
}

impl<T: FileTool + Send + Sync + 'static> ToolDyn for RigFileTool<T> {
    fn name(&self) -> String {
        self.inner.name().to_string()
    }

    fn definition(&self, _prompt: String) -> BoxFuture<'_, ToolDefinition> {
        let def = ToolDefinition {
            name: self.inner.name().to_string(),
            description: self.inner.description().to_string(),
            parameters: self.inner.parameters_schema(),
        };
        Box::pin(async move { def })
    }

    fn call(&self, args: String) -> BoxFuture<'_, Result<String, ToolError>> {
        let inner = Arc::clone(&self.inner);

        Box::pin(async move {
            // Parse args from JSON string
            let params: Value = serde_json::from_str(&args).map_err(|e| {
                ToolError::ToolCallError(Box::new(std::io::Error::other(format!(
                    "Invalid JSON arguments: {}",
                    e
                ))))
            })?;

            // Call the underlying FileTool
            let result = inner.call(params).await.map_err(|e| {
                ToolError::ToolCallError(Box::new(std::io::Error::other(e.to_string())))
            })?;

            // Return the content as a string (what rig expects)
            if result.is_error {
                Err(ToolError::ToolCallError(Box::new(std::io::Error::other(
                    result.content,
                ))))
            } else {
                Ok(result.content)
            }
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CONVENIENCE FUNCTIONS
// ═══════════════════════════════════════════════════════════════════════════

use super::{EditTool, GlobTool, GrepTool, ReadTool, ToolContext, WriteTool};

/// Create all file tools wrapped for rig integration
///
/// Returns a Vec of `Box<dyn ToolDyn>` ready to be added to a `RigAgentLoop`.
///
/// # Arguments
///
/// * `ctx` - Shared tool context (working_dir, permissions)
///
/// # Example
///
/// ```rust,ignore
/// let tools = nika::tools::create_rig_file_tools(ctx);
/// // Add to agent_builder.tools(tools)...
/// ```
pub fn create_rig_file_tools(ctx: Arc<ToolContext>) -> Vec<Box<dyn ToolDyn>> {
    vec![
        Box::new(RigFileTool::new(ReadTool::new(Arc::clone(&ctx)))),
        Box::new(RigFileTool::new(WriteTool::new(Arc::clone(&ctx)))),
        Box::new(RigFileTool::new(EditTool::new(Arc::clone(&ctx)))),
        Box::new(RigFileTool::new(GlobTool::new(Arc::clone(&ctx)))),
        Box::new(RigFileTool::new(GrepTool::new(ctx))),
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
    use tokio::fs;

    async fn setup_test() -> (TempDir, Arc<ToolContext>) {
        let temp_dir = TempDir::new().unwrap();
        let ctx = Arc::new(ToolContext::new(
            temp_dir.path().to_path_buf(),
            PermissionMode::YoloMode,
        ));
        (temp_dir, ctx)
    }

    #[tokio::test]
    async fn test_rig_file_tool_read() {
        let (temp_dir, ctx) = setup_test().await;

        // Create test file
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "Hello, World!").await.unwrap();

        // Wrap ReadTool
        let rig_tool = RigFileTool::new(ReadTool::new(ctx));

        // Verify name and definition
        assert_eq!(rig_tool.name(), "read");

        let def = rig_tool.definition("".to_string()).await;
        assert_eq!(def.name, "read");
        assert!(def.description.contains("Read a file"));

        // Call the tool
        let args = serde_json::json!({
            "file_path": file_path.to_string_lossy()
        })
        .to_string();

        let result = rig_tool.call(args).await;
        assert!(result.is_ok());
        let content = result.unwrap();
        assert!(content.contains("Hello, World!"));
    }

    #[tokio::test]
    async fn test_rig_file_tool_write() {
        let (temp_dir, ctx) = setup_test().await;

        let file_path = temp_dir.path().join("new_file.txt");

        let rig_tool = RigFileTool::new(WriteTool::new(ctx));

        let args = serde_json::json!({
            "file_path": file_path.to_string_lossy(),
            "content": "New content"
        })
        .to_string();

        let result = rig_tool.call(args).await;
        assert!(result.is_ok());

        // Verify file was created
        let content = fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(content, "New content");
    }

    #[tokio::test]
    async fn test_rig_file_tool_glob() {
        let (temp_dir, ctx) = setup_test().await;

        // Create test files
        fs::write(temp_dir.path().join("a.rs"), "fn a()")
            .await
            .unwrap();
        fs::write(temp_dir.path().join("b.rs"), "fn b()")
            .await
            .unwrap();
        fs::write(temp_dir.path().join("c.txt"), "text")
            .await
            .unwrap();

        let rig_tool = RigFileTool::new(GlobTool::new(ctx));

        let args = serde_json::json!({
            "pattern": "*.rs"
        })
        .to_string();

        let result = rig_tool.call(args).await;
        assert!(result.is_ok());
        let content = result.unwrap();
        assert!(content.contains("a.rs"));
        assert!(content.contains("b.rs"));
        assert!(!content.contains("c.txt"));
    }

    #[tokio::test]
    async fn test_create_rig_file_tools() {
        let (_temp_dir, ctx) = setup_test().await;

        let tools = create_rig_file_tools(ctx);

        assert_eq!(tools.len(), 5);

        // Check tool names
        let names: Vec<String> = tools.iter().map(|t| t.name()).collect();
        assert!(names.contains(&"read".to_string()));
        assert!(names.contains(&"write".to_string()));
        assert!(names.contains(&"edit".to_string()));
        assert!(names.contains(&"glob".to_string()));
        assert!(names.contains(&"grep".to_string()));
    }

    #[tokio::test]
    async fn test_rig_file_tool_error_handling() {
        let (_temp_dir, ctx) = setup_test().await;

        let rig_tool = RigFileTool::new(ReadTool::new(ctx));

        // Try to read non-existent file
        let args = serde_json::json!({
            "file_path": "/nonexistent/path/file.txt"
        })
        .to_string();

        let result = rig_tool.call(args).await;
        assert!(result.is_err());
    }
}
