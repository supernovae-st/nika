//! Secrets management -- env vars + optional keyring.

pub mod keyring;
mod fallback;
mod result;

pub use keyring::{mask_api_key, migrate_env_to_keyring, validate_key_format, KeyringError, MigrationReport, NikaKeyring};
pub use result::SecretsLoadResult;
pub use fallback::{daemon_available, get_secret, has_secret, load_from_daemon_or_fallback};

/// Get the environment variable name for a provider ID.
pub fn provider_env_var(provider: &str) -> &'static str {
    crate::core::provider_to_env_var(provider).unwrap_or("UNKNOWN_API_KEY")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{ProviderCategory, KNOWN_PROVIDERS};
    use secrecy::ExposeSecret;
    use serial_test::serial;
    #[test] fn test_provider_env_var_lookup() { assert_eq!(provider_env_var("anthropic"), "ANTHROPIC_API_KEY"); assert_eq!(provider_env_var("openai"), "OPENAI_API_KEY"); }
    #[test] fn test_provider_env_var_unknown() { assert_eq!(provider_env_var("nonexistent"), "UNKNOWN_API_KEY"); }
    #[test] fn test_daemon_available_check() { assert!(!daemon_available()); }
    #[tokio::test] #[serial] async fn test_get_secret_returns_env_var_value() { let key = "ANTHROPIC_API_KEY"; let orig = std::env::var(key).ok(); std::env::set_var(key, "sk-ant-test"); let s = get_secret("anthropic").await; assert!(s.is_some()); assert_eq!(s.unwrap().expose_secret(), "sk-ant-test"); match orig { Some(v) => std::env::set_var(key, v), None => unsafe { std::env::remove_var(key) } } }
    #[tokio::test] #[serial] async fn test_has_secret_true_when_env_set() { let key = "MISTRAL_API_KEY"; let orig = std::env::var(key).ok(); std::env::set_var(key, "test"); assert!(has_secret("mistral").await); match orig { Some(v) => std::env::set_var(key, v), None => unsafe { std::env::remove_var(key) } } }
    #[tokio::test] #[serial] async fn test_has_secret_false_when_env_empty() { let key = "XAI_API_KEY"; let orig = std::env::var(key).ok(); std::env::set_var(key, ""); assert!(!has_secret("xai").await); match orig { Some(v) => std::env::set_var(key, v), None => unsafe { std::env::remove_var(key) } } }
    #[tokio::test] async fn test_load_result_structure() { let r = load_from_daemon_or_fallback().await; let expected = KNOWN_PROVIDERS.iter().filter(|p| p.category == ProviderCategory::Llm).count(); let total = r.from_env.len() + r.not_found.len(); assert_eq!(total, expected); }
    #[tokio::test] async fn test_load_result_summary_not_empty() { let r = load_from_daemon_or_fallback().await; assert!(!r.summary().is_empty()); }
}
