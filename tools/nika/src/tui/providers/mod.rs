//! Unified provider access for Nika TUI
//!
//! Single source of truth: Uses `KNOWN_PROVIDERS` from nika::core (v0.27 spn fusion).
//! All provider operations should go through this module to avoid duplication.
//!
//! v0.27: Ollama removed
//!
//! # Architecture
//!
//! ```text
//! nika::core::KNOWN_PROVIDERS (18 providers: 6 LLM + 11 MCP + 1 Local)
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

// Re-export nika::core types (v0.27: migrated from spn-client)
pub use crate::core::{
    find_provider, provider_to_env_var, providers_by_category, validate_key_format, Provider,
    ProviderCategory, KNOWN_PROVIDERS,
};

// mask_key moved to local implementation
pub use self::mask::mask_key;

mod mask {
    /// Mask an API key for display (shows first 8 chars + ...)
    pub fn mask_key(key: &str) -> String {
        if key.len() > 8 {
            format!("{}...", &key[..8])
        } else {
            "***".to_string()
        }
    }
}

// Fallback types when spn-daemon feature disabled (still needed for SpnKeyring)
#[cfg(not(feature = "spn-daemon"))]
mod fallback;

/// Get all LLM providers (6: anthropic, openai, mistral, groq, deepseek, gemini)
/// v0.27: Ollama removed
pub fn llm_providers() -> Vec<&'static Provider> {
    providers_by_category(ProviderCategory::Llm)
}

/// Get all Local providers (1: native)
/// v0.27: Ollama removed
pub fn local_providers() -> Vec<&'static Provider> {
    providers_by_category(ProviderCategory::Local)
}

/// Get all MCP service providers (11: neo4j, github, slack, perplexity, firecrawl, supadata, ...)
pub fn mcp_providers() -> Vec<&'static Provider> {
    providers_by_category(ProviderCategory::Mcp)
}

/// Get environment variable name for a provider
pub fn env_var(provider: &str) -> &'static str {
    provider_to_env_var(provider).unwrap_or("UNKNOWN_API_KEY")
}

/// Get all provider IDs as static strings (for iteration)
pub fn all_provider_ids() -> impl Iterator<Item = &'static str> {
    KNOWN_PROVIDERS.iter().map(|p| p.id)
}

/// Get LLM provider IDs only (6)
/// v0.27: Ollama removed
pub fn llm_provider_ids() -> impl Iterator<Item = &'static str> {
    llm_providers().into_iter().map(|p| p.id)
}

/// Get MCP provider IDs only (11)
pub fn mcp_provider_ids() -> impl Iterator<Item = &'static str> {
    mcp_providers().into_iter().map(|p| p.id)
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_providers_count() {
        let count = llm_providers().len();
        // v0.27: Ollama removed
        assert_eq!(
            count, 6,
            "Expected 6 LLM providers (anthropic, openai, mistral, groq, deepseek, gemini)"
        );
    }

    #[test]
    fn test_local_providers_count() {
        let count = local_providers().len();
        // v0.27: Ollama removed
        assert_eq!(count, 1, "Expected 1 Local provider (native)");
    }

    #[test]
    fn test_mcp_providers_count() {
        let count = mcp_providers().len();
        assert_eq!(
            count, 11,
            "Expected 11 MCP providers (neo4j, github, slack, perplexity, firecrawl, supadata, ...)"
        );
    }

    #[test]
    fn test_all_providers_count() {
        let count = KNOWN_PROVIDERS.len();
        // v0.27: Ollama removed
        assert_eq!(
            count, 18,
            "Expected 18 total providers (6 LLM + 11 MCP + 1 Local)"
        );
    }

    #[test]
    fn test_llm_provider_ids() {
        let ids: Vec<_> = llm_provider_ids().collect();
        assert!(ids.contains(&"anthropic"));
        assert!(ids.contains(&"openai"));
        assert!(ids.contains(&"gemini"));
        // v0.27: Ollama removed
        assert!(
            !ids.contains(&"ollama"),
            "Ollama should not be in LLM providers"
        );
    }

    #[test]
    fn test_mcp_provider_ids() {
        let ids: Vec<_> = mcp_provider_ids().collect();
        assert!(ids.contains(&"neo4j"));
        assert!(ids.contains(&"perplexity"));
        assert!(ids.contains(&"firecrawl"));
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
    fn test_env_var_neo4j() {
        assert_eq!(env_var("neo4j"), "NEO4J_PASSWORD");
    }

    #[test]
    fn test_env_var_unknown() {
        // Unknown providers return UNKNOWN_API_KEY
        assert_eq!(env_var("unknown_provider"), "UNKNOWN_API_KEY");
    }

    #[test]
    fn test_mask_key() {
        assert_eq!(mask_key("sk-ant-api03-verylongkey"), "sk-ant-a...");
        assert_eq!(mask_key("short"), "***");
        assert_eq!(mask_key("12345678901"), "12345678...");
    }
}
