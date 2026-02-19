//! Timeline Widget
//!
//! Displays task execution as a horizontal timeline with markers.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};

use crate::tui::theme::TaskStatus;

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
}

impl TimelineEntry {
    pub fn new(id: impl Into<String>, status: TaskStatus) -> Self {
        Self {
            id: id.into(),
            status,
            duration_ms: None,
            is_current: false,
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
}

impl<'a> Timeline<'a> {
    pub fn new(entries: &'a [TimelineEntry]) -> Self {
        Self {
            entries,
            elapsed_ms: 0,
            style: Style::default(),
            frame: 0,
        }
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

    /// Get status color
    fn status_color(status: TaskStatus) -> Color {
        match status {
            TaskStatus::Pending => Color::Rgb(107, 114, 128), // gray
            TaskStatus::Running => Color::Rgb(245, 158, 11),  // amber
            TaskStatus::Success => Color::Rgb(34, 197, 94),   // green
            TaskStatus::Failed => Color::Rgb(239, 68, 68),    // red
            TaskStatus::Paused => Color::Rgb(6, 182, 212),    // cyan
        }
    }

    /// Get status icon (static version for non-running)
    fn status_icon_static(status: TaskStatus, is_current: bool) -> &'static str {
        if is_current && status != TaskStatus::Running {
            return "◉";
        }
        match status {
            TaskStatus::Pending => "○",
            TaskStatus::Running => "◉", // Will be replaced by spinner
            TaskStatus::Success => "●",
            TaskStatus::Failed => "⊗",
            TaskStatus::Paused => "◎",
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

        // Calculate layout
        let num_entries = self.entries.len();
        let available_width = area.width.saturating_sub(2) as usize;
        let entry_width = (available_width / num_entries.max(1)).max(3);

        // Draw timeline track (row 1)
        let track_y = area.y + 1;
        let track_char = "─";
        for x in area.x..(area.x + area.width) {
            buf.set_string(x, track_y, track_char, Style::default().fg(Color::DarkGray));
        }

        // Draw entries
        for (i, entry) in self.entries.iter().enumerate() {
            let x = area.x + (i * entry_width) as u16 + 1;
            if x >= area.x + area.width {
                break;
            }

            let color = Self::status_color(entry.status);
            let icon = self.status_icon(entry.status, entry.is_current);

            // Draw marker on track
            buf.set_string(x, track_y, icon, Style::default().fg(color));

            // Draw task ID below (truncated if needed)
            if area.height > 2 {
                let label_y = track_y + 1;
                let max_len = entry_width.saturating_sub(1);
                let label = if entry.id.len() > max_len {
                    &entry.id[..max_len]
                } else {
                    &entry.id
                };
                buf.set_string(x, label_y, label, Style::default().fg(color));
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
                Style::default().fg(Color::Cyan),
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
        // Verify colors are assigned
        assert_ne!(
            Timeline::status_color(TaskStatus::Running),
            Timeline::status_color(TaskStatus::Success)
        );
    }
}
