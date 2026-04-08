// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Download progress gauge for model pulls
//!

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, BorderType, Borders, Gauge, Widget},
};

// =============================================================================
// SEMANTIC COLOR CONSTANTS
// =============================================================================

/// Border color (blue)
const COLOR_BORDER: Color = Color::Rgb(59, 130, 246); // Blue-500

/// Progress bar (green)
const COLOR_PROGRESS: Color = Color::Rgb(34, 197, 94); // Emerald-500

/// Progress bar background
const COLOR_PROGRESS_BG: Color = Color::Rgb(31, 41, 55); // Gray-800

/// Size info text
const COLOR_SIZE_INFO: Color = Color::Rgb(156, 163, 175); // Gray-400

/// Speed info text
const COLOR_SPEED_INFO: Color = Color::Rgb(107, 114, 128); // Gray-500

/// Download progress gauge with model name and size
pub struct DownloadGauge<'a> {
    model_name: &'a str,
    pub progress: f64,
    downloaded_bytes: u64,
    total_bytes: u64,
    speed_bps: Option<u64>,
}

impl<'a> DownloadGauge<'a> {
    pub fn new(model_name: &'a str, progress: f64, downloaded: u64, total: u64) -> Self {
        Self {
            model_name,
            progress: progress.clamp(0.0, 1.0),
            downloaded_bytes: downloaded,
            total_bytes: total,
            speed_bps: None,
        }
    }

    pub fn speed(mut self, bps: u64) -> Self {
        self.speed_bps = Some(bps);
        self
    }

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

    pub fn format_speed(bps: u64) -> String {
        if bps >= 1_000_000 {
            format!("{:.1} MB/s", bps as f64 / 1_000_000.0)
        } else if bps >= 1_000 {
            format!("{:.1} KB/s", bps as f64 / 1_000.0)
        } else {
            format!("{} B/s", bps)
        }
    }
}

impl Widget for DownloadGauge<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 || area.width < 30 {
            return;
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(COLOR_BORDER))
            .title(format!(" Pulling {} ", self.model_name));

        let inner = block.inner(area);
        block.render(area, buf);

        // Progress bar
        let gauge_area = Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: 1,
        };

        let percentage = (self.progress * 100.0) as u16;
        let gauge = Gauge::default()
            .percent(percentage)
            .gauge_style(Style::default().fg(COLOR_PROGRESS).bg(COLOR_PROGRESS_BG))
            .use_unicode(true);

        gauge.render(gauge_area, buf);

        // Size info below
        if inner.height >= 2 {
            let size_info = format!(
                "{} / {}",
                Self::format_bytes(self.downloaded_bytes),
                Self::format_bytes(self.total_bytes),
            );
            buf.set_string(
                inner.x,
                inner.y + 1,
                &size_info,
                Style::default().fg(COLOR_SIZE_INFO),
            );

            if let Some(speed) = self.speed_bps {
                let speed_str = Self::format_speed(speed);
                let speed_x = inner.right().saturating_sub(speed_str.len() as u16);
                buf.set_string(
                    speed_x,
                    inner.y + 1,
                    &speed_str,
                    Style::default().fg(COLOR_SPEED_INFO),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    #[test]
    fn test_format_bytes_gb() {
        assert_eq!(DownloadGauge::format_bytes(4_700_000_000), "4.7 GB");
        assert_eq!(DownloadGauge::format_bytes(1_000_000_000), "1.0 GB");
    }

    #[test]
    fn test_format_bytes_mb() {
        assert_eq!(DownloadGauge::format_bytes(500_000_000), "500.0 MB");
        assert_eq!(DownloadGauge::format_bytes(1_500_000), "1.5 MB");
    }

    #[test]
    fn test_format_speed() {
        assert_eq!(DownloadGauge::format_speed(50_000_000), "50.0 MB/s");
        assert_eq!(DownloadGauge::format_speed(500_000), "500.0 KB/s");
    }

    #[test]
    fn test_progress_clamp() {
        let gauge = DownloadGauge::new("test", 1.5, 100, 100);
        assert!((gauge.progress - 1.0).abs() < f64::EPSILON);

        let gauge = DownloadGauge::new("test", -0.5, 0, 100);
        assert!((gauge.progress - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_gauge_renders() {
        let gauge = DownloadGauge::new("llama3.2", 0.45, 2_100_000_000, 4_700_000_000);

        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 4));
        gauge.render(Rect::new(0, 0, 60, 4), &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("llama3.2"));
    }
}
