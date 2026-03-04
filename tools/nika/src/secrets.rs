//! Unified secrets management (v0.20).
//!
//! ## Architecture
//!
//! ```text
//! Nika Process
//!     │
//!     └── Direct access
//!         ├── OS Keychain (keyring crate)
//!         └── Environment variables
//! ```
//!
//! ## Note
//!
//! The spn daemon integration (via spn-client) is disabled in this build.
//! To enable daemon support for unified keychain access, uncomment the
//! spn-client dependency in Cargo.toml and enable the spn-daemon feature.

use crate::tui::widgets::provider_modal::{provider_env_var, SpnKeyring};
use secrecy::SecretString;
use tracing::{debug, info, trace};

/// Provider names we try to load.
const PROVIDERS: &[&str] = &[
    "anthropic",
    "openai",
    "mistral",
    "groq",
    "deepseek",
    "gemini",
    "ollama",
];

/// Result of loading secrets.
#[derive(Debug, Clone, Default)]
pub struct SecretsLoadResult {
    /// Providers loaded from spn daemon (always empty without daemon).
    pub from_daemon: Vec<String>,
    /// Providers loaded from fallback (keyring/env).
    pub from_fallback: Vec<String>,
    /// Providers with no key found.
    pub not_found: Vec<String>,
    /// Whether daemon was available (always false without daemon).
    pub daemon_available: bool,
}

impl SecretsLoadResult {
    /// Total number of secrets loaded.
    pub fn total_loaded(&self) -> usize {
        self.from_daemon.len() + self.from_fallback.len()
    }

    /// Human-readable summary.
    pub fn summary(&self) -> String {
        if self.daemon_available {
            format!(
                "{} from daemon, {} fallback, {} not found",
                self.from_daemon.len(),
                self.from_fallback.len(),
                self.not_found.len()
            )
        } else {
            format!(
                "daemon unavailable, {} from fallback, {} not found",
                self.from_fallback.len(),
                self.not_found.len()
            )
        }
    }
}

/// Load secrets from keyring/env.
///
/// This is called during boot to inject secrets as environment variables
/// so that rig-core's `from_env()` pattern continues to work.
pub async fn load_from_daemon_or_fallback() -> SecretsLoadResult {
    let mut result = SecretsLoadResult {
        daemon_available: false,
        ..Default::default()
    };

    // Load from keyring/env
    for provider in PROVIDERS {
        let env_var = provider_env_var(provider);

        // Check if already in env
        if std::env::var(env_var).is_ok() {
            trace!("{}: already in env", provider);
            result.from_fallback.push(provider.to_string());
            continue;
        }

        if try_load_from_fallback(provider, env_var) {
            result.from_fallback.push(provider.to_string());
        } else {
            result.not_found.push(provider.to_string());
        }
    }

    info!("Secrets: {}", result.summary());
    result
}

/// Try loading from keyring and inject into env if found.
fn try_load_from_fallback(provider: &str, env_var: &str) -> bool {
    match SpnKeyring::get(provider) {
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

    // Fall back to keyring
    SpnKeyring::get_secret(provider).ok()
}

/// Check if a secret exists for a provider.
pub async fn has_secret(provider: &str) -> bool {
    let env_var = provider_env_var(provider);

    // Check env first
    if std::env::var(env_var).is_ok() {
        return true;
    }

    // Fall back to keyring
    SpnKeyring::exists(provider)
}

/// Check if daemon is available (always false without spn-daemon feature).
pub fn daemon_available() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_providers_list() {
        assert!(PROVIDERS.contains(&"anthropic"));
        assert!(PROVIDERS.contains(&"openai"));
        assert!(PROVIDERS.contains(&"ollama"));
    }

    #[test]
    fn test_secrets_load_result_summary() {
        let result = SecretsLoadResult {
            from_daemon: vec!["anthropic".into()],
            from_fallback: vec!["openai".into()],
            not_found: vec!["groq".into()],
            daemon_available: true,
        };
        assert_eq!(result.total_loaded(), 2);
        assert!(result.summary().contains("1 from daemon"));
    }

    #[test]
    fn test_secrets_load_result_no_daemon() {
        let result = SecretsLoadResult {
            from_daemon: vec![],
            from_fallback: vec!["anthropic".into()],
            not_found: vec![],
            daemon_available: false,
        };
        assert!(result.summary().contains("daemon unavailable"));
    }

    #[test]
    fn test_daemon_available_check() {
        // Daemon is not available in this build
        assert!(!daemon_available());
    }
}
