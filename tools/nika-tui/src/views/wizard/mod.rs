// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Wizard View for Nika Setup
//!
//! Full-screen setup wizard accessible from TUI Settings view.
//! Machine setup runs automatically on first command via `machine.rs`.
//!
//! # Architecture
//!
//! 1. Auto-setup on first command → `maybe_run_auto_setup()` in main.rs
//! 2. First TUI run → Modal prompt to run setup if not completed
//! 3. Settings view → "Re-run Setup Wizard" button
//!
//! # Steps
//!
//! 0. Welcome - Introduction and overview
//! 1. Providers - Configure API keys for cloud LLMs
//! 2. Models - Download local models (mistral.rs)
//! 3. McpServers - Configure MCP server connections
//! 4. EditorSync - Enable editor integrations
//! 5. Verification - Run diagnostics
//! 6. Complete - Summary and exit

mod keys;
mod render;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Gauge, Paragraph},
    Frame,
};

use ratatui::widgets::ListState;

use crate::state::TuiState;
use crate::theme::solarized;
use crate::theme::Theme;
use crate::views::{View, ViewAction};
use crate::wizard::{WizardConfig, WizardState, WizardStep};

/// Provider item for the provider list
#[derive(Debug, Clone)]
pub(crate) struct ProviderItem {
    pub(crate) id: &'static str,
    pub(crate) name: &'static str,
    pub(crate) env_var: &'static str,
    pub(crate) configured: bool,
}

/// Editor item for the editor list
#[derive(Debug, Clone)]
pub(crate) struct EditorItem {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) id: &'static str,
    pub(crate) name: &'static str,
    pub(crate) enabled: bool,
}

/// Wizard View - Full-screen setup wizard
///
/// Standalone view launched by `nika setup`, not part of normal TUI navigation.
pub struct WizardView {
    /// Wizard state machine
    pub state: WizardState,
    /// List state for provider selection
    pub(crate) provider_list_state: ListState,
    /// List state for editor selection
    pub(crate) editor_list_state: ListState,
    /// List state for MCP server selection
    pub(crate) mcp_list_state: ListState,
    /// Available providers
    pub(crate) providers: Vec<ProviderItem>,
    /// Available editors
    pub(crate) editors: Vec<EditorItem>,
    /// Available MCP servers (subset of 100 aliases)
    pub(crate) mcp_servers: Vec<&'static str>,
    /// Input buffer for API key entry
    pub(crate) input_buffer: String,
    /// Whether we're in input mode
    pub(crate) input_mode: bool,
    /// Currently selected provider for input
    pub(crate) input_provider: Option<usize>,
}

impl Default for WizardView {
    fn default() -> Self {
        Self::new()
    }
}

impl WizardView {
    /// Create a new wizard view
    pub fn new() -> Self {
        let mut provider_list_state = ListState::default();
        provider_list_state.select(Some(0));

        let mut editor_list_state = ListState::default();
        editor_list_state.select(Some(0));

        let mut mcp_list_state = ListState::default();
        mcp_list_state.select(Some(0));

        Self {
            state: WizardState::new(),
            provider_list_state,
            editor_list_state,
            mcp_list_state,
            providers: vec![
                ProviderItem {
                    id: "anthropic",
                    name: "Anthropic (Claude)",
                    env_var: "ANTHROPIC_API_KEY",
                    configured: std::env::var("ANTHROPIC_API_KEY").is_ok(),
                },
                ProviderItem {
                    id: "openai",
                    name: "OpenAI (GPT-4)",
                    env_var: "OPENAI_API_KEY",
                    configured: std::env::var("OPENAI_API_KEY").is_ok(),
                },
                ProviderItem {
                    id: "mistral",
                    name: "Mistral AI",
                    env_var: "MISTRAL_API_KEY",
                    configured: std::env::var("MISTRAL_API_KEY").is_ok(),
                },
                ProviderItem {
                    id: "groq",
                    name: "Groq",
                    env_var: "GROQ_API_KEY",
                    configured: std::env::var("GROQ_API_KEY").is_ok(),
                },
                ProviderItem {
                    id: "deepseek",
                    name: "DeepSeek",
                    env_var: "DEEPSEEK_API_KEY",
                    configured: std::env::var("DEEPSEEK_API_KEY").is_ok(),
                },
                ProviderItem {
                    id: "gemini",
                    name: "Google Gemini",
                    env_var: "GEMINI_API_KEY",
                    configured: std::env::var("GEMINI_API_KEY").is_ok(),
                },
            ],
            editors: vec![
                EditorItem {
                    id: "claude-code",
                    name: "Claude Code",
                    enabled: false,
                },
                EditorItem {
                    id: "cursor",
                    name: "Cursor",
                    enabled: false,
                },
                EditorItem {
                    id: "windsurf",
                    name: "Windsurf",
                    enabled: false,
                },
                EditorItem {
                    id: "vscode",
                    name: "VS Code",
                    enabled: false,
                },
            ],
            mcp_servers: vec![
                "neo4j",
                "github",
                "slack",
                "perplexity",
                "firecrawl",
                "filesystem",
                "postgres",
                "sqlite",
            ],
            input_buffer: String::new(),
            input_mode: false,
            input_provider: None,
        }
    }

    /// Check if wizard has been completed before
    /// Used for first-run detection in main TUI
    pub fn was_completed() -> bool {
        let config_path = dirs::home_dir()
            .map(|h| h.join(".nika").join("wizard.json"))
            .unwrap_or_default();

        if config_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&config_path) {
                if let Ok(config) = serde_json::from_str::<WizardConfig>(&content) {
                    return config.completed;
                }
            }
        }
        false
    }

    /// Save wizard completion state
    pub fn save_completion(&self) -> std::io::Result<()> {
        let config_path = dirs::home_dir()
            .map(|h| h.join(".nika").join("wizard.json"))
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "No home dir"))?;

        // Create .nika directory if needed
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let config = self.state.to_config();
        let json = serde_json::to_string_pretty(&config)?;
        std::fs::write(&config_path, json)?;
        Ok(())
    }

    /// Render progress bar at bottom
    fn render_progress(&self, frame: &mut Frame, area: Rect) {
        let progress = self.state.progress_percentage();
        let step_num = self.state.current_step.number();
        let total = 6;

        let label = format!(
            "Step {} of {}: {}",
            step_num.min(total),
            total,
            self.state.current_step.title()
        );

        let gauge = Gauge::default()
            .block(Block::default().borders(Borders::NONE))
            .gauge_style(Style::default().fg(solarized::CYAN).bg(solarized::BASE02))
            .percent(progress as u16)
            .label(Span::styled(label, Style::default().fg(solarized::BASE1)));

        frame.render_widget(gauge, area);
    }
}

impl View for WizardView {
    fn render(&mut self, frame: &mut Frame, area: Rect, _state: &TuiState, _theme: &Theme) {
        // Clear background
        frame.render_widget(Clear, area);

        // Main layout
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(10),   // Content
                Constraint::Length(2), // Progress
            ])
            .split(area);

        // Header
        let header = Paragraph::new(vec![Line::from(vec![
            Span::styled("🦋 ", Style::default()),
            Span::styled(
                "NIKA SETUP WIZARD",
                Style::default()
                    .fg(solarized::CYAN)
                    .add_modifier(Modifier::BOLD),
            ),
        ])])
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(solarized::BASE01)),
        );
        frame.render_widget(header, chunks[0]);

        // Content area with padding
        let content_area = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(10),
                Constraint::Percentage(80),
                Constraint::Percentage(10),
            ])
            .split(chunks[1])[1];

        // Render step content
        match self.state.current_step {
            WizardStep::Welcome => self.render_welcome(frame, content_area, _theme),
            WizardStep::Providers => self.render_providers(frame, content_area, _theme),
            WizardStep::Models => self.render_models(frame, content_area, _theme),
            WizardStep::McpServers => self.render_mcp_servers(frame, content_area, _theme),
            WizardStep::EditorSync => self.render_editor_sync(frame, content_area, _theme),
            WizardStep::Verification => self.render_verification(frame, content_area, _theme),
            WizardStep::Complete => self.render_complete(frame, content_area, _theme),
        }

        // Progress bar
        self.render_progress(frame, chunks[2]);
    }

    fn handle_key(&mut self, key: KeyEvent, _state: &mut TuiState) -> ViewAction {
        // Global keys
        match key.code {
            KeyCode::Esc => {
                if self.input_mode {
                    self.input_mode = false;
                    self.input_buffer.clear();
                    return ViewAction::None;
                }
                return ViewAction::Quit;
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return ViewAction::Quit;
            }
            _ => {}
        }

        // Step-specific handling
        match self.state.current_step {
            WizardStep::Welcome => {
                if matches!(key.code, KeyCode::Enter | KeyCode::Right) {
                    self.state.advance();
                }
            }
            WizardStep::Providers => {
                return self.handle_providers_key(key);
            }
            WizardStep::Models => match key.code {
                KeyCode::Right | KeyCode::Tab | KeyCode::Enter => {
                    self.state.advance();
                }
                KeyCode::Left => {
                    self.state.go_back();
                }
                _ => {}
            },
            WizardStep::McpServers => {
                return self.handle_mcp_key(key);
            }
            WizardStep::EditorSync => {
                return self.handle_editor_key(key);
            }
            WizardStep::Verification => match key.code {
                KeyCode::Right | KeyCode::Enter => {
                    self.state.advance();
                }
                KeyCode::Left => {
                    self.state.go_back();
                }
                _ => {}
            },
            WizardStep::Complete => {
                if matches!(key.code, KeyCode::Enter) {
                    // Save completion state
                    let _ = self.save_completion();
                    return ViewAction::Quit;
                }
            }
        }

        ViewAction::None
    }

    fn status_line(&self, _state: &TuiState) -> String {
        format!(
            "[Wizard] Step {}: {} | Progress: {}%",
            self.state.current_step.number(),
            self.state.current_step.title(),
            self.state.progress_percentage()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wizard_view_new() {
        let view = WizardView::new();
        assert_eq!(view.state.current_step, WizardStep::Welcome);
        assert_eq!(view.providers.len(), 6);
        assert_eq!(view.editors.len(), 4);
    }

    #[test]
    fn test_wizard_view_default() {
        let view = WizardView::default();
        assert_eq!(view.state.current_step, WizardStep::Welcome);
    }

    #[test]
    fn test_wizard_view_providers_count() {
        let view = WizardView::new();
        assert_eq!(view.providers.len(), 6);
        assert_eq!(view.providers[0].id, "anthropic");
        assert_eq!(view.providers[5].id, "gemini");
    }

    #[test]
    fn test_wizard_view_editors_count() {
        let view = WizardView::new();
        assert_eq!(view.editors.len(), 4);
        assert_eq!(view.editors[0].id, "claude-code");
    }

    #[test]
    fn test_wizard_view_mcp_servers() {
        let view = WizardView::new();
        assert!(!view.mcp_servers.is_empty());
        assert!(view.mcp_servers.contains(&"neo4j"));
    }

    #[test]
    fn test_wizard_view_status_line() {
        let view = WizardView::new();
        let state = TuiState::new("test.nika.yaml");
        let status = view.status_line(&state);
        assert!(status.contains("Wizard"));
        assert!(status.contains("Welcome"));
    }

    #[test]
    fn test_wizard_view_input_mode_default_false() {
        let view = WizardView::new();
        assert!(!view.input_mode);
        assert!(view.input_buffer.is_empty());
    }

    #[test]
    fn test_wizard_view_list_states_initialized() {
        let view = WizardView::new();
        assert_eq!(view.provider_list_state.selected(), Some(0));
        assert_eq!(view.editor_list_state.selected(), Some(0));
        assert_eq!(view.mcp_list_state.selected(), Some(0));
    }
}
