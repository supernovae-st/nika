//! MCP Integration Module (v0.2)
//!
//! Provides MCP (Model Context Protocol) client capabilities for Nika workflows.
//!
//! ## Module Structure
//!
//! - [`types`]: Core MCP types (McpConfig, ToolCallRequest, ToolCallResult, etc.)
//! - [`protocol`]: JSON-RPC 2.0 types (JsonRpcRequest, JsonRpcResponse, JsonRpcError)
//! - [`transport`]: Process spawn and lifecycle management (McpTransport)
//! - [`client`]: MCP client implementation with mock support
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
//! ## Low-Level Transport Usage
//!
//! ```rust,ignore
//! use nika::mcp::{McpTransport, JsonRpcRequest};
//! use serde_json::json;
//!
//! // Spawn MCP server process
//! let transport = McpTransport::new("npx", &["-y", "@novanet/mcp-server"])
//!     .with_env("NEO4J_URI", "bolt://localhost:7687");
//! let mut child = transport.spawn().await?;
//!
//! // Create JSON-RPC request
//! let request = JsonRpcRequest::new(1, "initialize", json!({
//!     "protocolVersion": "2024-11-05",
//!     "capabilities": {}
//! }));
//! ```

pub mod client;
pub mod protocol;
pub mod transport;
pub mod types;

// Re-export core types for convenience
pub use client::McpClient;
pub use protocol::{JsonRpcError, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
pub use transport::McpTransport;
pub use types::{
    ContentBlock, McpConfig, ResourceContent, ToolCallRequest, ToolCallResult, ToolDefinition,
};
