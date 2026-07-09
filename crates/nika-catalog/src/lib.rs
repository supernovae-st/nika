// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-catalog` — static catalogs for the Nika diamond.
//!
//! Build-time generated from TOML source-of-truth (`data/*.toml`) via
//! `phf_codegen`. O(1) case-insensitive lookup via `phf + unicase`.
//! Sorted arrays + `binary_search` for case-sensitive catalogs (builtins, transforms).
//! Pattern-matching function for model capabilities. 2-pass for model pricing.
//!
//! All lookups return `Option`, not `NikaError`. The catalog answers
//! "is this known?", the caller decides if "unknown" is an error.
//!
//! # Cargo features
//!
//! `default = ["full", "serde"]` — every catalog + serde derives.
//!
//! Subset compilation is supported for community-extension crates:
//! `nika-catalog = { default-features = false, features = ["extension-author"] }`
//! pulls in only types + `Tag` enum, no bundled data. See
//! [`COMMUNITY_EXTENSIONS.md`][1] for the full extension-author pattern,
//! cargo-feature reference table, and reserved field names.
//!
//! [1]: https://github.com/supernovae-st/nika/blob/nika-diamond/crates/nika-catalog/COMMUNITY_EXTENSIONS.md

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

// `data` is crate-internal — external consumers reach the generated
// statics via `nika_catalog::{ALL_*, find_*}` (lib.rs re-exports below)
// or `nika_catalog::lookup::*` (single public module entry point).
// Collapsing to two public paths avoids the drift an architect review
// flagged: callers could otherwise write `nika_catalog::data::generated::*`
// and our `cargo public-api` diff would churn on every data edit.
pub(crate) mod data;
pub mod error;
// The wire projection (`nika catalog --json` · MCP `nika_catalog`) needs
// serde derives + the provider catalog + the capability rule table.
#[cfg(all(feature = "serde", feature = "providers", feature = "capabilities"))]
pub mod export;
pub mod lookup;
pub mod suggest;
pub mod types;
pub mod validate;

// ── Re-exports for ergonomic usage ──────────────────────────────────────

pub use error::CatalogError;
pub use suggest::{Namespace, Suggestion, suggest, suggest_in};
// `types::*` already brings `validate_key_format`, `Tag`, and the rest of the
// type surface into scope — it's a pure-logic function requiring no feature.
pub use types::*;

// Lookup functions are feature-gated on their respective content catalogs.
#[cfg(feature = "embeddings")]
pub use lookup::find_embedding;
#[cfg(feature = "providers")]
pub use lookup::find_provider;
#[cfg(feature = "capabilities")]
pub use lookup::model_capabilities;
#[cfg(feature = "pricing")]
pub use lookup::{
    estimate_cost, estimate_cost_for, estimate_cost_usage_for, find_pricing, find_pricing_for,
    find_pricing_scoped,
};
#[cfg(feature = "builtins-transforms")]
pub use lookup::{find_builtin, find_transform, is_known_builtin, is_known_transform};
#[cfg(feature = "mcp")]
pub use lookup::{find_mcp_server, is_known_mcp_server, resolve_mcp_name};

// ── Iteration helpers ───────────────────────────────────────────────────

/// LLM providers (build-time generated from `data/llm-providers.toml`).
#[cfg(feature = "providers")]
#[must_use]
pub fn all_providers() -> &'static [types::Provider] {
    data::ALL_PROVIDERS
}

/// MCP server entries (build-time generated from `data/mcp-servers.toml`).
#[cfg(feature = "mcp")]
#[must_use]
pub fn all_mcp_servers() -> &'static [types::McpServer] {
    data::ALL_MCP_SERVERS
}

/// Embedding models (build-time generated from `data/embeddings.toml`).
#[cfg(feature = "embeddings")]
#[must_use]
pub fn all_embeddings() -> &'static [types::Embedding] {
    data::ALL_EMBEDDINGS
}

/// Builtin tools (alphabetically sorted, binary-search lookup).
#[cfg(feature = "builtins-transforms")]
#[must_use]
pub fn all_builtins() -> &'static [types::Builtin] {
    data::builtins::ALL_BUILTINS
}

/// Pipe transforms (alphabetically sorted, binary-search lookup).
#[cfg(feature = "builtins-transforms")]
#[must_use]
pub fn all_transforms() -> &'static [types::TransformDef] {
    data::transforms::ALL_TRANSFORMS
}

/// Model pricing entries (pattern-matched at lookup time).
#[cfg(feature = "pricing")]
#[must_use]
pub fn all_pricing() -> &'static [types::ModelPricing] {
    data::ALL_PRICING
}

/// Provenance of the vendored pricing snapshot — source URL · `as_of`
/// date · upstream sha256 prefix. Counts (rules · providers) derive from
/// [`all_pricing`] at read time, never embedded (the born-stale law).
#[cfg(feature = "pricing")]
#[must_use]
pub fn pricing_snapshot() -> &'static types::PricingSnapshot {
    &data::PRICING_SNAPSHOT
}

#[cfg(all(
    test,
    feature = "providers",
    feature = "mcp",
    feature = "embeddings",
    feature = "pricing",
    feature = "builtins-transforms"
))]
mod tests {
    use super::*;

    #[test]
    fn all_providers_non_empty() {
        // Session 4b added 7 providers (nvidia-nim, deepinfra, replicate,
        // hyperbolic, writer, databricks, cloudflare): 25 → 32; 2026-07-05
        // huggingface joined (+ nvidia-nim → nvidia rename): 32 → 33;
        // 2026-07-06 the 5 local servers got catalog rows: 33 → 38.
        assert_eq!(all_providers().len(), 38);
    }

    #[test]
    fn all_mcp_servers_non_empty() {
        assert_eq!(all_mcp_servers().len(), 105);
    }

    #[test]
    fn all_builtins_non_empty() {
        // Spec 26 · the 22 Rams-swept stdlib builtins + nika:compose + the
        // agent loop's self-verification intrinsic · ADR-096 · loop-only
        // like done) + nika:image_generate (stdlib §Media · the first
        // deferred-media graduate). Cascade · ADR-088 inspect (4
        // introspection → 1) + ADR-087 wait (sleep + wait_until → 1) +
        // ADR-086 convert (csv_to_json → 1) + D-2026-05-22-N6
        // stdlib-collapse 42→26.
        assert_eq!(all_builtins().len(), 26);
    }

    #[test]
    fn all_transforms_non_empty() {
        assert_eq!(all_transforms().len(), 65);
    }

    #[test]
    fn all_pricing_non_empty() {
        // Derived, never pinned — the generated catalog moves upstream.
        assert!(!all_pricing().is_empty());
    }
}
