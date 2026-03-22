//! Key event handling for HomeView
//!
//! Contains the search-mode and normal-mode key dispatch logic.
//! Called by the View trait impl in render.rs.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::HomeView;
use crate::state::TuiState;
use crate::views::{TuiView, ViewAction};
use crate::widgets::tree::TreeAction;

use super::PreviewMode;

impl HomeView {
    /// Handle a key event, returning an action
    ///
    /// Dispatched from `View::handle_key` in render.rs.
    pub(crate) fn handle_key_event(&mut self, key: KeyEvent, _state: &mut TuiState) -> ViewAction {
        // Handle search mode input
        if self.search_active {
            match key.code {
                // Cancel search
                KeyCode::Esc => {
                    self.search_active = false;
                    self.search_query.clear();
                    self.update_filter();
                    return ViewAction::None;
                }
                // Confirm selection
                KeyCode::Enter => {
                    self.search_active = false;
                    if let Some(entry) = self.selected_entry() {
                        if entry.is_dir {
                            self.toggle_folder();
                            return ViewAction::None;
                        } else {
                            return ViewAction::RunWorkflow(entry.path.clone());
                        }
                    }
                    return ViewAction::None;
                }
                // Navigation in search mode
                KeyCode::Up => {
                    self.select_prev();
                    return ViewAction::None;
                }
                KeyCode::Down => {
                    self.select_next();
                    return ViewAction::None;
                }
                // Backspace to delete
                KeyCode::Backspace => {
                    self.search_query.pop();
                    self.update_filter();
                    return ViewAction::None;
                }
                // Type character
                KeyCode::Char(c) => {
                    self.search_query.push(c);
                    self.update_filter();
                    return ViewAction::None;
                }
                _ => return ViewAction::None,
            }
        }

        // Normal mode key handling
        //
        // Priority order:
        // 1. Reserved keys (s, T, /, e, E, d, h, c, number keys)
        // 2. TreeAction navigation (j/k, Up/Down, Left/Right, Enter, g/G, etc.)
        // 3. Fallback to ViewAction::None

        // 1. Reserved keys - check these first before TreeAction
        match key.code {
            // 's' opens Settings view
            KeyCode::Char('s') => return ViewAction::OpenControl,

            // Shift+T or Ctrl+t toggles theme
            KeyCode::Char('T') => return ViewAction::ToggleTheme,
            KeyCode::Char('t') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return ViewAction::ToggleTheme;
            }

            // Start search with / or Ctrl+P
            KeyCode::Char('/') => {
                self.search_active = true;
                self.search_query.clear();
                return ViewAction::None;
            }
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.search_active = true;
                self.search_query.clear();
                return ViewAction::None;
            }

            // Edit: open in Studio (e key)
            KeyCode::Char('e') if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                if let Some(entry) = self.selected_entry() {
                    if !entry.is_dir {
                        return ViewAction::OpenInStudio(entry.path.clone());
                    }
                }
                return ViewAction::None;
            }

            // Toggle DAG expand mode (Shift+E)
            KeyCode::Char('E') => {
                self.dag_expanded = !self.dag_expanded;
                return ViewAction::None;
            }

            // Toggle preview mode: DAG ↔ YAML (D key, not Ctrl+D)
            KeyCode::Char('d') | KeyCode::Char('D')
                if !key.modifiers.contains(KeyModifiers::SHIFT)
                    && !key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                self.preview_mode = match self.preview_mode {
                    PreviewMode::Dag => PreviewMode::Yaml,
                    PreviewMode::Yaml => PreviewMode::Dag,
                };
                return ViewAction::None;
            }

            // 'h' toggles history (reserved, not TreeAction::Left)
            KeyCode::Char('h') => {
                self.history_expanded = !self.history_expanded;
                return ViewAction::None;
            }

            // Chat: switch to Command view
            KeyCode::Char('c') => return ViewAction::SwitchView(TuiView::Command),

            // View switching: number keys
            // 1=Studio, 2=Command, 3=Control
            KeyCode::Char('1') => return ViewAction::SwitchView(TuiView::Studio),
            KeyCode::Char('2') => return ViewAction::SwitchView(TuiView::Command),
            KeyCode::Char('3') => return ViewAction::SwitchView(TuiView::Control),

            // Validate selected workflow
            KeyCode::Char('v') => {
                if let Some(entry) = self.selected_entry() {
                    if !entry.is_dir {
                        return ViewAction::ValidateWorkflow(entry.path.clone());
                    }
                }
                return ViewAction::None;
            }

            // Tab: handled at app level for view cycling
            KeyCode::Tab => return ViewAction::None,

            _ => {} // Fall through to TreeAction handling
        }

        // 2. TreeAction navigation
        // Convert key to TreeAction and delegate to handle_tree_action
        if let Some(action) = TreeAction::from_key_event(key) {
            if let Some(view_action) = self.handle_tree_action(action) {
                return view_action;
            }
        }

        // 3. Fallback
        ViewAction::None
    }
}
