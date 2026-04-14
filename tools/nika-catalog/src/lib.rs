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

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod data;
pub mod error;
pub mod lookup;
pub mod suggest;
pub mod types;
pub mod validate;

// ── Re-exports for ergonomic usage ──────────────────────────────────────

pub use error::CatalogError;
pub use lookup::{
    estimate_cost, find_builtin, find_embedding, find_mcp_server, find_pricing,
    find_pricing_scoped, find_provider, find_transform, is_known_builtin, is_known_mcp_server,
    is_known_transform, model_capabilities, resolve_mcp_name, validate_key_format,
};
pub use suggest::{suggest, suggest_in, Namespace, Suggestion};
pub use types::*;

// ── Iteration helpers ───────────────────────────────────────────────────

/// LLM providers (build-time generated from `data/llm-providers.toml`).
#[must_use]
pub fn all_providers() -> &'static [types::Provider] {
    data::ALL_PROVIDERS
}

/// MCP server entries (build-time generated from `data/mcp-servers.toml`).
#[must_use]
pub fn all_mcp_servers() -> &'static [types::McpServer] {
    data::ALL_MCP_SERVERS
}

/// Embedding models (build-time generated from `data/embeddings.toml`).
#[must_use]
pub fn all_embeddings() -> &'static [types::Embedding] {
    data::ALL_EMBEDDINGS
}

/// Builtin tools (alphabetically sorted, binary-search lookup).
#[must_use]
pub fn all_builtins() -> &'static [types::Builtin] {
    data::builtins::ALL_BUILTINS
}

/// Pipe transforms (alphabetically sorted, binary-search lookup).
#[must_use]
pub fn all_transforms() -> &'static [types::TransformDef] {
    data::transforms::ALL_TRANSFORMS
}

/// Model pricing entries (pattern-matched at lookup time).
#[must_use]
pub fn all_pricing() -> &'static [types::ModelPricing] {
    data::models::ALL_PRICING
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_providers_non_empty() {
        assert_eq!(all_providers().len(), 21);
    }

    #[test]
    fn all_mcp_servers_non_empty() {
        assert_eq!(all_mcp_servers().len(), 105);
    }

    #[test]
    fn all_builtins_non_empty() {
        assert_eq!(all_builtins().len(), 63);
    }

    #[test]
    fn all_transforms_non_empty() {
        assert_eq!(all_transforms().len(), 65);
    }

    #[test]
    fn all_pricing_non_empty() {
        assert_eq!(all_pricing().len(), 62);
    }
}
