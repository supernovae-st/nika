//! Unified secrets management with spn daemon fallback (v0.20).
//!
//! ## Architecture
//!
//! ```text
//! Nika Process
//!     │
//!     ├── Try spn daemon (via spn-client) [requires spn-daemon feature]
//!     │   └── Unix socket IPC to ~/.spn/daemon.sock
//!     │
//!     └── Fallback to direct access
//!         ├── OS Keychain (keyring crate)
//!         └── Environment variables
//! ```
//!
//! ## Why?
//!
//! On macOS, each binary accessing the Keychain triggers an "Always Allow" prompt.
//! By using the spn daemon as the sole Keychain accessor, we avoid multiple prompts
//! and cache secrets in memory (with mlock protection against swap).

use crate::tui::widgets::provider_modal::{provider_env_var, SpnKeyring};
#[cfg(feature = "spn-daemon")]
use secrecy::ExposeSecret;
use secrecy::SecretString;
#[cfg(feature = "spn-daemon")]
use tracing::warn;
use tracing::{debug, info, trace};

/// Provider names we try to load from daemon.
const PROVIDERS: &[&str] = &[
    "anthropic",
    "openai",
    "mistral",
    "groq",
    "deepseek",
    "gemini",
    "ollama",
];

/// Result of loading secrets from daemon.
#[derive(Debug, Clone, Default)]
pub struct SecretsLoadResult {
    /// Providers loaded from spn daemon.
    pub from_daemon: Vec<String>,
    /// Providers loaded from fallback (keyring/env).
    pub from_fallback: Vec<String>,
    /// Providers with no key found.
    pub not_found: Vec<String>,
    /// Whether daemon was available.
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

/// Load secrets from spn daemon, falling back to keyring/env.
///
/// This is called during boot to inject secrets as environment variables
/// so that rig-core's `from_env()` pattern continues to work.
#[cfg(feature = "spn-daemon")]
pub async fn load_from_daemon_or_fallback() -> SecretsLoadResult {
    let mut result = SecretsLoadResult::default();

    // Try connecting to daemon first
    match spn_client::SpnClient::connect().await {
        Ok(mut client) => {
            result.daemon_available = true;
            info!("Connected to spn daemon");

            for provider in PROVIDERS {
                let env_var = provider_env_var(provider);

                // Skip if already in env (e.g., from .env file)
                if std::env::var(env_var).is_ok() {
                    trace!("{}: already in env, skipping", provider);
                    continue;
                }

                match client.get_secret(provider).await {
                    Ok(secret) => {
                        // Inject into environment for rig-core
                        std::env::set_var(env_var, secret.expose_secret());
                        result.from_daemon.push(provider.to_string());
                        debug!("{}: loaded from daemon → {}", provider, env_var);
                    }
                    Err(spn_client::Error::SecretNotFound { .. }) => {
                        trace!("{}: not in daemon cache", provider);
                        if try_load_from_fallback(provider, env_var) {
                            result.from_fallback.push(provider.to_string());
                        } else {
                            result.not_found.push(provider.to_string());
                        }
                    }
                    Err(e) => {
                        warn!("{}: daemon error: {}", provider, e);
                        if try_load_from_fallback(provider, env_var) {
                            result.from_fallback.push(provider.to_string());
                        } else {
                            result.not_found.push(provider.to_string());
                        }
                    }
                }
            }
        }
        Err(e) => {
            debug!("spn daemon not available: {}", e);
            result.daemon_available = false;

            // Fall back to direct keyring/env access
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
        }
    }

    info!("Secrets: {}", result.summary());
    result
}

/// Load secrets from keyring/env only (no daemon support).
#[cfg(not(feature = "spn-daemon"))]
pub async fn load_from_daemon_or_fallback() -> SecretsLoadResult {
    let mut result = SecretsLoadResult::default();
    result.daemon_available = false;

    // No daemon support - go directly to fallback
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

/// Get a secret for a provider (async, tries daemon first).
#[cfg(feature = "spn-daemon")]
pub async fn get_secret(provider: &str) -> Option<SecretString> {
    let env_var = provider_env_var(provider);

    // Check env first (may have been loaded at boot)
    if let Ok(value) = std::env::var(env_var) {
        if !value.is_empty() {
            return Some(SecretString::from(value));
        }
    }

    // Try daemon
    if let Ok(mut client) = spn_client::SpnClient::connect().await {
        if let Ok(secret) = client.get_secret(provider).await {
            return Some(secret);
        }
    }

    // Fall back to keyring
    SpnKeyring::get_secret(provider).ok()
}

/// Get a secret for a provider (no daemon support).
#[cfg(not(feature = "spn-daemon"))]
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
#[cfg(feature = "spn-daemon")]
pub async fn has_secret(provider: &str) -> bool {
    let env_var = provider_env_var(provider);

    // Check env first
    if std::env::var(env_var).is_ok() {
        return true;
    }

    // Try daemon
    if let Ok(mut client) = spn_client::SpnClient::connect().await {
        if let Ok(exists) = client.has_secret(provider).await {
            return exists;
        }
    }

    // Fall back to keyring
    SpnKeyring::exists(provider)
}

/// Check if a secret exists for a provider (no daemon support).
#[cfg(not(feature = "spn-daemon"))]
pub async fn has_secret(provider: &str) -> bool {
    let env_var = provider_env_var(provider);

    // Check env first
    if std::env::var(env_var).is_ok() {
        return true;
    }

    // Fall back to keyring
    SpnKeyring::exists(provider)
}

/// Check if daemon is available.
#[cfg(feature = "spn-daemon")]
pub fn daemon_available() -> bool {
    spn_client::daemon_socket_exists()
}

/// Daemon is not available without the feature.
#[cfg(not(feature = "spn-daemon"))]
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
        // In tests, daemon is typically not running
        // This just verifies the function doesn't panic
        let _available = daemon_available();
    }
}
