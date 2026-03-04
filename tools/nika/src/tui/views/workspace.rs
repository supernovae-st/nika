//! Workspace View - Unified 3-Panel Layout (v0.20)
//!
//! Combines Browser (file navigation) + Editor (YAML editing) + DAG Preview
//! in a single unified workspace for efficient workflow development.
//!
//! ```text
//! ┌────────────────────────────────────────────────────────────────────────────┐
//! │ NIKA WORKSPACE                                              [F10: Exit]   │
//! ├──────────────┬────────────────────────────────┬────────────────────────────┤
//! │              │                                │                            │
//! │   BROWSER    │           EDITOR               │         DAG PREVIEW        │
//! │              │                                │                            │
//! │  workflows/  │  schema: nika/workflow@0.9     │      ┌─────┐               │
//! │  ├─ a.yaml   │  workflow: my-workflow         │      │step1│               │
//! │  └─ b.yaml   │                                │      └──┬──┘               │
//! │              │  tasks:                        │         │                  │
//! │              │    - id: step1                 │      ┌──▼──┐               │
//! │              │      infer: "Generate..."      │      │step2│               │
//! │              │                                │      └─────┘               │
//! │              │                                │                            │
//! ├──────────────┴────────────────────────────────┴────────────────────────────┤
//! │ [Tab] Panel • [Ctrl+]] Ratio • [F10] Exit                                  │
//! └────────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Panel Ratios
//!
//! - **Balanced**: 20% / 50% / 30% (default)
//! - **EditorFocus**: 15% / 65% / 20% (Ctrl+] cycles)
//! - **BrowserFocus**: 35% / 45% / 20%
//! - **DagFocus**: 15% / 35% / 50%

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use super::trait_view::View;
use super::{HomeView, StudioView, TuiView, ViewAction};
use crate::tui::state::TuiState;
use crate::tui::theme::Theme;

/// Which panel has focus in the workspace
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorkspaceFocus {
    /// File browser panel (left)
    #[default]
    Browser,
    /// YAML editor panel (center)
    Editor,
    /// DAG preview panel (right)
    Dag,
}

impl WorkspaceFocus {
    /// Cycle to next panel (Tab)
    pub fn next(&self) -> Self {
        match self {
            WorkspaceFocus::Browser => WorkspaceFocus::Editor,
            WorkspaceFocus::Editor => WorkspaceFocus::Dag,
            WorkspaceFocus::Dag => WorkspaceFocus::Browser,
        }
    }

    /// Cycle to previous panel (Shift+Tab)
    pub fn prev(&self) -> Self {
        match self {
            WorkspaceFocus::Browser => WorkspaceFocus::Dag,
            WorkspaceFocus::Editor => WorkspaceFocus::Browser,
            WorkspaceFocus::Dag => WorkspaceFocus::Editor,
        }
    }

    /// Panel title for status bar
    pub fn title(&self) -> &'static str {
        match self {
            WorkspaceFocus::Browser => "Browser",
            WorkspaceFocus::Editor => "Editor",
            WorkspaceFocus::Dag => "DAG",
        }
    }
}

/// Panel size ratio presets
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorkspaceRatio {
    /// 20% / 50% / 30% - balanced view
    #[default]
    Balanced,
    /// 15% / 65% / 20% - maximize editor
    EditorFocus,
    /// 35% / 45% / 20% - maximize browser
    BrowserFocus,
    /// 15% / 35% / 50% - maximize DAG
    DagFocus,
}

impl WorkspaceRatio {
    /// Cycle to next ratio (Ctrl+])
    pub fn next(&self) -> Self {
        match self {
            WorkspaceRatio::Balanced => WorkspaceRatio::EditorFocus,
            WorkspaceRatio::EditorFocus => WorkspaceRatio::BrowserFocus,
            WorkspaceRatio::BrowserFocus => WorkspaceRatio::DagFocus,
            WorkspaceRatio::DagFocus => WorkspaceRatio::Balanced,
        }
    }

    /// Get layout constraints for this ratio
    pub fn constraints(&self) -> [Constraint; 3] {
        match self {
            WorkspaceRatio::Balanced => [
                Constraint::Percentage(20),
                Constraint::Percentage(50),
                Constraint::Percentage(30),
            ],
            WorkspaceRatio::EditorFocus => [
                Constraint::Percentage(15),
                Constraint::Percentage(65),
                Constraint::Percentage(20),
            ],
            WorkspaceRatio::BrowserFocus => [
                Constraint::Percentage(35),
                Constraint::Percentage(45),
                Constraint::Percentage(20),
            ],
            WorkspaceRatio::DagFocus => [
                Constraint::Percentage(15),
                Constraint::Percentage(35),
                Constraint::Percentage(50),
            ],
        }
    }

    /// Label for status bar
    pub fn label(&self) -> &'static str {
        match self {
            WorkspaceRatio::Balanced => "Balanced",
            WorkspaceRatio::EditorFocus => "Editor+",
            WorkspaceRatio::BrowserFocus => "Browser+",
            WorkspaceRatio::DagFocus => "DAG+",
        }
    }
}

/// Unified 3-panel workspace view
pub struct WorkspaceView {
    /// File browser panel (left) - reuses HomeView
    pub browser: HomeView,
    /// YAML editor panel (center) - reuses StudioView
    pub editor: StudioView,
    /// Currently focused panel
    pub focus: WorkspaceFocus,
    /// Current panel ratio
    pub ratio: WorkspaceRatio,
    /// View to return to when exiting (default: Browse)
    pub exit_to: TuiView,
}

impl Default for WorkspaceView {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceView {
    /// Create a new workspace view
    pub fn new() -> Self {
        Self {
            browser: HomeView::new(std::env::current_dir().unwrap_or_default()),
            editor: StudioView::new(),
            focus: WorkspaceFocus::default(),
            ratio: WorkspaceRatio::default(),
            exit_to: TuiView::Browse,
        }
    }

    /// Create workspace with specific root directory
    #[allow(dead_code)] // Public API for future use
    pub fn with_root(root: std::path::PathBuf) -> Self {
        Self {
            browser: HomeView::new(root),
            editor: StudioView::new(),
            focus: WorkspaceFocus::default(),
            ratio: WorkspaceRatio::default(),
            exit_to: TuiView::Browse,
        }
    }

    /// Open a file in the editor panel
    pub fn open_file(&mut self, path: std::path::PathBuf) {
        // Ignore errors for now - user will see empty editor
        let _ = self.editor.load_file(path);
        self.focus = WorkspaceFocus::Editor;
    }

    /// Get border style based on focus state
    fn border_style(&self, panel: WorkspaceFocus, theme: &Theme) -> Style {
        if self.focus == panel {
            Style::default()
                .fg(theme.border_focused)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.border_normal)
        }
    }

    /// Render the DAG preview panel (placeholder for now)
    fn render_dag_panel(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let style = self.border_style(WorkspaceFocus::Dag, theme);
        let block = Block::default()
            .title(Span::styled(" DAG Preview ", style))
            .borders(Borders::ALL)
            .border_style(style);

        // Placeholder content - will be replaced with actual DAG visualization
        let content = if self.editor.path.is_some() {
            vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  DAG visualization coming soon...",
                    Style::default().fg(theme.text_muted),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "  This panel will show:",
                    Style::default().fg(theme.text_primary),
                )),
                Line::from(Span::styled(
                    "  - Task dependency graph",
                    Style::default().fg(theme.text_muted),
                )),
                Line::from(Span::styled(
                    "  - Flow connections",
                    Style::default().fg(theme.text_muted),
                )),
                Line::from(Span::styled(
                    "  - Real-time validation",
                    Style::default().fg(theme.text_muted),
                )),
            ]
        } else {
            vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  No workflow loaded",
                    Style::default().fg(theme.text_muted),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "  Open a .nika.yaml file to see its DAG",
                    Style::default().fg(theme.text_muted),
                )),
            ]
        };

        let paragraph = Paragraph::new(content).block(block);
        frame.render_widget(paragraph, area);
    }
}

impl View for WorkspaceView {
    fn render(&mut self, frame: &mut Frame, area: Rect, state: &TuiState, theme: &Theme) {
        // Split into 3 horizontal panels
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(self.ratio.constraints())
            .split(area);

        // Render browser panel (left)
        let browser_style = self.border_style(WorkspaceFocus::Browser, theme);
        let browser_block = Block::default()
            .title(Span::styled(" Browser ", browser_style))
            .borders(Borders::ALL)
            .border_style(browser_style);

        // Render browser content inside block
        let browser_inner = browser_block.inner(chunks[0]);
        frame.render_widget(browser_block, chunks[0]);
        self.browser.render(frame, browser_inner, state, theme);

        // Render editor panel (center)
        let editor_style = self.border_style(WorkspaceFocus::Editor, theme);
        let editor_block = Block::default()
            .title(Span::styled(" Editor ", editor_style))
            .borders(Borders::ALL)
            .border_style(editor_style);

        let editor_inner = editor_block.inner(chunks[1]);
        frame.render_widget(editor_block, chunks[1]);
        self.editor.render(frame, editor_inner, state, theme);

        // Render DAG panel (right)
        self.render_dag_panel(frame, chunks[2], theme);
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut TuiState) -> ViewAction {
        // Global workspace shortcuts
        match (key.code, key.modifiers) {
            // F10: Exit workspace
            (KeyCode::F(10), KeyModifiers::NONE) => {
                return ViewAction::SwitchView(self.exit_to);
            }
            // Tab: Next panel
            (KeyCode::Tab, KeyModifiers::NONE) => {
                self.focus = self.focus.next();
                return ViewAction::None;
            }
            // Shift+Tab: Previous panel
            (KeyCode::BackTab, _) => {
                self.focus = self.focus.prev();
                return ViewAction::None;
            }
            // Ctrl+]: Cycle ratio
            (KeyCode::Char(']'), KeyModifiers::CONTROL) => {
                self.ratio = self.ratio.next();
                return ViewAction::None;
            }
            _ => {}
        }

        // Delegate to focused panel
        match self.focus {
            WorkspaceFocus::Browser => {
                let action = self.browser.handle_key(key, state);
                // If browser selects a file, open it in editor
                if let ViewAction::OpenInStudio(path) = action {
                    self.open_file(path);
                    return ViewAction::None;
                }
                action
            }
            WorkspaceFocus::Editor => self.editor.handle_key(key, state),
            WorkspaceFocus::Dag => {
                // DAG panel is read-only for now
                ViewAction::None
            }
        }
    }

    fn status_line(&self, _state: &TuiState) -> String {
        format!(
            "Workspace | {} | {} | [Tab] Panel • [Ctrl+]] Ratio • [F10] Exit",
            self.focus.title(),
            self.ratio.label()
        )
    }

    fn tick(&mut self, _state: &mut TuiState) {
        // Tick child views (they use their own tick() without state)
        self.browser.tick();
        self.editor.tick();
    }

    fn on_enter(&mut self, state: &mut TuiState) {
        // Initialize both child views
        self.browser.on_enter(state);
        self.editor.on_enter(state);
    }

    fn on_leave(&mut self, state: &mut TuiState) {
        // Cleanup child views
        self.browser.on_leave(state);
        self.editor.on_leave(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_focus_next_cycles() {
        assert_eq!(WorkspaceFocus::Browser.next(), WorkspaceFocus::Editor);
        assert_eq!(WorkspaceFocus::Editor.next(), WorkspaceFocus::Dag);
        assert_eq!(WorkspaceFocus::Dag.next(), WorkspaceFocus::Browser);
    }

    #[test]
    fn test_workspace_focus_prev_cycles() {
        assert_eq!(WorkspaceFocus::Browser.prev(), WorkspaceFocus::Dag);
        assert_eq!(WorkspaceFocus::Editor.prev(), WorkspaceFocus::Browser);
        assert_eq!(WorkspaceFocus::Dag.prev(), WorkspaceFocus::Editor);
    }

    #[test]
    fn test_workspace_ratio_next_cycles() {
        assert_eq!(WorkspaceRatio::Balanced.next(), WorkspaceRatio::EditorFocus);
        assert_eq!(
            WorkspaceRatio::EditorFocus.next(),
            WorkspaceRatio::BrowserFocus
        );
        assert_eq!(
            WorkspaceRatio::BrowserFocus.next(),
            WorkspaceRatio::DagFocus
        );
        assert_eq!(WorkspaceRatio::DagFocus.next(), WorkspaceRatio::Balanced);
    }

    #[test]
    fn test_workspace_ratio_constraints() {
        let balanced = WorkspaceRatio::Balanced.constraints();
        assert_eq!(balanced[0], Constraint::Percentage(20));
        assert_eq!(balanced[1], Constraint::Percentage(50));
        assert_eq!(balanced[2], Constraint::Percentage(30));

        let editor = WorkspaceRatio::EditorFocus.constraints();
        assert_eq!(editor[0], Constraint::Percentage(15));
        assert_eq!(editor[1], Constraint::Percentage(65));
        assert_eq!(editor[2], Constraint::Percentage(20));
    }

    #[test]
    fn test_workspace_view_new() {
        let view = WorkspaceView::new();
        assert_eq!(view.focus, WorkspaceFocus::Browser);
        assert_eq!(view.ratio, WorkspaceRatio::Balanced);
        assert_eq!(view.exit_to, TuiView::Browse);
    }

    #[test]
    fn test_workspace_focus_titles() {
        assert_eq!(WorkspaceFocus::Browser.title(), "Browser");
        assert_eq!(WorkspaceFocus::Editor.title(), "Editor");
        assert_eq!(WorkspaceFocus::Dag.title(), "DAG");
    }

    #[test]
    fn test_workspace_ratio_labels() {
        assert_eq!(WorkspaceRatio::Balanced.label(), "Balanced");
        assert_eq!(WorkspaceRatio::EditorFocus.label(), "Editor+");
        assert_eq!(WorkspaceRatio::BrowserFocus.label(), "Browser+");
        assert_eq!(WorkspaceRatio::DagFocus.label(), "DAG+");
    }

    // =========================================================================
    // KEY HANDLER TESTS (C2 fix - comprehensive coverage)
    // =========================================================================

    #[test]
    fn test_handle_key_f10_exits_workspace() {
        let mut view = WorkspaceView::new();
        view.exit_to = TuiView::Browse;
        let mut state = TuiState::new("test.nika.yaml");

        let key = KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE);
        let action = view.handle_key(key, &mut state);

        assert!(matches!(action, ViewAction::SwitchView(TuiView::Browse)));
    }

    #[test]
    fn test_handle_key_f10_exits_to_configured_view() {
        let mut view = WorkspaceView::new();
        view.exit_to = TuiView::Editor;
        let mut state = TuiState::new("test.nika.yaml");

        let key = KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE);
        let action = view.handle_key(key, &mut state);

        assert!(matches!(action, ViewAction::SwitchView(TuiView::Editor)));
    }

    #[test]
    fn test_handle_key_tab_cycles_focus_forward() {
        let mut view = WorkspaceView::new();
        let mut state = TuiState::new("test.nika.yaml");
        assert_eq!(view.focus, WorkspaceFocus::Browser);

        let key = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);

        // Browser -> Editor
        let action = view.handle_key(key, &mut state);
        assert!(matches!(action, ViewAction::None));
        assert_eq!(view.focus, WorkspaceFocus::Editor);

        // Editor -> Dag
        let action = view.handle_key(key, &mut state);
        assert!(matches!(action, ViewAction::None));
        assert_eq!(view.focus, WorkspaceFocus::Dag);

        // Dag -> Browser (cycle)
        let action = view.handle_key(key, &mut state);
        assert!(matches!(action, ViewAction::None));
        assert_eq!(view.focus, WorkspaceFocus::Browser);
    }

    #[test]
    fn test_handle_key_backtab_cycles_focus_backward() {
        let mut view = WorkspaceView::new();
        let mut state = TuiState::new("test.nika.yaml");
        assert_eq!(view.focus, WorkspaceFocus::Browser);

        let key = KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT);

        // Browser -> Dag (reverse)
        let action = view.handle_key(key, &mut state);
        assert!(matches!(action, ViewAction::None));
        assert_eq!(view.focus, WorkspaceFocus::Dag);

        // Dag -> Editor
        let action = view.handle_key(key, &mut state);
        assert!(matches!(action, ViewAction::None));
        assert_eq!(view.focus, WorkspaceFocus::Editor);

        // Editor -> Browser (cycle)
        let action = view.handle_key(key, &mut state);
        assert!(matches!(action, ViewAction::None));
        assert_eq!(view.focus, WorkspaceFocus::Browser);
    }

    #[test]
    fn test_handle_key_ctrl_bracket_cycles_ratio() {
        let mut view = WorkspaceView::new();
        let mut state = TuiState::new("test.nika.yaml");
        assert_eq!(view.ratio, WorkspaceRatio::Balanced);

        let key = KeyEvent::new(KeyCode::Char(']'), KeyModifiers::CONTROL);

        // Balanced -> EditorFocus
        let action = view.handle_key(key, &mut state);
        assert!(matches!(action, ViewAction::None));
        assert_eq!(view.ratio, WorkspaceRatio::EditorFocus);

        // EditorFocus -> BrowserFocus
        let action = view.handle_key(key, &mut state);
        assert!(matches!(action, ViewAction::None));
        assert_eq!(view.ratio, WorkspaceRatio::BrowserFocus);

        // BrowserFocus -> DagFocus
        let action = view.handle_key(key, &mut state);
        assert!(matches!(action, ViewAction::None));
        assert_eq!(view.ratio, WorkspaceRatio::DagFocus);

        // DagFocus -> Balanced (cycle)
        let action = view.handle_key(key, &mut state);
        assert!(matches!(action, ViewAction::None));
        assert_eq!(view.ratio, WorkspaceRatio::Balanced);
    }

    #[test]
    fn test_handle_key_dag_panel_is_readonly() {
        let mut view = WorkspaceView::new();
        view.focus = WorkspaceFocus::Dag;
        let mut state = TuiState::new("test.nika.yaml");

        // Any key in DAG panel returns None
        let keys = [
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
        ];

        for key in keys {
            let action = view.handle_key(key, &mut state);
            assert!(
                matches!(action, ViewAction::None),
                "DAG panel should ignore key: {:?}",
                key
            );
        }
    }

    #[test]
    fn test_handle_key_unhandled_returns_none() {
        let mut view = WorkspaceView::new();
        view.focus = WorkspaceFocus::Dag; // Use Dag to avoid delegation
        let mut state = TuiState::new("test.nika.yaml");

        // Random unhandled key
        let key = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT);
        let action = view.handle_key(key, &mut state);

        assert!(matches!(action, ViewAction::None));
    }

    #[test]
    fn test_status_line_shows_focus_and_ratio() {
        let mut view = WorkspaceView::new();
        let state = TuiState::new("test.nika.yaml");

        // Default state
        let status = view.status_line(&state);
        assert!(status.contains("Workspace"));
        assert!(status.contains("Browser"));
        assert!(status.contains("Balanced"));

        // Change focus
        view.focus = WorkspaceFocus::Editor;
        let status = view.status_line(&state);
        assert!(status.contains("Editor"));

        // Change ratio
        view.ratio = WorkspaceRatio::DagFocus;
        let status = view.status_line(&state);
        assert!(status.contains("DAG+"));
    }

    #[test]
    fn test_open_file_switches_to_editor_focus() {
        let mut view = WorkspaceView::new();
        assert_eq!(view.focus, WorkspaceFocus::Browser);

        // Opening a file should switch focus to Editor
        view.open_file(std::path::PathBuf::from("/tmp/test.nika.yaml"));
        assert_eq!(view.focus, WorkspaceFocus::Editor);
    }

    #[test]
    fn test_border_style_focused_vs_normal() {
        let view = WorkspaceView::new();
        let theme = Theme::default();

        // Focused panel gets focused border
        let focused_style = view.border_style(WorkspaceFocus::Browser, &theme);
        assert!(focused_style.add_modifier.contains(Modifier::BOLD));

        // Non-focused panel gets normal border
        let normal_style = view.border_style(WorkspaceFocus::Editor, &theme);
        assert!(!normal_style.add_modifier.contains(Modifier::BOLD));
    }
}
