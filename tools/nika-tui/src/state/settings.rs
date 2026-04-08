// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Settings Overlay State
//!
//! Contains the state management for the settings overlay, including
//! field navigation, editing, and configuration persistence.

use nika_engine::config::{mask_api_key, NikaConfig};

/// Settings overlay field identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsField {
    /// Anthropic API key field
    #[default]
    AnthropicKey,
    /// OpenAI API key field
    OpenAiKey,
    /// Default provider selector
    Provider,
    /// Default model selector
    Model,
}

impl SettingsField {
    /// Get all fields in order
    pub fn all() -> &'static [SettingsField] {
        &[
            SettingsField::AnthropicKey,
            SettingsField::OpenAiKey,
            SettingsField::Provider,
            SettingsField::Model,
        ]
    }

    /// Get next field (wrapping)
    pub fn next(&self) -> SettingsField {
        match self {
            SettingsField::AnthropicKey => SettingsField::OpenAiKey,
            SettingsField::OpenAiKey => SettingsField::Provider,
            SettingsField::Provider => SettingsField::Model,
            SettingsField::Model => SettingsField::AnthropicKey,
        }
    }

    /// Get previous field (wrapping)
    pub fn prev(&self) -> SettingsField {
        match self {
            SettingsField::AnthropicKey => SettingsField::Model,
            SettingsField::OpenAiKey => SettingsField::AnthropicKey,
            SettingsField::Provider => SettingsField::OpenAiKey,
            SettingsField::Model => SettingsField::Provider,
        }
    }

    /// Get field label for display
    pub fn label(&self) -> &'static str {
        match self {
            SettingsField::AnthropicKey => "Anthropic API Key",
            SettingsField::OpenAiKey => "OpenAI API Key",
            SettingsField::Provider => "Default Provider",
            SettingsField::Model => "Default Model",
        }
    }
}

/// Settings overlay state
#[derive(Debug, Clone, Default)]
pub struct SettingsState {
    /// Currently focused field
    pub focus: SettingsField,
    /// Edit mode active (typing in field)
    pub editing: bool,
    /// Input buffer for current field
    pub input_buffer: String,
    /// Cursor position in input buffer
    pub cursor: usize,
    /// Loaded configuration
    pub config: NikaConfig,
    /// Has unsaved changes
    pub dirty: bool,
    /// Status message (success/error feedback)
    pub status_message: Option<String>,
}

impl SettingsState {
    /// Create settings state with loaded config
    pub fn new(config: NikaConfig) -> Self {
        Self {
            config,
            ..Default::default()
        }
    }

    /// Focus next field
    pub fn focus_next(&mut self) {
        self.focus = self.focus.next();
        self.editing = false;
        self.input_buffer.clear();
        self.cursor = 0;
    }

    /// Focus previous field
    pub fn focus_prev(&mut self) {
        self.focus = self.focus.prev();
        self.editing = false;
        self.input_buffer.clear();
        self.cursor = 0;
    }

    /// Enter edit mode for current field
    pub fn start_edit(&mut self) {
        self.editing = true;
        // Load current value into buffer
        self.input_buffer = match self.focus {
            SettingsField::AnthropicKey => {
                self.config.api_keys.anthropic.clone().unwrap_or_default()
            }
            SettingsField::OpenAiKey => self.config.api_keys.openai.clone().unwrap_or_default(),
            SettingsField::Provider => self.config.defaults.provider.clone().unwrap_or_default(),
            SettingsField::Model => self.config.defaults.model.clone().unwrap_or_default(),
        };
        self.cursor = self.input_buffer.len();
    }

    /// Cancel edit mode, discard changes
    pub fn cancel_edit(&mut self) {
        self.editing = false;
        self.input_buffer.clear();
        self.cursor = 0;
    }

    /// Confirm edit, apply to config
    pub fn confirm_edit(&mut self) {
        if !self.editing {
            return;
        }

        let value = if self.input_buffer.is_empty() {
            None
        } else {
            Some(self.input_buffer.clone())
        };

        match self.focus {
            SettingsField::AnthropicKey => {
                self.config.api_keys.anthropic = value;
            }
            SettingsField::OpenAiKey => {
                self.config.api_keys.openai = value;
            }
            SettingsField::Provider => {
                self.config.defaults.provider = value;
            }
            SettingsField::Model => {
                self.config.defaults.model = value;
            }
        }

        self.dirty = true;
        self.editing = false;
        self.input_buffer.clear();
        self.cursor = 0;
    }

    /// Insert character at cursor
    pub fn insert_char(&mut self, c: char) {
        if !self.editing {
            return;
        }
        self.input_buffer.insert(self.cursor, c);
        self.cursor += 1;
    }

    /// Delete character before cursor
    pub fn backspace(&mut self) {
        if !self.editing || self.cursor == 0 {
            return;
        }
        self.cursor -= 1;
        self.input_buffer.remove(self.cursor);
    }

    /// Delete character at cursor
    pub fn delete(&mut self) {
        if !self.editing || self.cursor >= self.input_buffer.len() {
            return;
        }
        self.input_buffer.remove(self.cursor);
    }

    /// Move cursor left
    pub fn cursor_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    /// Move cursor right
    pub fn cursor_right(&mut self) {
        if self.cursor < self.input_buffer.len() {
            self.cursor += 1;
        }
    }

    /// Move cursor to start
    pub fn cursor_home(&mut self) {
        self.cursor = 0;
    }

    /// Move cursor to end
    pub fn cursor_end(&mut self) {
        self.cursor = self.input_buffer.len();
    }

    /// Save config to file
    pub fn save(&mut self) -> Result<(), String> {
        self.config.save().map_err(|e| e.to_string())?;
        self.dirty = false;
        self.status_message = Some("Settings saved".to_string());
        Ok(())
    }

    /// Check if a key is set for display
    pub fn key_status(&self, field: SettingsField) -> (bool, String) {
        match field {
            SettingsField::AnthropicKey => {
                if let Some(ref key) = self.config.api_keys.anthropic {
                    (true, mask_api_key(key, 12))
                } else {
                    (false, "Not set".to_string())
                }
            }
            SettingsField::OpenAiKey => {
                if let Some(ref key) = self.config.api_keys.openai {
                    (true, mask_api_key(key, 10))
                } else {
                    (false, "Not set".to_string())
                }
            }
            SettingsField::Provider => {
                if let Some(ref provider) = self.config.defaults.provider {
                    (true, provider.clone())
                } else {
                    let auto = self.config.default_provider().unwrap_or("none");
                    (false, format!("{} (auto)", auto))
                }
            }
            SettingsField::Model => {
                if let Some(ref model) = self.config.defaults.model {
                    (true, model.clone())
                } else {
                    (false, "Default".to_string())
                }
            }
        }
    }
}
