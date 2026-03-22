//! Control View — Infrastructure management
//!
//! Wraps the Settings view for provider configuration, MCP server
//! management, theme preferences, and diagnostics.
//!
//! Future expansion: model management, package registry, memory tiers.

use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};

use super::settings::SettingsView;
use super::{View, ViewAction};
use crate::state::TuiState;
use crate::theme::Theme;

/// Control View — Provider config, MCP servers, preferences
pub struct ControlView {
    /// Settings view (all current functionality preserved)
    pub settings: SettingsView,
}

impl Default for ControlView {
    fn default() -> Self {
        Self::new()
    }
}

impl ControlView {
    pub fn new() -> Self {
        Self {
            settings: SettingsView::new(),
        }
    }
}

impl View for ControlView {
    fn render(&mut self, frame: &mut Frame, area: Rect, state: &TuiState, theme: &Theme) {
        self.settings.render(frame, area, state, theme);
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut TuiState) -> ViewAction {
        self.settings.handle_key(key, state)
    }

    fn status_line(&self, state: &TuiState) -> String {
        self.settings.status_line(state)
    }

    fn on_enter(&mut self, state: &mut TuiState) {
        self.settings.on_enter(state);
    }

    fn on_leave(&mut self, state: &mut TuiState) {
        self.settings.on_leave(state);
    }

    fn tick(&mut self, state: &mut TuiState) {
        self.settings.tick(state);
    }
}
