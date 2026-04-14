// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! LLM provider catalog entry (v3 architecture).
//!
//! Target type for the TOML-driven `build.rs` codegen in Phase C step 5.
//! Coexists with the legacy [`super::Provider`] during the migration;
//! Phase C step 6 flips all consumers over and retires the legacy type.
//!
//! Model information is *folded into* the provider (per the
//! `NIKA_PCK_FINAL_TYPES.md` decision): a `Model` is never first-class,
//! it lives as one entry inside a provider's `models` list. The reader
//! who wants "Claude Sonnet 4.5" grabs the provider and asks for the
//! `"sonnet"` model nickname.

/// A single model exposed by a provider, with budget metadata.
///
/// `id` is the short nickname used by workflows (`provider: anthropic,
/// model: sonnet`). `model` is the wire-level identifier sent to the
/// provider's API (`claude-sonnet-4-5-20250929`).
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

/// Rich LLM provider catalog entry.
///
/// Generated into `$OUT_DIR/providers.rs` by the crate's `build.rs`
/// from `data/llm-providers.toml`. All fields are `&'static` — every
/// entry is embedded in the binary at compile time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProviderDef {
    /// Canonical id (e.g. `"anthropic"`, always lowercase).
    pub id: &'static str,
    /// Human-readable name (e.g. `"Anthropic Claude"`).
    pub name: &'static str,
    /// Alternative names that resolve to this provider (e.g. `["claude"]`).
    pub aliases: &'static [&'static str],
    /// Environment variable for the API key (e.g. `"ANTHROPIC_API_KEY"`).
    pub env_var: &'static str,
    /// Expected key prefix for format validation (e.g. `Some("sk-ant-")`).
    pub key_prefix: Option<&'static str>,
    /// Default model nickname to use when the caller does not specify one.
    pub default_model: &'static str,
    /// Cheap / fast model nickname for repair passes and cost-sensitive tasks.
    pub cheap_model: &'static str,
    /// Whether this provider requires an API key to function.
    pub requires_key: bool,
    /// Short description of the provider.
    pub description: &'static str,
    /// Known models exposed by this provider (nickname → wire model).
    pub models: &'static [ProviderModel],
}

#[cfg(test)]
mod tests {
    use super::*;

    const _: () = {
        const fn assert_copy_send_sync<T: Copy + Send + Sync>() {}
        assert_copy_send_sync::<ProviderModel>();
        assert_copy_send_sync::<ProviderDef>();
    };

    #[test]
    fn construct_provider_with_models() {
        const MODELS: &[ProviderModel] = &[ProviderModel {
            id: "sonnet",
            model: "claude-sonnet-4-5-20250929",
            context_window_tokens: 200_000,
            max_output_tokens: 8_192,
        }];
        let p = ProviderDef {
            id: "anthropic",
            name: "Anthropic Claude",
            aliases: &["claude"],
            env_var: "ANTHROPIC_API_KEY",
            key_prefix: Some("sk-ant-"),
            default_model: "sonnet",
            cheap_model: "haiku",
            requires_key: true,
            description: "Claude models.",
            models: MODELS,
        };
        assert_eq!(p.models.len(), 1);
        assert_eq!(p.models[0].id, "sonnet");
    }
}
