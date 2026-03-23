//! Matrix Rain Widget - Subtle animated background effect
//!
//! A lightweight Matrix-style rain effect using katakana and Nika mascots.
//! Designed to be subtle and fade out over time.
//!
//! ```text
//! ｱ   ｶ   ﾊ       ← falling katakana (sparse)
//!   ｲ     ﾋ  🦋
//! ｳ   ｷ       ﾗ   ← with rare Nika mascots
//! ```

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};

use crate::theme::solarized;

// ═══════════════════════════════════════════════════════════════════════════════
// CHARACTER SETS
// ═══════════════════════════════════════════════════════════════════════════════

/// Half-width Katakana (authentic Matrix style, monospace)
const KATAKANA_HALF: &[char] = &[
    'ｱ', 'ｲ', 'ｳ', 'ｴ', 'ｵ', 'ｶ', 'ｷ', 'ｸ', 'ｹ', 'ｺ', 'ｻ', 'ｼ', 'ｽ', 'ｾ', 'ｿ', 'ﾀ', 'ﾁ', 'ﾂ', 'ﾃ',
    'ﾄ', 'ﾅ', 'ﾆ', 'ﾇ', 'ﾈ', 'ﾉ', 'ﾊ', 'ﾋ', 'ﾌ', 'ﾍ', 'ﾎ', 'ﾏ', 'ﾐ', 'ﾑ', 'ﾒ', 'ﾓ', 'ﾔ', 'ﾕ', 'ﾖ',
    'ﾗ', 'ﾘ', 'ﾙ', 'ﾚ', 'ﾛ', 'ﾜ', 'ﾝ',
];

/// Full-width Katakana (more variety)
const KATAKANA_FULL: &[char] = &[
    'ア', 'イ', 'ウ', 'エ', 'オ', 'カ', 'キ', 'ク', 'ケ', 'コ', 'サ', 'シ', 'ス', 'セ', 'ソ', 'タ',
    'チ', 'ツ', 'テ', 'ト', 'ナ', 'ニ', 'ヌ', 'ネ', 'ノ', 'ハ', 'ヒ', 'フ', 'ヘ', 'ホ', 'マ', 'ミ',
    'ム', 'メ', 'モ', 'ヤ', 'ユ', 'ヨ', 'ラ', 'リ', 'ル', 'レ', 'ロ', 'ワ', 'ヲ', 'ン',
];

/// Hiragana (softer Japanese aesthetic)
const HIRAGANA: &[char] = &[
    'あ', 'い', 'う', 'え', 'お', 'か', 'き', 'く', 'け', 'こ', 'さ', 'し', 'す', 'せ', 'そ', 'た',
    'ち', 'つ', 'て', 'と', 'な', 'に', 'ぬ', 'ね', 'の', 'は', 'ひ', 'ふ', 'へ', 'ほ',
];

/// Hacker ASCII (code-like, terminal aesthetic)
const ASCII_HACKER: &[char] = &[
    '0', '1', '.', ':', '_', '-', '>', '<', '/', '\\', '|', '+', '*', '{', '}', '[', ']', '(', ')',
    '=', '#', '@', '$', '%', '^', '&', '~', '`', ';', '"', '\'', '!', '?',
];

/// Nika mascot emojis (6% of drops, brand identity)
/// 🦋 is 40% of emoji picks (dominant brand symbol)
const NIKA_MASCOTS: &[&str] = &[
    // === Nika Brand (40% = 20 entries out of ~50) ===
    "🦋",
    "🦋",
    "🦋",
    "🦋",
    "🦋",
    "🦋",
    "🦋",
    "🦋",
    "🦋",
    "🦋",
    "🦋",
    "🦋",
    "🦋",
    "🦋",
    "🦋",
    "🦋",
    "🦋",
    "🦋",
    "🦋",
    "🦋",
    // === Tech/Rust ===
    "🦀", // Rust crab
    "⚡", // Energy/async
    "✨", // Magic/AI
    "🔮", // Crystal ball (prediction)
    "💻", // Computer
    "🖥️", // Desktop
    "⌨️", // Keyboard
    // === Cosmic ===
    "🌌", // Galaxy
    "🪐", // Planet
    "💎", // Gem/quality
    "🌟", // Star
    "☄️", // Comet
    "🌙", // Moon
    "🔥", // Fire
    // === Animals ===
    "🐔", // Classic Nika
    "🦖", // T-Rex
    "🦕", // Bronto
    "🪼", // Jellyfish
    "🦙", // Llama (LLM!)
    "🐒", // Monkey
    "🦄", // Unicorn
    "🐯", // Tiger
    "🦁", // Lion
    "🦚", // Peacock
    "🦎", // Gecko
    "🦈", // Shark
    "🦞", // Lobster
    "🦑", // Squid
    "🐙", // Octopus
    "🦩", // Flamingo
    "🦜", // Parrot
    // === Special ===
    "🏝️", // Island
    "🗿", // Moai
    "🐦‍🔥", // Phoenix
    "🎭", // Theater masks
    "🎪", // Circus
    "👾", // Alien/retro game
    "🤖", // Robot
];

/// Rain colors (Solarized palette - vibrant multicolor)
const RAIN_COLORS: &[Color] = &[
    // === Primary Matrix colors (more frequent) ===
    solarized::CYAN,
    solarized::CYAN,
    solarized::GREEN,
    solarized::GREEN,
    // === Accent colors ===
    solarized::BLUE,
    solarized::VIOLET,
    solarized::MAGENTA,
    solarized::YELLOW,
    solarized::ORANGE,
    // === Muted (subtle background) ===
    solarized::BASE01,
    solarized::BASE00,
];

// ═══════════════════════════════════════════════════════════════════════════════
// RAIN DROP
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Clone)]
enum RainGlyph {
    Char(char),
    Emoji(&'static str),
}

impl RainGlyph {
    /// Write glyph to buffer at position. Avoids String allocation in render loop.
    fn write_to_buf(&self, buf: &mut Buffer, x: u16, y: u16, style: Style) {
        match self {
            RainGlyph::Char(c) => {
                // Use encode_utf8 to avoid heap allocation
                let mut char_buf = [0u8; 4];
                let s = c.encode_utf8(&mut char_buf);
                buf.set_string(x, y, s, style);
            }
            RainGlyph::Emoji(e) => {
                buf.set_string(x, y, *e, style);
            }
        }
    }

    fn width(&self) -> u16 {
        match self {
            RainGlyph::Char(_) => 1,
            RainGlyph::Emoji(_) => 2,
        }
    }
}

#[derive(Clone)]
struct RainDrop {
    y: i16,
    speed: u8,
    glyph: RainGlyph,
    color: Color,
    trail: u8,
}

// ═══════════════════════════════════════════════════════════════════════════════
// MATRIX RAIN WIDGET
// ═══════════════════════════════════════════════════════════════════════════════

/// Subtle Matrix Rain effect with fade-out support
pub struct MatrixRain {
    /// Animation frame counter (0-255)
    frame: u8,
    /// Density of rain drops (0.0 - 1.0), default very sparse
    density: f32,
    /// Opacity for fade effect (0.0 = invisible, 1.0 = full)
    opacity: f32,
    /// Whether to include Nika mascot emojis
    with_mascots: bool,
    /// Seed for reproducible randomness
    seed: u64,
}

impl Default for MatrixRain {
    fn default() -> Self {
        Self {
            frame: 0,
            density: 0.15, // Very sparse by default
            opacity: 1.0,
            with_mascots: true,
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
    /// Default is 0.15 (very sparse)
    pub fn density(mut self, density: f32) -> Self {
        self.density = density.clamp(0.0, 1.0);
        self
    }

    /// Set opacity for fade effect (0.0 = invisible, 1.0 = full)
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }

    /// Enable/disable Nika mascot emojis
    pub fn with_mascots(mut self, enable: bool) -> Self {
        self.with_mascots = enable;
        self
    }

    /// Alias for with_mascots
    pub fn with_emojis(self, enable: bool) -> Self {
        self.with_mascots(enable)
    }

    /// Set seed for reproducible randomness
    pub fn seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// Generate a random glyph (mixed Japanese + hacker ASCII + emojis)
    fn random_glyph(&self, rng: &mut SmallRng) -> RainGlyph {
        let roll: f32 = rng.gen();

        if self.with_mascots && roll < 0.06 {
            // 6% - Nika mascots (🦋 is 40% of these = dominant brand)
            let idx = rng.gen_range(0..NIKA_MASCOTS.len());
            RainGlyph::Emoji(NIKA_MASCOTS[idx])
        } else if roll < 0.40 {
            // 34% - Half-width Katakana (authentic Matrix)
            let idx = rng.gen_range(0..KATAKANA_HALF.len());
            RainGlyph::Char(KATAKANA_HALF[idx])
        } else if roll < 0.60 {
            // 20% - Full-width Katakana (variety)
            let idx = rng.gen_range(0..KATAKANA_FULL.len());
            RainGlyph::Char(KATAKANA_FULL[idx])
        } else if roll < 0.75 {
            // 15% - Hiragana (soft aesthetic)
            let idx = rng.gen_range(0..HIRAGANA.len());
            RainGlyph::Char(HIRAGANA[idx])
        } else {
            // 25% - Hacker ASCII (terminal vibe)
            let idx = rng.gen_range(0..ASCII_HACKER.len());
            RainGlyph::Char(ASCII_HACKER[idx])
        }
    }

    /// Generate rain drops for a column
    fn generate_drops(&self, col: u16, height: u16, rng: &mut SmallRng) -> Vec<RainDrop> {
        let mut drops = Vec::new();

        // Very few drops per column (sparse effect)
        let num_drops = ((height as f32 * self.density * 0.3) as usize).max(1);

        for i in 0..num_drops {
            // Stagger drops based on column and index
            // Safe casts: clamp u16 to i16 range to prevent wrapping
            let col_i16 = col.min(i16::MAX as u16) as i16;
            let i_i16 = (i as u16).min(i16::MAX as u16) as i16;
            let frame_i16 = self.frame as i16;
            let height_i16 = height.min(i16::MAX as u16) as i16;
            let base_y = (col_i16 * 7 + i_i16 * 17 + frame_i16 * 2) % (height_i16 * 2);
            let y = base_y - height_i16;

            let speed = rng.gen_range(1..=2); // Slower movement
            let color_idx = rng.gen_range(0..RAIN_COLORS.len());
            let trail = rng.gen_range(1..=3); // Short trails

            drops.push(RainDrop {
                y,
                speed,
                glyph: self.random_glyph(rng),
                color: RAIN_COLORS[color_idx],
                trail,
            });
        }

        drops
    }

    /// Apply opacity to a color
    fn apply_opacity(&self, color: Color, brightness: f32) -> Color {
        let effective = brightness * self.opacity;

        if effective < 0.1 {
            return solarized::BASE03; // Background (invisible)
        }

        match color {
            Color::Rgb(r, g, b) => Color::Rgb(
                (r as f32 * effective) as u8,
                (g as f32 * effective) as u8,
                (b as f32 * effective) as u8,
            ),
            c => {
                if effective < 0.5 {
                    solarized::BASE02
                } else {
                    c
                }
            }
        }
    }

}

impl Widget for MatrixRain {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 || self.opacity < 0.05 {
            return;
        }

        let seed = self.seed.wrapping_add(self.frame as u64);
        let mut rng = SmallRng::seed_from_u64(seed);

        // Process every 3rd column for sparse effect
        let mut col = 0u16;
        while col < area.width {
            let drops = self.generate_drops(col, area.height, &mut rng);

            for drop in drops {
                // Safe cast: drop.speed is u8 (1..=2), self.frame is u8 — no wrapping risk
                let current_y = drop.y + (self.frame as i16 / drop.speed as i16);

                // Render the drop and its short trail
                for trail_offset in 0..=drop.trail {
                    let y = current_y - trail_offset as i16;

                    // Safe cast: area.height is u16, clamp to i16 range
                    let height_i16 = area.height.min(i16::MAX as u16) as i16;
                    if y >= 0 && y < height_i16 {
                        let x = area.x + col;
                        let y_pos = area.y + y as u16;

                        if x < area.x + area.width && y_pos < area.y + area.height {
                            // Trail fades out
                            let brightness = if trail_offset == 0 {
                                0.8 // Head (not too bright)
                            } else {
                                0.4 - (trail_offset as f32 / drop.trail as f32) * 0.3
                            };

                            let color = self.apply_opacity(drop.color, brightness);

                            // Render glyph (zero-allocation path)
                            let glyph_width = drop.glyph.width();

                            if x + glyph_width <= area.x + area.width {
                                // Only render if not emoji or emoji fits
                                if matches!(drop.glyph, RainGlyph::Char(_)) || trail_offset == 0 {
                                    drop.glyph.write_to_buf(
                                        buf,
                                        x,
                                        y_pos,
                                        Style::default().fg(color),
                                    );
                                }
                            }
                        }
                    }
                }
            }

            col += 3; // Skip columns for sparse effect
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matrix_rain_new() {
        let rain = MatrixRain::new();
        assert_eq!(rain.frame, 0);
        assert!((rain.density - 0.15).abs() < 0.01);
        assert!((rain.opacity - 1.0).abs() < 0.01);
        assert!(rain.with_mascots);
    }

    #[test]
    fn test_matrix_rain_builder() {
        let rain = MatrixRain::new()
            .frame(42)
            .density(0.3)
            .opacity(0.5)
            .with_mascots(false)
            .seed(123);

        assert_eq!(rain.frame, 42);
        assert!((rain.density - 0.3).abs() < 0.01);
        assert!((rain.opacity - 0.5).abs() < 0.01);
        assert!(!rain.with_mascots);
        assert_eq!(rain.seed, 123);
    }

    #[test]
    fn test_density_clamping() {
        let rain = MatrixRain::new().density(2.0);
        assert!((rain.density - 1.0).abs() < 0.01);

        let rain = MatrixRain::new().density(-0.5);
        assert!((rain.density - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_opacity_clamping() {
        let rain = MatrixRain::new().opacity(1.5);
        assert!((rain.opacity - 1.0).abs() < 0.01);

        let rain = MatrixRain::new().opacity(-0.2);
        assert!((rain.opacity - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_render_empty_area() {
        let rain = MatrixRain::new();
        let area = Rect::new(0, 0, 0, 0);
        let mut buf = Buffer::empty(area);
        rain.render(area, &mut buf);
        // Should not panic
    }

    #[test]
    fn test_render_zero_opacity() {
        let rain = MatrixRain::new().opacity(0.0);
        let area = Rect::new(0, 0, 10, 5);
        let mut buf = Buffer::empty(area);
        rain.render(area, &mut buf);
        // Should skip rendering
    }

    #[test]
    fn test_nika_mascots_list() {
        // Verify all expected mascots are present
        assert!(NIKA_MASCOTS.contains(&"🦋"));
        assert!(NIKA_MASCOTS.contains(&"🦀"));
        assert!(NIKA_MASCOTS.contains(&"🐔"));
        assert!(NIKA_MASCOTS.contains(&"⚡"));
        assert!(NIKA_MASCOTS.contains(&"🪐"));
    }

    #[test]
    fn test_katakana_list() {
        // Verify katakana half-width chars are present
        assert!(KATAKANA_HALF.contains(&'ｱ'));
        assert!(KATAKANA_HALF.contains(&'ｶ'));
        assert!(KATAKANA_HALF.contains(&'ﾝ'));
        assert_eq!(KATAKANA_HALF.len(), 45); // Full half-width set
    }

}
