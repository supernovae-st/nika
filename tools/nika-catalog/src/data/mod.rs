// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

// The parent `data` module is `pub(crate)` (see lib.rs), so every `pub`
// item in here is "unreachable" via the module path on purpose — we
// expose them via re-exports from `crate::lookup::*` and `crate::*`.
// The rustc lint fires on the shape, not the actual reachability.
#![allow(unreachable_pub)]

//! Static catalog data — the actual entries.
//!
//! Hybrid strategy (Decision C):
//! - phf + unicase for case-insensitive lookups (providers, MCP aliases)
//! - Sorted arrays + `binary_search` for case-sensitive lookups (builtins, transforms)
//! - Pattern-matching function for model capabilities
//! - 2-pass matching (exact + contains) for pricing

pub mod builtins;
pub mod generated;
pub mod models;
pub mod transforms;

// In-crate re-exports for iteration helpers and the one lookup
// (`find_provider`) that runs inside `data/models.rs`. External access
// goes through `nika_catalog::{ALL_*, find_*}` (lib.rs) or the
// `nika_catalog::lookup::*` module — the `data` module is `pub(crate)`.
pub(crate) use generated::{find_provider, ALL_EMBEDDINGS, ALL_MCP_SERVERS, ALL_PROVIDERS};
