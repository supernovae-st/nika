// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Invoke verb bridge for TaskExecutor.
//!
//! - `nika:*` builtin path → delegates to `nika_verb_invoke::run()` via
//!   the `BuiltinRouter` kernel trait (S13-C2).
//! - MCP path → stays inline in this file (media processing coupling
//!   deferred to S14).

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

use tracing::instrument;
use uuid::Uuid;

use nika_kernel::builtin::{BuiltinError, BuiltinRouter as KernelBuiltinRouter};
use nika_kernel::caps::InvokeCaps;
use nika_kernel::mcp::{McpError, McpPool};
use nika_verb_invoke::{is_builtin as is_nika_builtin, InvokeInput, VerbInvokeError};

use crate::ast::InvokeParams;
use crate::binding::{template_resolve, ResolvedBindings};
use crate::error::NikaError;
use crate::event::EventKind;
use crate::mcp::McpClient;
use crate::runtime::BuiltinToolRouter;
use crate::store::RunContext;
use crate::util::INVOKE_TASK_DEADLINE;

use super::verbs::coerce_json_types;

// ═══════════════════════════════════════════════════════════════════════
// BuiltinRouterAdapter — bridges engine's concrete BuiltinToolRouter
// to the nika-kernel BuiltinRouter trait expected by nika-verb-invoke.
// ═══════════════════════════════════════════════════════════════════════

/// Adapter that implements `nika_kernel::builtin::BuiltinRouter` over the
/// engine's concrete `BuiltinToolRouter`.
///
/// The adapter re-adds the `nika:` prefix (the kernel trait receives bare
/// tool names like `"log"`; the engine router expects `"nika:log"`) and
/// maps `NikaError` to `BuiltinError::Other`.
#[derive(Clone)]
struct BuiltinRouterAdapter {
    inner: Arc<BuiltinToolRouter>,
}

impl std::fmt::Debug for BuiltinRouterAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BuiltinRouterAdapter").finish_non_exhaustive()
    }
}

impl KernelBuiltinRouter for BuiltinRouterAdapter {
    fn dispatch<'a>(
        &'a self,
        tool: &'a str,
        args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, BuiltinError>> + Send + 'a>> {
        let inner = Arc::clone(&self.inner);
        let tool_with_prefix = format!("nika:{tool}");
        Box::pin(async move {
            inner.dispatch(&tool_with_prefix, args).await.map_err(|e| {
                BuiltinError::Other {
                    tool: tool_with_prefix,
                    reason: e.to_string(),
                }
            })
        })
    }

    fn knows(&self, tool: &str) -> bool {
        // Engine's is_builtin takes the prefixed name.
        BuiltinToolRouter::is_builtin(&format!("nika:{tool}"))
    }
}

// ═══════════════════════════════════════════════════════════════════════
// NoopMcpPool — MCP dispatch still lives in the engine bridge (S13).
// The verb crate's non-builtin path is unreachable in S13 because the
// bridge checks `is_builtin` before calling `run()`. This noop pool only
// exists to satisfy `InvokeCaps::new()`'s type requirements.
// ═══════════════════════════════════════════════════════════════════════

/// Null `BlobStore` shim — the builtin path doesn't actually use blobs,
/// but `InvokeCaps::new` requires a `&dyn BlobStore` parameter.
#[derive(Debug)]
struct NullBlobStore;

#[async_trait::async_trait]
impl nika_kernel::store::BlobStore for NullBlobStore {
    async fn put(
        &self,
        _data: tokio_util::bytes::Bytes,
        _mime_type: &str,
    ) -> Result<nika_kernel::store::BlobMetadata, nika_kernel::store::BlobError> {
        Err(nika_kernel::store::BlobError::Io {
            reason: "null blob store (engine bridge shim)".to_string(),
        })
    }

    async fn get(
        &self,
        _hash: &str,
    ) -> Result<tokio_util::bytes::Bytes, nika_kernel::store::BlobError> {
        Err(nika_kernel::store::BlobError::NotFound {
            hash: "(null)".to_string(),
        })
    }

    async fn exists(&self, _hash: &str) -> bool {
        false
    }

    async fn delete(&self, _hash: &str) -> Result<(), nika_kernel::store::BlobError> {
        Ok(())
    }

    async fn stat(
        &self,
        _hash: &str,
    ) -> Result<nika_kernel::store::BlobMetadata, nika_kernel::store::BlobError> {
        Err(nika_kernel::store::BlobError::NotFound {
            hash: "(null)".to_string(),
        })
    }
}

/// Null `HttpClient` shim — same rationale as `NullBlobStore`.
#[derive(Debug)]
struct NullHttpClient;

#[async_trait::async_trait]
impl nika_kernel::http::HttpClient for NullHttpClient {
    async fn send(
        &self,
        _request: nika_kernel::http::HttpRequest,
    ) -> Result<nika_kernel::http::HttpResponse, nika_kernel::http::HttpError> {
        Err(nika_kernel::http::HttpError::Unsupported {
            feature: "null http client (engine bridge shim)".to_string(),
        })
    }
}

#[derive(Debug)]
struct NoopMcpPool;

impl McpPool for NoopMcpPool {
    fn call_tool<'a>(
        &'a self,
        server: &'a str,
        _tool: &'a str,
        _args: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, McpError>> + Send + 'a>> {
        Box::pin(async move {
            Err(McpError::ServerNotFound {
                server: server.to_string(),
            })
        })
    }

    fn read_resource<'a>(
        &'a self,
        uri: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String, McpError>> + Send + 'a>> {
        Box::pin(async move {
            Err(McpError::ResourceFailed {
                uri: uri.to_string(),
                reason: "engine bridge routes MCP inline in S13".to_string(),
            })
        })
    }

    fn has_server(&self, _server: &str) -> bool {
        false
    }
}

/// Convert a `VerbInvokeError` to `NikaError` at the bridge boundary.
fn map_verb_invoke_error(err: VerbInvokeError) -> NikaError {
    match err {
        VerbInvokeError::Builtin(e) => NikaError::BuiltinToolError {
            tool: match &e {
                BuiltinError::InvalidArgs { tool, .. }
                | BuiltinError::Io { tool, .. }
                | BuiltinError::Parse { tool, .. }
                | BuiltinError::Timeout { tool }
                | BuiltinError::Schema { tool, .. }
                | BuiltinError::Denied { tool, .. }
                | BuiltinError::Other { tool, .. } => tool.clone(),
                BuiltinError::AssertionFailed { .. } => "nika:assert".to_string(),
            },
            reason: e.to_string(),
        },
        // W14-B0: unreachable in S14 (bridge handles the MCP path inline
        // via McpClient directly; see run_invoke's non-builtin branch).
        // Pre-emptively mapped to accurate semantics for when dispatch()
        // becomes the live path in S15. InvokeParamError was wrong — MCP
        // failures are runtime/connectivity issues, not user-param bugs.
        VerbInvokeError::Mcp(e) => match e {
            nika_kernel::mcp::McpError::ServerNotFound { server } => NikaError::McpNotConnected {
                name: server,
            },
            nika_kernel::mcp::McpError::ToolCallFailed {
                tool, reason, ..
            } => NikaError::McpToolError {
                tool,
                reason,
                error_code: None,
            },
            nika_kernel::mcp::McpError::ResourceFailed { uri, reason } => {
                NikaError::McpResourceNotFound {
                    uri: format!("{uri} ({reason})"),
                }
            }
            nika_kernel::mcp::McpError::Connection { reason } => NikaError::McpProtocolError {
                reason,
            },
        },
        VerbInvokeError::Cancelled { task_id } => NikaError::TaskCancelled {
            task_id,
            reason: "workflow cancelled during invoke".to_string(),
        },
        VerbInvokeError::Timeout { duration_ms } => NikaError::McpTimeout {
            name: "builtin".to_string(),
            operation: "invoke".to_string(),
            timeout_secs: duration_ms / 1000,
        },
        VerbInvokeError::InvalidParams { reason } => NikaError::InvokeParamError { reason },
        VerbInvokeError::Validation { reason } => NikaError::ValidationError { reason },
        // VerbInvokeError is `#[non_exhaustive]` (invariant #25). S15 will
        // grow variants like `ResultTooLarge` when McpPool trait gains size
        // caps. Unmapped variants fall through to a generic McpProtocolError
        // here and must be explicitly mapped in the commit that introduces
        // them.
        other => NikaError::McpProtocolError {
            reason: format!("invoke: unmapped verb error variant: {other:?}"),
        },
    }
}

/// Recursively redact secrets from a JSON value for safe event logging.
fn redact_value(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::String(s) => serde_json::Value::String(crate::util::redact_secrets(s)),
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(redact_value).collect())
        }
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), redact_value(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}
use super::TaskExecutor;

impl TaskExecutor {
    /// Execute an invoke action (MCP tool call or resource read)
    ///
    /// # Template Resolution
    ///
    /// Templates like `{{with.variable}}` in params are resolved before calling the MCP tool.
    /// This enables for_each iterations to pass dynamic values to MCP tools.
    #[instrument(skip(self, bindings, datastore), fields(mcp = ?invoke.mcp))]
    pub(super) async fn run_invoke(
        &self,
        task_id: &Arc<str>,
        invoke: &InvokeParams,
        bindings: &ResolvedBindings,
        datastore: &RunContext,
    ) -> Result<String, NikaError> {
        // Validate invoke params (tool XOR resource)
        invoke.validate()?;

        // Generate unique call_id for correlation
        let call_id = Uuid::new_v4().to_string();
        let start_time = Instant::now();

        // Resolve templates FIRST, then emit event with resolved params
        // This fixes the bug where TUI showed literal {{with.topic}} instead of resolved values
        let resolved_params = if let Some(ref original_params) = invoke.params {
            let params_str = serde_json::to_string(original_params).map_err(|e| {
                NikaError::InvokeParamError {
                    reason: format!("Failed to serialize params: {}", e),
                }
            })?;
            // Short-circuit: skip template resolve + re-parse if no templates present
            if params_str.contains("{{") {
                let resolved_str = template_resolve(&params_str, bindings, datastore)?;
                let mut resolved = serde_json::from_str::<serde_json::Value>(&resolved_str)
                    .map_err(|e| NikaError::InvokeParamError {
                        reason: format!(
                            "Failed to parse resolved params '{}': {}",
                            resolved_str, e
                        ),
                    })?;
                // Coerce string values back to native JSON types after template resolution
                coerce_json_types(&mut resolved);
                Some(resolved)
            } else {
                Some(original_params.clone())
            }
        } else {
            None
        };

        // Resolve templates in tool name, resource, and mcp server name
        let resolved_tool = match &invoke.tool {
            Some(t) => Some(template_resolve(t, bindings, datastore)?.into_owned()),
            None => None,
        };
        let resolved_resource = match &invoke.resource {
            Some(r) => Some(template_resolve(r, bindings, datastore)?.into_owned()),
            None => None,
        };
        let resolved_mcp = match &invoke.mcp {
            Some(m) => Some(template_resolve(m, bindings, datastore)?.into_owned()),
            None => None,
        };

        // S14-BUG2: The McpInvoke event is emitted by nika-verb-invoke::run()
        // for the builtin path, and by the inline MCP code below for the
        // non-builtin path. Emitting here would duplicate for builtins.
        //
        // KNOWN REGRESSION (tracked for S15): the verb crate's McpInvoke
        // emission does not currently apply `redact_secrets` to params.
        // When dispatch() becomes the live path in S15, add a redaction
        // hook field to InvokeInput (params_display: Option<Arc<Value>>)
        // so the bridge can pre-redact before handing off.

        // ═══════════════════════════════════════════════════════════════
        // Builtin path — delegate to nika-verb-invoke via BuiltinRouter trait.
        //
        // The verb crate is the canonical emitter for McpInvoke + McpResponse.
        //
        // Media staging stays in the bridge because MediaRef construction
        // couples to nika-media types that don't belong in the verb crate
        // (S14+ extracts this via an InvokeCaps media channel).
        // ═══════════════════════════════════════════════════════════════
        if let Some(tool) = &resolved_tool {
            if is_nika_builtin(tool) {
                let adapter = BuiltinRouterAdapter {
                    inner: Arc::clone(&self.builtin_router),
                };
                let noop_mcp = NoopMcpPool;
                let clock = nika_clock::SystemClock;
                let fs = nika_fs::TokioFs;
                // Need a blob store — use the engine's CAS for consistency.
                // BlobStore trait isn't yet used by the builtin path, but
                // InvokeCaps::new requires it for the type.
                use nika_kernel::store::BlobStore;
                let blob_shim = NullBlobStore;
                let blobs: &dyn BlobStore = &blob_shim;
                // HttpClient also required by InvokeCaps — use a noop.
                let http_shim = NullHttpClient;
                use nika_kernel::http::HttpClient;
                let http: &dyn HttpClient = &http_shim;
                // Policy: clone the enforcer to avoid !Send RwLockReadGuard
                // across .await (GATE-S13-3).
                let policy_clone = self.policy_enforcer.read().clone();

                let caps = InvokeCaps::new(
                    &adapter,
                    &noop_mcp,
                    &fs,
                    &fs,
                    http,
                    blobs,
                    &policy_clone,
                    &clock,
                    &self.cancel_token,
                );

                let input = InvokeInput {
                    tool: Some(tool.clone()),
                    resource: None,
                    mcp_server: None,
                    params: resolved_params.clone(),
                    task_id: Arc::clone(task_id),
                    call_id: call_id.clone(),
                };

                // The bridge has already emitted McpInvoke above; the verb
                // crate emits its own, but we only need its McpResponse.
                // Accept the extra McpInvoke — no TUI impact.
                match nika_verb_invoke::run(&input, &caps, &self.event_log).await {
                    Ok(result_str) => {
                        // Parse the returned JSON string for media staging.
                        let result_value: serde_json::Value =
                            serde_json::from_str(&result_str)
                                .unwrap_or_else(|_| serde_json::Value::String(result_str.clone()));

                        // Stage MediaRef if the builtin tool returned CAS
                        // media data (same as before — bridge responsibility
                        // because MediaRef lives in nika-media).
                        if let serde_json::Value::Object(ref obj) = result_value {
                            if let (Some(hash), Some(mime_type)) = (
                                obj.get("hash").and_then(|v| v.as_str()),
                                obj.get("mime_type").and_then(|v| v.as_str()),
                            ) {
                                let size_bytes = obj
                                    .get("size_bytes")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or_else(|| {
                                        tracing::warn!(
                                            hash = %hash,
                                            "size_bytes missing from media tool result — defaulting to 0"
                                        );
                                        0
                                    });
                                let path = obj
                                    .get("path")
                                    .and_then(|v| v.as_str())
                                    .map(std::path::PathBuf::from)
                                    .unwrap_or_default();
                                let extension = obj
                                    .get("extension")
                                    .and_then(|v| v.as_str())
                                    .map(String::from)
                                    .unwrap_or_else(|| {
                                        crate::media::detect::mime_to_extension(mime_type)
                                    });
                                let media_ref = crate::media::MediaRef {
                                    hash: hash.to_string(),
                                    mime_type: mime_type.to_string(),
                                    size_bytes,
                                    path,
                                    extension,
                                    created_by: task_id.to_string(),
                                    metadata: obj
                                        .get("metadata")
                                        .and_then(|v| v.as_object())
                                        .cloned()
                                        .unwrap_or_default(),
                                };
                                datastore.set_media(task_id, vec![media_ref]);
                            }
                        }

                        return Ok(result_str);
                    }
                    Err(e) => {
                        return Err(map_verb_invoke_error(e));
                    }
                }
            }
        }

        // EMIT: McpInvoke event for the non-builtin (MCP) path.
        //
        // S14-BUG2 regression fix: the original bridge emitted this
        // before the builtin-vs-MCP branch. S14-BUG2 removed it for the
        // builtin path (the verb crate emits it now) but inadvertently
        // killed it for the MCP path too — test_execute_invoke_emits_mcp_events
        // caught the regression. Re-emitted here, inside the non-builtin
        // branch, with the same redact_value call chain.
        let mcp_server = resolved_mcp
            .clone()
            .unwrap_or_else(|| "builtin".to_string());
        self.event_log.emit(EventKind::McpInvoke {
            task_id: Arc::clone(task_id),
            call_id: call_id.clone(),
            mcp_server,
            tool: resolved_tool.clone(),
            resource: resolved_resource.clone(),
            params: resolved_params.as_ref().map(|p| Arc::new(redact_value(p))),
        });

        // Get or create MCP client (real or mock depending on config)
        // Mcp is Option<String>, validate() guarantees Some for non-builtin tools
        let mcp_name = resolved_mcp
            .as_deref()
            .ok_or_else(|| NikaError::ValidationError {
                reason: "MCP server name required for non-builtin tools".to_string(),
            })?;

        // Race MCP work against both deadline timeout AND cancellation token.
        // This ensures MCP calls abort promptly on workflow cancellation instead of
        // waiting up to INVOKE_TASK_DEADLINE (5 min).
        let mcp_work = async {
            let client = self.get_mcp_client(mcp_name).await?;
            const MAX_MCP_RESULT_SIZE: usize = 50 * 1024 * 1024;

            let result = if let Some(tool) = &resolved_tool {
                // Tool call path - use already-resolved params
                let params = resolved_params.clone().unwrap_or(serde_json::Value::Null);
                // Use call_tool_with_retry_events for McpRetry event emission
                let tool_result = client
                    .call_tool_with_retry_events(tool, params, task_id, &self.event_log)
                    .await?;

                // Enforce 50MB size limit on MCP tool results to prevent OOM
                let result_size = tool_result.content_size_bytes();
                if result_size > MAX_MCP_RESULT_SIZE {
                    return Err(NikaError::McpToolError {
                        tool: tool.clone(),
                        reason: format!(
                            "MCP tool result exceeds 50MB limit ({} bytes)",
                            result_size
                        ),
                        error_code: None,
                    });
                }

                // Check if tool returned an error
                if tool_result.is_error {
                    // Emit response event before returning error
                    let duration_ms = start_time.elapsed().as_millis() as u64;
                    let error_text = tool_result.text();
                    let err_val = serde_json::json!({"error": error_text.clone()});
                    self.event_log.emit(EventKind::McpResponse {
                        task_id: Arc::clone(task_id),
                        call_id: call_id.clone(),
                        output_len: error_text.len(),
                        duration_ms,
                        cached: false,
                        is_error: true,
                        response: Some(Arc::new(redact_value(&err_val))),
                    });
                    return Err(NikaError::McpToolError {
                        tool: tool.clone(),
                        reason: error_text,
                        error_code: None,
                    });
                }

                // Process media content (if any)
                if tool_result.has_media() {
                    use crate::mcp::types::ContentBlock;
                    use crate::media::{CasStore, MediaProcessor};

                    let media_blocks = tool_result.media_blocks();
                    let content_types: Vec<String> = media_blocks
                        .iter()
                        .filter_map(|b| match b {
                            ContentBlock::Text { .. } => {
                                tracing::warn!(
                                    "Unexpected text block in media processing, skipping"
                                );
                                None
                            }
                            ContentBlock::Image { .. } => Some("image".to_string()),
                            ContentBlock::Audio { .. } => Some("audio".to_string()),
                            ContentBlock::Resource(_) => Some("resource".to_string()),
                            ContentBlock::ResourceLink { .. } => Some("resource_link".to_string()),
                        })
                        .collect();

                    self.event_log.emit(EventKind::MediaExtracted {
                        task_id: Arc::clone(task_id),
                        block_count: media_blocks.len() as u32,
                        content_types,
                    });

                    let workspace_root = datastore.workspace_root();
                    let store = CasStore::workspace_default(&workspace_root);
                    // Use shared per-run budget from RunContext (not a fresh one per invoke)
                    let processor = MediaProcessor::with_shared_budget(
                        store,
                        std::sync::Arc::clone(datastore.media_budget()),
                    );

                    let process_results = processor
                        .process_all(&tool_result.content, task_id.as_ref())
                        .await;

                    // Process all results: emit events for EVERY block first,
                    // then fail on non-recoverable errors. This ensures the trace
                    // is complete even when the task fails partway through.
                    let mut media_refs = Vec::new();
                    let mut fatal_error: Option<crate::media::MediaError> = None;
                    for result in process_results {
                        match result {
                            Ok((media_ref, store_result)) => {
                                self.event_log.emit(EventKind::MediaProcessed {
                                    task_id: Arc::clone(task_id),
                                    hash: media_ref.hash.clone(),
                                    mime_type: media_ref.mime_type.clone(),
                                    size_bytes: media_ref.size_bytes,
                                });
                                self.event_log.emit(EventKind::MediaStored {
                                    task_id: Arc::clone(task_id),
                                    hash: media_ref.hash.clone(),
                                    path: media_ref.path.display().to_string(),
                                    size_bytes: media_ref.size_bytes,
                                    verified: store_result.verified,
                                    deduplicated: store_result.deduplicated,
                                    pipeline_ms: store_result.pipeline_ms,
                                });
                                media_refs.push(media_ref);
                            }
                            Err((block_index, error)) => {
                                self.event_log.emit(EventKind::MediaStoreFailed {
                                    task_id: Arc::clone(task_id),
                                    hash: String::new(),
                                    reason: format!("block {block_index}: {error}"),
                                });
                                // Capture first non-recoverable error for task failure
                                if fatal_error.is_none() && !error.is_recoverable() {
                                    fatal_error = Some(error);
                                }
                            }
                        }
                    }

                    // If any non-recoverable error occurred, fail the task
                    if let Some(error) = fatal_error {
                        return Err(NikaError::MediaError(error));
                    }

                    // Stage media refs in side-channel for runner to pick up
                    datastore.set_media(task_id, media_refs);
                }

                // Capture cache status before converting to JSON value
                let was_cached = tool_result.was_cached;

                // Text output flows as before (backward compat)
                let text = tool_result.text();
                let json_value = serde_json::from_str(&text).unwrap_or_else(|_| {
                    tracing::trace!(task = %task_id, "MCP tool returned non-JSON text, wrapping as string");
                    serde_json::Value::String(text)
                });
                (json_value, was_cached)
            } else if let Some(resource) = &resolved_resource {
                // Resource read path -- now handles blob data via media pipeline
                let content = client.read_resource(resource).await?;

                // Enforce 50MB size limit on MCP resource reads (mirrors tool call check)
                let resource_size = content.text.as_ref().map(|t| t.len()).unwrap_or(0)
                    + content.blob.as_ref().map(|b| b.len()).unwrap_or(0);
                if resource_size > MAX_MCP_RESULT_SIZE {
                    return Err(NikaError::McpToolError {
                        tool: format!("resource:{}", resource),
                        reason: format!(
                            "MCP resource exceeds 50MB limit ({} bytes)",
                            resource_size
                        ),
                        error_code: None,
                    });
                }

                // If resource has a blob, process it through the media pipeline
                if let Some(blob) = &content.blob {
                    use crate::mcp::types::ContentBlock;
                    use crate::media::{CasStore, MediaProcessor};

                    let mime = content
                        .mime_type
                        .clone()
                        .unwrap_or_else(|| "application/octet-stream".to_string());
                    let block = ContentBlock::Resource(
                        crate::mcp::types::ResourceContent::new(resource.clone())
                            .with_blob(blob.clone())
                            .with_optional_mime(content.mime_type.clone()),
                    );

                    tracing::debug!(
                        task_id = %task_id,
                        resource = %resource,
                        mime = %mime,
                        blob_len = blob.len(),
                        "Resource read returned blob data, processing through media pipeline"
                    );

                    self.event_log.emit(EventKind::MediaExtracted {
                        task_id: Arc::clone(task_id),
                        block_count: 1,
                        content_types: vec!["resource_blob".to_string()],
                    });

                    let workspace_root = datastore.workspace_root();
                    let store = CasStore::workspace_default(&workspace_root);
                    let processor = MediaProcessor::with_shared_budget(
                        store,
                        std::sync::Arc::clone(datastore.media_budget()),
                    );
                    let results = processor.process_all(&[block], task_id.as_ref()).await;
                    let mut media_refs = Vec::new();
                    let mut fatal_error: Option<crate::media::MediaError> = None;
                    for result in results {
                        match result {
                            Ok((media_ref, store_result)) => {
                                self.event_log.emit(EventKind::MediaProcessed {
                                    task_id: Arc::clone(task_id),
                                    hash: media_ref.hash.clone(),
                                    mime_type: media_ref.mime_type.clone(),
                                    size_bytes: media_ref.size_bytes,
                                });
                                self.event_log.emit(EventKind::MediaStored {
                                    task_id: Arc::clone(task_id),
                                    hash: media_ref.hash.clone(),
                                    path: media_ref.path.display().to_string(),
                                    size_bytes: media_ref.size_bytes,
                                    verified: store_result.verified,
                                    deduplicated: store_result.deduplicated,
                                    pipeline_ms: store_result.pipeline_ms,
                                });
                                media_refs.push(media_ref);
                            }
                            Err((idx, error)) => {
                                tracing::warn!(task_id = %task_id, error = %error, "Resource blob media processing failed");
                                self.event_log.emit(EventKind::MediaStoreFailed {
                                    task_id: Arc::clone(task_id),
                                    hash: format!("blob_index:{}", idx),
                                    reason: error.to_string(),
                                });
                                if fatal_error.is_none() && !error.is_recoverable() {
                                    fatal_error = Some(error);
                                }
                            }
                        }
                    }
                    if let Some(error) = fatal_error {
                        return Err(NikaError::MediaError(error));
                    }
                    if !media_refs.is_empty() {
                        datastore.set_media(task_id, media_refs);
                    }
                }

                // Text output (if any)
                let json_value = content
                    .text
                    .map(|t| {
                        serde_json::from_str(&t).unwrap_or_else(|_| {
                            tracing::trace!(task = %task_id, "MCP resource returned non-JSON text, wrapping as string");
                            serde_json::Value::String(t)
                        })
                    })
                    .unwrap_or(serde_json::Value::Null);
                (json_value, false) // Resources are never cached
            } else {
                return Err(NikaError::ValidationError {
                    reason: "invoke: task requires either 'tool' or 'resource' field".to_string(),
                });
            };

            Ok::<(serde_json::Value, Arc<McpClient>, bool), NikaError>((result.0, client, result.1))
        };

        // Use per-task timeout if specified, otherwise fall back to global deadline
        let deadline = invoke
            .timeout
            .map(std::time::Duration::from_secs)
            .unwrap_or(INVOKE_TASK_DEADLINE);

        let mcp_result = tokio::select! {
            result = tokio::time::timeout(deadline, mcp_work) => {
                result.map_err(|_| NikaError::McpTimeout {
                    name: mcp_name.to_string(),
                    operation: format!("invoke task (deadline {}s)", deadline.as_secs()),
                    timeout_secs: deadline.as_secs(),
                })?
            }
            _ = self.cancel_token.cancelled() => {
                Err(NikaError::TaskCancelled {
                    task_id: task_id.to_string(),
                    reason: "workflow cancelled during MCP invoke".to_string(),
                })
            }
        }?;

        let (result, _client, was_cached) = mcp_result;

        // EMIT: McpResponse event (with redacted response for TUI display)
        // is_error is always false here -- error cases return early with their own McpResponse
        let duration_ms = start_time.elapsed().as_millis() as u64;
        self.event_log.emit(EventKind::McpResponse {
            task_id: Arc::clone(task_id),
            call_id,
            output_len: result.to_string().len(),
            duration_ms,
            cached: was_cached,
            is_error: false,
            response: Some(Arc::new(redact_value(&result))),
        });

        // Return JSON string representation
        Ok(result.to_string())
    }
}
