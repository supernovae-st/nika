//! Unified secrets management (v0.20.1).
//!
//! ## Architecture
//!
//! ```text
//! With spn-daemon feature:
//!
//! Nika Process
//!     │
//!     └── spn-client (Unix socket IPC)
//!         │
//!         └── spn daemon (~/.spn/daemon.sock)
//!             │
//!             └── OS Keychain (single accessor, no popups)
//!
//! Without spn-daemon feature (fallback):
//!
//! Nika Process
//!     │
//!     └── Direct access
//!         ├── OS Keychain (keyring crate)
//!         └── Environment variables
//! ```
//!
//! ## Usage
//!
//! Enable daemon support with: `cargo build --features spn-daemon`
//!
//! The daemon solves macOS Keychain popup issues by being the sole keychain accessor.

use crate::tui::widgets::provider_modal::SpnKeyring;
use secrecy::SecretString;
use tracing::{debug, info, trace};

#[cfg(feature = "spn-daemon")]
use tracing::warn;

// Use spn-core's unified provider definitions (13+ providers: LLM + MCP)
#[cfg(feature = "spn-daemon")]
use spn_client::KNOWN_PROVIDERS;

// Use TUI module's provider_env_var which handles both spn-daemon and fallback cases
use crate::tui::widgets::provider_modal::provider_env_var;

/// Result of loading secrets.
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

// ═══════════════════════════════════════════════════════════════════════════════
// WITH spn-daemon FEATURE
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(feature = "spn-daemon")]
mod daemon_integration {
    use super::*;
    use spn_client::{ExposeSecret, SpnClient};
    use std::sync::OnceLock;
    use tokio::sync::Mutex;

    /// Cached daemon client (singleton).
    static CLIENT: OnceLock<Mutex<Option<SpnClient>>> = OnceLock::new();

    /// Initialize and cache the daemon client.
    async fn get_or_init_client() -> Option<&'static Mutex<Option<SpnClient>>> {
        // Initialize on first call
        if CLIENT.get().is_none() {
            match SpnClient::connect_with_fallback().await {
                Ok(client) => {
                    let is_fallback = client.is_fallback_mode();
                    let _ = CLIENT.set(Mutex::new(Some(client)));
                    if is_fallback {
                        warn!("spn daemon not running, using env var fallback");
                    } else {
                        debug!("Connected to spn daemon");
                    }
                }
                Err(e) => {
                    warn!("Failed to connect to spn daemon: {}", e);
                    let _ = CLIENT.set(Mutex::new(None));
                }
            }
        }
        CLIENT.get()
    }

    /// Check if daemon is available.
    pub fn daemon_available() -> bool {
        spn_client::daemon_socket_exists()
    }

    /// Load secrets from daemon or fallback.
    pub async fn load_from_daemon_or_fallback() -> SecretsLoadResult {
        let mut result = SecretsLoadResult::default();

        // Try to get/init daemon client
        let client_lock = match get_or_init_client().await {
            Some(lock) => lock,
            None => {
                // Failed to init, use pure fallback
                return load_fallback_only().await;
            }
        };

        let mut guard = client_lock.lock().await;
        let client = match guard.as_mut() {
            Some(c) => c,
            None => {
                drop(guard);
                return load_fallback_only().await;
            }
        };

        result.daemon_available = !client.is_fallback_mode();

        // Iterate all known providers (LLM + MCP = 13+)
        for p in KNOWN_PROVIDERS {
            let provider = p.id; // Use p.id for API calls, not p.name (display name)
            let env_var = p.env_var;

            // Check if already in env
            if std::env::var(env_var).is_ok() {
                trace!("{}: already in env", provider);
                result.from_fallback.push(provider.to_string());
                continue;
            }

            // Try daemon (or its internal fallback)
            match client.get_secret(provider).await {
                Ok(secret) => {
                    // Inject into env for rig-core compatibility
                    std::env::set_var(env_var, secret.expose_secret());
                    if client.is_fallback_mode() {
                        debug!("{}: loaded from env fallback → {}", provider, env_var);
                        result.from_fallback.push(provider.to_string());
                    } else {
                        debug!("{}: loaded from daemon → {}", provider, env_var);
                        result.from_daemon.push(provider.to_string());
                    }
                }
                Err(_) => {
                    // Try direct keyring as last resort
                    if try_load_from_keyring(provider, env_var) {
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

    /// Get a secret for a provider.
    pub async fn get_secret(provider: &str) -> Option<SecretString> {
        let env_var = provider_env_var(provider);

        // Check env first (may have been loaded at boot)
        if let Ok(value) = std::env::var(env_var) {
            if !value.is_empty() {
                return Some(SecretString::from(value));
            }
        }

        // Try daemon client
        if let Some(client_lock) = get_or_init_client().await {
            let mut guard = client_lock.lock().await;
            if let Some(client) = guard.as_mut() {
                if let Ok(secret) = client.get_secret(provider).await {
                    return Some(secret);
                }
            }
        }

        // Final fallback: direct keyring
        SpnKeyring::get_secret(provider).ok()
    }

    /// Check if a secret exists for a provider.
    pub async fn has_secret(provider: &str) -> bool {
        let env_var = provider_env_var(provider);

        // Check env first
        if std::env::var(env_var).is_ok() {
            return true;
        }

        // Try daemon client
        if let Some(client_lock) = get_or_init_client().await {
            let mut guard = client_lock.lock().await;
            if let Some(client) = guard.as_mut() {
                if let Ok(exists) = client.has_secret(provider).await {
                    return exists;
                }
            }
        }

        // Final fallback: direct keyring
        SpnKeyring::exists(provider)
    }

    /// Fallback-only loading (when daemon completely unavailable).
    async fn load_fallback_only() -> SecretsLoadResult {
        let mut result = SecretsLoadResult {
            daemon_available: false,
            ..Default::default()
        };

        // Iterate all known providers (LLM + MCP = 13+)
        for p in KNOWN_PROVIDERS {
            let provider = p.id; // Use p.id for API calls, not p.name (display name)
            let env_var = p.env_var;

            if std::env::var(env_var).is_ok() {
                trace!("{}: already in env", provider);
                result.from_fallback.push(provider.to_string());
                continue;
            }

            if try_load_from_keyring(provider, env_var) {
                result.from_fallback.push(provider.to_string());
            } else {
                result.not_found.push(provider.to_string());
            }
        }

        info!("Secrets (fallback only): {}", result.summary());
        result
    }

    /// Try loading from keyring and inject into env if found.
    fn try_load_from_keyring(provider: &str, env_var: &str) -> bool {
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
}

// ═══════════════════════════════════════════════════════════════════════════════
// WITHOUT spn-daemon FEATURE (fallback only)
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(not(feature = "spn-daemon"))]
mod fallback_only {
    use super::*;

    /// Provider names for fallback mode (LLM only, no spn-client).
    const PROVIDERS: &[(&str, &str)] = &[
        ("anthropic", "ANTHROPIC_API_KEY"),
        ("openai", "OPENAI_API_KEY"),
        ("mistral", "MISTRAL_API_KEY"),
        ("groq", "GROQ_API_KEY"),
        ("deepseek", "DEEPSEEK_API_KEY"),
        ("gemini", "GEMINI_API_KEY"),
        ("ollama", "OLLAMA_API_BASE_URL"),
    ];

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

        for (provider, env_var) in PROVIDERS {
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
}

// ═══════════════════════════════════════════════════════════════════════════════
// PUBLIC API (re-exports based on feature)
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(feature = "spn-daemon")]
pub use daemon_integration::{
    daemon_available, get_secret, has_secret, load_from_daemon_or_fallback,
};

#[cfg(not(feature = "spn-daemon"))]
pub use fallback_only::{daemon_available, get_secret, has_secret, load_from_daemon_or_fallback};

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_env_var() {
        // Verify provider_env_var returns expected env var names
        assert_eq!(provider_env_var("anthropic"), "ANTHROPIC_API_KEY");
        assert_eq!(provider_env_var("openai"), "OPENAI_API_KEY");
        assert_eq!(provider_env_var("ollama"), "OLLAMA_API_BASE_URL");
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
        // Without daemon running, should return false (or check socket existence)
        #[cfg(not(feature = "spn-daemon"))]
        assert!(!daemon_available());

        #[cfg(feature = "spn-daemon")]
        {
            // With feature, checks if socket exists
            let result = daemon_available();
            // Result depends on whether daemon is actually running
            let _ = result;
        }
    }
}
