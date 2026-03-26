//! Secrets service — env vars + optional keyring access.
//!
//! The daemon's secrets service is the centralized provider of API keys.
//! It checks environment variables first, then the system keychain.
//! This avoids the need for each `nika run` invocation to hit the keychain.

use crate::protocol::{ProviderSecretInfo, SecretSource};

/// Known LLM providers and their environment variable names.
const PROVIDERS: &[(&str, &str)] = &[
    ("anthropic", "ANTHROPIC_API_KEY"),
    ("openai", "OPENAI_API_KEY"),
    ("mistral", "MISTRAL_API_KEY"),
    ("groq", "GROQ_API_KEY"),
    ("deepseek", "DEEPSEEK_API_KEY"),
    ("gemini", "GEMINI_API_KEY"),
    ("xai", "XAI_API_KEY"),
];

/// The secrets service.
pub struct SecretService {
    // Future: keyring integration, secret rotation, etc.
}

impl SecretService {
    /// Create a new secrets service.
    pub fn new() -> Self {
        Self {}
    }

    /// Get a secret for a provider (env var lookup).
    pub async fn get_secret(&self, provider: &str) -> Option<String> {
        let env_var = provider_env_var(provider)?;
        std::env::var(env_var)
            .ok()
            .filter(|v| !v.is_empty())
    }

    /// Check if a secret exists for a provider.
    pub async fn has_secret(&self, provider: &str) -> bool {
        self.get_secret(provider).await.is_some()
    }

    /// List all provider secret status.
    pub async fn list_secrets(&self) -> Vec<ProviderSecretInfo> {
        let mut result = Vec::with_capacity(PROVIDERS.len());
        for &(provider, env_var) in PROVIDERS {
            let source = if std::env::var(env_var)
                .ok()
                .filter(|v| !v.is_empty())
                .is_some()
            {
                SecretSource::Env
            } else {
                SecretSource::NotFound
            };
            result.push(ProviderSecretInfo {
                provider: provider.to_string(),
                source,
            });
        }
        result
    }
}

fn provider_env_var(provider: &str) -> Option<&'static str> {
    PROVIDERS
        .iter()
        .find(|&&(p, _)| p == provider)
        .map(|&(_, env)| env)
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn provider_env_var_lookup() {
        assert_eq!(provider_env_var("anthropic"), Some("ANTHROPIC_API_KEY"));
        assert_eq!(provider_env_var("openai"), Some("OPENAI_API_KEY"));
        assert_eq!(provider_env_var("unknown"), None);
    }

    #[tokio::test]
    #[serial]
    async fn get_secret_from_env() {
        let orig = std::env::var("ANTHROPIC_API_KEY").ok();
        std::env::set_var("ANTHROPIC_API_KEY", "sk-test-123");

        let svc = SecretService::new();
        let secret = svc.get_secret("anthropic").await;
        assert_eq!(secret, Some("sk-test-123".into()));

        // Restore
        match orig {
            Some(v) => std::env::set_var("ANTHROPIC_API_KEY", v),
            None => unsafe { std::env::remove_var("ANTHROPIC_API_KEY") },
        }
    }

    #[tokio::test]
    #[serial]
    async fn get_secret_empty_returns_none() {
        let orig = std::env::var("XAI_API_KEY").ok();
        std::env::set_var("XAI_API_KEY", "");

        let svc = SecretService::new();
        let secret = svc.get_secret("xai").await;
        assert_eq!(secret, None);

        match orig {
            Some(v) => std::env::set_var("XAI_API_KEY", v),
            None => unsafe { std::env::remove_var("XAI_API_KEY") },
        }
    }

    #[tokio::test]
    async fn get_secret_unknown_provider_returns_none() {
        let svc = SecretService::new();
        let secret = svc.get_secret("nonexistent").await;
        assert_eq!(secret, None);
    }

    #[tokio::test]
    #[serial]
    async fn has_secret_true_when_set() {
        let orig = std::env::var("MISTRAL_API_KEY").ok();
        std::env::set_var("MISTRAL_API_KEY", "test-key");

        let svc = SecretService::new();
        assert!(svc.has_secret("mistral").await);

        match orig {
            Some(v) => std::env::set_var("MISTRAL_API_KEY", v),
            None => unsafe { std::env::remove_var("MISTRAL_API_KEY") },
        }
    }

    #[tokio::test]
    async fn list_secrets_returns_all_providers() {
        let svc = SecretService::new();
        let list = svc.list_secrets().await;
        assert_eq!(list.len(), PROVIDERS.len());
        assert!(list.iter().any(|p| p.provider == "anthropic"));
        assert!(list.iter().any(|p| p.provider == "openai"));
    }
}
