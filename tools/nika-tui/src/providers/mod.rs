//! Unified provider access for Nika TUI
//!
//! Single source of truth: Uses `KNOWN_PROVIDERS` from nika::core.
//! All provider operations should go through this module to avoid duplication.
//!
//!
//! # Architecture
//!
//! ```text
//! nika::core::KNOWN_PROVIDERS (19 providers: 7 LLM + 11 MCP + 1 Local)
//!         ↓
//! nika::tui::providers (this module)
//!         ↓
//! ┌───────────────────────────────────────────┐
//! │  provider_checker.rs  │  tabs/keys.rs    │
//! │  settings.rs          │  keys.rs         │
//! └───────────────────────────────────────────┘
//! ```

pub mod icons;
pub mod status;

// Re-export nika::core types
pub use nika_engine::core::{
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

// Note: Fallback module - all providers come from nika::core

/// Get all LLM providers (7: anthropic, openai, mistral, groq, deepseek, gemini, xai)
pub fn llm_providers() -> Vec<&'static Provider> {
    providers_by_category(ProviderCategory::Llm)
}

/// Get all Local providers (1: native)
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

/// Get LLM provider IDs only (7)
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
        assert_eq!(
            count, 14,
            "Expected 14 LLM providers (7 rig-core + 7 OpenAI-compat)"
        );
    }

    #[test]
    fn test_local_providers_count() {
        let count = local_providers().len();
        assert_eq!(count, 2, "Expected 2 Local providers (native + mock)");
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
        assert_eq!(
            count, 27,
            "Expected 27 total providers (14 LLM + 11 MCP + 2 Local)"
        );
    }

    #[test]
    fn test_llm_provider_ids() {
        let ids: Vec<_> = llm_provider_ids().collect();
        assert!(ids.contains(&"anthropic"));
        assert!(ids.contains(&"openai"));
        assert!(ids.contains(&"gemini"));
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
