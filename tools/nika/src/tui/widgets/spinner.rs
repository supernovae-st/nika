//! Spinner Widget
//!
//! Animated spinner for indicating ongoing operations.
//! Includes space-themed spinners for the Nika workflow engine.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};

/// Spinner animation frames
pub const BRAILLE_SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
pub const DOT_SPINNER: &[char] = &['⣾', '⣽', '⣻', '⢿', '⡿', '⣟', '⣯', '⣷'];
pub const PULSE_SPINNER: &[char] = &['○', '◔', '◑', '◕', '●', '◕', '◑', '◔'];

// Space-themed spinners (TIER 5)
pub const ROCKET_SPINNER: &[char] = &['🚀', '🔥', '✨', '💫', '⭐'];
pub const STARS_SPINNER: &[char] = &['✦', '✧', '★', '☆', '✵', '✶'];
pub const ORBIT_SPINNER: &[char] = &['◐', '◓', '◑', '◒'];
pub const COSMIC_SPINNER: &[char] = &['🌑', '🌒', '🌓', '🌔', '🌕', '🌖', '🌗', '🌘'];

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

    /// Use rocket spinner (space theme)
    pub fn rocket(mut self) -> Self {
        self.chars = ROCKET_SPINNER;
        self
    }

    /// Use stars spinner (space theme)
    pub fn stars(mut self) -> Self {
        self.chars = STARS_SPINNER;
        self
    }

    /// Use orbit spinner (space theme)
    pub fn orbit(mut self) -> Self {
        self.chars = ORBIT_SPINNER;
        self
    }

    /// Use cosmic moon phase spinner (space theme)
    pub fn cosmic(mut self) -> Self {
        self.chars = COSMIC_SPINNER;
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

/// Pulse text widget - makes text "breathe" by cycling through brightness levels
/// Creates a subtle animation effect that draws attention without being distracting
pub struct PulseText {
    /// Current animation frame (0-255)
    frame: u8,
    /// The text to display
    text: String,
    /// Base color (will be dimmed/brightened)
    base_color: Color,
    /// Whether to apply bold on peak brightness
    bold_on_peak: bool,
}

impl PulseText {
    /// Create a new pulse text widget
    pub fn new(text: impl Into<String>, frame: u8) -> Self {
        Self {
            frame,
            text: text.into(),
            base_color: Color::Cyan,
            bold_on_peak: true,
        }
    }

    /// Set the base color
    pub fn color(mut self, color: Color) -> Self {
        self.base_color = color;
        self
    }

    /// Whether to apply bold modifier at peak brightness
    pub fn bold_on_peak(mut self, bold: bool) -> Self {
        self.bold_on_peak = bold;
        self
    }

    /// Calculate brightness multiplier (0.4 to 1.0) based on frame
    /// Uses a sine wave for smooth breathing effect
    fn brightness_factor(&self) -> f32 {
        // 8 brightness levels, cycle every ~48 frames
        let phase = (self.frame / 6) % 8;
        // Sine-like wave: 0.4, 0.55, 0.7, 0.85, 1.0, 0.85, 0.7, 0.55
        match phase {
            0 => 0.4,
            1 => 0.55,
            2 => 0.7,
            3 => 0.85,
            4 => 1.0,
            5 => 0.85,
            6 => 0.7,
            7 => 0.55,
            _ => 0.7,
        }
    }

    /// Returns true if currently at peak brightness
    pub fn is_peak(&self) -> bool {
        (self.frame / 6) % 8 == 4
    }

    /// Get the adjusted color based on brightness
    fn adjusted_color(&self) -> Color {
        let factor = self.brightness_factor();

        match self.base_color {
            Color::Rgb(r, g, b) => Color::Rgb(
                (r as f32 * factor) as u8,
                (g as f32 * factor) as u8,
                (b as f32 * factor) as u8,
            ),
            // For non-RGB colors, we can't adjust brightness easily
            // so we just return the base color
            other => other,
        }
    }
}

impl Widget for PulseText {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let color = self.adjusted_color();
        let mut style = Style::default().fg(color);

        if self.bold_on_peak && self.is_peak() {
            style = style.add_modifier(Modifier::BOLD);
        }

        // Truncate text if needed
        let display = if self.text.len() > area.width as usize {
            format!("{}...", &self.text[..area.width as usize - 3])
        } else {
            self.text.clone()
        };

        buf.set_string(area.x, area.y, &display, style);
    }
}

/// Success celebration animation - displays a particle burst effect
pub struct ParticleBurst {
    /// Current animation frame (0-255)
    frame: u8,
    /// Center label text
    label: String,
    /// Color for particles
    color: Color,
}

impl ParticleBurst {
    /// Create a new particle burst centered on the given text
    pub fn new(label: impl Into<String>, frame: u8) -> Self {
        Self {
            frame,
            label: label.into(),
            color: Color::Rgb(34, 197, 94), // Green for success
        }
    }

    /// Set particle color
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Get particles for current frame
    /// Returns (offset_x, offset_y, char) tuples relative to center
    fn particles(&self) -> Vec<(i16, i16, char)> {
        let phase = (self.frame / 4) % 12;

        // Particle characters: sparkles expanding outward
        let _chars = ['✦', '✧', '★', '·', ' '];

        // Based on phase, particles expand outward then fade
        match phase {
            0 => vec![(0, 0, '✨')],
            1 => vec![(-1, 0, '✦'), (1, 0, '✦'), (0, -1, '✧'), (0, 0, '★')],
            2 => vec![
                (-2, 0, '✧'),
                (2, 0, '✧'),
                (0, -1, '✦'),
                (-1, -1, '·'),
                (1, -1, '·'),
            ],
            3 => vec![
                (-3, 0, '·'),
                (3, 0, '·'),
                (-2, -1, '✧'),
                (2, -1, '✧'),
                (-1, -1, '✦'),
                (1, -1, '✦'),
            ],
            4..=5 => vec![(-3, -1, '·'), (3, -1, '·'), (-2, -1, '·'), (2, -1, '·')],
            _ => vec![],
        }
    }
}

impl Widget for ParticleBurst {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 10 || area.height < 3 {
            // Just render the label if too small for animation
            let x = area.x + (area.width.saturating_sub(self.label.len() as u16)) / 2;
            buf.set_string(
                x,
                area.y + area.height / 2,
                &self.label,
                Style::default().fg(self.color).add_modifier(Modifier::BOLD),
            );
            return;
        }

        let center_x = area.x + area.width / 2;
        let center_y = area.y + area.height / 2;

        // Render particles
        for (dx, dy, ch) in self.particles() {
            let px = center_x as i16 + dx;
            let py = center_y as i16 + dy;

            if px >= area.x as i16
                && px < (area.x + area.width) as i16
                && py >= area.y as i16
                && py < (area.y + area.height) as i16
            {
                buf.set_string(
                    px as u16,
                    py as u16,
                    ch.to_string(),
                    Style::default().fg(self.color),
                );
            }
        }

        // Render the label in center
        let label_x = center_x.saturating_sub(self.label.len() as u16 / 2);
        buf.set_string(
            label_x,
            center_y,
            &self.label,
            Style::default().fg(self.color).add_modifier(Modifier::BOLD),
        );
    }
}

/// Failure shake animation - visual indication of error
pub struct ShakeText {
    /// Current animation frame (0-255)
    frame: u8,
    /// The text to display
    text: String,
    /// Error color
    color: Color,
    /// Shake intensity (1-3)
    intensity: u8,
}

impl ShakeText {
    /// Create a new shake text widget
    pub fn new(text: impl Into<String>, frame: u8) -> Self {
        Self {
            frame,
            text: text.into(),
            color: Color::Rgb(239, 68, 68), // Red for error
            intensity: 2,
        }
    }

    /// Set the color
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Set shake intensity (1-3)
    pub fn intensity(mut self, intensity: u8) -> Self {
        self.intensity = intensity.clamp(1, 3);
        self
    }

    /// Calculate horizontal offset for shake effect
    fn shake_offset(&self) -> i16 {
        let phase = (self.frame / 2) % 8;
        let max_offset = self.intensity as i16;

        // Quick back-and-forth motion that dampens
        match phase {
            0 => max_offset,
            1 => -max_offset,
            2 => max_offset - 1,
            3 => -(max_offset - 1),
            4 => 1,
            5 => -1,
            _ => 0,
        }
    }

    /// Returns true if shake animation is complete (one cycle)
    pub fn is_complete(&self) -> bool {
        (self.frame / 2) % 8 >= 6
    }
}

impl Widget for ShakeText {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let offset = self.shake_offset();
        let base_x = area.x as i16 + offset;

        // Ensure we stay within bounds
        let x = base_x.clamp(area.x as i16, (area.x + area.width - 1) as i16) as u16;

        // Truncate text if needed
        let max_len = area.width.saturating_sub(x - area.x) as usize;
        let display = if self.text.len() > max_len {
            format!("{}...", &self.text[..max_len.saturating_sub(3)])
        } else {
            self.text.clone()
        };

        let style = Style::default().fg(self.color).add_modifier(Modifier::BOLD);

        buf.set_string(x, area.y, &display, style);
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

    #[test]
    fn test_space_themed_spinners() {
        let rocket = Spinner::new(0).rocket();
        let stars = Spinner::new(0).stars();
        let orbit = Spinner::new(0).orbit();
        let cosmic = Spinner::new(0).cosmic();

        assert_eq!(rocket.current_char(), '🚀');
        assert_eq!(stars.current_char(), '✦');
        assert_eq!(orbit.current_char(), '◐');
        assert_eq!(cosmic.current_char(), '🌑');
    }

    #[test]
    fn test_rocket_spinner_cycles() {
        // Test that rocket spinner cycles through all frames
        let r0 = Spinner::new(0).rocket();
        let r1 = Spinner::new(6).rocket();
        let r2 = Spinner::new(12).rocket();

        assert_eq!(r0.current_char(), '🚀');
        assert_eq!(r1.current_char(), '🔥');
        assert_eq!(r2.current_char(), '✨');
    }

    #[test]
    fn test_stars_spinner_cycles() {
        let s0 = Spinner::new(0).stars();
        let s1 = Spinner::new(6).stars();
        let s2 = Spinner::new(12).stars();

        assert_eq!(s0.current_char(), '✦');
        assert_eq!(s1.current_char(), '✧');
        assert_eq!(s2.current_char(), '★');
    }

    #[test]
    fn test_pulse_text_brightness_varies() {
        let pt0 = PulseText::new("Test", 0);
        let pt1 = PulseText::new("Test", 24); // Peak brightness (frame 24 / 6 = 4)

        // At frame 0, brightness should be 0.4 (dimmest)
        assert!((pt0.brightness_factor() - 0.4).abs() < 0.01);

        // At frame 24, brightness should be 1.0 (peak)
        assert!((pt1.brightness_factor() - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_pulse_text_is_peak() {
        let not_peak = PulseText::new("Test", 0);
        let peak = PulseText::new("Test", 24);

        assert!(!not_peak.is_peak());
        assert!(peak.is_peak());
    }

    #[test]
    fn test_particle_burst_particles_expand() {
        let burst0 = ParticleBurst::new("Success!", 0);
        let burst1 = ParticleBurst::new("Success!", 4);

        let p0 = burst0.particles();
        let p1 = burst1.particles();

        // At frame 0, should have single center particle
        assert_eq!(p0.len(), 1);
        assert_eq!(p0[0].2, '✨');

        // At frame 4, should have more particles
        assert!(p1.len() > 1);
    }

    #[test]
    fn test_shake_text_offset_varies() {
        let shake0 = ShakeText::new("Error!", 0);
        let shake1 = ShakeText::new("Error!", 2);

        let off0 = shake0.shake_offset();
        let off1 = shake1.shake_offset();

        // Offset should change between frames
        assert_ne!(off0, off1);
    }

    #[test]
    fn test_shake_text_intensity() {
        let low = ShakeText::new("Error!", 0).intensity(1);
        let high = ShakeText::new("Error!", 0).intensity(3);

        // Higher intensity should have larger max offset
        assert!(low.shake_offset().abs() <= high.shake_offset().abs());
    }

    #[test]
    fn test_shake_text_is_complete() {
        let early = ShakeText::new("Error!", 0);
        let late = ShakeText::new("Error!", 14); // Frame 14 / 2 = 7, phase >= 6

        assert!(!early.is_complete());
        assert!(late.is_complete());
    }
}
