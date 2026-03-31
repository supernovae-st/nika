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

        // 3. Try vault directly (works even without daemon)
        {
            let nika_home = nika_daemon::daemon_dir()
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| dirs::home_dir().unwrap().join(".nika"));
            let vault = nika_core::vault::NikaVault::new(&nika_home.join("secrets"));
            if let Ok(Some(secret)) = vault.get(provider_id) {
                let val = secret.expose_secret().to_string();
                // SAFETY: called during sequential boot before spawning workflow tasks
                unsafe { std::env::set_var(env_var, &val) };
                debug!("{}: loaded from vault", provider_id);
                result.from_env.push(provider_id.to_string());
                continue;
            }
        }

        result.not_found.push(provider_id.to_string());
    }
    info!("Secrets: {}", result.summary());
    result
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

    // 3. Try vault directly (works even without daemon)
    let nika_home = nika_daemon::daemon_dir()
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| dirs::home_dir().unwrap().join(".nika"));
    let vault = nika_core::vault::NikaVault::new(&nika_home.join("secrets"));
    if let Ok(Some(secret)) = vault.get(provider) {
        let val = secret.expose_secret().to_string();
        // Inject into process env for caching
        // SAFETY: called during boot before spawning workflow tasks
        unsafe { std::env::set_var(env_var, &val) };
        debug!("{}: loaded from vault", provider);
        return Some(SecretString::from(val));
    }

    None
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

    // 3. Try vault directly
    let nika_home = nika_daemon::daemon_dir()
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| dirs::home_dir().unwrap().join(".nika"));
    let vault = nika_core::vault::NikaVault::new(&nika_home.join("secrets"));
    if let Ok(Some(_)) = vault.get(provider) {
        return true;
    }

    false
}

fn provider_env_var(provider: &str) -> &'static str {
    crate::core::provider_to_env_var(provider).unwrap_or("UNKNOWN_API_KEY")
}
