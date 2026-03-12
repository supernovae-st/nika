//! MCP Integration Module (v0.5.1)
//!
//! Provides MCP (Model Context Protocol) client capabilities for Nika workflows.
//! Uses Anthropic's official rmcp SDK for real MCP connections.
//!
//! ## Module Structure
//!
//! - [`client`]: High-level MCP client with mock support
//! - [`rmcp_adapter`]: Thin wrapper around rmcp SDK (internal)
//! - [`types`]: Core MCP types (McpConfig, ToolCallRequest, ToolCallResult, etc.)
//! - [`protocol`]: JSON-RPC 2.0 types (utility, for testing/debugging)
//! - [`validation`]: Parameter validation with schema caching (v0.5.1)
//!
//! ## Usage
//!
//! ```yaml
//! # Workflow with MCP server configuration
//! schema: nika/workflow@0.2
//! mcp:
//!   novanet:
//!     command: "npx"
//!     args: ["-y", "@novanet/mcp-server"]
//!     env:
//!       NEO4J_URI: "bolt://localhost:7687"
//!
//! tasks:
//!   - id: generate
//!     invoke: novanet.novanet_generate
//!     params:
//!       entity: "qr-code"
//!       locale: "fr-FR"
//! ```
//!
//! ## Client Usage
//!
//! ```rust,ignore
//! use nika::mcp::{McpClient, McpConfig};
//! use serde_json::json;
//!
//! // Create and connect to MCP server
//! let config = McpConfig::new("novanet", "npx")
//!     .with_args(["-y", "@novanet/mcp-server"]);
//! let client = McpClient::new(config)?;
//! client.connect().await?;
//!
//! // Call a tool
//! let result = client.call_tool("novanet_generate", json!({
//!     "entity": "qr-code",
//!     "locale": "fr-FR"
//! })).await?;
//!
//! // For testing, use mock client
//! let mock = McpClient::mock("novanet");
//! assert!(mock.is_connected());
//! ```
//!
//! ## Architecture
//!
//! ```text
//! McpClient (public API)
//!     │
//!     ├── Mock Mode ──► Direct mock responses (testing)
//!     │
//!     └── Real Mode ──► RmcpClientAdapter
//!                           │
//!                           └── rmcp::Service<ClientHandler>
//!                                   │
//!                                   └── TokioChildProcess transport
//! ```
//!
//! ## Debug Utilities
//!
//! The `protocol` module provides low-level JSON-RPC types
//! useful for testing or debugging MCP protocol interactions.

pub mod client;
pub mod protocol;
pub mod retry;
pub mod rmcp_adapter;
pub mod spn_config;
pub mod types;
pub mod validation;

// Re-export core types for convenience
pub use client::{CacheConfig, McpClient, McpPingError, McpPingResult, ResponseCacheStats};
pub use protocol::{JsonRpcError, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
// Note: RmcpClientAdapter is pub(crate) - access MCP via McpClient
pub use retry::{is_retryable_mcp_error, retry_mcp_call, McpRetryConfig};
pub use types::{
    ContentBlock, McpConfig, McpErrorCode, ResourceContent, ToolCallRequest, ToolCallResult,
    ToolDefinition,
};
pub use validation::{
    CacheStats, CachedSchema, ErrorEnhancer, McpValidator, ToolSchemaCache, ValidationConfig,
    ValidationError, ValidationErrorKind, ValidationResult,
};
// MCP config loading
pub use spn_config::{
    list_spn_mcp_servers, load_spn_mcp_servers, load_spn_mcp_servers_by_name,
    load_spn_mcp_servers_with_manager, spn_mcp_config_exists, SpnMcpConfig, SpnMcpConfigManager,
    SpnMcpServer, SpnMcpSource,
};
