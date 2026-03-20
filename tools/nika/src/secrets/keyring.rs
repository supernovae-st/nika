//! Secure API key storage via system keychain.
//!
//! Uses keyring-rs for cross-platform credential storage:
//! - macOS: Keychain Access
//! - Windows: Credential Manager
//! - Linux: Secret Service (GNOME Keyring, KWallet)
//!
//! ## native-keychain Feature
//!
//! The keyring crate is optional (via `native-keychain` feature).
//! Docker builds disable it since containers don't have OS keychains.
//! When disabled, fallback implementations return errors for keyring operations.
//!

use secrecy::SecretString;
use thiserror::Error;
use zeroize::Zeroizing;

// Imports used only with native-keychain feature
#[cfg(feature = "native-keychain")]
use colored::Colorize;

// Use nika::core directly instead of tui::providers::env_var
#[cfg(feature = "native-keychain")]
use crate::core::provider_to_env_var;

/// Service name for keyring entries.
const SERVICE_NAME: &str = "nika";

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

// ═══════════════════════════════════════════════════════════════════════════════
// NATIVE KEYCHAIN IMPLEMENTATION (requires OS keychain access)
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(feature = "native-keychain")]
mod native {
    use super::*;
    use keyring::Entry;

    /// Keyring wrapper for Nika API keys.
    pub struct NikaKeyring;

    impl NikaKeyring {
        /// Get API key for a provider as zeroizing string.
        ///
        /// The returned string will be automatically zeroized when dropped.
        ///
        /// Returns NotFound error if NIKA_SKIP_KEYCHAIN is set (for CI/testing).
        pub fn get(provider: &str) -> Result<Zeroizing<String>, KeyringError> {
            // Skip keychain access if NIKA_SKIP_KEYCHAIN is truthy
            if super::should_skip_keychain() {
                return Err(KeyringError::NotFound(format!(
                    "{} (keychain skipped via NIKA_SKIP_KEYCHAIN)",
                    provider
                )));
            }

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
        ///
        /// Guarded: returns error in test mode or when NIKA_SKIP_KEYCHAIN is set.
        pub fn set(provider: &str, key: &str) -> Result<(), KeyringError> {
            if cfg!(test) || super::should_skip_keychain() {
                return Err(KeyringError::StoreError(format!(
                    "{} (keychain skipped)",
                    provider
                )));
            }

            let entry = Entry::new(SERVICE_NAME, provider)
                .map_err(|e| KeyringError::AccessError(e.to_string()))?;

            entry
                .set_password(key)
                .map_err(|e| KeyringError::StoreError(e.to_string()))
        }

        /// Delete API key for a provider.
        ///
        /// Guarded: returns error in test mode or when NIKA_SKIP_KEYCHAIN is set.
        pub fn delete(provider: &str) -> Result<(), KeyringError> {
            if cfg!(test) || super::should_skip_keychain() {
                return Err(KeyringError::DeleteError(format!(
                    "{} (keychain skipped)",
                    provider
                )));
            }

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
        ///
        /// Returns None if NIKA_SKIP_KEYCHAIN is set (for CI/testing).
        pub fn get_masked(provider: &str) -> Option<String> {
            // Skip keychain access if NIKA_SKIP_KEYCHAIN is truthy
            if super::should_skip_keychain() {
                return None;
            }
            Self::get(provider).ok().map(|k| super::mask_api_key(&k))
        }
    }
}

/// Check if keychain access should be skipped.
///
/// Returns `true` in any of these cases:
/// - Binary was compiled in test mode (`cfg!(test)`)
/// - `NIKA_SKIP_KEYCHAIN` env var is truthy ("1", "true", "yes")
///
/// This prevents macOS Keychain popup storms during development.
/// Each `cargo build`/`cargo test` produces a binary with a new CDHash,
/// causing macOS to re-prompt for keychain access every time.
pub fn should_skip_keychain() -> bool {
    cfg!(test)
        || std::env::var("NIKA_SKIP_KEYCHAIN")
            .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
            .unwrap_or(false)
}

// ═══════════════════════════════════════════════════════════════════════════════
// STUB IMPLEMENTATION (when native-keychain feature is disabled)
// Used in Docker builds where OS keychain is not available.
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(not(feature = "native-keychain"))]
mod stub {
    use super::*;

    /// Stub keyring for environments without OS keychain (Docker, CI, etc.).
    /// All operations return errors - use environment variables instead.
    pub struct NikaKeyring;

    impl NikaKeyring {
        /// Always returns NotFound error (no keychain access in Docker).
        pub fn get(_provider: &str) -> Result<Zeroizing<String>, KeyringError> {
            Err(KeyringError::AccessError(
                "Keychain not available (native-keychain feature disabled)".into(),
            ))
        }

        /// Always returns error (no keychain access).
        pub fn get_secret(_provider: &str) -> Result<SecretString, KeyringError> {
            Err(KeyringError::AccessError(
                "Keychain not available (native-keychain feature disabled)".into(),
            ))
        }

        /// Always returns error (no keychain access).
        pub fn set(_provider: &str, _key: &str) -> Result<(), KeyringError> {
            Err(KeyringError::StoreError(
                "Keychain not available (native-keychain feature disabled)".into(),
            ))
        }

        /// Always returns error (no keychain access).
        pub fn delete(_provider: &str) -> Result<(), KeyringError> {
            Err(KeyringError::DeleteError(
                "Keychain not available (native-keychain feature disabled)".into(),
            ))
        }

        /// Always returns false (no keychain access).
        pub fn exists(_provider: &str) -> bool {
            false
        }

        /// Always returns None (no keychain access).
        pub fn get_masked(_provider: &str) -> Option<String> {
            None
        }
    }
}

// Re-export the appropriate implementation
#[cfg(feature = "native-keychain")]
pub use native::NikaKeyring;

#[cfg(not(feature = "native-keychain"))]
pub use stub::NikaKeyring;

// ═══════════════════════════════════════════════════════════════════════════════
// UTILITY FUNCTIONS (always available)
// ═══════════════════════════════════════════════════════════════════════════════

/// Mask API key for display.
///
/// Shows first 8 chars + "..." for keys longer than 8 chars.
pub fn mask_api_key(key: &str) -> String {
    if key.len() > 8 {
        format!("{}...", &key[..8])
    } else {
        "***".to_string()
    }
}

/// Validate API key format using nika::core provider definitions.
///
/// Returns Ok(()) if valid, Err(reason) if invalid.
pub fn validate_key_format(provider: &str, key: &str) -> Result<(), String> {
    use crate::core::{find_provider, validate_key_format as core_validate};

    // Empty key is always invalid
    if key.is_empty() {
        return Err("API key cannot be empty".to_string());
    }

    // Look up the provider
    let Some(prov) = find_provider(provider) else {
        // Unknown provider - accept any key format
        return Ok(());
    };

    // Validate key format against provider's prefix requirement
    if core_validate(prov, key) {
        Ok(())
    } else {
        Err(format!(
            "Invalid API key format for {}. Expected prefix: {}",
            provider,
            prov.key_prefix.unwrap_or("(any)")
        ))
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// MIGRATION (env → keyring) - Only available with native-keychain
// ═══════════════════════════════════════════════════════════════════════════════

const MIGRATEABLE_PROVIDERS: &[&str] = &[
    "anthropic",
    "openai",
    "mistral",
    "groq",
    "deepseek",
    "gemini",
    "xai",
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
///
/// With native-keychain: Actually migrates keys to OS keychain
/// Without: Returns report indicating migration not available
#[cfg(feature = "native-keychain")]
pub fn migrate_env_to_keyring() -> MigrationReport {
    let mut report = MigrationReport::default();

    for provider in MIGRATEABLE_PROVIDERS {
        // Use core::provider_to_env_var directly instead of tui::providers::env_var
        let env_var = provider_to_env_var(provider).unwrap_or("UNKNOWN_API_KEY");

        match std::env::var(env_var) {
            Ok(key) if !key.is_empty() => {
                if NikaKeyring::exists(provider) {
                    println!(
                        "  ├── {}: Found → {}",
                        env_var,
                        "Already in keychain".yellow()
                    );
                    report.skipped += 1;
                    continue;
                }

                print!("  ├── {}: Found → Migrating... ", env_var);
                match NikaKeyring::set(provider, &key) {
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

#[cfg(not(feature = "native-keychain"))]
pub fn migrate_env_to_keyring() -> MigrationReport {
    println!("  ⚠ Migration not available (native-keychain feature disabled)");
    println!("  ⚠ Use environment variables instead in Docker/container environments");
    MigrationReport {
        not_found: MIGRATEABLE_PROVIDERS
            .iter()
            .map(|s| s.to_string())
            .collect(),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_name_is_nika() {
        assert_eq!(SERVICE_NAME, "nika");
    }

    #[test]
    fn test_mask_api_key_standard() {
        let key = "sk-ant-api03-abc123xyz789def456ghi";
        assert_eq!(mask_api_key(key), "sk-ant-a...");
    }

    #[test]
    fn test_mask_api_key_short() {
        assert_eq!(mask_api_key("short"), "***");
        assert_eq!(mask_api_key("12345678"), "***");
    }

    #[test]
    fn test_mask_api_key_boundary() {
        assert_eq!(mask_api_key("123456789"), "12345678...");
    }

    #[test]
    fn test_mask_api_key_empty() {
        assert_eq!(mask_api_key(""), "***");
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
    fn test_keyring_error_display() {
        let err = KeyringError::NotFound("anthropic".to_string());
        assert!(err.to_string().contains("anthropic"));
    }

    #[test]
    fn test_migration_report_summary() {
        let report = MigrationReport {
            migrated: 2,
            skipped: 1,
            not_found: vec!["groq".into()],
            errors: vec![],
        };
        let summary = report.summary();
        assert!(summary.contains("2 migrated"));
        assert!(summary.contains("1 skipped"));
    }

    // ─── Bug 34: set/delete must be guarded in test mode ──────────────

    #[test]
    fn test_keyring_set_guarded_in_test_mode() {
        // In cfg!(test), NikaKeyring::set() must return an error
        // to prevent accidental keychain access during tests
        let result = NikaKeyring::set("test_provider", "test-key-value");
        assert!(
            result.is_err(),
            "NikaKeyring::set() must be guarded in test mode"
        );
    }

    #[test]
    fn test_keyring_delete_guarded_in_test_mode() {
        // In cfg!(test), NikaKeyring::delete() must return an error
        let result = NikaKeyring::delete("test_provider");
        assert!(
            result.is_err(),
            "NikaKeyring::delete() must be guarded in test mode"
        );
    }

    // ─── Bug 35: MIGRATEABLE_PROVIDERS must include xai ─────────────

    #[test]
    fn test_migrateable_providers_includes_xai() {
        assert!(
            MIGRATEABLE_PROVIDERS.contains(&"xai"),
            "MIGRATEABLE_PROVIDERS must include xai, got: {:?}",
            MIGRATEABLE_PROVIDERS
        );
    }

    #[test]
    fn test_migrateable_providers_count() {
        assert_eq!(
            MIGRATEABLE_PROVIDERS.len(),
            7,
            "Expected 7 migrateable providers (all LLM), got: {:?}",
            MIGRATEABLE_PROVIDERS
        );
    }

    // Tests that are only relevant with native-keychain
    // These tests are #[ignore] by default to avoid macOS Keychain popups.
    // Run explicitly with: cargo test -- --ignored
    #[cfg(feature = "native-keychain")]
    mod native_tests {
        use super::*;

        #[test]
        #[ignore = "Requires real OS keychain access — causes macOS popup"]
        fn test_nika_keyring_not_found() {
            // Test that querying a non-existent key returns NotFound
            let result = NikaKeyring::get("nonexistent_provider_test_xyz");
            assert!(matches!(
                result,
                Err(KeyringError::NotFound(_)) | Err(KeyringError::AccessError(_))
            ));
        }
    }

    // Tests for stub implementation
    #[cfg(not(feature = "native-keychain"))]
    mod stub_tests {
        use super::*;

        #[test]
        fn test_stub_get_returns_error() {
            let result = NikaKeyring::get("anthropic");
            assert!(result.is_err());
        }

        #[test]
        fn test_stub_exists_returns_false() {
            assert!(!NikaKeyring::exists("anthropic"));
        }

        #[test]
        fn test_stub_set_returns_error() {
            let result = NikaKeyring::set("anthropic", "test-key");
            assert!(result.is_err());
        }
    }
}
