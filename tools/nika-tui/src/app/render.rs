// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! App Frame Rendering
//!
//! Contains frame rendering logic for the unified TUI architecture.
//! Studio, Command, Control (3-view architecture)

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use nika_engine::error::{NikaError, Result};

use super::super::views::{TuiView, View};
use super::super::widgets::{
    check_terminal_size, star_tick, ConnectionStatus, Header, StatusBar, StatusMessageWidget,
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
            if current_view == TuiView::Command {
                self.state.ensure_timeline_cache();
            }

            // Extract values BEFORE mutable borrows for the closure
            let total_tokens = self.command_view.chat.total_tokens();
            let provider = self.command_view.chat.provider();

            // PERF: Only compute status_line for active view (avoids 1 extra format!())
            let status_text = match current_view {
                TuiView::Studio => self.studio_view.status_line(&self.state),
                TuiView::Command => self.command_view.status_line(&self.state),
                TuiView::Control => self.control_view.status_line(&self.state),
            };

            let loading = self.loading;
            // Check if welcome hint is still active (auto-dismiss after deadline)
            let show_welcome = self
                .welcome_hint_until
                .map(|d| std::time::Instant::now() < d)
                .unwrap_or(false);
            if !show_welcome {
                self.welcome_hint_until = None; // Clear expired hint
            }
            let theme = &self.theme;
            let state = &self.state;
            let star_field = &mut self.star_field;
            let command_view = &mut self.command_view;
            let studio_view = &mut self.studio_view;
            let control_view = &mut self.control_view;
            let workflow_path = &self.state.workflow.path;
            // P0 Fix: Use is_paused() accessor for unified pause state
            let paused = self.state.is_paused();
            let input_mode = self.input_mode;

            // Extract data for StatusBar metrics from the centralized pool
            let mcp_total = self.mcp_pool.config_count();
            let mcp_connected = self.mcp_pool.connected_count();

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

                    // PERF: Removed Clear + set_style that wrote to ALL buffer cells
                    // every frame, defeating ratatui's diff-based rendering.
                    // Widgets set their own backgrounds; unchanged cells are not
                    // flushed to the terminal backend.

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

                    // Render view content based on current view (3-view architecture)
                    match current_view {
                        TuiView::Studio => {
                            studio_view.render(frame, chunks[1], state, theme);
                        }
                        TuiView::Command => {
                            command_view.render(frame, chunks[1], state, theme);
                        }
                        TuiView::Control => {
                            control_view.render(frame, chunks[1], state, theme);
                        }
                    }

                    // PERF: Pre-computed star field — O(500) instead of O(10K)
                    star_field.ensure_size(chunks[1].width, chunks[1].height);
                    star_field.render(frame, chunks[1], star_tick());

                    // First-launch welcome hint (auto-dismisses after 10s or first keypress)
                    // Skip when loading is active to prevent overlay overlap
                    if show_welcome && !loading && chunks[1].height > 0 {
                        let hint_text = Line::from(vec![
                            Span::styled(" Press ", Style::default().fg(theme.text_secondary)),
                            Span::styled(
                                "?",
                                Style::default()
                                    .fg(theme.highlight)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(" for help, ", Style::default().fg(theme.text_secondary)),
                            Span::styled(
                                "Ctrl+P",
                                Style::default()
                                    .fg(theme.highlight)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                " for providers, ",
                                Style::default().fg(theme.text_secondary),
                            ),
                            Span::styled(
                                "1/2/3",
                                Style::default()
                                    .fg(theme.highlight)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                " to switch views ",
                                Style::default().fg(theme.text_secondary),
                            ),
                        ]);
                        let hint_area = Rect {
                            x: chunks[1].x,
                            y: chunks[1].y,
                            width: chunks[1].width,
                            height: 1,
                        };
                        let hint_widget = Paragraph::new(hint_text)
                            .alignment(Alignment::Center)
                            .style(Style::default().bg(theme.selection));
                        frame.render_widget(hint_widget, hint_area);
                    }

                    // Show loading indicator during startup (before on_enter completes)
                    if loading {
                        let spinner = ['◐', '◓', '◑', '◒'];
                        let idx = (state.frame / 15) as usize % spinner.len();
                        let loading_text = Line::from(vec![Span::styled(
                            format!("{} Starting Nika...", spinner[idx]),
                            Style::default()
                                .fg(theme.highlight)
                                .add_modifier(Modifier::BOLD),
                        )]);
                        let loading_widget =
                            Paragraph::new(loading_text).alignment(Alignment::Center);
                        // Center vertically in content area
                        let y = chunks[1].y + chunks[1].height / 2;
                        let loading_area = Rect {
                            x: chunks[1].x,
                            y,
                            width: chunks[1].width,
                            height: 1,
                        };
                        frame.render_widget(loading_widget, loading_area);
                    }

                    // Render status message if active (just above status bar)
                    // Skip when overlays are visible to prevent overlap
                    let overlay_visible = matches!(current_view, TuiView::Command)
                        && (command_view.chat.provider_modal.visible
                            || command_view.chat.command_palette.visible
                            || command_view.chat.help_overlay.visible);

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
                        .cache_tokens(state.metrics.cache_read_tokens)
                        .mcp(mcp_connected, mcp_total)
                        .connection(if mcp_connected > 0 && mcp_connected == mcp_total {
                            ConnectionStatus::Connected
                        } else if mcp_total > 0 {
                            ConnectionStatus::Connecting
                        } else {
                            ConnectionStatus::Disconnected
                        });
                    let status_bar = StatusBar::new(current_view, theme)
                        .frame(state.frame)
                        .mode(input_mode)
                        .metrics(metrics)
                        .custom_text(status_text);
                    frame.render_widget(status_bar, chunks[2]);
                })
                .map_err(|e| NikaError::TuiError {
                    reason: format!("Failed to draw frame: {}", e),
                })?;

            // Clear dirty flags after successful render
            // This prepares for skip-rendering of unchanged panels
            self.state.clear_dirty();
        }
        Ok(())
    }
}
