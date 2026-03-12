//! Task Progress Widget
//!
//! Background job progress tracking widget.
//! Shows task description, progress bar, status, and ETA.

use std::time::{Duration, Instant};

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, BorderType, Borders, Widget},
};

// =============================================================================
// SOLARIZED COLOR CONSTANTS
// =============================================================================

/// Border color (blue - solarized)
const COLOR_BORDER: Color = Color::Rgb(38, 139, 210); // Blue

/// Progress bar fill (green - solarized)
const COLOR_PROGRESS: Color = Color::Rgb(133, 153, 0); // Green

/// Progress bar background (base02 - solarized)
const COLOR_PROGRESS_BG: Color = Color::Rgb(7, 54, 66); // Base02

/// Pending color (yellow - solarized)
const COLOR_PENDING: Color = Color::Rgb(181, 137, 0); // Yellow

/// Running color (cyan - solarized)
const COLOR_RUNNING: Color = Color::Rgb(42, 161, 152); // Cyan

/// Complete color (green - solarized)
const COLOR_COMPLETE: Color = Color::Rgb(133, 153, 0); // Green

/// Paused color (violet - solarized)
const COLOR_PAUSED: Color = Color::Rgb(108, 113, 196); // Violet

/// Cancelled color (orange - solarized)
const COLOR_CANCELLED: Color = Color::Rgb(203, 75, 22); // Orange

/// Failed color (red - solarized)
const COLOR_FAILED: Color = Color::Rgb(220, 50, 47); // Red

/// Text color (base0 - solarized)
const COLOR_TEXT: Color = Color::Rgb(131, 148, 150); // Base0

/// Dim text color (base01 - solarized)
const COLOR_DIM: Color = Color::Rgb(88, 110, 117); // Base01

// =============================================================================
// TASK STATUS
// =============================================================================

/// Task progress status.
#[derive(Debug, Clone, PartialEq)]
pub enum TaskProgressStatus {
    /// Task is queued, waiting to start.
    Pending,
    /// Task is currently running.
    Running,
    /// Task completed successfully.
    Complete,
    /// Task is paused.
    Paused,
    /// Task was cancelled.
    Cancelled,
    /// Task failed with error message.
    Failed(String),
}

impl TaskProgressStatus {
    /// Get the display icon for this status.
    pub fn icon(&self) -> &'static str {
        match self {
            TaskProgressStatus::Pending => "○",
            TaskProgressStatus::Running => "●",
            TaskProgressStatus::Complete => "✓",
            TaskProgressStatus::Paused => "⏸",
            TaskProgressStatus::Cancelled => "⊘",
            TaskProgressStatus::Failed(_) => "✗",
        }
    }

    /// Get the status color.
    pub fn color(&self) -> Color {
        match self {
            TaskProgressStatus::Pending => COLOR_PENDING,
            TaskProgressStatus::Running => COLOR_RUNNING,
            TaskProgressStatus::Complete => COLOR_COMPLETE,
            TaskProgressStatus::Paused => COLOR_PAUSED,
            TaskProgressStatus::Cancelled => COLOR_CANCELLED,
            TaskProgressStatus::Failed(_) => COLOR_FAILED,
        }
    }

    /// Get the status label.
    pub fn label(&self) -> &str {
        match self {
            TaskProgressStatus::Pending => "Pending",
            TaskProgressStatus::Running => "Running",
            TaskProgressStatus::Complete => "Complete",
            TaskProgressStatus::Paused => "Paused",
            TaskProgressStatus::Cancelled => "Cancelled",
            TaskProgressStatus::Failed(_) => "Failed",
        }
    }

    /// Check if task is active (running or paused).
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            TaskProgressStatus::Running | TaskProgressStatus::Paused
        )
    }

    /// Check if task is terminal (complete, cancelled, or failed).
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TaskProgressStatus::Complete
                | TaskProgressStatus::Cancelled
                | TaskProgressStatus::Failed(_)
        )
    }
}

// =============================================================================
// TASK PROGRESS WIDGET
// =============================================================================

/// Background job progress widget.
///
/// Displays:
/// - Task ID and description
/// - Progress bar (0.0 - 1.0)
/// - Status icon and label
/// - Elapsed time
/// - ETA (estimated time remaining)
pub struct TaskProgress {
    /// Unique task identifier.
    pub task_id: String,
    /// Human-readable task description.
    pub description: String,
    /// Progress ratio (0.0 - 1.0).
    pub progress: f32,
    /// Current task status.
    pub status: TaskProgressStatus,
    /// When the task started.
    pub started_at: Instant,
    /// Estimated time remaining.
    pub eta: Option<Duration>,
}

impl TaskProgress {
    /// Create a new task progress widget.
    pub fn new(task_id: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            task_id: task_id.into(),
            description: description.into(),
            progress: 0.0,
            status: TaskProgressStatus::Pending,
            started_at: Instant::now(),
            eta: None,
        }
    }

    /// Set the progress ratio (0.0 - 1.0).
    pub fn progress(mut self, progress: f32) -> Self {
        self.progress = progress.clamp(0.0, 1.0);
        self
    }

    /// Set the task status.
    pub fn status(mut self, status: TaskProgressStatus) -> Self {
        self.status = status;
        self
    }

    /// Set the start time.
    pub fn started_at(mut self, started: Instant) -> Self {
        self.started_at = started;
        self
    }

    /// Set the ETA.
    pub fn eta(mut self, eta: Duration) -> Self {
        self.eta = Some(eta);
        self
    }

    /// Get elapsed time since start.
    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }

    /// Format duration for display (e.g., "2m 30s").
    pub fn format_duration(duration: Duration) -> String {
        let secs = duration.as_secs();
        if secs >= 3600 {
            let hours = secs / 3600;
            let mins = (secs % 3600) / 60;
            format!("{}h {}m", hours, mins)
        } else if secs >= 60 {
            let mins = secs / 60;
            let remaining = secs % 60;
            format!("{}m {}s", mins, remaining)
        } else {
            format!("{}s", secs)
        }
    }

    /// Render the progress bar portion.
    fn render_progress_bar(&self, area: Rect, buf: &mut Buffer) {
        if area.width < 5 {
            return;
        }

        let filled_width = ((area.width as f32) * self.progress).floor() as u16;
        let partial = (((area.width as f32) * self.progress).fract() * 8.0).floor() as usize;

        // Background
        let bg_style = Style::default().bg(COLOR_PROGRESS_BG);
        for x in area.x..(area.x + area.width) {
            buf.set_string(x, area.y, " ", bg_style);
        }

        // Filled portion
        let fill_style = Style::default().fg(COLOR_PROGRESS).bg(COLOR_PROGRESS);
        for x in area.x..(area.x + filled_width) {
            buf.set_string(x, area.y, "█", fill_style);
        }

        // Partial block
        if partial > 0 && filled_width < area.width {
            let partial_chars = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉"];
            let partial_char = partial_chars[partial.min(7)];
            buf.set_string(
                area.x + filled_width,
                area.y,
                partial_char,
                Style::default().fg(COLOR_PROGRESS).bg(COLOR_PROGRESS_BG),
            );
        }

        // Percentage in center
        let pct_text = format!("{:.0}%", self.progress * 100.0);
        if area.width > pct_text.len() as u16 + 2 {
            let text_x = area.x + (area.width.saturating_sub(pct_text.len() as u16)) / 2;
            buf.set_string(
                text_x,
                area.y,
                &pct_text,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            );
        }
    }
}

impl Widget for TaskProgress {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 || area.width < 30 {
            return;
        }

        // Build block with task ID and status
        let title = format!(
            " {} {} {} ",
            self.status.icon(),
            self.task_id,
            self.status.label()
        );

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(COLOR_BORDER))
            .title(title);

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height == 0 || inner.width < 10 {
            return;
        }

        // Line 1: Description
        let desc_truncated = if self.description.len() > inner.width as usize {
            format!("{}...", &self.description[..inner.width as usize - 3])
        } else {
            self.description.clone()
        };
        buf.set_string(
            inner.x,
            inner.y,
            &desc_truncated,
            Style::default().fg(COLOR_TEXT),
        );

        // Line 2: Progress bar
        if inner.height >= 2 {
            let bar_area = Rect {
                x: inner.x,
                y: inner.y + 1,
                width: inner.width,
                height: 1,
            };
            self.render_progress_bar(bar_area, buf);
        }

        // Line 3: Elapsed and ETA
        if inner.height >= 3 {
            let info_y = inner.y + 2;

            // Left: Elapsed
            let elapsed_str = format!("Elapsed: {}", Self::format_duration(self.elapsed()));
            buf.set_string(
                inner.x,
                info_y,
                &elapsed_str,
                Style::default().fg(COLOR_DIM),
            );

            // Right: ETA
            if let Some(eta) = self.eta {
                let eta_str = format!("ETA: {}", Self::format_duration(eta));
                let eta_x = inner.right().saturating_sub(eta_str.len() as u16);
                buf.set_string(eta_x, info_y, &eta_str, Style::default().fg(COLOR_DIM));
            }
        }

        // Line 4: Error message if failed
        if let TaskProgressStatus::Failed(ref msg) = self.status {
            if inner.height >= 4 {
                let error_y = inner.y + 3;
                let truncated = if msg.len() > inner.width as usize - 2 {
                    format!("{}...", &msg[..inner.width as usize - 5])
                } else {
                    msg.clone()
                };
                buf.set_string(
                    inner.x,
                    error_y,
                    &truncated,
                    Style::default()
                        .fg(COLOR_FAILED)
                        .add_modifier(Modifier::ITALIC),
                );
            }
        }
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_status_icon() {
        assert_eq!(TaskProgressStatus::Pending.icon(), "○");
        assert_eq!(TaskProgressStatus::Running.icon(), "●");
        assert_eq!(TaskProgressStatus::Complete.icon(), "✓");
        assert_eq!(TaskProgressStatus::Paused.icon(), "⏸");
        assert_eq!(TaskProgressStatus::Cancelled.icon(), "⊘");
        assert_eq!(TaskProgressStatus::Failed("err".to_string()).icon(), "✗");
    }

    #[test]
    fn test_task_status_label() {
        assert_eq!(TaskProgressStatus::Pending.label(), "Pending");
        assert_eq!(TaskProgressStatus::Running.label(), "Running");
        assert_eq!(TaskProgressStatus::Complete.label(), "Complete");
        assert_eq!(TaskProgressStatus::Paused.label(), "Paused");
        assert_eq!(TaskProgressStatus::Cancelled.label(), "Cancelled");
        assert_eq!(
            TaskProgressStatus::Failed("x".to_string()).label(),
            "Failed"
        );
    }

    #[test]
    fn test_task_status_is_active() {
        assert!(!TaskProgressStatus::Pending.is_active());
        assert!(TaskProgressStatus::Running.is_active());
        assert!(TaskProgressStatus::Paused.is_active());
        assert!(!TaskProgressStatus::Complete.is_active());
        assert!(!TaskProgressStatus::Cancelled.is_active());
        assert!(!TaskProgressStatus::Failed("err".to_string()).is_active());
    }

    #[test]
    fn test_task_status_is_terminal() {
        assert!(!TaskProgressStatus::Pending.is_terminal());
        assert!(!TaskProgressStatus::Running.is_terminal());
        assert!(!TaskProgressStatus::Paused.is_terminal());
        assert!(TaskProgressStatus::Complete.is_terminal());
        assert!(TaskProgressStatus::Cancelled.is_terminal());
        assert!(TaskProgressStatus::Failed("err".to_string()).is_terminal());
    }

    #[test]
    fn test_format_duration_hours() {
        assert_eq!(
            TaskProgress::format_duration(Duration::from_secs(3661)),
            "1h 1m"
        );
        assert_eq!(
            TaskProgress::format_duration(Duration::from_secs(7200)),
            "2h 0m"
        );
    }

    #[test]
    fn test_format_duration_minutes() {
        assert_eq!(
            TaskProgress::format_duration(Duration::from_secs(150)),
            "2m 30s"
        );
        assert_eq!(
            TaskProgress::format_duration(Duration::from_secs(60)),
            "1m 0s"
        );
    }

    #[test]
    fn test_format_duration_seconds() {
        assert_eq!(
            TaskProgress::format_duration(Duration::from_secs(45)),
            "45s"
        );
        assert_eq!(TaskProgress::format_duration(Duration::from_secs(0)), "0s");
    }

    #[test]
    fn test_task_progress_new() {
        let progress = TaskProgress::new("job-123", "Processing files");
        assert_eq!(progress.task_id, "job-123");
        assert_eq!(progress.description, "Processing files");
        assert!((progress.progress - 0.0).abs() < f32::EPSILON);
        assert_eq!(progress.status, TaskProgressStatus::Pending);
        assert!(progress.eta.is_none());
    }

    #[test]
    fn test_task_progress_builder() {
        let progress = TaskProgress::new("job-456", "Building project")
            .progress(0.75)
            .status(TaskProgressStatus::Running)
            .eta(Duration::from_secs(120));

        assert_eq!(progress.task_id, "job-456");
        assert!((progress.progress - 0.75).abs() < f32::EPSILON);
        assert_eq!(progress.status, TaskProgressStatus::Running);
        assert_eq!(progress.eta, Some(Duration::from_secs(120)));
    }

    #[test]
    fn test_progress_clamping() {
        let over = TaskProgress::new("test", "test").progress(1.5);
        assert!((over.progress - 1.0).abs() < f32::EPSILON);

        let under = TaskProgress::new("test", "test").progress(-0.5);
        assert!((under.progress - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_elapsed() {
        let progress = TaskProgress::new("test", "test");
        // Just verify it returns a duration without panicking
        let _elapsed = progress.elapsed();
    }

    #[test]
    fn test_widget_renders_without_panic() {
        let progress = TaskProgress::new("job-789", "Compiling sources")
            .progress(0.45)
            .status(TaskProgressStatus::Running)
            .eta(Duration::from_secs(60));

        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 6));
        progress.render(Rect::new(0, 0, 60, 6), &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("job-789"));
        assert!(content.contains("Compiling"));
    }

    #[test]
    fn test_widget_renders_failed_state() {
        let progress = TaskProgress::new("job-fail", "Bad task")
            .status(TaskProgressStatus::Failed("Out of memory".to_string()));

        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 6));
        progress.render(Rect::new(0, 0, 60, 6), &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("job-fail"));
        assert!(content.contains("Failed"));
    }

    #[test]
    fn test_widget_handles_small_area() {
        let progress = TaskProgress::new("test", "test");

        // Too small - should not panic
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 2));
        progress.render(Rect::new(0, 0, 10, 2), &mut buf);
    }

    #[test]
    fn test_status_color() {
        // Verify colors are different for different states
        assert_ne!(
            TaskProgressStatus::Pending.color(),
            TaskProgressStatus::Complete.color()
        );
        assert_ne!(
            TaskProgressStatus::Running.color(),
            TaskProgressStatus::Failed("err".to_string()).color()
        );
    }

    #[test]
    fn test_widget_renders_complete_state() {
        let progress = TaskProgress::new("job-done", "Finished task")
            .progress(1.0)
            .status(TaskProgressStatus::Complete);

        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 6));
        progress.render(Rect::new(0, 0, 60, 6), &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("Complete"));
    }

    #[test]
    fn test_widget_renders_paused_state() {
        let progress = TaskProgress::new("job-pause", "Paused task")
            .progress(0.5)
            .status(TaskProgressStatus::Paused);

        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 6));
        progress.render(Rect::new(0, 0, 60, 6), &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("Paused"));
    }

    #[test]
    fn test_widget_renders_cancelled_state() {
        let progress = TaskProgress::new("job-cancel", "Cancelled task")
            .progress(0.3)
            .status(TaskProgressStatus::Cancelled);

        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 6));
        progress.render(Rect::new(0, 0, 60, 6), &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("Cancelled"));
    }
}
