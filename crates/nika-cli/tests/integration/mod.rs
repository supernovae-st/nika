//! Integration tests for Nika workflows
//!
//! This module provides shared utilities for integration tests.
//! The actual test modules are included via #[path] in tests/integration.rs.
//!
//! ## Running Tests
//!
//! ```bash
//! # Run all integration tests (excluding ignored)
//! cargo test --test integration
//!
//! # Run MCP integration tests (requires NovaNet + Neo4j)
//! cargo test --features integration -- --ignored --test-threads=1
//!
//! # Run specific integration test
//! cargo test --features integration -- --ignored test_connect_to_novanet
//! ```
//!
//! ## Requirements for MCP Integration Tests
//!
//! - NovaNet MCP server binary at `NOVANET_MCP_PATH` or default location
//! - Neo4j running at `localhost:7687`
//! - Neo4j credentials (default: neo4j/novanetpassword)
//!
//! ## Note
//!
//! This file exists for documentation and module organization.
//! The test harness `tests/integration.rs` includes modules directly via `#[path]`.
