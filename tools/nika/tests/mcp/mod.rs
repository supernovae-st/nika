// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! MCP integration tests with real MCP servers.
//!
//! These tests validate MCP client functionality with actual servers.
//! Tests are organized by MCP server type.
//!
//! Run with: cargo test --test mcp_integration -- --ignored
//! Or set NIKA_MCP_TESTS=1 to run automatically

mod novanet_tests;
mod generic_mcp_tests;
