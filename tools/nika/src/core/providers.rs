//! Provider definitions for LLM and MCP services.
//!
//! This module defines the 18 known providers that nika supports, categorized as:
//! - **LLM providers** (6): Anthropic, OpenAI, Mistral, Groq, DeepSeek, Gemini
//! - **MCP providers** (11): Neo4j, GitHub, Slack, Perplexity, Firecrawl, Supadata, etc.
//! - **Local providers** (1): Native inference (mistral.rs)
//!
//! Note: Ollama removed in v0.27 — use `provider: native` with mistral.rs instead.

use std::fmt;

/// Category of provider service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderCategory {
    /// LLM API providers (Anthropic, OpenAI, etc.)
    Llm,
    /// MCP server providers (Neo4j, GitHub, etc.)
    Mcp,
    /// Local inference providers (native)
    Local,
}

impl fmt::Display for ProviderCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Llm => write!(f, "LLM"),
            Self::Mcp => write!(f, "MCP"),
            Self::Local => write!(f, "Local"),
        }
    }
}

/// A known provider with metadata for secret management.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provider {
    /// Unique identifier (e.g., "anthropic", "neo4j")
    pub id: &'static str,
    /// Human-readable name (e.g., "Anthropic Claude")
    pub name: &'static str,
    /// Environment variable for the API key (e.g., "ANTHROPIC_API_KEY")
    pub env_var: &'static str,
    /// Expected key prefix for validation (e.g., "sk-ant-")
    pub key_prefix: Option<&'static str>,
    /// Provider category (LLM, MCP, Local)
    pub category: ProviderCategory,
    /// Whether this provider requires an API key
    pub requires_key: bool,
    /// Short description of the provider
    pub description: &'static str,
}

/// All known providers (18 total — Ollama removed in v0.27).
///
/// ## Categories
///
/// - **LLM (6)**: anthropic, openai, mistral, groq, deepseek, gemini
/// - **MCP (11)**: neo4j, github, slack, perplexity, firecrawl, supadata, dataforseo, ahrefs, postgres, filesystem, memory
/// - **Local (1)**: native (mistral.rs)
pub static KNOWN_PROVIDERS: &[Provider] = &[
    // ═══════════════════════════════════════════════════════════════════════════
    // LLM PROVIDERS (6) — Ollama removed v0.27, use provider: native instead
    // ═══════════════════════════════════════════════════════════════════════════
    Provider {
        id: "anthropic",
        name: "Anthropic Claude",
        env_var: "ANTHROPIC_API_KEY",
        key_prefix: Some("sk-ant-"),
        category: ProviderCategory::Llm,
        requires_key: true,
        description: "Claude models (Opus, Sonnet, Haiku)",
    },
    Provider {
        id: "openai",
        name: "OpenAI",
        env_var: "OPENAI_API_KEY",
        key_prefix: Some("sk-"),
        category: ProviderCategory::Llm,
        requires_key: true,
        description: "GPT-4, GPT-4o, and other OpenAI models",
    },
    Provider {
        id: "mistral",
        name: "Mistral AI",
        env_var: "MISTRAL_API_KEY",
        key_prefix: None,
        category: ProviderCategory::Llm,
        requires_key: true,
        description: "Mistral Large, Medium, Small models",
    },
    Provider {
        id: "groq",
        name: "Groq",
        env_var: "GROQ_API_KEY",
        key_prefix: Some("gsk_"),
        category: ProviderCategory::Llm,
        requires_key: true,
        description: "Fast inference with Llama, Mixtral models",
    },
    Provider {
        id: "deepseek",
        name: "DeepSeek",
        env_var: "DEEPSEEK_API_KEY",
        key_prefix: Some("sk-"),
        category: ProviderCategory::Llm,
        requires_key: true,
        description: "DeepSeek Chat and Coder models",
    },
    Provider {
        id: "gemini",
        name: "Google Gemini",
        env_var: "GEMINI_API_KEY",
        key_prefix: None,
        category: ProviderCategory::Llm,
        requires_key: true,
        description: "Gemini Pro, Flash, and Ultra models",
    },
    // ═══════════════════════════════════════════════════════════════════════════
    // MCP PROVIDERS (11)
    // NOTE: Ollama removed in v0.27 — use provider: native (mistral.rs) instead
    // ═══════════════════════════════════════════════════════════════════════════
    Provider {
        id: "neo4j",
        name: "Neo4j",
        env_var: "NEO4J_PASSWORD",
        key_prefix: None,
        category: ProviderCategory::Mcp,
        requires_key: true,
        description: "Neo4j graph database MCP server",
    },
    Provider {
        id: "github",
        name: "GitHub",
        env_var: "GITHUB_TOKEN",
        key_prefix: Some("ghp_"),
        category: ProviderCategory::Mcp,
        requires_key: true,
        description: "GitHub API for repos, issues, PRs",
    },
    Provider {
        id: "slack",
        name: "Slack",
        env_var: "SLACK_BOT_TOKEN",
        key_prefix: Some("xoxb-"),
        category: ProviderCategory::Mcp,
        requires_key: true,
        description: "Slack workspace integration",
    },
    Provider {
        id: "perplexity",
        name: "Perplexity",
        env_var: "PERPLEXITY_API_KEY",
        key_prefix: Some("pplx-"),
        category: ProviderCategory::Mcp,
        requires_key: true,
        description: "Web search and research MCP server",
    },
    Provider {
        id: "firecrawl",
        name: "Firecrawl",
        env_var: "FIRECRAWL_API_KEY",
        key_prefix: Some("fc-"),
        category: ProviderCategory::Mcp,
        requires_key: true,
        description: "Web scraping and crawling MCP server",
    },
    Provider {
        id: "supadata",
        name: "Supadata",
        env_var: "SUPADATA_API_KEY",
        key_prefix: None,
        category: ProviderCategory::Mcp,
        requires_key: true,
        description: "Video transcription MCP server",
    },
    Provider {
        id: "dataforseo",
        name: "DataForSEO",
        env_var: "DATAFORSEO_API_KEY",
        key_prefix: None,
        category: ProviderCategory::Mcp,
        requires_key: true,
        description: "SEO data and keyword research",
    },
    Provider {
        id: "ahrefs",
        name: "Ahrefs",
        env_var: "AHREFS_API_KEY",
        key_prefix: None,
        category: ProviderCategory::Mcp,
        requires_key: true,
        description: "Backlink and SEO analysis",
    },
    Provider {
        id: "postgres",
        name: "PostgreSQL",
        env_var: "POSTGRES_URL",
        key_prefix: None,
        category: ProviderCategory::Mcp,
        requires_key: true,
        description: "PostgreSQL database MCP server",
    },
    Provider {
        id: "filesystem",
        name: "Filesystem",
        env_var: "FILESYSTEM_ALLOWED_PATHS",
        key_prefix: None,
        category: ProviderCategory::Mcp,
        requires_key: false,
        description: "Local filesystem access MCP server",
    },
    Provider {
        id: "memory",
        name: "Memory",
        env_var: "MEMORY_STORAGE_PATH",
        key_prefix: None,
        category: ProviderCategory::Mcp,
        requires_key: false,
        description: "Persistent memory MCP server",
    },
    // ═══════════════════════════════════════════════════════════════════════════
    // LOCAL PROVIDERS (2)
    // ═══════════════════════════════════════════════════════════════════════════
    Provider {
        id: "native",
        name: "Native Inference",
        env_var: "NIKA_NATIVE_MODEL_PATH",
        key_prefix: None,
        category: ProviderCategory::Local,
        requires_key: false,
        description: "Local GGUF models via mistral.rs",
    },
];

/// Find a provider by ID.
///
/// # Example
///
/// ```
/// use nika::core::providers::find_provider;
///
/// let provider = find_provider("anthropic").unwrap();
/// assert_eq!(provider.env_var, "ANTHROPIC_API_KEY");
/// ```
pub fn find_provider(id: &str) -> Option<&'static Provider> {
    KNOWN_PROVIDERS.iter().find(|p| p.id == id)
}

/// Get the environment variable name for a provider ID.
///
/// Returns `None` if the provider is not found.
///
/// # Example
///
/// ```
/// use nika::core::providers::provider_to_env_var;
///
/// assert_eq!(provider_to_env_var("anthropic"), Some("ANTHROPIC_API_KEY"));
/// assert_eq!(provider_to_env_var("unknown"), None);
/// ```
pub fn provider_to_env_var(id: &str) -> Option<&'static str> {
    find_provider(id).map(|p| p.env_var)
}

/// Get all providers in a specific category.
///
/// # Example
///
/// ```
/// use nika::core::providers::{providers_by_category, ProviderCategory};
///
/// let llm_providers = providers_by_category(ProviderCategory::Llm);
/// assert!(llm_providers.iter().any(|p| p.id == "anthropic"));
/// assert!(llm_providers.iter().any(|p| p.id == "openai"));
/// ```
pub fn providers_by_category(category: ProviderCategory) -> Vec<&'static Provider> {
    KNOWN_PROVIDERS
        .iter()
        .filter(|p| p.category == category)
        .collect()
}

/// Validate an API key format against the provider's expected prefix.
///
/// Returns `true` if:
/// - The provider has no prefix requirement, OR
/// - The key starts with the expected prefix
///
/// # Example
///
/// ```
/// use nika::core::providers::{find_provider, validate_key_format};
///
/// let anthropic = find_provider("anthropic").unwrap();
/// assert!(validate_key_format(anthropic, "sk-ant-1234567890"));
/// assert!(!validate_key_format(anthropic, "invalid-key"));
///
/// let mistral = find_provider("mistral").unwrap();
/// assert!(validate_key_format(mistral, "any-format-ok")); // No prefix requirement
/// ```
pub fn validate_key_format(provider: &Provider, key: &str) -> bool {
    match provider.key_prefix {
        Some(prefix) => key.starts_with(prefix),
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_providers_count() {
        // 6 LLM + 11 MCP + 1 Local = 18 total (Ollama removed in v0.27)
        assert_eq!(KNOWN_PROVIDERS.len(), 18);
    }

    #[test]
    fn test_provider_categories() {
        let llm = providers_by_category(ProviderCategory::Llm);
        let mcp = providers_by_category(ProviderCategory::Mcp);
        let local = providers_by_category(ProviderCategory::Local);

        // Ollama removed in v0.27 — use provider: native instead
        assert_eq!(llm.len(), 6);
        assert_eq!(mcp.len(), 11);
        assert_eq!(local.len(), 1);
    }

    #[test]
    fn test_find_provider() {
        let anthropic = find_provider("anthropic").unwrap();
        assert_eq!(anthropic.id, "anthropic");
        assert_eq!(anthropic.env_var, "ANTHROPIC_API_KEY");
        assert_eq!(anthropic.key_prefix, Some("sk-ant-"));
        assert!(anthropic.requires_key);

        // Native provider doesn't require key (local inference)
        let native = find_provider("native").unwrap();
        assert!(!native.requires_key);

        // Ollama was removed in v0.27 — use native instead
        assert!(find_provider("ollama").is_none());

        assert!(find_provider("nonexistent").is_none());
    }

    #[test]
    fn test_provider_to_env_var() {
        assert_eq!(provider_to_env_var("anthropic"), Some("ANTHROPIC_API_KEY"));
        assert_eq!(provider_to_env_var("openai"), Some("OPENAI_API_KEY"));
        assert_eq!(provider_to_env_var("neo4j"), Some("NEO4J_PASSWORD"));
        assert_eq!(provider_to_env_var("unknown"), None);
    }

    #[test]
    fn test_validate_key_format() {
        let anthropic = find_provider("anthropic").unwrap();
        assert!(validate_key_format(anthropic, "sk-ant-abc123"));
        assert!(!validate_key_format(anthropic, "sk-proj-abc123"));
        assert!(!validate_key_format(anthropic, "abc123"));

        let groq = find_provider("groq").unwrap();
        assert!(validate_key_format(groq, "gsk_abc123"));
        assert!(!validate_key_format(groq, "sk-abc123"));

        // Providers without prefix accept anything
        let mistral = find_provider("mistral").unwrap();
        assert!(validate_key_format(mistral, "any-key-format"));
    }

    #[test]
    fn test_provider_category_display() {
        assert_eq!(ProviderCategory::Llm.to_string(), "LLM");
        assert_eq!(ProviderCategory::Mcp.to_string(), "MCP");
        assert_eq!(ProviderCategory::Local.to_string(), "Local");
    }

    #[test]
    fn test_all_llm_providers_have_expected_fields() {
        for provider in providers_by_category(ProviderCategory::Llm) {
            assert!(!provider.id.is_empty());
            assert!(!provider.name.is_empty());
            assert!(!provider.env_var.is_empty());
            assert!(!provider.description.is_empty());
        }
    }

    #[test]
    fn test_all_mcp_providers_have_expected_fields() {
        for provider in providers_by_category(ProviderCategory::Mcp) {
            assert!(!provider.id.is_empty());
            assert!(!provider.name.is_empty());
            assert!(!provider.env_var.is_empty());
            assert!(!provider.description.is_empty());
        }
    }
}
