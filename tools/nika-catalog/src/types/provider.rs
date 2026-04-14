// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! LLM provider catalog entry.
//!
//! Model information is *folded into* the provider (per the
//! `NIKA_PCK_FINAL_TYPES.md` decision): a `Model` is never first-class,
//! it lives as one entry inside a provider's `models` list. The caller
//! who wants "Claude Sonnet 4.5" grabs the provider and asks for the
//! `"sonnet"` model nickname.
//!
//! All entries are materialised by `build.rs` from `data/llm-providers.toml`
//! and embedded as `&'static [Provider]` at compile time — zero runtime
//! parse cost, O(1) case-insensitive lookup via the `PROVIDER_INDEX` phf.

/// A single model exposed by a provider, with budget metadata.
///
/// `id` is the short nickname used by workflows (`provider: anthropic,
/// model: sonnet`). `model` is the wire-level identifier sent to the
/// provider's API (`claude-sonnet-4-5-20250929`).
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProviderModel {
    /// Short nickname (e.g. `"sonnet"`, `"haiku"`, `"mini"`).
    pub id: &'static str,
    /// Wire identifier sent to the provider API.
    pub model: &'static str,
    /// Maximum context window in tokens (input + output combined).
    pub context_window_tokens: u32,
    /// Maximum tokens the model can emit in a single response.
    pub max_output_tokens: u32,
}

/// LLM provider catalog entry.
///
/// Generated into `$OUT_DIR/providers.rs` by the crate's `build.rs`
/// from `data/llm-providers.toml`. All fields are `&'static` — every
/// entry is embedded in the binary at compile time.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Provider {
    /// Canonical id (e.g. `"anthropic"`, always lowercase).
    pub id: &'static str,
    /// Human-readable name (e.g. `"Anthropic Claude"`).
    pub name: &'static str,
    /// Alternative names that resolve to this provider (e.g. `["claude"]`).
    pub aliases: &'static [&'static str],
    /// Environment variable for the API key (e.g. `"ANTHROPIC_API_KEY"`).
    pub env_var: &'static str,
    /// Accepted key prefixes for format validation. Empty = no constraint.
    /// Multi-prefix supports providers like `OpenAI` (`sk-proj-` OR `sk-`).
    pub key_prefixes: &'static [&'static str],
    /// Default model (wire identifier) to use when the caller does not specify one.
    pub default_model: &'static str,
    /// Cheap / fast model (wire identifier) for repair passes and cost-sensitive tasks.
    pub cheap_model: &'static str,
    /// Whether this provider requires an API key to function.
    pub requires_key: bool,
    /// Short description of the provider.
    pub description: &'static str,
    /// Known models exposed by this provider (nickname → wire model).
    pub models: &'static [ProviderModel],
    /// Typed tags describing capabilities, deployment, and economics.
    /// Sorted + deduplicated (enforced by build.rs assertion).
    pub tags: &'static [crate::types::Tag],
    /// Community / vendor-specific tags not in the core [`Tag`] enum.
    /// No validation — passthrough escape hatch for extension crates.
    pub extra_tags: &'static [&'static str],
}

/// Validate an API key format against a provider's declared prefixes.
///
/// Rules:
///   * empty key → `false`
///   * empty `key_prefixes` → any non-empty key is accepted
///   * otherwise, key must start with one of the declared prefixes
#[must_use]
pub fn validate_key_format(provider: &Provider, key: &str) -> bool {
    if key.is_empty() {
        return false;
    }
    if provider.key_prefixes.is_empty() {
        return true;
    }
    provider.key_prefixes.iter().any(|p| key.starts_with(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    const _: () = {
        const fn assert_copy_send_sync<T: Copy + Send + Sync>() {}
        const fn assert_send_sync<T: Send + Sync>() {}
        assert_copy_send_sync::<ProviderModel>();
        // Provider is intentionally not Copy — 10 pointer-width fields
        // (~160 bytes on 64-bit) make implicit memcpy a footgun when
        // passed by value. Always pass `&Provider`.
        assert_send_sync::<Provider>();
    };

    const SONNET_MODELS: &[ProviderModel] = &[ProviderModel {
        id: "sonnet",
        model: "claude-sonnet-4-5-20250929",
        context_window_tokens: 200_000,
        max_output_tokens: 8_192,
    }];

    fn anthropic_fixture() -> Provider {
        const PREFIXES: &[&str] = &["sk-ant-"];
        Provider {
            id: "anthropic",
            name: "Anthropic Claude",
            aliases: &["claude"],
            env_var: "ANTHROPIC_API_KEY",
            key_prefixes: PREFIXES,
            default_model: "claude-sonnet-4-5-20250929",
            cheap_model: "claude-sonnet-4-5-20250929",
            requires_key: true,
            description: "Claude models.",
            models: SONNET_MODELS,
            tags: &[],
            extra_tags: &[],
        }
    }

    #[test]
    fn construct_provider_with_models() {
        let p = anthropic_fixture();
        assert_eq!(p.models.len(), 1);
        assert_eq!(p.models[0].id, "sonnet");
    }

    #[test]
    fn validate_key_accepts_matching_single_prefix() {
        let p = anthropic_fixture();
        assert!(validate_key_format(&p, "sk-ant-abc"));
        assert!(!validate_key_format(&p, "sk-proj-abc"));
    }

    #[test]
    fn validate_key_accepts_any_of_disjunctive_prefixes() {
        const MULTI: &[&str] = &["sk-proj-", "sk-"];
        let p = Provider {
            id: "openai",
            name: "OpenAI",
            aliases: &[],
            env_var: "OPENAI_API_KEY",
            key_prefixes: MULTI,
            default_model: "gpt-4o",
            cheap_model: "gpt-4o",
            requires_key: true,
            description: "OpenAI.",
            models: &[],
            tags: &[],
            extra_tags: &[],
        };
        assert!(validate_key_format(&p, "sk-proj-abc"));
        assert!(validate_key_format(&p, "sk-abc"));
        assert!(!validate_key_format(&p, "pk-abc"));
    }

    #[test]
    fn validate_key_rejects_empty_key() {
        assert!(!validate_key_format(&anthropic_fixture(), ""));
    }

    #[test]
    fn validate_key_accepts_any_when_no_prefixes_declared() {
        let p = Provider {
            id: "mistral",
            name: "Mistral",
            aliases: &[],
            env_var: "MISTRAL_API_KEY",
            key_prefixes: &[],
            default_model: "mistral-large-latest",
            cheap_model: "mistral-large-latest",
            requires_key: true,
            description: "Mistral.",
            models: &[],
            tags: &[],
            extra_tags: &[],
        };
        assert!(validate_key_format(&p, "any-format"));
        assert!(!validate_key_format(&p, ""));
    }
}
