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

#[cfg(feature = "builtins-transforms")]
pub mod builtin_prices;
#[cfg(feature = "builtins-transforms")]
pub mod builtins;

pub mod generated;

#[cfg(any(feature = "pricing", feature = "capabilities"))]
pub mod models;
#[cfg(all(test, feature = "pricing"))]
mod pricing_math_tests;

#[cfg(feature = "builtins-transforms")]
pub mod transforms;

// In-crate re-exports. Gated on the narrowest consumer:
//  * `ALL_PROVIDERS` — used by validate/suggest under `providers`
//  * `find_provider` — used ONLY by data/models.rs (pricing/capabilities)
//  * `ALL_MCP_SERVERS`, `ALL_EMBEDDINGS` — used by suggest/validate
#[cfg(feature = "embeddings")]
pub(crate) use generated::ALL_EMBEDDINGS;
#[cfg(feature = "mcp")]
pub(crate) use generated::ALL_MCP_SERVERS;
#[cfg(feature = "pricing")]
pub(crate) use generated::ALL_PRICING;
#[cfg(feature = "providers")]
pub(crate) use generated::ALL_PROVIDERS;
#[cfg(feature = "pricing")]
pub(crate) use generated::PRICING_SNAPSHOT;
#[cfg(feature = "capabilities")]
pub(crate) use generated::{CAPABILITY_DEFAULTS, CAPABILITY_RULES, find_provider};
