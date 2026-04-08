// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Secrets management -- env vars + daemon IPC + NikaVault.
//!
//! Resolution order for each provider:
//! 1. Environment variable (always checked first, zero overhead)
//! 2. Daemon IPC (if daemon socket exists, Unix only)
//! 3. NikaVault encrypted file store (~/.nika/secrets/vault.enc)

use crate::core::{ProviderCategory, KNOWN_PROVIDERS};
use crate::secrets::result::SecretsLoadResult;
use secrecy::{ExposeSecret, SecretString};
use tracing::{debug, info, trace};

/// Check if the daemon is running (socket file exists).
pub fn daemon_available() -> bool {
    #[cfg(unix)]
    return nika_daemon::daemon_socket_path().exists();
    #[cfg(not(unix))]
    false
}

/// Load secrets into the process environment from daemon IPC or vault.
///
/// **Call order guarantee**: this function MUST be called before any workflow
/// task execution so that `$env.VAR` bindings (resolved via `std::env::var`)
/// can see daemon/vault secrets. The call sites that enforce this:
///
/// - `main.rs` — called before `resolve_workflow_path` / task execution
/// - `boot.rs` — phase 4 (SecretsLoading), before MCP startup (phase 5)
///   and provider validation (phase 6)
///
/// Secrets are injected via `std::env::set_var`, making them visible to
/// `$env.VAR` binding resolution in `binding/resolve.rs`.
pub async fn load_from_daemon_or_fallback() -> SecretsLoadResult {
    let mut result = SecretsLoadResult::default();

    // Auto-start daemon if not running (Unix only, silent)
    #[cfg(unix)]
    if !daemon_available() && std::env::var("NIKA_NO_DAEMON").is_err() {
        let log_dir = nika_daemon::daemon_dir();
        let _ = std::fs::create_dir_all(&log_dir);
        let log_path = log_dir.join("daemon.log");
        match nika_daemon::lifecycle::daemonize(&log_path) {
            Ok(()) => {
                debug!("auto-started daemon in background");
                // Give daemon time to bind socket
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
            Err(e) => {
                trace!("daemon auto-start failed (non-fatal): {e}");
            }
        }
    }

    #[cfg(unix)]
    let daemon = if daemon_available() {
        Some(nika_daemon::DaemonClient::default_path())
    } else {
        None
    };
    #[cfg(not(unix))]
    let _daemon: Option<()> = None;

    let backend = nika_vault::VaultBackend::from_env();

    for provider in KNOWN_PROVIDERS
        .iter()
        .filter(|p| p.category == ProviderCategory::Llm)
    {
        let provider_id = provider.id;
        let env_var = provider.env_var;

        // 1. Check store + env var first (zero overhead)
        if super::store::resolve_env(env_var).is_some() {
            trace!("{}: already in env/store", provider_id);
            result.from_env.push(provider_id.to_string());
            continue;
        }

        // 2. If Doppler backend: try doppler before daemon/vault
        if backend == nika_vault::VaultBackend::Doppler {
            if let Ok(Some(val)) = nika_vault::DopplerBackend::get(env_var) {
                super::inject_secret_to_env(env_var, &val);
                debug!("{}: loaded from doppler", provider_id);
                result.from_env.push(provider_id.to_string());
                continue;
            }
            // Fall through to local sources on Doppler failure
        }

        // 3. Try daemon IPC (if available) — Unix only
        #[cfg(unix)]
        if let Some(ref client) = daemon {
            if let Ok(Some(secret)) = client.get_secret(provider_id).await {
                super::inject_secret_to_env(env_var, &secret);
                debug!("{}: loaded from daemon", provider_id);
                result.from_env.push(provider_id.to_string());
                continue;
            }
        }

        // 4. Try vault directly (works even without daemon)
        {
            #[cfg(unix)]
            let nika_home = nika_daemon::daemon_dir()
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".nika"));
            #[cfg(not(unix))]
            let nika_home = dirs::home_dir().unwrap_or_default().join(".nika");
            let vault = nika_vault::NikaVault::new(&nika_home.join("secrets"));
            if let Ok(Some(secret)) = vault.get(provider_id) {
                let val = secret.expose_secret().to_string();
                super::inject_secret_to_env(env_var, &val);
                debug!("{}: loaded from vault", provider_id);
                result.from_env.push(provider_id.to_string());
                continue;
            }
        }

        result.not_found.push(provider_id.to_string());
    }

    // ── Inject custom vault keys into the SecretStore ───────────────────
    //
    // Keys stored with "custom:" prefix (e.g. `nika keys set ELEVENLABS_API_KEY`)
    // are NOT LLM providers, so the KNOWN_PROVIDERS loop above skips them.
    // We inject them here so `$env.ELEVENLABS_API_KEY` bindings work at runtime.
    inject_custom_vault_keys(&backend);

    info!("Secrets: {}", result.summary());
    result
}

/// Inject custom (non-provider) vault keys into the in-process SecretStore.
///
/// Scans the vault for entries starting with `custom:` and injects them as
/// environment-style secrets. For example, `custom:ELEVENLABS_API_KEY` becomes
/// accessible as `$env.ELEVENLABS_API_KEY` in workflow bindings.
///
/// Skips keys already present in the environment to preserve env var priority.
fn inject_custom_vault_keys(backend: &nika_vault::VaultBackend) {
    // Doppler custom keys: not applicable (Doppler manages its own env injection)
    if *backend == nika_vault::VaultBackend::Doppler {
        return;
    }

    let vault = match super::vault::try_open_vault() {
        Some(v) => v,
        None => return,
    };

    let keys = match vault.list() {
        Ok(k) => k,
        Err(e) => {
            debug!("vault list failed (non-fatal): {e}");
            return;
        }
    };

    for key in keys.iter().filter(|k| k.starts_with("custom:")) {
        let env_name = match key.strip_prefix("custom:") {
            Some(name) if !name.is_empty() => name,
            _ => continue,
        };

        // Respect env var priority: don't overwrite existing values
        if super::store::resolve_env(env_name).is_some() {
            trace!("custom:{}: already in env/store, skipping", env_name);
            continue;
        }

        match vault.get(key) {
            Ok(Some(secret)) => {
                super::inject_secret_to_env(env_name, secret.expose_secret());
                debug!("custom:{}: injected from vault", env_name);
            }
            Ok(None) => {}
            Err(e) => {
                debug!("custom:{}: vault read failed (non-fatal): {e}", env_name);
            }
        }
    }
}

pub async fn get_secret(provider: &str) -> Option<SecretString> {
    let env_var = provider_env_var(provider);
    let backend = nika_vault::VaultBackend::from_env();

    // 1. Check store + env (always first, regardless of backend)
    if let Some(value) = super::store::resolve_env(env_var) {
        return Some(SecretString::from(value));
    }

    // 2. If Doppler backend: try doppler before daemon/vault
    if backend == nika_vault::VaultBackend::Doppler {
        if let Ok(Some(val)) = nika_vault::DopplerBackend::get(env_var) {
            super::inject_secret_to_env(env_var, &val);
            debug!("{}: loaded from doppler", provider);
            return Some(SecretString::from(val));
        }
        // Fall through to local vault on Doppler failure
        debug!("{}: doppler fallback to local vault", provider);
    }

    // 3. Try daemon (local backend path)
    #[cfg(unix)]
    if daemon_available() {
        let client = nika_daemon::DaemonClient::default_path();
        if let Ok(Some(secret)) = client.get_secret(provider).await {
            return Some(SecretString::from(secret));
        }
    }

    // 4. Try vault directly (works even without daemon)
    #[cfg(unix)]
    let nika_home = nika_daemon::daemon_dir()
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".nika"));
    #[cfg(not(unix))]
    let nika_home = dirs::home_dir().unwrap_or_default().join(".nika");
    let vault = nika_vault::NikaVault::new(&nika_home.join("secrets"));
    if let Ok(Some(secret)) = vault.get(provider) {
        let val = secret.expose_secret().to_string();
        super::inject_secret_to_env(env_var, &val);
        debug!("{}: loaded from vault", provider);
        return Some(SecretString::from(val));
    }

    None
}

pub async fn has_secret(provider: &str) -> bool {
    let env_var = provider_env_var(provider);
    let backend = nika_vault::VaultBackend::from_env();

    // 1. Check store + env
    if super::store::resolve_env(env_var).is_some() {
        return true;
    }

    // 2. If Doppler backend: try doppler
    if backend == nika_vault::VaultBackend::Doppler {
        if let Ok(Some(_)) = nika_vault::DopplerBackend::get(env_var) {
            return true;
        }
    }

    // Skip daemon + vault when NIKA_NO_DAEMON is set (env-only mode)
    if std::env::var("NIKA_NO_DAEMON").is_ok() {
        return false;
    }

    // 3. Try daemon
    #[cfg(unix)]
    if daemon_available() {
        let client = nika_daemon::DaemonClient::default_path();
        if let Ok(exists) = client.has_secret(provider).await {
            return exists;
        }
    }

    // 4. Try vault directly
    #[cfg(unix)]
    let nika_home = nika_daemon::daemon_dir()
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".nika"));
    #[cfg(not(unix))]
    let nika_home = dirs::home_dir().unwrap_or_default().join(".nika");
    let vault = nika_vault::NikaVault::new(&nika_home.join("secrets"));
    if let Ok(Some(_)) = vault.get(provider) {
        return true;
    }

    false
}

fn provider_env_var(provider: &str) -> &'static str {
    crate::core::provider_to_env_var(provider).unwrap_or("UNKNOWN_API_KEY")
}
