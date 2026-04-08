// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! TUI Configuration — nika.toml support (with .nika/config.toml legacy fallback)
//!
//! Provides persistent configuration for TUI preferences:
//! - Theme (dark, light, solarized)
//! - Mouse support
//! - Animations
//! - Provider/model defaults
//! - Studio settings

use nika_engine::util::fs::atomic_write;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

/// Config-related errors
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Failed to read config: {0}")]
    ReadError(#[from] std::io::Error),

    #[error("Failed to write config: {0}")]
    WriteError(std::io::Error),

    #[error("Failed to parse config: {0}")]
    ParseError(#[from] toml::de::Error),

    #[error("Failed to serialize config: {0}")]
    SerializeError(#[from] toml::ser::Error),

    #[error("Config directory not found")]
    NoDirFound,
}

/// Theme selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemeName {
    #[default]
    Dark,
    Light,
    Solarized,
}

impl ThemeName {
    pub fn label(&self) -> &'static str {
        match self {
            ThemeName::Dark => "Dark",
            ThemeName::Light => "Light",
            ThemeName::Solarized => "Solarized",
        }
    }

    pub fn cycle(&self) -> Self {
        match self {
            ThemeName::Dark => ThemeName::Light,
            ThemeName::Light => ThemeName::Solarized,
            ThemeName::Solarized => ThemeName::Dark,
        }
    }
}

/// TUI-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TuiSettings {
    /// Theme name
    pub theme: ThemeName,
    /// Enable mouse support
    pub mouse: bool,
    /// Enable animations (spinners, transitions)
    pub animations: bool,
    /// Show timestamps in messages
    pub show_timestamps: bool,
    /// Maximum messages to keep in history
    pub max_history: usize,
}

impl Default for TuiSettings {
    fn default() -> Self {
        Self {
            theme: ThemeName::Dark,
            mouse: true,
            animations: true,
            show_timestamps: false,
            max_history: 100,
        }
    }
}

/// Chat-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ChatSettings {
    /// Default provider (claude, openai, etc.)
    pub default_provider: Option<String>,
    /// Default model
    pub default_model: Option<String>,
    /// Show thinking/reasoning blocks
    pub show_thinking: bool,
    /// Enable deep thinking by default
    pub deep_thinking: bool,
    /// Auto-scroll to new messages
    pub auto_scroll: bool,
}

impl Default for ChatSettings {
    fn default() -> Self {
        Self {
            default_provider: None,
            default_model: None,
            show_thinking: true,
            deep_thinking: false,
            auto_scroll: true,
        }
    }
}

/// Studio editor configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StudioSettings {
    /// Auto-save enabled
    pub auto_save: bool,
    /// Auto-save interval in seconds
    pub auto_save_interval: u64,
    /// Tab width (spaces)
    pub tab_width: u8,
    /// Show line numbers
    pub line_numbers: bool,
    /// Highlight current line
    pub highlight_line: bool,
}

impl Default for StudioSettings {
    fn default() -> Self {
        Self {
            auto_save: true,
            auto_save_interval: 30,
            tab_width: 2,
            line_numbers: true,
            highlight_line: true,
        }
    }
}

/// Path configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PathSettings {
    /// Workflows directory
    pub workflows: PathBuf,
    /// Traces directory
    pub traces: PathBuf,
}

impl Default for PathSettings {
    fn default() -> Self {
        Self {
            workflows: PathBuf::from("."),
            traces: PathBuf::from(".nika/traces"),
        }
    }
}

/// Complete TUI configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TuiConfig {
    /// TUI settings
    pub tui: TuiSettings,
    /// Chat settings
    pub chat: ChatSettings,
    /// Studio settings
    pub studio: StudioSettings,
    /// Path settings
    pub paths: PathSettings,
}

impl TuiConfig {
    /// Load configuration from default path
    pub fn load() -> Result<Self, ConfigError> {
        let path = Self::config_path()?;
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            let config: TuiConfig = toml::from_str(&content)?;
            Ok(config)
        } else {
            Ok(Self::default())
        }
    }

    /// Load configuration, returning defaults on any error
    pub fn load_or_default() -> Self {
        Self::load().unwrap_or_default()
    }

    /// Save configuration to default path
    pub fn save(&self) -> Result<(), ConfigError> {
        let path = Self::config_path()?;
        let content = toml::to_string_pretty(self)?;
        atomic_write(&path, content.as_bytes()).map_err(ConfigError::WriteError)?;
        Ok(())
    }

    /// Get the configuration file path.
    ///
    /// Priority: nika.toml (new standard) > .nika/config.toml (legacy fallback)
    pub fn config_path() -> Result<PathBuf, ConfigError> {
        // Try nika.toml in current directory first (new standard)
        let nika_toml = PathBuf::from("nika.toml");
        if nika_toml.exists() {
            return Ok(nika_toml);
        }

        // Fallback: .nika/config.toml (legacy)
        let local = PathBuf::from(".nika/config.toml");
        if local.parent().map(|p| p.exists()).unwrap_or(false) {
            return Ok(local);
        }

        // Try to create .nika directory for TUI state
        let nika_dir = PathBuf::from(".nika");
        if !nika_dir.exists() {
            std::fs::create_dir_all(&nika_dir).ok();
        }

        if nika_dir.exists() {
            // Return nika.toml as the preferred path for new configs
            return Ok(PathBuf::from("nika.toml"));
        }

        Err(ConfigError::NoDirFound)
    }

    /// Check if config file exists
    pub fn exists() -> bool {
        Self::config_path().map(|p| p.exists()).unwrap_or(false)
    }

    /// Initialize default config file if it doesn't exist
    pub fn init_default() -> Result<bool, ConfigError> {
        let path = Self::config_path()?;
        if path.exists() {
            return Ok(false); // Already exists
        }

        let config = Self::default();
        config.save()?;
        Ok(true) // Created new
    }

    /// Get theme name
    pub fn theme(&self) -> ThemeName {
        self.tui.theme
    }

    /// Set theme
    pub fn set_theme(&mut self, theme: ThemeName) {
        self.tui.theme = theme;
    }

    /// Cycle to next theme
    pub fn cycle_theme(&mut self) {
        self.tui.theme = self.tui.theme.cycle();
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_name_cycle() {
        assert_eq!(ThemeName::Dark.cycle(), ThemeName::Light);
        assert_eq!(ThemeName::Light.cycle(), ThemeName::Solarized);
        assert_eq!(ThemeName::Solarized.cycle(), ThemeName::Dark);
    }

    #[test]
    fn test_theme_name_label() {
        assert_eq!(ThemeName::Dark.label(), "Dark");
        assert_eq!(ThemeName::Light.label(), "Light");
        assert_eq!(ThemeName::Solarized.label(), "Solarized");
    }

    #[test]
    fn test_config_defaults() {
        let config = TuiConfig::default();

        assert_eq!(config.tui.theme, ThemeName::Dark);
        assert!(config.tui.mouse);
        assert!(config.tui.animations);
        assert_eq!(config.tui.max_history, 100);

        assert!(config.chat.show_thinking);
        assert!(!config.chat.deep_thinking);
        assert!(config.chat.auto_scroll);

        assert!(config.studio.auto_save);
        assert_eq!(config.studio.auto_save_interval, 30);
        assert_eq!(config.studio.tab_width, 2);
    }

    #[test]
    fn test_config_serialize_deserialize() {
        let config = TuiConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();

        // Should contain expected sections
        assert!(toml_str.contains("[tui]"));
        assert!(toml_str.contains("[chat]"));
        assert!(toml_str.contains("[studio]"));
        assert!(toml_str.contains("[paths]"));

        // Should roundtrip
        let parsed: TuiConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.tui.theme, ThemeName::Dark);
    }

    #[test]
    fn test_config_parse_partial() {
        // Should handle partial config with defaults
        let partial = r#"
[tui]
theme = "light"

[chat]
show_thinking = false
"#;
        let config: TuiConfig = toml::from_str(partial).unwrap();
        assert_eq!(config.tui.theme, ThemeName::Light);
        assert!(config.tui.mouse); // Default
        assert!(!config.chat.show_thinking); // Overridden
        assert!(config.studio.auto_save); // Default
    }

    #[test]
    fn test_config_set_theme() {
        let mut config = TuiConfig::default();
        assert_eq!(config.theme(), ThemeName::Dark);

        config.set_theme(ThemeName::Solarized);
        assert_eq!(config.theme(), ThemeName::Solarized);

        config.cycle_theme();
        assert_eq!(config.theme(), ThemeName::Dark);
    }

    #[test]
    fn test_tui_settings_default() {
        let settings = TuiSettings::default();
        assert_eq!(settings.theme, ThemeName::Dark);
        assert!(settings.mouse);
        assert!(settings.animations);
    }

    #[test]
    fn test_chat_settings_default() {
        let settings = ChatSettings::default();
        assert!(settings.default_provider.is_none());
        assert!(settings.default_model.is_none());
        assert!(settings.show_thinking);
    }

    #[test]
    fn test_studio_settings_default() {
        let settings = StudioSettings::default();
        assert!(settings.auto_save);
        assert_eq!(settings.auto_save_interval, 30);
        assert_eq!(settings.tab_width, 2);
        assert!(settings.line_numbers);
    }

    #[test]
    fn test_path_settings_default() {
        let settings = PathSettings::default();
        assert_eq!(settings.workflows, PathBuf::from("."));
        assert_eq!(settings.traces, PathBuf::from(".nika/traces"));
    }
}
