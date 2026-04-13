// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-catalog` — static catalogs for the Nika diamond.
//!
//! Hybrid lookup strategy (Decision C):
//! - phf + unicase for case-insensitive catalogs (providers, MCP aliases)
//! - Sorted arrays + `binary_search` for case-sensitive catalogs (builtins, transforms)
//! - Pattern-matching function for model capabilities
//! - 2-pass matching (exact + contains) for model pricing
//!
//! All lookups return `Option`, not `NikaError`. The catalog answers
//! "is this known?", the caller decides if "unknown" is an error.
//!
//! See `CATALOG_RESEARCH_SYNTHESIS.md` for the 8-agent research rationale.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod data;
pub mod error;
pub mod lookup;
pub mod types;
pub mod validate;

// ── Re-exports for ergonomic usage ──────────────────────────────────────

pub use error::CatalogError;
pub use lookup::*;
pub use types::*;

// ── Iteration helpers ───────────────────────────────────────────────────

/// All 16 LLM providers.
#[must_use]
pub fn all_providers() -> &'static [&'static types::Provider] {
    data::providers::ALL_PROVIDERS
}

/// All 113 MCP server aliases.
#[must_use]
pub fn all_mcp_aliases() -> &'static [types::McpAlias] {
    data::mcp_aliases::ALL_MCP_ALIASES
}

/// All 63 builtin tools.
#[must_use]
pub fn all_builtins() -> &'static [types::Builtin] {
    data::builtins::ALL_BUILTINS
}

/// All 65 pipe transforms.
#[must_use]
pub fn all_transforms() -> &'static [types::TransformDef] {
    data::transforms::ALL_TRANSFORMS
}

/// All 61 model pricing entries.
#[must_use]
pub fn all_pricing() -> &'static [types::ModelPricing] {
    data::models::ALL_PRICING
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_providers_non_empty() {
        assert_eq!(all_providers().len(), 16);
    }

    #[test]
    fn all_mcp_aliases_non_empty() {
        assert_eq!(all_mcp_aliases().len(), 113);
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
        assert_eq!(all_pricing().len(), 61);
    }
}
