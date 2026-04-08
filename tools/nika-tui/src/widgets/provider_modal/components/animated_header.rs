// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Animated cosmic header for Provider Modal
//!
//! WOW Cosmic Edition
//! Displays animated star patterns around the modal title

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};

// =============================================================================
// SEMANTIC COLOR CONSTANTS
// =============================================================================

/// Title text
const COLOR_TITLE: Color = Color::Rgb(229, 231, 235); // Gray-200

/// Star animation frames for cosmic effect
const STAR_FRAMES: &[&[&str]] = &[
    &["✧", "･", "ﾟ", ":", "*", "✧", "･", "ﾟ", ":", "*"],
    &["✦", "*", ":", "ﾟ", "･", "✦", "*", ":", "ﾟ", "･"],
    &["★", "ﾟ", "･", "*", ":", "★", "ﾟ", "･", "*", ":"],
    &["✵", ":", "*", "･", "ﾟ", "✵", ":", "*", "･", "ﾟ"],
];

/// Colors for gradient effect (indigo -> purple -> pink)
const GRADIENT_COLORS: &[Color] = &[
    Color::Rgb(99, 102, 241), // Indigo-500
    Color::Rgb(139, 92, 246), // Violet-500
    Color::Rgb(236, 72, 153), // Pink-500
    Color::Rgb(139, 92, 246), // Violet-500
    Color::Rgb(99, 102, 241), // Indigo-500
];

/// Animated cosmic header widget
pub struct AnimatedHeader {
    /// Current animation frame
    frame: u8,
    /// Title text
    title: &'static str,
}

impl Default for AnimatedHeader {
    fn default() -> Self {
        Self::new()
    }
}

impl AnimatedHeader {
    /// Create new header with default title
    pub fn new() -> Self {
        Self {
            frame: 0,
            title: "PROVIDER COMMAND CENTER",
        }
    }

    /// Create header with custom title
    pub fn with_title(mut self, title: &'static str) -> Self {
        self.title = title;
        self
    }

    /// Set animation frame
    pub fn with_frame(mut self, frame: u8) -> Self {
        self.frame = frame;
        self
    }

    /// Advance animation by one tick
    pub fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    /// Get current star pattern
    fn current_stars(&self) -> &'static [&'static str] {
        let frame_idx = (self.frame as usize / 4) % STAR_FRAMES.len();
        STAR_FRAMES[frame_idx]
    }

    /// Get color for position in gradient
    fn gradient_color(&self, pos: usize, total: usize) -> Color {
        if total == 0 {
            return GRADIENT_COLORS[0];
        }
        let idx = (pos * GRADIENT_COLORS.len()) / total;
        GRADIENT_COLORS[idx.min(GRADIENT_COLORS.len() - 1)]
    }

    /// Render header to string for testing
    #[cfg(test)]
    pub fn render_to_string(&self, _width: usize) -> String {
        let stars = self.current_stars();
        let left_stars: String = stars[..5].join("");
        let right_stars: String = stars[5..].join("");
        format!("{}   ◆ {}   {}", left_stars, self.title, right_stars)
    }
}

impl Widget for AnimatedHeader {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 20 || area.height < 1 {
            return;
        }

        let stars = self.current_stars();
        let title_with_diamond = format!("◆ {} ◆", self.title);

        // Calculate positions
        let total_width = area.width as usize;
        let title_len = title_with_diamond.chars().count();
        let title_x = (total_width.saturating_sub(title_len)) / 2;

        // Render left stars with gradient
        let left_stars: String = stars[..5].join("");
        let left_x = area.x + (title_x.saturating_sub(left_stars.chars().count() + 2)) as u16;
        for (i, ch) in left_stars.chars().enumerate() {
            let color = self.gradient_color(i, 5);
            buf.set_string(
                left_x + i as u16,
                area.y,
                ch.to_string(),
                Style::default().fg(color),
            );
        }

        // Render title with bold style
        buf.set_string(
            area.x + title_x as u16,
            area.y,
            &title_with_diamond,
            Style::default()
                .fg(COLOR_TITLE)
                .add_modifier(Modifier::BOLD),
        );

        // Render right stars with gradient
        let right_stars: String = stars[5..].join("");
        let right_x = area.x + (title_x + title_len + 2) as u16;
        for (i, ch) in right_stars.chars().enumerate() {
            let color = self.gradient_color(4 - i.min(4), 5);
            buf.set_string(
                right_x + i as u16,
                area.y,
                ch.to_string(),
                Style::default().fg(color),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_animated_header_default() {
        let header = AnimatedHeader::new();
        assert_eq!(header.frame, 0);
        assert_eq!(header.title, "PROVIDER COMMAND CENTER");
    }

    #[test]
    fn test_animated_header_with_title() {
        let header = AnimatedHeader::new().with_title("CUSTOM TITLE");
        assert_eq!(header.title, "CUSTOM TITLE");
    }

    #[test]
    fn test_animated_header_tick() {
        let mut header = AnimatedHeader::new();
        assert_eq!(header.frame, 0);
        header.tick();
        assert_eq!(header.frame, 1);
        header.tick();
        assert_eq!(header.frame, 2);
    }

    #[test]
    fn test_animated_header_cycles_stars() {
        let header0 = AnimatedHeader::new().with_frame(0);
        let header16 = AnimatedHeader::new().with_frame(16);

        // Different frames should produce different star patterns
        // Frame 0: pattern 0, Frame 16: pattern 0 (cycles every 16 frames)
        let frame0 = header0.render_to_string(80);
        let frame16 = header16.render_to_string(80);

        // Same frame index (16/4 = 4, 4 % 4 = 0), same pattern
        assert_eq!(frame0, frame16);
    }

    #[test]
    fn test_animated_header_different_frames() {
        let header0 = AnimatedHeader::new().with_frame(0);
        let header4 = AnimatedHeader::new().with_frame(4);

        let frame0 = header0.render_to_string(80);
        let frame4 = header4.render_to_string(80);

        // Frame 0: pattern 0, Frame 4: pattern 1 (different)
        assert_ne!(frame0, frame4);
    }

    #[test]
    fn test_gradient_color_bounds() {
        let header = AnimatedHeader::new();

        // Test various positions
        let _c0 = header.gradient_color(0, 5);
        let _c2 = header.gradient_color(2, 5);
        let _c4 = header.gradient_color(4, 5);

        // Edge case: total = 0
        let c_zero = header.gradient_color(0, 0);
        assert_eq!(c_zero, GRADIENT_COLORS[0]);
    }

    #[test]
    fn test_animated_header_renders() {
        let header = AnimatedHeader::new();
        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 1));
        header.render(Rect::new(0, 0, 80, 1), &mut buf);

        // Check that title is rendered
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("PROVIDER COMMAND CENTER"));
    }

    #[test]
    fn test_animated_header_small_area_no_panic() {
        let header = AnimatedHeader::new();
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 1));
        // Should not panic on small area
        header.render(Rect::new(0, 0, 10, 1), &mut buf);
    }
}
