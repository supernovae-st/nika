// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `McpPoolAdapter` — bridges the engine's concrete `McpClientPool` to the
//! `nika_kernel::mcp::McpPool` trait expected by verb crates.
//!
//! # Responsibility split (S15)
//!
//! The adapter owns **transport + size cap + DTO translation**. It does
//! NOT own:
//!
//! - Media processing — the engine bridge at `runtime/executor/invoke.rs`
//!   continues to iterate `McpToolResult.content` blocks and push them
//!   through `MediaProcessor`, because the media pipeline is coupled to
//!   `CasStore`, `RunDatastore.media_budget()`, and the `set_media`
//!   side-channel.
//! - Outer timeout / workflow-cancel selection — the engine bridge wraps
//!   adapter calls in its own `tokio::select!{timeout, cancel_token}`
//!   frame, the same pattern S14-ε (verb-exec pre-spawn cancel) established.
//! - `McpInvoke` / `McpResponse` boundary events — the engine bridge
//!   emits these directly from `TaskExecutor::run_invoke` so
//!   redact_value + start_time context stay local.
//!
//! The adapter DOES:
//! - Hold `Arc<EventLog>` as a struct field (not through `McpCallOptions`
//!   — invariant #23: nika-kernel types stay nika-event-free).
//! - Thread the event log into `McpClient::call_tool_with_retry_events`
//!   so `McpRetry` events correlate via task_id.
//! - Race `opts.cancel.cancelled()` against the inner call for
//!   fine-grained cancel responsiveness (faster than drop-only).
//! - Enforce the 50 MB `MAX_MCP_RESULT_SIZE` cap BEFORE constructing the
//!   `McpToolResult` DTO.
//! - Translate `nika_mcp::{McpError, ToolCallResult, ResourceContent,
//!   ToolDefinition}` into the kernel DTOs.
//!
//! # Invariants in scope
//!
//! - #1 / #16 — never hold `parking_lot::RwLockReadGuard` across `.await`.
//!   The pool's `configs: RwLock<FxHashMap<_, _>>` is only touched via
//!   `get_or_connect`, which drops the guard before awaiting connect.
//! - #23 — kernel types only use std/primitive/core types; adapter maps
//!   to concrete `reqwest`-free DTOs on return.
//! - #24 — event emission is the engine bridge's concern, not the adapter.
//!
//! # Cancel latency caveat
//!
//! `call_tool` races cancel against the inner future. If cancel fires
//! mid-`call_tool_with_retry_events`, the rmcp transport is dropped but
//! server-side tool execution may finish and its result is discarded.
//! Latency bound: `≤ current in-flight rmcp round trip OR current retry
//! backoff sleep`. S16 may add a cancellable retry helper for tighter
//! bounds.

use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use nika_event::EventLog;
use nika_kernel::mcp::{
    McpCallOptions, McpError, McpPool, McpResourceContent, McpToolDescriptor, McpToolResult,
    MAX_MCP_RESULT_SIZE,
};
use nika_mcp::error::McpError as NikaMcpError;
use nika_mcp::pool::McpClientPool;

/// Concrete `McpPool` impl over `nika_mcp::McpClientPool`.
#[derive(Clone)]
pub struct McpPoolAdapter {
    pool: Arc<McpClientPool>,
    event_log: Arc<EventLog>,
}

impl std::fmt::Debug for McpPoolAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpPoolAdapter")
            .finish_non_exhaustive()
    }
}

impl McpPoolAdapter {
    /// Construct an adapter wrapping a shared `McpClientPool` and the
    /// `EventLog` used for `McpRetry` emission.
    pub fn new(pool: Arc<McpClientPool>, event_log: Arc<EventLog>) -> Self {
        Self { pool, event_log }
    }

    /// Returns the wrapped `McpClientPool`. Used by the engine bridge
    /// during S15 migration for code paths not yet routed through the
    /// adapter (media pipeline, etc.).
    pub fn inner(&self) -> &Arc<McpClientPool> {
        &self.pool
    }
}

/// Translate `nika_mcp::McpError` → `nika_kernel::mcp::McpError`.
///
/// Maps to the closest kernel-side variant while preserving the original
/// context in the `reason` / `server` / `tool` fields. The kernel's
/// `McpError` is `#[non_exhaustive]` so unmapped source variants fall
/// through the wildcard arm to `Connection` with a format-string carrying
/// the raw debug output (invariant #25).
fn map_mcp_error(err: NikaMcpError, server: &str, tool_hint: Option<&str>) -> McpError {
    match err {
        NikaMcpError::McpNotConnected { name } => McpError::ServerNotFound { server: name },
        NikaMcpError::McpNotConfigured { name } => McpError::ServerNotFound { server: name },
        NikaMcpError::McpStartError { name, reason } => McpError::Connection {
            reason: format!("start error on '{name}': {reason}"),
        },
        NikaMcpError::McpToolError {
            tool,
            reason,
            error_code,
        } => McpError::ToolCallFailed {
            server: server.to_string(),
            tool,
            reason: match error_code {
                Some(code) => format!("{reason} (code {code})"),
                None => reason,
            },
        },
        NikaMcpError::McpToolCallFailed { tool, reason } => McpError::ToolCallFailed {
            server: server.to_string(),
            tool,
            reason,
        },
        NikaMcpError::McpResourceNotFound { uri } => McpError::ResourceFailed {
            uri,
            reason: "resource not found".to_string(),
        },
        NikaMcpError::McpProtocolError { reason } => McpError::Connection { reason },
        NikaMcpError::McpInvalidResponse { tool, reason } => McpError::ToolCallFailed {
            server: server.to_string(),
            tool,
            reason: format!("invalid response: {reason}"),
        },
        NikaMcpError::McpValidationFailed {
            tool,
            details,
            ..
        } => McpError::ToolCallFailed {
            server: server.to_string(),
            tool,
            reason: format!("validation failed: {details}"),
        },
        NikaMcpError::McpSchemaError { tool, reason } => McpError::ToolCallFailed {
            server: server.to_string(),
            tool,
            reason: format!("schema error: {reason}"),
        },
        NikaMcpError::McpTimeout {
            name,
            operation,
            timeout_secs,
        } => McpError::Connection {
            reason: format!("timeout on '{name}' during {operation} after {timeout_secs}s"),
        },
        other => McpError::Connection {
            reason: format!(
                "unmapped nika_mcp error variant (server='{server}', tool={tool_hint:?}): {other:?}"
            ),
        },
    }
}

/// Translate `nika_mcp::types::ToolCallResult` → kernel DTO.
///
/// Field-by-field move; size is recomputed by `McpToolResult::new` using
/// the same algorithm as the source `content_size_bytes` method.
fn to_kernel_tool_result(src: nika_mcp::types::ToolCallResult) -> McpToolResult {
    McpToolResult::new(src.content, src.is_error, src.was_cached)
}

/// Translate `nika_mcp::types::ToolDefinition` → kernel descriptor.
fn to_kernel_descriptor(src: nika_mcp::types::ToolDefinition) -> McpToolDescriptor {
    let mut d = McpToolDescriptor::new(src.name);
    if let Some(desc) = src.description {
        d = d.with_description(desc);
    }
    if let Some(schema) = src.input_schema {
        d = d.with_input_schema(schema);
    }
    d
}

#[async_trait]
impl McpPool for McpPoolAdapter {
    async fn call_tool(
        &self,
        server: &str,
        tool: &str,
        args: serde_json::Value,
        opts: McpCallOptions,
    ) -> Result<McpToolResult, McpError> {
        // 1. Get or connect the client. `get_or_connect` drops its internal
        //    RwLockReadGuard before awaiting, so the future stays Send
        //    (invariants #1 / #16).
        let client = self
            .pool
            .get_or_connect(server)
            .await
            .map_err(|e| map_mcp_error(e, server, Some(tool)))?;

        // 2. Race cancel vs the inner call. `biased;` polls cancel first
        //    for tighter latency (rust-async-expert Phase 1 recommendation).
        let event_log: &EventLog = &self.event_log;
        let result = tokio::select! {
            biased;
            _ = opts.cancel.cancelled() => {
                return Err(McpError::Cancelled {
                    server: server.to_string(),
                    tool: tool.to_string(),
                });
            }
            r = client.call_tool_with_retry_events(tool, args, &opts.task_id, event_log) => r,
        };

        let tool_result = result.map_err(|e| map_mcp_error(e, server, Some(tool)))?;

        // 3. Enforce the 50 MB size cap BEFORE constructing the DTO.
        //    Adapter owns transport policy; kernel DTO stays infallible.
        let bytes = tool_result.content_size_bytes();
        if bytes > MAX_MCP_RESULT_SIZE {
            return Err(McpError::ResultTooLarge {
                bytes,
                limit: MAX_MCP_RESULT_SIZE,
            });
        }

        // 4. Translate to the kernel DTO.
        Ok(to_kernel_tool_result(tool_result))
    }

    async fn read_resource(
        &self,
        server: &str,
        uri: &str,
        cancel: &CancellationToken,
    ) -> Result<McpResourceContent, McpError> {
        let client = self
            .pool
            .get_or_connect(server)
            .await
            .map_err(|e| map_mcp_error(e, server, None))?;

        let result = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                return Err(McpError::Cancelled {
                    server: server.to_string(),
                    tool: format!("resource:{uri}"),
                });
            }
            r = client.read_resource(uri) => r,
        };

        let content = result.map_err(|e| map_mcp_error(e, server, None))?;

        // 50 MB cap on resource reads too (mirrors invoke.rs enforcement).
        let size = content.text.as_ref().map(|t| t.len()).unwrap_or(0)
            + content.blob.as_ref().map(|b| b.len()).unwrap_or(0);
        if size > MAX_MCP_RESULT_SIZE {
            return Err(McpError::ResultTooLarge {
                bytes: size,
                limit: MAX_MCP_RESULT_SIZE,
            });
        }

        Ok(content)
    }

    async fn list_tools(&self, server: &str) -> Result<Vec<McpToolDescriptor>, McpError> {
        let client = self
            .pool
            .get_or_connect(server)
            .await
            .map_err(|e| map_mcp_error(e, server, None))?;

        let defs = client
            .list_tools()
            .await
            .map_err(|e| map_mcp_error(e, server, None))?;

        Ok(defs.into_iter().map(to_kernel_descriptor).collect())
    }

    fn has_server(&self, server: &str) -> bool {
        self.pool.has_config(server)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Compile-time dyn compat assertion.
    const _: fn() = || {
        fn _assert_dyn(_: &dyn McpPool) {}
    };

    #[test]
    fn adapter_is_send_sync() {
        fn assert_send_sync<T: Send + Sync + ?Sized>() {}
        assert_send_sync::<McpPoolAdapter>();
        assert_send_sync::<dyn McpPool>();
    }

    #[test]
    fn map_mcp_error_preserves_tool_name() {
        let src = NikaMcpError::McpToolError {
            tool: "search".to_string(),
            reason: "boom".to_string(),
            error_code: Some(nika_mcp::types::McpErrorCode::ServerError(-32000)),
        };
        let mapped = map_mcp_error(src, "srv", Some("search"));
        match mapped {
            McpError::ToolCallFailed { server, tool, reason } => {
                assert_eq!(server, "srv");
                assert_eq!(tool, "search");
                assert!(reason.contains("boom"));
                // Error code is appended via `(code {code})` suffix
                assert!(reason.contains("code"));
            }
            other => panic!("expected ToolCallFailed, got {other:?}"),
        }
    }

    #[test]
    fn map_mcp_error_not_connected_to_server_not_found() {
        let src = NikaMcpError::McpNotConnected {
            name: "novanet".to_string(),
        };
        let mapped = map_mcp_error(src, "novanet", None);
        assert!(matches!(mapped, McpError::ServerNotFound { server } if server == "novanet"));
    }

    #[test]
    fn map_mcp_error_start_error_to_connection() {
        let src = NikaMcpError::McpStartError {
            name: "srv".to_string(),
            reason: "spawn failed".to_string(),
        };
        let mapped = map_mcp_error(src, "srv", None);
        match mapped {
            McpError::Connection { reason } => {
                assert!(reason.contains("spawn failed"));
                assert!(reason.contains("srv"));
            }
            other => panic!("expected Connection, got {other:?}"),
        }
    }

    #[test]
    fn to_kernel_tool_result_preserves_fields() {
        use nika_core::mcp::ContentBlock;
        let src = nika_mcp::types::ToolCallResult {
            content: vec![ContentBlock::text("hello")],
            is_error: true,
            was_cached: true,
        };
        let dst = to_kernel_tool_result(src);
        assert_eq!(dst.text(), "hello");
        assert!(dst.is_error);
        assert!(dst.was_cached);
        assert_eq!(dst.content_size_bytes, 5);
    }

    #[test]
    fn to_kernel_descriptor_preserves_schema_and_description() {
        let src = nika_mcp::types::ToolDefinition::new("search")
            .with_description("Search the web")
            .with_input_schema(serde_json::json!({"type": "object"}));
        let dst = to_kernel_descriptor(src);
        assert_eq!(dst.name, "search");
        assert_eq!(dst.description.as_deref(), Some("Search the web"));
        assert!(dst.input_schema.is_some());
    }
}
