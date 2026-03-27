//! Secrets management -- env vars + optional daemon IPC + optional keyring.
//!
//! Resolution order for each provider:
//! 1. Environment variable (always checked first, zero overhead)
//! 2. Daemon IPC (if daemon socket exists — keychain access via daemon, Unix only)
//! 3. Direct keyring (if native-keychain feature enabled and NIKA_SKIP_KEYCHAIN is not set)

use crate::core::{ProviderCategory, KNOWN_PROVIDERS};
use crate::secrets::keyring::{should_skip_keychain, NikaKeyring};
use crate::secrets::result::SecretsLoadResult;
use secrecy::SecretString;
use tracing::{debug, info, trace};

/// Check if the daemon is running (socket file exists).
pub fn daemon_available() -> bool {
    #[cfg(unix)]
    return nika_daemon::daemon_socket_path().exists();
    #[cfg(not(unix))]
    false
}

pub async fn load_from_daemon_or_fallback() -> SecretsLoadResult {
    let mut result = SecretsLoadResult::default();
    #[cfg(unix)]
    let daemon = if daemon_available() {
        Some(nika_daemon::DaemonClient::default_path())
    } else {
        None
    };
    #[cfg(not(unix))]
    let daemon: Option<()> = None;

    for provider in KNOWN_PROVIDERS
        .iter()
        .filter(|p| p.category == ProviderCategory::Llm)
    {
        let provider_id = provider.id;
        let env_var = provider.env_var;

        // 1. Check env var first (zero overhead)
        if std::env::var(env_var)
            .map(|v| !v.is_empty())
            .unwrap_or(false)
        {
            trace!("{}: already in env", provider_id);
            result.from_env.push(provider_id.to_string());
            continue;
        }

        // 2. Try daemon IPC (if available) — Unix only
        #[cfg(unix)]
        if let Some(ref client) = daemon {
            if let Ok(Some(secret)) = client.get_secret(provider_id).await {
                // SAFETY: called during sequential boot before spawning workflow tasks
                unsafe { std::env::set_var(env_var, &secret) };
                debug!("{}: loaded from daemon", provider_id);
                result.from_env.push(provider_id.to_string());
                continue;
            }
        }

        // 3. Try direct keyring (fallback)
        if try_load_from_keyring(provider_id, env_var) {
            result.from_env.push(provider_id.to_string());
        } else {
            result.not_found.push(provider_id.to_string());
        }
    }
    info!("Secrets: {}", result.summary());
    result
}

fn try_load_from_keyring(provider: &str, env_var: &str) -> bool {
    if cfg!(test) || should_skip_keychain() {
        trace!("{}: keychain skipped", provider);
        return false;
    }
    match NikaKeyring::get(provider) {
        Ok(secret) => {
            // SAFETY: called during sequential boot before spawning workflow tasks
            unsafe { std::env::set_var(env_var, &*secret) };
            debug!("{}: loaded from keyring", provider);
            true
        }
        Err(_) => {
            trace!("{}: not in keyring", provider);
            false
        }
    }
}

pub async fn get_secret(provider: &str) -> Option<SecretString> {
    let env_var = provider_env_var(provider);

    // 1. Check env
    if let Ok(value) = std::env::var(env_var) {
        if !value.is_empty() {
            return Some(SecretString::from(value));
        }
    }

    // 2. Try daemon
    #[cfg(unix)]
    if daemon_available() {
        let client = nika_daemon::DaemonClient::default_path();
        if let Ok(Some(secret)) = client.get_secret(provider).await {
            return Some(SecretString::from(secret));
        }
    }

    // 3. Try direct keyring
    if cfg!(test) || should_skip_keychain() {
        return None;
    }
    NikaKeyring::get_secret(provider).ok()
}

pub async fn has_secret(provider: &str) -> bool {
    let env_var = provider_env_var(provider);

    // 1. Check env
    if std::env::var(env_var)
        .map(|v| !v.is_empty())
        .unwrap_or(false)
    {
        return true;
    }

    // 2. Try daemon
    #[cfg(unix)]
    if daemon_available() {
        let client = nika_daemon::DaemonClient::default_path();
        if let Ok(exists) = client.has_secret(provider).await {
            return exists;
        }
    }

    // 3. Try direct keyring
    if cfg!(test) || should_skip_keychain() {
        return false;
    }
    NikaKeyring::exists(provider)
}

fn provider_env_var(provider: &str) -> &'static str {
    crate::core::provider_to_env_var(provider).unwrap_or("UNKNOWN_API_KEY")
}
