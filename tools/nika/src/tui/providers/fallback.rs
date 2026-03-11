//! Fallback provider definitions when spn-daemon feature is disabled
//!
//! This module provides minimal provider definitions that mirror spn-core
//! for builds without the spn-daemon dependency.
//!
//! v0.27: Ollama removed

/// LLM provider IDs (6 total)
/// v0.27: Ollama removed
pub static LLM_PROVIDER_IDS: &[&str] = &[
    "anthropic",
    "openai",
    "mistral",
    "groq",
    "deepseek",
    "gemini",
];

/// MCP service provider IDs (8 total)
pub static MCP_PROVIDER_IDS: &[&str] = &[
    "neo4j",
    "github",
    "slack",
    "perplexity",
    "firecrawl",
    "supadata",
    "dataforseo",
    "ahrefs",
];

/// Get environment variable name for a provider
///
/// This is the fallback when spn-core is not available.
/// v0.27: Ollama removed
pub fn provider_env_var(provider: &str) -> &'static str {
    match provider {
        // LLM providers (6)
        "anthropic" => "ANTHROPIC_API_KEY",
        "openai" => "OPENAI_API_KEY",
        "mistral" => "MISTRAL_API_KEY",
        "groq" => "GROQ_API_KEY",
        "deepseek" => "DEEPSEEK_API_KEY",
        "gemini" => "GEMINI_API_KEY",
        // Local providers (1)
        "native" => "NIKA_NATIVE_MODEL_PATH",
        // MCP providers
        "neo4j" => "NEO4J_PASSWORD",
        "github" => "GITHUB_TOKEN",
        "slack" => "SLACK_BOT_TOKEN",
        "perplexity" => "PERPLEXITY_API_KEY",
        "firecrawl" => "FIRECRAWL_API_KEY",
        "supadata" => "SUPADATA_API_KEY",
        "dataforseo" => "DATAFORSEO_API_KEY",
        "ahrefs" => "AHREFS_API_KEY",
        // Unknown
        _ => "UNKNOWN_API_KEY",
    }
}

/// Validate API key format (fallback - always returns true)
pub fn validate_key_format(_provider: &str, _key: &str) -> bool {
    // Without spn-core, we can't validate key formats
    // Return true to allow any key
    true
}

/// Mask an API key for display (fallback implementation, UTF-8 safe)
pub fn mask_key(key: &str) -> String {
    let char_count = key.chars().count();
    if char_count <= 8 {
        "*".repeat(char_count)
    } else {
        let first_4: String = key.chars().take(4).collect();
        let last_4: String = key.chars().skip(char_count - 4).collect();
        format!("{}...{}", first_4, last_4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_provider_count() {
        // v0.27: Ollama removed
        assert_eq!(LLM_PROVIDER_IDS.len(), 6);
    }

    #[test]
    fn test_mcp_provider_count() {
        assert_eq!(MCP_PROVIDER_IDS.len(), 8);
    }

    #[test]
    fn test_provider_env_var_anthropic() {
        assert_eq!(provider_env_var("anthropic"), "ANTHROPIC_API_KEY");
    }

    #[test]
    fn test_provider_env_var_gemini() {
        assert_eq!(provider_env_var("gemini"), "GEMINI_API_KEY");
    }

    #[test]
    fn test_provider_env_var_neo4j() {
        assert_eq!(provider_env_var("neo4j"), "NEO4J_PASSWORD");
    }

    #[test]
    fn test_mask_key_short() {
        assert_eq!(mask_key("abc"), "***");
    }

    #[test]
    fn test_mask_key_long() {
        assert_eq!(mask_key("sk-ant-1234567890abcdef"), "sk-a...cdef");
    }
}
