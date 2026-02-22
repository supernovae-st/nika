//! nika-mcp — MCP client integration for Nika workflow engine
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
//! - [`validation`]: Parameter validation with schema caching
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

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod client;
pub mod protocol;
pub mod rmcp_adapter;
pub mod types;
pub mod validation;

// Re-export core types for convenience
pub use client::{CacheConfig, McpClient, ResponseCacheStats};
pub use protocol::{JsonRpcError, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
// Note: RmcpClientAdapter is pub(crate) - access MCP via McpClient
pub use types::{
    ContentBlock, McpConfig, McpErrorCode, ResourceContent, ToolCallRequest, ToolCallResult,
    ToolDefinition,
};
pub use validation::{
    CacheStats, CachedSchema, ErrorEnhancer, McpValidator, ToolSchemaCache, ValidationConfig,
    ValidationError, ValidationErrorKind, ValidationResult,
};
