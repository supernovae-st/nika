//! Home View - Workflow browser with file tree and preview
//!
//! Layout:
//! ```text
//! +-----------------------------------+---------------------------------------------+
//! | FILES (40%)                       | PREVIEW (60%)                               |
//! | Tree view of .nika.yaml files     | YAML syntax highlighted                     |
//! +-----------------------------------+---------------------------------------------+
//! | HISTORY: recent workflow runs (toggleable with [h])                             |
//! +---------------------------------------------------------------------------------+
//! ```

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

use super::trait_view::View;
use super::ViewAction;
use crate::tui::standalone::{BrowserEntry, StandaloneState};
use crate::tui::state::TuiState;
use crate::tui::theme::Theme;
use crate::tui::views::TuiView;

/// Home view state
pub struct HomeView {
    /// File browser state (reuses StandaloneState from standalone.rs)
    pub standalone: StandaloneState,
    /// List state for file selection (ratatui ListState)
    pub list_state: ListState,
    /// Whether history bar is expanded
    pub history_expanded: bool,
    /// Show welcome screen (v0.5.2+)
    pub show_welcome: bool,
}

impl HomeView {
    /// Create a new HomeView for the given root directory
    pub fn new(root: PathBuf) -> Self {
        let standalone = StandaloneState::new(root);
        let mut list_state = ListState::default();
        let show_welcome = standalone.browser_entries.is_empty();
        if !standalone.browser_entries.is_empty() {
            list_state.select(Some(0));
        }
        Self {
            standalone,
            list_state,
            history_expanded: false,
            show_welcome,
        }
    }

    /// Get currently selected entry
    pub fn selected_entry(&self) -> Option<&BrowserEntry> {
        self.list_state
            .selected()
            .and_then(|i| self.standalone.browser_entries.get(i))
    }

    /// Move selection up
    pub fn select_prev(&mut self) {
        if let Some(selected) = self.list_state.selected() {
            if selected > 0 {
                self.list_state.select(Some(selected - 1));
                self.standalone.browser_index = selected - 1;
                self.standalone.update_preview();
            }
        }
    }

    /// Move selection down
    pub fn select_next(&mut self) {
        if let Some(selected) = self.list_state.selected() {
            if selected < self.standalone.browser_entries.len().saturating_sub(1) {
                self.list_state.select(Some(selected + 1));
                self.standalone.browser_index = selected + 1;
                self.standalone.update_preview();
            }
        }
    }

    /// Toggle folder open/closed (for directory entries)
    pub fn toggle_folder(&mut self) {
        if let Some(entry) = self.selected_entry() {
            if entry.is_dir {
                // Toggle expanded state
                if let Some(selected) = self.list_state.selected() {
                    if let Some(entry) = self.standalone.browser_entries.get_mut(selected) {
                        entry.expanded = !entry.expanded;
                    }
                }
            }
        }
    }

    /// Render the files panel (left 40%)
    fn render_files(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let items: Vec<ListItem> = self
            .standalone
            .browser_entries
            .iter()
            .map(|entry| {
                let icon = if entry.is_dir {
                    if entry.expanded {
                        "v "
                    } else {
                        "> "
                    }
                } else {
                    "  "
                };
                let indent = "  ".repeat(entry.depth);
                let name = entry
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| entry.display_name.clone());
                ListItem::new(format!("{}{}{}", indent, icon, name))
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" FILES ")
                    .border_style(Style::default().fg(theme.border_normal)),
            )
            .highlight_style(
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        frame.render_stateful_widget(list, area, &mut self.list_state.clone());
    }

    /// Render the preview panel (right 60%)
    fn render_preview(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let content = if let Some(entry) = self.selected_entry() {
            if entry.is_dir {
                "Select a workflow file to preview".to_string()
            } else {
                self.standalone.preview_content.clone()
            }
        } else {
            "No file selected".to_string()
        };

        // Add line numbers
        let lines: Vec<Line> = content
            .lines()
            .enumerate()
            .map(|(i, line)| {
                Line::from(vec![
                    Span::styled(
                        format!("{:4} | ", i + 1),
                        Style::default().fg(theme.text_muted),
                    ),
                    Span::raw(line),
                ])
            })
            .collect();

        let paragraph = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" PREVIEW ")
                .border_style(Style::default().fg(theme.border_normal)),
        );

        frame.render_widget(paragraph, area);
    }

    /// Render the welcome screen (v0.5.2+)
    fn render_welcome(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let welcome_lines = vec![
            Line::from(vec![
                Span::styled("🐔 ", Style::default()),
                Span::styled(
                    "Welcome to Nika",
                    Style::default()
                        .fg(theme.highlight)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" v0.5.2", Style::default().fg(theme.text_muted)),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Semantic YAML workflow engine for AI tasks",
                Style::default().fg(theme.text_primary),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "── Quick Start ──",
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("  1. ", Style::default().fg(theme.text_muted)),
                Span::raw("Create a workflow: "),
                Span::styled("example.nika.yaml", Style::default().fg(theme.highlight)),
            ]),
            Line::from(vec![
                Span::styled("  2. ", Style::default().fg(theme.text_muted)),
                Span::raw("Run it: "),
                Span::styled(
                    "nika run example.nika.yaml",
                    Style::default().fg(theme.highlight),
                ),
            ]),
            Line::from(vec![
                Span::styled("  3. ", Style::default().fg(theme.text_muted)),
                Span::raw("Or browse files here with "),
                Span::styled("↑/↓", Style::default().fg(theme.highlight)),
                Span::raw(" and "),
                Span::styled("Enter", Style::default().fg(theme.highlight)),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "── Keybindings ──",
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Tab     ", Style::default().fg(theme.highlight)),
                Span::raw("Switch view (Chat/Home/Studio/Monitor)"),
            ]),
            Line::from(vec![
                Span::styled("  ↑/↓     ", Style::default().fg(theme.highlight)),
                Span::raw("Navigate files"),
            ]),
            Line::from(vec![
                Span::styled("  Enter   ", Style::default().fg(theme.highlight)),
                Span::raw("Run workflow / Open folder"),
            ]),
            Line::from(vec![
                Span::styled("  e       ", Style::default().fg(theme.highlight)),
                Span::raw("Edit in Studio"),
            ]),
            Line::from(vec![
                Span::styled("  h       ", Style::default().fg(theme.highlight)),
                Span::raw("Toggle history"),
            ]),
            Line::from(vec![
                Span::styled("  ?       ", Style::default().fg(theme.highlight)),
                Span::raw("Help overlay"),
            ]),
            Line::from(vec![
                Span::styled("  q       ", Style::default().fg(theme.highlight)),
                Span::raw("Quit"),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "── 5 Verbs ──",
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("  ⚡ infer:  ", Style::default().fg(theme.highlight)),
                Span::raw("LLM text generation"),
            ]),
            Line::from(vec![
                Span::styled("  📟 exec:   ", Style::default().fg(theme.highlight)),
                Span::raw("Shell command"),
            ]),
            Line::from(vec![
                Span::styled("  🛰️ fetch:  ", Style::default().fg(theme.highlight)),
                Span::raw("HTTP request"),
            ]),
            Line::from(vec![
                Span::styled("  🔌 invoke: ", Style::default().fg(theme.highlight)),
                Span::raw("MCP tool call"),
            ]),
            Line::from(vec![
                Span::styled("  🐔 agent:  ", Style::default().fg(theme.highlight)),
                Span::raw("Multi-turn agentic loop"),
            ]),
        ];

        let paragraph = Paragraph::new(welcome_lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" WELCOME ")
                .border_style(Style::default().fg(theme.border_normal)),
        );

        frame.render_widget(paragraph, area);
    }

    /// Render the history bar (bottom, toggleable)
    fn render_history(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let max_items = if self.history_expanded { 10 } else { 5 };
        let items: Vec<Span> = self
            .standalone
            .history
            .iter()
            .rev() // Most recent first
            .take(max_items)
            .map(|h| {
                let status = if h.success { "+" } else { "x" };
                let color = if h.success {
                    theme.status_success
                } else {
                    theme.status_failed
                };
                let name = h
                    .workflow_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                Span::styled(
                    format!(" {} {} ({}) ", status, name, h.duration_display()),
                    Style::default().fg(color),
                )
            })
            .collect();

        let toggle_hint = if self.history_expanded { "^" } else { "v" };
        let title = format!(" HISTORY [h] {} ", toggle_hint);

        let content = if items.is_empty() {
            Line::from(Span::styled(
                " No history yet ",
                Style::default().fg(theme.text_muted),
            ))
        } else {
            Line::from(items)
        };

        let paragraph = Paragraph::new(content).block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(Style::default().fg(theme.border_normal)),
        );

        frame.render_widget(paragraph, area);
    }
}

impl View for HomeView {
    fn render(&self, frame: &mut Frame, area: Rect, _state: &TuiState, theme: &Theme) {
        // Show welcome screen when no files or explicitly requested
        if self.show_welcome || self.standalone.browser_entries.is_empty() {
            // Layout: Welcome (60%) | Tips (40%) above, History bar below
            let history_height = if self.history_expanded { 6 } else { 3 };

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(10), Constraint::Length(history_height)])
                .split(area);

            // Render welcome screen in main area
            self.render_welcome(frame, chunks[0], theme);

            // History bar (bottom)
            self.render_history(frame, chunks[1], theme);
            return;
        }

        // Normal layout: Files (40%) | Preview (60%) above, History bar below
        let history_height = if self.history_expanded { 6 } else { 3 };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(10), Constraint::Length(history_height)])
            .split(area);

        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(chunks[0]);

        // Files panel (left 40%)
        self.render_files(frame, main_chunks[0], theme);

        // Preview panel (right 60%)
        self.render_preview(frame, main_chunks[1], theme);

        // History bar (bottom)
        self.render_history(frame, chunks[1], theme);
    }

    fn handle_key(&mut self, key: KeyEvent, _state: &mut TuiState) -> ViewAction {
        match key.code {
            // Quit
            KeyCode::Char('q') => ViewAction::Quit,

            // Navigation: j/k or up/down
            KeyCode::Up | KeyCode::Char('k') => {
                self.select_prev();
                ViewAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.select_next();
                ViewAction::None
            }

            // Enter: run workflow or toggle folder
            KeyCode::Enter => {
                if let Some(entry) = self.selected_entry() {
                    if entry.is_dir {
                        self.toggle_folder();
                        ViewAction::None
                    } else {
                        ViewAction::RunWorkflow(entry.path.clone())
                    }
                } else {
                    ViewAction::None
                }
            }

            // Edit: open in Studio
            KeyCode::Char('e') => {
                if let Some(entry) = self.selected_entry() {
                    if !entry.is_dir {
                        return ViewAction::OpenInStudio(entry.path.clone());
                    }
                }
                ViewAction::None
            }

            // Toggle history expansion
            KeyCode::Char('h') => {
                self.history_expanded = !self.history_expanded;
                ViewAction::None
            }

            // Chat overlay toggle
            KeyCode::Char('c') => ViewAction::ToggleChatOverlay,

            // View switching: number keys and shortcuts
            KeyCode::Char('1') | KeyCode::Char('a') => ViewAction::SwitchView(TuiView::Chat),
            KeyCode::Char('3') | KeyCode::Char('s') => ViewAction::SwitchView(TuiView::Studio),
            KeyCode::Char('4') | KeyCode::Char('m') => ViewAction::SwitchView(TuiView::Monitor),

            // Tab: handled at app level for view cycling
            KeyCode::Tab => ViewAction::None,

            _ => ViewAction::None,
        }
    }

    fn status_line(&self, _state: &TuiState) -> String {
        let workflow_count = self
            .standalone
            .browser_entries
            .iter()
            .filter(|e| !e.is_dir)
            .count();
        let history_count = self.standalone.history.len();
        format!(
            "{} workflows | {} in history",
            workflow_count, history_count
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_home_view_new_creates_valid_state() {
        let view = HomeView::new(PathBuf::from("."));
        assert!(!view.history_expanded);
        // ListState is initialized
        assert!(view.list_state.selected().is_none() || view.list_state.selected().is_some());
    }

    #[test]
    fn test_home_view_select_navigation() {
        let mut view = HomeView::new(PathBuf::from("."));

        // Add some mock entries for testing
        view.standalone.browser_entries.clear();
        view.standalone.browser_entries.push(BrowserEntry::new(
            PathBuf::from("test1.nika.yaml"),
            &PathBuf::from("."),
        ));
        view.standalone.browser_entries.push(BrowserEntry::new(
            PathBuf::from("test2.nika.yaml"),
            &PathBuf::from("."),
        ));
        view.list_state.select(Some(0));

        // Navigate down
        view.select_next();
        assert_eq!(view.list_state.selected(), Some(1));

        // Navigate up
        view.select_prev();
        assert_eq!(view.list_state.selected(), Some(0));

        // Navigate up at top (should stay at 0)
        view.select_prev();
        assert_eq!(view.list_state.selected(), Some(0));
    }

    #[test]
    fn test_home_view_history_toggle() {
        let mut view = HomeView::new(PathBuf::from("."));
        assert!(!view.history_expanded);

        view.history_expanded = true;
        assert!(view.history_expanded);

        view.history_expanded = false;
        assert!(!view.history_expanded);
    }

    #[test]
    fn test_home_view_selected_entry_with_empty_list() {
        let mut view = HomeView::new(PathBuf::from("."));
        view.standalone.browser_entries.clear();
        view.list_state.select(None);

        assert!(view.selected_entry().is_none());
    }

    #[test]
    fn test_home_view_status_line() {
        let mut view = HomeView::new(PathBuf::from("."));
        view.standalone.browser_entries.clear();
        view.standalone.browser_entries.push(BrowserEntry::new(
            PathBuf::from("test.nika.yaml"),
            &PathBuf::from("."),
        ));
        view.standalone.history.clear();

        let state = TuiState::new("test.nika.yaml");
        let status = view.status_line(&state);
        assert!(status.contains("1 workflows"));
        assert!(status.contains("0 in history"));
    }

    // === Welcome Screen Tests (MEDIUM 13) ===

    #[test]
    fn test_welcome_shows_when_no_workflows() {
        // Use a non-existent directory so StandaloneState starts empty
        let view = HomeView::new(PathBuf::from("/nonexistent/path/that/has/no/nika/files"));

        // show_welcome should be true when no entries exist
        assert!(
            view.standalone.browser_entries.is_empty(),
            "Browser should be empty for non-existent path"
        );
        assert!(view.show_welcome, "Welcome should show when no workflows");
    }

    #[test]
    fn test_welcome_hides_when_workflows_exist() {
        let mut view = HomeView::new(PathBuf::from("."));
        view.standalone.browser_entries.clear();
        view.standalone.browser_entries.push(BrowserEntry::new(
            PathBuf::from("test.nika.yaml"),
            &PathBuf::from("."),
        ));

        // Re-evaluate show_welcome based on entries
        view.show_welcome = view.standalone.browser_entries.is_empty();

        assert!(
            !view.show_welcome,
            "Welcome should hide when workflows exist"
        );
    }

    #[test]
    fn test_welcome_screen_renders_without_panic() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let mut view = HomeView::new(PathBuf::from("."));
        view.standalone.browser_entries.clear();
        view.show_welcome = true;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let state = TuiState::new("test.nika.yaml");
        let theme = Theme::novanet();

        terminal
            .draw(|frame| {
                view.render(frame, frame.area(), &state, &theme);
            })
            .unwrap();

        // Check that the welcome screen contains expected elements
        let buffer = terminal.backend().buffer();
        let content: String = buffer.content.iter().map(|c| c.symbol()).collect();

        // Check for branding
        assert!(
            content.contains("Nika"),
            "Welcome should show Nika branding"
        );

        // Check for version
        assert!(
            content.contains("v0.5.2"),
            "Welcome should show version number"
        );
    }

    #[test]
    fn test_welcome_screen_contains_quick_start() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let mut view = HomeView::new(PathBuf::from("."));
        view.standalone.browser_entries.clear();
        view.show_welcome = true;

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let state = TuiState::new("test.nika.yaml");
        let theme = Theme::novanet();

        terminal
            .draw(|frame| {
                view.render(frame, frame.area(), &state, &theme);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content.iter().map(|c| c.symbol()).collect();

        // Check for quick start hints
        assert!(
            content.contains("QUICK START") || content.contains("Quick Start"),
            "Welcome should contain quick start section"
        );
    }

    #[test]
    fn test_welcome_screen_contains_keybindings() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let mut view = HomeView::new(PathBuf::from("."));
        view.standalone.browser_entries.clear();
        view.show_welcome = true;

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let state = TuiState::new("test.nika.yaml");
        let theme = Theme::novanet();

        terminal
            .draw(|frame| {
                view.render(frame, frame.area(), &state, &theme);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content.iter().map(|c| c.symbol()).collect();

        // Check for keybinding hints
        assert!(
            content.contains("Tab") || content.contains("⇥"),
            "Welcome should show Tab keybinding"
        );
        assert!(
            content.contains("Enter") || content.contains("⏎"),
            "Welcome should show Enter keybinding"
        );
    }
}
