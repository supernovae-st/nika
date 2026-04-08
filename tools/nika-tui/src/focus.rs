// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Panel Navigation for 3-View Architecture
//!
//! Defines `PanelId` -- the canonical panel identifiers for the 3-view TUI
//! (Studio, Command, Control). Each view has its own set of focusable panels.

use super::views::TuiView;

/// Panel identifiers for 3-view architecture
///
/// Each view has its own set of panels that can receive focus.
/// Views: Studio (default), Command, Control
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelId {
    // === Studio View (s) ===
    /// File browser panel
    StudioFiles,
    /// YAML editor panel
    StudioEditor,
    /// DAG preview panel
    StudioDag,
    /// Diagnostics panel
    StudioDiagnostics,

    // === Runner View (r) ===
    /// Mission control panel
    RunnerMission,
    /// DAG visualization
    RunnerDag,
    /// NovaNet context panel
    RunnerNovanet,
    /// Agent reasoning panel
    RunnerReasoning,

    // === Chat Playground (3) ===
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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_default_for_view() {
        assert_eq!(
            PanelId::default_for_view(TuiView::Studio),
            PanelId::StudioEditor
        );
        assert_eq!(
            PanelId::default_for_view(TuiView::Command),
            PanelId::ChatInput
        );
        assert_eq!(
            PanelId::default_for_view(TuiView::Control),
            PanelId::StudioEditor
        );
    }
}
