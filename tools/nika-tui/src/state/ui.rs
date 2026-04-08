// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! UI state slice — focus, mode, scroll, theme, tab state
//!
//! Extracted from TuiState to isolate UI interaction state
//! from execution and data tracking concerns.

use std::collections::HashMap;

use super::types::{MonitorPanel, TuiMode};
use crate::theme::ThemeMode;
use crate::views::{DagTab, MissionTab, NovanetTab, ReasoningTab};

/// UI-related state extracted from TuiState
///
/// Holds focus tracking, interaction mode, scroll positions,
/// theme preferences, and tab state.
#[derive(Debug)]
pub struct UiState {
    /// Currently focused panel
    pub focus: MonitorPanel,
    /// Current interaction mode
    pub mode: TuiMode,
    /// Scroll offset per panel
    pub scroll: HashMap<MonitorPanel, usize>,
    /// Theme mode: dark or light
    pub theme_mode: ThemeMode,

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
            focus: MonitorPanel::Progress,
            mode: TuiMode::Normal,
            scroll: HashMap::new(),
            theme_mode: ThemeMode::default(),
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

    pub fn set_focus(&mut self, panel: MonitorPanel) {
        self.focus = panel;
    }

    pub fn set_mode(&mut self, mode: TuiMode) {
        self.mode = mode;
    }

    pub fn scroll_offset(&self, panel: &MonitorPanel) -> usize {
        self.scroll.get(panel).copied().unwrap_or(0)
    }

    pub fn set_scroll(&mut self, panel: MonitorPanel, offset: usize) {
        self.scroll.insert(panel, offset);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ui_state_default() {
        let ui = UiState::new();
        assert_eq!(ui.focus, MonitorPanel::Progress);
        assert!(matches!(ui.mode, TuiMode::Normal));
        assert!(ui.scroll.is_empty());
    }

    #[test]
    fn test_ui_state_focus() {
        let mut ui = UiState::new();
        ui.set_focus(MonitorPanel::Agent);
        assert_eq!(ui.focus, MonitorPanel::Agent);
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
        assert_eq!(ui.scroll_offset(&MonitorPanel::Dag), 0);
        ui.set_scroll(MonitorPanel::Dag, 42);
        assert_eq!(ui.scroll_offset(&MonitorPanel::Dag), 42);
    }
}
