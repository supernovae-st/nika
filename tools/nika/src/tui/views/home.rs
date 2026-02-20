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

// Allow dead code - HomeView will be integrated in Task 5.1
#![allow(dead_code)]

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
}

impl HomeView {
    /// Create a new HomeView for the given root directory
    pub fn new(root: PathBuf) -> Self {
        let standalone = StandaloneState::new(root);
        let mut list_state = ListState::default();
        if !standalone.browser_entries.is_empty() {
            list_state.select(Some(0));
        }
        Self {
            standalone,
            list_state,
            history_expanded: false,
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
        // Layout: Files (40%) | Preview (60%) above, History bar below
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

            // Tab: cycle to next view (Studio)
            KeyCode::Tab => ViewAction::SwitchView(TuiView::Studio),

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
}
