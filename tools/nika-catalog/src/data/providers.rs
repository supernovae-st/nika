// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Static provider catalog — 21 LLM-only providers with phf+unicase lookup.
//!
//! Decision B (locked): providers are LLM-only. The 11 ex-MCP "providers"
//! from legacy migrate to [`crate::data::mcp_aliases`].

use phf::phf_map;
use unicase::UniCase;

use crate::types::Provider;

// ═══════════════════════════════════════════════════════════════════════════
// Provider definitions (21 total: 7 cloud + 7 OpenAI-compat + 5 enterprise + native + mock)
// ═══════════════════════════════════════════════════════════════════════════

static ANTHROPIC: Provider = Provider {
    id: "anthropic",
    name: "Anthropic Claude",
    aliases: &["claude"],
    env_var: "ANTHROPIC_API_KEY",
    key_prefix: Some("sk-ant-"),
    default_model: "claude-sonnet-4-20250514",
    cheap_model: "claude-haiku-4-5-20251001",
    requires_key: true,
    description: "Claude models (Opus, Sonnet, Haiku)",
};

static OPENAI: Provider = Provider {
    id: "openai",
    name: "OpenAI",
    aliases: &["gpt"],
    env_var: "OPENAI_API_KEY",
    key_prefix: Some("sk-"),
    default_model: "gpt-4o",
    cheap_model: "gpt-4o-mini",
    requires_key: true,
    description: "GPT-4, GPT-4o, and other OpenAI models",
};

static MISTRAL: Provider = Provider {
    id: "mistral",
    name: "Mistral AI",
    aliases: &[],
    env_var: "MISTRAL_API_KEY",
    key_prefix: None,
    default_model: "mistral-large-latest",
    cheap_model: "mistral-small-latest",
    requires_key: true,
    description: "Mistral Large, Medium, Small models",
};

static GROQ: Provider = Provider {
    id: "groq",
    name: "Groq",
    aliases: &[],
    env_var: "GROQ_API_KEY",
    key_prefix: Some("gsk_"),
    default_model: "llama-3.3-70b-versatile",
    cheap_model: "llama-3.1-8b-instant",
    requires_key: true,
    description: "Fast inference with Llama, Mixtral models",
};

static DEEPSEEK: Provider = Provider {
    id: "deepseek",
    name: "DeepSeek",
    aliases: &["deep-seek"],
    env_var: "DEEPSEEK_API_KEY",
    key_prefix: Some("sk-"),
    default_model: "deepseek-chat",
    cheap_model: "deepseek-chat",
    requires_key: true,
    description: "DeepSeek Chat and Coder models",
};

static GEMINI: Provider = Provider {
    id: "gemini",
    name: "Google Gemini",
    aliases: &["google"],
    env_var: "GEMINI_API_KEY",
    key_prefix: None,
    default_model: "gemini-2.5-flash",
    cheap_model: "gemini-2.0-flash",
    requires_key: true,
    description: "Gemini Pro, Flash, and Ultra models",
};

static XAI: Provider = Provider {
    id: "xai",
    name: "xAI Grok",
    aliases: &["grok"],
    env_var: "XAI_API_KEY",
    key_prefix: None,
    default_model: "grok-3",
    cheap_model: "grok-3-mini-fast",
    requires_key: true,
    description: "Grok models (Grok-3, Grok-4)",
};

// OpenAI-compatible providers (7)

static OPENROUTER: Provider = Provider {
    id: "openrouter",
    name: "OpenRouter",
    aliases: &["or"],
    env_var: "OPENROUTER_API_KEY",
    key_prefix: Some("sk-or-"),
    default_model: "anthropic/claude-sonnet-4-20250514",
    cheap_model: "anthropic/claude-haiku-4-5-20251001",
    requires_key: true,
    description: "200+ models via unified gateway",
};

static TOGETHER: Provider = Provider {
    id: "together",
    name: "Together AI",
    aliases: &["together-ai"],
    env_var: "TOGETHER_API_KEY",
    key_prefix: None,
    default_model: "meta-llama/Llama-3.3-70B-Instruct-Turbo",
    cheap_model: "meta-llama/Llama-3.1-8B-Instruct-Turbo",
    requires_key: true,
    description: "Open-source models (Llama, Mixtral, Qwen)",
};

static FIREWORKS: Provider = Provider {
    id: "fireworks",
    name: "Fireworks AI",
    aliases: &["fw"],
    env_var: "FIREWORKS_API_KEY",
    key_prefix: Some("fw_"),
    default_model: "accounts/fireworks/models/llama-v3p3-70b-instruct",
    cheap_model: "accounts/fireworks/models/llama-v3p1-8b-instruct",
    requires_key: true,
    description: "Fast inference for open-source models",
};

static CEREBRAS: Provider = Provider {
    id: "cerebras",
    name: "Cerebras",
    aliases: &[],
    env_var: "CEREBRAS_API_KEY",
    key_prefix: Some("csk-"),
    default_model: "llama-3.3-70b",
    cheap_model: "llama-3.1-8b",
    requires_key: true,
    description: "Wafer-scale inference (2000+ tok/sec)",
};

static SAMBANOVA: Provider = Provider {
    id: "sambanova",
    name: "SambaNova",
    aliases: &["samba"],
    env_var: "SAMBANOVA_API_KEY",
    key_prefix: None,
    default_model: "Meta-Llama-3.3-70B-Instruct",
    cheap_model: "Meta-Llama-3.1-8B-Instruct",
    requires_key: true,
    description: "RDU-accelerated inference",
};

static COHERE: Provider = Provider {
    id: "cohere",
    name: "Cohere",
    aliases: &["command-r"],
    env_var: "COHERE_API_KEY",
    key_prefix: None,
    default_model: "command-r-plus",
    cheap_model: "command-r",
    requires_key: true,
    description: "Command R+ and Embed models",
};

static AI21: Provider = Provider {
    id: "ai21",
    name: "AI21 Labs",
    aliases: &["jamba"],
    env_var: "AI21_API_KEY",
    key_prefix: None,
    default_model: "jamba-1.5-large",
    cheap_model: "jamba-1.5-mini",
    requires_key: true,
    description: "Jamba models (SSM-Transformer hybrid)",
};

// Enterprise / specialized providers (5)

static BEDROCK: Provider = Provider {
    id: "bedrock",
    name: "AWS Bedrock",
    aliases: &["aws-bedrock"],
    env_var: "AWS_ACCESS_KEY_ID",
    key_prefix: None,
    default_model: "anthropic.claude-sonnet-4-20250514-v1:0",
    cheap_model: "anthropic.claude-haiku-4-5-20251001-v1:0",
    requires_key: true,
    description: "AWS-managed inference gateway (Claude, Llama, Mistral)",
};

static AZURE: Provider = Provider {
    id: "azure",
    name: "Azure OpenAI",
    aliases: &["azure-openai"],
    env_var: "AZURE_OPENAI_API_KEY",
    key_prefix: None,
    default_model: "gpt-4o",
    cheap_model: "gpt-4o-mini",
    requires_key: true,
    description: "Azure-hosted OpenAI models with enterprise compliance",
};

static VERTEX: Provider = Provider {
    id: "vertex",
    name: "Google Vertex AI",
    aliases: &["gcp"],
    env_var: "GOOGLE_APPLICATION_CREDENTIALS",
    key_prefix: None,
    default_model: "gemini-2.5-flash",
    cheap_model: "gemini-2.0-flash",
    requires_key: true,
    description: "GCP-managed AI gateway (Gemini, Claude, Llama)",
};

static PERPLEXITY_LLM: Provider = Provider {
    id: "perplexity",
    name: "Perplexity",
    aliases: &["pplx"],
    env_var: "PERPLEXITY_API_KEY",
    key_prefix: Some("pplx-"),
    default_model: "sonar-pro",
    cheap_model: "sonar",
    requires_key: true,
    description: "Search-augmented LLM with real-time web access",
};

static VOYAGE: Provider = Provider {
    id: "voyage",
    name: "Voyage AI",
    aliases: &[],
    env_var: "VOYAGE_API_KEY",
    key_prefix: Some("pa-"),
    default_model: "voyage-3-large",
    cheap_model: "voyage-3-lite",
    requires_key: true,
    description: "Best-in-class embedding models for RAG and search",
};

// Special providers (2)

static NATIVE: Provider = Provider {
    id: "native",
    name: "Native Inference",
    aliases: &["local"],
    env_var: "NIKA_NATIVE_MODEL_PATH",
    key_prefix: None,
    default_model: "Qwen/Qwen3-8B",
    cheap_model: "Qwen/Qwen3-8B",
    requires_key: false,
    description: "Local GGUF models via mistral.rs",
};

static MOCK: Provider = Provider {
    id: "mock",
    name: "Mock",
    aliases: &[],
    env_var: "",
    key_prefix: None,
    default_model: "mock-default",
    cheap_model: "mock-default",
    requires_key: false,
    description: "Deterministic test responses — no API calls, no keys needed",
};

// ═══════════════════════════════════════════════════════════════════════════
// Flat array for iteration
// ═══════════════════════════════════════════════════════════════════════════

/// All 21 providers in definition order (7 cloud + 7 compat + 5 enterprise + native + mock).
pub static ALL_PROVIDERS: &[&Provider] = &[
    &ANTHROPIC, &OPENAI, &MISTRAL, &GROQ, &DEEPSEEK, &GEMINI, &XAI,
    &OPENROUTER, &TOGETHER, &FIREWORKS, &CEREBRAS, &SAMBANOVA, &COHERE, &AI21,
    &BEDROCK, &AZURE, &VERTEX, &PERPLEXITY_LLM, &VOYAGE,
    &NATIVE, &MOCK,
];

// ═══════════════════════════════════════════════════════════════════════════
// phf + unicase map (case-insensitive lookup)
// ═══════════════════════════════════════════════════════════════════════════

/// Case-insensitive provider lookup map.
///
/// Maps canonical ids AND aliases to provider references.
/// `UniCase::ascii()` wraps `&str` on stack (Copy), hashes with ASCII lowering.
static PROVIDER_MAP: phf::Map<UniCase<&'static str>, &'static Provider> = phf_map! {
    // Canonical IDs
    UniCase::ascii("anthropic") => &ANTHROPIC,
    UniCase::ascii("openai") => &OPENAI,
    UniCase::ascii("mistral") => &MISTRAL,
    UniCase::ascii("groq") => &GROQ,
    UniCase::ascii("deepseek") => &DEEPSEEK,
    UniCase::ascii("gemini") => &GEMINI,
    UniCase::ascii("xai") => &XAI,
    UniCase::ascii("openrouter") => &OPENROUTER,
    UniCase::ascii("together") => &TOGETHER,
    UniCase::ascii("fireworks") => &FIREWORKS,
    UniCase::ascii("cerebras") => &CEREBRAS,
    UniCase::ascii("sambanova") => &SAMBANOVA,
    UniCase::ascii("cohere") => &COHERE,
    UniCase::ascii("ai21") => &AI21,
    UniCase::ascii("native") => &NATIVE,
    UniCase::ascii("mock") => &MOCK,
    // Aliases
    UniCase::ascii("claude") => &ANTHROPIC,
    UniCase::ascii("gpt") => &OPENAI,
    UniCase::ascii("deep-seek") => &DEEPSEEK,
    UniCase::ascii("google") => &GEMINI,
    UniCase::ascii("grok") => &XAI,
    UniCase::ascii("or") => &OPENROUTER,
    UniCase::ascii("together-ai") => &TOGETHER,
    UniCase::ascii("fw") => &FIREWORKS,
    UniCase::ascii("samba") => &SAMBANOVA,
    UniCase::ascii("command-r") => &COHERE,
    UniCase::ascii("jamba") => &AI21,
    UniCase::ascii("aws-bedrock") => &BEDROCK,
    UniCase::ascii("azure-openai") => &AZURE,
    UniCase::ascii("gcp") => &VERTEX,
    UniCase::ascii("pplx") => &PERPLEXITY_LLM,
    UniCase::ascii("local") => &NATIVE,
    // New canonical IDs
    UniCase::ascii("bedrock") => &BEDROCK,
    UniCase::ascii("azure") => &AZURE,
    UniCase::ascii("vertex") => &VERTEX,
    UniCase::ascii("perplexity") => &PERPLEXITY_LLM,
    UniCase::ascii("voyage") => &VOYAGE,
};

/// Find a provider by id or alias (case-insensitive, O(1) via phf).
///
/// # Examples
///
/// ```
/// use nika_catalog::data::providers::find_provider;
///
/// let p = find_provider("anthropic").unwrap();
/// assert_eq!(p.id, "anthropic");
///
/// // Case-insensitive
/// let p = find_provider("Anthropic").unwrap();
/// assert_eq!(p.id, "anthropic");
///
/// // Alias
/// let p = find_provider("claude").unwrap();
/// assert_eq!(p.id, "anthropic");
/// ```
#[must_use]
pub fn find_provider(name: &str) -> Option<&'static Provider> {
    PROVIDER_MAP.get(&UniCase::ascii(name)).copied()
}

/// Validate an API key format against a provider's expected prefix.
///
/// Returns `false` for empty keys. Returns `true` if the provider has
/// no prefix requirement or the key starts with the expected prefix.
#[must_use]
pub fn validate_key_format(provider: &Provider, key: &str) -> bool {
    if key.is_empty() {
        return false;
    }
    match provider.key_prefix {
        Some(prefix) => key.starts_with(prefix),
        None => true,
    }
}

#[cfg(test)]
#[allow(clippy::disallowed_types, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn provider_count() {
        assert_eq!(ALL_PROVIDERS.len(), 21);
    }

    #[test]
    fn find_all_canonical_ids() {
        let ids = [
            "anthropic", "openai", "mistral", "groq", "deepseek", "gemini", "xai",
            "openrouter", "together", "fireworks", "cerebras", "sambanova", "cohere", "ai21",
            "bedrock", "azure", "vertex", "perplexity", "voyage",
            "native", "mock",
        ];
        for id in ids {
            assert!(
                find_provider(id).is_some(),
                "canonical id `{id}` not found in provider map"
            );
        }
    }

    #[test]
    fn find_by_alias() {
        let cases = [
            ("claude", "anthropic"),
            ("gpt", "openai"),
            ("deep-seek", "deepseek"),
            ("google", "gemini"),
            ("grok", "xai"),
            ("or", "openrouter"),
            ("together-ai", "together"),
            ("fw", "fireworks"),
            ("samba", "sambanova"),
            ("command-r", "cohere"),
            ("jamba", "ai21"),
            ("aws-bedrock", "bedrock"),
            ("azure-openai", "azure"),
            ("gcp", "vertex"),
            ("pplx", "perplexity"),
            ("local", "native"),
        ];
        for (alias, expected_id) in cases {
            let p = find_provider(alias).unwrap_or_else(|| panic!("alias `{alias}` not found"));
            assert_eq!(p.id, expected_id, "alias `{alias}` resolved to wrong provider");
        }
    }

    #[test]
    fn case_insensitive_lookup() {
        let cases = ["Anthropic", "ANTHROPIC", "Claude", "CLAUDE", "OpenAI", "GPT"];
        for name in cases {
            assert!(
                find_provider(name).is_some(),
                "case-insensitive lookup failed for `{name}`"
            );
        }
    }

    #[test]
    fn unknown_returns_none() {
        assert!(find_provider("ollama").is_none());
        assert!(find_provider("nonexistent").is_none());
        assert!(find_provider("").is_none());
    }

    #[test]
    fn all_providers_have_required_fields() {
        for provider in ALL_PROVIDERS {
            assert!(!provider.id.is_empty(), "provider has empty id");
            assert!(!provider.name.is_empty(), "provider `{}` has empty name", provider.id);
            assert!(
                !provider.description.is_empty(),
                "provider `{}` has empty description",
                provider.id
            );
            assert!(
                !provider.default_model.is_empty(),
                "provider `{}` has empty default_model",
                provider.id
            );
            assert!(
                !provider.cheap_model.is_empty(),
                "provider `{}` has empty cheap_model",
                provider.id
            );
        }
    }

    #[test]
    fn all_providers_with_key_have_env_var() {
        for provider in ALL_PROVIDERS {
            if provider.requires_key {
                assert!(
                    !provider.env_var.is_empty(),
                    "provider `{}` requires key but has empty env_var",
                    provider.id
                );
            }
        }
    }

    #[test]
    fn key_format_validation() {
        let anthropic = find_provider("anthropic").unwrap();
        assert!(validate_key_format(anthropic, "sk-ant-abc123"));
        assert!(!validate_key_format(anthropic, "sk-proj-abc123"));

        let groq = find_provider("groq").unwrap();
        assert!(validate_key_format(groq, "gsk_abc123"));
        assert!(!validate_key_format(groq, "sk-abc123"));

        // No prefix = anything non-empty passes
        let mistral = find_provider("mistral").unwrap();
        assert!(validate_key_format(mistral, "any-format"));

        // Empty key always fails
        assert!(!validate_key_format(anthropic, ""));
        assert!(!validate_key_format(mistral, ""));
    }

    #[test]
    fn no_duplicate_ids_in_all() {
        let mut seen = std::collections::HashSet::new();
        for provider in ALL_PROVIDERS {
            assert!(
                seen.insert(provider.id),
                "duplicate provider id: `{}`",
                provider.id
            );
        }
    }

    #[test]
    fn all_provider_aliases_in_map() {
        for provider in ALL_PROVIDERS {
            let p = find_provider(provider.id)
                .unwrap_or_else(|| panic!("canonical id `{}` not in PROVIDER_MAP", provider.id));
            assert_eq!(p.id, provider.id);
            for alias in provider.aliases {
                let p = find_provider(alias)
                    .unwrap_or_else(|| panic!("alias `{alias}` for `{}` not in PROVIDER_MAP", provider.id));
                assert_eq!(p.id, provider.id, "alias `{alias}` resolves to wrong provider");
            }
        }
    }
}
