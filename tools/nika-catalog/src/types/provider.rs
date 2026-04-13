// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! LLM provider definition — 16 entries (7 cloud + 7 OpenAI-compat + native + mock).
//!
//! Decision B (locked): providers are LLM-only. MCP providers live in [`crate::types::McpAlias`].
//! `ProviderCategory` enum deleted — every provider IS an LLM provider.

/// A known LLM provider with metadata for secret management and model resolution.
///
/// All 16 providers are defined statically in [`crate::data::providers`].
/// Lookups are case-insensitive via phf + unicase.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Provider {
    /// Canonical identifier (e.g. `"anthropic"`). Always lowercase.
    pub id: &'static str,
    /// Human-readable name (e.g. `"Anthropic Claude"`).
    pub name: &'static str,
    /// Alternative names that resolve to this provider (e.g. `["claude"]`).
    pub aliases: &'static [&'static str],
    /// Environment variable for the API key (e.g. `"ANTHROPIC_API_KEY"`).
    pub env_var: &'static str,
    /// Expected key prefix for format validation (e.g. `Some("sk-ant-")`).
    pub key_prefix: Option<&'static str>,
    /// Default model to use when none specified (e.g. `"claude-sonnet-4-20250514"`).
    pub default_model: &'static str,
    /// Cheap/fast model for repair passes and cost-sensitive tasks.
    pub cheap_model: &'static str,
    /// Whether this provider requires an API key to function.
    pub requires_key: bool,
    /// Short description of the provider.
    pub description: &'static str,
}
