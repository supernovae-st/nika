//! Focus State for Panel Navigation
//!
//! Manages which panel is currently focused and provides Tab/Shift+Tab navigation.
//! Updated for 6-Views Architecture (v0.12)

use super::views::TuiView;

/// Panel identifiers for 6-view architecture (v0.12)
///
/// Each view has its own set of panels that can receive focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelId {
    // ═══ Explorer View (1) ═══
    /// File browser panel
    ExplorerFiles,
    /// DAG preview panel
    ExplorerDag,
    /// YAML preview panel
    ExplorerYaml,
    /// Execution history panel
    ExplorerHistory,

    // ═══ Chat View (2) ═══
    /// Conversation history
    ChatConversation,
    /// Text input field
    ChatInput,
    /// Context/files panel
    ChatContext,

    // ═══ Editor View (3) ═══
    /// File explorer
    EditorFiles,
    /// YAML editor
    EditorEditor,
    /// Diagnostics panel
    EditorDiagnostics,

    // ═══ Runner View (4) ═══
    /// Mission control panel
    RunnerMission,
    /// DAG visualization
    RunnerDag,
    /// NovaNet context panel
    RunnerNovanet,
    /// Agent reasoning panel
    RunnerReasoning,

    // ═══ Scheduler View (5) ═══
    /// Schedule list panel
    SchedulerList,
    /// Timeline panel
    SchedulerTimeline,
    /// Run history panel
    SchedulerHistory,
    /// Cron editor panel
    SchedulerCronEditor,
}

impl PanelId {
    /// Get all panels for a specific view
    pub fn panels_for_view(view: TuiView) -> &'static [PanelId] {
        match view {
            TuiView::Explorer => &[
                PanelId::ExplorerFiles,
                PanelId::ExplorerDag,
                PanelId::ExplorerYaml,
                PanelId::ExplorerHistory,
            ],
            TuiView::Chat => &[
                PanelId::ChatConversation,
                PanelId::ChatInput,
                PanelId::ChatContext,
            ],
            TuiView::Editor => &[
                PanelId::EditorFiles,
                PanelId::EditorEditor,
                PanelId::EditorDiagnostics,
            ],
            TuiView::Runner => &[
                PanelId::RunnerMission,
                PanelId::RunnerDag,
                PanelId::RunnerNovanet,
                PanelId::RunnerReasoning,
            ],
            TuiView::Scheduler => &[
                PanelId::SchedulerList,
                PanelId::SchedulerTimeline,
                PanelId::SchedulerHistory,
                PanelId::SchedulerCronEditor,
            ],
            // Settings is auxiliary view without panel navigation (v0.12)
            TuiView::Settings => &[],
        }
    }

    /// Get the view this panel belongs to
    pub fn view(&self) -> TuiView {
        match self {
            PanelId::ExplorerFiles
            | PanelId::ExplorerDag
            | PanelId::ExplorerYaml
            | PanelId::ExplorerHistory => TuiView::Explorer,
            PanelId::ChatConversation | PanelId::ChatInput | PanelId::ChatContext => TuiView::Chat,
            PanelId::EditorFiles | PanelId::EditorEditor | PanelId::EditorDiagnostics => {
                TuiView::Editor
            }
            PanelId::RunnerMission
            | PanelId::RunnerDag
            | PanelId::RunnerNovanet
            | PanelId::RunnerReasoning => TuiView::Runner,
            PanelId::SchedulerList
            | PanelId::SchedulerTimeline
            | PanelId::SchedulerHistory
            | PanelId::SchedulerCronEditor => TuiView::Scheduler,
        }
    }

    /// Get the default panel for a view
    pub fn default_for_view(view: TuiView) -> PanelId {
        match view {
            TuiView::Explorer => PanelId::ExplorerFiles,
            TuiView::Chat => PanelId::ChatInput,
            TuiView::Editor => PanelId::EditorEditor,
            TuiView::Runner => PanelId::RunnerMission,
            TuiView::Scheduler => PanelId::SchedulerList,
            // Auxiliary views don't have panels, return Explorer's default
            TuiView::Settings => PanelId::ExplorerFiles,
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
        let state = FocusState::new(PanelId::ExplorerFiles);
        assert_eq!(state.current(), PanelId::ExplorerFiles);
    }

    #[test]
    fn test_focus_changes_current() {
        let mut state = FocusState::new(PanelId::ExplorerFiles);
        state.focus(PanelId::ExplorerDag);
        assert_eq!(state.current(), PanelId::ExplorerDag);
    }

    #[test]
    fn test_focus_pushes_to_stack() {
        let mut state = FocusState::new(PanelId::ExplorerFiles);
        state.focus(PanelId::ExplorerDag);
        assert!(state.back());
        assert_eq!(state.current(), PanelId::ExplorerFiles);
    }

    #[test]
    fn test_next_panel_cycles() {
        let mut state = FocusState::new(PanelId::ExplorerFiles);
        state.next_panel();
        assert_eq!(state.current(), PanelId::ExplorerDag);
        state.next_panel();
        assert_eq!(state.current(), PanelId::ExplorerYaml);
        state.next_panel();
        assert_eq!(state.current(), PanelId::ExplorerHistory);
        state.next_panel();
        assert_eq!(state.current(), PanelId::ExplorerFiles); // Cycles back
    }

    #[test]
    fn test_prev_panel_cycles() {
        let mut state = FocusState::new(PanelId::ExplorerFiles);
        state.prev_panel();
        assert_eq!(state.current(), PanelId::ExplorerHistory); // Wraps to end
    }

    #[test]
    fn test_reset_to_view() {
        let mut state = FocusState::new(PanelId::ExplorerFiles);
        state.focus(PanelId::ExplorerDag);
        state.focus(PanelId::ExplorerYaml);
        state.reset_to_view(TuiView::Chat);
        assert_eq!(state.current(), PanelId::ChatInput);
        assert!(!state.back()); // Stack cleared
    }

    #[test]
    fn test_panels_for_view() {
        let explorer_panels = PanelId::panels_for_view(TuiView::Explorer);
        assert_eq!(explorer_panels.len(), 4);

        let chat_panels = PanelId::panels_for_view(TuiView::Chat);
        assert_eq!(chat_panels.len(), 3);

        let runner_panels = PanelId::panels_for_view(TuiView::Runner);
        assert_eq!(runner_panels.len(), 4);

        let scheduler_panels = PanelId::panels_for_view(TuiView::Scheduler);
        assert_eq!(scheduler_panels.len(), 4);
    }

    #[test]
    fn test_panel_view() {
        assert_eq!(PanelId::ExplorerFiles.view(), TuiView::Explorer);
        assert_eq!(PanelId::ChatInput.view(), TuiView::Chat);
        assert_eq!(PanelId::EditorEditor.view(), TuiView::Editor);
        assert_eq!(PanelId::RunnerDag.view(), TuiView::Runner);
        assert_eq!(PanelId::SchedulerList.view(), TuiView::Scheduler);
    }
}
