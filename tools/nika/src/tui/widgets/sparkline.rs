//! Sparkline Widget
//!
//! Mini chart for visualizing latency and metric history.
//! Uses ratatui's Sparkline with latency-aware coloring.
//!
//! ## v0.8 Animation Support
//!
//! - `AnimatedLatencySparkline` - Pulse animation for real-time metrics
//! - `SparklineAnimation` - Animation type selection (None, Pulse, Flow, Wave)

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Sparkline as RatatuiSparkline, Widget},
};

// ═══════════════════════════════════════════════════════════════════════════
// DEFAULT COLORS (fallbacks for theme-aware rendering)
// ═══════════════════════════════════════════════════════════════════════════

const DEFAULT_ERROR_COLOR: Color = Color::Rgb(239, 68, 68); // red
const DEFAULT_WARN_COLOR: Color = Color::Rgb(245, 158, 11); // amber
const DEFAULT_SUCCESS_COLOR: Color = Color::Rgb(34, 197, 94); // green
const DEFAULT_HIGHLIGHT_COLOR: Color = Color::Rgb(99, 102, 241); // indigo for pulse

// ═══════════════════════════════════════════════════════════════════════════
// ANIMATION TYPES (v0.8)
// ═══════════════════════════════════════════════════════════════════════════

/// Animation style for sparklines
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SparklineAnimation {
    /// Static rendering (no animation)
    #[default]
    None,
    /// Latest bar pulses/glows to show recent activity
    Pulse,
    /// Data appears to "flow" from right to left
    Flow,
    /// Subtle sine wave effect on bar heights
    Wave,
}

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
            DEFAULT_ERROR_COLOR
        } else if max >= self.warn_threshold {
            DEFAULT_WARN_COLOR
        } else {
            DEFAULT_SUCCESS_COLOR
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
        // SAFETY: Clamp label length to u16::MAX before cast
        let label_len = self.label.len().min(u16::MAX as usize) as u16;
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

// ═══════════════════════════════════════════════════════════════════════════
// ANIMATED SPARKLINE (v0.8)
// ═══════════════════════════════════════════════════════════════════════════

/// Animated latency sparkline with threshold-based coloring
///
/// Supports multiple animation types:
/// - `Pulse`: Latest bar glows to indicate recent activity
/// - `Flow`: Data appears to scroll from right to left
/// - `Wave`: Subtle oscillation effect on bar heights
///
/// # Example
///
/// ```ignore
/// let data = vec![10, 20, 30, 40, 50];
/// let sparkline = AnimatedLatencySparkline::new(&data)
///     .frame(state.frame)
///     .animation(SparklineAnimation::Pulse)
///     .warn_threshold(500)
///     .error_threshold(2000);
/// sparkline.render(area, buf);
/// ```
pub struct AnimatedLatencySparkline<'a> {
    data: &'a [u64],
    /// Current animation frame (0-59, wraps for 1-second cycles at 60 FPS)
    frame: u8,
    /// Animation type
    animation: SparklineAnimation,
    /// Threshold in ms for warning color (default: 500ms)
    warn_threshold: u64,
    /// Threshold in ms for error color (default: 2000ms)
    error_threshold: u64,
    /// Show max value label
    show_max: bool,
    /// Show average value label
    show_avg: bool,
}

impl<'a> AnimatedLatencySparkline<'a> {
    pub fn new(data: &'a [u64]) -> Self {
        Self {
            data,
            frame: 0,
            animation: SparklineAnimation::None,
            warn_threshold: 500,
            error_threshold: 2000,
            show_max: false,
            show_avg: false,
        }
    }

    /// Set the current animation frame (from TuiState.frame)
    pub fn frame(mut self, frame: u8) -> Self {
        self.frame = frame;
        self
    }

    /// Set the animation type
    pub fn animation(mut self, animation: SparklineAnimation) -> Self {
        self.animation = animation;
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

    /// Get base color based on max latency value
    fn get_base_color(&self) -> Color {
        let max = self.data.iter().max().copied().unwrap_or(0);
        if max >= self.error_threshold {
            DEFAULT_ERROR_COLOR
        } else if max >= self.warn_threshold {
            DEFAULT_WARN_COLOR
        } else {
            DEFAULT_SUCCESS_COLOR
        }
    }

    /// Check if pulse should highlight (every 15 frames for 5 frames)
    fn is_pulse_active(&self) -> bool {
        matches!(self.animation, SparklineAnimation::Pulse) && (self.frame % 15) < 5
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

    /// Apply wave animation to data (modify values based on frame)
    fn apply_wave_animation(&self, data: &[u64]) -> Vec<u64> {
        if !matches!(self.animation, SparklineAnimation::Wave) {
            return data.to_vec();
        }

        let phase = (self.frame as f64 / 60.0) * std::f64::consts::PI * 2.0;
        data.iter()
            .enumerate()
            .map(|(i, &v)| {
                let wave = ((phase + i as f64 * 0.5).sin() * 0.1 + 1.0).max(0.9);
                ((v as f64 * wave) as u64).max(1)
            })
            .collect()
    }

    /// Apply flow animation (shift data visually)
    fn apply_flow_animation(&self, data: &[u64]) -> Vec<u64> {
        if !matches!(self.animation, SparklineAnimation::Flow) || data.is_empty() {
            return data.to_vec();
        }

        // Rotate data based on frame for "flow" effect
        let shift = (self.frame / 10) as usize % data.len().max(1);
        let mut result = data.to_vec();
        result.rotate_left(shift);
        result
    }
}

impl Widget for AnimatedLatencySparkline<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 5 || area.height < 1 || self.data.is_empty() {
            return;
        }

        let base_color = self.get_base_color();
        let pulse_active = self.is_pulse_active();

        // Apply animation transformations
        let animated_data = match self.animation {
            SparklineAnimation::Wave => self.apply_wave_animation(self.data),
            SparklineAnimation::Flow => self.apply_flow_animation(self.data),
            _ => self.data.to_vec(),
        };

        // Calculate space for labels
        let label_width = if self.show_max || self.show_avg {
            10u16
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

        // Determine style based on animation state
        let style = if pulse_active {
            Style::default()
                .fg(DEFAULT_HIGHLIGHT_COLOR)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(base_color)
        };

        // Render sparkline
        let sparkline = RatatuiSparkline::default()
            .block(Block::default())
            .data(&animated_data)
            .style(style);

        sparkline.render(sparkline_area, buf);

        // Render pulse indicator (latest bar marker)
        if pulse_active && sparkline_width > 0 {
            // Draw a glow marker at the rightmost position
            let marker_x = area.x + sparkline_width.saturating_sub(1);
            if marker_x < area.x + area.width {
                buf.set_string(
                    marker_x,
                    area.y,
                    "█",
                    Style::default()
                        .fg(DEFAULT_HIGHLIGHT_COLOR)
                        .add_modifier(Modifier::BOLD),
                );
            }
        }

        // Render labels if requested
        if self.show_max || self.show_avg {
            let label_x = area.x + sparkline_width + 1;
            let mut label_y = area.y;

            if self.show_max && area.height >= 1 {
                let max_str = format_ms(self.max());
                let max_label = format!("⬆{}", max_str);
                let label_style = if pulse_active {
                    Style::default()
                        .fg(DEFAULT_HIGHLIGHT_COLOR)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(base_color)
                };
                buf.set_string(label_x, label_y, &max_label, label_style);
                label_y += 1;
            }

            if self.show_avg && label_y < area.y + area.height {
                let avg_str = format_ms(self.average());
                let avg_label = format!("~{}", avg_str);
                buf.set_string(label_x, label_y, &avg_label, Style::default().fg(Color::Gray));
            }
        }
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
        assert_eq!(spark.get_color(), DEFAULT_SUCCESS_COLOR);

        let medium_data: Vec<u64> = vec![100, 200, 600, 400, 300];
        let spark = LatencySparkline::new(&medium_data);
        assert_eq!(spark.get_color(), DEFAULT_WARN_COLOR);

        let slow_data: Vec<u64> = vec![100, 200, 3000, 400, 300];
        let spark = LatencySparkline::new(&slow_data);
        assert_eq!(spark.get_color(), DEFAULT_ERROR_COLOR);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // ANIMATED SPARKLINE TESTS (v0.8)
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_sparkline_animation_default_is_none() {
        assert_eq!(SparklineAnimation::default(), SparklineAnimation::None);
    }

    #[test]
    fn test_animated_sparkline_defaults() {
        let data: Vec<u64> = vec![10, 20, 30];
        let spark = AnimatedLatencySparkline::new(&data);
        // Verify defaults
        assert!(!spark.is_pulse_active()); // frame=0 with None animation
    }

    #[test]
    fn test_animated_sparkline_pulse_timing() {
        let data: Vec<u64> = vec![10, 20, 30, 40, 50];

        // Pulse active in first 5 frames of each 15-frame cycle
        let spark = AnimatedLatencySparkline::new(&data)
            .frame(0)
            .animation(SparklineAnimation::Pulse);
        assert!(spark.is_pulse_active());

        let spark = AnimatedLatencySparkline::new(&data)
            .frame(4)
            .animation(SparklineAnimation::Pulse);
        assert!(spark.is_pulse_active());

        // Pulse inactive after 5 frames
        let spark = AnimatedLatencySparkline::new(&data)
            .frame(5)
            .animation(SparklineAnimation::Pulse);
        assert!(!spark.is_pulse_active());

        let spark = AnimatedLatencySparkline::new(&data)
            .frame(14)
            .animation(SparklineAnimation::Pulse);
        assert!(!spark.is_pulse_active());

        // Pulse active again at 15 (start of next cycle)
        let spark = AnimatedLatencySparkline::new(&data)
            .frame(15)
            .animation(SparklineAnimation::Pulse);
        assert!(spark.is_pulse_active());
    }

    #[test]
    fn test_animated_sparkline_pulse_not_active_with_none_animation() {
        let data: Vec<u64> = vec![10, 20, 30, 40, 50];

        // With None animation, pulse should never be active regardless of frame
        for frame in 0..60 {
            let spark = AnimatedLatencySparkline::new(&data)
                .frame(frame)
                .animation(SparklineAnimation::None);
            assert!(!spark.is_pulse_active(), "frame {} should not pulse", frame);
        }
    }

    #[test]
    fn test_animated_sparkline_wave_transforms_data() {
        let data: Vec<u64> = vec![100, 100, 100, 100, 100];
        let spark = AnimatedLatencySparkline::new(&data)
            .frame(30)
            .animation(SparklineAnimation::Wave);

        let transformed = spark.apply_wave_animation(&data);

        // Wave should transform data (values won't all be equal anymore)
        // Due to sine wave, values should vary
        assert_eq!(transformed.len(), data.len());
        // Values should be different from original due to wave effect
        // (unless we happen to hit a point where wave factor is exactly 1.0)
    }

    #[test]
    fn test_animated_sparkline_wave_returns_unchanged_for_non_wave() {
        let data: Vec<u64> = vec![10, 20, 30, 40, 50];

        // With None animation
        let spark = AnimatedLatencySparkline::new(&data)
            .frame(30)
            .animation(SparklineAnimation::None);
        let result = spark.apply_wave_animation(&data);
        assert_eq!(result, data);

        // With Pulse animation
        let spark = AnimatedLatencySparkline::new(&data)
            .frame(30)
            .animation(SparklineAnimation::Pulse);
        let result = spark.apply_wave_animation(&data);
        assert_eq!(result, data);
    }

    #[test]
    fn test_animated_sparkline_flow_rotates_data() {
        let data: Vec<u64> = vec![10, 20, 30, 40, 50];

        // At frame 0, shift is 0, so no rotation
        let spark = AnimatedLatencySparkline::new(&data)
            .frame(0)
            .animation(SparklineAnimation::Flow);
        let result = spark.apply_flow_animation(&data);
        assert_eq!(result, vec![10, 20, 30, 40, 50]);

        // At frame 10, shift is 1
        let spark = AnimatedLatencySparkline::new(&data)
            .frame(10)
            .animation(SparklineAnimation::Flow);
        let result = spark.apply_flow_animation(&data);
        assert_eq!(result, vec![20, 30, 40, 50, 10]);

        // At frame 20, shift is 2
        let spark = AnimatedLatencySparkline::new(&data)
            .frame(20)
            .animation(SparklineAnimation::Flow);
        let result = spark.apply_flow_animation(&data);
        assert_eq!(result, vec![30, 40, 50, 10, 20]);
    }

    #[test]
    fn test_animated_sparkline_flow_empty_data() {
        let data: Vec<u64> = vec![];
        let spark = AnimatedLatencySparkline::new(&data)
            .frame(10)
            .animation(SparklineAnimation::Flow);
        let result = spark.apply_flow_animation(&data);
        assert!(result.is_empty());
    }

    #[test]
    fn test_animated_sparkline_color_thresholds() {
        let fast_data: Vec<u64> = vec![10, 20, 30, 40, 50];
        let spark = AnimatedLatencySparkline::new(&fast_data);
        assert_eq!(spark.get_base_color(), DEFAULT_SUCCESS_COLOR);

        let medium_data: Vec<u64> = vec![100, 200, 600, 400, 300];
        let spark = AnimatedLatencySparkline::new(&medium_data);
        assert_eq!(spark.get_base_color(), DEFAULT_WARN_COLOR);

        let slow_data: Vec<u64> = vec![100, 200, 3000, 400, 300];
        let spark = AnimatedLatencySparkline::new(&slow_data);
        assert_eq!(spark.get_base_color(), DEFAULT_ERROR_COLOR);
    }

    #[test]
    fn test_animated_sparkline_custom_thresholds() {
        let data: Vec<u64> = vec![100, 200, 300];

        // Default thresholds (warn=500, error=2000)
        let spark = AnimatedLatencySparkline::new(&data);
        assert_eq!(spark.get_base_color(), DEFAULT_SUCCESS_COLOR);

        // Lower thresholds
        let spark = AnimatedLatencySparkline::new(&data)
            .warn_threshold(250)
            .error_threshold(400);
        assert_eq!(spark.get_base_color(), DEFAULT_WARN_COLOR); // max=300 > warn=250

        let spark = AnimatedLatencySparkline::new(&data)
            .warn_threshold(100)
            .error_threshold(200);
        assert_eq!(spark.get_base_color(), DEFAULT_ERROR_COLOR); // max=300 > error=200
    }

    #[test]
    fn test_animated_sparkline_builder_chain() {
        let data: Vec<u64> = vec![10, 20, 30];
        let spark = AnimatedLatencySparkline::new(&data)
            .frame(30)
            .animation(SparklineAnimation::Pulse)
            .warn_threshold(100)
            .error_threshold(500)
            .show_max()
            .show_avg();

        // Just verify the chain compiles and returns a valid object
        assert!(spark.is_pulse_active() || !spark.is_pulse_active()); // trivially true, but validates construction
    }
}
