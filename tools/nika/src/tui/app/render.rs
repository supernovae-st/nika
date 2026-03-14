//! App Frame Rendering
//!
//! Contains frame rendering logic for the unified TUI architecture.
//! v0.22 4-Views: Studio, Runner, Chat, Settings (Scheduler removed)

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::widgets::{Clear, Paragraph};

use crate::error::{NikaError, Result};

use super::super::theme::TaskStatus;
use super::super::views::{TuiView, View};
use super::super::widgets::{
    check_terminal_size, ConnectionStatus, Header, NikaIntro, StatusBar, StatusMessageWidget,
    StatusMetrics,
};
use super::App;

impl App {
    /// Render the current frame based on active view
    ///
    /// Uses the View trait's render method for each view type.
    /// Each view handles its own layout and widget rendering.
    ///
    /// Layout: Header (1 line) + Content (dynamic) + StatusBar (1 line)
    pub(crate) fn render_unified_frame(&mut self) -> Result<()> {
        let current_view = self.current_view;

        if let Some(ref mut terminal) = self.terminal {
            // Ensure timeline cache is up-to-date before rendering Monitor view
            if current_view == TuiView::Runner {
                self.state.ensure_timeline_cache();
            }

            // All views use unified layout with Header + Content + StatusBar
            // Extract read-only values BEFORE taking mutable references
            // This allows render() to take &mut self for scroll state updates
            let total_tokens = self.chat_view.total_tokens();
            let provider = self.chat_view.provider();
            let chat_status = self.chat_view.status_line(&self.state);
            let _home_status = self
                .home_view
                .as_ref()
                .map(|hv| hv.status_line(&self.state))
                .unwrap_or_default();
            let studio_status = self.studio_view.status_line(&self.state);
            let monitor_status = {
                let task_count = self.state.tasks.len();
                let completed = self
                    .state
                    .tasks
                    .values()
                    .filter(|t| t.status == TaskStatus::Success)
                    .count();
                format!("Tasks: {}/{}", completed, task_count)
            };

            // Extract references to avoid borrow issues with the closure
            let theme = &self.theme;
            let state = &self.state;
            let chat_view = &mut self.chat_view;
            let _home_view = &mut self.home_view;
            let studio_view = &mut self.studio_view;
            let settings_view = &mut self.settings_view;
            let monitor_view = &mut self.monitor_view;
            let workflow_path = &self.state.workflow.path;
            let intro_state = &self.intro_state;
                                                 // P0 Fix: Use is_paused() accessor for unified pause state
            let paused = self.state.is_paused();
            let input_mode = self.input_mode;

            // Extract data for StatusBar metrics from the centralized pool
            let mcp_total = self.mcp_pool.config_count();
            let mcp_connected = self.mcp_pool.connected_count();

            // Get custom status text from current view (using pre-computed values)
            let status_text = match current_view {
                TuiView::Studio => studio_status,
                TuiView::Runner => monitor_status,
                TuiView::Chat => chat_status,
                TuiView::Settings => settings_view.status_line(state),
            };

            terminal
                .draw(|frame| {
                    let size = frame.area();

                    let layout_mode = check_terminal_size(size);

                    // If terminal is too small, show overlay and return early
                    if !layout_mode.is_usable() {
                        use super::super::widgets::TerminalTooSmallOverlay;
                        frame.render_widget(Clear, size);
                        let overlay = TerminalTooSmallOverlay::new(size.width, size.height);
                        frame.render_widget(overlay, size);
                        return;
                    }

                    if let Some(intro) = intro_state {
                        if !intro.is_done() {
                            let intro_widget = NikaIntro::new(intro);
                            frame.render_widget(intro_widget, size);
                            return;
                        }
                    }

                    // Block without borders renders nothing - use Paragraph for proper fill
                    frame.render_widget(Clear, size);
                    let bg = Paragraph::new("").style(Style::default().bg(theme.background));
                    frame.render_widget(bg, size);

                    // Layout: Header (1) + Content (dynamic) + StatusBar (1)
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Length(1), // Header
                            Constraint::Min(0),    // Content
                            Constraint::Length(1), // StatusBar
                        ])
                        .split(size);

                    // Render header
                    let workflow_name = std::path::Path::new(workflow_path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("No workflow");
                    let header = Header::new(current_view, theme)
                        .context(workflow_name)
                        .status(if paused { "PAUSED" } else { "" });
                    frame.render_widget(header, chunks[0]);

                    // Render view content based on current view (4-view architecture)
                    match current_view {
                        TuiView::Studio => {
                            studio_view.render(frame, chunks[1], state, theme);
                        }
                        TuiView::Runner => {
                            monitor_view.render(frame, chunks[1], state, theme);
                        }
                        TuiView::Chat => {
                            chat_view.render(frame, chunks[1], state, theme);
                        }
                        TuiView::Settings => {
                            settings_view.render(frame, chunks[1], state, theme);
                        }
                    }

                    // Render status message if active (just above status bar)
                    // Skip when overlays are visible to prevent overlap
                    let overlay_visible = matches!(current_view, TuiView::Chat)
                        && (chat_view.provider_modal.visible
                            || chat_view.command_palette.visible
                            || chat_view.help_overlay.visible);

                    if !overlay_visible {
                        if let Some(msg) = state.status_messages.current() {
                            // Position status message at bottom of content area
                            let msg_area = Rect {
                                x: chunks[1].x,
                                y: chunks[1].bottom().saturating_sub(1),
                                width: chunks[1].width,
                                height: 1,
                            };
                            let status_widget = StatusMessageWidget::new(Some(msg));
                            frame.render_widget(status_widget, msg_area);
                        }
                    }

                    // Render status bar with metrics and custom status text
                    let metrics = StatusMetrics::new()
                        .provider(provider)
                        .tokens(total_tokens)
                        .mcp(mcp_connected, mcp_total)
                        .connection(if mcp_total > 0 {
                            ConnectionStatus::Connected
                        } else {
                            ConnectionStatus::Disconnected
                        });
                    let status_bar = StatusBar::new(current_view, theme)
                        .mode(input_mode)
                        .metrics(metrics)
                        .custom_text(status_text);
                    frame.render_widget(status_bar, chunks[2]);
                })
                .map_err(|e| NikaError::TuiError {
                    reason: format!("Failed to draw frame: {}", e),
                })?;
        }
        Ok(())
    }
}
