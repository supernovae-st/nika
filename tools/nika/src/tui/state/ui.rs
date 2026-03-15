//! UI state slice — focus, mode, scroll, theme, overlays
//!
//! Extracted from TuiState to isolate UI interaction state
//! from execution and data tracking concerns.

use std::collections::HashMap;

use super::chat_overlay::ChatOverlayState;
use super::types::{PanelId, TuiMode};
use crate::tui::theme::ThemeMode;
use crate::tui::views::{DagTab, MissionTab, NovanetTab, ReasoningTab};

/// UI-related state extracted from TuiState
///
/// Holds focus tracking, interaction mode, scroll positions,
/// theme preferences, and overlay states.
#[derive(Debug)]
pub struct UiState {
    /// Currently focused panel
    pub focus: PanelId,
    /// Current interaction mode
    pub mode: TuiMode,
    /// Scroll offset per panel
    pub scroll: HashMap<PanelId, usize>,
    /// Theme mode: dark or light
    pub theme_mode: ThemeMode,
    /// Chat overlay state (contextual AI assistance)
    pub chat_overlay: ChatOverlayState,

    // ── Tab state ──
    /// Mission Control panel tab (Progress / IO / Output)
    pub mission_tab: MissionTab,
    /// DAG panel tab (Graph / YAML)
    pub dag_tab: DagTab,
    /// NovaNet panel tab (Summary / Full JSON)
    pub novanet_tab: NovanetTab,
    /// Reasoning panel tab (Turns / Thinking)
    pub reasoning_tab: ReasoningTab,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            focus: PanelId::Progress,
            mode: TuiMode::Normal,
            scroll: HashMap::new(),
            theme_mode: ThemeMode::default(),
            chat_overlay: ChatOverlayState::new(),
            mission_tab: MissionTab::default(),
            dag_tab: DagTab::default(),
            novanet_tab: NovanetTab::default(),
            reasoning_tab: ReasoningTab::default(),
        }
    }
}

impl UiState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_focus(&mut self, panel: PanelId) {
        self.focus = panel;
    }

    pub fn set_mode(&mut self, mode: TuiMode) {
        self.mode = mode;
    }

    pub fn scroll_offset(&self, panel: &PanelId) -> usize {
        self.scroll.get(panel).copied().unwrap_or(0)
    }

    pub fn set_scroll(&mut self, panel: PanelId, offset: usize) {
        self.scroll.insert(panel, offset);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ui_state_default() {
        let ui = UiState::new();
        assert_eq!(ui.focus, PanelId::Progress);
        assert!(matches!(ui.mode, TuiMode::Normal));
        assert!(ui.scroll.is_empty());
    }

    #[test]
    fn test_ui_state_focus() {
        let mut ui = UiState::new();
        ui.set_focus(PanelId::Agent);
        assert_eq!(ui.focus, PanelId::Agent);
    }

    #[test]
    fn test_ui_state_mode() {
        let mut ui = UiState::new();
        ui.set_mode(TuiMode::Search);
        assert!(matches!(ui.mode, TuiMode::Search));
    }

    #[test]
    fn test_ui_state_scroll() {
        let mut ui = UiState::new();
        assert_eq!(ui.scroll_offset(&PanelId::Dag), 0);
        ui.set_scroll(PanelId::Dag, 42);
        assert_eq!(ui.scroll_offset(&PanelId::Dag), 42);
    }
}
