//! Tool Context - Shared state and permissions for file tools
//!
//! Manages:
//! - Working directory (security boundary)
//! - Read files tracking (for edit validation)
//! - Permission mode (Plan, AcceptEdits, YoloMode)
//! - Event emission for observability

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::ToolErrorCode;
use crate::error::NikaError;

// ═══════════════════════════════════════════════════════════════════════════
// PERMISSION MODE
// ═══════════════════════════════════════════════════════════════════════════

/// Permission mode for tool operations
///
/// Inspired by Gemini CLI's Yolo Mode and Claude Code's permission levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PermissionMode {
    /// Deny all file operations
    Deny,

    /// Ask before each operation (Plan mode)
    ///
    /// Returns `PermissionDenied` and emits a permission request event.
    /// The UI should handle this and prompt the user.
    #[default]
    Plan,

    /// Auto-approve file edits and writes (create/modify), ask for delete
    ///
    /// Allows builtin tools like `nika:write` and `nika:edit` to operate
    /// without prompting. Good for workflow execution where file output is expected.
    AcceptEdits,

    /// Auto-approve all operations (Yolo mode)
    ///
    /// Use with caution! Best for sandboxed environments.
    YoloMode,
}

impl PermissionMode {
    /// Check if an operation is allowed
    pub fn allows(&self, operation: ToolOperation) -> bool {
        match self {
            PermissionMode::Deny => false,
            PermissionMode::Plan => false, // Always ask
            PermissionMode::AcceptEdits => {
                matches!(operation, ToolOperation::Edit | ToolOperation::Write)
            }
            PermissionMode::YoloMode => true,
        }
    }

    /// Get display name
    pub fn display_name(&self) -> &'static str {
        match self {
            PermissionMode::Deny => "Deny",
            PermissionMode::Plan => "Plan",
            PermissionMode::AcceptEdits => "AcceptEdits",
            PermissionMode::YoloMode => "YoloMode (Yolo)",
        }
    }
}

/// Type of tool operation for permission checking
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolOperation {
    /// Reading a file (usually safe)
    Read,
    /// Creating a new file
    Write,
    /// Modifying an existing file
    Edit,
    /// Searching files (glob/grep)
    Search,
}

// ═══════════════════════════════════════════════════════════════════════════
// TOOL EVENT
// ═══════════════════════════════════════════════════════════════════════════

/// Events emitted by tools for observability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolEvent {
    /// File was read
    FileRead {
        path: String,
        lines: usize,
        truncated: bool,
    },

    /// File was written (created)
    FileWritten { path: String, bytes: usize },

    /// File was edited
    FileEdited {
        path: String,
        replacements: usize,
        diff_preview: String,
    },

    /// Glob search completed
    GlobSearch {
        pattern: String,
        matches: usize,
        base_path: String,
    },

    /// Grep search completed
    GrepSearch {
        pattern: String,
        files_searched: usize,
        matches: usize,
    },

    /// Permission request (for Plan mode)
    PermissionRequest {
        operation: String,
        path: String,
        details: String,
    },

    /// Permission granted by user
    PermissionGranted { operation: String, path: String },

    /// Permission denied by user
    PermissionDeniedByUser { operation: String, path: String },
}

// ═══════════════════════════════════════════════════════════════════════════
// TOOL CONTEXT
// ═══════════════════════════════════════════════════════════════════════════

/// Shared context for all file tools
///
/// Thread-safe via `Arc<ToolContext>` + `RwLock` for mutable state.
pub struct ToolContext {
    /// Working directory (security boundary)
    ///
    /// All file operations must be within this directory.
    /// Mutable via RwLock so with_base_path() can update it after construction.
    working_dir: RwLock<PathBuf>,

    /// Files that have been read (for edit validation)
    ///
    /// Edit operations require the file to be read first.
    read_files: RwLock<HashSet<PathBuf>>,

    /// Current permission mode
    permission_mode: RwLock<PermissionMode>,

    /// Event sender for observability (optional)
    event_tx: Option<mpsc::Sender<ToolEvent>>,
}

impl ToolContext {
    /// Create a new tool context
    ///
    /// # Arguments
    ///
    /// * `working_dir` - Security boundary for file operations
    /// * `permission_mode` - Initial permission level
    pub fn new(working_dir: PathBuf, permission_mode: PermissionMode) -> Self {
        // Canonicalize working_dir to resolve symlinks (e.g., /var → /private/var on macOS)
        // This ensures validate_path() comparisons work correctly
        let working_dir = working_dir.canonicalize().unwrap_or(working_dir);
        Self {
            working_dir: RwLock::new(working_dir),
            read_files: RwLock::new(HashSet::new()),
            permission_mode: RwLock::new(permission_mode),
            event_tx: None,
        }
    }

    /// Create context with event channel
    pub fn with_events(mut self, tx: mpsc::Sender<ToolEvent>) -> Self {
        self.event_tx = Some(tx);
        self
    }

    /// Get the working directory
    pub fn working_dir(&self) -> PathBuf {
        self.working_dir.read().clone()
    }

    /// Update the working directory (B02 fix: called from with_base_path)
    pub fn set_working_dir(&self, dir: PathBuf) {
        let dir = dir.canonicalize().unwrap_or(dir);
        *self.working_dir.write() = dir;
    }

    /// Get current permission mode
    pub fn permission_mode(&self) -> PermissionMode {
        *self.permission_mode.read()
    }

    /// Set permission mode
    pub fn set_permission_mode(&self, mode: PermissionMode) {
        *self.permission_mode.write() = mode;
    }

    /// Validate a path is safe to access
    ///
    /// Relative paths are resolved against the working directory.
    /// Returns the canonicalized path if valid.
    ///
    /// # Errors
    ///
    /// - `PathOutOfBounds` if path is outside working directory
    pub fn validate_path(&self, file_path: &str) -> Result<PathBuf, NikaError> {
        let working_dir = self.working_dir.read().clone();
        let raw_path = PathBuf::from(file_path);

        // Resolve relative paths against working directory
        let path = if raw_path.is_absolute() {
            raw_path
        } else {
            working_dir.join(&raw_path)
        };

        // Canonicalize to resolve .. and symlinks
        // For existing files: canonicalize directly
        // For non-existent files: find first existing ancestor, canonicalize, then append remaining
        let normalized = if path.exists() {
            path.canonicalize().unwrap_or(path)
        } else {
            // Find the first existing ancestor and canonicalize from there
            // This handles deeply nested paths like /var/folders/.../nested/deep/file.txt
            // where /var → /private/var on macOS
            self.canonicalize_with_ancestors(&path)
        };

        // Must be within working directory
        if !normalized.starts_with(&working_dir) {
            return Err(NikaError::ToolError {
                code: ToolErrorCode::PathOutOfBounds.code(),
                message: format!(
                    "Path '{}' is outside working directory '{}'. \
                     Use --workdir to change the base directory, or use an absolute path within it.",
                    file_path,
                    working_dir.display()
                ),
            });
        }

        Ok(normalized)
    }

    /// Normalize a path without requiring it to exist
    fn normalize_path(&self, path: &Path) -> PathBuf {
        let mut components = Vec::new();

        for component in path.components() {
            match component {
                std::path::Component::ParentDir => {
                    components.pop();
                }
                std::path::Component::CurDir => {}
                _ => components.push(component),
            }
        }

        components.iter().collect()
    }

    /// Canonicalize a path by finding the first existing ancestor
    ///
    /// This handles paths like `/var/folders/.../nested/deep/file.txt` where:
    /// - `/var` → `/private/var` on macOS (symlink)
    /// - `nested/deep/` doesn't exist yet
    ///
    /// We walk up until we find an existing directory, canonicalize it,
    /// then append the remaining components.
    fn canonicalize_with_ancestors(&self, path: &Path) -> PathBuf {
        let mut ancestors: Vec<&std::ffi::OsStr> = Vec::new();
        let mut current = path;

        // Walk up until we find an existing ancestor
        while !current.exists() {
            if let Some(file_name) = current.file_name() {
                ancestors.push(file_name);
            }
            if let Some(parent) = current.parent() {
                current = parent;
            } else {
                // No existing ancestor found, fall back to normalize
                return self.normalize_path(path);
            }
        }

        // Canonicalize the existing ancestor
        let canonical_base = current
            .canonicalize()
            .unwrap_or_else(|_| current.to_path_buf());

        // Append the non-existent components in reverse order
        let mut result = canonical_base;
        for component in ancestors.into_iter().rev() {
            result = result.join(component);
        }

        result
    }

    /// Check if operation is allowed by current permission mode
    pub fn check_permission(&self, operation: ToolOperation) -> Result<(), NikaError> {
        let mode = self.permission_mode();

        if mode.allows(operation) {
            return Ok(());
        }

        // For Plan mode, we could emit a permission request event
        // For now, just deny
        Err(NikaError::ToolError {
            code: ToolErrorCode::PermissionDenied.code(),
            message: format!(
                "Permission denied: {:?} not allowed in {} mode",
                operation,
                mode.display_name()
            ),
        })
    }

    /// Mark a file as read (for edit validation)
    pub fn mark_as_read(&self, path: &Path) {
        // Canonicalize to handle symlinks (e.g., /var → /private/var on macOS)
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.read_files.write().insert(canonical);
    }

    /// Check if a file has been read (required before edit)
    pub fn was_read(&self, path: &Path) -> bool {
        // Canonicalize for consistent comparison
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.read_files.read().contains(&canonical)
    }

    /// Validate that file was read before edit
    pub fn validate_read_before_edit(&self, path: &Path) -> Result<(), NikaError> {
        if !self.was_read(path) {
            return Err(NikaError::ToolError {
                code: ToolErrorCode::MustReadFirst.code(),
                message: format!(
                    "Must read file before editing: {}. Use the Read tool first.",
                    path.display()
                ),
            });
        }
        Ok(())
    }

    /// Emit a tool event. Logs a warning if the channel is closed/full.
    pub async fn emit(&self, event: ToolEvent) {
        if let Some(ref tx) = self.event_tx {
            if let Err(e) = tx.send(event).await {
                tracing::debug!(error = %e, "Tool event channel closed, event dropped");
            }
        }
    }

    /// Clear read files tracking (for testing or reset)
    pub fn clear_read_tracking(&self) {
        self.read_files.write().clear();
    }
}

impl std::fmt::Debug for ToolContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolContext")
            .field("working_dir", &*self.working_dir.read())
            .field("permission_mode", &self.permission_mode())
            .field("read_files_count", &self.read_files.read().len())
            .finish()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST UTILITIES (SHARED)
// ═══════════════════════════════════════════════════════════════════════════

/// Shared test utilities for file tools
///
/// This module provides common test setup patterns used across all file tools.
/// Import in other tool test modules via:
/// ```ignore
/// use crate::tools::context::testing::{setup_test, create_test_file};
/// ```
#[cfg(test)]
pub mod testing {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tempfile::TempDir;

    /// Standard test setup for file tools.
    ///
    /// Creates a temporary directory and a ToolContext in YoloMode.
    /// The TempDir is returned to keep it alive for the duration of the test.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use crate::tools::context::testing::setup_test;
    ///
    /// #[tokio::test]
    /// async fn test_my_tool() {
    ///     let (temp_dir, ctx) = setup_test().await;
    ///     // Use ctx for tool operations
    /// }
    /// ```
    pub async fn setup_test() -> (TempDir, Arc<ToolContext>) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let ctx = Arc::new(ToolContext::new(
            temp_dir.path().to_path_buf(),
            PermissionMode::YoloMode,
        ));
        (temp_dir, ctx)
    }

    /// Create a test file with content in the temp directory.
    ///
    /// Returns the absolute path to the created file.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let (temp_dir, ctx) = setup_test().await;
    /// let path = create_test_file(&temp_dir, "test.txt", "Hello World").await;
    /// ```
    pub async fn create_test_file(dir: &TempDir, name: &str, content: &str) -> PathBuf {
        let path = dir.path().join(name);
        tokio::fs::write(&path, content)
            .await
            .expect("Failed to write test file");
        path
    }

    /// Create a nested directory structure with test files.
    ///
    /// The `files` parameter is a slice of (relative_path, content) tuples.
    /// Parent directories are created automatically.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let (temp_dir, ctx) = setup_test().await;
    /// create_test_tree(&temp_dir, &[
    ///     ("src/main.rs", "fn main() {}"),
    ///     ("src/lib.rs", "pub mod foo;"),
    ///     ("tests/test.rs", "#[test] fn it_works() {}"),
    /// ]).await;
    /// ```
    pub async fn create_test_tree(dir: &TempDir, files: &[(&str, &str)]) {
        for (name, content) in files {
            let path = dir.path().join(name);
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .expect("Failed to create directories");
            }
            tokio::fs::write(&path, content)
                .await
                .expect("Failed to write test file");
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::sync::Arc;

    fn test_context() -> Arc<ToolContext> {
        let working_dir = env::current_dir().unwrap();
        Arc::new(ToolContext::new(working_dir, PermissionMode::YoloMode))
    }

    #[test]
    fn test_permission_mode_allows() {
        assert!(!PermissionMode::Deny.allows(ToolOperation::Read));
        assert!(!PermissionMode::Plan.allows(ToolOperation::Edit));
        assert!(PermissionMode::AcceptEdits.allows(ToolOperation::Edit));
        assert!(PermissionMode::AcceptEdits.allows(ToolOperation::Write));
        assert!(PermissionMode::YoloMode.allows(ToolOperation::Write));
    }

    #[test]
    fn test_validate_path_relative_resolved() {
        let ctx = test_context();
        let result = ctx.validate_path("src/main.rs");
        // Relative paths should be resolved against working_dir, not rejected
        assert!(
            result.is_ok(),
            "relative path should resolve: {:?}",
            result.err()
        );
        let resolved = result.unwrap();
        assert!(resolved.is_absolute(), "resolved path must be absolute");
        assert!(resolved.ends_with("src/main.rs"));
    }

    #[test]
    fn test_validate_path_within_working_dir() {
        let ctx = test_context();
        let wd = ctx.working_dir();
        let working_dir = wd.to_string_lossy();
        let valid_path = format!("{}/src/main.rs", working_dir);

        let result = ctx.validate_path(&valid_path);
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    #[test]
    fn test_validate_path_outside_working_dir() {
        let ctx = test_context();
        let result = ctx.validate_path("/etc/passwd");
        assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
        assert!(result.unwrap_err().to_string().contains("outside"));
    }

    #[test]
    fn test_read_tracking() {
        let ctx = test_context();
        let path = PathBuf::from("/test/file.rs");

        assert!(!ctx.was_read(&path));
        ctx.mark_as_read(&path);
        assert!(ctx.was_read(&path));

        ctx.clear_read_tracking();
        assert!(!ctx.was_read(&path));
    }

    #[test]
    fn test_validate_read_before_edit() {
        let ctx = test_context();
        let path = PathBuf::from("/test/file.rs");

        // Should fail before read
        let result = ctx.validate_read_before_edit(&path);
        assert!(result.is_err(), "Should fail but got: {:?}", result.ok());

        // Should pass after read
        ctx.mark_as_read(&path);
        let result = ctx.validate_read_before_edit(&path);
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    #[test]
    fn test_permission_mode_change() {
        let ctx = test_context();

        assert_eq!(ctx.permission_mode(), PermissionMode::YoloMode);

        ctx.set_permission_mode(PermissionMode::Plan);
        assert_eq!(ctx.permission_mode(), PermissionMode::Plan);
    }

    #[test]
    fn test_check_permission_deny_mode() {
        let working_dir = env::current_dir().unwrap();
        let ctx = ToolContext::new(working_dir, PermissionMode::Deny);

        let result = ctx.check_permission(ToolOperation::Read);
        assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
    }

    #[test]
    fn test_check_permission_accept_all() {
        let ctx = test_context();

        let result = ctx.check_permission(ToolOperation::Read);
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        let result = ctx.check_permission(ToolOperation::Write);
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        let result = ctx.check_permission(ToolOperation::Edit);
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        let result = ctx.check_permission(ToolOperation::Search);
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }
}
