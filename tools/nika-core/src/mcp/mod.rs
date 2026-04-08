// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! MCP content types (protocol-agnostic, no runtime dependencies).
//!
//! Extracted from nika-mcp to enable direct use by nika-engine, nika-media,
//! and other crates without pulling in the full MCP client (rmcp).

mod content;

pub use content::{ContentBlock, ResourceContent};
