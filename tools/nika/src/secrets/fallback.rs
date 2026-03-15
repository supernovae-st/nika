//! Fallback secrets management (when nika-daemon feature is NOT enabled).
//!
//! Uses direct keyring access and environment variables.
//! This module is only compiled when nika-daemon is not enabled.

use crate::core::{ProviderCategory, KNOWN_PROVIDERS};
use crate::secrets::keyring::{should_skip_keychain, NikaKeyring};
use crate::secrets::result::SecretsLoadResult;
use secrecy::SecretString;
use tracing::{debug, info, trace};

/// Check if daemon is available (always false without feature).
pub fn daemon_available() -> bool {
    false
}

/// Load secrets from keyring/env (no daemon).
pub async fn load_from_daemon_or_fallback() -> SecretsLoadResult {
    let mut result = SecretsLoadResult {
        daemon_available: false,
        ..Default::default()
    };

    // Iterate LLM providers only (7) when daemon not available
    // MCP and Local providers typically don't need secrets loaded this way
    for provider in KNOWN_PROVIDERS
        .iter()
        .filter(|p| p.category == ProviderCategory::Llm)
    {
        let provider_id = provider.id;
        let env_var = provider.env_var;

        // Check if already in env
        if std::env::var(env_var).is_ok() {
            trace!("{}: already in env", provider_id);
            result.from_fallback.push(provider_id.to_string());
            continue;
        }

        if try_load_from_fallback(provider_id, env_var) {
            result.from_fallback.push(provider_id.to_string());
        } else {
            result.not_found.push(provider_id.to_string());
        }
    }

    info!("Secrets: {}", result.summary());
    result
}

/// Try loading from keyring and inject into env if found.
///
/// During tests (`cfg(test)`), always returns false to avoid macOS Keychain popups.
fn try_load_from_fallback(provider: &str, env_var: &str) -> bool {
    // Never access real keychain during tests — prevents macOS popup storms
    if cfg!(test) || should_skip_keychain() {
        trace!(
            "{}: keychain skipped (test mode or NIKA_SKIP_KEYCHAIN)",
            provider
        );
        return false;
    }

    match NikaKeyring::get(provider) {
        Ok(secret) => {
            std::env::set_var(env_var, &*secret);
            debug!("{}: loaded from keyring → {}", provider, env_var);
            true
        }
        Err(_) => {
            trace!("{}: not in keyring", provider);
            false
        }
    }
}

/// Get a secret for a provider.
pub async fn get_secret(provider: &str) -> Option<SecretString> {
    let env_var = provider_env_var(provider);

    // Check env first (may have been loaded at boot)
    if let Ok(value) = std::env::var(env_var) {
        if !value.is_empty() {
            return Some(SecretString::from(value));
        }
    }

    // Skip keychain in tests to prevent macOS popup storms
    if cfg!(test) || should_skip_keychain() {
        return None;
    }

    // Fall back to keyring
    NikaKeyring::get_secret(provider).ok()
}

/// Check if a secret exists for a provider.
pub async fn has_secret(provider: &str) -> bool {
    let env_var = provider_env_var(provider);

    // Check env first
    if std::env::var(env_var).is_ok() {
        return true;
    }

    // Skip keychain in tests to prevent macOS popup storms
    if cfg!(test) || should_skip_keychain() {
        return false;
    }

    // Fall back to keyring
    NikaKeyring::exists(provider)
}

/// Get the environment variable name for a provider ID.
fn provider_env_var(provider: &str) -> &'static str {
    // Use KNOWN_PROVIDERS as source of truth
    crate::core::provider_to_env_var(provider).unwrap_or("UNKNOWN_API_KEY")
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::ExposeSecret;
    use serial_test::serial;

    // ─── daemon_available ─────────────────────────────────────────────────

    #[test]
    fn test_daemon_available_always_false() {
        // Without nika-daemon feature, daemon is never available
        assert!(!daemon_available());
    }

    // ─── provider_env_var (private helper) ────────────────────────────────

    #[test]
    fn test_provider_env_var_known_providers() {
        assert_eq!(provider_env_var("anthropic"), "ANTHROPIC_API_KEY");
        assert_eq!(provider_env_var("openai"), "OPENAI_API_KEY");
        assert_eq!(provider_env_var("mistral"), "MISTRAL_API_KEY");
        assert_eq!(provider_env_var("groq"), "GROQ_API_KEY");
        assert_eq!(provider_env_var("deepseek"), "DEEPSEEK_API_KEY");
        assert_eq!(provider_env_var("gemini"), "GEMINI_API_KEY");
    }

    #[test]
    fn test_provider_env_var_unknown_returns_fallback() {
        assert_eq!(provider_env_var("nonexistent"), "UNKNOWN_API_KEY");
        assert_eq!(provider_env_var(""), "UNKNOWN_API_KEY");
    }

    // ─── try_load_from_fallback ───────────────────────────────────────────

    #[test]
    fn test_try_load_from_fallback_skips_keychain_in_test_mode() {
        // cfg!(test) is true, so keychain is always skipped
        let result = try_load_from_fallback("anthropic", "ANTHROPIC_API_KEY");
        assert!(!result);
    }

    // ─── get_secret ───────────────────────────────────────────────────────

    #[tokio::test]
    #[serial]
    async fn test_get_secret_returns_env_var_value() {
        let key = "ANTHROPIC_API_KEY";
        let original = std::env::var(key).ok();

        std::env::set_var(key, "sk-ant-test-key-12345");
        let secret = get_secret("anthropic").await;
        assert!(secret.is_some());
        assert_eq!(secret.unwrap().expose_secret(), "sk-ant-test-key-12345");

        // Restore
        match original {
            Some(v) => std::env::set_var(key, v),
            None => unsafe { std::env::remove_var(key) },
        }
    }

    #[tokio::test]
    #[serial]
    async fn test_get_secret_returns_none_when_env_empty() {
        let key = "DEEPSEEK_API_KEY";
        let original = std::env::var(key).ok();

        std::env::set_var(key, "");
        let secret = get_secret("deepseek").await;
        // Empty string is treated as not set
        assert!(secret.is_none());

        // Restore
        match original {
            Some(v) => std::env::set_var(key, v),
            None => unsafe { std::env::remove_var(key) },
        }
    }

    #[tokio::test]
    #[serial]
    async fn test_get_secret_returns_none_when_env_unset() {
        let key = "GROQ_API_KEY";
        let original = std::env::var(key).ok();

        unsafe { std::env::remove_var(key) };
        // In test mode, keychain is skipped, so returns None
        let secret = get_secret("groq").await;
        assert!(secret.is_none());

        // Restore
        if let Some(v) = original {
            std::env::set_var(key, v);
        }
    }

    #[tokio::test]
    async fn test_get_secret_unknown_provider_returns_none() {
        // Unknown provider maps to UNKNOWN_API_KEY which won't be set
        let secret = get_secret("nonexistent_provider").await;
        assert!(secret.is_none());
    }

    // ─── has_secret ───────────────────────────────────────────────────────

    #[tokio::test]
    #[serial]
    async fn test_has_secret_true_when_env_set() {
        let key = "MISTRAL_API_KEY";
        let original = std::env::var(key).ok();

        std::env::set_var(key, "test-key");
        assert!(has_secret("mistral").await);

        // Restore
        match original {
            Some(v) => std::env::set_var(key, v),
            None => unsafe { std::env::remove_var(key) },
        }
    }

    #[tokio::test]
    #[serial]
    async fn test_has_secret_false_when_env_unset() {
        let key = "GEMINI_API_KEY";
        let original = std::env::var(key).ok();

        unsafe { std::env::remove_var(key) };
        // In test mode, keychain skipped → false
        assert!(!has_secret("gemini").await);

        // Restore
        if let Some(v) = original {
            std::env::set_var(key, v);
        }
    }

    #[tokio::test]
    async fn test_has_secret_unknown_provider_false() {
        assert!(!has_secret("fantasy_provider").await);
    }

    // ─── load_from_daemon_or_fallback ─────────────────────────────────────

    #[tokio::test]
    async fn test_load_result_daemon_unavailable() {
        let result = load_from_daemon_or_fallback().await;
        assert!(!result.daemon_available);
        assert!(result.from_daemon.is_empty());
    }

    #[tokio::test]
    async fn test_load_result_only_processes_llm_providers() {
        let result = load_from_daemon_or_fallback().await;
        // The combined count should equal the number of LLM providers
        let llm_count = KNOWN_PROVIDERS
            .iter()
            .filter(|p| p.category == ProviderCategory::Llm)
            .count();
        let total = result.from_fallback.len() + result.not_found.len();
        assert_eq!(
            total, llm_count,
            "Should process exactly {} LLM providers, got {} fallback + {} not_found",
            llm_count, result.from_fallback.len(), result.not_found.len()
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_load_result_detects_env_vars() {
        let key = "OPENAI_API_KEY";
        let original = std::env::var(key).ok();

        std::env::set_var(key, "sk-test-openai-key");
        let result = load_from_daemon_or_fallback().await;

        // openai should be in from_fallback (found in env)
        assert!(
            result.from_fallback.contains(&"openai".to_string()),
            "openai should be detected from env, got: {:?}",
            result.from_fallback
        );

        // Restore
        match original {
            Some(v) => std::env::set_var(key, v),
            None => unsafe { std::env::remove_var(key) },
        }
    }

    #[tokio::test]
    async fn test_load_result_summary_format() {
        let result = load_from_daemon_or_fallback().await;
        let summary = result.summary();
        assert!(
            summary.contains("daemon unavailable"),
            "Summary should indicate daemon unavailable: {}",
            summary
        );
    }
}
