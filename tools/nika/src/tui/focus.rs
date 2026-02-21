//! Focus State for Panel Navigation
//!
//! Manages which panel is currently focused and provides Tab/Shift+Tab navigation.

use super::views::TuiView;

/// Panel identifiers for 4-view architecture
///
/// Each view has its own set of panels that can receive focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelId {
    // ═══ Home View (Standalone) ═══
    /// File browser panel
    HomeFiles,
    /// DAG preview panel
    HomeDag,
    /// YAML preview panel
    HomeYaml,
    /// Execution history panel
    HomeHistory,

    // ═══ Chat View ═══
    /// Conversation history
    ChatConversation,
    /// Text input field
    ChatInput,
    /// Context/files panel
    ChatContext,

    // ═══ Studio View ═══
    /// File explorer
    StudioFiles,
    /// YAML editor
    StudioEditor,
    /// Diagnostics panel
    StudioDiagnostics,

    // ═══ Monitor View (Execution) ═══
    /// Mission control panel
    MonitorMission,
    /// DAG visualization
    MonitorDag,
    /// NovaNet context panel
    MonitorNovanet,
    /// Agent reasoning panel
    MonitorReasoning,
}

impl PanelId {
    /// Get all panels for a specific view
    pub fn panels_for_view(view: TuiView) -> &'static [PanelId] {
        match view {
            TuiView::Home => &[
                PanelId::HomeFiles,
                PanelId::HomeDag,
                PanelId::HomeYaml,
                PanelId::HomeHistory,
            ],
            TuiView::Chat => &[
                PanelId::ChatConversation,
                PanelId::ChatInput,
                PanelId::ChatContext,
            ],
            TuiView::Studio => &[
                PanelId::StudioFiles,
                PanelId::StudioEditor,
                PanelId::StudioDiagnostics,
            ],
            TuiView::Monitor => &[
                PanelId::MonitorMission,
                PanelId::MonitorDag,
                PanelId::MonitorNovanet,
                PanelId::MonitorReasoning,
            ],
        }
    }

    /// Get the view this panel belongs to
    pub fn view(&self) -> TuiView {
        match self {
            PanelId::HomeFiles | PanelId::HomeDag | PanelId::HomeYaml | PanelId::HomeHistory => {
                TuiView::Home
            }
            PanelId::ChatConversation | PanelId::ChatInput | PanelId::ChatContext => TuiView::Chat,
            PanelId::StudioFiles | PanelId::StudioEditor | PanelId::StudioDiagnostics => {
                TuiView::Studio
            }
            PanelId::MonitorMission
            | PanelId::MonitorDag
            | PanelId::MonitorNovanet
            | PanelId::MonitorReasoning => TuiView::Monitor,
        }
    }

    /// Get the default panel for a view
    pub fn default_for_view(view: TuiView) -> PanelId {
        match view {
            TuiView::Home => PanelId::HomeFiles,
            TuiView::Chat => PanelId::ChatInput,
            TuiView::Studio => PanelId::StudioEditor,
            TuiView::Monitor => PanelId::MonitorMission,
        }
    }
}

/// Focus state manager for keyboard navigation
#[derive(Debug, Clone)]
pub struct FocusState {
    /// Currently focused panel
    current: PanelId,
    /// Focus history stack for back navigation
    stack: Vec<PanelId>,
}

impl FocusState {
    /// Create new focus state with initial panel
    pub fn new(initial: PanelId) -> Self {
        Self {
            current: initial,
            stack: Vec::with_capacity(8),
        }
    }

    /// Get currently focused panel
    pub fn current(&self) -> PanelId {
        self.current
    }

    /// Focus a specific panel, pushing current to stack
    pub fn focus(&mut self, panel: PanelId) {
        if panel != self.current {
            self.stack.push(self.current);
            self.current = panel;
            // Keep stack bounded
            if self.stack.len() > 16 {
                self.stack.remove(0);
            }
        }
    }

    /// Move to next panel in current view (Tab)
    pub fn next_panel(&mut self) {
        let view = self.current.view();
        let panels = PanelId::panels_for_view(view);
        if let Some(idx) = panels.iter().position(|&p| p == self.current) {
            let next_idx = (idx + 1) % panels.len();
            self.focus(panels[next_idx]);
        }
    }

    /// Move to previous panel in current view (Shift+Tab)
    pub fn prev_panel(&mut self) {
        let view = self.current.view();
        let panels = PanelId::panels_for_view(view);
        if let Some(idx) = panels.iter().position(|&p| p == self.current) {
            let prev_idx = if idx == 0 { panels.len() - 1 } else { idx - 1 };
            self.focus(panels[prev_idx]);
        }
    }

    /// Go back to previous focus (if any)
    pub fn back(&mut self) -> bool {
        if let Some(prev) = self.stack.pop() {
            self.current = prev;
            true
        } else {
            false
        }
    }

    /// Check if a panel is currently focused
    pub fn is_focused(&self, panel: PanelId) -> bool {
        self.current == panel
    }

    /// Reset focus to default panel for a view
    pub fn reset_to_view(&mut self, view: TuiView) {
        let default = PanelId::default_for_view(view);
        self.stack.clear();
        self.current = default;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_focus_state() {
        let state = FocusState::new(PanelId::HomeFiles);
        assert_eq!(state.current(), PanelId::HomeFiles);
    }

    #[test]
    fn test_focus_changes_current() {
        let mut state = FocusState::new(PanelId::HomeFiles);
        state.focus(PanelId::HomeDag);
        assert_eq!(state.current(), PanelId::HomeDag);
    }

    #[test]
    fn test_focus_pushes_to_stack() {
        let mut state = FocusState::new(PanelId::HomeFiles);
        state.focus(PanelId::HomeDag);
        assert!(state.back());
        assert_eq!(state.current(), PanelId::HomeFiles);
    }

    #[test]
    fn test_next_panel_cycles() {
        let mut state = FocusState::new(PanelId::HomeFiles);
        state.next_panel();
        assert_eq!(state.current(), PanelId::HomeDag);
        state.next_panel();
        assert_eq!(state.current(), PanelId::HomeYaml);
        state.next_panel();
        assert_eq!(state.current(), PanelId::HomeHistory);
        state.next_panel();
        assert_eq!(state.current(), PanelId::HomeFiles); // Cycles back
    }

    #[test]
    fn test_prev_panel_cycles() {
        let mut state = FocusState::new(PanelId::HomeFiles);
        state.prev_panel();
        assert_eq!(state.current(), PanelId::HomeHistory); // Wraps to end
    }

    #[test]
    fn test_reset_to_view() {
        let mut state = FocusState::new(PanelId::HomeFiles);
        state.focus(PanelId::HomeDag);
        state.focus(PanelId::HomeYaml);
        state.reset_to_view(TuiView::Chat);
        assert_eq!(state.current(), PanelId::ChatInput);
        assert!(!state.back()); // Stack cleared
    }

    #[test]
    fn test_panels_for_view() {
        let home_panels = PanelId::panels_for_view(TuiView::Home);
        assert_eq!(home_panels.len(), 4);

        let chat_panels = PanelId::panels_for_view(TuiView::Chat);
        assert_eq!(chat_panels.len(), 3);

        let monitor_panels = PanelId::panels_for_view(TuiView::Monitor);
        assert_eq!(monitor_panels.len(), 4);
    }

    #[test]
    fn test_panel_view() {
        assert_eq!(PanelId::HomeFiles.view(), TuiView::Home);
        assert_eq!(PanelId::ChatInput.view(), TuiView::Chat);
        assert_eq!(PanelId::StudioEditor.view(), TuiView::Studio);
        assert_eq!(PanelId::MonitorDag.view(), TuiView::Monitor);
    }
}
