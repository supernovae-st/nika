// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Cross-reference validation for catalog integrity.
//!
//! Used by xtask and unit tests to ensure static data is consistent.

use crate::error::CatalogError;

/// Validate all catalog cross-references. Returns all errors found.
///
/// Only available under the `full` feature set — this is the xtask /
/// integrity-check entry point, not a runtime hot path.
///
/// Checks:
/// - Builtin array is sorted
/// - Transform array is sorted
/// - No duplicate names in any catalog
/// - Cloud provider default/cheap models have pricing entries
#[cfg(all(
    feature = "providers",
    feature = "mcp",
    feature = "pricing",
    feature = "builtins-transforms"
))]
#[must_use]
pub fn validate_catalog_integrity() -> Vec<CatalogError> {
    use crate::data::{builtins, transforms, ALL_MCP_SERVERS, ALL_PROVIDERS};

    let mut errors = Vec::new();
    validate_sorted("builtins", builtins::ALL_BUILTINS.iter().map(|b| b.name), &mut errors);
    validate_sorted("transforms", transforms::ALL_TRANSFORMS.iter().map(|t| t.name), &mut errors);
    validate_no_duplicates("providers", ALL_PROVIDERS.iter().map(|p| p.id), &mut errors);
    validate_no_duplicates("builtins", builtins::ALL_BUILTINS.iter().map(|b| b.name), &mut errors);
    validate_no_duplicates("transforms", transforms::ALL_TRANSFORMS.iter().map(|t| t.name), &mut errors);
    validate_no_duplicates("mcp_servers", ALL_MCP_SERVERS.iter().map(|s| s.id), &mut errors);
    validate_provider_models(&mut errors);
    errors
}

// Gated on the same feature set as the only caller
// (`validate_catalog_integrity`) so the function simply does not exist
// under narrow configs — avoids the `allow(dead_code)` footgun flagged
// by the diamond-discipline zero-dead-code rule.
#[cfg(all(
    feature = "providers",
    feature = "mcp",
    feature = "pricing",
    feature = "builtins-transforms"
))]
fn validate_sorted<'a>(
    catalog: &'static str,
    names: impl Iterator<Item = &'a str>,
    errors: &mut Vec<CatalogError>,
) {
    let names: Vec<&str> = names.collect();
    for (i, pair) in names.windows(2).enumerate() {
        if pair[0] >= pair[1] {
            errors.push(CatalogError::NotSorted {
                catalog,
                index: i + 1,
                prev: pair[0].to_string(),
                current: pair[1].to_string(),
            });
        }
    }
}

#[cfg(all(
    feature = "providers",
    feature = "mcp",
    feature = "pricing",
    feature = "builtins-transforms"
))]
fn validate_no_duplicates<'a>(
    catalog: &'static str,
    names: impl Iterator<Item = &'a str>,
    errors: &mut Vec<CatalogError>,
) {
    let mut seen = std::collections::BTreeSet::new();
    for name in names {
        if !seen.insert(name) {
            errors.push(CatalogError::DuplicateEntry {
                catalog,
                name: name.to_string(),
            });
        }
    }
}

/// Check that cloud providers' default/cheap models have pricing entries.
///
/// OpenAI-compat providers (openrouter, together, fireworks, etc.) use
/// org-prefixed model names (e.g. "anthropic/claude-sonnet-4-20250514")
/// that won't match raw pricing patterns — they are exempt.
/// Native and mock providers are also exempt (no cloud pricing).
#[cfg(all(feature = "providers", feature = "pricing"))]
#[cfg_attr(
    not(all(feature = "mcp", feature = "builtins-transforms")),
    allow(dead_code)
)]
fn validate_provider_models(errors: &mut Vec<CatalogError>) {
    use crate::data::{models, ALL_PROVIDERS};
    const CLOUD_IDS: &[&str] = &[
        "anthropic", "openai", "mistral", "groq", "deepseek", "gemini", "xai",
    ];
    for provider in ALL_PROVIDERS {
        if !CLOUD_IDS.contains(&provider.id) {
            continue;
        }
        if models::find_pricing(provider.default_model).is_none() {
            errors.push(CatalogError::MissingPricing {
                provider: provider.id,
                model: provider.default_model,
            });
        }
        if models::find_pricing(provider.cheap_model).is_none() {
            errors.push(CatalogError::MissingPricing {
                provider: provider.id,
                model: provider.cheap_model,
            });
        }
    }
}

#[cfg(test)]
#[allow(clippy::disallowed_types, clippy::panic)]
mod tests {
    use super::*;

    #[cfg(all(
        feature = "providers",
        feature = "mcp",
        feature = "pricing",
        feature = "builtins-transforms"
    ))]
    #[test]
    fn catalog_integrity_is_clean() {
        let errors = validate_catalog_integrity();
        assert!(
            errors.is_empty(),
            "catalog integrity check failed with {} errors: {errors:?}",
            errors.len()
        );
    }

    #[test]
    fn validate_sorted_detects_unsorted() {
        let mut errors = Vec::new();
        validate_sorted("test", ["b", "a"].iter().copied(), &mut errors);
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            CatalogError::NotSorted { catalog, index, prev, current } => {
                assert_eq!(*catalog, "test");
                assert_eq!(*index, 1);
                assert_eq!(prev, "b");
                assert_eq!(current, "a");
            }
            other => panic!("expected NotSorted, got {other:?}"),
        }
    }

    #[test]
    fn validate_sorted_accepts_sorted() {
        let mut errors = Vec::new();
        validate_sorted("test", ["a", "b", "c"].iter().copied(), &mut errors);
        assert!(errors.is_empty());
    }

    #[test]
    fn validate_no_duplicates_detects_dups() {
        let mut errors = Vec::new();
        validate_no_duplicates("test", ["a", "b", "a"].iter().copied(), &mut errors);
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            CatalogError::DuplicateEntry { catalog, name } => {
                assert_eq!(*catalog, "test");
                assert_eq!(name, "a");
            }
            other => panic!("expected DuplicateEntry, got {other:?}"),
        }
    }

    #[test]
    fn validate_no_duplicates_accepts_unique() {
        let mut errors = Vec::new();
        validate_no_duplicates("test", ["a", "b", "c"].iter().copied(), &mut errors);
        assert!(errors.is_empty());
    }

    #[cfg(all(feature = "providers", feature = "pricing"))]
    #[test]
    fn provider_models_have_pricing() {
        let mut errors = Vec::new();
        validate_provider_models(&mut errors);
        assert!(
            errors.is_empty(),
            "cloud provider model pricing missing: {errors:?}"
        );
    }
}
