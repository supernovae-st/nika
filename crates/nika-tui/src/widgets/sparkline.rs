//! Sparkline Widget
//!
//! Mini chart for visualizing latency and metric history.
//! Uses ratatui's Sparkline with latency-aware coloring.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, Sparkline as RatatuiSparkline, Widget},
};

/// Latency sparkline with threshold-based coloring
pub struct LatencySparkline<'a> {
    data: &'a [u64],
    title: &'a str,
    /// Threshold in ms for warning color (default: 500ms)
    warn_threshold: u64,
    /// Threshold in ms for error color (default: 2000ms)
    error_threshold: u64,
    /// Show max value label
    show_max: bool,
    /// Show average value label
    show_avg: bool,
}

impl<'a> LatencySparkline<'a> {
    pub fn new(data: &'a [u64]) -> Self {
        Self {
            data,
            title: "",
            warn_threshold: 500,
            error_threshold: 2000,
            show_max: false,
            show_avg: false,
        }
    }

    pub fn title(mut self, title: &'a str) -> Self {
        self.title = title;
        self
    }

    pub fn warn_threshold(mut self, ms: u64) -> Self {
        self.warn_threshold = ms;
        self
    }

    pub fn error_threshold(mut self, ms: u64) -> Self {
        self.error_threshold = ms;
        self
    }

    pub fn show_max(mut self) -> Self {
        self.show_max = true;
        self
    }

    pub fn show_avg(mut self) -> Self {
        self.show_avg = true;
        self
    }

    /// Get color based on max latency value
    fn get_color(&self) -> Color {
        let max = self.data.iter().max().copied().unwrap_or(0);
        if max >= self.error_threshold {
            Color::Rgb(239, 68, 68) // red
        } else if max >= self.warn_threshold {
            Color::Rgb(245, 158, 11) // amber
        } else {
            Color::Rgb(34, 197, 94) // green
        }
    }

    /// Calculate average latency
    fn average(&self) -> u64 {
        if self.data.is_empty() {
            return 0;
        }
        self.data.iter().sum::<u64>() / self.data.len() as u64
    }

    /// Get max latency
    fn max(&self) -> u64 {
        self.data.iter().max().copied().unwrap_or(0)
    }
}

impl Widget for LatencySparkline<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 5 || area.height < 1 {
            return;
        }

        let color = self.get_color();

        // Calculate how much space for labels
        let label_width = if self.show_max || self.show_avg {
            10u16 // "avg:999ms" or "max:999ms"
        } else {
            0
        };

        let sparkline_width = area.width.saturating_sub(label_width);
        let sparkline_area = Rect {
            x: area.x,
            y: area.y,
            width: sparkline_width,
            height: area.height,
        };

        // Render sparkline
        let sparkline = RatatuiSparkline::default()
            .block(Block::default())
            .data(self.data)
            .style(Style::default().fg(color));

        sparkline.render(sparkline_area, buf);

        // Render labels if requested
        if self.show_max || self.show_avg {
            let label_x = area.x + sparkline_width + 1;
            let mut label_y = area.y;

            if self.show_max && area.height >= 1 {
                let max_str = format_ms(self.max());
                let max_label = format!("⬆{}", max_str);
                buf.set_string(label_x, label_y, &max_label, Style::default().fg(color));
                label_y += 1;
            }

            if self.show_avg && label_y < area.y + area.height {
                let avg_str = format_ms(self.average());
                let avg_label = format!("~{}", avg_str);
                buf.set_string(
                    label_x, // Fixed: was incorrectly using label_y as X coordinate
                    label_y,
                    &avg_label,
                    Style::default().fg(Color::Gray),
                );
            }
        }
    }
}

/// Compact sparkline with inline label
pub struct MiniSparkline<'a> {
    data: &'a [u64],
    label: &'a str,
    color: Color,
}

impl<'a> MiniSparkline<'a> {
    pub fn new(data: &'a [u64], label: &'a str) -> Self {
        Self {
            data,
            label,
            color: Color::Cyan,
        }
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}

impl Widget for MiniSparkline<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 8 || area.height < 1 {
            return;
        }

        // Render label first
        let label_len = self.label.len() as u16;
        buf.set_string(
            area.x,
            area.y,
            self.label,
            Style::default().fg(Color::DarkGray),
        );

        // Sparkline takes remaining space
        let sparkline_area = Rect {
            x: area.x + label_len + 1,
            y: area.y,
            width: area.width.saturating_sub(label_len + 1),
            height: 1,
        };

        let sparkline = RatatuiSparkline::default()
            .block(Block::default())
            .data(self.data)
            .style(Style::default().fg(self.color));

        sparkline.render(sparkline_area, buf);
    }
}

/// Sparkline with border and title
pub struct BorderedSparkline<'a> {
    data: &'a [u64],
    title: &'a str,
    color: Color,
}

impl<'a> BorderedSparkline<'a> {
    pub fn new(data: &'a [u64], title: &'a str) -> Self {
        Self {
            data,
            title,
            color: Color::Cyan,
        }
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}

impl Widget for BorderedSparkline<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 6 || area.height < 3 {
            return;
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .title(self.title)
            .border_style(Style::default().fg(Color::DarkGray));

        let inner = block.inner(area);
        block.render(area, buf);

        let sparkline = RatatuiSparkline::default()
            .block(Block::default())
            .data(self.data)
            .style(Style::default().fg(self.color));

        sparkline.render(inner, buf);
    }
}

/// Format milliseconds for display
fn format_ms(ms: u64) -> String {
    if ms >= 1000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("{}ms", ms)
    }
}

/// Latency history tracker
///
/// Uses `VecDeque` for O(1) push/pop operations instead of Vec's O(n) remove(0).
#[derive(Debug, Clone, Default)]
pub struct LatencyHistory {
    /// Ring buffer of latency values in ms
    values: std::collections::VecDeque<u64>,
    /// Maximum number of values to keep
    max_size: usize,
    /// Cached slice for sparkline rendering (to avoid allocation on data() calls)
    cache: Vec<u64>,
    /// Whether cache needs refresh
    cache_dirty: bool,
}

impl LatencyHistory {
    pub fn new(max_size: usize) -> Self {
        Self {
            values: std::collections::VecDeque::with_capacity(max_size),
            max_size,
            cache: Vec::with_capacity(max_size),
            cache_dirty: true,
        }
    }

    pub fn push(&mut self, latency_ms: u64) {
        if self.values.len() >= self.max_size {
            self.values.pop_front(); // O(1) with VecDeque vs O(n) with Vec
        }
        self.values.push_back(latency_ms);
        self.cache_dirty = true;
    }

    /// Get data as a slice for sparkline rendering
    ///
    /// Uses internal cache to avoid allocation on repeated calls within same frame.
    pub fn data(&mut self) -> &[u64] {
        if self.cache_dirty {
            self.cache.clear();
            self.cache.extend(self.values.iter().copied());
            self.cache_dirty = false;
        }
        &self.cache
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn average(&self) -> u64 {
        if self.values.is_empty() {
            return 0;
        }
        self.values.iter().sum::<u64>() / self.values.len() as u64
    }

    pub fn max(&self) -> u64 {
        self.values.iter().max().copied().unwrap_or(0)
    }

    pub fn min(&self) -> u64 {
        self.values.iter().min().copied().unwrap_or(0)
    }

    pub fn clear(&mut self) {
        self.values.clear();
        self.cache_dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_latency_history_ring_buffer() {
        let mut history = LatencyHistory::new(5);

        // Fill to capacity
        for i in 1..=5 {
            history.push(i * 10);
        }
        assert_eq!(history.len(), 5);
        assert_eq!(history.data(), &[10, 20, 30, 40, 50]);

        // Add one more - should remove oldest (O(1) with VecDeque)
        history.push(60);
        assert_eq!(history.len(), 5);
        assert_eq!(history.data(), &[20, 30, 40, 50, 60]);
    }

    #[test]
    fn test_latency_history_stats() {
        let mut history = LatencyHistory::new(10);
        history.push(100);
        history.push(200);
        history.push(300);

        assert_eq!(history.average(), 200);
        assert_eq!(history.max(), 300);
        assert_eq!(history.min(), 100);
    }

    #[test]
    fn test_latency_history_empty() {
        let history = LatencyHistory::new(10);
        assert!(history.is_empty());
        assert_eq!(history.average(), 0);
        assert_eq!(history.max(), 0);
        assert_eq!(history.min(), 0);
    }

    #[test]
    fn test_format_ms() {
        assert_eq!(format_ms(50), "50ms");
        assert_eq!(format_ms(999), "999ms");
        assert_eq!(format_ms(1000), "1.0s");
        assert_eq!(format_ms(1500), "1.5s");
        assert_eq!(format_ms(10000), "10.0s");
    }

    #[test]
    fn test_latency_sparkline_color_thresholds() {
        let fast_data: Vec<u64> = vec![10, 20, 30, 40, 50];
        let spark = LatencySparkline::new(&fast_data);
        assert_eq!(spark.get_color(), Color::Rgb(34, 197, 94)); // green

        let medium_data: Vec<u64> = vec![100, 200, 600, 400, 300];
        let spark = LatencySparkline::new(&medium_data);
        assert_eq!(spark.get_color(), Color::Rgb(245, 158, 11)); // amber

        let slow_data: Vec<u64> = vec![100, 200, 3000, 400, 300];
        let spark = LatencySparkline::new(&slow_data);
        assert_eq!(spark.get_color(), Color::Rgb(239, 68, 68)); // red
    }
}
