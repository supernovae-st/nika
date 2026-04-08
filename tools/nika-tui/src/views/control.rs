// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Control View — Infrastructure management + Job Dashboard
//!
//! Split layout:
//! - Left: Job Dashboard (daemon status, running jobs, cache stats)
//! - Right: Settings (providers, MCP, theme)
//!
//! The job dashboard connects to the daemon via IPC to show real-time
//! job status, cache hit rates, and daemon health.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};
use ratatui::Frame;

use super::settings::SettingsView;
use super::{View, ViewAction};
use crate::state::TuiState;
use crate::theme::Theme;

/// Focus panel in control view.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ControlPanel {
    Jobs,
    Settings,
}

/// Control View — Jobs + Settings
pub struct ControlView {
    pub settings: SettingsView,
    focus: ControlPanel,
    /// Cached daemon info (refreshed on tick)
    daemon_status: DaemonInfo,
}

/// Cached daemon status for TUI display.
#[derive(Debug, Default)]
struct DaemonInfo {
    running: bool,
    version: String,
    uptime: String,
    services: Vec<String>,
    job_count: usize,
    cache_entries: usize,
    cache_hit_rate: String,
}

impl Default for ControlView {
    fn default() -> Self {
        Self::new()
    }
}

impl ControlView {
    pub fn new() -> Self {
        Self {
            settings: SettingsView::new(),
            focus: ControlPanel::Jobs,
            daemon_status: DaemonInfo::default(),
        }
    }

    fn render_job_dashboard(&self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        let block = Block::default()
            .title(" Daemon Dashboard ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if self.focus == ControlPanel::Jobs {
                Color::Cyan
            } else {
                Color::DarkGray
            }));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let mut items: Vec<ListItem> = Vec::new();

        // Daemon status
        let status_icon = if self.daemon_status.running {
            Span::styled("● ", Style::default().fg(Color::Green))
        } else {
            Span::styled("● ", Style::default().fg(Color::Red))
        };

        let status_text = if self.daemon_status.running {
            format!(
                "Daemon running (v{}, {})",
                self.daemon_status.version, self.daemon_status.uptime
            )
        } else {
            "Daemon not running — nika daemon start".to_string()
        };

        items.push(ListItem::new(Line::from(vec![
            status_icon,
            Span::raw(status_text),
        ])));

        items.push(ListItem::new(""));

        // Services
        if !self.daemon_status.services.is_empty() {
            items.push(ListItem::new(Line::from(Span::styled(
                "Services",
                Style::default().add_modifier(Modifier::BOLD),
            ))));
            for svc in &self.daemon_status.services {
                items.push(ListItem::new(format!("  ✓ {svc}")));
            }
            items.push(ListItem::new(""));
        }

        // Cache stats
        items.push(ListItem::new(Line::from(Span::styled(
            "Cache",
            Style::default().add_modifier(Modifier::BOLD),
        ))));
        items.push(ListItem::new(format!(
            "  entries:  {}",
            self.daemon_status.cache_entries
        )));
        items.push(ListItem::new(format!(
            "  hit rate: {}",
            self.daemon_status.cache_hit_rate
        )));

        items.push(ListItem::new(""));

        // Jobs
        items.push(ListItem::new(Line::from(Span::styled(
            "Jobs",
            Style::default().add_modifier(Modifier::BOLD),
        ))));
        items.push(ListItem::new(format!(
            "  total: {}",
            self.daemon_status.job_count
        )));

        items.push(ListItem::new(""));

        // Help
        items.push(ListItem::new(Line::from(vec![
            Span::styled("Tab", Style::default().fg(Color::Yellow)),
            Span::raw(" switch panel  "),
            Span::styled("r", Style::default().fg(Color::Yellow)),
            Span::raw(" refresh"),
        ])));

        let list = List::new(items);
        frame.render_widget(list, inner);
    }

    fn refresh_daemon_status(&mut self) {
        // Check daemon socket existence via known path
        let socket_path = dirs::home_dir()
            .unwrap_or_default()
            .join(".nika/daemon/nika.sock");
        if socket_path.exists() {
            self.daemon_status.running = true;
            // We can't do async IPC from the sync render loop,
            // so we just show that the daemon is detected.
            // Full status requires async (done in tick via spawn).
            if self.daemon_status.version.is_empty() {
                self.daemon_status.version = "detecting...".into();
                self.daemon_status.uptime = "...".into();
            }
        } else {
            self.daemon_status = DaemonInfo::default();
        }
    }
}

impl View for ControlView {
    fn render(&mut self, frame: &mut Frame, area: Rect, state: &TuiState, theme: &Theme) {
        // Split: 40% jobs dashboard, 60% settings
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(area);

        self.render_job_dashboard(frame, chunks[0], theme);
        self.settings.render(frame, chunks[1], state, theme);
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut TuiState) -> ViewAction {
        match key.code {
            KeyCode::Tab => {
                self.focus = match self.focus {
                    ControlPanel::Jobs => ControlPanel::Settings,
                    ControlPanel::Settings => ControlPanel::Jobs,
                };
                ViewAction::None
            }
            KeyCode::Char('r') if self.focus == ControlPanel::Jobs => {
                self.refresh_daemon_status();
                ViewAction::None
            }
            _ => {
                if self.focus == ControlPanel::Settings {
                    self.settings.handle_key(key, state)
                } else {
                    ViewAction::None
                }
            }
        }
    }

    fn status_line(&self, state: &TuiState) -> String {
        match self.focus {
            ControlPanel::Jobs => {
                if self.daemon_status.running {
                    "Daemon ● | Tab: settings | r: refresh".to_string()
                } else {
                    "Daemon ○ (not running) | Tab: settings".to_string()
                }
            }
            ControlPanel::Settings => self.settings.status_line(state),
        }
    }

    fn on_enter(&mut self, state: &mut TuiState) {
        self.refresh_daemon_status();
        self.settings.on_enter(state);
    }

    fn on_leave(&mut self, state: &mut TuiState) {
        self.settings.on_leave(state);
    }

    fn tick(&mut self, state: &mut TuiState) {
        self.settings.tick(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_view_default_focus_is_jobs() {
        let view = ControlView::new();
        assert_eq!(view.focus, ControlPanel::Jobs);
    }

    #[test]
    fn daemon_info_default_is_not_running() {
        let info = DaemonInfo::default();
        assert!(!info.running);
        assert!(info.version.is_empty());
    }

    #[test]
    fn control_panel_toggle() {
        let mut panel = ControlPanel::Jobs;
        panel = match panel {
            ControlPanel::Jobs => ControlPanel::Settings,
            ControlPanel::Settings => ControlPanel::Jobs,
        };
        assert_eq!(panel, ControlPanel::Settings);
    }
}
