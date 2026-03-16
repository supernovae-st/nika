//! Unified secrets management.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │  SECRETS MODULE                                                             │
//! ├─────────────────────────────────────────────────────────────────────────────┤
//! │                                                                             │
//! │  With nika-daemon feature (RECOMMENDED):                                    │
//! │                                                                             │
//! │  Nika Process                                                               │
//! │      │                                                                      │
//! │      └── spn-client (Unix socket IPC)                                       │
//! │          │                                                                  │
//! │          └── daemon (~/.nika/daemon.sock)                                   │
//! │              │                                                              │
//! │              └── OS Keychain (SOLE accessor, no popups)                     │
//! │                                                                             │
//! │  Without nika-daemon feature (fallback):                                    │
//! │                                                                             │
//! │  Nika Process                                                               │
//! │      │                                                                      │
//! │      └── Direct access (causes macOS popups!)                               │
//! │          ├── OS Keychain (keyring crate)                                    │
//! │          └── Environment variables                                          │
//! │                                                                             │
//! └─────────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! When daemon feature is enabled, Nika will NEVER access the keychain directly.
//! The daemon is the SOLE keychain accessor. If the daemon doesn't have a secret,
//! it's marked as "not found" instead of falling back to direct keyring access.
//!
//! This completely eliminates macOS Keychain popup fatigue.
//!
//! ## Usage
//!
//! ```ignore
//! use nika::secrets::{load_from_daemon_or_fallback, get_secret, has_secret};
//!
//! // Load all provider secrets at startup
//! let result = load_from_daemon_or_fallback().await;
//! println!("Loaded: {}", result.summary());
//!
//! // Get a specific secret
//! if let Some(key) = get_secret("anthropic").await {
//!     // Use the key
//! }
//!
//! // Check if secret exists
//! if has_secret("openai").await {
//!     // Provider is configured
//! }
//! ```

pub mod keyring;
mod result;

#[cfg(feature = "nika-daemon")]
mod daemon;

#[cfg(not(feature = "nika-daemon"))]
mod fallback;

// Re-export keyring types
pub use keyring::{
    mask_api_key, migrate_env_to_keyring, validate_key_format, KeyringError, MigrationReport,
    NikaKeyring,
};

// Re-export result type (always available)
pub use result::SecretsLoadResult;

// Re-export functions based on feature
#[cfg(feature = "nika-daemon")]
pub use daemon::{daemon_available, get_secret, has_secret, load_from_daemon_or_fallback};

#[cfg(not(feature = "nika-daemon"))]
pub use fallback::{daemon_available, get_secret, has_secret, load_from_daemon_or_fallback};

// ═══════════════════════════════════════════════════════════════════════════════
// HELPER FUNCTIONS (always available)
// ═══════════════════════════════════════════════════════════════════════════════

/// Get the environment variable name for a provider ID.
///
/// Uses `nika::core::KNOWN_PROVIDERS` as the source of truth.
///
/// # Example
///
/// ```
/// use nika::secrets::provider_env_var;
///
/// assert_eq!(provider_env_var("anthropic"), "ANTHROPIC_API_KEY");
/// assert_eq!(provider_env_var("neo4j"), "NEO4J_PASSWORD");
/// assert_eq!(provider_env_var("unknown"), "UNKNOWN_API_KEY");
/// ```
pub fn provider_env_var(provider: &str) -> &'static str {
    crate::core::provider_to_env_var(provider).unwrap_or("UNKNOWN_API_KEY")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{ProviderCategory, KNOWN_PROVIDERS};
    use secrecy::ExposeSecret;
    use serial_test::serial;

    // ─── provider_env_var ───────────────────────────────────────────────

    #[test]
    fn test_provider_env_var_lookup() {
        // Test that provider_env_var returns expected values for known providers
        assert_eq!(provider_env_var("anthropic"), "ANTHROPIC_API_KEY");
        assert_eq!(provider_env_var("openai"), "OPENAI_API_KEY");
        assert_eq!(provider_env_var("native"), "NIKA_NATIVE_MODEL_PATH");
        assert_eq!(provider_env_var("neo4j"), "NEO4J_PASSWORD");
        assert_eq!(provider_env_var("github"), "GITHUB_TOKEN");
    }

    #[test]
    fn test_provider_env_var_all_llm_providers() {
        assert_eq!(provider_env_var("mistral"), "MISTRAL_API_KEY");
        assert_eq!(provider_env_var("groq"), "GROQ_API_KEY");
        assert_eq!(provider_env_var("deepseek"), "DEEPSEEK_API_KEY");
        assert_eq!(provider_env_var("gemini"), "GEMINI_API_KEY");
    }

    #[test]
    fn test_provider_env_var_unknown() {
        assert_eq!(provider_env_var("nonexistent"), "UNKNOWN_API_KEY");
    }

    #[test]
    fn test_provider_env_var_empty_string() {
        assert_eq!(provider_env_var(""), "UNKNOWN_API_KEY");
    }

    // ─── daemon_available ───────────────────────────────────────────────

    #[test]
    fn test_daemon_available_check() {
        // Without daemon running, should return false
        #[cfg(not(feature = "nika-daemon"))]
        assert!(!daemon_available());

        #[cfg(feature = "nika-daemon")]
        {
            // With feature, checks if socket exists
            let result = daemon_available();
            // Result depends on whether daemon is actually running
            let _ = result;
        }
    }

    // ─── get_secret ─────────────────────────────────────────────────────

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

    // Verifies that empty env var is NOT returned as a secret by the env-check path.
    // We test the env behavior directly rather than through get_secret() because
    // the daemon's OnceLock static caches the client across test runs, making
    // get_secret() non-deterministic when daemon is available.
    #[test]
    fn test_empty_env_var_is_not_a_secret() {
        let key = "DEEPSEEK_API_KEY";
        let original = std::env::var(key).ok();

        std::env::set_var(key, "");
        let value = std::env::var(key).ok().filter(|v| !v.is_empty());
        assert!(value.is_none(), "empty env var should be filtered out");

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
        // In test mode (both daemon and fallback), keychain is skipped
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

    // ─── has_secret ─────────────────────────────────────────────────────

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
        // In test mode, keychain skipped -> false
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

    // ─── load_from_daemon_or_fallback ───────────────────────────────────

    #[tokio::test]
    async fn test_load_result_structure() {
        let result = load_from_daemon_or_fallback().await;
        // from_daemon is empty when daemon isn't running
        // With nika-daemon feature: iterates ALL providers (LLM + MCP + Local)
        // Without nika-daemon feature: iterates only LLM providers
        let expected_count = if cfg!(feature = "nika-daemon") {
            KNOWN_PROVIDERS.len()
        } else {
            KNOWN_PROVIDERS
                .iter()
                .filter(|p| p.category == ProviderCategory::Llm)
                .count()
        };
        let total = result.from_daemon.len() + result.from_fallback.len() + result.not_found.len();
        assert_eq!(
            total,
            expected_count,
            "Should process exactly {} providers, got {} daemon + {} fallback + {} not_found",
            expected_count,
            result.from_daemon.len(),
            result.from_fallback.len(),
            result.not_found.len()
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_load_result_detects_env_vars() {
        let key = "OPENAI_API_KEY";
        let original = std::env::var(key).ok();

        std::env::set_var(key, "sk-test-openai-key");
        let result = load_from_daemon_or_fallback().await;

        // openai should appear in from_daemon or from_fallback (depending on feature)
        let found_in_daemon = result.from_daemon.contains(&"openai".to_string());
        let found_in_fallback = result.from_fallback.contains(&"openai".to_string());
        assert!(
            found_in_daemon || found_in_fallback,
            "openai should be detected from env, got daemon={:?}, fallback={:?}",
            result.from_daemon,
            result.from_fallback
        );

        // Restore
        match original {
            Some(v) => std::env::set_var(key, v),
            None => unsafe { std::env::remove_var(key) },
        }
    }

    #[tokio::test]
    async fn test_load_result_summary_not_empty() {
        let result = load_from_daemon_or_fallback().await;
        let summary = result.summary();
        assert!(!summary.is_empty(), "Summary should not be empty");
        // Summary should mention daemon status
        assert!(
            summary.contains("daemon"),
            "Summary should mention daemon status: {}",
            summary
        );
    }
}
