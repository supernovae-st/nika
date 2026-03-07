//! Secure API key storage via system keychain.
//!
//! Uses keyring-rs for cross-platform credential storage:
//! - macOS: Keychain Access
//! - Windows: Credential Manager
//! - Linux: Secret Service (GNOME Keyring, KWallet)
//!
//! Service name "spn" is shared with supernovae-cli for unified key management.
//!
//! ## v0.20: spn-core Integration
//!
//! Validation and masking now delegate to spn-core via spn-client re-exports.
//! Local wrappers maintain backward compatibility with existing API signatures.

use colored::Colorize;
use keyring::Entry;
use secrecy::SecretString;
use thiserror::Error;
use zeroize::Zeroizing;

// Unified provider access (single source of truth)
use crate::tui::providers::env_var as provider_env_var;

// Import spn-core types via spn-client re-exports (when spn-daemon feature enabled)
#[cfg(feature = "spn-daemon")]
use spn_client::{mask_key as spn_mask_key, validate_key_format as spn_validate_key_format};

/// Service name for keyring entries.
/// Uses "spn" for unified keyring with supernovae-cli.
const SERVICE_NAME: &str = "spn";

/// Keyring error types.
#[derive(Debug, Error)]
pub enum KeyringError {
    #[error("Failed to access keyring: {0}")]
    AccessError(String),
    #[error("Key not found for provider: {0}")]
    NotFound(String),
    #[error("Failed to store key: {0}")]
    StoreError(String),
    #[error("Failed to delete key: {0}")]
    DeleteError(String),
}

/// Keyring wrapper for spn API keys (unified with supernovae-cli).
pub struct SpnKeyring;

impl SpnKeyring {
    /// Get API key for a provider as zeroizing string.
    ///
    /// The returned string will be automatically zeroized when dropped.
    pub fn get(provider: &str) -> Result<Zeroizing<String>, KeyringError> {
        let entry = Entry::new(SERVICE_NAME, provider)
            .map_err(|e| KeyringError::AccessError(e.to_string()))?;

        let password = entry.get_password().map_err(|e| match e {
            keyring::Error::NoEntry => KeyringError::NotFound(provider.to_string()),
            _ => KeyringError::AccessError(e.to_string()),
        })?;

        Ok(Zeroizing::new(password))
    }

    /// Get API key wrapped in SecretString for maximum safety.
    pub fn get_secret(provider: &str) -> Result<SecretString, KeyringError> {
        let key = Self::get(provider)?;
        Ok(SecretString::from((*key).clone()))
    }

    /// Store API key for a provider.
    pub fn set(provider: &str, key: &str) -> Result<(), KeyringError> {
        let entry = Entry::new(SERVICE_NAME, provider)
            .map_err(|e| KeyringError::AccessError(e.to_string()))?;

        entry
            .set_password(key)
            .map_err(|e| KeyringError::StoreError(e.to_string()))
    }

    /// Delete API key for a provider.
    pub fn delete(provider: &str) -> Result<(), KeyringError> {
        let entry = Entry::new(SERVICE_NAME, provider)
            .map_err(|e| KeyringError::AccessError(e.to_string()))?;

        entry
            .delete_credential()
            .map_err(|e| KeyringError::DeleteError(e.to_string()))
    }

    /// Check if key exists for a provider.
    pub fn exists(provider: &str) -> bool {
        Self::get(provider).is_ok()
    }

    /// Get masked version of stored key.
    pub fn get_masked(provider: &str) -> Option<String> {
        Self::get(provider).ok().map(|k| mask_api_key(&k))
    }
}

/// Mask API key for display.
///
/// With spn-daemon feature: uses spn_core::mask_key (shows 7 chars + ••••••••)
/// Without: shows first 6 + ... + last 1 char
#[cfg(feature = "spn-daemon")]
pub fn mask_api_key(key: &str) -> String {
    spn_mask_key(key)
}

#[cfg(not(feature = "spn-daemon"))]
pub fn mask_api_key(key: &str) -> String {
    if key.len() <= 10 {
        return "****".to_string();
    }
    let prefix = &key[..6.min(key.len())];
    let suffix = &key[key.len().saturating_sub(1)..];
    format!("{}...{}", prefix, suffix)
}

/// Validate API key format (basic checks).
///
/// With spn-daemon feature: delegates to spn_core::validate_key_format
/// Without: uses local validation rules
#[cfg(feature = "spn-daemon")]
pub fn validate_key_format(provider: &str, key: &str) -> Result<(), String> {
    let result = spn_validate_key_format(provider, key);
    if result.is_valid() {
        Ok(())
    } else {
        Err(result.to_string())
    }
}

#[cfg(not(feature = "spn-daemon"))]
pub fn validate_key_format(provider: &str, key: &str) -> Result<(), String> {
    if key.trim().is_empty() {
        return Err("API key cannot be empty".into());
    }

    match provider {
        "anthropic" => {
            if !key.starts_with("sk-ant-") {
                return Err("Anthropic keys start with 'sk-ant-'".into());
            }
            if key.len() < 40 {
                return Err("Key seems too short".into());
            }
        }
        "openai" => {
            if !key.starts_with("sk-") {
                return Err("OpenAI keys start with 'sk-'".into());
            }
        }
        "mistral" | "groq" | "deepseek" => {
            if key.len() < 32 {
                return Err("Key seems too short".into());
            }
        }
        "ollama" => {
            // Ollama doesn't use API keys, but may use base URL
        }
        _ => {
            if key.len() < 10 {
                return Err("Key seems too short".into());
            }
        }
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// MIGRATION (env → keyring)
// ═══════════════════════════════════════════════════════════════════════════════

const MIGRATEABLE_PROVIDERS: &[&str] = &[
    "anthropic",
    "openai",
    "mistral",
    "groq",
    "deepseek",
    "gemini",
];

#[derive(Debug, Default)]
pub struct MigrationReport {
    pub migrated: usize,
    pub skipped: usize,
    pub not_found: Vec<String>,
    pub errors: Vec<(String, String)>,
}

impl MigrationReport {
    pub fn summary(&self) -> String {
        format!(
            "Migration complete: {} migrated, {} skipped, {} not found",
            self.migrated,
            self.skipped,
            self.not_found.len()
        )
    }
}

/// Migrate API keys from environment variables to system keychain.
pub fn migrate_env_to_keyring() -> MigrationReport {
    let mut report = MigrationReport::default();

    for provider in MIGRATEABLE_PROVIDERS {
        let env_var = provider_env_var(provider);

        match std::env::var(env_var) {
            Ok(key) if !key.is_empty() => {
                if SpnKeyring::exists(provider) {
                    println!(
                        "  ├── {}: Found → {}",
                        env_var,
                        "Already in keychain".yellow()
                    );
                    report.skipped += 1;
                    continue;
                }

                print!("  ├── {}: Found → Migrating... ", env_var);
                match SpnKeyring::set(provider, &key) {
                    Ok(()) => {
                        println!("{}", "✓".green());
                        report.migrated += 1;
                    }
                    Err(e) => {
                        println!("{} ({})", "✗".red(), e);
                        report.errors.push((provider.to_string(), e.to_string()));
                    }
                }
            }
            _ => {
                println!("  ├── {}: {}", env_var, "Not found".dimmed());
                report.not_found.push(provider.to_string());
            }
        }
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_name_is_spn() {
        assert_eq!(SERVICE_NAME, "spn");
    }

    #[test]
    fn test_mask_api_key_standard() {
        let key = "sk-ant-api03-abc123xyz789def456ghi";
        // spn-daemon uses spn_core::mask_key (7 chars + ••••••••)
        // without feature uses local format (6 chars + ... + 1)
        #[cfg(feature = "spn-daemon")]
        assert_eq!(mask_api_key(key), "sk-ant-••••••••");
        #[cfg(not(feature = "spn-daemon"))]
        assert_eq!(mask_api_key(key), "sk-ant...i");
    }

    #[test]
    fn test_mask_api_key_short() {
        #[cfg(feature = "spn-daemon")]
        {
            assert_eq!(mask_api_key("short"), "short••••••••");
            assert_eq!(mask_api_key("1234567890"), "1234567••••••••");
        }
        #[cfg(not(feature = "spn-daemon"))]
        {
            assert_eq!(mask_api_key("short"), "****");
            assert_eq!(mask_api_key("1234567890"), "****");
        }
    }

    #[test]
    fn test_mask_api_key_empty() {
        #[cfg(feature = "spn-daemon")]
        assert_eq!(mask_api_key(""), "••••••••");
        #[cfg(not(feature = "spn-daemon"))]
        assert_eq!(mask_api_key(""), "****");
    }

    #[test]
    fn test_validate_anthropic_key_valid() {
        let result =
            validate_key_format("anthropic", "sk-ant-api03-abcdefghijklmnopqrstuvwxyz123456");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_anthropic_key_wrong_prefix() {
        let result = validate_key_format("anthropic", "sk-wrong-prefix");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_openai_key_valid() {
        let result = validate_key_format("openai", "sk-proj-abcdefghijklmnop");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_empty_key_rejected() {
        let result = validate_key_format("anthropic", "");
        assert!(result.is_err());
    }

    #[test]
    fn test_provider_env_var() {
        assert_eq!(provider_env_var("anthropic"), "ANTHROPIC_API_KEY");
        assert_eq!(provider_env_var("openai"), "OPENAI_API_KEY");
        assert_eq!(provider_env_var("gemini"), "GEMINI_API_KEY");
    }
}
