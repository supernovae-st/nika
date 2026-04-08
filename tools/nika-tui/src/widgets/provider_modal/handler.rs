// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Event handler for provider modal
//!
//! Processes keyboard input and returns actions for the modal.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use zeroize::Zeroizing;

use super::state::{ProviderModalState, ProviderModalTab};
use super::tabs::ProviderInfo;

/// Result of event handling
#[derive(Debug, Clone, PartialEq)]
pub struct HandleResult {
    /// Whether the event was consumed
    pub consumed: bool,
    /// Action to perform (if any)
    pub action: Option<ModalAction>,
}

impl HandleResult {
    /// Event was consumed, no action needed
    pub fn consumed() -> Self {
        Self {
            consumed: true,
            action: None,
        }
    }

    /// Event was consumed with an action to perform
    pub fn consumed_with_action(action: ModalAction) -> Self {
        Self {
            consumed: true,
            action: Some(action),
        }
    }

    /// Event was not consumed
    pub fn ignored() -> Self {
        Self {
            consumed: false,
            action: None,
        }
    }
}

/// Actions that require async handling or state updates outside the modal
#[derive(Debug, Clone, PartialEq)]
pub enum ModalAction {
    /// Check provider connection
    CheckProvider { provider: &'static str },
    /// Select provider as active (Enter on Cloud tab)
    SelectProvider {
        provider: &'static str,
        model: String,
    },
    /// Test API key for a provider
    TestApiKey { provider: &'static str },
    /// Save API key for a provider (without testing)
    /// SEC: key wrapped in Zeroizing to scrub from heap on drop
    SaveApiKey {
        provider: &'static str,
        key: Zeroizing<String>,
    },
    /// Save API key and then test it (recommended flow)
    /// SEC: key wrapped in Zeroizing to scrub from heap on drop
    SaveAndTestApiKey {
        provider: &'static str,
        key: Zeroizing<String>,
    },
    /// Pull native model
    PullModel { model: String },
    /// Delete native model
    DeleteModel { model: String },
    /// Refresh native models list
    RefreshNativeModels,
    /// Refresh all provider statuses
    RefreshProviders,
    /// Close the modal
    Close,
}

/// Event handler for modal keyboard input
pub struct ModalEventHandler;

impl ModalEventHandler {
    /// Handle keyboard event for the modal
    ///
    /// Returns whether the event was consumed and any action to perform.
    pub fn handle(state: &mut ProviderModalState, key: KeyEvent) -> HandleResult {
        // Modal must be visible to handle events
        if !state.visible {
            return HandleResult::ignored();
        }

        // In input mode, delegate to input handler
        if state.key_input_mode {
            return Self::handle_input_mode(state, key);
        }

        // In expanded provider mode, handle model selection
        if state.is_provider_expanded() {
            return Self::handle_expanded_mode(state, key);
        }

        match key.code {
            // Close modal
            KeyCode::Esc | KeyCode::Char('q') => {
                state.close();
                HandleResult::consumed_with_action(ModalAction::Close)
            }

            // Tab navigation
            KeyCode::Tab => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    state.prev_tab();
                } else {
                    state.next_tab();
                }
                HandleResult::consumed()
            }

            // Backtab (Shift+Tab on some terminals)
            KeyCode::BackTab => {
                state.prev_tab();
                HandleResult::consumed()
            }

            // Number key tab switching
            KeyCode::Char(c @ '1'..='4') => {
                if let Some(tab) = ProviderModalTab::from_key(c) {
                    state.switch_tab(tab);
                }
                HandleResult::consumed()
            }

            // Navigation (vim-style: h/j/k/l + arrows)
            KeyCode::Up | KeyCode::Char('k') => {
                state.navigate_up();
                HandleResult::consumed()
            }
            KeyCode::Down | KeyCode::Char('j') => {
                state.navigate_down();
                HandleResult::consumed()
            }
            KeyCode::Left | KeyCode::Char('h') => {
                state.navigate_left();
                HandleResult::consumed()
            }
            KeyCode::Right | KeyCode::Char('l') => {
                state.navigate_right();
                HandleResult::consumed()
            }

            // Enter - context-sensitive action
            KeyCode::Enter => Self::handle_enter(state),

            // Space - expand provider to show models (Cloud tab)
            KeyCode::Char(' ') if state.active_tab == ProviderModalTab::Cloud => {
                state.expand_selected_provider();
                HandleResult::consumed()
            }

            // Tab-specific actions
            KeyCode::Char('p') if state.active_tab == ProviderModalTab::Native => {
                let model = state
                    .native_models
                    .get(state.selected_idx)
                    .map(|m| m.name.clone())
                    .unwrap_or_else(|| "llama3.2".to_string());
                HandleResult::consumed_with_action(ModalAction::PullModel { model })
            }
            KeyCode::Char('d') if state.active_tab == ProviderModalTab::Native => {
                if let Some(m) = state.native_models.get(state.selected_idx) {
                    let model = m.name.clone();
                    HandleResult::consumed_with_action(ModalAction::DeleteModel { model })
                } else {
                    HandleResult::consumed()
                }
            }
            KeyCode::Char('t') if state.active_tab == ProviderModalTab::Keys => {
                HandleResult::consumed_with_action(ModalAction::TestApiKey {
                    provider: Self::selected_provider(state),
                })
            }
            KeyCode::Char('r') => match state.active_tab {
                ProviderModalTab::Cloud => {
                    HandleResult::consumed_with_action(ModalAction::RefreshProviders)
                }
                ProviderModalTab::Native => {
                    HandleResult::consumed_with_action(ModalAction::RefreshNativeModels)
                }
                _ => HandleResult::consumed(),
            },

            _ => HandleResult::ignored(),
        }
    }

    /// Handle expanded provider mode (model selection)
    fn handle_expanded_mode(state: &mut ProviderModalState, key: KeyEvent) -> HandleResult {
        let providers = ProviderInfo::all_providers();
        let provider_idx = state.expanded_provider_idx.unwrap_or(0);
        let model_count = providers
            .get(provider_idx)
            .map(|p| p.models.len())
            .unwrap_or(0);

        match key.code {
            // Collapse (close model list, stay in modal)
            KeyCode::Esc => {
                state.collapse_provider();
                HandleResult::consumed()
            }

            // Navigate model list
            KeyCode::Up | KeyCode::Char('k') => {
                state.model_navigate_up(model_count);
                HandleResult::consumed()
            }
            KeyCode::Down | KeyCode::Char('j') => {
                state.model_navigate_down(model_count);
                HandleResult::consumed()
            }

            // Collapse and go left/right in grid
            KeyCode::Left | KeyCode::Char('h') => {
                state.collapse_provider();
                state.navigate_left();
                HandleResult::consumed()
            }
            KeyCode::Right | KeyCode::Char('l') => {
                state.collapse_provider();
                state.navigate_right();
                HandleResult::consumed()
            }

            // Select model and close modal
            KeyCode::Enter | KeyCode::Char(' ') => {
                let provider = Self::selected_cloud_provider_by_idx(provider_idx);
                let model = providers
                    .get(provider_idx)
                    .and_then(|p| p.models.get(state.model_selection_idx))
                    .map(|m| m.id.to_string())
                    .unwrap_or_else(|| Self::default_model_for_provider(provider_idx).to_string());

                // Update state
                state.active_provider_idx = Some(provider_idx);
                state.active_model = Some(model.clone());
                state.collapse_provider();
                state.visible = false;

                HandleResult::consumed_with_action(ModalAction::SelectProvider { provider, model })
            }

            // Close modal entirely
            KeyCode::Char('q') => {
                state.collapse_provider();
                state.close();
                HandleResult::consumed_with_action(ModalAction::Close)
            }

            _ => HandleResult::consumed(),
        }
    }

    /// Handle input mode (API key entry)
    fn handle_input_mode(state: &mut ProviderModalState, key: KeyEvent) -> HandleResult {
        use zeroize::Zeroize;

        match key.code {
            // Cancel input
            KeyCode::Esc => {
                state.key_input_mode = false;
                // SEC: Zeroize buffer to scrub API key material from memory
                state.key_input_buffer.zeroize();
                HandleResult::consumed()
            }

            // Submit key - saves AND tests (full flow)
            KeyCode::Enter => {
                // SEC: move key out of buffer (no clone) into Zeroizing wrapper
                let key_value = Zeroizing::new(std::mem::take(&mut state.key_input_buffer));
                state.key_input_mode = false;

                if !key_value.is_empty() {
                    HandleResult::consumed_with_action(ModalAction::SaveAndTestApiKey {
                        provider: Self::selected_provider(state),
                        key: key_value,
                    })
                } else {
                    HandleResult::consumed()
                }
            }

            // Delete character
            KeyCode::Backspace => {
                state.key_input_buffer.pop();
                HandleResult::consumed()
            }

            // Type character
            KeyCode::Char(c) => {
                // Allow alphanumeric and common API key characters
                // API keys often contain: -, _, ., +, = (base64), :, /
                if c.is_alphanumeric()
                    || c == '-'
                    || c == '_'
                    || c == '.'
                    || c == '+'
                    || c == '='
                    || c == ':'
                    || c == '/'
                {
                    state.key_input_buffer.push(c);
                }
                HandleResult::consumed()
            }

            // Paste handling would go here
            _ => HandleResult::consumed(),
        }
    }

    /// Handle Enter key based on current tab
    fn handle_enter(state: &mut ProviderModalState) -> HandleResult {
        match state.active_tab {
            ProviderModalTab::Keys => {
                // Enter input mode to edit/add key
                state.key_input_mode = true;
                HandleResult::consumed()
            }
            ProviderModalTab::Cloud => {
                // Enter now expands the provider to show model list
                // (Space also expands, Enter then selects from the expanded list)
                state.expand_selected_provider();
                HandleResult::consumed()
            }
            ProviderModalTab::Native => {
                // Would show model details or pull dialog
                HandleResult::consumed()
            }
            ProviderModalTab::Config => {
                // Would edit setting
                HandleResult::consumed()
            }
        }
    }

    /// Get currently selected provider name for Keys tab
    ///
    /// Uses the canonical `llm_provider_ids()` list so new providers (e.g. xAI)
    /// are automatically covered without updating a parallel match table.
    fn selected_provider(state: &ProviderModalState) -> &'static str {
        crate::providers::llm_provider_ids()
            .nth(state.selected_idx)
            .unwrap_or("anthropic")
    }

    /// Get cloud provider name by index
    ///
    /// Uses the canonical `llm_provider_ids()` list — single source of truth.
    fn selected_cloud_provider_by_idx(idx: usize) -> &'static str {
        crate::providers::llm_provider_ids()
            .nth(idx)
            .unwrap_or("anthropic")
    }

    /// Get default model for provider index
    fn default_model_for_provider(idx: usize) -> &'static str {
        match idx {
            0 => "claude-sonnet-4",
            1 => "gpt-4o",
            2 => "mistral-large",
            3 => "llama-3.3-70b",
            4 => "deepseek-chat",
            5 => "gemini-2.0-flash",
            6 => "grok-3",
            _ => "claude-sonnet-4",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::state::NativeModelInfo;
    use super::*;

    fn key_event(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn key_event_with_mod(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn test_handle_ignored_when_not_visible() {
        let mut state = ProviderModalState::default();
        state.visible = false;

        let result = ModalEventHandler::handle(&mut state, key_event(KeyCode::Tab));

        assert!(!result.consumed);
        assert!(result.action.is_none());
    }

    #[test]
    fn test_handle_tab_key_switches_tab() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.active_tab = ProviderModalTab::Cloud;

        let result = ModalEventHandler::handle(&mut state, key_event(KeyCode::Tab));

        assert!(result.consumed);
        assert_eq!(state.active_tab, ProviderModalTab::Native);
    }

    #[test]
    fn test_handle_shift_tab_switches_tab_backwards() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.active_tab = ProviderModalTab::Native;

        let result = ModalEventHandler::handle(
            &mut state,
            key_event_with_mod(KeyCode::Tab, KeyModifiers::SHIFT),
        );

        assert!(result.consumed);
        assert_eq!(state.active_tab, ProviderModalTab::Cloud);
    }

    #[test]
    fn test_handle_backtab_switches_tab_backwards() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.active_tab = ProviderModalTab::Keys;

        let result = ModalEventHandler::handle(&mut state, key_event(KeyCode::BackTab));

        assert!(result.consumed);
        assert_eq!(state.active_tab, ProviderModalTab::Native);
    }

    #[test]
    fn test_handle_number_keys_switch_tabs() {
        let mut state = ProviderModalState::default();
        state.visible = true;

        ModalEventHandler::handle(&mut state, key_event(KeyCode::Char('2')));
        assert_eq!(state.active_tab, ProviderModalTab::Native);

        ModalEventHandler::handle(&mut state, key_event(KeyCode::Char('3')));
        assert_eq!(state.active_tab, ProviderModalTab::Keys);

        ModalEventHandler::handle(&mut state, key_event(KeyCode::Char('4')));
        assert_eq!(state.active_tab, ProviderModalTab::Config);

        ModalEventHandler::handle(&mut state, key_event(KeyCode::Char('1')));
        assert_eq!(state.active_tab, ProviderModalTab::Cloud);
    }

    #[test]
    fn test_handle_esc_closes_modal() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.open();

        let result = ModalEventHandler::handle(&mut state, key_event(KeyCode::Esc));

        assert!(result.consumed);
        assert_eq!(result.action, Some(ModalAction::Close));
        assert!(!state.visible);
    }

    #[test]
    fn test_handle_q_closes_modal() {
        let mut state = ProviderModalState::default();
        state.visible = true;

        let result = ModalEventHandler::handle(&mut state, key_event(KeyCode::Char('q')));

        assert!(result.consumed);
        assert_eq!(result.action, Some(ModalAction::Close));
    }

    // Note: Cloud tab (default) uses 2x3 grid navigation
    // Up/Down move by rows (±3), Left/Right move by columns (±1)
    // Grid layout:
    //   0 1 2  (row 0: Claude, OpenAI, Mistral)
    //   3 4 5  (row 1: Groq, DeepSeek, Gemini)

    #[test]
    fn test_handle_up_navigates_grid() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.selected_idx = 4; // Row 1, col 1 (DeepSeek)

        ModalEventHandler::handle(&mut state, key_event(KeyCode::Up));

        // Should move up one row: 4 -> 1 (OpenAI)
        assert_eq!(state.selected_idx, 1);
    }

    #[test]
    fn test_handle_k_navigates_up_grid() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.selected_idx = 5; // Row 1, col 2 (Gemini)

        ModalEventHandler::handle(&mut state, key_event(KeyCode::Char('k')));

        // Should move up one row: 5 -> 2 (Mistral)
        assert_eq!(state.selected_idx, 2);
    }

    #[test]
    fn test_handle_down_navigates_grid() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.selected_idx = 2; // Row 0, col 2 (Mistral)

        ModalEventHandler::handle(&mut state, key_event(KeyCode::Down));

        // Should move down one row: 2 -> 5 (Gemini)
        assert_eq!(state.selected_idx, 5);
    }

    #[test]
    fn test_handle_j_navigates_down_grid() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.selected_idx = 1; // Row 0, col 1 (OpenAI)

        ModalEventHandler::handle(&mut state, key_event(KeyCode::Char('j')));

        // Should move down one row: 1 -> 4 (DeepSeek)
        assert_eq!(state.selected_idx, 4);
    }

    #[test]
    fn test_handle_left_navigates_grid() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.selected_idx = 2; // Row 0, col 2 (Mistral)

        ModalEventHandler::handle(&mut state, key_event(KeyCode::Left));

        // Should move left: 2 -> 1 (OpenAI)
        assert_eq!(state.selected_idx, 1);
    }

    #[test]
    fn test_handle_h_navigates_left_grid() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.selected_idx = 5; // Row 1, col 2 (Gemini)

        ModalEventHandler::handle(&mut state, key_event(KeyCode::Char('h')));

        // Should move left: 5 -> 4 (DeepSeek)
        assert_eq!(state.selected_idx, 4);
    }

    #[test]
    fn test_handle_right_navigates_grid() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.selected_idx = 3; // Row 1, col 0 (Groq)

        ModalEventHandler::handle(&mut state, key_event(KeyCode::Right));

        // Should move right: 3 -> 4 (DeepSeek)
        assert_eq!(state.selected_idx, 4);
    }

    #[test]
    fn test_handle_l_navigates_right_grid() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.selected_idx = 0; // Row 0, col 0 (Claude)

        ModalEventHandler::handle(&mut state, key_event(KeyCode::Char('l')));

        // Should move right: 0 -> 1 (OpenAI)
        assert_eq!(state.selected_idx, 1);
    }

    #[test]
    fn test_grid_navigation_wraps() {
        let mut state = ProviderModalState::default();
        state.visible = true;

        // At left edge, wraps to right of same row
        state.selected_idx = 0;
        ModalEventHandler::handle(&mut state, key_event(KeyCode::Left));
        assert_eq!(state.selected_idx, 2); // Wraps to end of row 0

        // At right edge (col 2), wraps to left of same row
        state.selected_idx = 2;
        ModalEventHandler::handle(&mut state, key_event(KeyCode::Right));
        assert_eq!(state.selected_idx, 0); // Wraps to start of row 0

        // At top row, wraps to bottom (same column)
        state.selected_idx = 1;
        ModalEventHandler::handle(&mut state, key_event(KeyCode::Up));
        assert_eq!(state.selected_idx, 4); // Wraps to row 1, col 1

        // At bottom row, wraps to top (same column)
        state.selected_idx = 4;
        ModalEventHandler::handle(&mut state, key_event(KeyCode::Down));
        assert_eq!(state.selected_idx, 1); // Wraps to row 0, col 1
    }

    #[test]
    fn test_handle_enter_in_keys_tab_enters_input_mode() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.active_tab = ProviderModalTab::Keys;

        let result = ModalEventHandler::handle(&mut state, key_event(KeyCode::Enter));

        assert!(result.consumed);
        assert!(state.key_input_mode);
    }

    #[test]
    fn test_handle_enter_in_cloud_tab_expands_provider() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.active_tab = ProviderModalTab::Cloud;
        state.selected_idx = 1; // OpenAI

        let result = ModalEventHandler::handle(&mut state, key_event(KeyCode::Enter));

        assert!(result.consumed);
        // Enter now expands the provider to show model list
        assert!(result.action.is_none());
        assert!(state.is_provider_expanded());
        assert_eq!(state.expanded_provider_idx, Some(1));
        assert!(state.visible); // Modal stays open
    }

    #[test]
    fn test_handle_space_in_cloud_tab_expands_provider() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.active_tab = ProviderModalTab::Cloud;
        state.selected_idx = 2; // Mistral

        let result = ModalEventHandler::handle(&mut state, key_event(KeyCode::Char(' ')));

        assert!(result.consumed);
        assert!(state.is_provider_expanded());
        assert_eq!(state.expanded_provider_idx, Some(2));
    }

    #[test]
    fn test_expanded_mode_enter_selects_model() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.active_tab = ProviderModalTab::Cloud;
        state.selected_idx = 1; // OpenAI
        state.expand_selected_provider();
        state.model_selection_idx = 1; // Select second model (gpt-4o-mini)

        let result = ModalEventHandler::handle(&mut state, key_event(KeyCode::Enter));

        assert!(result.consumed);
        assert_eq!(
            result.action,
            Some(ModalAction::SelectProvider {
                provider: "openai",
                model: "gpt-4o-mini".to_string(),
            })
        );
        assert!(!state.visible);
        assert_eq!(state.active_provider_idx, Some(1));
        assert_eq!(state.active_model, Some("gpt-4o-mini".to_string()));
    }

    #[test]
    fn test_expanded_mode_esc_collapses() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.active_tab = ProviderModalTab::Cloud;
        state.expand_selected_provider();

        let result = ModalEventHandler::handle(&mut state, key_event(KeyCode::Esc));

        assert!(result.consumed);
        assert!(!state.is_provider_expanded());
        assert!(state.visible); // Modal stays open
    }

    #[test]
    fn test_expanded_mode_up_down_navigates_models() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.active_tab = ProviderModalTab::Cloud;
        state.selected_idx = 0; // Claude (has 3 models)
        state.expand_selected_provider();

        // Down
        ModalEventHandler::handle(&mut state, key_event(KeyCode::Down));
        assert_eq!(state.model_selection_idx, 1);

        // Down again
        ModalEventHandler::handle(&mut state, key_event(KeyCode::Down));
        assert_eq!(state.model_selection_idx, 2);

        // Down wraps to first
        ModalEventHandler::handle(&mut state, key_event(KeyCode::Down));
        assert_eq!(state.model_selection_idx, 0);

        // Up wraps to last
        ModalEventHandler::handle(&mut state, key_event(KeyCode::Up));
        assert_eq!(state.model_selection_idx, 2);
    }

    #[test]
    fn test_expanded_mode_left_right_collapses_and_navigates() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.active_tab = ProviderModalTab::Cloud;
        state.selected_idx = 1; // OpenAI (middle of row)
        state.expand_selected_provider();

        // Left collapses and navigates left
        ModalEventHandler::handle(&mut state, key_event(KeyCode::Left));
        assert!(!state.is_provider_expanded());
        assert_eq!(state.selected_idx, 0); // Claude
    }

    #[test]
    fn test_handle_p_in_native_tab_pulls_selected_model() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.active_tab = ProviderModalTab::Native;
        state.native_models = vec![NativeModelInfo {
            name: "phi3:mini".to_string(),
            size: 1_000_000,
            digest: "abc123".to_string(),
            modified_at: "2024-01-01".to_string(),
            details: Default::default(),
        }];
        state.selected_idx = 0;

        let result = ModalEventHandler::handle(&mut state, key_event(KeyCode::Char('p')));

        assert!(result.consumed);
        match result.action {
            Some(ModalAction::PullModel { ref model }) => {
                assert_eq!(
                    model, "phi3:mini",
                    "PullModel must use the selected model name"
                );
            }
            _ => panic!("Expected PullModel action"),
        }
    }

    #[test]
    fn test_handle_d_in_native_tab_deletes_selected_model() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.active_tab = ProviderModalTab::Native;
        state.native_models = vec![NativeModelInfo {
            name: "llama3.2:latest".to_string(),
            size: 2_000_000,
            digest: "def456".to_string(),
            modified_at: "2024-01-01".to_string(),
            details: Default::default(),
        }];
        state.selected_idx = 0;

        let result = ModalEventHandler::handle(&mut state, key_event(KeyCode::Char('d')));

        assert!(result.consumed);
        match result.action {
            Some(ModalAction::DeleteModel { ref model }) => {
                assert_eq!(model, "llama3.2:latest");
            }
            _ => panic!("Expected DeleteModel action"),
        }
    }

    #[test]
    fn test_handle_d_in_native_tab_no_models_no_action() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.active_tab = ProviderModalTab::Native;
        state.native_models.clear();

        let result = ModalEventHandler::handle(&mut state, key_event(KeyCode::Char('d')));

        assert!(result.consumed);
        assert!(
            result.action.is_none(),
            "DeleteModel with no models must produce no action"
        );
    }

    #[test]
    fn test_handle_t_in_keys_tab_tests_api_key() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.active_tab = ProviderModalTab::Keys;

        let result = ModalEventHandler::handle(&mut state, key_event(KeyCode::Char('t')));

        assert!(result.consumed);
        assert!(matches!(
            result.action,
            Some(ModalAction::TestApiKey { .. })
        ));
    }

    #[test]
    fn test_handle_r_in_cloud_tab_refreshes_providers() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.active_tab = ProviderModalTab::Cloud;

        let result = ModalEventHandler::handle(&mut state, key_event(KeyCode::Char('r')));

        assert!(result.consumed);
        assert_eq!(result.action, Some(ModalAction::RefreshProviders));
    }

    #[test]
    fn test_handle_r_in_native_tab_refreshes_models() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.active_tab = ProviderModalTab::Native;

        let result = ModalEventHandler::handle(&mut state, key_event(KeyCode::Char('r')));

        assert!(result.consumed);
        assert_eq!(result.action, Some(ModalAction::RefreshNativeModels));
    }

    #[test]
    fn test_input_mode_esc_cancels() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.key_input_mode = true;
        state.key_input_buffer = "sk-test".to_string();

        let result = ModalEventHandler::handle(&mut state, key_event(KeyCode::Esc));

        assert!(result.consumed);
        assert!(!state.key_input_mode);
        assert!(state.key_input_buffer.is_empty());
    }

    #[test]
    fn test_input_mode_enter_saves_and_tests_key() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.key_input_mode = true;
        state.active_tab = ProviderModalTab::Keys;
        state.selected_idx = 0; // Anthropic
        state.key_input_buffer = "sk-ant-test123".to_string();

        let result = ModalEventHandler::handle(&mut state, key_event(KeyCode::Enter));

        assert!(result.consumed);
        assert!(!state.key_input_mode);
        assert_eq!(
            result.action,
            Some(ModalAction::SaveAndTestApiKey {
                provider: "anthropic",
                key: "sk-ant-test123".to_string().into(),
            })
        );
    }

    #[test]
    fn test_input_mode_enter_empty_does_not_save() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.key_input_mode = true;
        state.key_input_buffer.clear();

        let result = ModalEventHandler::handle(&mut state, key_event(KeyCode::Enter));

        assert!(result.consumed);
        assert!(result.action.is_none());
    }

    #[test]
    fn test_input_mode_backspace_removes_char() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.key_input_mode = true;
        state.key_input_buffer = "test".to_string();

        ModalEventHandler::handle(&mut state, key_event(KeyCode::Backspace));

        assert_eq!(state.key_input_buffer, "tes");
    }

    #[test]
    fn test_input_mode_char_adds_to_buffer() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.key_input_mode = true;
        state.key_input_buffer = "sk-".to_string();

        ModalEventHandler::handle(&mut state, key_event(KeyCode::Char('a')));
        ModalEventHandler::handle(&mut state, key_event(KeyCode::Char('b')));

        assert_eq!(state.key_input_buffer, "sk-ab");
    }

    #[test]
    fn test_input_mode_allows_dashes_and_underscores() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.key_input_mode = true;
        state.key_input_buffer.clear();

        ModalEventHandler::handle(&mut state, key_event(KeyCode::Char('a')));
        ModalEventHandler::handle(&mut state, key_event(KeyCode::Char('-')));
        ModalEventHandler::handle(&mut state, key_event(KeyCode::Char('_')));
        ModalEventHandler::handle(&mut state, key_event(KeyCode::Char('b')));

        assert_eq!(state.key_input_buffer, "a-_b");
    }

    #[test]
    fn test_input_mode_allows_api_key_characters() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.key_input_mode = true;
        state.key_input_buffer.clear();

        // API keys can contain: alphanumeric, -, _, ., +, =, :, /
        for c in ['s', 'k', '-', 'a', 'n', 't', '.', '+', '=', ':', '/'] {
            ModalEventHandler::handle(&mut state, key_event(KeyCode::Char(c)));
        }

        assert_eq!(state.key_input_buffer, "sk-ant.+=:/");
    }

    #[test]
    fn test_handle_result_constructors() {
        let consumed = HandleResult::consumed();
        assert!(consumed.consumed);
        assert!(consumed.action.is_none());

        let with_action = HandleResult::consumed_with_action(ModalAction::Close);
        assert!(with_action.consumed);
        assert_eq!(with_action.action, Some(ModalAction::Close));

        let ignored = HandleResult::ignored();
        assert!(!ignored.consumed);
        assert!(ignored.action.is_none());
    }
}
