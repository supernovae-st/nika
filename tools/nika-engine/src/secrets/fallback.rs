//! Secrets management -- env vars + optional keyring.

use crate::core::{ProviderCategory, KNOWN_PROVIDERS};
use crate::secrets::keyring::{should_skip_keychain, NikaKeyring};
use crate::secrets::result::SecretsLoadResult;
use secrecy::SecretString;
use tracing::{debug, info, trace};

pub fn daemon_available() -> bool {
    false
}

pub async fn load_from_daemon_or_fallback() -> SecretsLoadResult {
    let mut result = SecretsLoadResult::default();
    for provider in KNOWN_PROVIDERS
        .iter()
        .filter(|p| p.category == ProviderCategory::Llm)
    {
        let provider_id = provider.id;
        let env_var = provider.env_var;
        if std::env::var(env_var)
            .map(|v| !v.is_empty())
            .unwrap_or(false)
        {
            trace!("{}: already in env", provider_id);
            result.from_env.push(provider_id.to_string());
            continue;
        }
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
    let keychain_boot = std::env::var("NIKA_KEYCHAIN_BOOT")
        .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);
    if cfg!(test) || should_skip_keychain() || !keychain_boot {
        trace!("{}: keychain skipped", provider);
        return false;
    }
    match NikaKeyring::get(provider) {
        Ok(secret) => {
            std::env::set_var(env_var, &*secret);
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
    if let Ok(value) = std::env::var(env_var) {
        if !value.is_empty() {
            return Some(SecretString::from(value));
        }
    }
    let keychain_allowed = std::env::var("NIKA_KEYCHAIN_BOOT")
        .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);
    if cfg!(test) || should_skip_keychain() || !keychain_allowed {
        return None;
    }
    NikaKeyring::get_secret(provider).ok()
}

pub async fn has_secret(provider: &str) -> bool {
    let env_var = provider_env_var(provider);
    if std::env::var(env_var)
        .map(|v| !v.is_empty())
        .unwrap_or(false)
    {
        return true;
    }
    let keychain_allowed = std::env::var("NIKA_KEYCHAIN_BOOT")
        .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);
    if cfg!(test) || should_skip_keychain() || !keychain_allowed {
        return false;
    }
    NikaKeyring::exists(provider)
}

fn provider_env_var(provider: &str) -> &'static str {
    crate::core::provider_to_env_var(provider).unwrap_or("UNKNOWN_API_KEY")
}
