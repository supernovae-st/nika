//! Download Progress Widget
//!
//! Multi-file download progress bar with speed and ETA display.
//! Used for model downloads and package installations.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, BorderType, Borders, Gauge, Widget},
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

/// Verifying color (cyan - solarized)
const COLOR_VERIFYING: Color = Color::Rgb(42, 161, 152); // Cyan

/// Complete color (green - solarized)
const COLOR_COMPLETE: Color = Color::Rgb(133, 153, 0); // Green

/// Failed color (red - solarized)
const COLOR_FAILED: Color = Color::Rgb(220, 50, 47); // Red

/// Text color (base0 - solarized)
const COLOR_TEXT: Color = Color::Rgb(131, 148, 150); // Base0

/// Dim text color (base01 - solarized)
const COLOR_DIM: Color = Color::Rgb(88, 110, 117); // Base01

// =============================================================================
// DOWNLOAD STATUS
// =============================================================================

/// Download status for a single file.
#[derive(Debug, Clone, PartialEq)]
pub enum DownloadStatus {
    /// Waiting to start download.
    Pending,
    /// Currently downloading.
    Downloading,
    /// Verifying file integrity (checksum).
    Verifying,
    /// Download completed successfully.
    Complete,
    /// Download failed with error message.
    Failed(String),
}

impl DownloadStatus {
    /// Get the display icon for this status.
    pub fn icon(&self) -> &'static str {
        match self {
            DownloadStatus::Pending => "○",
            DownloadStatus::Downloading => "◐",
            DownloadStatus::Verifying => "◑",
            DownloadStatus::Complete => "●",
            DownloadStatus::Failed(_) => "✗",
        }
    }

    /// Get the status color.
    pub fn color(&self) -> Color {
        match self {
            DownloadStatus::Pending => COLOR_PENDING,
            DownloadStatus::Downloading => COLOR_PROGRESS,
            DownloadStatus::Verifying => COLOR_VERIFYING,
            DownloadStatus::Complete => COLOR_COMPLETE,
            DownloadStatus::Failed(_) => COLOR_FAILED,
        }
    }

    /// Get the status label.
    pub fn label(&self) -> &str {
        match self {
            DownloadStatus::Pending => "Pending",
            DownloadStatus::Downloading => "Downloading",
            DownloadStatus::Verifying => "Verifying",
            DownloadStatus::Complete => "Complete",
            DownloadStatus::Failed(_) => "Failed",
        }
    }

    /// Check if download is in progress.
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            DownloadStatus::Downloading | DownloadStatus::Verifying
        )
    }

    /// Check if download is terminal (complete or failed).
    pub fn is_terminal(&self) -> bool {
        matches!(self, DownloadStatus::Complete | DownloadStatus::Failed(_))
    }
}

// =============================================================================
// DOWNLOAD PROGRESS WIDGET
// =============================================================================

/// Multi-file download progress widget.
///
/// Displays:
/// - File name with status icon
/// - Progress bar (filled based on bytes downloaded)
/// - Downloaded/total bytes
/// - Download speed (MB/s)
/// - ETA (estimated time remaining)
pub struct DownloadProgress {
    /// File name being downloaded.
    pub file_name: String,
    /// Bytes downloaded so far.
    pub bytes_downloaded: u64,
    /// Total bytes to download (None if unknown).
    pub total_bytes: Option<u64>,
    /// Current download speed in bytes per second.
    pub speed_bps: f64,
    /// Estimated time remaining in seconds.
    pub eta_seconds: Option<u64>,
    /// Current download status.
    pub status: DownloadStatus,
}

impl DownloadProgress {
    /// Create a new download progress widget.
    pub fn new(file_name: impl Into<String>) -> Self {
        Self {
            file_name: file_name.into(),
            bytes_downloaded: 0,
            total_bytes: None,
            speed_bps: 0.0,
            eta_seconds: None,
            status: DownloadStatus::Pending,
        }
    }

    /// Set the bytes downloaded.
    pub fn downloaded(mut self, bytes: u64) -> Self {
        self.bytes_downloaded = bytes;
        self
    }

    /// Set the total bytes.
    pub fn total(mut self, bytes: u64) -> Self {
        self.total_bytes = Some(bytes);
        self
    }

    /// Set the download speed in bytes per second.
    pub fn speed(mut self, bps: f64) -> Self {
        self.speed_bps = bps;
        self
    }

    /// Set the ETA in seconds.
    pub fn eta(mut self, seconds: u64) -> Self {
        self.eta_seconds = Some(seconds);
        self
    }

    /// Set the download status.
    pub fn status(mut self, status: DownloadStatus) -> Self {
        self.status = status;
        self
    }

    /// Calculate progress ratio (0.0 to 1.0).
    pub fn progress_ratio(&self) -> f64 {
        match self.total_bytes {
            Some(total) if total > 0 => {
                (self.bytes_downloaded as f64 / total as f64).clamp(0.0, 1.0)
            }
            _ => 0.0,
        }
    }

    /// Format bytes for display (e.g., "1.5 GB").
    pub fn format_bytes(bytes: u64) -> String {
        if bytes >= 1_000_000_000 {
            format!("{:.1} GB", bytes as f64 / 1_000_000_000.0)
        } else if bytes >= 1_000_000 {
            format!("{:.1} MB", bytes as f64 / 1_000_000.0)
        } else if bytes >= 1_000 {
            format!("{:.1} KB", bytes as f64 / 1_000.0)
        } else {
            format!("{} B", bytes)
        }
    }

    /// Format speed for display (e.g., "50.0 MB/s").
    pub fn format_speed(bps: f64) -> String {
        if bps >= 1_000_000.0 {
            format!("{:.1} MB/s", bps / 1_000_000.0)
        } else if bps >= 1_000.0 {
            format!("{:.1} KB/s", bps / 1_000.0)
        } else {
            format!("{:.0} B/s", bps)
        }
    }

    /// Format ETA for display (e.g., "2m 30s").
    pub fn format_eta(seconds: u64) -> String {
        if seconds >= 3600 {
            let hours = seconds / 3600;
            let mins = (seconds % 3600) / 60;
            format!("{}h {}m", hours, mins)
        } else if seconds >= 60 {
            let mins = seconds / 60;
            let secs = seconds % 60;
            format!("{}m {}s", mins, secs)
        } else {
            format!("{}s", seconds)
        }
    }
}

impl Widget for DownloadProgress {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 || area.width < 30 {
            return;
        }

        // Build block with file name and status
        let title = format!(
            " {} {} {} ",
            self.status.icon(),
            self.file_name,
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

        // Render progress bar on first line
        let gauge_area = Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: 1,
        };

        let percentage = (self.progress_ratio() * 100.0) as u16;
        let gauge_color = self.status.color();

        let gauge = Gauge::default()
            .percent(percentage)
            .gauge_style(Style::default().fg(gauge_color).bg(COLOR_PROGRESS_BG))
            .use_unicode(true);

        gauge.render(gauge_area, buf);

        // Render info line if we have space
        if inner.height >= 2 {
            let info_y = inner.y + 1;

            // Left side: bytes downloaded / total
            let bytes_info = match self.total_bytes {
                Some(total) => format!(
                    "{} / {}",
                    Self::format_bytes(self.bytes_downloaded),
                    Self::format_bytes(total)
                ),
                None => Self::format_bytes(self.bytes_downloaded),
            };
            buf.set_string(
                inner.x,
                info_y,
                &bytes_info,
                Style::default().fg(COLOR_TEXT),
            );

            // Right side: speed and ETA
            let mut right_parts = Vec::new();

            if self.speed_bps > 0.0 {
                right_parts.push(Self::format_speed(self.speed_bps));
            }

            if let Some(eta) = self.eta_seconds {
                right_parts.push(format!("ETA: {}", Self::format_eta(eta)));
            }

            if !right_parts.is_empty() {
                let right_info = right_parts.join(" | ");
                let right_x = inner.right().saturating_sub(right_info.len() as u16);
                buf.set_string(right_x, info_y, &right_info, Style::default().fg(COLOR_DIM));
            }
        }

        // Render error message if failed
        if let DownloadStatus::Failed(ref msg) = self.status {
            if inner.height >= 3 {
                let error_y = inner.y + 2;
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
    fn test_download_status_icon() {
        assert_eq!(DownloadStatus::Pending.icon(), "○");
        assert_eq!(DownloadStatus::Downloading.icon(), "◐");
        assert_eq!(DownloadStatus::Verifying.icon(), "◑");
        assert_eq!(DownloadStatus::Complete.icon(), "●");
        assert_eq!(DownloadStatus::Failed("err".to_string()).icon(), "✗");
    }

    #[test]
    fn test_download_status_is_active() {
        assert!(!DownloadStatus::Pending.is_active());
        assert!(DownloadStatus::Downloading.is_active());
        assert!(DownloadStatus::Verifying.is_active());
        assert!(!DownloadStatus::Complete.is_active());
        assert!(!DownloadStatus::Failed("err".to_string()).is_active());
    }

    #[test]
    fn test_download_status_is_terminal() {
        assert!(!DownloadStatus::Pending.is_terminal());
        assert!(!DownloadStatus::Downloading.is_terminal());
        assert!(!DownloadStatus::Verifying.is_terminal());
        assert!(DownloadStatus::Complete.is_terminal());
        assert!(DownloadStatus::Failed("err".to_string()).is_terminal());
    }

    #[test]
    fn test_format_bytes_gb() {
        assert_eq!(DownloadProgress::format_bytes(4_700_000_000), "4.7 GB");
        assert_eq!(DownloadProgress::format_bytes(1_000_000_000), "1.0 GB");
    }

    #[test]
    fn test_format_bytes_mb() {
        assert_eq!(DownloadProgress::format_bytes(500_000_000), "500.0 MB");
        assert_eq!(DownloadProgress::format_bytes(1_500_000), "1.5 MB");
    }

    #[test]
    fn test_format_bytes_kb() {
        assert_eq!(DownloadProgress::format_bytes(500_000), "500.0 KB");
        assert_eq!(DownloadProgress::format_bytes(1_500), "1.5 KB");
    }

    #[test]
    fn test_format_bytes_b() {
        assert_eq!(DownloadProgress::format_bytes(500), "500 B");
        assert_eq!(DownloadProgress::format_bytes(0), "0 B");
    }

    #[test]
    fn test_format_speed() {
        assert_eq!(DownloadProgress::format_speed(50_000_000.0), "50.0 MB/s");
        assert_eq!(DownloadProgress::format_speed(500_000.0), "500.0 KB/s");
        assert_eq!(DownloadProgress::format_speed(500.0), "500 B/s");
    }

    #[test]
    fn test_format_eta_hours() {
        assert_eq!(DownloadProgress::format_eta(3661), "1h 1m");
        assert_eq!(DownloadProgress::format_eta(7200), "2h 0m");
    }

    #[test]
    fn test_format_eta_minutes() {
        assert_eq!(DownloadProgress::format_eta(150), "2m 30s");
        assert_eq!(DownloadProgress::format_eta(60), "1m 0s");
    }

    #[test]
    fn test_format_eta_seconds() {
        assert_eq!(DownloadProgress::format_eta(45), "45s");
        assert_eq!(DownloadProgress::format_eta(0), "0s");
    }

    #[test]
    fn test_progress_ratio() {
        let progress = DownloadProgress::new("test.bin")
            .downloaded(500)
            .total(1000);
        assert!((progress.progress_ratio() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_progress_ratio_no_total() {
        let progress = DownloadProgress::new("test.bin").downloaded(500);
        assert!((progress.progress_ratio() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_progress_ratio_zero_total() {
        let progress = DownloadProgress::new("test.bin").downloaded(500).total(0);
        assert!((progress.progress_ratio() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_progress_ratio_over_100() {
        let progress = DownloadProgress::new("test.bin")
            .downloaded(1500)
            .total(1000);
        assert!((progress.progress_ratio() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_builder_pattern() {
        let progress = DownloadProgress::new("model.gguf")
            .downloaded(1_000_000)
            .total(10_000_000)
            .speed(500_000.0)
            .eta(20)
            .status(DownloadStatus::Downloading);

        assert_eq!(progress.file_name, "model.gguf");
        assert_eq!(progress.bytes_downloaded, 1_000_000);
        assert_eq!(progress.total_bytes, Some(10_000_000));
        assert_eq!(progress.speed_bps, 500_000.0);
        assert_eq!(progress.eta_seconds, Some(20));
        assert_eq!(progress.status, DownloadStatus::Downloading);
    }

    #[test]
    fn test_widget_renders_without_panic() {
        let progress = DownloadProgress::new("test.bin")
            .downloaded(500_000_000)
            .total(1_000_000_000)
            .speed(50_000_000.0)
            .eta(10)
            .status(DownloadStatus::Downloading);

        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 5));
        progress.render(Rect::new(0, 0, 60, 5), &mut buf);

        // Check that content was rendered
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("test.bin"));
    }

    #[test]
    fn test_widget_renders_failed_state() {
        let progress = DownloadProgress::new("broken.bin")
            .status(DownloadStatus::Failed("Connection timeout".to_string()));

        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 5));
        progress.render(Rect::new(0, 0, 60, 5), &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("broken.bin"));
        assert!(content.contains("Failed"));
    }

    #[test]
    fn test_widget_handles_small_area() {
        let progress = DownloadProgress::new("test.bin");

        // Too small - should not panic
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 2));
        progress.render(Rect::new(0, 0, 10, 2), &mut buf);
    }

    #[test]
    fn test_status_color() {
        // Just verify colors are different for different states
        assert_ne!(
            DownloadStatus::Pending.color(),
            DownloadStatus::Complete.color()
        );
        assert_ne!(
            DownloadStatus::Downloading.color(),
            DownloadStatus::Failed("err".to_string()).color()
        );
    }

    #[test]
    fn test_status_label() {
        assert_eq!(DownloadStatus::Pending.label(), "Pending");
        assert_eq!(DownloadStatus::Downloading.label(), "Downloading");
        assert_eq!(DownloadStatus::Verifying.label(), "Verifying");
        assert_eq!(DownloadStatus::Complete.label(), "Complete");
        assert_eq!(DownloadStatus::Failed("x".to_string()).label(), "Failed");
    }
}
