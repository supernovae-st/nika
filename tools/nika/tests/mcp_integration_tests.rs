//! MCP integration test harness for Nika
//!
//! Tests MCP server interactions including NovaNet.
//!
//! ## Running Tests
//!
//! ```bash
//! # Start NovaNet MCP server first
//! cd novanet && pnpm mcp:start
//!
//! # Run MCP tests (ignored by default)
//! cargo test --test mcp_integration_tests -- --ignored
//! ```

#[path = "mcp/novanet_tests.rs"]
mod novanet_tests;

#[path = "mcp/generic_mcp_tests.rs"]
mod generic_mcp_tests;
