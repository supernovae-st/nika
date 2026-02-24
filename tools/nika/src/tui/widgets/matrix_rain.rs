//! Matrix Rain Widget - Dithered emoji/ASCII cascade effect
//!
//! Creates a satisfying visual effect with random characters, emojis,
//! and colors cascading down the screen like the Matrix.
//!
//! ```text
//! ✨ n 🧠 _ a 🔥 j _ 🦙 p ★ 7 💫 _
//! 🐔 < _ o 2 h q 🧀 _ p 🍕 4 4 5 d
//! ⚡ e c j _ _ o 2 h q m 🔥 _ p [ _
//! ```

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};

use crate::tui::theme::solarized;

/// Characters to use in the matrix effect (ASCII + special)
const MATRIX_CHARS: &[char] = &[
    // ASCII lowercase
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's',
    't', 'u', 'v', 'w', 'x', 'y', 'z', // Numbers
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', // Special ASCII
    '_', '<', '>', '[', ']', '{', '}', '/', '\\', '*', '#', '@', '^', '-', '+', '=', '~', '|', '.',
    ',', ':', ';',
];

/// Emojis to sprinkle in the matrix (high impact visual)
const MATRIX_EMOJIS: &[&str] = &[
    // Tech/AI
    "🧠", "🤖", "⚡", "💫", "✨", "🔥", "⭐", "★", // Animals (Nika mascots)
    "🐔", "🐤", "🦙", "🐭", // Food (fun)
    "🧀", "🍕", "🌶️", // Symbols
    "❤️", "💜", "💛", "🔮", "💎", "🎯", "🚀", // Nature
    "🌸", "🌟", "✦", "✧",
];

/// Matrix rain colors (Solarized accents)
const MATRIX_COLORS: &[Color] = &[
    solarized::CYAN,
    solarized::GREEN,
    solarized::YELLOW,
    solarized::ORANGE,
    solarized::MAGENTA,
    solarized::VIOLET,
    solarized::BLUE,
    solarized::RED,
];

/// A single drop in the matrix rain
#[derive(Clone)]
struct RainDrop {
    /// Current row position
    y: i16,
    /// Speed (rows per frame)
    speed: u8,
    /// Character/emoji at this position
    glyph: RainGlyph,
    /// Color for this drop
    color: Color,
    /// Trail length
    trail: u8,
}

#[derive(Clone)]
enum RainGlyph {
    Char(char),
    Emoji(&'static str),
}

impl RainGlyph {
    fn as_str(&self) -> String {
        match self {
            RainGlyph::Char(c) => c.to_string(),
            RainGlyph::Emoji(e) => e.to_string(),
        }
    }

    fn width(&self) -> u16 {
        match self {
            RainGlyph::Char(_) => 1,
            RainGlyph::Emoji(_) => 2, // Most emojis are 2 cells wide
        }
    }
}

/// Matrix Rain effect widget
///
/// Renders a cascading rain of characters and emojis with Solarized colors.
pub struct MatrixRain {
    /// Animation frame counter (0-255)
    frame: u8,
    /// Density of rain drops (0.0 - 1.0)
    density: f32,
    /// Whether to include emojis
    with_emojis: bool,
    /// Seed for reproducible randomness
    seed: u64,
}

impl Default for MatrixRain {
    fn default() -> Self {
        Self {
            frame: 0,
            density: 0.3,
            with_emojis: true,
            seed: 42,
        }
    }
}

impl MatrixRain {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set animation frame (0-255)
    pub fn frame(mut self, frame: u8) -> Self {
        self.frame = frame;
        self
    }

    /// Set rain density (0.0 = sparse, 1.0 = dense)
    pub fn density(mut self, density: f32) -> Self {
        self.density = density.clamp(0.0, 1.0);
        self
    }

    /// Enable/disable emojis
    pub fn with_emojis(mut self, enable: bool) -> Self {
        self.with_emojis = enable;
        self
    }

    /// Set seed for reproducible randomness
    pub fn seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// Generate a random glyph
    fn random_glyph(&self, rng: &mut SmallRng) -> RainGlyph {
        // 20% chance for emoji if enabled
        if self.with_emojis && rng.gen_bool(0.2) {
            let idx = rng.gen_range(0..MATRIX_EMOJIS.len());
            RainGlyph::Emoji(MATRIX_EMOJIS[idx])
        } else {
            let idx = rng.gen_range(0..MATRIX_CHARS.len());
            RainGlyph::Char(MATRIX_CHARS[idx])
        }
    }

    /// Generate rain drops for a column
    fn generate_drops(&self, col: u16, height: u16, rng: &mut SmallRng) -> Vec<RainDrop> {
        let mut drops = Vec::new();
        let num_drops = (height as f32 * self.density * 0.5) as usize;

        for i in 0..num_drops.max(1) {
            // Stagger drops based on column and index
            let base_y =
                (col as i16 * 7 + i as i16 * 13 + self.frame as i16 * 2) % (height as i16 * 2);
            let y = base_y - height as i16; // Start above visible area

            let speed = rng.gen_range(1..=3);
            let color_idx = rng.gen_range(0..MATRIX_COLORS.len());
            let trail = rng.gen_range(2..=6);

            drops.push(RainDrop {
                y,
                speed,
                glyph: self.random_glyph(rng),
                color: MATRIX_COLORS[color_idx],
                trail,
            });
        }

        drops
    }
}

impl Widget for MatrixRain {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        // Create seeded RNG for reproducible but animated results
        let seed = self.seed.wrapping_add(self.frame as u64);
        let mut rng = SmallRng::seed_from_u64(seed);

        // Process each column
        let mut col = 0u16;
        while col < area.width {
            // Generate drops for this column
            let drops = self.generate_drops(col, area.height, &mut rng);

            for drop in drops {
                // Calculate current position based on frame
                let current_y = drop.y + (self.frame as i16 / drop.speed as i16);

                // Render the drop and its trail
                for trail_offset in 0..=drop.trail {
                    let y = current_y - trail_offset as i16;

                    if y >= 0 && y < area.height as i16 {
                        let x = area.x + col;
                        let y = area.y + y as u16;

                        // Check bounds
                        if x < area.x + area.width && y < area.y + area.height {
                            // Trail fades out
                            let brightness = if trail_offset == 0 {
                                1.0 // Head is brightest
                            } else {
                                1.0 - (trail_offset as f32 / drop.trail as f32) * 0.7
                            };

                            // Get color with fading
                            let color = if brightness > 0.8 {
                                drop.color
                            } else if brightness > 0.5 {
                                // Dim version
                                match drop.color {
                                    Color::Rgb(r, g, b) => Color::Rgb(
                                        (r as f32 * brightness) as u8,
                                        (g as f32 * brightness) as u8,
                                        (b as f32 * brightness) as u8,
                                    ),
                                    c => c,
                                }
                            } else {
                                solarized::BASE01 // Very dim
                            };

                            // Generate glyph for this position (changes per frame for effect)
                            let glyph = if trail_offset == 0 {
                                drop.glyph.clone()
                            } else {
                                // Trail uses random chars
                                RainGlyph::Char(MATRIX_CHARS[rng.gen_range(0..MATRIX_CHARS.len())])
                            };

                            if let Some(cell) = buf.cell_mut((x, y)) {
                                cell.set_symbol(&glyph.as_str());
                                cell.set_style(Style::default().fg(color));
                            }
                        }
                    }
                }
            }

            // Skip width based on potential emoji
            col += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn test_matrix_rain_renders() {
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let rain = MatrixRain::new().frame(0).density(0.5).with_emojis(true);
                frame.render_widget(rain, frame.area());
            })
            .unwrap();

        // Just verify it doesn't panic
    }

    #[test]
    fn test_matrix_rain_animation() {
        let backend = TestBackend::new(20, 5);
        let mut terminal = Terminal::new(backend).unwrap();

        // Render multiple frames
        for frame in 0..10 {
            terminal
                .draw(|f| {
                    let rain = MatrixRain::new().frame(frame);
                    f.render_widget(rain, f.area());
                })
                .unwrap();
        }
    }

    #[test]
    fn test_matrix_rain_density() {
        let rain_sparse = MatrixRain::new().density(0.1);
        let rain_dense = MatrixRain::new().density(0.9);

        // Verify density is clamped
        let rain_over = MatrixRain::new().density(2.0);
        assert!(rain_over.density <= 1.0);

        let rain_under = MatrixRain::new().density(-1.0);
        assert!(rain_under.density >= 0.0);
    }

    #[test]
    fn test_rain_glyph_width() {
        let char_glyph = RainGlyph::Char('a');
        assert_eq!(char_glyph.width(), 1);

        let emoji_glyph = RainGlyph::Emoji("🧠");
        assert_eq!(emoji_glyph.width(), 2);
    }
}
