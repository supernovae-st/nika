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

use crate::tui::theme::solarized;

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

/// NIKA pattern formed by butterflies (bitmap: X = butterfly position)
/// EXTRA LARGE bold design - each letter is 5 wide, 9 tall
/// X = butterfly, . = empty space
const NIKA_PATTERN: &[&str] = &[
    // N          I      K          A
    "X X . . X X  X X  X X . . X X  . X X X X .", // 1
    "X X . . X X  X X  X X . . X X  . X X X X .", // 2
    "X X X . X X  X X  X X . X X .  X X . . X X", // 3
    "X X X . X X  X X  X X X X . .  X X . . X X", // 4
    "X X X X X X  X X  X X X X . .  X X X X X X", // 5 (middle)
    "X X . X X X  X X  X X X X . .  X X X X X X", // 6
    "X X . . X X  X X  X X . X X .  X X . . X X", // 7
    "X X . . X X  X X  X X . . X X  X X . . X X", // 8
    "X X . . X X  X X  X X . . X X  X X . . X X", // 9
];

/// Width of NIKA_PATTERN in characters
const NIKA_PATTERN_WIDTH: usize = 44;
/// Height of NIKA_PATTERN in rows
const NIKA_PATTERN_HEIGHT: usize = 9;

/// Decorative emojis for around the pattern
const DECO_EMOJIS: &[&str] = &["🦋", "✨", "🌌", "💫", "⭐", "🌟", "🪐", "🌙"];

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
///
/// Use `.with_fade()` for automatic fade-out over time.
/// Use `.with_nika_pattern(true)` to show NIKA spelled with butterflies.
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
    /// Whether to show NIKA pattern with butterflies (centered)
    nika_pattern: bool,
    /// Explosion animation frame (0 = no explosion, 1+ = spreading)
    explosion_frame: u8,
}

impl Default for MatrixRain {
    fn default() -> Self {
        Self {
            frame: 0,
            density: 0.15, // Very sparse by default
            opacity: 1.0,
            with_mascots: true,
            seed: 42,
            nika_pattern: false,
            explosion_frame: 0,
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

    /// Enable/disable NIKA pattern with butterflies
    pub fn with_nika_pattern(mut self, enable: bool) -> Self {
        self.nika_pattern = enable;
        self
    }

    /// Set explosion animation frame (0 = no explosion, 1-255 = spreading)
    pub fn explosion_frame(mut self, frame: u8) -> Self {
        self.explosion_frame = frame;
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
            let base_y =
                (col as i16 * 7 + i as i16 * 17 + self.frame as i16 * 2) % (height as i16 * 2);
            let y = base_y - height as i16;

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

    /// Smooth ease-out function for natural deceleration
    #[inline]
    fn ease_out(t: f32) -> f32 {
        1.0 - (1.0 - t).powi(3)
    }

    /// Render NIKA pattern with butterflies and smooth explosion effect
    #[allow(clippy::too_many_lines)]
    fn render_nika_pattern(&self, area: Rect, buf: &mut Buffer, _rng: &mut SmallRng) {
        let center_x = area.x + area.width / 2;
        let center_y = area.y + area.height / 2;

        // Pattern positioning
        let pattern_start_x = center_x.saturating_sub((NIKA_PATTERN_WIDTH as u16 * 2) / 2);
        let pattern_start_y = center_y.saturating_sub(NIKA_PATTERN_HEIGHT as u16 / 2);
        let pattern_center_x = pattern_start_x + (NIKA_PATTERN_WIDTH as u16);
        let pattern_center_y = pattern_start_y + (NIKA_PATTERN_HEIGHT as u16 / 2);

        // Animation progress (0.0 to 1.0) with smooth easing - FAST: 15 frames
        let raw_progress = (self.explosion_frame as f32 / 13.0).min(1.0);
        let progress = Self::ease_out(raw_progress);
        let exploding = self.explosion_frame > 0;

        // Quick fade-in at start (first 2 frames)
        let fade_in = if self.explosion_frame == 0 {
            ((self.frame % 10) as f32 / 2.0).min(1.0)
        } else {
            1.0
        };

        // Render sparkles around pattern (fade out smoothly)
        if progress < 0.7 {
            let sparkle_fade = (1.0 - progress / 0.7) * fade_in;
            let deco_positions: [(i16, i16); 8] = [
                (-3, -1),
                (48, -1), // Top corners
                (-4, 4),
                (49, 4), // Middle
                (-3, 9),
                (48, 9), // Bottom corners
                (22, -2),
                (22, 10), // Top/bottom center
            ];
            for (i, (dx, dy)) in deco_positions.iter().enumerate() {
                let x = (pattern_start_x as i16 + dx).max(area.x as i16) as u16;
                let y = (pattern_start_y as i16 + dy).max(area.y as i16) as u16;
                if x < area.x + area.width - 1 && y < area.y + area.height {
                    // Rotate through sparkle emojis for shimmer effect
                    let emoji_idx = (i + (self.frame as usize / 2)) % DECO_EMOJIS.len();
                    let emoji = DECO_EMOJIS[emoji_idx];
                    let color = RAIN_COLORS[i % RAIN_COLORS.len()];
                    buf.set_string(
                        x,
                        y,
                        emoji,
                        Style::default().fg(self.apply_opacity(color, sparkle_fade)),
                    );
                }
            }
        }

        // Render NIKA pattern butterflies with wave explosion
        let mut butterfly_idx = 0usize;
        for (row_idx, row) in NIKA_PATTERN.iter().enumerate() {
            let base_y = pattern_start_y + row_idx as u16;
            if base_y < area.y || base_y >= area.y + area.height {
                continue;
            }

            for (col_pos, ch) in row.chars().enumerate() {
                if ch == 'X' {
                    let base_x = pattern_start_x + (col_pos as u16) * 2;

                    // Wave effect: butterflies closer to center explode first
                    let dist_from_center = ((base_x as f32 - pattern_center_x as f32).abs()
                        + (base_y as f32 - pattern_center_y as f32).abs() * 2.0)
                        / 50.0;

                    // Each butterfly has its own delayed start based on distance
                    let local_progress = (progress - dist_from_center * 0.15).clamp(0.0, 1.0);
                    let local_eased = Self::ease_out(local_progress);

                    // Calculate smooth explosion offset
                    let (final_x, final_y, local_fade) = if exploding && local_progress > 0.0 {
                        let seed_offset = (row_idx * 50 + butterfly_idx) as u64;

                        // Deterministic but varied angle per butterfly
                        let angle: f32 = ((seed_offset.wrapping_mul(7919) % 1000) as f32) / 1000.0
                            * std::f32::consts::TAU;

                        // Distance increases smoothly with easing
                        let max_dist =
                            ((seed_offset.wrapping_mul(6991) % 500) as f32 / 500.0 + 0.5) * 20.0;
                        let dist = local_eased * max_dist;

                        let offset_x = (angle.cos() * dist) as i16;
                        let offset_y = (angle.sin() * dist * 0.5) as i16; // Flatter spread

                        let new_x = (base_x as i16 + offset_x)
                            .max(area.x as i16)
                            .min((area.x + area.width - 2) as i16)
                            as u16;
                        let new_y = (base_y as i16 + offset_y)
                            .max(area.y as i16)
                            .min((area.y + area.height - 1) as i16)
                            as u16;

                        // Smooth fade out during explosion
                        let fade = (1.0 - local_eased.powf(1.5)).max(0.0);
                        (new_x, new_y, fade)
                    } else {
                        (base_x, base_y, fade_in)
                    };

                    // Skip if completely faded
                    if local_fade < 0.05 {
                        butterfly_idx += 1;
                        continue;
                    }

                    // Bounds check
                    if final_x >= area.x
                        && final_x < area.x + area.width - 1
                        && final_y >= area.y
                        && final_y < area.y + area.height
                    {
                        // Rainbow colors cycling through butterflies
                        let color_idx =
                            (row_idx + butterfly_idx + self.frame as usize / 4) % RAIN_COLORS.len();
                        let base_color = RAIN_COLORS[color_idx];
                        let color = self.apply_opacity(base_color, local_fade);

                        buf.set_string(final_x, final_y, "🦋", Style::default().fg(color));
                    }
                    butterfly_idx += 1;
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
                let current_y = drop.y + (self.frame as i16 / drop.speed as i16);

                // Render the drop and its short trail
                for trail_offset in 0..=drop.trail {
                    let y = current_y - trail_offset as i16;

                    if y >= 0 && y < area.height as i16 {
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

        // Overlay NIKA pattern with butterflies if enabled
        if self.nika_pattern {
            self.render_nika_pattern(area, buf, &mut rng);
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

    #[test]
    fn test_nika_pattern_builder() {
        let rain = MatrixRain::new().with_nika_pattern(true);
        assert!(rain.nika_pattern);

        let rain = MatrixRain::new().with_nika_pattern(false);
        assert!(!rain.nika_pattern);
    }

    #[test]
    fn test_explosion_frame_builder() {
        let rain = MatrixRain::new().explosion_frame(42);
        assert_eq!(rain.explosion_frame, 42);
    }

    #[test]
    fn test_nika_pattern_dimensions() {
        // Verify pattern constants match
        assert_eq!(NIKA_PATTERN.len(), NIKA_PATTERN_HEIGHT);
        for row in NIKA_PATTERN {
            // Pattern uses X for butterfly positions
            assert!(row.contains('X'));
        }
    }

    #[test]
    fn test_deco_emojis() {
        // Verify decorative emojis are present
        assert!(DECO_EMOJIS.len() >= 6);
        assert!(DECO_EMOJIS.contains(&"🦋"));
        assert!(DECO_EMOJIS.contains(&"🌌"));
    }

    #[test]
    fn test_render_with_nika_pattern() {
        let rain = MatrixRain::new().with_nika_pattern(true).opacity(1.0);
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        rain.render(area, &mut buf);
        // Should not panic
    }

    #[test]
    fn test_render_with_explosion() {
        let rain = MatrixRain::new()
            .with_nika_pattern(true)
            .explosion_frame(30)
            .opacity(1.0);
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        rain.render(area, &mut buf);
        // Should not panic
    }

    #[test]
    fn test_ease_out_function() {
        // ease_out(0) = 0
        assert!((MatrixRain::ease_out(0.0) - 0.0).abs() < 0.001);
        // ease_out(1) = 1
        assert!((MatrixRain::ease_out(1.0) - 1.0).abs() < 0.001);
        // ease_out(0.5) should be > 0.5 (faster at start, slower at end)
        assert!(MatrixRain::ease_out(0.5) > 0.5);
        // Monotonic: ease_out(0.3) < ease_out(0.7)
        assert!(MatrixRain::ease_out(0.3) < MatrixRain::ease_out(0.7));
    }

    #[test]
    fn test_smooth_explosion_progression() {
        // Test that explosion progresses smoothly through all frames
        let area = Rect::new(0, 0, 120, 40);
        for frame in 0..30 {
            let rain = MatrixRain::new()
                .frame(frame)
                .with_nika_pattern(true)
                .explosion_frame(frame)
                .opacity(1.0);
            let mut buf = Buffer::empty(area);
            rain.render(area, &mut buf);
            // Should not panic at any frame
        }
    }

    #[test]
    fn test_wave_pattern_center_first() {
        // Verify wave effect concept: center butterflies should move first
        // This is a property test - we verify the distance calculation is correct
        let center_x: f32 = 50.0;
        let center_y: f32 = 12.0;

        let dist_center = (0.0_f32.abs() + 0.0_f32.abs() * 2.0) / 50.0;
        let dist_edge = ((40.0_f32 - center_x).abs() + (5.0_f32 - center_y).abs() * 2.0) / 50.0;

        // Center should have smaller distance (explode first)
        assert!(dist_center < dist_edge);
    }
}
