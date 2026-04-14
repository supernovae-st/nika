// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Static catalog data — the actual entries.
//!
//! Hybrid strategy (Decision C):
//! - phf + unicase for case-insensitive lookups (providers, MCP aliases)
//! - Sorted arrays + `binary_search` for case-sensitive lookups (builtins, transforms)
//! - Pattern-matching function for model capabilities
//! - 2-pass matching (exact + contains) for pricing

pub mod builtins;
pub mod generated;
pub mod mcp_aliases;
pub mod models;
pub mod providers;
pub mod transforms;
