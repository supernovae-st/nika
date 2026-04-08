// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Timeline Widget
//!
//! Displays task execution as a horizontal timeline with markers.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};

use crate::theme::{TaskStatus, Theme};

// ═══════════════════════════════════════════════════════════════════════════
// DEFAULT COLORS (fallbacks when no theme)
// ═══════════════════════════════════════════════════════════════════════════

const DEFAULT_PENDING_COLOR: Color = Color::Rgb(107, 114, 128); // gray
const DEFAULT_RUNNING_COLOR: Color = Color::Rgb(245, 158, 11); // amber
const DEFAULT_SUCCESS_COLOR: Color = Color::Rgb(34, 197, 94); // green
const DEFAULT_FAILED_COLOR: Color = Color::Rgb(239, 68, 68); // red
const DEFAULT_PAUSED_COLOR: Color = Color::Rgb(6, 182, 212); // cyan

/// Single entry in the timeline
#[derive(Debug, Clone)]
pub struct TimelineEntry {
    /// Task ID
    pub id: String,
    /// Task status
    pub status: TaskStatus,
    /// Duration in ms (if completed)
    pub duration_ms: Option<u64>,
    /// Is this the current task?
    pub is_current: bool,
    /// Has a breakpoint set (TIER 2.3)
    pub has_breakpoint: bool,
}

impl TimelineEntry {
    pub fn new(id: impl Into<String>, status: TaskStatus) -> Self {
        Self {
            id: id.into(),
            status,
            duration_ms: None,
            is_current: false,
            has_breakpoint: false,
        }
    }

    pub fn with_duration(mut self, ms: u64) -> Self {
        self.duration_ms = Some(ms);
        self
    }

    pub fn current(mut self) -> Self {
        self.is_current = true;
        self
    }

    /// Mark this entry as having a breakpoint (TIER 2.3)
    pub fn with_breakpoint(mut self, has_bp: bool) -> Self {
        self.has_breakpoint = has_bp;
        self
    }
}

/// Animated spinner frames for running tasks
const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Timeline widget showing task progress
pub struct Timeline<'a> {
    entries: &'a [TimelineEntry],
    elapsed_ms: u64,
    style: Style,
    /// Animation frame (for spinners)
    frame: u8,
    /// Optional theme for colors
    theme: Option<&'a Theme>,
}

impl<'a> Timeline<'a> {
    pub fn new(entries: &'a [TimelineEntry]) -> Self {
        Self {
            entries,
            elapsed_ms: 0,
            style: Style::default(),
            frame: 0,
            theme: None,
        }
    }

    /// Set the theme for colors
    pub fn with_theme(mut self, theme: &'a Theme) -> Self {
        self.theme = Some(theme);
        self
    }

    pub fn elapsed(mut self, ms: u64) -> Self {
        self.elapsed_ms = ms;
        self
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Set animation frame for spinners
    pub fn with_frame(mut self, frame: u8) -> Self {
        self.frame = frame;
        self
    }

    /// Get current spinner character
    fn spinner(&self) -> &'static str {
        let idx = (self.frame / 6) as usize % SPINNER_FRAMES.len();
        SPINNER_FRAMES[idx]
    }

    /// Get status color with theme fallback
    fn status_color(&self, status: TaskStatus) -> Color {
        match status {
            TaskStatus::Queued => self
                .theme
                .map(|t| t.text_muted)
                .unwrap_or(DEFAULT_PENDING_COLOR),
            TaskStatus::Pending => self
                .theme
                .map(|t| t.status_pending)
                .unwrap_or(DEFAULT_PENDING_COLOR),
            TaskStatus::Running => self
                .theme
                .map(|t| t.status_running)
                .unwrap_or(DEFAULT_RUNNING_COLOR),
            TaskStatus::Success => self
                .theme
                .map(|t| t.status_success)
                .unwrap_or(DEFAULT_SUCCESS_COLOR),
            TaskStatus::Failed => self
                .theme
                .map(|t| t.status_failed)
                .unwrap_or(DEFAULT_FAILED_COLOR),
            TaskStatus::Paused => self
                .theme
                .map(|t| t.status_paused)
                .unwrap_or(DEFAULT_PAUSED_COLOR),
            TaskStatus::Skipped => self
                .theme
                .map(|t| t.text_muted)
                .unwrap_or(DEFAULT_PENDING_COLOR),
        }
    }

    /// Get status icon (static version for non-running)
    fn status_icon_static(status: TaskStatus, is_current: bool) -> &'static str {
        if is_current && status != TaskStatus::Running {
            return "◉";
        }
        match status {
            TaskStatus::Queued => "○",
            TaskStatus::Pending => "◦",
            TaskStatus::Running => "◉", // Will be replaced by spinner
            TaskStatus::Success => "●",
            TaskStatus::Failed => "⊗",
            TaskStatus::Paused => "◎",
            TaskStatus::Skipped => "⊘",
        }
    }

    /// Get status icon (animated for running tasks)
    fn status_icon(&self, status: TaskStatus, is_current: bool) -> &str {
        if status == TaskStatus::Running {
            return self.spinner();
        }
        Self::status_icon_static(status, is_current)
    }
}

impl Widget for Timeline<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 || area.width < 10 || self.entries.is_empty() {
            return;
        }

        // Extract theme colors with fallbacks
        let muted_color = self.theme.map(|t| t.text_muted).unwrap_or(Color::DarkGray);
        let highlight_color = self.theme.map(|t| t.highlight).unwrap_or(Color::Cyan);

        // Calculate layout
        let num_entries = self.entries.len();
        let available_width = area.width.saturating_sub(2) as usize;
        let entry_width = (available_width / num_entries.max(1)).max(3);

        // Draw timeline track (row 1)
        let track_y = area.y + 1;
        let track_char = "─";
        for x in area.x..(area.x + area.width) {
            buf.set_string(x, track_y, track_char, Style::default().fg(muted_color));
        }

        // Draw entries
        for (i, entry) in self.entries.iter().enumerate() {
            let x = area.x + (i * entry_width) as u16 + 1;
            if x >= area.x + area.width {
                break;
            }

            let color = self.status_color(entry.status);
            let icon = self.status_icon(entry.status, entry.is_current);

            // Draw breakpoint indicator above track (TIER 2.3)
            if entry.has_breakpoint && area.y > 0 {
                buf.set_string(
                    x,
                    area.y,
                    "🔴",
                    Style::default().fg(self.status_color(TaskStatus::Failed)),
                );
            }

            // Draw marker on track
            buf.set_string(x, track_y, icon, Style::default().fg(color));

            // Draw task ID below (truncated if needed, UTF-8 safe)
            if area.height > 2 {
                let label_y = track_y + 1;
                let max_len = entry_width.saturating_sub(1);
                let label: String = if entry.id.chars().count() > max_len {
                    entry.id.chars().take(max_len).collect()
                } else {
                    entry.id.clone()
                };
                buf.set_string(x, label_y, &label, Style::default().fg(color));
            }
        }

        // Draw elapsed time at the end
        if area.height > 0 {
            let elapsed_str = format_duration(self.elapsed_ms);
            let elapsed_x = area.x + area.width.saturating_sub(elapsed_str.len() as u16 + 1);
            buf.set_string(
                elapsed_x,
                area.y,
                &elapsed_str,
                Style::default().fg(highlight_color),
            );
        }
    }
}

/// Format duration as MM:SS or HH:MM:SS
fn format_duration(ms: u64) -> String {
    let total_secs = ms / 1000;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    if hours > 0 {
        format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        format!("{:02}:{:02}", minutes, seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeline_entry_creation() {
        let entry = TimelineEntry::new("task1", TaskStatus::Running)
            .with_duration(500)
            .current();

        assert_eq!(entry.id, "task1");
        assert_eq!(entry.status, TaskStatus::Running);
        assert_eq!(entry.duration_ms, Some(500));
        assert!(entry.is_current);
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(0), "00:00");
        assert_eq!(format_duration(5000), "00:05");
        assert_eq!(format_duration(65000), "01:05");
        assert_eq!(format_duration(3661000), "01:01:01");
    }

    #[test]
    fn test_status_colors() {
        // Create a timeline instance to test status_color
        let entries: &[TimelineEntry] = &[];
        let timeline = Timeline::new(entries);

        // Verify colors are assigned and different for different statuses
        assert_ne!(
            timeline.status_color(TaskStatus::Running),
            timeline.status_color(TaskStatus::Success)
        );
    }

    #[test]
    fn test_timeline_entry_breakpoint() {
        // Default: no breakpoint
        let entry = TimelineEntry::new("task1", TaskStatus::Pending);
        assert!(!entry.has_breakpoint);

        // With breakpoint
        let entry = TimelineEntry::new("task1", TaskStatus::Pending).with_breakpoint(true);
        assert!(entry.has_breakpoint);

        // With breakpoint false
        let entry = TimelineEntry::new("task1", TaskStatus::Pending).with_breakpoint(false);
        assert!(!entry.has_breakpoint);
    }
}
