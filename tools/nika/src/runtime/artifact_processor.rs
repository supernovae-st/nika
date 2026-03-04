//! Artifact Processor - Write task outputs to disk (v0.18)
//!
//! Integrates the artifact system with the task execution flow.
//! Called after successful task completion when `artifact:` is configured.

use std::path::PathBuf;
use std::sync::Arc;

use tracing::{debug, warn};

use crate::ast::artifact::{ArtifactFormat, ArtifactMode, ArtifactOutput, ArtifactSpec, ArtifactsConfig};
use crate::error::NikaError;
use crate::event::{EventKind, EventLog};
use crate::io::security::DEFAULT_ARTIFACT_DIR;
use crate::io::writer::{ArtifactWriter, WriteRequest, WriteResult};
use crate::io::atomic::{write_append, write_fail, write_unique};
use crate::serde_yaml;
use crate::OutputFormat;

/// Result of processing artifacts for a task
#[derive(Debug, Clone)]
pub struct ArtifactProcessResult {
    /// Number of artifacts successfully written
    pub written: usize,
    /// Paths of written artifacts
    pub paths: Vec<PathBuf>,
    /// Any errors that occurred (non-fatal)
    pub errors: Vec<String>,
}

/// Process artifacts for a completed task
///
/// # Arguments
///
/// * `task_id` - The task ID
/// * `output` - The task output as a string
/// * `artifact_spec` - Task-level artifact configuration
/// * `workflow_config` - Workflow-level artifact defaults
/// * `base_path` - Base path for artifact resolution (workflow directory)
/// * `event_log` - Optional event log for emitting artifact events
///
/// # Returns
///
/// `ArtifactProcessResult` with write status and any errors
pub async fn process_task_artifacts(
    task_id: &str,
    output: &str,
    artifact_spec: &ArtifactSpec,
    workflow_config: Option<&ArtifactsConfig>,
    base_path: &std::path::Path,
    event_log: Option<&EventLog>,
) -> ArtifactProcessResult {
    let mut result = ArtifactProcessResult {
        written: 0,
        paths: Vec::new(),
        errors: Vec::new(),
    };

    // Get the artifact outputs to write based on spec type
    let outputs = match artifact_spec {
        ArtifactSpec::Enabled(false) => {
            // Artifacts disabled for this task
            return result;
        }
        ArtifactSpec::Enabled(true) => {
            // Use defaults - generate single output with task_id as filename
            let format = workflow_config
                .map(|c| &c.format)
                .unwrap_or(&ArtifactFormat::Text);
            vec![ArtifactOutput {
                path: format!("{}.{}", task_id, format.extension()),
                source: None,
                format: Some(*format),
                mode: workflow_config.map(|c| c.mode),
            }]
        }
        ArtifactSpec::Single(output_spec) => {
            vec![output_spec.clone()]
        }
        ArtifactSpec::Multiple(outputs) => {
            outputs.clone()
        }
    };

    // Resolve artifact directory
    let artifact_dir = resolve_artifact_dir(workflow_config, base_path);

    // Get max size from workflow config
    let max_size = workflow_config
        .map(|c| c.max_size)
        .unwrap_or(crate::ast::artifact::DEFAULT_MAX_ARTIFACT_SIZE);

    // Create artifact writer
    let writer = ArtifactWriter::new(&artifact_dir, task_id)
        .with_max_size(max_size);

    // Process each output
    for output_spec in outputs {
        match write_single_artifact(
            task_id,
            output,
            &output_spec,
            workflow_config,
            &writer,
        ).await {
            Ok(write_result) => {
                debug!(
                    task_id = %task_id,
                    path = %write_result.path.display(),
                    size = write_result.size,
                    "Artifact written"
                );

                // Emit ArtifactWritten event if event_log provided
                if let Some(log) = event_log {
                    log.emit(EventKind::ArtifactWritten {
                        task_id: Arc::from(task_id),
                        path: write_result.path.display().to_string(),
                        size: write_result.size,
                        format: format!("{:?}", write_result.format).to_lowercase(),
                    });
                }

                result.written += 1;
                result.paths.push(write_result.path);
            }
            Err(e) => {
                warn!(
                    task_id = %task_id,
                    path = %output_spec.path,
                    error = %e,
                    "Failed to write artifact"
                );

                // Emit ArtifactFailed event if event_log provided
                if let Some(log) = event_log {
                    log.emit(EventKind::ArtifactFailed {
                        task_id: Arc::from(task_id),
                        path: output_spec.path.clone(),
                        reason: e.to_string(),
                    });
                }

                result.errors.push(format!("{}: {}", output_spec.path, e));
            }
        }
    }

    result
}

/// Write a single artifact output
async fn write_single_artifact(
    task_id: &str,
    output: &str,
    output_spec: &ArtifactOutput,
    workflow_config: Option<&ArtifactsConfig>,
    writer: &ArtifactWriter,
) -> Result<WriteResult, NikaError> {
    // Determine format (task spec > workflow default)
    let format = output_spec.format
        .or(workflow_config.map(|c| c.format))
        .unwrap_or(ArtifactFormat::Text);

    // Determine mode (task spec > workflow default)
    let mode = output_spec.mode
        .or(workflow_config.map(|c| c.mode))
        .unwrap_or(ArtifactMode::Overwrite);

    // Convert content based on format
    let content = format_output(output, format)?;

    // Convert ArtifactFormat to OutputFormat for the writer
    let output_format = match format {
        ArtifactFormat::Text => OutputFormat::Text,
        ArtifactFormat::Json => OutputFormat::Json,
        ArtifactFormat::Yaml => OutputFormat::Text, // YAML treated as text for validation
    };

    // Build write request - we need to keep output_format for WriteResult
    let request = WriteRequest::new(task_id, &output_spec.path)
        .with_content(content)
        .with_format(output_format.clone());

    // Handle different write modes
    match mode {
        ArtifactMode::Overwrite => {
            writer.write(request).await
        }
        ArtifactMode::Append => {
            // For append mode, we need to use atomic append
            let resolved_path = writer.validate_path(task_id, &output_spec.path)?;
            write_append(&resolved_path, request.content.as_bytes()).await
                .map_err(|e| NikaError::ArtifactWriteError {
                    path: resolved_path.display().to_string(),
                    reason: format!("Append failed: {}", e),
                })?;
            Ok(WriteResult {
                path: resolved_path,
                size: request.content.len() as u64,
                format: output_format.clone(),
            })
        }
        ArtifactMode::Unique => {
            // For unique mode, generate unique filename
            let resolved_path = writer.validate_path(task_id, &output_spec.path)?;
            let unique_path = write_unique(&resolved_path, request.content.as_bytes()).await
                .map_err(|e| NikaError::ArtifactWriteError {
                    path: resolved_path.display().to_string(),
                    reason: format!("Unique write failed: {}", e),
                })?;
            Ok(WriteResult {
                path: unique_path,
                size: request.content.len() as u64,
                format: output_format.clone(),
            })
        }
        ArtifactMode::Fail => {
            // For fail mode, error if file exists
            let resolved_path = writer.validate_path(task_id, &output_spec.path)?;
            write_fail(&resolved_path, request.content.as_bytes()).await
                .map_err(|e| NikaError::ArtifactWriteError {
                    path: resolved_path.display().to_string(),
                    reason: format!("Write failed (file may exist): {}", e),
                })?;
            Ok(WriteResult {
                path: resolved_path,
                size: request.content.len() as u64,
                format: output_format.clone(),
            })
        }
    }
}

/// Format output content based on artifact format
fn format_output(output: &str, format: ArtifactFormat) -> Result<String, NikaError> {
    match format {
        ArtifactFormat::Text => Ok(output.to_string()),
        ArtifactFormat::Json => {
            // Try to parse as JSON and pretty-print
            match serde_json::from_str::<serde_json::Value>(output) {
                Ok(value) => {
                    serde_json::to_string_pretty(&value)
                        .map_err(|e| NikaError::ArtifactWriteError {
                            path: "".to_string(),
                            reason: format!("Failed to format JSON: {}", e),
                        })
                }
                Err(_) => {
                    // If not valid JSON, wrap as string
                    Ok(serde_json::to_string_pretty(&output)
                        .unwrap_or_else(|_| format!("\"{}\"", output)))
                }
            }
        }
        ArtifactFormat::Yaml => {
            // Try to parse as JSON first, then convert to YAML
            match serde_json::from_str::<serde_json::Value>(output) {
                Ok(value) => {
                    serde_yaml::to_string(&value)
                        .map_err(|e| NikaError::ArtifactWriteError {
                            path: "".to_string(),
                            reason: format!("Failed to format YAML: {}", e),
                        })
                }
                Err(_) => {
                    // If not valid JSON, just use as-is
                    Ok(output.to_string())
                }
            }
        }
    }
}

/// Resolve artifact directory from workflow config
///
/// Creates the directory if it doesn't exist and canonicalizes the path
/// to avoid macOS symlink issues (e.g., /var -> /private/var).
fn resolve_artifact_dir(
    workflow_config: Option<&ArtifactsConfig>,
    base_path: &std::path::Path,
) -> PathBuf {
    let dir_str = workflow_config
        .and_then(|c| c.dir.as_deref())
        .unwrap_or(DEFAULT_ARTIFACT_DIR);

    let artifact_dir = base_path.join(dir_str);

    // Create directory if it doesn't exist
    if !artifact_dir.exists() {
        if let Err(e) = std::fs::create_dir_all(&artifact_dir) {
            tracing::warn!(
                path = %artifact_dir.display(),
                error = %e,
                "Failed to create artifact directory"
            );
            return artifact_dir;
        }
    }

    // Canonicalize to resolve symlinks (important for macOS /var -> /private/var)
    artifact_dir.canonicalize().unwrap_or(artifact_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_format_output_text() {
        let result = format_output("hello world", ArtifactFormat::Text);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "hello world");
    }

    #[test]
    fn test_format_output_json_valid() {
        let result = format_output(r#"{"key":"value"}"#, ArtifactFormat::Json);
        assert!(result.is_ok());
        let formatted = result.unwrap();
        assert!(formatted.contains("key"));
        assert!(formatted.contains("value"));
    }

    #[test]
    fn test_format_output_json_invalid() {
        let result = format_output("not json", ArtifactFormat::Json);
        assert!(result.is_ok());
        // Should be wrapped as JSON string
        let formatted = result.unwrap();
        assert!(formatted.contains("not json"));
    }

    #[test]
    fn test_format_output_yaml() {
        let result = format_output(r#"{"key":"value"}"#, ArtifactFormat::Yaml);
        assert!(result.is_ok());
        let formatted = result.unwrap();
        assert!(formatted.contains("key"));
    }

    #[test]
    fn test_resolve_artifact_dir_default() {
        let base = PathBuf::from("/project");
        let dir = resolve_artifact_dir(None, &base);
        assert_eq!(dir, PathBuf::from("/project/.nika/artifacts"));
    }

    #[test]
    fn test_resolve_artifact_dir_custom() {
        let base = PathBuf::from("/project");
        let config = ArtifactsConfig {
            dir: Some("output".to_string()),
            ..Default::default()
        };
        let dir = resolve_artifact_dir(Some(&config), &base);
        assert_eq!(dir, PathBuf::from("/project/output"));
    }

    #[tokio::test]
    async fn test_process_task_artifacts_disabled() {
        let base = tempdir().unwrap();
        let result = process_task_artifacts(
            "task1",
            "output",
            &ArtifactSpec::Enabled(false),
            None,
            base.path(),
            None, // No event log for tests
        ).await;

        assert_eq!(result.written, 0);
        assert!(result.paths.is_empty());
        assert!(result.errors.is_empty());
    }

    #[tokio::test]
    async fn test_process_task_artifacts_enabled() {
        let base = tempdir().unwrap();
        let artifact_dir = base.path().join(".nika/artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();

        let result = process_task_artifacts(
            "task1",
            "test output",
            &ArtifactSpec::Enabled(true),
            None,
            base.path(),
            None, // No event log for tests
        ).await;

        // Print errors for debugging
        if !result.errors.is_empty() {
            eprintln!("Artifact errors: {:?}", result.errors);
        }

        assert_eq!(result.written, 1, "Expected 1 artifact written, errors: {:?}", result.errors);
        assert!(!result.paths.is_empty());
        assert!(result.errors.is_empty(), "Unexpected errors: {:?}", result.errors);
    }

    #[tokio::test]
    async fn test_process_task_artifacts_single() {
        let base = tempdir().unwrap();
        let artifact_dir = base.path().join(".nika/artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();

        let spec = ArtifactSpec::Single(ArtifactOutput {
            path: "output.json".to_string(),
            source: None,
            format: Some(ArtifactFormat::Json),
            mode: None,
        });

        let result = process_task_artifacts(
            "task1",
            r#"{"result": "success"}"#,
            &spec,
            None,
            base.path(),
            None, // No event log for tests
        ).await;

        assert_eq!(result.written, 1);
        assert!(result.paths[0].ends_with("output.json"));
    }

    #[tokio::test]
    async fn test_process_task_artifacts_multiple() {
        let base = tempdir().unwrap();
        let artifact_dir = base.path().join(".nika/artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();

        let spec = ArtifactSpec::Multiple(vec![
            ArtifactOutput {
                path: "raw.txt".to_string(),
                source: None,
                format: Some(ArtifactFormat::Text),
                mode: None,
            },
            ArtifactOutput {
                path: "processed.json".to_string(),
                source: None,
                format: Some(ArtifactFormat::Json),
                mode: None,
            },
        ]);

        let result = process_task_artifacts(
            "task1",
            "test data",
            &spec,
            None,
            base.path(),
            None, // No event log for tests
        ).await;

        assert_eq!(result.written, 2);
        assert_eq!(result.paths.len(), 2);
    }
}
