//! Artifact Processor - Write task outputs to disk
//!
//! Integrates the artifact system with the task execution flow.
//! Called after successful task completion when `artifact:` is configured.
//!
//! Supports template-based artifacts via the `template:` field
//! which supports `{{with.*}}` bindings for dynamic content generation.

use std::path::PathBuf;
use std::sync::Arc;

use tracing::{debug, warn};

use crate::ast::artifact::{
    ArtifactFormat, ArtifactMode, ArtifactOutput, ArtifactSpec, ArtifactsConfig,
};
use crate::ast::OutputFormat;
use crate::binding::{template_resolve, ResolvedBindings};
use crate::error::NikaError;
use crate::event::{EventKind, EventLog};
use crate::io::atomic::{write_append, write_fail, write_unique};
use crate::io::security::DEFAULT_ARTIFACT_DIR;
use crate::io::writer::{
    ArtifactWriter, BinarySource, BinaryWriteRequest, WriteRequest, WriteResult,
};
use crate::media::MediaRef;
use crate::serde_yaml;
use crate::store::RunContext;

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
/// * `media_refs` - Media files produced by the task (for binary artifact source resolution)
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
    media_refs: &[MediaRef],
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
            // Use defaults - generate single output with task_id as filename.
            // Auto-promote to Binary when the task produced media but no explicit
            // workflow-level format is set, so `artifact: true` just works for media tasks.
            let format = if !media_refs.is_empty()
                && workflow_config.is_none_or(|c| c.format == ArtifactFormat::Text)
            {
                &ArtifactFormat::Binary
            } else {
                workflow_config
                    .map(|c| &c.format)
                    .unwrap_or(&ArtifactFormat::Text)
            };
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

    // Process each output (index used for positional media matching)
    for (artifact_index, output_spec) in outputs.iter().enumerate() {
        match write_single_artifact(
            task_id,
            output,
            output_spec,
            workflow_config,
            &writer,
            bindings,
            datastore,
            media_refs,
            artifact_index,
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
                    let checksum = write_result.checksum.clone().or_else(|| {
                        if write_result.format == OutputFormat::Binary {
                            resolve_binary_checksum(output_spec, media_refs)
                        } else {
                            None
                        }
                    });
                    // Use original ArtifactFormat for the event, not internal OutputFormat.
                    // This preserves "markdown" instead of collapsing to "text".
                    let format_str = output_spec
                        .format
                        .map(|f| format!("{:?}", f).to_lowercase())
                        .unwrap_or_else(|| format!("{:?}", write_result.format).to_lowercase());
                    log.emit(EventKind::ArtifactWritten {
                        task_id: Arc::from(task_id),
                        path: write_result.path.display().to_string(),
                        size: write_result.size,
                        format: format_str,
                        checksum,
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
#[allow(clippy::too_many_arguments)]
async fn write_single_artifact(
    task_id: &str,
    output: &str,
    output_spec: &ArtifactOutput,
    workflow_config: Option<&ArtifactsConfig>,
    writer: &ArtifactWriter,
    bindings: &ResolvedBindings,
    datastore: &RunContext,
    media_refs: &[MediaRef],
    artifact_index: usize,
) -> Result<WriteResult, NikaError> {
    // Determine format (task spec > workflow default).
    //
    // Auto-promote to Binary when the task produced media content but no
    // explicit format was set. Without this, a workflow like:
    //   artifact: { path: output.png }
    // would default to Text and write the JSON metadata string instead of
    // the actual image bytes — the root cause of "broken pixel" artifacts.
    let explicit_format = output_spec.format.or(workflow_config.map(|c| c.format));
    let format = match explicit_format {
        Some(f) => f,
        None if !media_refs.is_empty() => {
            debug!(
                task_id = %task_id,
                "Auto-promoting artifact format to Binary (task produced media)"
            );
            ArtifactFormat::Binary
        }
        None => ArtifactFormat::Text,
    };

    // Determine mode (task spec > workflow default) — computed before binary
    // branch so that binary artifacts can validate and reject unsupported modes.
    let mode = output_spec
        .mode
        .or(workflow_config.map(|c| c.mode))
        .unwrap_or(ArtifactMode::Overwrite);

    // Binary format: resolve source to CAS path and copy
    if format == ArtifactFormat::Binary {
        // Defense-in-depth: if media_refs is empty, try to construct a MediaRef
        // from the task output JSON. This handles cases where set_media() was not
        // called (e.g., older code paths) but the output contains CAS hash/path.
        let fallback_refs;
        let effective_media_refs = if media_refs.is_empty() {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(output) {
                if let (Some(hash), Some(path_str)) = (
                    parsed.get("hash").and_then(|v| v.as_str()),
                    parsed.get("path").and_then(|v| v.as_str()),
                ) {
                    debug!(
                        task_id = %task_id,
                        hash = %hash,
                        "Binary artifact fallback: constructing MediaRef from output JSON"
                    );
                    fallback_refs = vec![MediaRef {
                        hash: hash.to_string(),
                        mime_type: parsed
                            .get("mime_type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("application/octet-stream")
                            .to_string(),
                        size_bytes: parsed
                            .get("size_bytes")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0),
                        path: std::path::PathBuf::from(path_str),
                        extension: parsed
                            .get("extension")
                            .and_then(|v| v.as_str())
                            .map(String::from)
                            .unwrap_or_default(),
                        created_by: task_id.to_string(),
                        metadata: serde_json::Map::new(),
                    }];
                    &fallback_refs
                } else {
                    media_refs
                }
            } else {
                media_refs
            }
        } else {
            media_refs
        };

        return write_binary_artifact(
            task_id,
            output_spec,
            mode,
            writer,
            bindings,
            datastore,
            effective_media_refs,
            artifact_index,
        )
        .await;
    }

    // Determine content source: source binding > template > task output
    let raw_content: String = if let Some(ref source_alias) = output_spec.source {
        // Resolve from bindings (with: block or upstream task output)
        debug!(
            task_id = %task_id,
            source = %source_alias,
            "Resolving artifact source binding"
        );
        if let Some(value) = bindings.get(source_alias) {
            match value {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            }
        } else {
            // Try datastore (task outputs stored by task ID)
            match datastore.get_output(source_alias) {
                Some(arc_value) => match arc_value.as_ref() {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                },
                None => {
                    warn!(
                        task_id = %task_id,
                        source = %source_alias,
                        "Artifact source binding not found, falling back to task output"
                    );
                    output.to_string()
                }
            }
        }
    } else if let Some(ref tpl) = output_spec.template {
        // Replace {{output}} with actual task output before template resolution
        let tpl_with_output = tpl.replace("{{output}}", output);

        // Resolve template with bindings (handles {{with.*}}, {{inputs.*}}, etc.)
        debug!(
            task_id = %task_id,
            template = %tpl,
            "Resolving artifact template"
        );
        match template_resolve(&tpl_with_output, bindings, datastore) {
            Ok(resolved) => resolved.into_owned(),
            Err(e) => {
                warn!(
                    task_id = %task_id,
                    template = %tpl,
                    error = %e,
                    "Failed to resolve artifact template, using raw template"
                );
                // On template resolution failure, use the pre-resolved template
                tpl_with_output
            }
        }
    } else {
        // No source or template - use task output directly
        output.to_string()
    };

    // Convert content based on format
    let content = format_output(&raw_content, format)?;

    // Convert ArtifactFormat to OutputFormat for the writer
    let output_format = match format {
        ArtifactFormat::Text => OutputFormat::Text,
        ArtifactFormat::Json => OutputFormat::Json,
        ArtifactFormat::Yaml => OutputFormat::Text, // YAML treated as text for validation
        ArtifactFormat::Binary => OutputFormat::Text, // Binary bypasses format_output entirely
        ArtifactFormat::Markdown => OutputFormat::Text, // Markdown treated as text
    };

    // Pre-resolve {{with.*}} and {{output}} binding references in the path
    // before the TemplateResolver handles {{task_id}}, {{date}}, etc.
    let resolved_path =
        resolve_artifact_path_bindings(&output_spec.path, output, bindings, datastore);

    // Normalize the artifact path to prevent doubled paths when user specifies
    // full path like ./artifacts/custom.txt instead of just custom.txt
    let artifact_dir_str = workflow_config
        .and_then(|c| c.dir.as_deref())
        .unwrap_or(DEFAULT_ARTIFACT_DIR);
    let normalized_path = normalize_artifact_path(&resolved_path, artifact_dir_str);

    // Build write request - we need to keep output_format for WriteResult
    let request = WriteRequest::new(task_id, &normalized_path)
        .with_content(content)
        .with_format(output_format.clone());

    // Handle different write modes
    // Compute BLAKE3 checksum for text content before writing
    let checksum = Some(format!(
        "blake3:{}",
        blake3::hash(request.content.as_bytes()).to_hex()
    ));

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
                checksum,
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
                checksum,
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
                checksum,
            })
        }
    }
}

/// Write a binary artifact from a media reference.
///
/// Resolves the `source` binding to a media hash or path, then copies from CAS store.
/// Falls back to the first media ref if no explicit source is specified.
///
/// # Mode support
///
/// Binary artifacts only support `Overwrite` (default) and `Fail` modes.
/// `Append` and `Unique` are rejected with NIKA-281 because binary data
/// cannot be meaningfully appended or deduplicated by filename suffix.
#[allow(clippy::too_many_arguments)]
async fn write_binary_artifact(
    task_id: &str,
    output_spec: &ArtifactOutput,
    mode: ArtifactMode,
    writer: &ArtifactWriter,
    bindings: &ResolvedBindings,
    datastore: &RunContext,
    media_refs: &[MediaRef],
    artifact_index: usize,
) -> Result<WriteResult, NikaError> {
    // Reject unsupported modes for binary artifacts
    match mode {
        ArtifactMode::Append => {
            return Err(NikaError::ArtifactWriteError {
                path: output_spec.path.clone(),
                reason: "Binary artifacts do not support append mode".to_string(),
            });
        }
        ArtifactMode::Unique => {
            return Err(NikaError::ArtifactWriteError {
                path: output_spec.path.clone(),
                reason: "Binary artifacts do not support unique mode".to_string(),
            });
        }
        ArtifactMode::Overwrite | ArtifactMode::Fail => {
            // Supported -- continue
        }
    }

    // Resolve source to a MediaRef:
    // 1. If source is specified, look it up in bindings/media_refs
    // 2. Otherwise, use first media ref from the task
    //
    // Resolution order for `source: alias`:
    //   a) Direct match: media_refs.created_by == alias || media_refs.hash == alias
    //   b) Binding indirection: resolve alias -> source task ID -> media_refs.created_by
    //   c) Hash indirection: binding value is a hash string -> media_refs.hash == hash
    let media_ref = if let Some(ref source_alias) = output_spec.source {
        // Try to find media by source alias (could be a task_id or hash)
        // First check if any media ref was created by a task matching the source alias
        let from_media = media_refs
            .iter()
            .find(|m| m.created_by == *source_alias || m.hash == *source_alias);
        if let Some(mr) = from_media {
            mr.clone()
        } else {
            // Try binding indirection: source_alias is a with-binding alias
            // that maps to a task (e.g., `source: img` where `with: { img: $gen_img }`)
            // Resolve the source task ID and find media by created_by.
            let from_binding_source = bindings
                .source_task_id(source_alias)
                .and_then(|task_id| media_refs.iter().find(|m| m.created_by == task_id).cloned());

            if let Some(mr) = from_binding_source {
                mr
            } else {
                // Try resolving from bindings — the value might contain a media hash.
                // Handles:
                //   - String value starting with "blake3:" → directly used as hash
                //   - String value containing JSON with .hash field → extract it
                //   - Object value → extract .hash field (e.g., nika:import output)
                fn extract_hash_from_value(value: &serde_json::Value) -> Option<String> {
                    match value {
                        serde_json::Value::String(s) => {
                            if s.starts_with("blake3:") || s.starts_with("sha256:") {
                                Some(s.clone())
                            } else if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(s)
                            {
                                parsed
                                    .get("hash")
                                    .and_then(|v| v.as_str())
                                    .map(String::from)
                            } else {
                                Some(s.clone())
                            }
                        }
                        serde_json::Value::Object(obj) => {
                            obj.get("hash").and_then(|v| v.as_str()).map(String::from)
                        }
                        _ => None,
                    }
                }

                let hash_value = if let Some(value) = bindings.get(source_alias) {
                    extract_hash_from_value(value)
                } else {
                    datastore
                        .get_output(source_alias)
                        .and_then(|v| extract_hash_from_value(v.as_ref()))
                };

                if let Some(hash) = hash_value {
                    // Find media ref by hash in current task's media_refs
                    if let Some(mr) = media_refs.iter().find(|m| m.hash == hash).cloned() {
                        mr
                    } else {
                        // Hash not in current task's media — construct MediaRef from
                        // the source binding JSON (e.g., upstream nika:import output).
                        // This is the common case for `source: alias` where alias
                        // points to a different task's media output.
                        //
                        // The binding value may be:
                        //   - Value::Object with hash/path fields
                        //   - Value::String containing JSON with hash/path fields
                        let binding_value = bindings.get(source_alias).cloned().or_else(|| {
                            datastore
                                .get_output(source_alias)
                                .map(|v| v.as_ref().clone())
                        });
                        // Normalize: if binding is a JSON string, parse it to Object
                        let binding_obj = match &binding_value {
                            Some(serde_json::Value::Object(_)) => binding_value.clone(),
                            Some(serde_json::Value::String(s)) => {
                                serde_json::from_str::<serde_json::Value>(s)
                                    .ok()
                                    .filter(|v| v.is_object())
                            }
                            _ => None,
                        };
                        if let Some(serde_json::Value::Object(obj)) = binding_obj {
                            if let Some(path_str) = obj.get("path").and_then(|v| v.as_str()) {
                                MediaRef {
                                    hash: hash.clone(),
                                    mime_type: obj
                                        .get("mime_type")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("application/octet-stream")
                                        .to_string(),
                                    size_bytes: obj
                                        .get("size_bytes")
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(0),
                                    path: std::path::PathBuf::from(path_str),
                                    extension: obj
                                        .get("extension")
                                        .and_then(|v| v.as_str())
                                        .map(String::from)
                                        .unwrap_or_default(),
                                    created_by: source_alias.to_string(),
                                    metadata: serde_json::Map::new(),
                                }
                            } else {
                                return Err(NikaError::ArtifactWriteError {
                                    path: output_spec.path.clone(),
                                    reason: format!(
                                        "Binary artifact source '{}' resolved to hash '{}' but no media ref matches and binding has no 'path' field",
                                        source_alias, hash
                                    ),
                                });
                            }
                        } else {
                            return Err(NikaError::ArtifactWriteError {
                                path: output_spec.path.clone(),
                                reason: format!(
                                    "Binary artifact source '{}' resolved to hash '{}' but no media ref matches",
                                    source_alias, hash
                                ),
                            });
                        }
                    }
                } else {
                    return Err(NikaError::ArtifactWriteError {
                        path: output_spec.path.clone(),
                        reason: format!(
                            "Binary artifact source '{}' not found in media refs or bindings",
                            source_alias
                        ),
                    });
                }
            }
        }
    } else {
        // No explicit source — use positional matching: artifact[i] → media[i].
        // Falls back to first media ref when index is out of bounds (e.g., single
        // media with multiple artifacts, or more artifacts than media refs).
        media_refs
            .get(artifact_index)
            .or_else(|| media_refs.first())
            .cloned()
            .ok_or_else(|| NikaError::ArtifactWriteError {
                path: output_spec.path.clone(),
                reason: "Binary artifact requires media content but task produced no media"
                    .to_string(),
            })?
    };

    debug!(
        task_id = %task_id,
        hash = %media_ref.hash,
        path = %media_ref.path.display(),
        "Writing binary artifact from CAS"
    );

    // Pre-resolve binding references in the path
    let resolved_path = resolve_artifact_path_bindings(&output_spec.path, "", bindings, datastore);

    // Normalize the artifact path
    let artifact_dir_str = ""; // Binary artifacts use the raw path
    let normalized_path = normalize_artifact_path(&resolved_path, artifact_dir_str);

    // For Fail mode, check that the target does not already exist
    if mode == ArtifactMode::Fail {
        let resolved = writer.validate_path(task_id, &normalized_path)?;
        if resolved.exists() {
            return Err(NikaError::ArtifactWriteError {
                path: resolved.display().to_string(),
                reason: "File already exists and mode is 'fail'".to_string(),
            });
        }
    }

    let request = BinaryWriteRequest {
        task_id: task_id.to_string(),
        output_path: normalized_path,
        source: BinarySource::CasPath(media_ref.path.clone()),
        expected_size: media_ref.size_bytes,
    };

    writer.write_binary(request).await
}

/// Resolve the blake3 checksum for a binary artifact from media refs.
///
/// Looks up the matching MediaRef by source alias or falls back to the first ref.
/// Returns `Some("blake3:...")` if found, `None` otherwise.
fn resolve_binary_checksum(
    output_spec: &ArtifactOutput,
    media_refs: &[MediaRef],
) -> Option<String> {
    if let Some(ref source_alias) = output_spec.source {
        // Match by creator task_id or by hash
        media_refs
            .iter()
            .find(|m| m.created_by == *source_alias || m.hash == *source_alias)
            .map(|m| m.hash.clone())
    } else {
        // No explicit source -- use first media ref (same logic as write_binary_artifact)
        media_refs.first().map(|m| m.hash.clone())
    }
}

/// Format output content based on artifact format
fn format_output(output: &str, format: ArtifactFormat) -> Result<String, NikaError> {
    match format {
        ArtifactFormat::Text | ArtifactFormat::Markdown => Ok(output.to_string()),
        ArtifactFormat::Json => {
            // Strip markdown code fences that LLMs often wrap around JSON/YAML output
            let cleaned = strip_code_fences(output);
            let output = &cleaned;
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
            // Strip markdown code fences that LLMs often wrap around YAML output
            let cleaned = strip_code_fences(output);
            let output = &cleaned;
            // Try to parse as JSON first, then convert to YAML
            match serde_json::from_str::<serde_json::Value>(output) {
                Ok(value) => {
                    serde_yaml::to_string(&value).map_err(|e| NikaError::ArtifactWriteError {
                        path: "".to_string(),
                        reason: format!("Failed to format YAML: {}", e),
                    })
                }
                Err(_) => {
                    // Not valid JSON — validate as YAML before accepting
                    match serde_yaml::from_str::<serde_json::Value>(output) {
                        Ok(_) => Ok(output.to_string()),
                        Err(e) => {
                            tracing::warn!(
                                error = %e,
                                "Artifact format: yaml content is neither valid JSON nor YAML, writing as-is"
                            );
                            Ok(output.to_string())
                        }
                    }
                }
            }
        }
        ArtifactFormat::Binary => {
            // Binary artifacts are handled separately via write_binary()
            // This path should not be reached for binary format
            Err(NikaError::ArtifactWriteError {
                path: "".to_string(),
                reason: "Binary format must be written via write_binary(), not format_output()"
                    .to_string(),
            })
        }
    }
}

/// Strip markdown code fences (```json ... ``` or ```yaml ... ```) from LLM output.
fn strip_code_fences(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.starts_with("```") {
        let after_fence = if let Some(newline_pos) = trimmed.find('\n') {
            &trimmed[newline_pos + 1..]
        } else {
            return trimmed.to_string();
        };
        if let Some(stripped) = after_fence.strip_suffix("```") {
            stripped.trim().to_string()
        } else {
            after_fence.trim().to_string()
        }
    } else {
        trimmed.to_string()
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

/// Sanitize a value for safe use in file paths.
///
/// Replaces path-dangerous characters with underscores and truncates
/// to prevent excessively long paths. This is the security boundary
/// where user-controlled binding values enter the filesystem path context.
fn sanitize_for_path(value: &str) -> String {
    value
        .replace(['/', '\\', ':'], "_")
        .replace('\0', "")
        .replace("..", "_")
        .replace('~', "_")
        .chars()
        .take(200)
        .collect::<String>()
        .trim()
        .to_string()
}

/// Pre-resolve `{{with.*}}` and `{{output}}` binding references in an artifact path.
///
/// This is a targeted pre-pass that resolves binding-based templates in artifact
/// paths before they reach the `TemplateResolver` (which handles `{{task_id}}`,
/// `{{date}}`, etc.). The two template systems remain independent.
///
/// Supported patterns:
/// - `{{with.alias}}` — Resolves from the task's `with:` bindings
/// - `{{output}}` — Resolves to the current task's output (sanitized)
///
/// Values are sanitized via `sanitize_for_path()` to prevent path traversal.
/// Unresolved `{{with.*}}` references are left as-is (will error in TemplateResolver).
fn resolve_artifact_path_bindings(
    path: &str,
    output: &str,
    bindings: &ResolvedBindings,
    datastore: &RunContext,
) -> String {
    let mut result = path.to_string();
    let mut pos = 0;

    while let Some(start) = result[pos..].find("{{") {
        let start = pos + start;
        let Some(end) = result[start..].find("}}") else {
            break;
        };
        let end = start + end + 2;

        let var_name = result[start + 2..end - 2].trim();

        if var_name == "output" {
            let sanitized = sanitize_for_path(output.trim());
            result.replace_range(start..end, &sanitized);
            pos = start + sanitized.len();
        } else if let Some(alias) = var_name.strip_prefix("with.") {
            // Extract top-level alias (e.g., "with.timestamp" → "timestamp")
            let top_alias = alias.split('.').next().unwrap_or(alias);

            // Check for media paths: {{with.alias.media[N].field}}
            // Media refs live in the TaskResult side-channel, not in the task
            // output value, so we must resolve via datastore.resolve_path()
            // using the original source task ID.
            let nested_path = alias.split_once('.').map(|x| x.1).unwrap_or("");
            let is_media_path = nested_path == "media"
                || nested_path.starts_with("media.")
                || nested_path.starts_with("media[");

            if is_media_path {
                // Resolve media path via datastore using source task ID
                if let Some(source_task_id) = bindings.source_task_id(top_alias) {
                    let full_path = format!("{}.{}", source_task_id, nested_path);
                    if let Some(value) = datastore.resolve_path(&full_path) {
                        let raw_value = match &value {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        let sanitized = sanitize_for_path(&raw_value);
                        result.replace_range(start..end, &sanitized);
                        pos = start + sanitized.len();
                    } else {
                        pos = end;
                    }
                } else {
                    pos = end;
                }
            } else if let Some(value) = bindings.get(top_alias) {
                // For nested paths like "with.data.name", do JSONPath-like access
                let raw_value = if alias.contains('.') {
                    // Navigate into the JSON value
                    let parts: Vec<&str> = alias.splitn(2, '.').collect();
                    if parts.len() == 2 {
                        json_path_value(value, parts[1])
                    } else {
                        value_to_string(value)
                    }
                } else {
                    value_to_string(value)
                };
                let sanitized = sanitize_for_path(&raw_value);
                result.replace_range(start..end, &sanitized);
                pos = start + sanitized.len();
            } else {
                // Unknown alias — leave as-is for TemplateResolver to handle/error
                pos = end;
            }
        } else if let Some(input_path) = var_name.strip_prefix("inputs.") {
            // Resolve {{inputs.param}} from datastore
            let full_path = format!("inputs.{}", input_path);
            if let Some(value) = datastore.resolve_input_path(&full_path) {
                let raw_value = match &value {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                let sanitized = sanitize_for_path(&raw_value);
                result.replace_range(start..end, &sanitized);
                pos = start + sanitized.len();
            } else {
                pos = end;
            }
        } else {
            // Not a binding reference (e.g., {{task_id}}, {{date}}) — skip
            pos = end;
        }
    }

    result
}

/// Convert a serde_json::Value to a path-friendly string
fn value_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => "null".to_string(),
        // Arrays and objects get compact JSON representation
        other => other.to_string(),
    }
}

/// Simple dot-path navigation into a serde_json::Value
fn json_path_value(value: &serde_json::Value, path: &str) -> String {
    let mut current = value;
    for part in path.split('.') {
        match current {
            serde_json::Value::Object(map) => {
                if let Some(next) = map.get(part) {
                    current = next;
                } else {
                    return format!("{{{{with.{}}}}}", path);
                }
            }
            serde_json::Value::Array(arr) => {
                if let Ok(idx) = part.parse::<usize>() {
                    if let Some(next) = arr.get(idx) {
                        current = next;
                    } else {
                        return format!("{{{{with.{}}}}}", path);
                    }
                } else {
                    return format!("{{{{with.{}}}}}", path);
                }
            }
            _ => return format!("{{{{with.{}}}}}", path),
        }
    }
    value_to_string(current)
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

/// Write artifacts.json manifest when `manifest: true` is set in workflow config.
///
/// Collects all ArtifactWritten events and writes a JSON manifest listing each artifact.
pub fn write_artifact_manifest(
    event_log: &EventLog,
    workflow_config: &ArtifactsConfig,
    base_path: &std::path::Path,
) {
    if !workflow_config.manifest {
        return;
    }

    let artifacts_dir = workflow_config
        .dir
        .as_deref()
        .map(|d| base_path.join(d))
        .unwrap_or_else(|| base_path.join(DEFAULT_ARTIFACT_DIR));

    // Collect all ArtifactWritten events
    let entries: Vec<serde_json::Value> = event_log.with_events(|events| {
        events
            .iter()
            .filter_map(|e| match &e.kind {
                EventKind::ArtifactWritten {
                    task_id,
                    path,
                    size,
                    format,
                    checksum,
                } => Some(serde_json::json!({
                    "task_id": task_id.as_ref(),
                    "path": path,
                    "size": size,
                    "format": format,
                    "checksum": checksum,
                })),
                _ => None,
            })
            .collect()
    });

    if entries.is_empty() {
        debug!("No artifacts written — skipping manifest");
        return;
    }

    let manifest = serde_json::json!({
        "version": 1,
        "artifacts": entries,
    });

    let manifest_path = artifacts_dir.join("artifacts.json");
    if let Some(parent) = manifest_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::warn!(
                path = %manifest_path.display(),
                error = %e,
                "Failed to create manifest directory"
            );
            return;
        }
    }

    match serde_json::to_string_pretty(&manifest) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&manifest_path, json) {
                tracing::warn!(
                    path = %manifest_path.display(),
                    error = %e,
                    "Failed to write artifact manifest"
                );
            } else {
                debug!(
                    path = %manifest_path.display(),
                    count = entries.len(),
                    "Artifact manifest written"
                );
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "Failed to serialize artifact manifest");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_write_artifact_manifest_creates_file() {
        let dir = tempdir().unwrap();
        let event_log = EventLog::new();

        // Emit some ArtifactWritten events
        event_log.emit(EventKind::ArtifactWritten {
            task_id: Arc::from("task1"),
            path: "output/report.md".to_string(),
            size: 1024,
            format: "markdown".to_string(),
            checksum: None,
        });
        event_log.emit(EventKind::ArtifactWritten {
            task_id: Arc::from("task2"),
            path: "output/data.json".to_string(),
            size: 512,
            format: "json".to_string(),
            checksum: Some("abc123".to_string()),
        });

        let config = ArtifactsConfig {
            dir: Some("output".to_string()),
            manifest: true,
            ..Default::default()
        };

        write_artifact_manifest(&event_log, &config, dir.path());

        let manifest_path = dir.path().join("output/artifacts.json");
        assert!(manifest_path.exists(), "Manifest file should be created");

        let content = std::fs::read_to_string(&manifest_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["version"], 1);
        assert_eq!(parsed["artifacts"].as_array().unwrap().len(), 2);
        assert_eq!(parsed["artifacts"][0]["task_id"], "task1");
        assert_eq!(parsed["artifacts"][1]["checksum"], "abc123");
    }

    #[test]
    fn test_write_artifact_manifest_skipped_when_false() {
        let dir = tempdir().unwrap();
        let event_log = EventLog::new();

        event_log.emit(EventKind::ArtifactWritten {
            task_id: Arc::from("task1"),
            path: "out.md".to_string(),
            size: 100,
            format: "text".to_string(),
            checksum: None,
        });

        let config = ArtifactsConfig {
            manifest: false,
            ..Default::default()
        };

        write_artifact_manifest(&event_log, &config, dir.path());

        let manifest_path = dir.path().join(".nika/artifacts/artifacts.json");
        assert!(
            !manifest_path.exists(),
            "Manifest should NOT be created when manifest: false"
        );
    }

    #[test]
    fn test_write_artifact_manifest_no_artifacts_skips() {
        let dir = tempdir().unwrap();
        let event_log = EventLog::new();

        let config = ArtifactsConfig {
            manifest: true,
            ..Default::default()
        };

        write_artifact_manifest(&event_log, &config, dir.path());

        let manifest_path = dir.path().join(".nika/artifacts/artifacts.json");
        assert!(
            !manifest_path.exists(),
            "Manifest should NOT be created when no artifacts exist"
        );
    }

    #[test]
    fn test_format_output_text() {
        let result = format_output("hello world", ArtifactFormat::Text);
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        assert_eq!(result.unwrap(), "hello world");
    }

    #[test]
    fn test_format_output_json_valid() {
        let result = format_output(r#"{"key":"value"}"#, ArtifactFormat::Json);
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        let formatted = result.unwrap();
        assert!(formatted.contains("key"));
        assert!(formatted.contains("value"));
    }

    #[test]
    fn test_format_output_json_invalid() {
        let result = format_output("not json", ArtifactFormat::Json);
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        // Should be wrapped as JSON string
        let formatted = result.unwrap();
        assert!(formatted.contains("not json"));
    }

    #[test]
    fn test_format_output_yaml() {
        let result = format_output(r#"{"key":"value"}"#, ArtifactFormat::Yaml);
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        let formatted = result.unwrap();
        assert!(formatted.contains("key"));
    }

    #[tokio::test]
    async fn test_resolve_artifact_dir_default() {
        let base = PathBuf::from("/project");
        let dir = resolve_artifact_dir(None, &base).await;
        assert_eq!(dir, PathBuf::from("/project/./artifacts"));
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
            &[],
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
            &[],
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
            &[],
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
            &[],
        )
        .await;

        assert_eq!(result.written, 2);
        assert_eq!(result.paths.len(), 2);
    }

    // ========== BUG-3: artifact source resolution ==========

    #[tokio::test]
    async fn test_artifact_source_from_binding() {
        let base = tempdir().unwrap();
        let artifact_dir = base.path().join(".nika/artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();

        // Set up bindings with a "report_data" alias
        let mut bindings = ResolvedBindings::new();
        bindings.set(
            "report_data".to_string(),
            serde_json::Value::String("Content from binding source".to_string()),
        );
        let datastore = RunContext::new();

        let spec = ArtifactSpec::Single(ArtifactOutput {
            path: "report.txt".to_string(),
            source: Some("report_data".to_string()),
            template: None,
            format: Some(ArtifactFormat::Text),
            mode: None,
        });

        let result = process_task_artifacts(
            "task1",
            "this is the task output (should NOT be written)",
            &spec,
            None,
            base.path(),
            None,
            &bindings,
            &datastore,
            &[],
        )
        .await;

        assert_eq!(result.written, 1, "artifact should be written");
        assert!(result.errors.is_empty(), "no errors expected");

        // Verify file content comes from source binding, not task output
        let content = std::fs::read_to_string(&result.paths[0]).unwrap();
        assert_eq!(content, "Content from binding source");
        assert!(!content.contains("should NOT be written"));
    }

    #[tokio::test]
    async fn test_artifact_source_fallback_to_task_output() {
        let base = tempdir().unwrap();
        let artifact_dir = base.path().join(".nika/artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        let bindings = ResolvedBindings::new();
        let datastore = RunContext::new();

        // source points to a non-existent binding → should fall back to task output
        let spec = ArtifactSpec::Single(ArtifactOutput {
            path: "fallback.txt".to_string(),
            source: Some("nonexistent".to_string()),
            template: None,
            format: Some(ArtifactFormat::Text),
            mode: None,
        });

        let result = process_task_artifacts(
            "task1",
            "task output fallback",
            &spec,
            None,
            base.path(),
            None,
            &bindings,
            &datastore,
            &[],
        )
        .await;

        assert_eq!(result.written, 1);
        let content = std::fs::read_to_string(&result.paths[0]).unwrap();
        assert_eq!(content, "task output fallback");
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
                "# Report\n\nUser: {{with.data.name}}, Age: {{with.data.age}}".to_string(),
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
            &[],
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
            &[],
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
            template: Some("Hello {{with.missing}}!".to_string()),
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
            &[],
        )
        .await;

        // Should still write, but with raw template (on resolution error)
        assert_eq!(result.written, 1);

        let artifact_content = std::fs::read_to_string(&result.paths[0]).unwrap();
        // On template resolution failure, it uses the raw template
        assert_eq!(artifact_content, "Hello {{with.missing}}!");
    }

    // ========== resolve_artifact_path_bindings tests ==========

    #[test]
    fn test_path_bindings_with_alias() {
        let mut bindings = ResolvedBindings::default();
        bindings.set("timestamp", serde_json::json!("2024-01-15_14-30-00"));

        let result = resolve_artifact_path_bindings(
            "./outputs/result-{{with.timestamp}}.json",
            "task output",
            &bindings,
            &RunContext::new(),
        );
        assert_eq!(result, "./outputs/result-2024-01-15_14-30-00.json");
    }

    #[test]
    fn test_path_bindings_output() {
        let bindings = ResolvedBindings::default();

        let result = resolve_artifact_path_bindings(
            "./outputs/{{output}}.json",
            "my-report",
            &bindings,
            &RunContext::new(),
        );
        assert_eq!(result, "./outputs/my-report.json");
    }

    #[test]
    fn test_path_bindings_mixed_with_builtins() {
        let mut bindings = ResolvedBindings::default();
        bindings.set("locale", serde_json::json!("fr-FR"));

        // {{with.locale}} should resolve, {{task_id}} should be left for TemplateResolver
        let result = resolve_artifact_path_bindings(
            "{{task_id}}/{{with.locale}}/output.json",
            "",
            &bindings,
            &RunContext::new(),
        );
        assert_eq!(result, "{{task_id}}/fr-FR/output.json");
    }

    #[test]
    fn test_path_bindings_nested_json() {
        let mut bindings = ResolvedBindings::default();
        bindings.set("meta", serde_json::json!({"slug": "qr-code", "version": 2}));

        let result = resolve_artifact_path_bindings(
            "./outputs/{{with.meta.slug}}-v{{with.meta.version}}.json",
            "",
            &bindings,
            &RunContext::new(),
        );
        assert_eq!(result, "./outputs/qr-code-v2.json");
    }

    #[test]
    fn test_path_bindings_sanitizes_slashes() {
        let mut bindings = ResolvedBindings::default();
        bindings.set("name", serde_json::json!("../../etc/passwd"));

        let result = resolve_artifact_path_bindings(
            "./outputs/{{with.name}}.txt",
            "",
            &bindings,
            &RunContext::new(),
        );
        // Path traversal characters should be sanitized
        assert!(!result.contains(".."));
        assert!(!result.contains("etc/passwd"));
    }

    #[test]
    fn test_path_bindings_sanitizes_output() {
        let bindings = ResolvedBindings::default();

        let result = resolve_artifact_path_bindings(
            "./outputs/{{output}}.txt",
            "../../../etc/passwd",
            &bindings,
            &RunContext::new(),
        );
        assert!(!result.contains("../"));
        assert!(!result.contains("etc/passwd"));
    }

    #[test]
    fn test_path_bindings_unknown_alias_preserved() {
        let bindings = ResolvedBindings::default();

        // Unknown binding should be left as-is
        let result = resolve_artifact_path_bindings(
            "./outputs/{{with.unknown}}.json",
            "",
            &bindings,
            &RunContext::new(),
        );
        assert_eq!(result, "./outputs/{{with.unknown}}.json");
    }

    #[test]
    fn test_path_bindings_no_bindings_passthrough() {
        let bindings = ResolvedBindings::default();

        // Path with no binding references should pass through unchanged
        let result = resolve_artifact_path_bindings(
            "{{task_id}}/{{date}}/output.json",
            "",
            &bindings,
            &RunContext::new(),
        );
        assert_eq!(result, "{{task_id}}/{{date}}/output.json");
    }

    #[test]
    fn test_path_bindings_truncates_long_values() {
        let mut bindings = ResolvedBindings::default();
        let long_value = "a".repeat(300);
        bindings.set("name", serde_json::json!(long_value));

        let result =
            resolve_artifact_path_bindings("{{with.name}}.txt", "", &bindings, &RunContext::new());
        // sanitize_for_path truncates to 200 chars
        assert!(result.len() <= 204); // 200 + ".txt"
    }

    #[tokio::test]
    async fn test_e2e_artifact_path_with_bindings() {
        let base = tempdir().unwrap();
        let artifact_dir = base.path().join(".nika/artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();

        let mut bindings = ResolvedBindings::default();
        bindings.set("timestamp", serde_json::json!("2024-01-15_14-30-00"));

        let datastore = RunContext::new();

        let spec = ArtifactSpec::Single(ArtifactOutput {
            path: "result-{{with.timestamp}}.json".to_string(),
            source: None,
            template: None,
            format: Some(ArtifactFormat::Json),
            mode: None,
        });

        let result = process_task_artifacts(
            "save_result",
            r#"{"status": "ok"}"#,
            &spec,
            None,
            base.path(),
            None,
            &bindings,
            &datastore,
            &[],
        )
        .await;

        assert_eq!(
            result.written, 1,
            "Expected 1 artifact written, errors: {:?}",
            result.errors
        );
        assert!(
            result.paths[0]
                .display()
                .to_string()
                .contains("result-2024-01-15_14-30-00.json"),
            "Expected resolved path, got: {}",
            result.paths[0].display()
        );
    }

    // ========== sanitize_for_path tests ==========

    #[test]
    fn test_sanitize_for_path_clean() {
        assert_eq!(sanitize_for_path("hello-world"), "hello-world");
    }

    #[test]
    fn test_sanitize_for_path_slashes() {
        assert_eq!(sanitize_for_path("a/b/c"), "a_b_c");
    }

    #[test]
    fn test_sanitize_for_path_backslashes() {
        assert_eq!(sanitize_for_path("a\\b\\c"), "a_b_c");
    }

    #[test]
    fn test_sanitize_for_path_dotdot() {
        assert_eq!(sanitize_for_path("../escape"), "__escape");
    }

    #[test]
    fn test_sanitize_for_path_null() {
        assert_eq!(sanitize_for_path("a\0b"), "ab");
    }

    #[test]
    fn test_sanitize_for_path_tilde() {
        assert_eq!(sanitize_for_path("~/home"), "__home");
    }

    #[test]
    fn test_sanitize_for_path_truncation() {
        let long = "x".repeat(300);
        assert_eq!(sanitize_for_path(&long).len(), 200);
    }

    // ========== Binary artifact tests ==========

    #[tokio::test]
    async fn test_process_binary_artifact_from_media_ref() {
        use crate::media::MediaRef;

        let base = tempdir().unwrap();
        let artifact_dir = base.path().join(".nika/artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();

        // Create a fake CAS file
        let cas_dir = base.path().join(".nika/media/store/ab");
        std::fs::create_dir_all(&cas_dir).unwrap();
        let cas_file = cas_dir.join("cdef1234");
        let binary_data = b"\x89PNG\r\n\x1a\n fake image data";
        std::fs::write(&cas_file, binary_data).unwrap();

        let bindings = ResolvedBindings::default();
        let datastore = RunContext::new();

        let media_refs = vec![MediaRef {
            hash: "blake3:abcdef1234".to_string(),
            mime_type: "image/png".to_string(),
            size_bytes: binary_data.len() as u64,
            path: cas_file.clone(),
            extension: "png".to_string(),
            created_by: "gen_img".to_string(),
            metadata: serde_json::Map::new(),
        }];

        let spec = ArtifactSpec::Single(ArtifactOutput {
            path: "output/image.bin".to_string(),
            source: None, // Use first media ref
            template: None,
            format: Some(ArtifactFormat::Binary),
            mode: None,
        });

        let result = process_task_artifacts(
            "gen_img",
            "text output (ignored for binary)",
            &spec,
            None,
            base.path(),
            None,
            &bindings,
            &datastore,
            &media_refs,
        )
        .await;

        assert_eq!(
            result.written, 1,
            "Expected 1 binary artifact, errors: {:?}",
            result.errors
        );
        assert!(
            result.errors.is_empty(),
            "Unexpected errors: {:?}",
            result.errors
        );

        // Verify file was copied correctly
        let written = std::fs::read(&result.paths[0]).unwrap();
        assert_eq!(written, binary_data);
    }

    #[tokio::test]
    async fn test_process_binary_artifact_with_source() {
        use crate::media::MediaRef;

        let base = tempdir().unwrap();
        let artifact_dir = base.path().join(".nika/artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();

        // Create two fake CAS files
        let cas_dir = base.path().join(".nika/media/store/ab");
        std::fs::create_dir_all(&cas_dir).unwrap();
        let cas_file1 = cas_dir.join("file1");
        let cas_file2 = cas_dir.join("file2");
        std::fs::write(&cas_file1, b"image data 1").unwrap();
        std::fs::write(&cas_file2, b"image data 2").unwrap();

        let bindings = ResolvedBindings::default();
        let datastore = RunContext::new();

        let media_refs = vec![
            MediaRef {
                hash: "blake3:hash1".to_string(),
                mime_type: "image/png".to_string(),
                size_bytes: 12,
                path: cas_file1,
                extension: "png".to_string(),
                created_by: "gen_img".to_string(),
                metadata: serde_json::Map::new(),
            },
            MediaRef {
                hash: "blake3:hash2".to_string(),
                mime_type: "image/jpeg".to_string(),
                size_bytes: 12,
                path: cas_file2.clone(),
                extension: "jpg".to_string(),
                created_by: "gen_thumb".to_string(),
                metadata: serde_json::Map::new(),
            },
        ];

        // Specify source by creator task_id
        let spec = ArtifactSpec::Single(ArtifactOutput {
            path: "output/thumb.bin".to_string(),
            source: Some("gen_thumb".to_string()),
            template: None,
            format: Some(ArtifactFormat::Binary),
            mode: None,
        });

        let result = process_task_artifacts(
            "save_thumb",
            "",
            &spec,
            None,
            base.path(),
            None,
            &bindings,
            &datastore,
            &media_refs,
        )
        .await;

        assert_eq!(result.written, 1, "errors: {:?}", result.errors);
        let written = std::fs::read(&result.paths[0]).unwrap();
        assert_eq!(written, b"image data 2");
    }

    // ========== Binary artifact edge cases ==========

    #[tokio::test]
    async fn test_binary_artifact_missing_source_binding_error() {
        let base = tempdir().unwrap();
        let artifact_dir = base.path().join(".nika/artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        let bindings = ResolvedBindings::default();
        let datastore = RunContext::new();

        let spec = ArtifactSpec::Single(ArtifactOutput {
            path: "output.bin".to_string(),
            source: Some("nonexistent_source".to_string()),
            template: None,
            format: Some(ArtifactFormat::Binary),
            mode: None,
        });

        let result = process_task_artifacts(
            "task1",
            "",
            &spec,
            None,
            base.path(),
            None,
            &bindings,
            &datastore,
            &[], // No media refs
        )
        .await;

        assert_eq!(result.written, 0);
        assert_eq!(result.errors.len(), 1);
        assert!(
            result.errors[0].contains("not found"),
            "Error should mention source not found: {}",
            result.errors[0]
        );
    }

    #[tokio::test]
    async fn test_binary_artifact_no_media_no_source_error() {
        let base = tempdir().unwrap();
        let artifact_dir = base.path().join(".nika/artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        let bindings = ResolvedBindings::default();
        let datastore = RunContext::new();

        let spec = ArtifactSpec::Single(ArtifactOutput {
            path: "output.bin".to_string(),
            source: None, // No source specified
            template: None,
            format: Some(ArtifactFormat::Binary),
            mode: None,
        });

        let result = process_task_artifacts(
            "task1",
            "text output",
            &spec,
            None,
            base.path(),
            None,
            &bindings,
            &datastore,
            &[], // No media refs either
        )
        .await;

        assert_eq!(result.written, 0);
        assert_eq!(result.errors.len(), 1);
        assert!(
            result.errors[0].contains("no media"),
            "Error should mention no media: {}",
            result.errors[0]
        );
    }

    // ═══════════════════════════════════════════════════
    // Binary artifact fallback from output JSON
    // ═══════════════════════════════════════════════════

    /// Defense-in-depth: when media_refs is empty but the task output
    /// contains JSON with hash/path fields (e.g., from fetch binary or
    /// builtin media tools before the set_media fix), the artifact
    /// processor should construct a MediaRef from the output and succeed.
    #[tokio::test]
    async fn test_binary_artifact_fallback_from_output_json() {
        let base = tempdir().unwrap();
        let artifact_dir = base.path().join(".nika/artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();

        // Create a CAS file that the fallback MediaRef will point to
        let cas_dir = base.path().join(".nika/media/store/ab");
        std::fs::create_dir_all(&cas_dir).unwrap();
        let cas_file = cas_dir.join("fallback_cas");
        std::fs::write(&cas_file, b"fake png data").unwrap();

        let bindings = ResolvedBindings::default();
        let datastore = RunContext::new();

        let spec = ArtifactSpec::Single(ArtifactOutput {
            path: "output.png".to_string(),
            source: None,
            template: None,
            format: Some(ArtifactFormat::Binary),
            mode: None,
        });

        // Task output is JSON string with hash/path (like fetch binary returns)
        let output_json = serde_json::json!({
            "hash": "blake3:fallback_cas",
            "mime_type": "image/png",
            "size_bytes": 13,
            "path": cas_file.to_string_lossy(),
        });

        let result = process_task_artifacts(
            "task_fallback",
            &output_json.to_string(),
            &spec,
            None,
            base.path(),
            None,
            &bindings,
            &datastore,
            &[], // Empty media_refs — simulates pre-fix state
        )
        .await;

        assert_eq!(
            result.written, 1,
            "Fallback should write 1 artifact, errors: {:?}",
            result.errors
        );
        assert!(
            result.errors.is_empty(),
            "No errors expected: {:?}",
            result.errors
        );
    }

    // ═══════════════════════════════════════════════════
    // Binary artifact mode validation tests
    // ═══════════════════════════════════════════════════

    fn setup_binary_mode_fixtures() -> (
        tempfile::TempDir,
        Vec<crate::media::MediaRef>,
        ResolvedBindings,
        RunContext,
    ) {
        use crate::media::MediaRef;
        let base = tempdir().unwrap();
        std::fs::create_dir_all(base.path().join(".nika/artifacts")).unwrap();
        let cas_dir = base.path().join(".nika/media/store/ab");
        std::fs::create_dir_all(&cas_dir).unwrap();
        let cas_file = cas_dir.join("testbin");
        std::fs::write(&cas_file, b"binary payload").unwrap();
        let media_refs = vec![MediaRef {
            hash: "blake3:testbin".to_string(),
            mime_type: "application/octet-stream".to_string(),
            size_bytes: 14,
            path: cas_file,
            extension: "bin".to_string(),
            created_by: "producer".to_string(),
            metadata: serde_json::Map::new(),
        }];
        (
            base,
            media_refs,
            ResolvedBindings::default(),
            RunContext::new(),
        )
    }

    #[tokio::test]
    async fn test_binary_mode_append_is_rejected() {
        let (base, media_refs, bindings, datastore) = setup_binary_mode_fixtures();
        let spec = ArtifactSpec::Single(ArtifactOutput {
            path: "output.bin".to_string(),
            source: None,
            template: None,
            format: Some(ArtifactFormat::Binary),
            mode: Some(ArtifactMode::Append),
        });
        let result = process_task_artifacts(
            "producer",
            "",
            &spec,
            None,
            base.path(),
            None,
            &bindings,
            &datastore,
            &media_refs,
        )
        .await;
        assert_eq!(result.written, 0, "Append mode must be rejected for binary");
        assert_eq!(result.errors.len(), 1);
        assert!(
            result.errors[0].contains("Binary artifacts do not support append mode"),
            "got: {}",
            result.errors[0]
        );
    }

    #[tokio::test]
    async fn test_binary_mode_unique_is_rejected() {
        let (base, media_refs, bindings, datastore) = setup_binary_mode_fixtures();
        let spec = ArtifactSpec::Single(ArtifactOutput {
            path: "output.bin".to_string(),
            source: None,
            template: None,
            format: Some(ArtifactFormat::Binary),
            mode: Some(ArtifactMode::Unique),
        });
        let result = process_task_artifacts(
            "producer",
            "",
            &spec,
            None,
            base.path(),
            None,
            &bindings,
            &datastore,
            &media_refs,
        )
        .await;
        assert_eq!(result.written, 0, "Unique mode must be rejected for binary");
        assert_eq!(result.errors.len(), 1);
        assert!(
            result.errors[0].contains("Binary artifacts do not support unique mode"),
            "got: {}",
            result.errors[0]
        );
    }

    #[tokio::test]
    async fn test_binary_mode_overwrite_succeeds() {
        let (base, media_refs, bindings, datastore) = setup_binary_mode_fixtures();
        let spec = ArtifactSpec::Single(ArtifactOutput {
            path: "output.bin".to_string(),
            source: None,
            template: None,
            format: Some(ArtifactFormat::Binary),
            mode: Some(ArtifactMode::Overwrite),
        });
        let result = process_task_artifacts(
            "producer",
            "",
            &spec,
            None,
            base.path(),
            None,
            &bindings,
            &datastore,
            &media_refs,
        )
        .await;
        assert_eq!(
            result.written, 1,
            "Overwrite should work, errors: {:?}",
            result.errors
        );
        assert!(result.errors.is_empty());
        assert_eq!(std::fs::read(&result.paths[0]).unwrap(), b"binary payload");
    }

    #[tokio::test]
    async fn test_binary_mode_fail_rejects_existing_file() {
        let (base, media_refs, bindings, datastore) = setup_binary_mode_fixtures();
        // Pre-create the target so fail mode triggers
        let target = base.path().join("./artifacts/output.bin");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, b"existing data").unwrap();
        let spec = ArtifactSpec::Single(ArtifactOutput {
            path: "output.bin".to_string(),
            source: None,
            template: None,
            format: Some(ArtifactFormat::Binary),
            mode: Some(ArtifactMode::Fail),
        });
        let result = process_task_artifacts(
            "producer",
            "",
            &spec,
            None,
            base.path(),
            None,
            &bindings,
            &datastore,
            &media_refs,
        )
        .await;
        assert_eq!(result.written, 0, "Fail mode should reject existing file");
        assert_eq!(result.errors.len(), 1);
        assert!(
            result.errors[0].contains("already exists"),
            "got: {}",
            result.errors[0]
        );
    }

    #[tokio::test]
    async fn test_binary_mode_fail_succeeds_for_new_file() {
        let (base, media_refs, bindings, datastore) = setup_binary_mode_fixtures();
        let spec = ArtifactSpec::Single(ArtifactOutput {
            path: "fresh_output.bin".to_string(),
            source: None,
            template: None,
            format: Some(ArtifactFormat::Binary),
            mode: Some(ArtifactMode::Fail),
        });
        let result = process_task_artifacts(
            "producer",
            "",
            &spec,
            None,
            base.path(),
            None,
            &bindings,
            &datastore,
            &media_refs,
        )
        .await;
        assert_eq!(
            result.written, 1,
            "Fail mode should succeed for new file, errors: {:?}",
            result.errors
        );
        assert!(result.errors.is_empty());
        assert_eq!(std::fs::read(&result.paths[0]).unwrap(), b"binary payload");
    }

    // ========== Media binding template tests (source_task_id tracking) ==========

    #[test]
    fn test_path_bindings_media_hash_via_source_task() {
        use crate::media::MediaRef;
        use crate::store::TaskResult;
        use std::sync::Arc;
        use std::time::Duration;

        let datastore = RunContext::new();
        let mut task_result =
            TaskResult::success_str("LLM text output".to_string(), Duration::from_millis(100));
        task_result.media = vec![MediaRef {
            hash: "blake3:af1349b9".to_string(),
            mime_type: "image/png".to_string(),
            size_bytes: 4096,
            path: std::path::PathBuf::from("/tmp/cas/af/1349b9"),
            extension: "png".to_string(),
            created_by: "gen_img".to_string(),
            metadata: serde_json::Map::new(),
        }];
        datastore.insert(Arc::from("gen_img"), task_result);

        let mut bindings = ResolvedBindings::new();
        bindings.set_with_source("img", serde_json::json!("LLM text output"), "gen_img");

        let result = resolve_artifact_path_bindings(
            "output/{{with.img.media[0].hash}}.bin",
            "",
            &bindings,
            &datastore,
        );
        assert_eq!(
            result, "output/blake3_af1349b9.bin",
            "Media hash should resolve via source task ID, with : sanitized to _"
        );
    }

    #[test]
    fn test_path_bindings_media_extension_via_source_task() {
        use crate::media::MediaRef;
        use crate::store::TaskResult;
        use std::sync::Arc;
        use std::time::Duration;

        let datastore = RunContext::new();
        let mut task_result =
            TaskResult::success_str("output".to_string(), Duration::from_millis(50));
        task_result.media = vec![MediaRef {
            hash: "blake3:deadbeef".to_string(),
            mime_type: "image/png".to_string(),
            size_bytes: 1024,
            path: std::path::PathBuf::from("/tmp/cas/de/adbeef"),
            extension: "png".to_string(),
            created_by: "gen_img".to_string(),
            metadata: serde_json::Map::new(),
        }];
        datastore.insert(Arc::from("gen_img"), task_result);

        let mut bindings = ResolvedBindings::new();
        bindings.set_with_source("img", serde_json::json!("output"), "gen_img");

        let result = resolve_artifact_path_bindings(
            "output/{{with.img.media[0].extension}}/result.bin",
            "",
            &bindings,
            &datastore,
        );
        assert_eq!(
            result, "output/png/result.bin",
            "Media extension should resolve via source task ID"
        );
    }

    #[test]
    fn test_path_bindings_media_without_source_task_unresolved() {
        let bindings = ResolvedBindings::new();
        let datastore = RunContext::new();

        let result = resolve_artifact_path_bindings(
            "output/{{with.img.media[0].hash}}.bin",
            "",
            &bindings,
            &datastore,
        );
        assert_eq!(
            result, "output/{{with.img.media[0].hash}}.bin",
            "Without source task tracking, media path should remain unresolved"
        );
    }

    #[tokio::test]
    async fn test_binary_artifact_source_via_binding_alias() {
        use crate::media::MediaRef;
        use crate::store::TaskResult;
        use std::sync::Arc;
        use std::time::Duration;

        let base = tempdir().unwrap();
        let artifact_dir = base.path().join(".nika/artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();

        let cas_dir = base.path().join(".nika/media/store/ab");
        std::fs::create_dir_all(&cas_dir).unwrap();
        let cas_file = cas_dir.join("cdef1234");
        let binary_data = b"\x89PNG fake image";
        std::fs::write(&cas_file, binary_data).unwrap();

        let datastore = RunContext::new();
        let mut task_result =
            TaskResult::success_str("generated image".to_string(), Duration::from_millis(100));
        task_result.media = vec![MediaRef {
            hash: "blake3:abcdef1234".to_string(),
            mime_type: "image/png".to_string(),
            size_bytes: binary_data.len() as u64,
            path: cas_file.clone(),
            extension: "png".to_string(),
            created_by: "gen_img".to_string(),
            metadata: serde_json::Map::new(),
        }];
        datastore.insert(Arc::from("gen_img"), task_result);

        let mut bindings = ResolvedBindings::new();
        bindings.set_with_source("img", serde_json::json!("generated image"), "gen_img");

        let media_refs = vec![MediaRef {
            hash: "blake3:abcdef1234".to_string(),
            mime_type: "image/png".to_string(),
            size_bytes: binary_data.len() as u64,
            path: cas_file,
            extension: "png".to_string(),
            created_by: "gen_img".to_string(),
            metadata: serde_json::Map::new(),
        }];

        let spec = ArtifactSpec::Single(ArtifactOutput {
            path: "output/image.bin".to_string(),
            source: Some("img".to_string()),
            template: None,
            format: Some(ArtifactFormat::Binary),
            mode: None,
        });

        let result = process_task_artifacts(
            "save_img",
            "",
            &spec,
            None,
            base.path(),
            None,
            &bindings,
            &datastore,
            &media_refs,
        )
        .await;

        assert_eq!(
            result.written, 1,
            "Binary artifact should resolve via binding alias indirection, errors: {:?}",
            result.errors
        );
        assert!(
            result.errors.is_empty(),
            "No errors expected: {:?}",
            result.errors
        );

        let written = std::fs::read(&result.paths[0]).unwrap();
        assert_eq!(
            written, binary_data,
            "Binary content should match CAS source"
        );
    }

    #[tokio::test]
    async fn test_binary_artifact_path_with_media_extension_template() {
        use crate::media::MediaRef;
        use crate::store::TaskResult;
        use std::sync::Arc;
        use std::time::Duration;

        let base = tempdir().unwrap();
        let artifact_dir = base.path().join(".nika/artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();

        let cas_dir = base.path().join(".nika/media/store/xx");
        std::fs::create_dir_all(&cas_dir).unwrap();
        let cas_file = cas_dir.join("yy1234");
        let binary_data = b"image bytes";
        std::fs::write(&cas_file, binary_data).unwrap();

        let datastore = RunContext::new();
        let mut task_result =
            TaskResult::success_str("done".to_string(), Duration::from_millis(50));
        task_result.media = vec![MediaRef {
            hash: "blake3:xxyy1234".to_string(),
            mime_type: "image/jpeg".to_string(),
            size_bytes: binary_data.len() as u64,
            path: cas_file.clone(),
            extension: "jpg".to_string(),
            created_by: "gen_img".to_string(),
            metadata: serde_json::Map::new(),
        }];
        datastore.insert(Arc::from("gen_img"), task_result);

        let mut bindings = ResolvedBindings::new();
        bindings.set_with_source("img", serde_json::json!("done"), "gen_img");

        let media_refs = vec![MediaRef {
            hash: "blake3:xxyy1234".to_string(),
            mime_type: "image/jpeg".to_string(),
            size_bytes: binary_data.len() as u64,
            path: cas_file,
            extension: "jpg".to_string(),
            created_by: "gen_img".to_string(),
            metadata: serde_json::Map::new(),
        }];

        let spec = ArtifactSpec::Single(ArtifactOutput {
            path: "output/result.{{with.img.media[0].extension}}".to_string(),
            source: None,
            template: None,
            format: Some(ArtifactFormat::Binary),
            mode: None,
        });

        let result = process_task_artifacts(
            "gen_img",
            "",
            &spec,
            None,
            base.path(),
            None,
            &bindings,
            &datastore,
            &media_refs,
        )
        .await;

        assert_eq!(result.written, 1, "errors: {:?}", result.errors);

        let path_str = result.paths[0].display().to_string();
        assert!(
            path_str.ends_with("result.jpg"),
            "Path should end with resolved extension 'result.jpg', got: {}",
            path_str
        );

        let written = std::fs::read(&result.paths[0]).unwrap();
        assert_eq!(written, binary_data);
    }

    /// Auto-promote to Binary when task produced media but format is unspecified.
    ///
    /// Before this fix, `artifact: { path: output.png }` without `format: binary`
    /// would write the JSON metadata string instead of the actual image bytes.
    #[tokio::test]
    async fn test_auto_promote_binary_when_media_refs_present() {
        let base = tempdir().unwrap();
        let artifact_dir = base.path().join(".nika/artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();

        // Create a CAS-like file with binary content
        let cas_dir = base.path().join("cas");
        std::fs::create_dir_all(&cas_dir).unwrap();
        let binary_data = b"\x89PNG\r\n\x1a\n fake png data";
        let cas_file = cas_dir.join("testcas");
        std::fs::write(&cas_file, binary_data).unwrap();

        let media_refs = vec![MediaRef {
            hash: "blake3:abc123".to_string(),
            mime_type: "image/png".to_string(),
            size_bytes: binary_data.len() as u64,
            path: cas_file,
            extension: "png".to_string(),
            created_by: "gen_img".to_string(),
            metadata: serde_json::Map::new(),
        }];

        let bindings = ResolvedBindings::default();
        let datastore = RunContext::new();

        // NO format specified — should auto-detect Binary from media_refs
        let spec = ArtifactSpec::Single(ArtifactOutput {
            path: "output.png".to_string(),
            source: None,
            template: None,
            format: None, // <-- deliberately omitted
            mode: None,
        });

        let json_output = r#"{"hash":"blake3:abc123","mime_type":"image/png","size_bytes":22}"#;

        let result = process_task_artifacts(
            "gen_img",
            json_output,
            &spec,
            None,
            base.path(),
            None,
            &bindings,
            &datastore,
            &media_refs,
        )
        .await;

        assert_eq!(result.written, 1, "errors: {:?}", result.errors);

        // The artifact should contain the actual binary data, NOT the JSON string
        let written = std::fs::read(&result.paths[0]).unwrap();
        assert_eq!(
            written, binary_data,
            "Artifact should contain binary image data, not JSON metadata"
        );
        assert!(
            !String::from_utf8_lossy(&written).contains("blake3:"),
            "Artifact must not contain JSON metadata hash"
        );
    }

    /// When format IS explicitly set to Text, media_refs should NOT auto-promote.
    #[tokio::test]
    async fn test_explicit_text_format_not_overridden_by_media_refs() {
        let base = tempdir().unwrap();
        let artifact_dir = base.path().join(".nika/artifacts");
        std::fs::create_dir_all(&artifact_dir).unwrap();

        let media_refs = vec![MediaRef {
            hash: "blake3:abc123".to_string(),
            mime_type: "image/png".to_string(),
            size_bytes: 100,
            path: PathBuf::from("/nonexistent"),
            extension: "png".to_string(),
            created_by: "gen_img".to_string(),
            metadata: serde_json::Map::new(),
        }];

        let bindings = ResolvedBindings::default();
        let datastore = RunContext::new();

        // Explicit format: text — should NOT auto-promote
        let spec = ArtifactSpec::Single(ArtifactOutput {
            path: "metadata.json".to_string(),
            source: None,
            template: None,
            format: Some(ArtifactFormat::Text),
            mode: None,
        });

        let json_output = r#"{"hash":"blake3:abc123"}"#;

        let result = process_task_artifacts(
            "gen_img",
            json_output,
            &spec,
            None,
            base.path(),
            None,
            &bindings,
            &datastore,
            &media_refs,
        )
        .await;

        assert_eq!(result.written, 1, "errors: {:?}", result.errors);

        // Should contain the text output, not binary
        let written = std::fs::read_to_string(&result.paths[0]).unwrap();
        assert!(
            written.contains("blake3:"),
            "Explicit text format should write JSON metadata"
        );
    }
}
