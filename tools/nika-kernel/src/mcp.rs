// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! McpPool trait — abstract MCP server connection pool.
//!
//! Verb crates hold `&dyn McpPool` in `InvokeCaps` to call MCP tools
//! and read MCP resources without depending on the concrete `McpClientPool`
//! in `nika-engine`.
//!
//! Concrete implementation lives in `nika-engine::mcp::McpClientPool`.

/// Error type for MCP pool operations.
#[derive(Debug, thiserror::Error)]
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
}

/// Abstract MCP server connection pool.
///
/// Object-safe: no generics, no `Self`-returning methods.
/// Send + Sync for use across `tokio::spawn` boundaries.
pub trait McpPool: Send + Sync {
    /// Call an MCP tool on the specified server.
    ///
    /// # Arguments
    /// * `server` — MCP server name as configured in `.mcp.json`
    /// * `tool` — tool name on that server
    /// * `args` — JSON-encoded arguments
    fn call_tool<'a>(
        &'a self,
        server: &'a str,
        tool: &'a str,
        args: serde_json::Value,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<serde_json::Value, McpError>> + Send + 'a>,
    >;

    /// Read an MCP resource by URI.
    fn read_resource<'a>(
        &'a self,
        uri: &'a str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<String, McpError>> + Send + 'a>,
    >;

    /// Check whether an MCP server is configured and reachable.
    fn has_server(&self, server: &str) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn mcp_pool_is_object_safe() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Arc<dyn McpPool>>();
    }

    #[test]
    fn mcp_error_display() {
        let e = McpError::ServerNotFound {
            server: "test".into(),
        };
        assert!(e.to_string().contains("NIKA-100"));
        assert!(e.to_string().contains("test"));
    }
}
