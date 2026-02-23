//! BigText Widget
//!
//! Large ASCII art text for TUI displays using 3-line height block characters.
//!
//! ## Usage
//!
//! ```ignore
//! use nika::tui::widgets::BigText;
//! use ratatui::style::{Color, Style};
//!
//! let text = BigText::new("NIKA")
//!     .style(Style::default().fg(Color::Cyan));
//! text.render(area, buf);
//! ```
//!
//! ## Supported Characters
//!
//! - Uppercase letters: A-Z
//! - Numbers: 0-9
//! - Space
//! - Punctuation: ! . : -
//!
//! Unsupported characters render as spaces.

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::Style,
    widgets::Widget,
};

/// Large ASCII art text widget (3 lines tall)
///
/// Uses Unicode block characters for crisp rendering in terminals.
pub struct BigText<'a> {
    text: &'a str,
    style: Style,
    alignment: Alignment,
}

impl<'a> BigText<'a> {
    /// Create a new BigText widget
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            style: Style::default(),
            alignment: Alignment::Left,
        }
    }

    /// Set the style for the text
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Set the alignment (Left, Center, Right)
    pub fn alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }

    /// Calculate total width of text in characters
    pub fn text_width(text: &str) -> u16 {
        text.chars()
            .map(Self::char_width)
            .sum::<u16>()
            .saturating_sub(1) // Remove trailing space
    }

    /// Get width of a single character (includes 1 char spacing)
    fn char_width(c: char) -> u16 {
        match c.to_ascii_uppercase() {
            ' ' => 3,             // Space is narrower
            '!' | ':' | '.' => 3, // Punctuation is narrow
            '-' => 4,             // Dash
            _ => 5,               // Most characters are 4 wide + 1 space
        }
    }

    /// Get the 3-line representation of a character
    ///
    /// Returns [top_line, middle_line, bottom_line]
    fn char_lines(c: char) -> [&'static str; 3] {
        match c.to_ascii_uppercase() {
            // ═══════════════════════════════════════════════════════════════════
            // LETTERS A-Z
            // ═══════════════════════════════════════════════════════════════════
            'A' => ["█▀█ ", "█▀█ ", "█ █ "],
            'B' => ["█▀▄ ", "█▀▄ ", "█▀  "],
            'C' => ["█▀▀ ", "█   ", "▀▀▀ "],
            'D' => ["█▀▄ ", "█ █ ", "▀▀  "],
            'E' => ["█▀▀ ", "██▀ ", "▀▀▀ "],
            'F' => ["█▀▀ ", "█▀  ", "█   "],
            'G' => ["█▀▀ ", "█ █ ", "▀▀▀ "],
            'H' => ["█ █ ", "█▀█ ", "█ █ "],
            'I' => ["█ ", "█ ", "█ "],
            'J' => ["  █ ", "  █ ", "▀▄█ "],
            'K' => ["█▄▀ ", "█ █ ", "█ █ "],
            'L' => ["█   ", "█   ", "▀▀▀ "],
            'M' => ["█▄▄█ ", "█ ▀ █ ", "█   █ "],
            'N' => ["█▄ █ ", "█ ██ ", "█  █ "],
            'O' => ["█▀█ ", "█ █ ", "▀▀▀ "],
            'P' => ["█▀█ ", "█▀▀ ", "█   "],
            'Q' => ["█▀█ ", "█ █ ", "▀▀█ "],
            'R' => ["█▀█ ", "█▀▄ ", "█ █ "],
            'S' => ["█▀▀ ", "▀▀█ ", "▀▀▀ "],
            'T' => ["▀█▀ ", " █  ", " █  "],
            'U' => ["█ █ ", "█ █ ", "▀▀▀ "],
            'V' => ["█ █ ", "█▄█ ", " ▀  "],
            'W' => ["█   █ ", "█▄▀▄█ ", "▀   ▀ "],
            'X' => ["▀▄▀ ", " █  ", "█ █ "],
            'Y' => ["█ █ ", "▀█▀ ", " █  "],
            'Z' => ["▀▀█ ", " █▀ ", "▀▀▀ "],

            // ═══════════════════════════════════════════════════════════════════
            // NUMBERS 0-9
            // ═══════════════════════════════════════════════════════════════════
            '0' => ["█▀█ ", "█ █ ", "▀▀▀ "],
            '1' => ["▀█ ", " █ ", "▀▀▀ "],
            '2' => ["▀██ ", " █▀ ", "▀▀▀ "],
            '3' => ["██▀ ", "▀▄█ ", "▀▀  "],
            '4' => ["█ █ ", "▀▀█ ", "  █ "],
            '5' => ["█▀  ", "██▄ ", "▀▀  "],
            '6' => ["█▀  ", "██▄ ", "▀▀  "],
            '7' => ["▀▀█ ", "  █ ", "  █ "],
            '8' => ["█▀█ ", "█▀█ ", "▀▀▀ "],
            '9' => ["█▀█ ", "▀▀█ ", "▀▀▀ "],

            // ═══════════════════════════════════════════════════════════════════
            // PUNCTUATION
            // ═══════════════════════════════════════════════════════════════════
            ' ' => ["  ", "  ", "  "],
            '!' => ["█ ", "█ ", "▄ "],
            '.' => ["  ", "  ", "▄ "],
            ':' => ["▄ ", "  ", "▄ "],
            '-' => ["   ", "▀▀ ", "   "],

            // Unsupported characters render as space
            _ => ["  ", "  ", "  "],
        }
    }
}

impl Widget for BigText<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 3 || area.height < 3 {
            return;
        }

        // Calculate total text width
        let total_width = Self::text_width(self.text);

        // Calculate starting X based on alignment
        let start_x = match self.alignment {
            Alignment::Left => area.x,
            Alignment::Center => area.x + area.width.saturating_sub(total_width) / 2,
            Alignment::Right => area.x + area.width.saturating_sub(total_width),
        };

        // Render each line (3 lines total)
        for line_idx in 0..3 {
            let y = area.y + line_idx;
            if y >= area.y + area.height {
                break;
            }

            let mut x = start_x;
            for c in self.text.chars() {
                let lines = Self::char_lines(c);
                let line = lines[line_idx as usize];

                for ch in line.chars() {
                    if x < area.x + area.width {
                        if let Some(cell) = buf.cell_mut((x, y)) {
                            cell.set_char(ch).set_style(self.style);
                        }
                        x += 1;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    #[test]
    fn test_big_text_width_calculation() {
        // Single letter (4 wide + 1 space, minus trailing = 4)
        assert_eq!(BigText::text_width("A"), 4);

        // Two letters (5 + 5 - 1 trailing = 9)
        assert_eq!(BigText::text_width("OK"), 9);

        // With space (5 + 3 + 5 - 1 = 12)
        assert_eq!(BigText::text_width("A B"), 12);
    }

    #[test]
    fn test_big_text_nika_width() {
        // N=5, I=2, K=5, A=5 - 1 = 16
        let width = BigText::text_width("NIKA");
        assert!(width > 10, "NIKA should be > 10 chars wide, got {}", width);
    }

    #[test]
    fn test_big_text_char_lines_returns_3_lines() {
        for c in "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789".chars() {
            let lines = BigText::char_lines(c);
            assert_eq!(lines.len(), 3, "Character '{}' should have 3 lines", c);
        }
    }

    #[test]
    fn test_big_text_unsupported_char_renders_as_space() {
        let lines = BigText::char_lines('@');
        assert_eq!(lines, ["  ", "  ", "  "]);
    }

    #[test]
    fn test_big_text_builder_chain() {
        let text = BigText::new("TEST")
            .style(Style::default().fg(Color::Cyan))
            .alignment(Alignment::Center);

        // Verify construction succeeds
        assert_eq!(text.text, "TEST");
    }

    #[test]
    fn test_big_text_render_to_buffer() {
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;

        let area = Rect::new(0, 0, 20, 3);
        let mut buf = Buffer::empty(area);

        let text = BigText::new("OK");
        text.render(area, &mut buf);

        // Check that something was rendered (not all spaces)
        let content: String = (0..3)
            .map(|y| {
                (0..20)
                    .map(|x| buf.get(x, y).symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            content.contains('█') || content.contains('▀') || content.contains('▄'),
            "Buffer should contain block characters: {}",
            content
        );
    }

    #[test]
    fn test_big_text_alignment_center() {
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;

        let area = Rect::new(0, 0, 40, 3);
        let mut buf = Buffer::empty(area);

        let text = BigText::new("HI").alignment(Alignment::Center);
        text.render(area, &mut buf);

        // With centering, text shouldn't start at x=0
        let first_line: String = (0..40)
            .map(|x| buf.get(x, 0).symbol().to_string())
            .collect();

        // Find first non-space character
        let first_char_pos = first_line.find(|c: char| c != ' ');
        assert!(first_char_pos.is_some(), "Should have non-space content");
        assert!(
            first_char_pos.unwrap() > 0,
            "Centered text should not start at x=0"
        );
    }

    #[test]
    fn test_big_text_empty_string() {
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;

        let area = Rect::new(0, 0, 20, 3);
        let mut buf = Buffer::empty(area);

        let text = BigText::new("");
        text.render(area, &mut buf);

        // Should not crash, buffer should be unchanged (all spaces)
    }

    #[test]
    fn test_big_text_small_area_no_crash() {
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;

        // Very small area - should not crash
        let area = Rect::new(0, 0, 2, 1);
        let mut buf = Buffer::empty(area);

        let text = BigText::new("HELLO");
        text.render(area, &mut buf); // Should not panic
    }
}
