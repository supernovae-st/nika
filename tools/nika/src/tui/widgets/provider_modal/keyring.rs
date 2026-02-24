//! Secure API key storage via system keychain
//!
//! Uses keyring-rs for cross-platform credential storage:
//! - macOS: Keychain Access
//! - Windows: Credential Locker
//! - Linux: Secret Service (GNOME Keyring, KWallet)

use keyring::Entry;
use thiserror::Error;

/// Service name for keyring entries
const SERVICE_NAME: &str = "nika";

/// Keyring error types
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

/// Keyring wrapper for Nika API keys
pub struct NikaKeyring;

impl NikaKeyring {
    /// Get API key for a provider
    pub fn get(provider: &str) -> Result<String, KeyringError> {
        let entry = Entry::new(SERVICE_NAME, provider)
            .map_err(|e| KeyringError::AccessError(e.to_string()))?;

        entry.get_password().map_err(|e| match e {
            keyring::Error::NoEntry => KeyringError::NotFound(provider.to_string()),
            _ => KeyringError::AccessError(e.to_string()),
        })
    }

    /// Store API key for a provider
    pub fn set(provider: &str, key: &str) -> Result<(), KeyringError> {
        let entry = Entry::new(SERVICE_NAME, provider)
            .map_err(|e| KeyringError::AccessError(e.to_string()))?;

        entry
            .set_password(key)
            .map_err(|e| KeyringError::StoreError(e.to_string()))
    }

    /// Delete API key for a provider
    pub fn delete(provider: &str) -> Result<(), KeyringError> {
        let entry = Entry::new(SERVICE_NAME, provider)
            .map_err(|e| KeyringError::AccessError(e.to_string()))?;

        entry
            .delete_credential()
            .map_err(|e| KeyringError::DeleteError(e.to_string()))
    }

    /// Check if key exists for a provider
    pub fn exists(provider: &str) -> bool {
        Self::get(provider).is_ok()
    }

    /// Get masked version of stored key
    pub fn get_masked(provider: &str) -> Option<String> {
        Self::get(provider).ok().map(|k| mask_api_key(&k))
    }
}

/// Mask API key for display (show first 6 and last 1 char)
pub fn mask_api_key(key: &str) -> String {
    if key.len() <= 10 {
        return "****".to_string();
    }
    let prefix = &key[..6.min(key.len())];
    let suffix = &key[key.len().saturating_sub(1)..];
    format!("{}...{}", prefix, suffix)
}

/// Validate API key format (basic checks)
pub fn validate_key_format(provider: &str, key: &str) -> Result<(), String> {
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
        "mistral" => {
            if key.len() < 32 {
                return Err("Key seems too short".into());
            }
        }
        _ => {}
    }
    Ok(())
}

/// Get environment variable name for provider
pub fn provider_env_var(provider: &str) -> &'static str {
    match provider {
        "anthropic" => "ANTHROPIC_API_KEY",
        "openai" => "OPENAI_API_KEY",
        "mistral" => "MISTRAL_API_KEY",
        "groq" => "GROQ_API_KEY",
        "deepseek" => "DEEPSEEK_API_KEY",
        "ollama" => "OLLAMA_API_BASE_URL",
        _ => "UNKNOWN_API_KEY",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_api_key_standard() {
        let key = "sk-ant-api03-abc123xyz789def456ghi";
        assert_eq!(mask_api_key(key), "sk-ant...i");
    }

    #[test]
    fn test_mask_api_key_short() {
        assert_eq!(mask_api_key("short"), "****");
        assert_eq!(mask_api_key("1234567890"), "****");
    }

    #[test]
    fn test_mask_api_key_empty() {
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
        assert!(result.unwrap_err().contains("sk-ant-"));
    }

    #[test]
    fn test_validate_openai_key_valid() {
        let result = validate_key_format("openai", "sk-proj-abcdefghijklmnop");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_openai_key_wrong_prefix() {
        let result = validate_key_format("openai", "wrong-key");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_unknown_provider_accepts_any() {
        let result = validate_key_format("unknown", "any-key-format");
        assert!(result.is_ok());
    }

    #[test]
    fn test_provider_env_var() {
        assert_eq!(provider_env_var("anthropic"), "ANTHROPIC_API_KEY");
        assert_eq!(provider_env_var("openai"), "OPENAI_API_KEY");
        assert_eq!(provider_env_var("mistral"), "MISTRAL_API_KEY");
        assert_eq!(provider_env_var("groq"), "GROQ_API_KEY");
        assert_eq!(provider_env_var("deepseek"), "DEEPSEEK_API_KEY");
    }
}
