//! Daemon integration for secrets management.
//!
//! Uses spn-client for IPC communication with the nika daemon.
//! The daemon is the SOLE keychain accessor to prevent macOS popups.

use crate::core::KNOWN_PROVIDERS;
use crate::secrets::result::SecretsLoadResult;
use secrecy::SecretString;
use spn_client::{ExposeSecret, SpnClient};
use std::sync::OnceLock;
use tokio::sync::Mutex;
use tracing::{debug, info, trace, warn};

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
                    warn!("nika daemon not running, using env var fallback");
                } else {
                    debug!("Connected to nika daemon");
                }
            }
            Err(e) => {
                warn!("Failed to connect to nika daemon: {}", e);
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

    // Iterate all known providers (LLM + MCP + Local = 20)
    for p in KNOWN_PROVIDERS {
        let provider_id = p.id;
        let env_var = p.env_var;

        // Check if already in env
        if std::env::var(env_var).is_ok() {
            trace!("{}: already in env", provider_id);
            result.from_fallback.push(provider_id.to_string());
            continue;
        }

        // Try daemon (or its internal fallback)
        match client.get_secret(provider_id).await {
            Ok(secret) => {
                // Inject into env for rig-core compatibility
                std::env::set_var(env_var, secret.expose_secret());
                if client.is_fallback_mode() {
                    debug!("{}: loaded from env fallback → {}", provider_id, env_var);
                    result.from_fallback.push(provider_id.to_string());
                } else {
                    debug!("{}: loaded from daemon → {}", provider_id, env_var);
                    result.from_daemon.push(provider_id.to_string());
                }
            }
            Err(_) => {
                // v0.22: Do NOT fall back to direct keyring when daemon is available.
                // The daemon is the SOLE keychain accessor to prevent macOS popups.
                // If daemon doesn't have the secret, mark as not found.
                trace!("{}: not found in daemon", provider_id);
                result.not_found.push(provider_id.to_string());
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

    // v0.22: Do NOT fall back to direct keyring - daemon is sole accessor
    None
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

    // v0.22: Do NOT fall back to direct keyring - daemon is sole accessor
    false
}

/// Fallback-only loading (when daemon completely unavailable).
/// v0.22: Only checks env vars - NO direct keyring access to prevent popups.
async fn load_fallback_only() -> SecretsLoadResult {
    let mut result = SecretsLoadResult {
        daemon_available: false,
        ..Default::default()
    };

    // Iterate all known providers (LLM + MCP + Local = 20)
    for p in KNOWN_PROVIDERS {
        let provider_id = p.id;
        let env_var = p.env_var;

        if std::env::var(env_var).is_ok() {
            trace!("{}: already in env", provider_id);
            result.from_fallback.push(provider_id.to_string());
        } else {
            // v0.22: Do NOT fall back to keyring - only env vars when daemon unavailable
            result.not_found.push(provider_id.to_string());
        }
    }

    info!("Secrets (fallback only): {}", result.summary());
    result
}

/// Get the environment variable name for a provider ID.
fn provider_env_var(provider: &str) -> &'static str {
    // Use KNOWN_PROVIDERS as source of truth
    crate::core::provider_to_env_var(provider).unwrap_or("UNKNOWN_API_KEY")
}
