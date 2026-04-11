// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `McpPool` trait — abstract MCP server connection pool.
//!
//! Verb crates hold `&dyn McpPool` in `InvokeCaps` / `AgentCaps` to call MCP
//! tools, read MCP resources, and enumerate servers without depending on the
//! concrete `McpClientPool` in `nika-engine`.
//!
//! Concrete implementation lives in `nika-engine::runtime::mcp::adapter` as
//! `McpPoolAdapter`.
//!
//! # Design notes (S15)
//!
//! - **No EventLog in trait signature** (invariant #23 / `caps.rs` header).
//!   `nika-kernel` does not depend on `nika-event`. Adapters that need to
//!   emit MCP retry/progress events hold `Arc<EventLog>` as a field of
//!   their own, not through `McpCallOptions`.
//! - **Unified `McpToolResult` DTO** — mirrors `nika_mcp::ToolCallResult`
//!   using only `nika_core::mcp::{ContentBlock, ResourceContent}` + primitives.
//!   Verb crates never import `nika-mcp`.
//! - **50 MB cap enforced by adapters, not the constructor.** `new()` is
//!   infallible so mocks can build arbitrarily large results for tests.
//!   Adapters emit `McpError::ResultTooLarge` BEFORE constructing the DTO.
//! - **`#[non_exhaustive]` + `new()` constructors** on every struct (invariant #19).
//!   Downstream code never hits E0639.

use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use nika_core::mcp::{ContentBlock, ResourceContent};

/// Hard cap on MCP tool result size (50 MB).
///
/// Adapters reject oversized results with [`McpError::ResultTooLarge`]
/// **before** constructing an [`McpToolResult`].
pub const MAX_MCP_RESULT_SIZE: usize = 50 * 1024 * 1024;

/// Error type for MCP pool operations.
///
/// `#[non_exhaustive]` (invariant #25) — downstream `From<McpError>` impls
/// MUST include a wildcard arm that maps unmapped variants to a generic
/// error with a `format!("unmapped mcp error variant: {other:?}")` message.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum McpError {
    /// MCP server not found or not configured.
    #[error("[NIKA-100] MCP server '{server}' not found")]
    ServerNotFound { server: String },

    /// MCP tool call failed.
    #[error("[NIKA-100] MCP tool '{tool}' on server '{server}' failed: {reason}")]
    ToolCallFailed {
        server: String,
        tool: String,
        reason: String,
    },

    /// MCP resource read failed.
    #[error("[NIKA-100] MCP resource '{uri}' failed: {reason}")]
    ResourceFailed { uri: String, reason: String },

    /// Connection or transport error.
    #[error("[NIKA-101] MCP connection error: {reason}")]
    Connection { reason: String },

    /// Tool result exceeds the 50 MB size cap (S15-A0).
    ///
    /// Emitted by adapters when `tool_result.content_size_bytes() > MAX_MCP_RESULT_SIZE`.
    /// `bytes` is the measured size, `limit` is [`MAX_MCP_RESULT_SIZE`].
    #[error("[NIKA-100] MCP tool result {bytes} bytes exceeds {limit} byte limit")]
    ResultTooLarge { bytes: usize, limit: usize },

    /// Operation cancelled via [`McpCallOptions::cancel`] (S15-A0).
    #[error("[NIKA-100] MCP operation cancelled: server='{server}' tool='{tool}'")]
    Cancelled { server: String, tool: String },
}

// ─────────────────────────────────────────────────────────────────────────
//  McpToolResult — kernel DTO for `call_tool` results
// ─────────────────────────────────────────────────────────────────────────

/// Result of an MCP tool call, as seen by verb crates.
///
/// Mirrors `nika_mcp::ToolCallResult` but uses only L0 types
/// (`nika_core::mcp::ContentBlock`) + primitives. Verb crates never import
/// `nika-mcp` directly.
///
/// **Invariant**: any `McpToolResult` returned by an [`McpPool`] impl
/// satisfies `content_size_bytes <= MAX_MCP_RESULT_SIZE`. Adapters enforce
/// this with an explicit size gate BEFORE calling [`McpToolResult::new`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct McpToolResult {
    /// Content blocks returned by the tool.
    pub content: Vec<ContentBlock>,
    /// Whether the tool signalled a semantic error (protocol-level, not
    /// transport-level). The tool ran, but returned an error payload.
    pub is_error: bool,
    /// Whether this result was served from the MCP response cache.
    pub was_cached: bool,
    /// Precomputed size in bytes across all content blocks.
    pub content_size_bytes: usize,
}

impl McpToolResult {
    /// Construct a new tool result. `content_size_bytes` is computed from
    /// the blocks using the same algorithm as
    /// `nika_mcp::ToolCallResult::content_size_bytes` for byte-identical
    /// migration semantics.
    ///
    /// **Infallible** (invariant #19): the 50 MB size cap lives in adapters,
    /// not here. Mocks can build arbitrarily large results.
    pub fn new(content: Vec<ContentBlock>, is_error: bool, was_cached: bool) -> Self {
        let content_size_bytes = content.iter().map(byte_size_of_block).sum();
        Self {
            content,
            is_error,
            was_cached,
            content_size_bytes,
        }
    }

    /// Check if any block is non-text (image/audio/resource/resource_link).
    pub fn has_media(&self) -> bool {
        self.content.iter().any(|b| !b.is_text())
    }

    /// Iterator over non-text content blocks.
    pub fn media_blocks(&self) -> impl Iterator<Item = &ContentBlock> {
        self.content.iter().filter(|b| !b.is_text())
    }

    /// Concatenate all `Text` blocks into a single string, separated by
    /// newlines. Mirrors `nika_mcp::ToolCallResult::text`.
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                ContentBlock::Image { .. }
                | ContentBlock::Audio { .. }
                | ContentBlock::Resource(_)
                | ContentBlock::ResourceLink { .. } => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Compute the byte size of a single content block. Matches
/// `nika_mcp::ToolCallResult::content_size_bytes` arm semantics exactly.
fn byte_size_of_block(block: &ContentBlock) -> usize {
    match block {
        ContentBlock::Text { text } => text.len(),
        ContentBlock::Image { data, .. } => data.len(),
        ContentBlock::Audio { data, .. } => data.len(),
        ContentBlock::Resource(rc) => {
            rc.text.as_ref().map(|t| t.len()).unwrap_or(0)
                + rc.blob.as_ref().map(|b| b.len()).unwrap_or(0)
        }
        ContentBlock::ResourceLink { .. } => 128, // small metadata estimate
    }
}

// ─────────────────────────────────────────────────────────────────────────
//  McpResourceContent — kernel alias for nika-core's ResourceContent
// ─────────────────────────────────────────────────────────────────────────

/// Resource content returned by [`McpPool::read_resource`].
///
/// Alias of `nika_core::mcp::ResourceContent`. The full
/// `{ uri, mime_type, text, blob }` shape is preserved — the adapter does
/// not drop the blob field (unlike the pre-S15 trait which returned `String`).
pub type McpResourceContent = ResourceContent;

// ─────────────────────────────────────────────────────────────────────────
//  McpToolDescriptor — kernel DTO for `list_tools`
// ─────────────────────────────────────────────────────────────────────────

/// Tool descriptor returned by [`McpPool::list_tools`].
///
/// JSON Schema validation belongs to the consumer, not the kernel —
/// `input_schema` is just `serde_json::Value`.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct McpToolDescriptor {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Option<serde_json::Value>,
}

impl McpToolDescriptor {
    /// Construct a new descriptor with only `name`. Use `with_description`
    /// / `with_input_schema` to fill optional fields.
    pub fn new(name: String) -> Self {
        Self {
            name,
            description: None,
            input_schema: None,
        }
    }

    /// Builder: set the human-readable description.
    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    /// Builder: set the JSON Schema for the tool's input parameters.
    pub fn with_input_schema(mut self, schema: serde_json::Value) -> Self {
        self.input_schema = Some(schema);
        self
    }
}

// ─────────────────────────────────────────────────────────────────────────
//  McpCallOptions — aggregate options for `call_tool`
// ─────────────────────────────────────────────────────────────────────────

/// Aggregate options for [`McpPool::call_tool`]. Future-proofs the trait
/// surface — adding fields here does NOT break impls.
///
/// **Lifetime note**: fields are OWNED (`Arc<str>`, `CancellationToken`),
/// never borrowed. Futures derived from `call_tool` can therefore be
/// `tokio::spawn`'d across a `'static` boundary without lifetime gymnastics.
///
/// **No events field** (invariant #23): `nika-kernel` does not depend on
/// `nika-event`. Adapters hold `Arc<EventLog>` as a field of their own.
/// If a future session needs to pipe an event sink through the call, add
/// it here via `with_events(…)` — the `#[non_exhaustive]` attribute
/// reserves the slot.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct McpCallOptions {
    pub task_id: Arc<str>,
    pub cancel: CancellationToken,
}

impl McpCallOptions {
    /// Construct from task_id + cancel token (invariant #19 constructor).
    pub fn new(task_id: Arc<str>, cancel: CancellationToken) -> Self {
        Self { task_id, cancel }
    }
}

// ─────────────────────────────────────────────────────────────────────────
//  McpPool trait
// ─────────────────────────────────────────────────────────────────────────

/// Abstract MCP server connection pool.
///
/// Object-safe: no generics, no `Self`-returning methods. Verb crates
/// consume this via `&dyn McpPool` in `InvokeCaps` / `AgentCaps`.
///
/// # Cancellation semantics
///
/// `call_tool` races `opts.cancel.cancelled()` against the inner MCP round
/// trip. When cancel fires, the adapter drops the in-flight future —
/// cancel latency is `≤ current in-flight rmcp round-trip OR current retry
/// backoff sleep`, not instantaneous (deferred to S16: a dedicated
/// cancellable retry helper).
#[async_trait]
pub trait McpPool: Send + Sync {
    /// Call an MCP tool on the specified server.
    ///
    /// # Arguments
    /// - `server` — MCP server name (as configured in `.mcp.json`)
    /// - `tool` — tool name on that server
    /// - `args` — JSON-encoded arguments (already template-resolved)
    /// - `opts` — task_id + cancel token
    ///
    /// # Errors
    /// - [`McpError::ServerNotFound`] if the server is not configured
    /// - [`McpError::ToolCallFailed`] on transport/protocol failure
    /// - [`McpError::ResultTooLarge`] if the tool result exceeds
    ///   [`MAX_MCP_RESULT_SIZE`] — the adapter enforces this BEFORE
    ///   constructing the returned `McpToolResult`
    /// - [`McpError::Cancelled`] if `opts.cancel` fires before completion
    /// - [`McpError::Connection`] on socket/rmcp errors
    ///
    /// # Semantic vs transport errors
    ///
    /// A successful `Ok(McpToolResult { is_error: true, .. })` means the
    /// tool ran and returned an error payload — caller decides whether to
    /// propagate. `Err(McpError)` means transport or adapter-level failure.
    async fn call_tool(
        &self,
        server: &str,
        tool: &str,
        args: serde_json::Value,
        opts: McpCallOptions,
    ) -> Result<McpToolResult, McpError>;

    /// Read an MCP resource by URI.
    ///
    /// Returns the full `McpResourceContent` shape including the `blob`
    /// field — the pre-S15 trait returned `String` and silently dropped
    /// blob data.
    async fn read_resource(
        &self,
        server: &str,
        uri: &str,
        cancel: &CancellationToken,
    ) -> Result<McpResourceContent, McpError>;

    /// Enumerate available tools on the given server.
    async fn list_tools(&self, server: &str) -> Result<Vec<McpToolDescriptor>, McpError>;

    /// Check whether an MCP server is configured and reachable.
    fn has_server(&self, server: &str) -> bool;
}

// Compile-time object safety: if this ever breaks, the trait accidentally
// grew a generic method, a `Self`-returning method, or a non-dispatchable
// parameter.
const _: fn() = || {
    fn _assert_object_safe(_: &dyn McpPool) {}
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_pool_is_send_sync_object() {
        fn assert_send_sync<T: Send + Sync + ?Sized>() {}
        assert_send_sync::<dyn McpPool>();
        assert_send_sync::<Arc<dyn McpPool>>();
    }

    #[test]
    fn mcp_error_display() {
        let e = McpError::ServerNotFound {
            server: "test".into(),
        };
        assert!(e.to_string().contains("NIKA-100"));
        assert!(e.to_string().contains("test"));

        let e = McpError::ResultTooLarge {
            bytes: 60_000_000,
            limit: MAX_MCP_RESULT_SIZE,
        };
        let s = e.to_string();
        assert!(s.contains("60000000"));
        assert!(s.contains(&MAX_MCP_RESULT_SIZE.to_string()));
    }

    #[test]
    fn mcp_tool_result_new_computes_size() {
        let blocks = vec![
            ContentBlock::text("hello"),
            ContentBlock::text("world"),
        ];
        let result = McpToolResult::new(blocks, false, false);
        // "hello" + "world" = 10 bytes
        assert_eq!(result.content_size_bytes, 10);
        assert!(!result.is_error);
        assert!(!result.was_cached);
        assert!(!result.has_media());
    }

    #[test]
    fn mcp_tool_result_text_joins_with_newlines() {
        let blocks = vec![
            ContentBlock::text("line1"),
            ContentBlock::text("line2"),
        ];
        let result = McpToolResult::new(blocks, false, false);
        assert_eq!(result.text(), "line1\nline2");
    }

    #[test]
    fn mcp_tool_result_media_detection() {
        let blocks = vec![
            ContentBlock::text("prose"),
            ContentBlock::image("base64data", "image/png"),
        ];
        let result = McpToolResult::new(blocks, false, false);
        assert!(result.has_media());
        assert_eq!(result.media_blocks().count(), 1);
        // size = "prose".len() (5) + "base64data".len() (10) = 15
        assert_eq!(result.content_size_bytes, 15);
    }

    #[test]
    fn mcp_tool_descriptor_builder() {
        let d = McpToolDescriptor::new("search".to_string())
            .with_description("Search the web".to_string())
            .with_input_schema(serde_json::json!({"type": "object"}));
        assert_eq!(d.name, "search");
        assert_eq!(d.description.as_deref(), Some("Search the web"));
        assert!(d.input_schema.is_some());
    }

    #[test]
    fn mcp_call_options_new() {
        let opts = McpCallOptions::new(
            Arc::from("task-1"),
            CancellationToken::new(),
        );
        assert_eq!(&*opts.task_id, "task-1");
        assert!(!opts.cancel.is_cancelled());
    }

    // Verify that the `ResultTooLarge` variant carries both numbers so the
    // mapping layer in the engine can format a useful error message.
    #[test]
    fn result_too_large_carries_bytes_and_limit() {
        let err = McpError::ResultTooLarge {
            bytes: 52_428_801,
            limit: MAX_MCP_RESULT_SIZE,
        };
        if let McpError::ResultTooLarge { bytes, limit } = err {
            assert!(bytes > limit);
        } else {
            panic!("wrong variant");
        }
    }
}
