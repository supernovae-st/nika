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

use crate::ast::OutputFormat;
use crate::error::NikaError;
use crate::io::atomic::write_atomic;
use crate::io::security::{validate_artifact_path, validate_canonicalized_boundary};
use crate::io::template::TemplateResolver;

/// Default maximum artifact size (100 MB) — matches nika-core DEFAULT_MAX_ARTIFACT_SIZE
pub const DEFAULT_MAX_SIZE: u64 = 100 * 1024 * 1024;

/// Source for binary artifact data
#[derive(Debug, Clone)]
pub(crate) enum BinarySource {
    /// Copy from a CAS store path
    CasPath(PathBuf),
}

/// Request to write a binary artifact
#[derive(Debug, Clone)]
pub(crate) struct BinaryWriteRequest {
    /// Task ID that produced this binary
    pub task_id: String,
    /// Output path template (may contain `{{var}}` placeholders)
    pub output_path: String,
    /// Source of binary data
    pub source: BinarySource,
    /// Expected file size (for size limit check before copy)
    pub expected_size: u64,
}

/// Result of a successful write operation
#[derive(Debug, Clone)]
pub struct WriteResult {
    /// Final resolved path where artifact was written
    pub path: PathBuf,
    /// Size in bytes of the written content
    pub size: u64,
    /// Format used for output
    pub format: OutputFormat,
    /// BLAKE3 content hash (hex-encoded with `blake3:` prefix).
    /// Present for text/json/markdown artifacts; binary artifacts use CAS hashes instead.
    pub checksum: Option<String>,
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

        // Post-creation symlink check: canonicalize parent (which now exists on
        // disk) to detect symlinks that escape the artifact boundary.
        let final_path = validate_artifact_path(&self.artifact_dir, Path::new(&resolved_path))?;
        if let Some(parent) = final_path.parent() {
            validate_canonicalized_boundary(&self.artifact_dir, parent).map_err(|e| {
                NikaError::ArtifactPathError {
                    path: final_path.display().to_string(),
                    reason: e.to_string(),
                }
            })?;
        }

        // Write atomically
        write_atomic(&final_path, request.content.as_bytes())
            .await
            .map_err(|e| NikaError::ArtifactWriteError {
                path: final_path.display().to_string(),
                reason: format!("Atomic write failed: {}", e),
            })?;

        let checksum = format!(
            "blake3:{}",
            blake3::hash(request.content.as_bytes()).to_hex()
        );

        Ok(WriteResult {
            path: final_path,
            size: content_size,
            format: request.format,
            checksum: Some(checksum),
        })
    }

    /// Write a binary artifact by copying from CAS store path.
    ///
    /// Reads CAS file data through `CasStore::read_raw()` which transparently
    /// strips compression framing, then writes the original user data to the
    /// artifact path using atomic temp+rename.
    ///
    /// # Arguments
    ///
    /// * `request` - Binary write request with source path and metadata
    ///
    /// # Errors
    ///
    /// - `NikaError::ArtifactSizeExceeded` if source exceeds max_size
    /// - `NikaError::ArtifactWriteError` if copy fails or source missing
    /// - `NikaError::ArtifactPathError` if output path validation fails
    pub(crate) async fn write_binary(
        &self,
        request: BinaryWriteRequest,
    ) -> Result<WriteResult, NikaError> {
        // Check size limit before copy
        if request.expected_size > self.max_size {
            return Err(NikaError::ArtifactSizeExceeded {
                path: request.output_path.clone(),
                size: request.expected_size,
                max_size: self.max_size,
            });
        }

        // Resolve template variables in output path
        let resolver = TemplateResolver::new(&request.task_id, &self.workflow_name);
        let resolved_path = resolver.resolve(&request.output_path)?;

        // Validate path stays within artifact directory
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

        // Post-creation symlink check: canonicalize parent (which now exists on
        // disk) to detect symlinks that escape the artifact boundary.
        let final_path = validate_artifact_path(&self.artifact_dir, Path::new(&resolved_path))?;
        if let Some(parent) = final_path.parent() {
            validate_canonicalized_boundary(&self.artifact_dir, parent).map_err(|e| {
                NikaError::ArtifactPathError {
                    path: final_path.display().to_string(),
                    reason: e.to_string(),
                }
            })?;
        }

        // Write binary from CAS source using atomic temp+rename pattern.
        //
        // CAS files may contain compression framing bytes (media-compression feature).
        // We read through CasStore::read_raw() which transparently decompresses,
        // then write the original user data to the artifact path. A raw file copy
        // would leak the framing byte prefix ([0x00] or [0x01][zstd...]) into
        // the output artifact.
        let size = match request.source {
            BinarySource::CasPath(ref src) => {
                // Read CAS file and strip compression framing if present.
                // This calls transparent_decompress under media-compression.
                let data = crate::media::CasStore::read_raw(src).await.map_err(|e| {
                    NikaError::ArtifactWriteError {
                        path: final_path.display().to_string(),
                        reason: format!("CAS read failed: {}", e),
                    }
                })?;

                let size = data.len() as u64;

                // Atomic write: temp file + rename
                write_atomic(&final_path, &data).await.map_err(|e| {
                    NikaError::ArtifactWriteError {
                        path: final_path.display().to_string(),
                        reason: format!("Atomic write failed: {}", e),
                    }
                })?;

                size
            }
        };

        Ok(WriteResult {
            path: final_path,
            size,
            format: OutputFormat::Binary,
            checksum: None, // Binary checksums use CAS hashes from MediaRef
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

    #[tokio::test]
    async fn test_write_binary_from_cas_path() {
        let temp = tempdir().unwrap();
        let artifact_dir = temp.path().join("artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        let canonical_dir = artifact_dir.canonicalize().unwrap();
        let writer = ArtifactWriter::new(canonical_dir, "test-workflow");

        // Create a fake CAS file
        let cas_dir = temp.path().join("cas");
        std::fs::create_dir_all(&cas_dir).unwrap();
        let cas_file = cas_dir.join("testfile");
        let data = b"binary image data here";
        std::fs::write(&cas_file, data).unwrap();

        let request = BinaryWriteRequest {
            task_id: "gen_img".to_string(),
            output_path: "images/result.bin".to_string(),
            source: BinarySource::CasPath(cas_file),
            expected_size: data.len() as u64,
        };

        let result = writer.write_binary(request).await.unwrap();
        assert!(result.path.ends_with("images/result.bin"));
        assert_eq!(result.size, data.len() as u64);

        // Verify file content
        let written = std::fs::read(&result.path).unwrap();
        assert_eq!(written, data);
    }

    #[tokio::test]
    async fn test_write_binary_always_overwrites() {
        // Binary artifacts always use overwrite semantics (no append mode)
        let temp = tempdir().unwrap();
        let artifact_dir = temp.path().join("artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        let canonical_dir = artifact_dir.canonicalize().unwrap();
        let writer = ArtifactWriter::new(canonical_dir, "test-workflow");

        let cas_file = temp.path().join("dummy");
        std::fs::write(&cas_file, b"test").unwrap();

        let request = BinaryWriteRequest {
            task_id: "task1".to_string(),
            output_path: "output.bin".to_string(),
            source: BinarySource::CasPath(cas_file),
            expected_size: 4,
        };

        // write_binary always overwrites — append is not supported for binary
        let result = writer.write_binary(request).await;
        assert!(
            result.is_ok(),
            "Binary write should succeed (overwrite mode)"
        );
    }

    #[tokio::test]
    async fn test_write_binary_size_limit() {
        let temp = tempdir().unwrap();
        let artifact_dir = temp.path().join("artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        let canonical_dir = artifact_dir.canonicalize().unwrap();
        let writer = ArtifactWriter::new(canonical_dir, "test-workflow").with_max_size(10);

        let cas_file = temp.path().join("bigfile");
        let data = vec![0u8; 100];
        std::fs::write(&cas_file, &data).unwrap();

        let request = BinaryWriteRequest {
            task_id: "task1".to_string(),
            output_path: "output.bin".to_string(),
            source: BinarySource::CasPath(cas_file),
            expected_size: 100,
        };

        let result = writer.write_binary(request).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, NikaError::ArtifactSizeExceeded { .. }));
    }

    #[tokio::test]
    async fn test_write_binary_missing_source() {
        let temp = tempdir().unwrap();
        let artifact_dir = temp.path().join("artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        let canonical_dir = artifact_dir.canonicalize().unwrap();
        let writer = ArtifactWriter::new(canonical_dir, "test-workflow");

        let request = BinaryWriteRequest {
            task_id: "task1".to_string(),
            output_path: "output.bin".to_string(),
            source: BinarySource::CasPath(PathBuf::from("/nonexistent/cas/file")),
            expected_size: 42,
        };

        let result = writer.write_binary(request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_write_binary_overwrite_existing() {
        let temp = tempdir().unwrap();
        let artifact_dir = temp.path().join("artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        let canonical_dir = artifact_dir.canonicalize().unwrap();
        let writer = ArtifactWriter::new(canonical_dir, "test-workflow");

        // Create CAS files with different content
        let cas_dir = temp.path().join("cas");
        std::fs::create_dir_all(&cas_dir).unwrap();
        let cas_file_v1 = cas_dir.join("v1");
        let cas_file_v2 = cas_dir.join("v2");
        std::fs::write(&cas_file_v1, b"version 1 data").unwrap();
        std::fs::write(&cas_file_v2, b"version 2 data -- longer").unwrap();

        // First write
        let request1 = BinaryWriteRequest {
            task_id: "task1".to_string(),
            output_path: "output.bin".to_string(),
            source: BinarySource::CasPath(cas_file_v1),
            expected_size: 14,
        };
        let result1 = writer.write_binary(request1).await.unwrap();
        assert_eq!(std::fs::read(&result1.path).unwrap(), b"version 1 data");

        // Second write to same path (overwrite) — must succeed
        let request2 = BinaryWriteRequest {
            task_id: "task1".to_string(),
            output_path: "output.bin".to_string(),
            source: BinarySource::CasPath(cas_file_v2),
            expected_size: 24,
        };
        let result2 = writer.write_binary(request2).await.unwrap();
        assert_eq!(
            std::fs::read(&result2.path).unwrap(),
            b"version 2 data -- longer",
            "Overwrite should replace file content"
        );
    }

    #[tokio::test]
    async fn test_write_binary_format_is_binary() {
        let temp = tempdir().unwrap();
        let artifact_dir = temp.path().join("artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        let canonical_dir = artifact_dir.canonicalize().unwrap();
        let writer = ArtifactWriter::new(canonical_dir, "test-workflow");

        let cas_file = temp.path().join("data");
        std::fs::write(&cas_file, b"test").unwrap();

        let request = BinaryWriteRequest {
            task_id: "task1".to_string(),
            output_path: "output.bin".to_string(),
            source: BinarySource::CasPath(cas_file),
            expected_size: 4,
        };

        let result = writer.write_binary(request).await.unwrap();
        assert_eq!(
            result.format,
            OutputFormat::Binary,
            "Binary write should report Binary format"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // SECURITY: Binary artifact output path traversal tests
    // Verify that write_binary blocks output paths containing ".."
    // or absolute paths, matching the same protections as text write.
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_write_binary_output_path_traversal_blocked() {
        let temp = tempdir().unwrap();
        let artifact_dir = temp.path().join("artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        let canonical_dir = artifact_dir.canonicalize().unwrap();
        let writer = ArtifactWriter::new(canonical_dir, "test-workflow");

        let cas_file = temp.path().join("data");
        std::fs::write(&cas_file, b"secret").unwrap();

        let request = BinaryWriteRequest {
            task_id: "task1".to_string(),
            output_path: "../../../etc/shadow".to_string(),
            source: BinarySource::CasPath(cas_file),
            expected_size: 6,
        };

        let result = writer.write_binary(request).await;
        assert!(
            result.is_err(),
            "Path traversal in binary output must be blocked"
        );
        let err = result.unwrap_err();
        assert!(
            matches!(err, NikaError::ArtifactPathError { .. }),
            "Expected ArtifactPathError, got: {:?}",
            err
        );
    }

    #[tokio::test]
    async fn test_write_binary_output_absolute_path_blocked() {
        let temp = tempdir().unwrap();
        let artifact_dir = temp.path().join("artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        let canonical_dir = artifact_dir.canonicalize().unwrap();
        let writer = ArtifactWriter::new(canonical_dir, "test-workflow");

        let cas_file = temp.path().join("data");
        std::fs::write(&cas_file, b"test").unwrap();

        let request = BinaryWriteRequest {
            task_id: "task1".to_string(),
            output_path: "/tmp/escape.bin".to_string(),
            source: BinarySource::CasPath(cas_file),
            expected_size: 4,
        };

        let result = writer.write_binary(request).await;
        assert!(
            result.is_err(),
            "Absolute output path in binary write must be blocked"
        );
    }

    #[tokio::test]
    async fn test_write_binary_output_hidden_traversal_blocked() {
        let temp = tempdir().unwrap();
        let artifact_dir = temp.path().join("artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        let canonical_dir = artifact_dir.canonicalize().unwrap();
        let writer = ArtifactWriter::new(canonical_dir, "test-workflow");

        let cas_file = temp.path().join("data");
        std::fs::write(&cas_file, b"test").unwrap();

        let request = BinaryWriteRequest {
            task_id: "task1".to_string(),
            output_path: "a/../../escape.bin".to_string(),
            source: BinarySource::CasPath(cas_file),
            expected_size: 4,
        };

        let result = writer.write_binary(request).await;
        assert!(
            result.is_err(),
            "Hidden traversal in binary output path must be blocked"
        );
    }

    #[tokio::test]
    async fn test_write_binary_output_null_byte_blocked() {
        let temp = tempdir().unwrap();
        let artifact_dir = temp.path().join("artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        let canonical_dir = artifact_dir.canonicalize().unwrap();
        let writer = ArtifactWriter::new(canonical_dir, "test-workflow");

        let cas_file = temp.path().join("data");
        std::fs::write(&cas_file, b"test").unwrap();

        let request = BinaryWriteRequest {
            task_id: "task1".to_string(),
            output_path: "output\0.bin".to_string(),
            source: BinarySource::CasPath(cas_file),
            expected_size: 4,
        };

        let result = writer.write_binary(request).await;
        assert!(
            result.is_err(),
            "Null bytes in binary output path must be blocked"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // SECURITY: Binary artifact source path validation tests
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_write_binary_source_outside_cas_fails_gracefully() {
        let temp = tempdir().unwrap();
        let artifact_dir = temp.path().join("artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        let canonical_dir = artifact_dir.canonicalize().unwrap();
        let writer = ArtifactWriter::new(canonical_dir, "test-workflow");

        let request = BinaryWriteRequest {
            task_id: "task1".to_string(),
            output_path: "output.bin".to_string(),
            source: BinarySource::CasPath(PathBuf::from("/nonexistent/fake/cas/ab/cdef")),
            expected_size: 42,
        };

        let result = writer.write_binary(request).await;
        assert!(
            result.is_err(),
            "Missing CAS source file must produce an error"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_write_binary_symlink_source_reads_target() {
        // Verify behavior when CAS path is a symlink: reflink_or_copy follows
        // symlinks by default. This test documents the behavior.
        use std::os::unix::fs::symlink;

        let temp = tempdir().unwrap();
        let artifact_dir = temp.path().join("artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        let canonical_dir = artifact_dir.canonicalize().unwrap();
        let writer = ArtifactWriter::new(canonical_dir, "test-workflow");

        let real_file = temp.path().join("real_data");
        std::fs::write(&real_file, b"real content").unwrap();
        let symlink_path = temp.path().join("cas_symlink");
        symlink(&real_file, &symlink_path).unwrap();

        let request = BinaryWriteRequest {
            task_id: "task1".to_string(),
            output_path: "output.bin".to_string(),
            source: BinarySource::CasPath(symlink_path),
            expected_size: 12,
        };

        let result = writer.write_binary(request).await.unwrap();
        let written = std::fs::read(&result.path).unwrap();
        assert_eq!(
            written, b"real content",
            "Symlink source is followed by reflink_or_copy (documented behavior)"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_write_binary_symlink_in_output_parent_dir() {
        // Security test: if a parent directory in the output path is a symlink
        // that points outside the artifact directory, validate_artifact_path
        // uses normalize_path (logical, not filesystem), so it cannot detect this.
        // This test documents the known limitation.
        use std::os::unix::fs::symlink;

        let temp = tempdir().unwrap();
        let artifact_dir = temp.path().join("artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        let canonical_dir = artifact_dir.canonicalize().unwrap();
        let writer = ArtifactWriter::new(&canonical_dir, "test-workflow");

        let cas_file = temp.path().join("data");
        std::fs::write(&cas_file, b"test data").unwrap();

        // Create a symlink inside the artifact dir that points outside
        let escape_target = temp.path().join("escape_target");
        std::fs::create_dir_all(&escape_target).unwrap();
        let symlink_dir = canonical_dir.join("evil_link");
        symlink(&escape_target, &symlink_dir).unwrap();

        let request = BinaryWriteRequest {
            task_id: "task1".to_string(),
            output_path: "evil_link/file.bin".to_string(),
            source: BinarySource::CasPath(cas_file),
            expected_size: 9,
        };

        // Symlink escape is now BLOCKED by post-mkdir canonicalize check.
        let result = writer.write_binary(request).await;
        assert!(
            result.is_err(),
            "Symlink escape should be blocked by canonicalize check"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("traversal") || err.contains("escape"),
            "Error should mention traversal/escape, got: {}",
            err
        );

        let escaped_file = escape_target.join("file.bin");
        assert!(
            !escaped_file.exists(),
            "File should NOT be written through symlink escape"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Edge case tests for reflink-copy behavior
    // ═══════════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_write_binary_empty_source_file() {
        // Edge case: source file is 0 bytes
        // reflink_or_copy should handle this gracefully
        let temp = tempdir().unwrap();
        let artifact_dir = temp.path().join("artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        let canonical_dir = artifact_dir.canonicalize().unwrap();
        let writer = ArtifactWriter::new(canonical_dir, "test-workflow");

        let cas_file = temp.path().join("empty");
        std::fs::write(&cas_file, b"").unwrap();

        let request = BinaryWriteRequest {
            task_id: "task1".to_string(),
            output_path: "output.bin".to_string(),
            source: BinarySource::CasPath(cas_file),
            expected_size: 0,
        };

        let result = writer.write_binary(request).await.unwrap();
        assert_eq!(result.size, 0, "Empty source should produce empty output");

        let content = std::fs::read(&result.path).unwrap();
        assert!(content.is_empty(), "Output file should be empty");
    }

    #[tokio::test]
    async fn test_write_binary_source_is_symlink() {
        // Edge case: source file is a symlink — reflink should follow it
        // On macOS, clonefile handles symlinks. The fallback fs::copy also follows symlinks.
        let temp = tempdir().unwrap();
        let artifact_dir = temp.path().join("artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        let canonical_dir = artifact_dir.canonicalize().unwrap();
        let writer = ArtifactWriter::new(canonical_dir, "test-workflow");

        // Create real file and a symlink to it
        let real_file = temp.path().join("real_data");
        std::fs::write(&real_file, b"symlink target data").unwrap();

        let symlink_path = temp.path().join("link_to_real");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real_file, &symlink_path).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&real_file, &symlink_path).unwrap();

        let request = BinaryWriteRequest {
            task_id: "task1".to_string(),
            output_path: "from_symlink.bin".to_string(),
            source: BinarySource::CasPath(symlink_path),
            expected_size: 19,
        };

        let result = writer.write_binary(request).await.unwrap();
        let content = std::fs::read(&result.path).unwrap();
        assert_eq!(
            content, b"symlink target data",
            "Should copy the symlink target content"
        );
        assert_eq!(result.size, 19);
    }

    #[tokio::test]
    async fn test_write_binary_source_deleted_before_copy() {
        // Edge case: CAS file deleted between request creation and copy execution
        let temp = tempdir().unwrap();
        let artifact_dir = temp.path().join("artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        let canonical_dir = artifact_dir.canonicalize().unwrap();
        let writer = ArtifactWriter::new(canonical_dir, "test-workflow");

        // Create then immediately delete the source file
        let cas_file = temp.path().join("ephemeral");
        std::fs::write(&cas_file, b"will be deleted").unwrap();
        std::fs::remove_file(&cas_file).unwrap();

        let request = BinaryWriteRequest {
            task_id: "task1".to_string(),
            output_path: "output.bin".to_string(),
            source: BinarySource::CasPath(cas_file),
            expected_size: 15,
        };

        let result = writer.write_binary(request).await;
        assert!(result.is_err(), "Should fail when source file is missing");

        let err = result.unwrap_err();
        if let NikaError::ArtifactWriteError { reason, .. } = &err {
            assert!(
                reason.contains("CAS read failed"),
                "Error should mention CAS read failure: {}",
                reason
            );
        } else {
            panic!("Expected ArtifactWriteError, got: {:?}", err);
        }
    }

    #[cfg(unix)] // Uses unix-specific PermissionsExt::from_mode()
    #[tokio::test]
    async fn test_write_binary_read_only_source_works() {
        // Edge case: source file is read-only — copy should still work
        let temp = tempdir().unwrap();
        let artifact_dir = temp.path().join("artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        let canonical_dir = artifact_dir.canonicalize().unwrap();
        let writer = ArtifactWriter::new(canonical_dir, "test-workflow");

        let cas_file = temp.path().join("readonly");
        std::fs::write(&cas_file, b"read-only content").unwrap();

        // Make the source file read-only
        let mut perms = std::fs::metadata(&cas_file).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&cas_file, perms).unwrap();

        let request = BinaryWriteRequest {
            task_id: "task1".to_string(),
            output_path: "output.bin".to_string(),
            source: BinarySource::CasPath(cas_file.clone()),
            expected_size: 17,
        };

        let result = writer.write_binary(request).await.unwrap();
        assert_eq!(result.size, 17);
        let content = std::fs::read(&result.path).unwrap();
        assert_eq!(content, b"read-only content");

        // Restore permissions for cleanup
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o644);
        std::fs::set_permissions(&cas_file, perms).unwrap();
    }

    #[tokio::test]
    async fn test_write_binary_no_partial_file_on_missing_source() {
        // Verify that when reflink_or_copy fails, no partial destination file is left
        let temp = tempdir().unwrap();
        let artifact_dir = temp.path().join("artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        let canonical_dir = artifact_dir.canonicalize().unwrap();
        let writer = ArtifactWriter::new(&canonical_dir, "test-workflow");

        let request = BinaryWriteRequest {
            task_id: "task1".to_string(),
            output_path: "should_not_exist.bin".to_string(),
            source: BinarySource::CasPath(PathBuf::from("/nonexistent/path/file")),
            expected_size: 100,
        };

        let _ = writer.write_binary(request).await;

        // Verify no partial file was left at the destination
        let ghost = canonical_dir.join("should_not_exist.bin");
        assert!(
            !ghost.exists(),
            "Failed copy should not leave a partial file at: {}",
            ghost.display()
        );
    }

    /// Stress test: 10 concurrent binary writes to the SAME output path.
    ///
    /// Verifies the retry loop handles the TOCTOU race between remove_file
    /// and reflink_or_copy (which uses create_new / O_EXCL internally).
    /// All writes must succeed and the final file must be consistent.
    #[tokio::test]
    async fn test_write_binary_concurrent_same_path() {
        use std::sync::Arc as StdArc;
        use tokio::task::JoinSet;

        let temp = tempdir().unwrap();
        let artifact_dir = temp.path().join("artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        let canonical_dir = artifact_dir.canonicalize().unwrap();
        let writer = StdArc::new(ArtifactWriter::new(&canonical_dir, "test-workflow"));

        // Create 10 distinct CAS source files with identifiable content
        let cas_dir = temp.path().join("cas");
        std::fs::create_dir_all(&cas_dir).unwrap();

        let mut cas_files = Vec::new();
        for i in 0..10u8 {
            let cas_file = cas_dir.join(format!("source_{}", i));
            // Each file has unique content: 1KB of repeated byte value
            let data = vec![i; 1024];
            std::fs::write(&cas_file, &data).unwrap();
            cas_files.push(cas_file);
        }

        // Spawn 10 concurrent write_binary calls to the SAME output path
        let mut join_set = JoinSet::new();
        for (i, cas_file) in cas_files.into_iter().enumerate() {
            let writer = StdArc::clone(&writer);
            join_set.spawn(async move {
                let request = BinaryWriteRequest {
                    task_id: format!("task_{}", i),
                    output_path: "shared_output.bin".to_string(),
                    source: BinarySource::CasPath(cas_file),
                    expected_size: 1024,
                };
                (i, writer.write_binary(request).await)
            });
        }

        // Collect results — all should succeed (no errors)
        let mut successes = 0;
        let mut errors = 0;
        while let Some(join_result) = join_set.join_next().await {
            let (idx, write_result) = join_result.expect("task should not panic");
            match write_result {
                Ok(result) => {
                    assert_eq!(result.size, 1024, "task_{} wrote wrong size", idx);
                    successes += 1;
                }
                Err(e) => {
                    eprintln!("task_{} failed: {}", idx, e);
                    errors += 1;
                }
            }
        }

        // ALL writes must succeed — no TOCTOU errors
        assert_eq!(
            errors, 0,
            "All concurrent binary writes should succeed, but {} failed",
            errors
        );
        assert_eq!(successes, 10, "All 10 writes should succeed");

        // The final file must contain valid data (1024 bytes of a single repeated value)
        let final_path = canonical_dir.join("shared_output.bin");
        let content = std::fs::read(&final_path).unwrap();
        assert_eq!(content.len(), 1024, "File should be exactly 1024 bytes");

        // All bytes should be the same value (from one consistent writer)
        let first_byte = content[0];
        assert!(
            content.iter().all(|&b| b == first_byte),
            "File content should be consistent (all bytes from one writer), \
             but found mixed data — indicates corruption from concurrent writes"
        );
    }
}
