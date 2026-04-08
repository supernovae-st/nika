// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! MCP Integration Module — re-exports from nika-mcp crate.
//!
//! See `nika-mcp` crate for full documentation.

// Re-export everything from nika-mcp
pub use nika_mcp::*;

// Re-export submodules for path compatibility
pub use nika_mcp::error;
pub use nika_mcp::types;
pub use nika_mcp::validation;
