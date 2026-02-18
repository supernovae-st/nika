//! MCP Integration Module (v0.2)
//!
//! Provides MCP (Model Context Protocol) client capabilities for Nika workflows.
//!
//! ## Module Structure
//!
//! - [`types`]: Core MCP protocol types (McpConfig, ToolCallRequest, ToolCallResult, etc.)
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

pub mod client;
pub mod types;

// Re-export core types for convenience
pub use client::McpClient;
pub use types::{
    ContentBlock, McpConfig, ResourceContent, ToolCallRequest, ToolCallResult, ToolDefinition,
};
