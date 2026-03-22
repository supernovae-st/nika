//! Focus State for Panel Navigation
//!
//! Manages which panel is currently focused and provides Tab/Shift+Tab navigation.
//! Updated for 3-Views Architecture

use super::views::TuiView;

/// Panel identifiers for 3-view architecture
///
/// Each view has its own set of panels that can receive focus.
/// Views: Studio (default), Command, Control
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelId {
    // ═══ Studio View (s) ═══
    /// File browser panel
    StudioFiles,
    /// YAML editor panel
    StudioEditor,
    /// DAG preview panel
    StudioDag,
    /// Diagnostics panel
    StudioDiagnostics,

    // ═══ Runner View (r) ═══
    /// Mission control panel
    RunnerMission,
    /// DAG visualization
    RunnerDag,
    /// NovaNet context panel
    RunnerNovanet,
    /// Agent reasoning panel
    RunnerReasoning,

    // ═══ Chat Playground (3) ═══
    /// Conversation history
    ChatConversation,
    /// Text input field
    ChatInput,
    /// Context/files panel
    ChatContext,
}

impl PanelId {
    /// Get all panels for a specific view
    pub fn panels_for_view(view: TuiView) -> &'static [PanelId] {
        match view {
            TuiView::Studio => &[
                PanelId::StudioFiles,
                PanelId::StudioEditor,
                PanelId::StudioDag,
                PanelId::StudioDiagnostics,
            ],
            // Command view includes chat panels (Phase 4 will add runner panels)
            TuiView::Command => &[
                PanelId::ChatConversation,
                PanelId::ChatInput,
                PanelId::ChatContext,
            ],
            // Control is auxiliary view without panel navigation
            TuiView::Control => &[],
        }
    }

    /// Get the view this panel belongs to
    pub fn view(&self) -> TuiView {
        match self {
            // Studio panels
            PanelId::StudioFiles
            | PanelId::StudioEditor
            | PanelId::StudioDag
            | PanelId::StudioDiagnostics => TuiView::Studio,
            // Chat panels
            PanelId::ChatConversation | PanelId::ChatInput | PanelId::ChatContext => {
                TuiView::Command
            }
            // Runner panels
            PanelId::RunnerMission
            | PanelId::RunnerDag
            | PanelId::RunnerNovanet
            | PanelId::RunnerReasoning => TuiView::Command,
        }
    }

    /// Get the default panel for a view
    pub fn default_for_view(view: TuiView) -> PanelId {
        match view {
            TuiView::Studio => PanelId::StudioEditor,
            TuiView::Command => PanelId::ChatInput,
            // Settings doesn't have panels - default to Studio editor
            TuiView::Control => PanelId::StudioEditor,
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
        let state = FocusState::new(PanelId::StudioFiles);
        assert_eq!(state.current(), PanelId::StudioFiles);
    }

    #[test]
    fn test_focus_changes_current() {
        let mut state = FocusState::new(PanelId::StudioFiles);
        state.focus(PanelId::StudioDag);
        assert_eq!(state.current(), PanelId::StudioDag);
    }

    #[test]
    fn test_focus_pushes_to_stack() {
        let mut state = FocusState::new(PanelId::StudioFiles);
        state.focus(PanelId::StudioDag);
        assert!(state.back());
        assert_eq!(state.current(), PanelId::StudioFiles);
    }

    #[test]
    fn test_next_panel_cycles() {
        let mut state = FocusState::new(PanelId::StudioFiles);
        state.next_panel();
        assert_eq!(state.current(), PanelId::StudioEditor);
        state.next_panel();
        assert_eq!(state.current(), PanelId::StudioDag);
        state.next_panel();
        assert_eq!(state.current(), PanelId::StudioDiagnostics);
        state.next_panel();
        assert_eq!(state.current(), PanelId::StudioFiles); // Cycles back
    }

    #[test]
    fn test_prev_panel_cycles() {
        let mut state = FocusState::new(PanelId::StudioFiles);
        state.prev_panel();
        assert_eq!(state.current(), PanelId::StudioDiagnostics); // Wraps to end
    }

    #[test]
    fn test_reset_to_view() {
        let mut state = FocusState::new(PanelId::StudioFiles);
        state.focus(PanelId::StudioDag);
        state.focus(PanelId::StudioEditor);
        state.reset_to_view(TuiView::Command);
        assert_eq!(state.current(), PanelId::ChatInput);
        assert!(!state.back()); // Stack cleared
    }

    #[test]
    fn test_panels_for_view() {
        // Studio has 4 panels (Files, Editor, Dag, Diagnostics)
        let studio_panels = PanelId::panels_for_view(TuiView::Studio);
        assert_eq!(studio_panels.len(), 4);

        let command_panels = PanelId::panels_for_view(TuiView::Command);
        assert_eq!(command_panels.len(), 3);

        // Control has no panels
        let control_panels = PanelId::panels_for_view(TuiView::Control);
        assert_eq!(control_panels.len(), 0);
    }

    #[test]
    fn test_panel_view() {
        // Studio panels
        assert_eq!(PanelId::StudioFiles.view(), TuiView::Studio);
        assert_eq!(PanelId::StudioEditor.view(), TuiView::Studio);
        assert_eq!(PanelId::StudioDag.view(), TuiView::Studio);
        assert_eq!(PanelId::StudioDiagnostics.view(), TuiView::Studio);
        // Chat panels
        assert_eq!(PanelId::ChatInput.view(), TuiView::Command);
        // Runner panels
        assert_eq!(PanelId::RunnerDag.view(), TuiView::Command);
    }
}
