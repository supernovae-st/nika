// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Shared credential metadata for providers and MCP servers.
//!
//! Several services appear in both the Provider catalog (LLM API) and the
//! MCP server catalog (tool endpoint) — Perplexity, Stripe, `OpenAI`, etc.
//! `Credential` is the shared pointer to where the API key lives, so both
//! catalogs can reference the same entry without duplicating metadata.
//!
//! Activation in v0.90.0+ — current `Provider` / `McpAlias` types still
//! carry inline `env_var` / `key_prefix`. Phase C step 6 (migrate statics)
//! may rewire them; for now this type is available for build.rs codegen
//! and future catalog consumers.

/// Pointer to where a service's API key is stored and how to validate it.
///
/// Intentionally narrow — the goal is de-duplication across Provider and
/// `McpServer`. Richer per-context metadata (required, secret, description)
/// stays on [`super::EnvVarSpec`] at the call site.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Credential {
    /// Environment variable name holding the API key.
    pub env_var: &'static str,
    /// Expected key prefix for format validation (e.g. `Some("sk-ant-")`).
    pub key_prefix: Option<&'static str>,
    /// URL to the provider's documentation for obtaining a key.
    pub docs_url: Option<&'static str>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const _: () = {
        const fn assert_copy_send_sync<T: Copy + Send + Sync>() {}
        assert_copy_send_sync::<Credential>();
    };

    #[test]
    fn credential_construction() {
        let c = Credential {
            env_var: "ANTHROPIC_API_KEY",
            key_prefix: Some("sk-ant-"),
            docs_url: Some("https://docs.anthropic.com/en/api/getting-started"),
        };
        assert_eq!(c.env_var, "ANTHROPIC_API_KEY");
        assert!(c.key_prefix.is_some());
        assert!(c.docs_url.is_some());
    }

    #[test]
    fn credential_without_docs() {
        let c = Credential {
            env_var: "MCP_TOKEN",
            key_prefix: None,
            docs_url: None,
        };
        assert!(c.key_prefix.is_none());
        assert!(c.docs_url.is_none());
    }
}
