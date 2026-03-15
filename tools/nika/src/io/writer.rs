//! Artifact Writer - File Persistence for Task Outputs
//!
//! The `ArtifactWriter` is the main entry point for writing task outputs to disk.
//! It combines atomic writes, path security, and template resolution.
//!
//! # Features
//!
//! - **Atomic Writes**: Uses temp file + rename pattern for crash safety
//! - **Path Security**: Validates paths to prevent traversal attacks
//! - **Template Resolution**: Supports `{{task_id}}`, `{{date}}`, etc. in paths
//! - **Size Limits**: Enforces configurable maximum file sizes
//! - **Format Support**: JSON, YAML, and raw text output
//!
//! # Example
//!
//! ```ignore
//! use nika::io::writer::{ArtifactWriter, WriteRequest};
//! use nika::ast::artifact::OutputFormat;
//!
//! let writer = ArtifactWriter::new("/project/.nika/artifacts", "my-workflow")?;
//! let request = WriteRequest::new("generate_page", "pages/{{task_id}}.json")
//!     .with_content(r#"{"title": "Hello"}"#)
//!     .with_format(OutputFormat::Json);
//! writer.write(request).await?;
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tokio::fs;

use crate::error::NikaError;
use crate::io::atomic::write_atomic;
use crate::io::security::validate_artifact_path;
use crate::io::template::TemplateResolver;
use crate::OutputFormat;

/// Default maximum artifact size (10 MB)
pub const DEFAULT_MAX_SIZE: u64 = 10 * 1024 * 1024;

/// Result of a successful write operation
#[derive(Debug, Clone)]
pub struct WriteResult {
    /// Final resolved path where artifact was written
    pub path: PathBuf,
    /// Size in bytes of the written content
    pub size: u64,
    /// Format used for output
    pub format: OutputFormat,
}

/// Request to write an artifact
#[derive(Debug, Clone)]
pub struct WriteRequest {
    /// Task ID that produced this output
    pub task_id: String,
    /// Output path template (may contain `{{var}}` placeholders)
    pub output_path: String,
    /// Content to write (serialized)
    pub content: String,
    /// Output format (affects serialization validation)
    pub format: OutputFormat,
    /// Custom template variables
    pub vars: HashMap<String, String>,
}

impl WriteRequest {
    /// Create a new write request
    pub fn new(task_id: impl Into<String>, output_path: impl Into<String>) -> Self {
        Self {
            task_id: task_id.into(),
            output_path: output_path.into(),
            content: String::new(),
            format: OutputFormat::Text,
            vars: HashMap::new(),
        }
    }

    /// Set the content to write
    pub fn with_content(mut self, content: impl Into<String>) -> Self {
        self.content = content.into();
        self
    }

    /// Set the output format
    pub fn with_format(mut self, format: OutputFormat) -> Self {
        self.format = format;
        self
    }

    /// Add a custom template variable
    #[cfg(test)]
    pub fn with_var(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.vars.insert(key.into(), value.into());
        self
    }

    /// Add multiple template variables
    #[cfg(test)]
    pub fn with_vars(mut self, vars: HashMap<String, String>) -> Self {
        self.vars.extend(vars);
        self
    }
}

/// Artifact writer for persisting task outputs
#[derive(Debug)]
pub struct ArtifactWriter {
    /// Base directory for artifacts (validated)
    artifact_dir: PathBuf,
    /// Workflow name for template resolution
    workflow_name: String,
    /// Maximum artifact size in bytes
    max_size: u64,
}

impl ArtifactWriter {
    /// Create a new artifact writer
    ///
    /// # Arguments
    ///
    /// * `artifact_dir` - Base directory for artifacts (must be absolute)
    /// * `workflow_name` - Workflow name for template variables
    ///
    /// # Errors
    ///
    /// Returns `NikaError::ArtifactPathError` if the directory path is invalid
    pub fn new(artifact_dir: impl Into<PathBuf>, workflow_name: impl Into<String>) -> Self {
        Self {
            artifact_dir: artifact_dir.into(),
            workflow_name: workflow_name.into(),
            max_size: DEFAULT_MAX_SIZE,
        }
    }

    /// Set the maximum artifact size
    pub fn with_max_size(mut self, max_size: u64) -> Self {
        self.max_size = max_size;
        self
    }

    /// Write an artifact to disk
    ///
    /// This method:
    /// 1. Validates JSON format if `OutputFormat::Json`
    /// 2. Resolves template variables in the output path
    /// 3. Validates the path stays within the artifact directory
    /// 4. Validates the content size
    /// 5. Creates parent directories as needed
    /// 6. Writes atomically (temp + rename)
    /// 7. Final path validation before atomic commit
    ///
    /// # Arguments
    ///
    /// * `request` - The write request with content and metadata
    ///
    /// # Returns
    ///
    /// `WriteResult` with the final path and metadata
    ///
    /// # Errors
    ///
    /// - `NikaError::ArtifactPathError` if path validation fails
    /// - `NikaError::ArtifactSizeExceeded` if content exceeds max_size
    /// - `NikaError::ArtifactWriteError` if write fails or JSON is invalid
    pub async fn write(&self, request: WriteRequest) -> Result<WriteResult, NikaError> {
        // Check size limit
        let content_size = request.content.len() as u64;
        if content_size > self.max_size {
            return Err(NikaError::ArtifactSizeExceeded {
                path: request.output_path.clone(),
                size: content_size,
                max_size: self.max_size,
            });
        }

        // Validate JSON format if specified
        if matches!(request.format, OutputFormat::Json) && !request.content.is_empty() {
            if let Err(e) = serde_json::from_str::<serde_json::Value>(&request.content) {
                return Err(NikaError::ArtifactWriteError {
                    path: request.output_path.clone(),
                    reason: format!("Invalid JSON content: {}", e),
                });
            }
        }

        // Resolve template variables (validates custom vars for path traversal)
        let resolver = TemplateResolver::new(&request.task_id, &self.workflow_name)
            .with_vars(request.vars.clone())?;
        let resolved_path = resolver.resolve(&request.output_path)?;

        // Validate the path stays within artifact directory
        let full_path = validate_artifact_path(&self.artifact_dir, Path::new(&resolved_path))?;

        // Create parent directories
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| NikaError::ArtifactWriteError {
                    path: parent.display().to_string(),
                    reason: format!("Failed to create parent directories: {}", e),
                })?;
        }

        // Final path validation after directory creation (mitigates TOCTOU)
        // Re-validate now that parent directories exist and can be canonicalized
        let final_path = validate_artifact_path(&self.artifact_dir, Path::new(&resolved_path))?;

        // Write atomically
        write_atomic(&final_path, request.content.as_bytes())
            .await
            .map_err(|e| NikaError::ArtifactWriteError {
                path: final_path.display().to_string(),
                reason: format!("Atomic write failed: {}", e),
            })?;

        Ok(WriteResult {
            path: final_path,
            size: content_size,
            format: request.format,
        })
    }

    /// Validate a path without writing
    ///
    /// Useful for pre-validation before expensive operations
    pub fn validate_path(&self, task_id: &str, output_path: &str) -> Result<PathBuf, NikaError> {
        let resolver = TemplateResolver::new(task_id, &self.workflow_name);
        let resolved_path = resolver.resolve(output_path)?;
        validate_artifact_path(&self.artifact_dir, Path::new(&resolved_path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_writer() -> (ArtifactWriter, tempfile::TempDir) {
        let temp = tempdir().unwrap();
        let artifact_dir = temp.path().join("artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        let canonical_dir = artifact_dir.canonicalize().unwrap();
        let writer = ArtifactWriter::new(canonical_dir, "test-workflow");
        (writer, temp)
    }

    #[tokio::test]
    async fn test_write_simple() {
        let (writer, _temp) = test_writer();
        let request = WriteRequest::new("task1", "output.json")
            .with_content(r#"{"key": "value"}"#)
            .with_format(OutputFormat::Json);

        let result = writer.write(request).await.unwrap();
        assert!(result.path.ends_with("output.json"));
        assert_eq!(result.size, 16);
        assert!(matches!(result.format, OutputFormat::Json));
    }

    #[tokio::test]
    async fn test_write_with_template() {
        let (writer, _temp) = test_writer();
        let request = WriteRequest::new("generate_page", "{{task_id}}/output.json")
            .with_content("test content");

        let result = writer.write(request).await.unwrap();
        assert!(result.path.to_string_lossy().contains("generate_page"));
    }

    #[tokio::test]
    async fn test_write_nested_path() {
        let (writer, _temp) = test_writer();
        let request =
            WriteRequest::new("task1", "deep/nested/path/output.txt").with_content("hello");

        let result = writer.write(request).await.unwrap();
        assert!(result.path.ends_with("deep/nested/path/output.txt"));
    }

    #[tokio::test]
    async fn test_write_size_exceeded() {
        let temp = tempdir().unwrap();
        let artifact_dir = temp.path().join("artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        let canonical_dir = artifact_dir.canonicalize().unwrap();

        let writer = ArtifactWriter::new(canonical_dir, "test").with_max_size(10);
        let request = WriteRequest::new("task1", "output.txt")
            .with_content("this content is longer than 10 bytes");

        let result = writer.write(request).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, NikaError::ArtifactSizeExceeded { .. }));
    }

    #[tokio::test]
    async fn test_write_path_traversal_blocked() {
        let (writer, _temp) = test_writer();
        let request =
            WriteRequest::new("task1", "../../../etc/passwd").with_content("malicious content");

        let result = writer.write(request).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, NikaError::ArtifactPathError { .. }));
    }

    #[tokio::test]
    async fn test_write_absolute_path_blocked() {
        let (writer, _temp) = test_writer();
        let request = WriteRequest::new("task1", "/etc/passwd").with_content("test");

        let result = writer.write(request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_write_custom_vars() {
        let (writer, _temp) = test_writer();
        // Note: custom vars cannot contain '/' - use separate path segments in template
        let request = WriteRequest::new("task1", "locales/{{locale}}/{{entity}}.json")
            .with_content("{}")
            .with_var("locale", "fr-FR")
            .with_var("entity", "qr-code");

        let result = writer.write(request).await.unwrap();
        assert!(result.path.to_string_lossy().contains("fr-FR"));
        assert!(result.path.to_string_lossy().contains("qr-code"));
    }

    #[tokio::test]
    async fn test_write_invalid_json_rejected() {
        let (writer, _temp) = test_writer();
        let request = WriteRequest::new("task1", "output.json")
            .with_content("{ invalid json }")
            .with_format(OutputFormat::Json);

        let result = writer.write(request).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        if let NikaError::ArtifactWriteError { reason, .. } = err {
            assert!(reason.contains("Invalid JSON"));
        } else {
            panic!("Expected ArtifactWriteError");
        }
    }

    #[tokio::test]
    async fn test_write_valid_json_accepted() {
        let (writer, _temp) = test_writer();
        let request = WriteRequest::new("task1", "output.json")
            .with_content(r#"{"valid": true, "nested": {"key": 123}}"#)
            .with_format(OutputFormat::Json);

        let result = writer.write(request).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_write_var_path_traversal_blocked() {
        let (writer, _temp) = test_writer();
        let request = WriteRequest::new("task1", "{{entity}}/output.json")
            .with_content("{}")
            .with_var("entity", "../escape");

        let result = writer.write(request).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, NikaError::TemplateError { .. }));
    }

    #[test]
    fn test_validate_path() {
        let (writer, _temp) = test_writer();
        let result = writer.validate_path("task1", "output.json");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_path_traversal() {
        let (writer, _temp) = test_writer();
        let result = writer.validate_path("task1", "../escape.txt");
        assert!(result.is_err());
    }

    #[test]
    fn test_writer_max_size() {
        let temp = tempdir().unwrap();
        let writer = ArtifactWriter::new(temp.path(), "test").with_max_size(1024);
        assert_eq!(writer.max_size, 1024);
    }

    #[test]
    fn test_write_request_builder() {
        let mut vars = HashMap::new();
        vars.insert("key1".to_string(), "val1".to_string());
        vars.insert("key2".to_string(), "val2".to_string());

        let request = WriteRequest::new("task", "path.txt")
            .with_content("content")
            .with_format(OutputFormat::Json)
            .with_vars(vars);

        assert_eq!(request.task_id, "task");
        assert_eq!(request.output_path, "path.txt");
        assert_eq!(request.content, "content");
        assert!(matches!(request.format, OutputFormat::Json));
        assert_eq!(request.vars.len(), 2);
    }
}
