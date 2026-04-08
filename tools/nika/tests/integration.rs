// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Integration test harness for Nika
//!
//! This test module contains end-to-end tests that verify complete workflow
//! parsing and execution behavior.
//!
//! ## Running Tests
//!
//! ```bash
//! # Run all integration tests (excluding ignored MCP tests)
//! cargo test --test integration
//!
//! # Run MCP integration tests (requires NovaNet + Neo4j)
//! cargo test --features integration -- --ignored --test-threads=1
//!
//! # Run specific integration test
//! cargo test --features integration -- --ignored test_connect_to_novanet
//! ```
//!
//! ## Test Categories
//!
//! - **invoke_workflow**: Workflow parsing tests (no external dependencies)
//! - **novanet_test**: MCP integration tests (requires NovaNet + Neo4j)

// Include helper module for integration tests
#[path = "integration/helpers.rs"]
mod helpers;

// MCP integration tests (feature-gated and ignored by default)
#[cfg(feature = "integration")]
#[path = "integration/novanet_test.rs"]
mod novanet_test;

// E2E workflow integration tests (feature-gated and ignored by default)
#[cfg(feature = "integration")]
#[path = "integration/e2e_workflow_test.rs"]
mod e2e_workflow_test;
