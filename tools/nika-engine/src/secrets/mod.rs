//! Secrets management -- env vars + daemon IPC + NikaVault.

mod fallback;
pub mod key_utils;
mod result;
pub mod store;
pub mod vault;

pub use fallback::{daemon_available, get_secret, has_secret, load_from_daemon_or_fallback};
pub use key_utils::{
    mask_api_key, migrate_env_to_vault, validate_key_format, KeyringError, MigrationReport,
};
pub use result::SecretsLoadResult;
pub use store::resolve_env;

/// Get the environment variable name for a provider ID.
pub fn provider_env_var(provider: &str) -> &'static str {
    crate::core::provider_to_env_var(provider).unwrap_or("UNKNOWN_API_KEY")
}

/// Inject a secret into the in-process SecretStore.
///
/// Thread-safe: backed by a `DashMap`, callable from any context including
/// `nika serve` worker threads and Tokio task pools.
pub fn inject_secret_to_env(env_var: &str, value: &str) {
    store::set_secret(env_var, value);
}

/// Check if a provider's API key is available (store + env).
///
/// Drop-in replacement for `Provider::has_env_key()` that also checks
/// the in-process SecretStore (populated from daemon/vault at boot).
pub fn has_provider_key(provider: &crate::core::Provider) -> bool {
    store::resolve_env(provider.env_var).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{ProviderCategory, KNOWN_PROVIDERS};
    use secrecy::ExposeSecret;
    use serial_test::serial;
    #[test]
    fn test_provider_env_var_lookup() {
        assert_eq!(provider_env_var("anthropic"), "ANTHROPIC_API_KEY");
        assert_eq!(provider_env_var("openai"), "OPENAI_API_KEY");
    }
    #[test]
    fn test_provider_env_var_unknown() {
        assert_eq!(provider_env_var("nonexistent"), "UNKNOWN_API_KEY");
    }
    #[test]
    #[serial]
    fn test_daemon_available_check() {
        // Isolate from real daemon by pointing NIKA_HOME to a tempdir
        let dir = tempfile::tempdir().unwrap();
        let orig = std::env::var("NIKA_HOME").ok();
        std::env::set_var("NIKA_HOME", dir.path());

        assert!(
            !daemon_available(),
            "No daemon socket should exist in isolated tempdir"
        );

        match orig {
            Some(v) => std::env::set_var("NIKA_HOME", v),
            None => unsafe { std::env::remove_var("NIKA_HOME") },
        }
    }
    #[tokio::test]
    #[serial]
    async fn test_get_secret_returns_env_var_value() {
        let key = "ANTHROPIC_API_KEY";
        let orig = std::env::var(key).ok();
        std::env::set_var(key, "sk-ant-test");
        let s = get_secret("anthropic").await;
        assert!(s.is_some());
        assert_eq!(s.unwrap().expose_secret(), "sk-ant-test");
        match orig {
            Some(v) => std::env::set_var(key, v),
            None => unsafe { std::env::remove_var(key) },
        }
    }
    #[tokio::test]
    #[serial]
    async fn test_has_secret_true_when_env_set() {
        let key = "MISTRAL_API_KEY";
        let orig = std::env::var(key).ok();
        std::env::set_var(key, "test");
        assert!(has_secret("mistral").await);
        match orig {
            Some(v) => std::env::set_var(key, v),
            None => unsafe { std::env::remove_var(key) },
        }
    }
    #[tokio::test]
    #[serial]
    async fn test_has_secret_false_when_env_empty() {
        let key = "XAI_API_KEY";
        let orig = std::env::var(key).ok();
        let orig_no_daemon = std::env::var("NIKA_NO_DAEMON").ok();
        std::env::set_var(key, "");
        // Also clear the in-process store (may have been populated by prior tests)
        store::remove_secret(key);
        // Prevent fallback to daemon/vault which may have the real key
        std::env::set_var("NIKA_NO_DAEMON", "1");
        assert!(!has_secret("xai").await);
        match orig {
            Some(v) => std::env::set_var(key, v),
            None => unsafe { std::env::remove_var(key) },
        }
        match orig_no_daemon {
            Some(v) => std::env::set_var("NIKA_NO_DAEMON", v),
            None => unsafe { std::env::remove_var("NIKA_NO_DAEMON") },
        }
    }
    #[tokio::test]
    async fn test_load_result_structure() {
        let r = load_from_daemon_or_fallback().await;
        let expected = KNOWN_PROVIDERS
            .iter()
            .filter(|p| p.category == ProviderCategory::Llm)
            .count();
        let total = r.from_env.len() + r.not_found.len();
        assert_eq!(total, expected);
    }
    #[tokio::test]
    async fn test_load_result_summary_not_empty() {
        let r = load_from_daemon_or_fallback().await;
        assert!(!r.summary().is_empty());
    }

    #[tokio::test]
    #[serial]
    async fn test_daemon_not_started_if_nika_no_daemon() {
        let dir = tempfile::tempdir().unwrap();
        let orig_home = std::env::var("NIKA_HOME").ok();
        let orig_no_daemon = std::env::var("NIKA_NO_DAEMON").ok();

        std::env::set_var("NIKA_HOME", dir.path());
        std::env::set_var("NIKA_NO_DAEMON", "1");

        let _result = load_from_daemon_or_fallback().await;

        // No daemon socket should have been created in the tempdir
        let socket_exists = dir.path().read_dir().unwrap().any(|entry| {
            entry
                .ok()
                .map(|e| e.file_name().to_string_lossy().contains("sock"))
                .unwrap_or(false)
        });
        assert!(
            !socket_exists,
            "NIKA_NO_DAEMON=1 should prevent daemon auto-start"
        );

        match orig_home {
            Some(v) => std::env::set_var("NIKA_HOME", v),
            None => unsafe { std::env::remove_var("NIKA_HOME") },
        }
        match orig_no_daemon {
            Some(v) => std::env::set_var("NIKA_NO_DAEMON", v),
            None => unsafe { std::env::remove_var("NIKA_NO_DAEMON") },
        }
    }

    #[tokio::test]
    #[serial]
    async fn test_secrets_from_env_without_daemon() {
        let dir = tempfile::tempdir().unwrap();
        let orig_home = std::env::var("NIKA_HOME").ok();
        let orig_no_daemon = std::env::var("NIKA_NO_DAEMON").ok();
        let orig_key = std::env::var("ANTHROPIC_API_KEY").ok();

        std::env::set_var("NIKA_HOME", dir.path());
        std::env::set_var("NIKA_NO_DAEMON", "1");
        std::env::set_var("ANTHROPIC_API_KEY", "test-key-123");

        let secret = get_secret("anthropic").await;
        assert!(
            secret.is_some(),
            "get_secret should return Some when env var is set"
        );
        assert_eq!(
            secret.unwrap().expose_secret(),
            "test-key-123",
            "get_secret should return the env var value"
        );

        let exists = has_secret("anthropic").await;
        assert!(exists, "has_secret should return true when env var is set");

        match orig_home {
            Some(v) => std::env::set_var("NIKA_HOME", v),
            None => unsafe { std::env::remove_var("NIKA_HOME") },
        }
        match orig_no_daemon {
            Some(v) => std::env::set_var("NIKA_NO_DAEMON", v),
            None => unsafe { std::env::remove_var("NIKA_NO_DAEMON") },
        }
        match orig_key {
            Some(v) => std::env::set_var("ANTHROPIC_API_KEY", v),
            None => unsafe { std::env::remove_var("ANTHROPIC_API_KEY") },
        }
    }

    #[tokio::test]
    #[serial]
    async fn test_secrets_fallback_clear_error_when_no_source() {
        let dir = tempfile::tempdir().unwrap();
        let orig_home = std::env::var("NIKA_HOME").ok();
        let orig_no_daemon = std::env::var("NIKA_NO_DAEMON").ok();
        let orig_key = std::env::var("ANTHROPIC_API_KEY").ok();

        std::env::set_var("NIKA_HOME", dir.path());
        std::env::set_var("NIKA_NO_DAEMON", "1");
        // Ensure the key is fully unset, not just empty
        unsafe { std::env::remove_var("ANTHROPIC_API_KEY") };

        let exists = has_secret("anthropic").await;
        assert!(
            !exists,
            "has_secret should return false when no env var and no daemon"
        );

        let secret = get_secret("anthropic").await;
        assert!(
            secret.is_none(),
            "get_secret should return None when no env var and no daemon"
        );

        match orig_home {
            Some(v) => std::env::set_var("NIKA_HOME", v),
            None => unsafe { std::env::remove_var("NIKA_HOME") },
        }
        match orig_no_daemon {
            Some(v) => std::env::set_var("NIKA_NO_DAEMON", v),
            None => unsafe { std::env::remove_var("NIKA_NO_DAEMON") },
        }
        match orig_key {
            Some(v) => std::env::set_var("ANTHROPIC_API_KEY", v),
            None => unsafe { std::env::remove_var("ANTHROPIC_API_KEY") },
        }
    }

    #[test]
    #[serial]
    fn test_doppler_backend_selected_from_env() {
        use nika_vault::VaultBackend;
        std::env::set_var("NIKA_VAULT_BACKEND", "doppler");
        assert_eq!(VaultBackend::from_env(), VaultBackend::Doppler);
        unsafe { std::env::remove_var("NIKA_VAULT_BACKEND") };
    }

    #[test]
    #[serial]
    fn test_local_backend_is_default() {
        use nika_vault::VaultBackend;
        unsafe { std::env::remove_var("NIKA_VAULT_BACKEND") };
        assert_eq!(VaultBackend::from_env(), VaultBackend::Local);
    }

    #[tokio::test]
    #[serial]
    async fn test_doppler_fallback_to_local_on_error() {
        // When NIKA_VAULT_BACKEND=doppler but doppler CLI is not installed,
        // the system should fall through to the local vault without erroring.
        let dir = tempfile::tempdir().unwrap();
        let orig_home = std::env::var("NIKA_HOME").ok();
        let orig_no_daemon = std::env::var("NIKA_NO_DAEMON").ok();
        let orig_backend = std::env::var("NIKA_VAULT_BACKEND").ok();
        let orig_key = std::env::var("ANTHROPIC_API_KEY").ok();

        std::env::set_var("NIKA_HOME", dir.path());
        std::env::set_var("NIKA_NO_DAEMON", "1");
        std::env::set_var("NIKA_VAULT_BACKEND", "doppler");
        unsafe { std::env::remove_var("ANTHROPIC_API_KEY") };

        // has_secret should not panic or error — it should gracefully return false
        let exists = has_secret("anthropic").await;
        assert!(
            !exists,
            "has_secret should return false when doppler unavailable and no local vault"
        );

        // get_secret should return None gracefully
        let secret = get_secret("anthropic").await;
        assert!(
            secret.is_none(),
            "get_secret should return None when doppler unavailable and no local vault"
        );

        match orig_home {
            Some(v) => std::env::set_var("NIKA_HOME", v),
            None => unsafe { std::env::remove_var("NIKA_HOME") },
        }
        match orig_no_daemon {
            Some(v) => std::env::set_var("NIKA_NO_DAEMON", v),
            None => unsafe { std::env::remove_var("NIKA_NO_DAEMON") },
        }
        match orig_backend {
            Some(v) => std::env::set_var("NIKA_VAULT_BACKEND", v),
            None => unsafe { std::env::remove_var("NIKA_VAULT_BACKEND") },
        }
        match orig_key {
            Some(v) => std::env::set_var("ANTHROPIC_API_KEY", v),
            None => unsafe { std::env::remove_var("ANTHROPIC_API_KEY") },
        }
    }

    #[tokio::test]
    #[serial]
    async fn test_load_result_env_provider_detected() {
        let dir = tempfile::tempdir().unwrap();
        let orig_home = std::env::var("NIKA_HOME").ok();
        let orig_no_daemon = std::env::var("NIKA_NO_DAEMON").ok();
        let orig_key = std::env::var("OPENAI_API_KEY").ok();

        std::env::set_var("NIKA_HOME", dir.path());
        std::env::set_var("NIKA_NO_DAEMON", "1");
        std::env::set_var("OPENAI_API_KEY", "sk-test-123");

        let result = load_from_daemon_or_fallback().await;

        assert!(
            result.from_env.contains(&"openai".to_string()),
            "openai should appear in from_env when OPENAI_API_KEY is set"
        );
        assert!(
            result.total_loaded() >= 1,
            "at least 1 provider should be loaded, got {}",
            result.total_loaded()
        );
        let summary = result.summary();
        assert!(
            summary.contains("from env"),
            "summary should mention 'from env': {summary}"
        );

        match orig_home {
            Some(v) => std::env::set_var("NIKA_HOME", v),
            None => unsafe { std::env::remove_var("NIKA_HOME") },
        }
        match orig_no_daemon {
            Some(v) => std::env::set_var("NIKA_NO_DAEMON", v),
            None => unsafe { std::env::remove_var("NIKA_NO_DAEMON") },
        }
        match orig_key {
            Some(v) => std::env::set_var("OPENAI_API_KEY", v),
            None => unsafe { std::env::remove_var("OPENAI_API_KEY") },
        }
    }
}
