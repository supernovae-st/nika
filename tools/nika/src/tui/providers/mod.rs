//! Unified provider access for Nika TUI
//!
//! Single source of truth: Uses `KNOWN_PROVIDERS` from spn-core (via spn-client).
//! All provider operations should go through this module to avoid duplication.
//!
//! # Architecture
//!
//! ```text
//! spn-core::KNOWN_PROVIDERS (13 providers)
//!         ↓
//! spn-client re-exports
//!         ↓
//! nika::tui::providers (this module)
//!         ↓
//! ┌───────────────────────────────────────────┐
//! │  provider_checker.rs  │  tabs/keys.rs    │
//! │  settings.rs          │  keyring.rs      │
//! └───────────────────────────────────────────┘
//! ```

pub mod icons;
pub mod status;

// Re-export spn-client types when feature enabled
#[cfg(feature = "spn-daemon")]
pub use spn_client::{
    find_provider, mask_key, provider_to_env_var, providers_by_category, validate_key_format,
    Provider, ProviderCategory, KNOWN_PROVIDERS,
};

// Fallback types when spn-daemon feature disabled
#[cfg(not(feature = "spn-daemon"))]
mod fallback;

#[cfg(not(feature = "spn-daemon"))]
pub use fallback::*;

/// Get all LLM providers (7: anthropic, openai, mistral, groq, deepseek, gemini, ollama)
#[cfg(feature = "spn-daemon")]
pub fn llm_providers() -> impl Iterator<Item = &'static Provider> {
    providers_by_category(ProviderCategory::Llm)
        .chain(providers_by_category(ProviderCategory::Local))
}

/// Get all MCP service providers (6: neo4j, github, slack, perplexity, firecrawl, supadata)
#[cfg(feature = "spn-daemon")]
pub fn mcp_providers() -> impl Iterator<Item = &'static Provider> {
    providers_by_category(ProviderCategory::Mcp)
}

/// Get environment variable name for a provider
///
/// Delegates to spn-core when spn-daemon feature enabled.
#[cfg(feature = "spn-daemon")]
pub fn env_var(provider: &str) -> &'static str {
    provider_to_env_var(provider).unwrap_or("UNKNOWN_API_KEY")
}

/// Get all provider IDs as static strings (for iteration without spn-core types)
#[cfg(feature = "spn-daemon")]
pub fn all_provider_ids() -> impl Iterator<Item = &'static str> {
    KNOWN_PROVIDERS.iter().map(|p| p.id)
}

/// Get LLM provider IDs only
#[cfg(feature = "spn-daemon")]
pub fn llm_provider_ids() -> impl Iterator<Item = &'static str> {
    llm_providers().map(|p| p.id)
}

/// Get MCP provider IDs only
#[cfg(feature = "spn-daemon")]
pub fn mcp_provider_ids() -> impl Iterator<Item = &'static str> {
    mcp_providers().map(|p| p.id)
}

// ═══════════════════════════════════════════════════════════════════════════════
// Fallback implementations (no spn-daemon)
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(not(feature = "spn-daemon"))]
pub fn env_var(provider: &str) -> &'static str {
    fallback::provider_env_var(provider)
}

#[cfg(not(feature = "spn-daemon"))]
pub fn llm_provider_ids() -> impl Iterator<Item = &'static str> {
    fallback::LLM_PROVIDER_IDS.iter().copied()
}

#[cfg(not(feature = "spn-daemon"))]
pub fn mcp_provider_ids() -> impl Iterator<Item = &'static str> {
    fallback::MCP_PROVIDER_IDS.iter().copied()
}

#[cfg(not(feature = "spn-daemon"))]
pub fn all_provider_ids() -> impl Iterator<Item = &'static str> {
    fallback::LLM_PROVIDER_IDS
        .iter()
        .chain(fallback::MCP_PROVIDER_IDS.iter())
        .copied()
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "spn-daemon")]
    fn test_llm_providers_count() {
        let count = llm_providers().count();
        assert_eq!(
            count, 7,
            "Expected 7 LLM providers (anthropic, openai, mistral, groq, deepseek, gemini, ollama)"
        );
    }

    #[test]
    #[cfg(feature = "spn-daemon")]
    fn test_mcp_providers_count() {
        let count = mcp_providers().count();
        assert_eq!(
            count, 6,
            "Expected 6 MCP providers (neo4j, github, slack, perplexity, firecrawl, supadata)"
        );
    }

    #[test]
    fn test_llm_provider_ids() {
        let ids: Vec<_> = llm_provider_ids().collect();
        assert!(ids.contains(&"anthropic"));
        assert!(ids.contains(&"openai"));
        assert!(ids.contains(&"gemini"), "Gemini must be present");
        assert!(ids.contains(&"ollama"));
    }

    #[test]
    fn test_mcp_provider_ids() {
        let ids: Vec<_> = mcp_provider_ids().collect();
        assert!(ids.contains(&"neo4j"));
        assert!(ids.contains(&"perplexity"));
    }

    #[test]
    fn test_env_var_anthropic() {
        assert_eq!(env_var("anthropic"), "ANTHROPIC_API_KEY");
    }

    #[test]
    fn test_env_var_gemini() {
        assert_eq!(env_var("gemini"), "GEMINI_API_KEY");
    }

    #[test]
    fn test_env_var_unknown() {
        // Unknown providers return UNKNOWN_API_KEY
        assert_eq!(env_var("unknown_provider"), "UNKNOWN_API_KEY");
    }
}
