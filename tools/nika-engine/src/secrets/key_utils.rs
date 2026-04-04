//! API key validation and masking utilities.
//!
//! Previously contained OS credential storage integration.
//! All secret storage is now handled by NikaVault (nika-core/src/vault.rs).
//! This module retains the shared utility functions used across the codebase.

use thiserror::Error;

/// Secret storage error types.
#[derive(Debug, Error)]
pub enum KeyringError {
    #[error("Failed to access vault: {0}")]
    AccessError(String),
    #[error("Key not found for provider: {0}")]
    NotFound(String),
    #[error("Failed to store key: {0}")]
    StoreError(String),
    #[error("Failed to delete key: {0}")]
    DeleteError(String),
}

// ═══════════════════════════════════════════════════════════════════════════════
// UTILITY FUNCTIONS
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
// MIGRATION REPORT (kept for API compatibility)
// ═══════════════════════════════════════════════════════════════════════════════

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

const MIGRATEABLE_PROVIDERS: &[&str] = &[
    "anthropic",
    "openai",
    "mistral",
    "groq",
    "deepseek",
    "gemini",
    "xai",
];

/// Migrate API keys from environment variables to NikaVault.
pub fn migrate_env_to_vault() -> MigrationReport {
    use crate::core::provider_to_env_var;
    use colored::Colorize;

    let nika_home = dirs::home_dir()
        .expect("home directory required")
        .join(".nika");
    let vault = nika_vault::NikaVault::new(&nika_home.join("secrets"));
    let mut report = MigrationReport::default();

    for provider in MIGRATEABLE_PROVIDERS {
        let env_var = provider_to_env_var(provider).unwrap_or("UNKNOWN_API_KEY");

        match std::env::var(env_var) {
            Ok(key) if !key.is_empty() => {
                // Check if already in vault
                if vault.get(provider).ok().flatten().is_some() {
                    println!("  ├── {}: Found → {}", env_var, "Already in vault".yellow());
                    report.skipped += 1;
                    continue;
                }

                print!("  ├── {}: Found → Migrating... ", env_var);
                match vault.set(provider, &key) {
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
    fn test_secret_error_display() {
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
}
