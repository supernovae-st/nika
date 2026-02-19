//! Spinner Widget
//!
//! Animated spinner for indicating ongoing operations.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};

/// Spinner animation frames
pub const BRAILLE_SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
pub const DOT_SPINNER: &[char] = &['⣾', '⣽', '⣻', '⢿', '⡿', '⣟', '⣯', '⣷'];
pub const PULSE_SPINNER: &[char] = &['○', '◔', '◑', '◕', '●', '◕', '◑', '◔'];

/// Animated spinner widget
pub struct Spinner {
    /// Current animation frame (0-255, will be modulated)
    frame: u8,
    /// Spinner type
    chars: &'static [char],
    /// Color
    color: Color,
}

impl Spinner {
    /// Create a new spinner with the given frame counter
    pub fn new(frame: u8) -> Self {
        Self {
            frame,
            chars: BRAILLE_SPINNER,
            color: Color::Cyan,
        }
    }

    /// Use braille spinner (default)
    pub fn braille(mut self) -> Self {
        self.chars = BRAILLE_SPINNER;
        self
    }

    /// Use dot/pulse spinner
    pub fn dots(mut self) -> Self {
        self.chars = DOT_SPINNER;
        self
    }

    /// Use pulse spinner (for status)
    pub fn pulse(mut self) -> Self {
        self.chars = PULSE_SPINNER;
        self
    }

    /// Set spinner color
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Get current spinner character
    pub fn current_char(&self) -> char {
        let idx = (self.frame / 6) as usize % self.chars.len();
        self.chars[idx]
    }
}

impl Widget for Spinner {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let ch = self.current_char();
        buf.set_string(
            area.x,
            area.y,
            ch.to_string(),
            Style::default().fg(self.color),
        );
    }
}

/// Progress dots animation (e.g., "Loading...")
pub struct ProgressDots {
    /// Current frame
    frame: u8,
    /// Base text
    text: String,
    /// Color
    color: Color,
}

impl ProgressDots {
    pub fn new(text: impl Into<String>, frame: u8) -> Self {
        Self {
            frame,
            text: text.into(),
            color: Color::Gray,
        }
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}

impl Widget for ProgressDots {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        // Cycle through 0, 1, 2, 3 dots
        let dots = (self.frame / 15) as usize % 4;
        let display = format!("{}{}", self.text, ".".repeat(dots));

        buf.set_string(area.x, area.y, &display, Style::default().fg(self.color));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spinner_cycles() {
        let s0 = Spinner::new(0);
        let s1 = Spinner::new(6);
        let s2 = Spinner::new(12);

        // Should cycle through different chars
        assert_eq!(s0.current_char(), '⠋');
        assert_eq!(s1.current_char(), '⠙');
        assert_eq!(s2.current_char(), '⠹');
    }

    #[test]
    fn test_spinner_wraps() {
        // Frame 60 should wrap back to start
        let s = Spinner::new(60);
        assert_eq!(s.current_char(), '⠋');
    }

    #[test]
    fn test_spinner_types() {
        let braille = Spinner::new(0).braille();
        let dots = Spinner::new(0).dots();
        let pulse = Spinner::new(0).pulse();

        assert_eq!(braille.current_char(), '⠋');
        assert_eq!(dots.current_char(), '⣾');
        assert_eq!(pulse.current_char(), '○');
    }
}
