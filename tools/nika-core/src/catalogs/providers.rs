// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Provider definitions for LLM and MCP services.
//!
//! This module defines the 19 known providers that nika supports, categorized as:
//! - **LLM providers** (7): Anthropic, OpenAI, Mistral, Groq, DeepSeek, Gemini, xAI
//! - **MCP providers** (11): Neo4j, GitHub, Slack, Perplexity, Firecrawl, Supadata, etc.
//! - **Local providers** (1): Native inference (mistral.rs)
//!

use serde::{Deserialize, Serialize};
use std::fmt;

/// Category of provider service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    /// Alternative names that resolve to this provider (e.g., "claude" for anthropic)
    pub aliases: &'static [&'static str],
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

impl Provider {
    /// Check if this provider's API key is available in the environment.
    pub fn has_env_key(&self) -> bool {
        std::env::var(self.env_var).is_ok_and(|v| !v.trim().is_empty())
    }
}

/// All known providers (27 total).
///
/// ## Categories
///
/// - **LLM rig-core (7)**: anthropic, openai, mistral, groq, deepseek, gemini, xai
/// - **LLM OpenAI-compat (7)**: openrouter, together, fireworks, cerebras, sambanova, cohere, ai21
/// - **MCP (11)**: neo4j, github, slack, perplexity, firecrawl, supadata, dataforseo, ahrefs, postgres, filesystem, memory
/// - **Local (2)**: native (mistral.rs), mock
pub static KNOWN_PROVIDERS: &[Provider] = &[
    // =============================================================================
    // LLM PROVIDERS (7)
    // =============================================================================
    Provider {
        id: "anthropic",
        name: "Anthropic Claude",
        aliases: &["claude"],
        env_var: "ANTHROPIC_API_KEY",
        key_prefix: Some("sk-ant-"),
        category: ProviderCategory::Llm,
        requires_key: true,
        description: "Claude models (Opus, Sonnet, Haiku)",
    },
    Provider {
        id: "openai",
        name: "OpenAI",
        aliases: &["gpt"],
        env_var: "OPENAI_API_KEY",
        key_prefix: Some("sk-"),
        category: ProviderCategory::Llm,
        requires_key: true,
        description: "GPT-4, GPT-4o, and other OpenAI models",
    },
    Provider {
        id: "mistral",
        name: "Mistral AI",
        aliases: &[],
        env_var: "MISTRAL_API_KEY",
        key_prefix: None,
        category: ProviderCategory::Llm,
        requires_key: true,
        description: "Mistral Large, Medium, Small models",
    },
    Provider {
        id: "groq",
        name: "Groq",
        aliases: &[],
        env_var: "GROQ_API_KEY",
        key_prefix: Some("gsk_"),
        category: ProviderCategory::Llm,
        requires_key: true,
        description: "Fast inference with Llama, Mixtral models",
    },
    Provider {
        id: "deepseek",
        name: "DeepSeek",
        aliases: &["deep-seek"],
        env_var: "DEEPSEEK_API_KEY",
        key_prefix: Some("sk-"),
        category: ProviderCategory::Llm,
        requires_key: true,
        description: "DeepSeek Chat and Coder models",
    },
    Provider {
        id: "gemini",
        name: "Google Gemini",
        aliases: &["google"],
        env_var: "GEMINI_API_KEY",
        key_prefix: None,
        category: ProviderCategory::Llm,
        requires_key: true,
        description: "Gemini Pro, Flash, and Ultra models",
    },
    Provider {
        id: "xai",
        name: "xAI Grok",
        aliases: &["grok"],
        env_var: "XAI_API_KEY",
        key_prefix: None,
        category: ProviderCategory::Llm,
        requires_key: true,
        description: "Grok models (Grok-3, Grok-4)",
    },
    // =============================================================================
    // OPENAI-COMPAT LLM PROVIDERS (7) — zero Rust code, config-driven
    // =============================================================================
    Provider {
        id: "openrouter",
        name: "OpenRouter",
        aliases: &["or"],
        env_var: "OPENROUTER_API_KEY",
        key_prefix: Some("sk-or-"),
        category: ProviderCategory::Llm,
        requires_key: true,
        description: "200+ models via unified gateway",
    },
    Provider {
        id: "together",
        name: "Together AI",
        aliases: &["together-ai"],
        env_var: "TOGETHER_API_KEY",
        key_prefix: None,
        category: ProviderCategory::Llm,
        requires_key: true,
        description: "Open-source models (Llama, Mixtral, Qwen)",
    },
    Provider {
        id: "fireworks",
        name: "Fireworks AI",
        aliases: &["fw"],
        env_var: "FIREWORKS_API_KEY",
        key_prefix: Some("fw_"),
        category: ProviderCategory::Llm,
        requires_key: true,
        description: "Fast inference for open-source models",
    },
    Provider {
        id: "cerebras",
        name: "Cerebras",
        aliases: &[],
        env_var: "CEREBRAS_API_KEY",
        key_prefix: Some("csk-"),
        category: ProviderCategory::Llm,
        requires_key: true,
        description: "Wafer-scale inference (2000+ tok/sec)",
    },
    Provider {
        id: "sambanova",
        name: "SambaNova",
        aliases: &["samba"],
        env_var: "SAMBANOVA_API_KEY",
        key_prefix: None,
        category: ProviderCategory::Llm,
        requires_key: true,
        description: "RDU-accelerated inference",
    },
    Provider {
        id: "cohere",
        name: "Cohere",
        aliases: &["command-r"],
        env_var: "COHERE_API_KEY",
        key_prefix: None,
        category: ProviderCategory::Llm,
        requires_key: true,
        description: "Command R+ and Embed models",
    },
    Provider {
        id: "ai21",
        name: "AI21 Labs",
        aliases: &["jamba"],
        env_var: "AI21_API_KEY",
        key_prefix: None,
        category: ProviderCategory::Llm,
        requires_key: true,
        description: "Jamba models (SSM-Transformer hybrid)",
    },
    // =============================================================================
    // MCP PROVIDERS (11)
    // =============================================================================
    Provider {
        id: "neo4j",
        name: "Neo4j",
        aliases: &[],
        env_var: "NEO4J_PASSWORD",
        key_prefix: None,
        category: ProviderCategory::Mcp,
        requires_key: true,
        description: "Neo4j graph database MCP server",
    },
    Provider {
        id: "github",
        name: "GitHub",
        aliases: &[],
        env_var: "GITHUB_TOKEN",
        key_prefix: Some("ghp_"),
        category: ProviderCategory::Mcp,
        requires_key: true,
        description: "GitHub API for repos, issues, PRs",
    },
    Provider {
        id: "slack",
        name: "Slack",
        aliases: &[],
        env_var: "SLACK_BOT_TOKEN",
        key_prefix: Some("xoxb-"),
        category: ProviderCategory::Mcp,
        requires_key: true,
        description: "Slack workspace integration",
    },
    Provider {
        id: "perplexity",
        name: "Perplexity",
        aliases: &[],
        env_var: "PERPLEXITY_API_KEY",
        key_prefix: Some("pplx-"),
        category: ProviderCategory::Mcp,
        requires_key: true,
        description: "Web search and research MCP server",
    },
    Provider {
        id: "firecrawl",
        name: "Firecrawl",
        aliases: &[],
        env_var: "FIRECRAWL_API_KEY",
        key_prefix: Some("fc-"),
        category: ProviderCategory::Mcp,
        requires_key: true,
        description: "Web scraping and crawling MCP server",
    },
    Provider {
        id: "supadata",
        name: "Supadata",
        aliases: &[],
        env_var: "SUPADATA_API_KEY",
        key_prefix: None,
        category: ProviderCategory::Mcp,
        requires_key: true,
        description: "Video transcription MCP server",
    },
    Provider {
        id: "dataforseo",
        name: "DataForSEO",
        aliases: &[],
        env_var: "DATAFORSEO_API_KEY",
        key_prefix: None,
        category: ProviderCategory::Mcp,
        requires_key: true,
        description: "SEO data and keyword research",
    },
    Provider {
        id: "ahrefs",
        name: "Ahrefs",
        aliases: &[],
        env_var: "AHREFS_API_KEY",
        key_prefix: None,
        category: ProviderCategory::Mcp,
        requires_key: true,
        description: "Backlink and SEO analysis",
    },
    Provider {
        id: "postgres",
        name: "PostgreSQL",
        aliases: &[],
        env_var: "POSTGRES_URL",
        key_prefix: None,
        category: ProviderCategory::Mcp,
        requires_key: true,
        description: "PostgreSQL database MCP server",
    },
    Provider {
        id: "filesystem",
        name: "Filesystem",
        aliases: &[],
        env_var: "FILESYSTEM_ALLOWED_PATHS",
        key_prefix: None,
        category: ProviderCategory::Mcp,
        requires_key: false,
        description: "Local filesystem access MCP server",
    },
    Provider {
        id: "memory",
        name: "Memory",
        aliases: &[],
        env_var: "MEMORY_STORAGE_PATH",
        key_prefix: None,
        category: ProviderCategory::Mcp,
        requires_key: false,
        description: "Persistent memory MCP server",
    },
    // =============================================================================
    // LOCAL PROVIDERS (1)
    // =============================================================================
    Provider {
        id: "native",
        name: "Native Inference",
        aliases: &["local"],
        env_var: "NIKA_NATIVE_MODEL_PATH",
        key_prefix: None,
        category: ProviderCategory::Local,
        requires_key: false,
        description: "Local GGUF models via mistral.rs",
    },
    // =============================================================================
    // MOCK PROVIDER (1)
    // =============================================================================
    Provider {
        id: "mock",
        name: "Mock",
        aliases: &[],
        env_var: "",
        key_prefix: None,
        category: ProviderCategory::Local,
        requires_key: false,
        description: "Deterministic test responses — no API calls, no keys needed",
    },
];

/// Find a provider by ID or alias (case-insensitive).
///
/// Matches against `provider.id` first, then `provider.aliases`.
///
/// # Example
///
/// ```
/// use nika_core::catalogs::providers::find_provider;
///
/// let provider = find_provider("anthropic").unwrap();
/// assert_eq!(provider.env_var, "ANTHROPIC_API_KEY");
///
/// // Also resolves aliases
/// let same = find_provider("claude").unwrap();
/// assert_eq!(same.id, "anthropic");
/// ```
pub fn find_provider(name: &str) -> Option<&'static Provider> {
    let lower = name.to_lowercase();
    KNOWN_PROVIDERS
        .iter()
        .find(|p| p.id == lower || p.aliases.iter().any(|a| *a == lower))
}

/// Get the environment variable name for a provider ID.
///
/// Returns `None` if the provider is not found.
///
/// # Example
///
/// ```
/// use nika_core::catalogs::providers::provider_to_env_var;
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
/// use nika_core::catalogs::providers::{providers_by_category, ProviderCategory};
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
/// use nika_core::catalogs::providers::{find_provider, validate_key_format};
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
        // 14 LLM + 11 MCP + 2 Local = 27 total
        assert_eq!(KNOWN_PROVIDERS.len(), 27);
    }

    #[test]
    fn test_provider_categories() {
        let llm = providers_by_category(ProviderCategory::Llm);
        let mcp = providers_by_category(ProviderCategory::Mcp);
        let local = providers_by_category(ProviderCategory::Local);

        assert_eq!(llm.len(), 14); // 7 rig-core + 7 OpenAI-compat
        assert_eq!(mcp.len(), 11);
        assert_eq!(local.len(), 2); // native + mock
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

        assert!(find_provider("ollama").is_none());
        assert!(find_provider("nonexistent").is_none());
    }

    #[test]
    fn test_find_provider_by_alias() {
        // "claude" -> anthropic
        let p = find_provider("claude").unwrap();
        assert_eq!(p.id, "anthropic");

        // "gpt" -> openai
        let p = find_provider("gpt").unwrap();
        assert_eq!(p.id, "openai");

        // "deep-seek" -> deepseek
        let p = find_provider("deep-seek").unwrap();
        assert_eq!(p.id, "deepseek");

        // "google" -> gemini
        let p = find_provider("google").unwrap();
        assert_eq!(p.id, "gemini");

        // "local" -> native
        let p = find_provider("local").unwrap();
        assert_eq!(p.id, "native");

        // "or" -> openrouter
        let p = find_provider("or").unwrap();
        assert_eq!(p.id, "openrouter");

        // "together-ai" -> together
        let p = find_provider("together-ai").unwrap();
        assert_eq!(p.id, "together");

        // "fw" -> fireworks
        let p = find_provider("fw").unwrap();
        assert_eq!(p.id, "fireworks");

        // "samba" -> sambanova
        let p = find_provider("samba").unwrap();
        assert_eq!(p.id, "sambanova");

        // "command-r" -> cohere
        let p = find_provider("command-r").unwrap();
        assert_eq!(p.id, "cohere");

        // "jamba" -> ai21
        let p = find_provider("jamba").unwrap();
        assert_eq!(p.id, "ai21");

        // case-insensitive
        let p = find_provider("Claude").unwrap();
        assert_eq!(p.id, "anthropic");
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
