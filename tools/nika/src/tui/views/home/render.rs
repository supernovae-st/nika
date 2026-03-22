//! Rendering logic for HomeView
//!
//! Contains the files panel, history bar, and the top-level View trait implementation
//! (render + status_line). The preview panel is in preview.rs.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph, Widget},
    Frame,
};

use crossterm::event::KeyEvent;

use super::HomeView;
use crate::tui::state::TuiState;
use crate::tui::theme::{TaskStatus, Theme};
use crate::tui::views::view_trait::View;
use crate::tui::views::ViewAction;
use crate::tui::widgets::{
    AnimatedLatencySparkline, MatrixRain, SparklineAnimation, Timeline, TimelineEntry,
};

impl HomeView {
    /// Render the files panel (left 40%)
    pub(crate) fn render_files(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // Split area for search bar if active
        let (search_area, list_area) = if self.search_active {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(5)])
                .split(area);
            (Some(chunks[0]), chunks[1])
        } else {
            (None, area)
        };

        // Render search bar if active
        if let Some(search_area) = search_area {
            let search_text = format!("🔍 {}", self.search_query);
            let search_bar = Paragraph::new(search_text)
                .style(Style::default().fg(theme.text_primary))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" SEARCH (Esc to cancel) ")
                        .border_style(Style::default().fg(theme.highlight)),
                );
            frame.render_widget(search_bar, search_area);
        }

        // Use filtered entries (📁 folder, 📄 file)
        let items: Vec<ListItem> = self
            .filtered_entries()
            .iter()
            .map(|entry| {
                let icon = if entry.is_dir {
                    if entry.expanded {
                        "📂 " // Open folder
                    } else {
                        "📁 " // Closed folder
                    }
                } else {
                    "📄 " // Workflow file
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

        // Build title with project root and .nika indicator
        // NOTE: has_nika_dir is cached at HomeView creation to avoid syscall per frame
        let project_name = self
            .standalone
            .root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "~".to_string());
        let nika_marker = if self.has_nika_dir { " ◆" } else { "" };

        let title = if self.search_active && !self.search_query.is_empty() {
            format!(
                " 📁 {}{} ({} matches) ",
                project_name,
                nika_marker,
                self.filtered_indices.len()
            )
        } else {
            format!(" 📁 {}{} ", project_name, nika_marker)
        };

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(Style::default().fg(theme.border_normal)),
            )
            .highlight_style(
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        frame.render_stateful_widget(list, list_area, &mut self.list_state.clone());
    }

    /// Render the history bar (bottom, toggleable)
    /// Uses Timeline widget for visual history display
    pub(crate) fn render_history(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let toggle_hint = if self.history_expanded { "^" } else { "v" };
        let title = format!(" HISTORY [h] {} ", toggle_hint);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(theme.border_normal));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if self.standalone.history.is_empty() {
            let empty_msg =
                Paragraph::new(" No history yet ").style(Style::default().fg(theme.text_muted));
            frame.render_widget(empty_msg, inner);
            return;
        }

        // Convert HistoryEntry to TimelineEntry for visual display
        let max_items = if self.history_expanded { 10 } else { 5 };
        let entries: Vec<TimelineEntry> = self
            .standalone
            .history
            .iter()
            .rev() // Most recent first
            .take(max_items)
            .enumerate()
            .map(|(i, h)| {
                let name = h
                    .workflow_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unknown".to_string());

                let status = if h.success {
                    TaskStatus::Success
                } else {
                    TaskStatus::Failed
                };

                let mut entry = TimelineEntry::new(name, status).with_duration(h.duration_ms);
                if i == 0 {
                    entry = entry.current(); // Mark most recent
                }
                entry
            })
            .collect();

        // Collect latency data for sparkline
        let latency_data: Vec<u64> = self
            .standalone
            .history
            .iter()
            .rev()
            .take(max_items)
            .map(|h| h.duration_ms)
            .collect();

        // Calculate total elapsed time from all runs
        let total_ms: u64 = latency_data.iter().sum();

        // Split inner area - Timeline (70%) | Sparkline (30%)
        let history_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(inner);

        // Left: Timeline
        let timeline = Timeline::new(&entries)
            .elapsed(total_ms)
            .with_theme(theme)
            .with_frame(self.frame);
        frame.render_widget(timeline, history_chunks[0]);

        // Right: Latency sparkline
        if !latency_data.is_empty() {
            let sparkline = AnimatedLatencySparkline::new(&latency_data)
                .frame(self.frame)
                .animation(SparklineAnimation::Pulse)
                .warn_threshold(1000) // 1s warning
                .error_threshold(5000); // 5s error
            frame.render_widget(sparkline, history_chunks[1]);
        }
    }
}

impl View for HomeView {
    fn handle_key(&mut self, key: KeyEvent, state: &mut TuiState) -> ViewAction {
        self.handle_key_event(key, state)
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _state: &TuiState, theme: &Theme) {
        // Matrix Rain background effect (full screen, renders FIRST)
        if self.matrix_effect_enabled && self.rain_opacity > 0.05 {
            let rain_area = Rect {
                x: area.x + 1,
                y: area.y + 1,
                width: area.width.saturating_sub(2),
                height: area.height.saturating_sub(2),
            };
            MatrixRain::new()
                .frame(self.frame)
                .density(0.08) // Very subtle, smooth
                .opacity(self.rain_opacity)
                .with_mascots(true)
                .render(rain_area, frame.buffer_mut());
        }

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
