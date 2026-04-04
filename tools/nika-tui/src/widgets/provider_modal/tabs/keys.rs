//! API Keys tab
//!
//! Displays provider API keys with masked display and edit capability.
//! Shows key status (configured, verified, invalid) for each provider.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};

use crate::unicode::truncate_to_width;

// =============================================================================
// SEMANTIC COLOR CONSTANTS
// =============================================================================

/// Header text - high contrast
const COLOR_TEXT_PRIMARY: Color = Color::Rgb(229, 231, 235); // Gray-200

/// Secondary text - labels, hints
const COLOR_TEXT_SECONDARY: Color = Color::Rgb(156, 163, 175); // Gray-400

/// Muted text - shortcuts, hints
const COLOR_TEXT_MUTED: Color = Color::Rgb(107, 114, 128); // Gray-500

/// Selected row background
const COLOR_BG_SELECTED: Color = Color::Rgb(30, 41, 59); // Slate-800

/// Status: not configured
const COLOR_STATUS_UNCONFIGURED: Color = Color::Rgb(107, 114, 128); // Gray-500

/// Status: saving (in progress)
const COLOR_STATUS_SAVING: Color = Color::Rgb(250, 204, 21); // Yellow-400

/// Status: testing (in progress)
const COLOR_STATUS_TESTING: Color = Color::Rgb(96, 165, 250); // Blue-400

/// Status: configured / verified (success)
const COLOR_STATUS_SUCCESS: Color = Color::Rgb(34, 197, 94); // Emerald-500

/// Status: invalid (error)
const COLOR_STATUS_ERROR: Color = Color::Rgb(239, 68, 68); // Red-500

use crate::providers::{env_var as provider_env_var, icons::provider_icon, llm_provider_ids};

use super::super::state::ApiKeyState;
use nika_engine::secrets::mask_api_key;

/// Provider key entry for display
#[derive(Debug, Clone)]
pub struct ProviderKeyEntry {
    pub provider: &'static str,
    pub icon: &'static str,
    pub env_var: &'static str,
    pub state: ApiKeyState,
}

impl ProviderKeyEntry {
    /// Create default entries for all 7 LLM providers
    ///
    /// Uses centralized `llm_provider_ids()` from providers module.
    pub fn all_providers() -> Vec<Self> {
        llm_provider_ids()
            .map(|provider| Self {
                provider,
                icon: provider_icon(provider),
                env_var: provider_env_var(provider),
                state: Self::detect_state(provider),
            })
            .collect()
    }

    /// Detect key state from vault or environment variable.
    ///
    /// Checks env var first (fast), then vault. Returns Stored for vault keys,
    /// Configured for env var keys.
    fn detect_state(provider: &str) -> ApiKeyState {
        // Priority 1: Check env var (fast, no I/O)
        let env_var = provider_env_var(provider);
        if let Ok(key) = std::env::var(env_var) {
            if !key.is_empty() {
                return ApiKeyState::Configured {
                    masked: mask_api_key(&key),
                };
            }
        }

        // Priority 2: Check vault (skip in test mode to avoid I/O)
        if !cfg!(test) {
            let nika_home = dirs::home_dir().unwrap_or_default().join(".nika");
            let vault = nika_vault::NikaVault::new(&nika_home.join("secrets"));
            if let Ok(Some(secret)) = vault.get(provider) {
                use secrecy::ExposeSecret;
                return ApiKeyState::Stored {
                    masked: mask_api_key(secret.expose_secret()),
                };
            }
        }

        ApiKeyState::NotConfigured
    }

    /// Get display name (title case)
    pub fn display_name(&self) -> &'static str {
        match self.provider {
            "anthropic" => "Anthropic",
            "openai" => "OpenAI",
            "mistral" => "Mistral",
            "groq" => "Groq",
            "deepseek" => "DeepSeek",
            "gemini" => "Gemini",
            "xai" => "xAI",
            _ => "Unknown",
        }
    }
}

/// API Keys tab widget
pub struct KeysTab<'a> {
    entries: Vec<ProviderKeyEntry>,
    selected_idx: usize,
    input_mode: bool,
    input_buffer: &'a str,
}

impl<'a> KeysTab<'a> {
    pub fn new(selected_idx: usize, input_mode: bool, input_buffer: &'a str) -> Self {
        Self {
            entries: ProviderKeyEntry::all_providers(),
            selected_idx,
            input_mode,
            input_buffer,
        }
    }

    /// Create with custom entries (for testing)
    pub fn with_entries(
        entries: Vec<ProviderKeyEntry>,
        selected_idx: usize,
        input_mode: bool,
        input_buffer: &'a str,
    ) -> Self {
        Self {
            entries,
            selected_idx,
            input_mode,
            input_buffer,
        }
    }

    /// Get entry count
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Get selected provider name
    pub fn selected_provider(&self) -> Option<&str> {
        self.entries.get(self.selected_idx).map(|e| e.provider)
    }
}

impl Widget for KeysTab<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 {
            return;
        }

        // Header
        let header = "API Keys";
        buf.set_string(
            area.x + 1,
            area.y,
            header,
            Style::default()
                .fg(COLOR_TEXT_PRIMARY)
                .add_modifier(Modifier::BOLD),
        );

        // Shortcuts hint
        let shortcuts = "[Enter] Edit  [t] Test";
        let shortcuts_x = area.right().saturating_sub(shortcuts.len() as u16 + 1);
        buf.set_string(
            shortcuts_x,
            area.y,
            shortcuts,
            Style::default().fg(COLOR_TEXT_MUTED),
        );

        // Calculate list area
        let list_height = if self.input_mode {
            area.height.saturating_sub(5) // Leave room for input
        } else {
            area.height.saturating_sub(2) // Just header
        };

        // Render provider list
        for (i, entry) in self.entries.iter().enumerate() {
            let y = area.y + 2 + i as u16;
            if y >= area.y + 2 + list_height {
                break;
            }

            let is_selected = i == self.selected_idx;
            let style = if is_selected {
                Style::default()
                    .fg(COLOR_TEXT_PRIMARY)
                    .bg(COLOR_BG_SELECTED)
            } else {
                Style::default().fg(COLOR_TEXT_SECONDARY)
            };

            // Selection indicator
            let indicator = if is_selected { "▸" } else { " " };

            // Status indicator and color
            let (status_icon, status_color) = match &entry.state {
                ApiKeyState::NotConfigured => ("○", COLOR_STATUS_UNCONFIGURED),
                ApiKeyState::Saving { .. } => ("⏳", COLOR_STATUS_SAVING),
                ApiKeyState::Testing { .. } => ("⠹", COLOR_STATUS_TESTING),
                ApiKeyState::Configured { .. } => ("●", COLOR_STATUS_SUCCESS),
                ApiKeyState::Stored { .. } => ("🔐", COLOR_STATUS_SUCCESS), // Vault
                ApiKeyState::Verified { .. } => ("✓", COLOR_STATUS_SUCCESS),
                ApiKeyState::Invalid { .. } => ("✗", COLOR_STATUS_ERROR),
            };

            // Key display
            let key_display = match &entry.state {
                ApiKeyState::NotConfigured => "Not configured".to_string(),
                ApiKeyState::Saving { masked } => format!("{} Saving...", masked),
                ApiKeyState::Testing { masked } => format!("{} Testing...", masked),
                ApiKeyState::Configured { masked } => format!("{} (env)", masked), // Clarify source
                ApiKeyState::Stored { masked } => format!("{} (vault)", masked),   // Vault
                ApiKeyState::Verified { masked, latency_ms } => {
                    format!("{} ({}ms)", masked, latency_ms)
                }
                ApiKeyState::Invalid { masked, error } => {
                    format!("{} - {}", masked, error)
                }
            };

            // Line: indicator icon Provider .......... status key_display
            let provider_part = format!("{} {} {:12}", indicator, entry.icon, entry.display_name());
            buf.set_string(area.x + 1, y, &provider_part, style);

            // Status icon with color
            let status_x = area.x + 1 + provider_part.len() as u16 + 2;
            buf.set_string(status_x, y, status_icon, Style::default().fg(status_color));

            // Key display
            let key_x = status_x + 2;
            let max_key_len = (area.right().saturating_sub(key_x + 1)) as usize;
            let truncated_key = if key_display.len() > max_key_len {
                let truncated = truncate_to_width(&key_display, max_key_len.saturating_sub(1));
                format!("{}…", truncated)
            } else {
                key_display
            };
            buf.set_string(key_x, y, &truncated_key, style);
        }

        // Input mode: show input field at bottom
        if self.input_mode {
            let input_y = area.bottom().saturating_sub(3);

            // Label
            let selected_name = self
                .entries
                .get(self.selected_idx)
                .map(|e| e.display_name())
                .unwrap_or("Provider");
            let label = format!("Enter API key for {}:", selected_name);
            buf.set_string(
                area.x + 1,
                input_y,
                &label,
                Style::default().fg(COLOR_TEXT_SECONDARY),
            );

            // Input field (masked with asterisks)
            // Cap display width to leave room for hint (max 80 chars or area width - hint)
            let input_y = input_y + 1;
            let hint = "Press Enter to save, Esc to cancel";
            let max_input_width = (area.width as usize).saturating_sub(hint.len() + 6);
            let masked_len = self.input_buffer.len().min(max_input_width);
            let masked_input: String = "*".repeat(masked_len);
            let overflow_indicator = if self.input_buffer.len() > max_input_width {
                "..."
            } else {
                ""
            };
            let input_display = format!("[{}{}]", masked_input, overflow_indicator);
            buf.set_string(
                area.x + 1,
                input_y,
                &input_display,
                Style::default()
                    .fg(COLOR_TEXT_PRIMARY)
                    .add_modifier(Modifier::BOLD),
            );

            // Hint (positioned after input)
            buf.set_string(
                area.x + 1 + input_display.len() as u16 + 2,
                input_y,
                hint,
                Style::default().fg(COLOR_TEXT_MUTED),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    #[test]
    fn test_provider_key_entry_all_providers_returns_7() {
        let entries = ProviderKeyEntry::all_providers();
        assert_eq!(entries.len(), 7);
    }

    #[test]
    fn test_provider_key_entry_has_correct_providers() {
        let entries = ProviderKeyEntry::all_providers();
        assert_eq!(entries[0].provider, "anthropic");
        assert_eq!(entries[1].provider, "openai");
        assert_eq!(entries[2].provider, "mistral");
        assert_eq!(entries[3].provider, "groq");
        assert_eq!(entries[4].provider, "deepseek");
        assert_eq!(entries[5].provider, "gemini");
        assert_eq!(entries[6].provider, "xai");
    }

    #[test]
    fn test_provider_key_entry_display_names() {
        let entries = ProviderKeyEntry::all_providers();
        assert_eq!(entries[0].display_name(), "Anthropic");
        assert_eq!(entries[1].display_name(), "OpenAI");
        assert_eq!(entries[2].display_name(), "Mistral");
    }

    // ─── Bug 36: display_name must return "xAI" for xai provider ────

    #[test]
    fn test_xai_display_name() {
        let entry = ProviderKeyEntry {
            provider: "xai",
            icon: "\u{1d54f}",
            env_var: "XAI_API_KEY",
            state: ApiKeyState::NotConfigured,
        };
        assert_eq!(
            entry.display_name(),
            "xAI",
            "display_name() must return 'xAI' for xai provider, not 'Unknown'"
        );
    }

    // ─── Bug 15: detect_state must not access vault in test mode ────

    #[test]
    fn test_detect_state_skips_vault_in_test_mode() {
        // In test mode, detect_state should never try vault access.
        // It should only check env vars.
        let state = ProviderKeyEntry::detect_state("anthropic");
        // Without an env var set, it should be NotConfigured (not Stored from vault)
        // This test verifies no vault I/O is triggered
        assert!(
            !matches!(state, ApiKeyState::Stored { .. }),
            "detect_state must not access vault in test mode"
        );
    }

    #[test]
    fn test_keys_tab_entry_count() {
        let tab = KeysTab::new(0, false, "");
        assert_eq!(tab.entry_count(), 7);
    }

    #[test]
    fn test_keys_tab_selected_provider() {
        let tab = KeysTab::new(2, false, "");
        assert_eq!(tab.selected_provider(), Some("mistral"));
    }

    #[test]
    fn test_keys_tab_renders_header() {
        let tab = KeysTab::new(0, false, "");
        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 15));
        tab.render(Rect::new(0, 0, 60, 15), &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("API Keys"));
    }

    #[test]
    fn test_keys_tab_renders_providers() {
        let tab = KeysTab::new(0, false, "");
        let mut buf = Buffer::empty(Rect::new(0, 0, 70, 15));
        tab.render(Rect::new(0, 0, 70, 15), &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("Anthropic"));
        assert!(content.contains("OpenAI"));
        assert!(content.contains("Mistral"));
    }

    #[test]
    fn test_keys_tab_renders_selection() {
        let tab = KeysTab::new(1, false, "");
        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 15));
        tab.render(Rect::new(0, 0, 60, 15), &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        // Selection indicator should be present
        assert!(content.contains("▸"));
    }

    #[test]
    fn test_keys_tab_input_mode_renders_input_field() {
        let tab = KeysTab::new(0, true, "test");
        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 15));
        tab.render(Rect::new(0, 0, 80, 15), &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("Enter API key"));
        assert!(content.contains("****")); // Masked input
    }

    #[test]
    fn test_keys_tab_with_custom_entries() {
        let entries = vec![ProviderKeyEntry {
            provider: "test",
            icon: "🔑",
            env_var: "TEST_API_KEY",
            state: ApiKeyState::Configured {
                masked: "sk-t...t".to_string(),
            },
        }];
        let tab = KeysTab::with_entries(entries, 0, false, "");
        assert_eq!(tab.entry_count(), 1);
    }

    #[test]
    fn test_keys_tab_small_area_no_panic() {
        let tab = KeysTab::new(0, false, "");
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 2));
        // Should not panic
        tab.render(Rect::new(0, 0, 20, 2), &mut buf);
    }

    #[test]
    fn test_keys_tab_configured_state_shows_masked_key() {
        let entries = vec![ProviderKeyEntry {
            provider: "anthropic",
            icon: "🧠",
            env_var: "ANTHROPIC_API_KEY",
            state: ApiKeyState::Configured {
                masked: "sk-ant...x".to_string(),
            },
        }];
        let tab = KeysTab::with_entries(entries, 0, false, "");
        let mut buf = Buffer::empty(Rect::new(0, 0, 70, 10));
        tab.render(Rect::new(0, 0, 70, 10), &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("sk-ant...x"));
    }

    #[test]
    fn test_keys_tab_not_configured_state() {
        let entries = vec![ProviderKeyEntry {
            provider: "openai",
            icon: "🤖",
            env_var: "OPENAI_API_KEY",
            state: ApiKeyState::NotConfigured,
        }];
        let tab = KeysTab::with_entries(entries, 0, false, "");
        let mut buf = Buffer::empty(Rect::new(0, 0, 70, 10));
        tab.render(Rect::new(0, 0, 70, 10), &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("Not configured"));
    }

    #[test]
    fn test_keys_tab_verified_state_shows_latency() {
        let entries = vec![ProviderKeyEntry {
            provider: "mistral",
            icon: "🌀",
            env_var: "MISTRAL_API_KEY",
            state: ApiKeyState::Verified {
                masked: "mist...y".to_string(),
                latency_ms: 142,
            },
        }];
        let tab = KeysTab::with_entries(entries, 0, false, "");
        let mut buf = Buffer::empty(Rect::new(0, 0, 70, 10));
        tab.render(Rect::new(0, 0, 70, 10), &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("142ms"));
    }

    #[test]
    fn test_keys_tab_invalid_state_shows_error() {
        let entries = vec![ProviderKeyEntry {
            provider: "groq",
            icon: "⚡",
            env_var: "GROQ_API_KEY",
            state: ApiKeyState::Invalid {
                masked: "grq...z".to_string(),
                error: "401 Unauthorized".to_string(),
            },
        }];
        let tab = KeysTab::with_entries(entries, 0, false, "");
        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 10));
        tab.render(Rect::new(0, 0, 80, 10), &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("401 Unauthorized"));
    }
}
