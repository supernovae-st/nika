//! Gauge Widget
//!
//! Progress bar with label and percentage.

use std::borrow::Cow;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};

/// Progress gauge widget
pub struct Gauge<'a> {
    /// Progress ratio (0.0 to 1.0)
    ratio: f64,
    /// Label text
    label: Cow<'a, str>,
    /// Gauge style
    style: Style,
    /// Fill color
    fill_color: Color,
    /// Background color
    bg_color: Color,
    /// Show percentage text
    show_percent: bool,
}

impl<'a> Gauge<'a> {
    pub fn new(ratio: f64) -> Self {
        Self {
            ratio: ratio.clamp(0.0, 1.0),
            label: Cow::Borrowed(""),
            style: Style::default(),
            fill_color: Color::Rgb(99, 102, 241), // indigo
            bg_color: Color::Rgb(55, 65, 81),     // gray-700
            show_percent: true,
        }
    }

    pub fn label(mut self, label: &'a str) -> Self {
        self.label = Cow::Borrowed(label);
        self
    }

    pub fn label_owned(mut self, label: String) -> Self {
        self.label = Cow::Owned(label);
        self
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    pub fn fill_color(mut self, color: Color) -> Self {
        self.fill_color = color;
        self
    }

    pub fn bg_color(mut self, color: Color) -> Self {
        self.bg_color = color;
        self
    }

    pub fn show_percent(mut self, show: bool) -> Self {
        self.show_percent = show;
        self
    }

    /// Create a gauge with status-appropriate coloring
    pub fn for_progress(completed: usize, total: usize) -> Self {
        let ratio = if total == 0 {
            0.0
        } else {
            completed as f64 / total as f64
        };

        let color = if ratio >= 1.0 {
            Color::Rgb(34, 197, 94) // green - complete
        } else if ratio > 0.0 {
            Color::Rgb(245, 158, 11) // amber - in progress
        } else {
            Color::Rgb(107, 114, 128) // gray - not started
        };

        Self::new(ratio)
            .fill_color(color)
            .label_owned(format!("{}/{}", completed, total))
    }
}

impl Widget for Gauge<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width < 5 {
            return;
        }

        // Use block characters for smooth progress
        // Full blocks: ████████
        // Partial:     ▏▎▍▌▋▊▉█

        let gauge_width = area.width as f64;
        let filled_width = (gauge_width * self.ratio).floor() as u16;
        let partial = ((gauge_width * self.ratio).fract() * 8.0).floor() as usize;

        // Draw background
        let bg_style = Style::default().bg(self.bg_color);
        for x in area.x..(area.x + area.width) {
            buf.set_string(x, area.y, " ", bg_style);
        }

        // Draw filled portion
        let fill_style = Style::default().fg(self.fill_color).bg(self.fill_color);
        for x in area.x..(area.x + filled_width) {
            buf.set_string(x, area.y, "█", fill_style);
        }

        // Draw partial block
        if partial > 0 && filled_width < area.width {
            let partial_chars = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉"];
            let partial_char = partial_chars[partial.min(7)];
            buf.set_string(
                area.x + filled_width,
                area.y,
                partial_char,
                Style::default().fg(self.fill_color).bg(self.bg_color),
            );
        }

        // Draw label/percentage in center
        let text = if self.show_percent {
            if self.label.is_empty() {
                format!("{:.0}%", self.ratio * 100.0)
            } else {
                format!("{} ({:.0}%)", self.label, self.ratio * 100.0)
            }
        } else {
            self.label.to_string()
        };

        if !text.is_empty() && area.width > text.len() as u16 + 2 {
            let text_x = area.x + (area.width.saturating_sub(text.len() as u16)) / 2;
            buf.set_string(
                text_x,
                area.y,
                &text,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gauge_creation() {
        let gauge = Gauge::new(0.5).label("Test");
        assert_eq!(gauge.ratio, 0.5);
        assert_eq!(gauge.label, "Test");
    }

    #[test]
    fn test_gauge_clamping() {
        let over = Gauge::new(1.5);
        assert_eq!(over.ratio, 1.0);

        let under = Gauge::new(-0.5);
        assert_eq!(under.ratio, 0.0);
    }

    #[test]
    fn test_for_progress() {
        let gauge = Gauge::for_progress(5, 10);
        assert_eq!(gauge.ratio, 0.5);

        let empty = Gauge::for_progress(0, 0);
        assert_eq!(empty.ratio, 0.0);
    }
}
