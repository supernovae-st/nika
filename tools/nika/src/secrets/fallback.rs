//! Fallback secrets management (when nika-daemon feature is NOT enabled).
//!
//! Uses direct keyring access and environment variables.
//! This module is only compiled when nika-daemon is not enabled.

use crate::core::{ProviderCategory, KNOWN_PROVIDERS};
use crate::secrets::result::SecretsLoadResult;
use secrecy::SecretString;
use tracing::{debug, info, trace};

// ═══════════════════════════════════════════════════════════════════════════════
// TUI-CONDITIONAL IMPORTS
// When TUI is enabled, use the full NikaKeyring from TUI.
// When TUI is disabled, use fallback stub below.
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(feature = "tui")]
use crate::tui::widgets::provider_modal::NikaKeyring;

// Fallback: NikaKeyring stub without TUI (keyring not available without TUI feature)
#[cfg(not(feature = "tui"))]
mod fallback_keyring {
    use secrecy::SecretString;
    use std::io::{Error, ErrorKind};

    /// Stub NikaKeyring when TUI is disabled.
    /// All operations return errors since keyring requires TUI dependencies.
    pub struct NikaKeyring;

    impl NikaKeyring {
        pub fn get(_provider: &str) -> Result<String, Error> {
            Err(Error::new(
                ErrorKind::Unsupported,
                "Keyring access requires TUI feature",
            ))
        }

        pub fn get_secret(_provider: &str) -> Result<SecretString, Error> {
            Err(Error::new(
                ErrorKind::Unsupported,
                "Keyring access requires TUI feature",
            ))
        }

        pub fn exists(_provider: &str) -> bool {
            false
        }
    }
}

#[cfg(not(feature = "tui"))]
use fallback_keyring::NikaKeyring;

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
fn try_load_from_fallback(provider: &str, env_var: &str) -> bool {
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

    // Fall back to keyring
    NikaKeyring::exists(provider)
}

/// Get the environment variable name for a provider ID.
fn provider_env_var(provider: &str) -> &'static str {
    // Use KNOWN_PROVIDERS as source of truth
    crate::core::provider_to_env_var(provider).unwrap_or("UNKNOWN_API_KEY")
}
