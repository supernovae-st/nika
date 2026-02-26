//! Split View - Side-by-side Editor + Runner (v0.13)
//!
//! Combines Editor (YAML editing) and Runner (execution monitoring) in a split pane.
//!
//! # Layout
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                              SPLIT VIEW                                 │
//! ├──────────────────────────────┬──────────────────────────────────────────┤
//! │       EDITOR (Left)          │           RUNNER (Right)                 │
//! │                              │                                          │
//! │  workflow.nika.yaml          │   ┌──────────┐    ┌──────────┐           │
//! │  ────────────────            │   │  task-1  │───►│  task-2  │           │
//! │  schema: nika/workflow@0.5   │   └──────────┘    └──────────┘           │
//! │  workflow: example           │                                          │
//! │                              │   Progress: 50% ████░░░░░                │
//! │  tasks:                      │   Tokens: 1.2K | Cost: $0.05             │
//! │    - id: task-1              │                                          │
//! │      infer: "Generate..."    │                                          │
//! │                              │                                          │
//! ├──────────────────────────────┴──────────────────────────────────────────┤
//! │ Split • Editor: line 5, col 12 | Runner: 2/4 tasks • [Tab] switch       │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Navigation
//!
//! - `Tab` / `Shift+Tab`: Switch focus between left (Editor) and right (Runner)
//! - `Ctrl+]`: Toggle split ratio (50/50 → 70/30 → 30/70 → 50/50)
//! - `Escape`: Exit split view (return to last single view)
//! - All other keys: Forwarded to the focused pane
//!
//! # Keyboard Shortcuts
//!
//! | Key | Action |
//! |-----|--------|
//! | `F9` | Toggle split view (from Editor or Runner) |
//! | `Tab` | Switch focus between panes |
//! | `Ctrl+]` | Cycle split ratio |
//! | `Escape` | Exit to single view |

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    widgets::{Block, Borders},
    Frame,
};

use super::{monitor::MonitorView, studio::StudioView, View, ViewAction};
use crate::tui::state::TuiState;
use crate::tui::theme::Theme;
use crate::tui::views::TuiView;

/// Which pane has focus in split view
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SplitFocus {
    /// Left pane (Editor) has focus
    #[default]
    Left,
    /// Right pane (Runner) has focus
    Right,
}

impl SplitFocus {
    /// Toggle focus between left and right
    pub fn toggle(&mut self) {
        *self = match self {
            SplitFocus::Left => SplitFocus::Right,
            SplitFocus::Right => SplitFocus::Left,
        };
    }
}

/// Split ratio presets
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SplitRatio {
    /// 50% / 50%
    #[default]
    Equal,
    /// 70% / 30% (Editor dominant)
    EditorDominant,
    /// 30% / 70% (Runner dominant)
    RunnerDominant,
}

impl SplitRatio {
    /// Cycle to next ratio
    pub fn cycle(&self) -> Self {
        match self {
            SplitRatio::Equal => SplitRatio::EditorDominant,
            SplitRatio::EditorDominant => SplitRatio::RunnerDominant,
            SplitRatio::RunnerDominant => SplitRatio::Equal,
        }
    }

    /// Get constraints for the split layout
    pub fn constraints(&self) -> [Constraint; 2] {
        match self {
            SplitRatio::Equal => [Constraint::Percentage(50), Constraint::Percentage(50)],
            SplitRatio::EditorDominant => [Constraint::Percentage(70), Constraint::Percentage(30)],
            SplitRatio::RunnerDominant => [Constraint::Percentage(30), Constraint::Percentage(70)],
        }
    }

    /// Display label
    pub fn label(&self) -> &'static str {
        match self {
            SplitRatio::Equal => "50/50",
            SplitRatio::EditorDominant => "70/30",
            SplitRatio::RunnerDominant => "30/70",
        }
    }
}

/// Split view combining Editor and Runner side by side
pub struct SplitView {
    /// Left pane: Editor (Studio) view
    pub editor: StudioView,
    /// Right pane: Runner (Monitor) view
    pub runner: MonitorView,
    /// Which pane currently has focus
    pub focus: SplitFocus,
    /// Current split ratio
    pub ratio: SplitRatio,
    /// View to return to on exit (usually Editor or Runner)
    pub exit_to: TuiView,
}

impl std::fmt::Debug for SplitView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SplitView")
            .field("focus", &self.focus)
            .field("ratio", &self.ratio)
            .field("exit_to", &self.exit_to)
            .finish_non_exhaustive()
    }
}

impl Default for SplitView {
    fn default() -> Self {
        Self::new()
    }
}

impl SplitView {
    /// Create a new split view
    pub fn new() -> Self {
        Self {
            editor: StudioView::new(),
            runner: MonitorView::new(),
            focus: SplitFocus::Left,
            ratio: SplitRatio::Equal,
            exit_to: TuiView::Editor,
        }
    }

    /// Create split view with a specific file loaded in editor
    #[allow(dead_code)] // Used in tests and future F9 integration
    pub fn with_file(path: std::path::PathBuf) -> Self {
        let mut editor = StudioView::new();
        // Try to load file, but don't fail if it doesn't exist
        let _ = editor.load_file(path);
        Self {
            editor,
            runner: MonitorView::new(),
            focus: SplitFocus::Left,
            ratio: SplitRatio::Equal,
            exit_to: TuiView::Editor,
        }
    }

    /// Set the view to return to when exiting split mode
    #[allow(dead_code)] // Used in tests and future F9 integration
    pub fn with_exit_to(mut self, view: TuiView) -> Self {
        self.exit_to = view;
        self
    }

    /// Toggle focus between panes
    pub fn toggle_focus(&mut self) {
        self.focus.toggle();
    }

    /// Cycle split ratio
    pub fn cycle_ratio(&mut self) {
        self.ratio = self.ratio.cycle();
    }

    /// Get the focused view's status
    fn focused_view_indicator(&self) -> (&'static str, &'static str) {
        match self.focus {
            SplitFocus::Left => ("◀", " "),
            SplitFocus::Right => (" ", "▶"),
        }
    }
}

impl View for SplitView {
    fn render(&mut self, frame: &mut Frame, area: Rect, state: &TuiState, theme: &Theme) {
        // Create horizontal split layout
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(self.ratio.constraints())
            .split(area);

        let (left_indicator, right_indicator) = self.focused_view_indicator();

        // Render left pane (Editor) with focus indicator
        let left_border_color = if self.focus == SplitFocus::Left {
            theme.border_focused
        } else {
            theme.border_normal
        };

        // Create a block for left pane
        let left_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(left_border_color))
            .title(format!("{}Editor", left_indicator));

        let left_inner = left_block.inner(chunks[0]);
        frame.render_widget(left_block, chunks[0]);
        self.editor.render(frame, left_inner, state, theme);

        // Render right pane (Runner) with focus indicator
        let right_border_color = if self.focus == SplitFocus::Right {
            theme.border_focused
        } else {
            theme.border_normal
        };

        let right_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(right_border_color))
            .title(format!("{}Runner", right_indicator));

        let right_inner = right_block.inner(chunks[1]);
        frame.render_widget(right_block, chunks[1]);
        self.runner.render(frame, right_inner, state, theme);
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut TuiState) -> ViewAction {
        // Global split view shortcuts
        match (key.code, key.modifiers) {
            // Escape: Exit split view
            (KeyCode::Esc, KeyModifiers::NONE) => {
                return ViewAction::SwitchView(self.exit_to);
            }

            // Tab: Toggle focus between panes
            (KeyCode::Tab, KeyModifiers::NONE) => {
                self.toggle_focus();
                return ViewAction::None;
            }

            // Shift+Tab: Also toggle focus (reverse)
            (KeyCode::BackTab, _) => {
                self.toggle_focus();
                return ViewAction::None;
            }

            // Ctrl+]: Cycle split ratio
            (KeyCode::Char(']'), KeyModifiers::CONTROL) => {
                self.cycle_ratio();
                return ViewAction::None;
            }

            // F9: Exit split view (same as Escape)
            (KeyCode::F(9), KeyModifiers::NONE) => {
                return ViewAction::SwitchView(self.exit_to);
            }

            _ => {}
        }

        // Forward key to focused pane
        match self.focus {
            SplitFocus::Left => {
                let action = self.editor.handle_key(key, state);
                // If editor tries to switch views, keep it in split mode
                match action {
                    ViewAction::SwitchView(_) => ViewAction::None,
                    other => other,
                }
            }
            SplitFocus::Right => {
                let action = self.runner.handle_key(key, state);
                // If runner tries to switch views, keep it in split mode
                match action {
                    ViewAction::SwitchView(_) => ViewAction::None,
                    other => other,
                }
            }
        }
    }

    fn status_line(&self, state: &TuiState) -> String {
        let editor_status = self.editor.status_line(state);
        let _runner_status = self.runner.status_line(state); // Available for future use
        let focus_str = match self.focus {
            SplitFocus::Left => "Editor",
            SplitFocus::Right => "Runner",
        };

        format!(
            "Split [{}] • {} • {} | [Tab] switch • [Ctrl+]] ratio",
            self.ratio.label(),
            editor_status,
            focus_str
        )
    }

    fn tick(&mut self, state: &mut TuiState) {
        // Tick both panes
        self.editor.tick();
        self.runner.tick(state);
    }

    fn on_enter(&mut self, state: &mut TuiState) {
        // Notify both views they're active
        self.editor.on_enter(state);
        self.runner.on_enter(state);
    }

    fn on_leave(&mut self, state: &mut TuiState) {
        // Notify both views they're inactive
        self.editor.on_leave(state);
        self.runner.on_leave(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_view_new() {
        let view = SplitView::new();
        assert_eq!(view.focus, SplitFocus::Left);
        assert_eq!(view.ratio, SplitRatio::Equal);
        assert_eq!(view.exit_to, TuiView::Editor);
    }

    #[test]
    fn test_split_focus_toggle() {
        let mut focus = SplitFocus::Left;
        focus.toggle();
        assert_eq!(focus, SplitFocus::Right);
        focus.toggle();
        assert_eq!(focus, SplitFocus::Left);
    }

    #[test]
    fn test_split_ratio_cycle() {
        let ratio = SplitRatio::Equal;
        assert_eq!(ratio.cycle(), SplitRatio::EditorDominant);
        assert_eq!(ratio.cycle().cycle(), SplitRatio::RunnerDominant);
        assert_eq!(ratio.cycle().cycle().cycle(), SplitRatio::Equal);
    }

    #[test]
    fn test_split_ratio_constraints() {
        let equal = SplitRatio::Equal.constraints();
        assert_eq!(equal[0], Constraint::Percentage(50));
        assert_eq!(equal[1], Constraint::Percentage(50));

        let editor = SplitRatio::EditorDominant.constraints();
        assert_eq!(editor[0], Constraint::Percentage(70));
        assert_eq!(editor[1], Constraint::Percentage(30));

        let runner = SplitRatio::RunnerDominant.constraints();
        assert_eq!(runner[0], Constraint::Percentage(30));
        assert_eq!(runner[1], Constraint::Percentage(70));
    }

    #[test]
    fn test_split_ratio_label() {
        assert_eq!(SplitRatio::Equal.label(), "50/50");
        assert_eq!(SplitRatio::EditorDominant.label(), "70/30");
        assert_eq!(SplitRatio::RunnerDominant.label(), "30/70");
    }

    #[test]
    fn test_split_view_toggle_focus() {
        let mut view = SplitView::new();
        assert_eq!(view.focus, SplitFocus::Left);
        view.toggle_focus();
        assert_eq!(view.focus, SplitFocus::Right);
        view.toggle_focus();
        assert_eq!(view.focus, SplitFocus::Left);
    }

    #[test]
    fn test_split_view_cycle_ratio() {
        let mut view = SplitView::new();
        assert_eq!(view.ratio, SplitRatio::Equal);
        view.cycle_ratio();
        assert_eq!(view.ratio, SplitRatio::EditorDominant);
        view.cycle_ratio();
        assert_eq!(view.ratio, SplitRatio::RunnerDominant);
        view.cycle_ratio();
        assert_eq!(view.ratio, SplitRatio::Equal);
    }

    #[test]
    fn test_split_view_with_exit_to() {
        let view = SplitView::new().with_exit_to(TuiView::Runner);
        assert_eq!(view.exit_to, TuiView::Runner);
    }

    #[test]
    fn test_split_view_focused_indicator() {
        let mut view = SplitView::new();

        // Left focused
        let (left, right) = view.focused_view_indicator();
        assert_eq!(left, "◀");
        assert_eq!(right, " ");

        // Right focused
        view.toggle_focus();
        let (left, right) = view.focused_view_indicator();
        assert_eq!(left, " ");
        assert_eq!(right, "▶");
    }

    #[test]
    fn test_split_view_status_line() {
        let view = SplitView::new();
        let state = TuiState::new("test.nika.yaml");
        let status = view.status_line(&state);
        assert!(status.contains("Split"));
        assert!(status.contains("50/50"));
        assert!(status.contains("Editor")); // Focused pane
    }

    #[test]
    fn test_split_view_handle_key_tab_toggles_focus() {
        let mut view = SplitView::new();
        let mut state = TuiState::new("test.nika.yaml");

        assert_eq!(view.focus, SplitFocus::Left);

        let key = KeyEvent::from(KeyCode::Tab);
        let action = view.handle_key(key, &mut state);

        assert!(matches!(action, ViewAction::None));
        assert_eq!(view.focus, SplitFocus::Right);
    }

    #[test]
    fn test_split_view_handle_key_escape_exits() {
        let mut view = SplitView::new();
        let mut state = TuiState::new("test.nika.yaml");

        let key = KeyEvent::from(KeyCode::Esc);
        let action = view.handle_key(key, &mut state);

        assert!(matches!(action, ViewAction::SwitchView(TuiView::Editor)));
    }

    #[test]
    fn test_split_view_handle_key_f9_exits() {
        let mut view = SplitView::new().with_exit_to(TuiView::Runner);
        let mut state = TuiState::new("test.nika.yaml");

        let key = KeyEvent::from(KeyCode::F(9));
        let action = view.handle_key(key, &mut state);

        assert!(matches!(action, ViewAction::SwitchView(TuiView::Runner)));
    }

    #[test]
    fn test_split_view_implements_view_trait() {
        let view = SplitView::new();
        let _: &dyn View = &view;
    }

    #[test]
    fn test_split_view_default() {
        let view = SplitView::default();
        assert_eq!(view.focus, SplitFocus::Left);
        assert_eq!(view.ratio, SplitRatio::Equal);
    }
}
