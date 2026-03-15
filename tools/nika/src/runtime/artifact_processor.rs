//! Artifact Processor - Write task outputs to disk
//!
//! Integrates the artifact system with the task execution flow.
//! Called after successful task completion when `artifact:` is configured.
//!
//! Supports template-based artifacts via the `template:` field
//! which supports `{{use.*}}` bindings for dynamic content generation.

use std::path::PathBuf;
use std::sync::Arc;

use tracing::{debug, warn};

use crate::ast::artifact::{
    ArtifactFormat, ArtifactMode, ArtifactOutput, ArtifactSpec, ArtifactsConfig,
};
use crate::binding::{template_resolve, ResolvedBindings};
use crate::error::NikaError;
use crate::event::{EventKind, EventLog};
use crate::io::atomic::{write_append, write_fail, write_unique};
use crate::io::security::DEFAULT_ARTIFACT_DIR;
use crate::io::writer::{ArtifactWriter, WriteRequest, WriteResult};
use crate::serde_yaml;
use crate::store::RunContext;
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
/// * `bindings` - Resolved bindings for template resolution
/// * `datastore` - Data store for lazy binding resolution
///
/// # Returns
///
/// `ArtifactProcessResult` with write status and any errors
#[allow(clippy::too_many_arguments)]
pub async fn process_task_artifacts(
    task_id: &str,
    output: &str,
    artifact_spec: &ArtifactSpec,
    workflow_config: Option<&ArtifactsConfig>,
    base_path: &std::path::Path,
    event_log: Option<&EventLog>,
    bindings: &ResolvedBindings,
    datastore: &RunContext,
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
                template: None,
                format: Some(*format),
                mode: workflow_config.map(|c| c.mode),
            }]
        }
        ArtifactSpec::Single(output_spec) => {
            vec![output_spec.clone()]
        }
        ArtifactSpec::Multiple(outputs) => outputs.clone(),
    };

    // Resolve artifact directory
    let artifact_dir = resolve_artifact_dir(workflow_config, base_path).await;

    // Get max size from workflow config
    let max_size = workflow_config
        .map(|c| c.max_size)
        .unwrap_or(crate::ast::artifact::DEFAULT_MAX_ARTIFACT_SIZE);

    // Create artifact writer
    let writer = ArtifactWriter::new(&artifact_dir, task_id).with_max_size(max_size);

    // Process each output
    for output_spec in outputs {
        match write_single_artifact(
            task_id,
            output,
            &output_spec,
            workflow_config,
            &writer,
            bindings,
            datastore,
        )
        .await
        {
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
///
/// Supports `template:` field - if set, resolves template with bindings
/// instead of using task output directly.
async fn write_single_artifact(
    task_id: &str,
    output: &str,
    output_spec: &ArtifactOutput,
    workflow_config: Option<&ArtifactsConfig>,
    writer: &ArtifactWriter,
    bindings: &ResolvedBindings,
    datastore: &RunContext,
) -> Result<WriteResult, NikaError> {
    // Determine format (task spec > workflow default)
    let format = output_spec
        .format
        .or(workflow_config.map(|c| c.format))
        .unwrap_or(ArtifactFormat::Text);

    // Determine mode (task spec > workflow default)
    let mode = output_spec
        .mode
        .or(workflow_config.map(|c| c.mode))
        .unwrap_or(ArtifactMode::Overwrite);

    // Determine content source - template or task output
    let raw_content: String = if let Some(ref tpl) = output_spec.template {
        // Resolve template with bindings
        debug!(
            task_id = %task_id,
            template = %tpl,
            "Resolving artifact template"
        );
        match template_resolve(tpl, bindings, datastore) {
            Ok(resolved) => resolved.into_owned(),
            Err(e) => {
                warn!(
                    task_id = %task_id,
                    template = %tpl,
                    error = %e,
                    "Failed to resolve artifact template, using raw template"
                );
                // On template resolution failure, use the raw template
                tpl.clone()
            }
        }
    } else {
        // No template - use task output directly
        output.to_string()
    };

    // Convert content based on format
    let content = format_output(&raw_content, format)?;

    // Convert ArtifactFormat to OutputFormat for the writer
    let output_format = match format {
        ArtifactFormat::Text => OutputFormat::Text,
        ArtifactFormat::Json => OutputFormat::Json,
        ArtifactFormat::Yaml => OutputFormat::Text, // YAML treated as text for validation
    };

    // Normalize the artifact path to prevent doubled paths when user specifies
    // full path like ./artifacts/custom.txt instead of just custom.txt
    let artifact_dir_str = workflow_config
        .and_then(|c| c.dir.as_deref())
        .unwrap_or(DEFAULT_ARTIFACT_DIR);
    let normalized_path = normalize_artifact_path(&output_spec.path, artifact_dir_str);

    // Build write request - we need to keep output_format for WriteResult
    let request = WriteRequest::new(task_id, &normalized_path)
        .with_content(content)
        .with_format(output_format.clone());

    // Handle different write modes
    match mode {
        ArtifactMode::Overwrite => writer.write(request).await,
        ArtifactMode::Append => {
            // For append mode, we need to use atomic append
            let resolved_path = writer.validate_path(task_id, &normalized_path)?;
            write_append(&resolved_path, request.content.as_bytes())
                .await
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
            let resolved_path = writer.validate_path(task_id, &normalized_path)?;
            let unique_path = write_unique(&resolved_path, request.content.as_bytes())
                .await
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
            let resolved_path = writer.validate_path(task_id, &normalized_path)?;
            write_fail(&resolved_path, request.content.as_bytes())
                .await
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
                Ok(value) => serde_json::to_string_pretty(&value).map_err(|e| {
                    NikaError::ArtifactWriteError {
                        path: "".to_string(),
                        reason: format!("Failed to format JSON: {}", e),
                    }
                }),
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
                    serde_yaml::to_string(&value).map_err(|e| NikaError::ArtifactWriteError {
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
async fn resolve_artifact_dir(
    workflow_config: Option<&ArtifactsConfig>,
    base_path: &std::path::Path,
) -> PathBuf {
    let dir_str = workflow_config
        .and_then(|c| c.dir.as_deref())
        .unwrap_or(DEFAULT_ARTIFACT_DIR);

    let artifact_dir = base_path.join(dir_str);

    // Create directory if it doesn't exist (non-blocking)
    if !artifact_dir.exists() {
        if let Err(e) = tokio::fs::create_dir_all(&artifact_dir).await {
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

/// Normalize artifact output path to prevent doubled paths
///
/// If the artifact path starts with `./` and contains the artifact_dir path,
/// strip the redundant prefix to prevent paths like:
/// `artifacts/./artifacts/custom.txt` → `artifacts/custom.txt`
///
/// This handles the common user mistake of specifying full paths in artifact spec.
fn normalize_artifact_path(path: &str, artifact_dir_str: &str) -> String {
    let path = path.trim();
    let artifact_dir = artifact_dir_str
        .trim_start_matches("./")
        .trim_end_matches('/');

    // Check if path starts with ./ and contains the artifact_dir
    if path.starts_with("./") {
        let path_without_dot = path.trim_start_matches("./");
        // If path starts with artifact_dir, strip it to get the relative part
        if path_without_dot.starts_with(artifact_dir) {
            let relative = path_without_dot
                .trim_start_matches(artifact_dir)
                .trim_start_matches('/');
            if !relative.is_empty() {
                debug!(
                    original = %path,
                    normalized = %relative,
                    "Normalized artifact path (removed redundant prefix)"
                );
                return relative.to_string();
            }
        }
    }

    path.to_string()
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

    #[tokio::test]
    async fn test_resolve_artifact_dir_default() {
        let base = PathBuf::from("/project");
        let dir = resolve_artifact_dir(None, &base).await;
        assert_eq!(dir, PathBuf::from("/project/.nika/artifacts"));
    }

    #[tokio::test]
    async fn test_resolve_artifact_dir_custom() {
        let base = PathBuf::from("/project");
        let config = ArtifactsConfig {
            dir: Some("output".to_string()),
            ..Default::default()
        };
        let dir = resolve_artifact_dir(Some(&config), &base).await;
        assert_eq!(dir, PathBuf::from("/project/output"));
    }

    #[tokio::test]
    async fn test_process_task_artifacts_disabled() {
        let base = tempdir().unwrap();
        let bindings = ResolvedBindings::default();
        let datastore = RunContext::new();
        let result = process_task_artifacts(
            "task1",
            "output",
            &ArtifactSpec::Enabled(false),
            None,
            base.path(),
            None, // No event log for tests
            &bindings,
            &datastore,
        )
        .await;

        assert_eq!(result.written, 0);
        assert!(result.paths.is_empty());
        assert!(result.errors.is_empty());
    }

    #[tokio::test]
    async fn test_process_task_artifacts_enabled() {
        let base = tempdir().unwrap();
        let artifact_dir = base.path().join(".nika/artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        let bindings = ResolvedBindings::default();
        let datastore = RunContext::new();

        let result = process_task_artifacts(
            "task1",
            "test output",
            &ArtifactSpec::Enabled(true),
            None,
            base.path(),
            None, // No event log for tests
            &bindings,
            &datastore,
        )
        .await;

        // Print errors for debugging
        if !result.errors.is_empty() {
            eprintln!("Artifact errors: {:?}", result.errors);
        }

        assert_eq!(
            result.written, 1,
            "Expected 1 artifact written, errors: {:?}",
            result.errors
        );
        assert!(!result.paths.is_empty());
        assert!(
            result.errors.is_empty(),
            "Unexpected errors: {:?}",
            result.errors
        );
    }

    #[tokio::test]
    async fn test_process_task_artifacts_single() {
        let base = tempdir().unwrap();
        let artifact_dir = base.path().join(".nika/artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        let bindings = ResolvedBindings::default();
        let datastore = RunContext::new();

        let spec = ArtifactSpec::Single(ArtifactOutput {
            path: "output.json".to_string(),
            source: None,
            template: None,
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
            &bindings,
            &datastore,
        )
        .await;

        assert_eq!(result.written, 1);
        assert!(result.paths[0].ends_with("output.json"));
    }

    #[tokio::test]
    async fn test_process_task_artifacts_multiple() {
        let base = tempdir().unwrap();
        let artifact_dir = base.path().join(".nika/artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        let bindings = ResolvedBindings::default();
        let datastore = RunContext::new();

        let spec = ArtifactSpec::Multiple(vec![
            ArtifactOutput {
                path: "raw.txt".to_string(),
                source: None,
                template: None,
                format: Some(ArtifactFormat::Text),
                mode: None,
            },
            ArtifactOutput {
                path: "processed.json".to_string(),
                source: None,
                template: None,
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
            &bindings,
            &datastore,
        )
        .await;

        assert_eq!(result.written, 2);
        assert_eq!(result.paths.len(), 2);
    }

    // ========== normalize_artifact_path tests ==========

    #[test]
    fn test_normalize_artifact_path_simple_filename() {
        // Simple filename should not be modified
        let result = normalize_artifact_path("custom.txt", "./examples/.test-output/artifacts");
        assert_eq!(result, "custom.txt");
    }

    #[test]
    fn test_normalize_artifact_path_doubled_path() {
        // Doubled path should be normalized
        let result = normalize_artifact_path(
            "./examples/.test-output/artifacts/custom.txt",
            "./examples/.test-output/artifacts",
        );
        assert_eq!(result, "custom.txt");
    }

    #[test]
    fn test_normalize_artifact_path_nested_doubled() {
        // Nested doubled path should be normalized
        let result =
            normalize_artifact_path("./output/artifacts/subdir/file.json", "./output/artifacts");
        assert_eq!(result, "subdir/file.json");
    }

    #[test]
    fn test_normalize_artifact_path_no_leading_dot() {
        // Path without leading ./ should not be modified
        let result = normalize_artifact_path("subdir/file.txt", "./artifacts");
        assert_eq!(result, "subdir/file.txt");
    }

    #[test]
    fn test_normalize_artifact_path_different_prefix() {
        // Path that doesn't match artifact_dir should not be modified
        let result = normalize_artifact_path("./other/path/file.txt", "./artifacts");
        assert_eq!(result, "./other/path/file.txt");
    }

    #[test]
    fn test_normalize_artifact_path_default_dir() {
        // Works with default artifact directory
        let result = normalize_artifact_path("./.nika/artifacts/output.json", ".nika/artifacts");
        assert_eq!(result, "output.json");
    }

    // ========== Template resolution tests ==========

    #[tokio::test]
    async fn test_artifact_template_resolution() {
        use crate::store::TaskResult;
        use std::sync::Arc;
        use std::time::Duration;

        let base = tempdir().unwrap();
        let artifact_dir = base.path().join(".nika/artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();

        // Create datastore with task result that has JSON data
        let datastore = RunContext::new();
        let task_result = TaskResult::success_str(
            r#"{"name": "Alice", "age": 30}"#.to_string(),
            Duration::from_millis(100),
        );
        datastore.insert(Arc::from("generate_data"), task_result);

        // Create bindings that reference the upstream task
        let mut bindings = ResolvedBindings::default();
        bindings.set("data", serde_json::json!({"name": "Alice", "age": 30}));

        // Create artifact spec with template
        let spec = ArtifactSpec::Single(ArtifactOutput {
            path: "report.md".to_string(),
            source: None,
            template: Some(
                "# Report\n\nUser: {{use.data.name}}, Age: {{use.data.age}}".to_string(),
            ),
            format: Some(ArtifactFormat::Text),
            mode: None,
        });

        let result = process_task_artifacts(
            "generate_report",
            "task output (ignored when template is set)",
            &spec,
            None,
            base.path(),
            None,
            &bindings,
            &datastore,
        )
        .await;

        assert_eq!(
            result.written, 1,
            "Expected 1 artifact written, errors: {:?}",
            result.errors
        );
        assert!(
            result.errors.is_empty(),
            "Unexpected errors: {:?}",
            result.errors
        );

        // Read the artifact content and verify template was resolved
        let artifact_content = std::fs::read_to_string(&result.paths[0]).unwrap();
        assert_eq!(artifact_content, "# Report\n\nUser: Alice, Age: 30");
    }

    #[tokio::test]
    async fn test_artifact_without_template_uses_output() {
        let base = tempdir().unwrap();
        let artifact_dir = base.path().join(".nika/artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        let bindings = ResolvedBindings::default();
        let datastore = RunContext::new();

        // Create artifact spec WITHOUT template
        let spec = ArtifactSpec::Single(ArtifactOutput {
            path: "output.txt".to_string(),
            source: None,
            template: None, // No template - should use task output
            format: Some(ArtifactFormat::Text),
            mode: None,
        });

        let result = process_task_artifacts(
            "task1",
            "This is the task output",
            &spec,
            None,
            base.path(),
            None,
            &bindings,
            &datastore,
        )
        .await;

        assert_eq!(result.written, 1);

        // Read the artifact content and verify it's the task output
        let artifact_content = std::fs::read_to_string(&result.paths[0]).unwrap();
        assert_eq!(artifact_content, "This is the task output");
    }

    #[tokio::test]
    async fn test_artifact_template_with_missing_binding() {
        let base = tempdir().unwrap();
        let artifact_dir = base.path().join(".nika/artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        let bindings = ResolvedBindings::default(); // Empty bindings
        let datastore = RunContext::new();

        // Create artifact spec with template that references missing binding
        let spec = ArtifactSpec::Single(ArtifactOutput {
            path: "report.md".to_string(),
            source: None,
            template: Some("Hello {{use.missing}}!".to_string()),
            format: Some(ArtifactFormat::Text),
            mode: None,
        });

        let result = process_task_artifacts(
            "task1",
            "fallback output",
            &spec,
            None,
            base.path(),
            None,
            &bindings,
            &datastore,
        )
        .await;

        // Should still write, but with raw template (on resolution error)
        assert_eq!(result.written, 1);

        let artifact_content = std::fs::read_to_string(&result.paths[0]).unwrap();
        // On template resolution failure, it uses the raw template
        assert_eq!(artifact_content, "Hello {{use.missing}}!");
    }
}
