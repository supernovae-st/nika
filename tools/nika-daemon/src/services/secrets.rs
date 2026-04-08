// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Secrets service — env vars + NikaVault encrypted store.
//!
//! The daemon's secrets service is the centralized provider of API keys.
//! Resolution order:
//! 1. Environment variable (zero overhead)
//! 2. NikaVault encrypted file store (~/.nika/secrets/vault.enc)
//!
//! All secret storage uses NikaVault encrypted file store.

use crate::protocol::{ProviderSecretInfo, SecretSource};
use nika_vault::NikaVault;
use secrecy::ExposeSecret;
use tracing::{debug, trace};

/// Known LLM providers and their environment variable names.
pub const PROVIDERS: &[(&str, &str)] = &[
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
    vault: NikaVault,
}

impl Default for SecretService {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretService {
    /// Create a new secrets service with vault at `~/.nika/secrets/`.
    pub fn new() -> Self {
        let secrets_dir = crate::daemon_dir().join("secrets");
        Self {
            vault: NikaVault::new(&secrets_dir),
        }
    }

    /// Get a secret for a provider.
    ///
    /// Resolution: env var → NikaVault → None
    pub async fn get_secret(&self, provider: &str) -> Option<String> {
        // 1. Check env var (zero overhead)
        if let Some(value) = get_from_env(provider) {
            trace!("{}: found in env", provider);
            return Some(value);
        }

        // 2. Try encrypted vault
        match self.vault.get(provider) {
            Ok(Some(secret)) => {
                debug!("{}: found in vault", provider);
                return Some(secret.expose_secret().to_string());
            }
            Ok(None) => {}
            Err(e) => {
                debug!("{}: vault read error: {}", provider, e);
            }
        }

        None
    }

    /// Check if a secret exists for a provider.
    pub async fn has_secret(&self, provider: &str) -> bool {
        self.get_secret(provider).await.is_some()
    }

    /// Store a secret for a provider in the encrypted vault.
    pub async fn set_secret(&self, provider: &str, key: &str) -> Result<bool, String> {
        self.vault
            .set(provider, key)
            .map_err(|e| format!("vault store error: {e}"))?;
        Ok(true)
    }

    /// Delete a secret for a provider from the encrypted vault.
    pub async fn delete_secret(&self, provider: &str) -> Result<bool, String> {
        self.vault
            .delete(provider)
            .map_err(|e| format!("vault delete error: {e}"))
    }

    /// List all provider secret status.
    pub async fn list_secrets(&self) -> Vec<ProviderSecretInfo> {
        let vault_providers: Vec<String> = self.vault.list().unwrap_or_default();
        let mut result = Vec::with_capacity(PROVIDERS.len());
        for &(provider, _) in PROVIDERS {
            let source = if get_from_env(provider).is_some() {
                SecretSource::Env
            } else if vault_providers.iter().any(|p| p == provider) {
                SecretSource::Vault
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

fn get_from_env(provider: &str) -> Option<String> {
    let env_var = provider_env_var(provider)?;
    std::env::var(env_var).ok().filter(|v| !v.is_empty())
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

    /// Create a SecretService with vault pointing at a tempdir.
    fn make_test_service() -> (tempfile::TempDir, SecretService) {
        let dir = tempfile::TempDir::new().unwrap();
        let svc = SecretService {
            vault: NikaVault::new(dir.path()),
        };
        (dir, svc)
    }

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

    // Test-only fake API key — not a real secret, never leaves the test vault.
    const TEST_FAKE_API_KEY: &str = "sk-test-fixture-not-real";

    #[tokio::test]
    #[serial]
    async fn set_secret_via_vault() {
        std::env::set_var("NIKA_VAULT_PASSPHRASE", "test-daemon");
        let (_dir, svc) = make_test_service();
        let result = svc.set_secret("anthropic", TEST_FAKE_API_KEY).await;
        assert!(
            result.is_ok(),
            "set_secret should succeed via vault, got: {result:?}"
        );
        assert!(result.unwrap(), "should return true");
        // Verify get returns it (clear env to force vault lookup)
        let orig = std::env::var("ANTHROPIC_API_KEY").ok();
        unsafe { std::env::remove_var("ANTHROPIC_API_KEY") };
        let secret = svc.get_secret("anthropic").await;
        assert_eq!(secret, Some(TEST_FAKE_API_KEY.to_string()));
        if let Some(v) = orig {
            std::env::set_var("ANTHROPIC_API_KEY", v);
        }
    }

    #[tokio::test]
    #[serial]
    async fn delete_secret_via_vault() {
        std::env::set_var("NIKA_VAULT_PASSPHRASE", "test-daemon");
        let (_dir, svc) = make_test_service();
        svc.set_secret("anthropic", TEST_FAKE_API_KEY)
            .await
            .unwrap();
        let result = svc.delete_secret("anthropic").await;
        assert!(
            result.is_ok(),
            "delete_secret should succeed via vault, got: {result:?}"
        );
        assert!(result.unwrap(), "should return true when deleted");
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
