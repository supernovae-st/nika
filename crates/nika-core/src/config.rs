//! Nika Configuration Module
//!
//! Manages persistent configuration for API keys and defaults.
//! Config is stored in `~/.config/nika/config.toml`.
//!
//! ## Priority Order (highest to lowest)
//!
//! 1. Environment variables (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`)
//! 2. Config file (`~/.config/nika/config.toml`)
//! 3. Defaults

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{NikaError, Result};

/// Main configuration structure
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct NikaConfig {
    /// API keys for LLM providers
    #[serde(default)]
    pub api_keys: ApiKeys,

    /// Default provider and model settings
    #[serde(default)]
    pub defaults: Defaults,
}

/// API keys configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ApiKeys {
    /// Anthropic API key (sk-ant-...)
    pub anthropic: Option<String>,

    /// OpenAI API key (sk-proj-... or sk-...)
    pub openai: Option<String>,
}

/// Default settings
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Defaults {
    /// Default provider (claude, openai)
    pub provider: Option<String>,

    /// Default model (claude-sonnet-4-20250514, gpt-4o, etc.)
    pub model: Option<String>,
}

impl NikaConfig {
    /// Get the config directory path
    ///
    /// Returns `~/.config/nika/` on Unix, `%APPDATA%/nika/` on Windows
    pub fn config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("nika")
    }

    /// Get the config file path
    ///
    /// Returns `~/.config/nika/config.toml`
    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    /// Load configuration from file
    ///
    /// Returns default config if file doesn't exist.
    /// Returns error if file exists but is malformed.
    pub fn load() -> Result<Self> {
        let path = Self::config_path();

        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&path).map_err(|e| NikaError::ConfigError {
            reason: format!("Failed to read config file: {}", e),
        })?;

        toml::from_str(&content).map_err(|e| NikaError::ConfigError {
            reason: format!("Failed to parse config file: {}", e),
        })
    }

    /// Save configuration to file
    ///
    /// Creates the config directory if it doesn't exist.
    pub fn save(&self) -> Result<()> {
        let dir = Self::config_dir();
        let path = Self::config_path();

        // Create directory if needed
        if !dir.exists() {
            fs::create_dir_all(&dir).map_err(|e| NikaError::ConfigError {
                reason: format!("Failed to create config directory: {}", e),
            })?;
        }

        // Serialize to TOML
        let content = toml::to_string_pretty(self).map_err(|e| NikaError::ConfigError {
            reason: format!("Failed to serialize config: {}", e),
        })?;

        // Write file
        fs::write(&path, content).map_err(|e| NikaError::ConfigError {
            reason: format!("Failed to write config file: {}", e),
        })?;

        Ok(())
    }

    /// Merge with environment variables
    ///
    /// Environment variables take precedence over config file values.
    pub fn with_env(mut self) -> Self {
        // Check for Anthropic key in env
        if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            if !key.is_empty() {
                self.api_keys.anthropic = Some(key);
            }
        }

        // Check for OpenAI key in env
        if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            if !key.is_empty() {
                self.api_keys.openai = Some(key);
            }
        }

        self
    }

    /// Get effective Anthropic API key
    ///
    /// Returns key from config (env vars should be merged first via `with_env()`)
    pub fn anthropic_key(&self) -> Option<&str> {
        self.api_keys.anthropic.as_deref()
    }

    /// Get effective OpenAI API key
    pub fn openai_key(&self) -> Option<&str> {
        self.api_keys.openai.as_deref()
    }

    /// Check if any API key is configured
    pub fn has_any_key(&self) -> bool {
        self.api_keys.anthropic.is_some() || self.api_keys.openai.is_some()
    }

    /// Get default provider (or auto-detect from available keys)
    pub fn default_provider(&self) -> Option<&str> {
        self.defaults.provider.as_deref().or_else(|| {
            // Auto-detect based on available keys
            if self.api_keys.anthropic.is_some() {
                Some("claude")
            } else if self.api_keys.openai.is_some() {
                Some("openai")
            } else {
                None
            }
        })
    }

    /// Get default model for provider
    pub fn default_model(&self) -> Option<&str> {
        self.defaults.model.as_deref()
    }
}

/// Mask an API key for display
///
/// Shows first N chars + asterisks, e.g. "sk-ant-api03-***"
pub fn mask_api_key(key: &str, visible_chars: usize) -> String {
    if key.is_empty() {
        return String::new();
    }

    let visible = key.len().min(visible_chars);
    format!("{}***", &key[..visible])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use tempfile::TempDir;

    #[test]
    fn test_config_path_contains_nika() {
        let path = NikaConfig::config_path();
        assert!(path.to_string_lossy().contains("nika"));
        assert!(path.to_string_lossy().ends_with("config.toml"));
    }

    #[test]
    fn test_config_dir_is_parent_of_config_path() {
        let dir = NikaConfig::config_dir();
        let path = NikaConfig::config_path();
        assert_eq!(path.parent().unwrap(), dir);
    }

    #[test]
    fn test_default_config_is_empty() {
        let config = NikaConfig::default();
        assert!(config.api_keys.anthropic.is_none());
        assert!(config.api_keys.openai.is_none());
        assert!(config.defaults.provider.is_none());
        assert!(config.defaults.model.is_none());
    }

    #[test]
    fn test_config_save_and_load_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        let config = NikaConfig {
            api_keys: ApiKeys {
                anthropic: Some("sk-ant-test-key".into()),
                openai: Some("sk-openai-test".into()),
            },
            defaults: Defaults {
                provider: Some("claude".into()),
                model: Some("claude-sonnet-4-20250514".into()),
            },
        };

        // Manually save to temp path
        let content = toml::to_string_pretty(&config).unwrap();
        fs::write(&config_path, &content).unwrap();

        // Load from temp path
        let loaded_content = fs::read_to_string(&config_path).unwrap();
        let loaded: NikaConfig = toml::from_str(&loaded_content).unwrap();

        assert_eq!(config, loaded);
    }

    #[test]
    fn test_env_overrides_config() {
        // Set env var
        env::set_var("ANTHROPIC_API_KEY", "sk-ant-from-env");

        let config = NikaConfig {
            api_keys: ApiKeys {
                anthropic: Some("sk-ant-from-config".into()),
                openai: None,
            },
            ..Default::default()
        }
        .with_env();

        // Env should override config
        assert_eq!(config.anthropic_key(), Some("sk-ant-from-env"));

        // Cleanup
        env::remove_var("ANTHROPIC_API_KEY");
    }

    #[test]
    fn test_env_does_not_override_with_empty() {
        env::set_var("OPENAI_API_KEY", "");

        let config = NikaConfig {
            api_keys: ApiKeys {
                anthropic: None,
                openai: Some("sk-from-config".into()),
            },
            ..Default::default()
        }
        .with_env();

        // Empty env should not override
        assert_eq!(config.openai_key(), Some("sk-from-config"));

        env::remove_var("OPENAI_API_KEY");
    }

    #[test]
    fn test_has_any_key() {
        let empty = NikaConfig::default();
        assert!(!empty.has_any_key());

        let with_anthropic = NikaConfig {
            api_keys: ApiKeys {
                anthropic: Some("key".into()),
                openai: None,
            },
            ..Default::default()
        };
        assert!(with_anthropic.has_any_key());

        let with_openai = NikaConfig {
            api_keys: ApiKeys {
                anthropic: None,
                openai: Some("key".into()),
            },
            ..Default::default()
        };
        assert!(with_openai.has_any_key());
    }

    #[test]
    fn test_default_provider_autodetect() {
        // No keys = no provider
        let empty = NikaConfig::default();
        assert!(empty.default_provider().is_none());

        // Anthropic key = claude provider
        let anthropic = NikaConfig {
            api_keys: ApiKeys {
                anthropic: Some("key".into()),
                openai: None,
            },
            ..Default::default()
        };
        assert_eq!(anthropic.default_provider(), Some("claude"));

        // OpenAI key = openai provider
        let openai = NikaConfig {
            api_keys: ApiKeys {
                anthropic: None,
                openai: Some("key".into()),
            },
            ..Default::default()
        };
        assert_eq!(openai.default_provider(), Some("openai"));

        // Explicit provider overrides auto-detect
        let explicit = NikaConfig {
            api_keys: ApiKeys {
                anthropic: Some("key".into()),
                openai: Some("key".into()),
            },
            defaults: Defaults {
                provider: Some("openai".into()),
                model: None,
            },
        };
        assert_eq!(explicit.default_provider(), Some("openai"));
    }

    #[test]
    fn test_mask_api_key() {
        assert_eq!(
            mask_api_key("sk-ant-api03-abcdefghij", 12),
            "sk-ant-api03***"
        );
        assert_eq!(mask_api_key("sk-proj-abc", 7), "sk-proj***");
        assert_eq!(mask_api_key("short", 10), "short***"); // Key shorter than visible chars
        assert_eq!(mask_api_key("", 10), "");
    }

    #[test]
    fn test_toml_format() {
        let config = NikaConfig {
            api_keys: ApiKeys {
                anthropic: Some("sk-ant-test".into()),
                openai: None,
            },
            defaults: Defaults {
                provider: Some("claude".into()),
                model: None,
            },
        };

        let toml_str = toml::to_string_pretty(&config).unwrap();

        // Should contain expected sections
        assert!(toml_str.contains("[api_keys]"));
        assert!(toml_str.contains("anthropic = \"sk-ant-test\""));
        assert!(toml_str.contains("[defaults]"));
        assert!(toml_str.contains("provider = \"claude\""));
    }

    #[test]
    fn test_load_nonexistent_file_returns_default() {
        // This test uses the actual config path, so we save/restore if it exists
        let path = NikaConfig::config_path();
        let backup = if path.exists() {
            Some(fs::read_to_string(&path).unwrap())
        } else {
            None
        };

        // Remove file if it exists
        if path.exists() {
            fs::remove_file(&path).unwrap();
        }

        // Load should return default
        let config = NikaConfig::load().unwrap();
        assert_eq!(config, NikaConfig::default());

        // Restore backup if needed
        if let Some(content) = backup {
            fs::create_dir_all(path.parent().unwrap()).ok();
            fs::write(&path, content).unwrap();
        }
    }
}
