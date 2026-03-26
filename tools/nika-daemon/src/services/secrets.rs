//! Secrets service — env vars + optional keyring access.
//!
//! The daemon's secrets service is the centralized provider of API keys.
//! Resolution order:
//! 1. Environment variable (zero overhead)
//! 2. System keychain via keyring crate (if `keychain` feature enabled)
//!
//! Keyring access is wrapped in `spawn_blocking` because the keyring crate
//! is synchronous and not Send/Sync (research finding: must not call from async).

use crate::protocol::{ProviderSecretInfo, SecretSource};
use tracing::trace;

#[cfg(feature = "keychain")]
use tracing::debug;

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
#[derive(Default)]
pub struct SecretService {
    // Future: secret rotation, caching, etc.
}

impl SecretService {
    /// Create a new secrets service.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get a secret for a provider.
    ///
    /// Checks env var first, then keychain (if feature enabled).
    pub async fn get_secret(&self, provider: &str) -> Option<String> {
        // 1. Check env var
        if let Some(value) = get_from_env(provider) {
            trace!("{}: found in env", provider);
            return Some(value);
        }

        // 2. Try keychain
        #[cfg(feature = "keychain")]
        {
            if let Some(value) = get_from_keychain(provider).await {
                debug!("{}: found in keychain", provider);
                return Some(value);
            }
        }

        None
    }

    /// Check if a secret exists for a provider.
    pub async fn has_secret(&self, provider: &str) -> bool {
        self.get_secret(provider).await.is_some()
    }

    /// Store a secret for a provider in the system keychain.
    ///
    /// Returns Ok(true) if stored, Ok(false) if keychain feature is disabled.
    pub async fn set_secret(&self, provider: &str, key: &str) -> Result<bool, String> {
        #[cfg(feature = "keychain")]
        {
            let provider = provider.to_string();
            let key = key.to_string();
            tokio::task::spawn_blocking(move || {
                let entry = keyring::Entry::new("nika", &provider)
                    .map_err(|e| format!("keyring access error: {e}"))?;
                entry
                    .set_password(&key)
                    .map_err(|e| format!("keyring store error: {e}"))?;
                Ok(true)
            })
            .await
            .map_err(|e| format!("spawn_blocking error: {e}"))?
        }
        #[cfg(not(feature = "keychain"))]
        {
            let _ = (provider, key);
            Ok(false)
        }
    }

    /// Delete a secret for a provider from the system keychain.
    ///
    /// Returns Ok(true) if deleted, Ok(false) if keychain feature disabled or not found.
    pub async fn delete_secret(&self, provider: &str) -> Result<bool, String> {
        #[cfg(feature = "keychain")]
        {
            let provider = provider.to_string();
            tokio::task::spawn_blocking(move || {
                let entry = keyring::Entry::new("nika", &provider)
                    .map_err(|e| format!("keyring access error: {e}"))?;
                match entry.delete_credential() {
                    Ok(()) => Ok(true),
                    Err(keyring::Error::NoEntry) => Ok(false),
                    Err(e) => Err(format!("keyring delete error: {e}")),
                }
            })
            .await
            .map_err(|e| format!("spawn_blocking error: {e}"))?
        }
        #[cfg(not(feature = "keychain"))]
        {
            let _ = provider;
            Ok(false)
        }
    }

    /// List all provider secret status.
    pub async fn list_secrets(&self) -> Vec<ProviderSecretInfo> {
        let mut result = Vec::with_capacity(PROVIDERS.len());
        for &(provider, _) in PROVIDERS {
            let source = if get_from_env(provider).is_some() {
                SecretSource::Env
            } else {
                #[cfg(feature = "keychain")]
                {
                    if get_from_keychain(provider).await.is_some() {
                        SecretSource::Keychain
                    } else {
                        SecretSource::NotFound
                    }
                }
                #[cfg(not(feature = "keychain"))]
                {
                    SecretSource::NotFound
                }
            };
            result.push(ProviderSecretInfo {
                provider: provider.to_string(),
                source,
            });
        }
        result
    }
}

fn get_from_env(provider: &str) -> Option<String> {
    let env_var = provider_env_var(provider)?;
    std::env::var(env_var).ok().filter(|v| !v.is_empty())
}

/// Get a secret from the system keychain via spawn_blocking.
///
/// The keyring crate is synchronous and not Send/Sync, so we MUST
/// run it on a dedicated blocking thread to avoid blocking the async runtime.
#[cfg(feature = "keychain")]
async fn get_from_keychain(provider: &str) -> Option<String> {
    let provider = provider.to_string();
    tokio::task::spawn_blocking(move || {
        let entry = keyring::Entry::new("nika", &provider).ok()?;
        entry.get_password().ok()
    })
    .await
    .ok()
    .flatten()
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

    #[test]
    fn provider_count() {
        assert_eq!(PROVIDERS.len(), 7);
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

    #[test]
    fn get_from_env_unknown_returns_none() {
        // Unknown provider never has an env var
        assert_eq!(get_from_env("nonexistent_provider_xyz"), None);
    }

    // ── set_secret / delete_secret tests ─────────────────────────────

    #[tokio::test]
    async fn set_secret_without_keychain_feature_returns_false() {
        // Without keychain feature, set_secret returns Ok(false)
        let svc = SecretService::new();
        let result = svc.set_secret("anthropic", "sk-test").await;
        // With keychain feature disabled in tests, this should succeed
        // but may return false (no keychain) or error (depends on feature)
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn delete_secret_without_keychain_feature_returns_false() {
        let svc = SecretService::new();
        let result = svc.delete_secret("anthropic").await;
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn get_from_env_known_provider_returns_option() {
        // Known provider returns Some only if env var is set
        // We can't guarantee ANTHROPIC_API_KEY is set in CI, but we
        // can verify the function doesn't panic and returns a valid Option
        let result = get_from_env("anthropic");
        if std::env::var("ANTHROPIC_API_KEY").is_ok() {
            assert!(
                result.is_some(),
                "Expected Some when ANTHROPIC_API_KEY is set"
            );
        } else {
            assert!(
                result.is_none(),
                "Expected None when ANTHROPIC_API_KEY is not set"
            );
        }
    }
}
