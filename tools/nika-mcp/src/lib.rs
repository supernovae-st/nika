// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! MCP Client Module - Model Context Protocol integration
//!
//! Provides MCP client functionality for connecting to and communicating
//! with MCP servers. Key types:
//! - `McpClient`: Single server connection with tool calling
//! - `McpClientPool`: Connection pool for multiple servers
//! - `McpConfig`: Server configuration
//! - `ToolDefinition`: Tool schema definition

pub mod client;
pub mod error;
pub mod pool;
pub mod protocol;
pub mod retry;
pub mod rmcp_adapter;
pub mod server;
pub mod types;
pub mod validation;

// Re-export public types
pub use client::{CacheConfig, McpClient, McpPingError, McpPingResult, ResponseCacheStats};
pub use error::{McpError, Result};
pub use pool::McpClientPool;
pub use protocol::{JsonRpcError, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
pub use retry::{is_retryable_mcp_error, retry_mcp_call, McpRetryConfig};
pub use types::{
    ContentBlock, McpConfig, McpErrorCode, ResourceContent, ToolCallRequest, ToolCallResult,
    ToolDefinition,
};
pub use validation::{
    CacheStats, CachedSchema, ErrorEnhancer, McpValidator, ToolSchemaCache, ValidationConfig,
    ValidationError, ValidationErrorKind, ValidationResult,
};

/// MCP server inline configuration (command + args + env).
///
/// Defined here to avoid circular dependency with nika crate's AST types.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct McpConfigInline {
    /// Command to spawn the MCP server
    pub command: String,
    /// Arguments to pass to the command
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables for the server process
    #[serde(default)]
    pub env: rustc_hash::FxHashMap<String, String>,
    /// Working directory for the server process
    pub cwd: Option<String>,
}

// Timeout constants (moved from nika's util module)
pub(crate) const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);
pub(crate) const MCP_CALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);
pub(crate) const RECONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
