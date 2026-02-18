//! MCP Integration Module (v0.2)
//!
//! Provides MCP (Model Context Protocol) client capabilities for Nika workflows.
//!
//! ## Module Structure
//!
//! - [`types`]: Core MCP protocol types (McpConfig, ToolCallRequest, ToolCallResult, etc.)
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

pub mod types;

// Re-export core types for convenience
pub use types::{
    ContentBlock, McpConfig, ResourceContent, ToolCallRequest, ToolCallResult, ToolDefinition,
};
