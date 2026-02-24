//! Event handler for provider modal
//!
//! Processes keyboard input and returns actions for the modal.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::state::{ProviderModalState, ProviderModalTab};

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
    /// Test API key for a provider
    TestApiKey { provider: &'static str },
    /// Save API key for a provider
    SaveApiKey { provider: &'static str, key: String },
    /// Pull Ollama model
    PullModel { model: String },
    /// Delete Ollama model
    DeleteModel { model: String },
    /// Refresh Ollama models list
    RefreshOllamaModels,
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

            // Tab-specific actions
            KeyCode::Char('p') if state.active_tab == ProviderModalTab::Ollama => {
                // Would get actual model from selection
                HandleResult::consumed_with_action(ModalAction::PullModel {
                    model: "llama3.2".to_string(),
                })
            }
            KeyCode::Char('d') if state.active_tab == ProviderModalTab::Ollama => {
                HandleResult::consumed_with_action(ModalAction::DeleteModel {
                    model: "selected".to_string(),
                })
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
                ProviderModalTab::Ollama => {
                    HandleResult::consumed_with_action(ModalAction::RefreshOllamaModels)
                }
                _ => HandleResult::consumed(),
            },

            _ => HandleResult::ignored(),
        }
    }

    /// Handle input mode (API key entry)
    fn handle_input_mode(state: &mut ProviderModalState, key: KeyEvent) -> HandleResult {
        match key.code {
            // Cancel input
            KeyCode::Esc => {
                state.key_input_mode = false;
                state.key_input_buffer.clear();
                HandleResult::consumed()
            }

            // Submit key
            KeyCode::Enter => {
                let key_value = state.key_input_buffer.clone();
                state.key_input_mode = false;
                state.key_input_buffer.clear();

                if !key_value.is_empty() {
                    HandleResult::consumed_with_action(ModalAction::SaveApiKey {
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
                // Allow alphanumeric, dashes, underscores
                if c.is_alphanumeric() || c == '-' || c == '_' {
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
                // Check connection for selected provider
                HandleResult::consumed_with_action(ModalAction::CheckProvider {
                    provider: Self::selected_cloud_provider(state),
                })
            }
            ProviderModalTab::Ollama => {
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
    fn selected_provider(_state: &ProviderModalState) -> &'static str {
        // This would actually look up from the entries based on selected_idx
        // For now, return a placeholder
        "anthropic"
    }

    /// Get currently selected cloud provider name
    fn selected_cloud_provider(state: &ProviderModalState) -> &'static str {
        match state.selected_idx {
            0 => "anthropic",
            1 => "openai",
            2 => "mistral",
            3 => "groq",
            4 => "deepseek",
            5 => "ollama",
            _ => "anthropic",
        }
    }
}

#[cfg(test)]
mod tests {
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
        assert_eq!(state.active_tab, ProviderModalTab::Ollama);
    }

    #[test]
    fn test_handle_shift_tab_switches_tab_backwards() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.active_tab = ProviderModalTab::Ollama;

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
        assert_eq!(state.active_tab, ProviderModalTab::Ollama);
    }

    #[test]
    fn test_handle_number_keys_switch_tabs() {
        let mut state = ProviderModalState::default();
        state.visible = true;

        ModalEventHandler::handle(&mut state, key_event(KeyCode::Char('2')));
        assert_eq!(state.active_tab, ProviderModalTab::Ollama);

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
    //   3 4 5  (row 1: Groq, DeepSeek, Ollama)

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
        state.selected_idx = 5; // Row 1, col 2 (Ollama)

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

        // Should move down one row: 2 -> 5 (Ollama)
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
        state.selected_idx = 5; // Row 1, col 2 (Ollama)

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
    fn test_grid_navigation_respects_boundaries() {
        let mut state = ProviderModalState::default();
        state.visible = true;

        // At left edge, can't go left
        state.selected_idx = 0;
        ModalEventHandler::handle(&mut state, key_event(KeyCode::Left));
        assert_eq!(state.selected_idx, 0); // Stays at 0

        // At right edge (col 2), can't go right
        state.selected_idx = 2;
        ModalEventHandler::handle(&mut state, key_event(KeyCode::Right));
        assert_eq!(state.selected_idx, 2); // Stays at 2

        // At top row, can't go up
        state.selected_idx = 1;
        ModalEventHandler::handle(&mut state, key_event(KeyCode::Up));
        assert_eq!(state.selected_idx, 1); // Stays at 1

        // At bottom row, can't go down
        state.selected_idx = 4;
        ModalEventHandler::handle(&mut state, key_event(KeyCode::Down));
        assert_eq!(state.selected_idx, 4); // Stays at 4
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
    fn test_handle_enter_in_cloud_tab_checks_provider() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.active_tab = ProviderModalTab::Cloud;
        state.selected_idx = 1;

        let result = ModalEventHandler::handle(&mut state, key_event(KeyCode::Enter));

        assert!(result.consumed);
        assert_eq!(
            result.action,
            Some(ModalAction::CheckProvider { provider: "openai" })
        );
    }

    #[test]
    fn test_handle_p_in_ollama_tab_pulls_model() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.active_tab = ProviderModalTab::Ollama;

        let result = ModalEventHandler::handle(&mut state, key_event(KeyCode::Char('p')));

        assert!(result.consumed);
        assert!(matches!(result.action, Some(ModalAction::PullModel { .. })));
    }

    #[test]
    fn test_handle_d_in_ollama_tab_deletes_model() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.active_tab = ProviderModalTab::Ollama;

        let result = ModalEventHandler::handle(&mut state, key_event(KeyCode::Char('d')));

        assert!(result.consumed);
        assert!(matches!(
            result.action,
            Some(ModalAction::DeleteModel { .. })
        ));
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
    fn test_handle_r_in_ollama_tab_refreshes_models() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.active_tab = ProviderModalTab::Ollama;

        let result = ModalEventHandler::handle(&mut state, key_event(KeyCode::Char('r')));

        assert!(result.consumed);
        assert_eq!(result.action, Some(ModalAction::RefreshOllamaModels));
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
    fn test_input_mode_enter_saves_key() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.key_input_mode = true;
        state.key_input_buffer = "sk-ant-test123".to_string();

        let result = ModalEventHandler::handle(&mut state, key_event(KeyCode::Enter));

        assert!(result.consumed);
        assert!(!state.key_input_mode);
        assert!(matches!(
            result.action,
            Some(ModalAction::SaveApiKey { .. })
        ));
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
