//! Matrix Rain Reveal Widget - Streaming text with falling rain effect
//!
//! Characters fall from above like Matrix rain, land with sparks,
//! and reveal the actual text in a left-to-right wave.
//!
//! ```text
//!      ｱ    ｶ    ﾊ           ← rain falling
//!      ｲ    ｷ    ﾋ    ﾗ
//!      ▼    ▼    ▼    ▼
//! ─────────────────────────
//! Hello world, how ｱｲｳ ｶｷ   ← text with chaos + reveal
//! ```

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Widget,
};

use crate::tui::theme::solarized;

// ═══════════════════════════════════════════════════════════════════════════════
// CONSTANTS
// ═══════════════════════════════════════════════════════════════════════════════

/// Half-width Katakana (monospace, Matrix style)
const KATAKANA: &[char] = &[
    'ｱ', 'ｲ', 'ｳ', 'ｴ', 'ｵ', 'ｶ', 'ｷ', 'ｸ', 'ｹ', 'ｺ', 'ｻ', 'ｼ', 'ｽ', 'ｾ', 'ｿ', 'ﾀ', 'ﾁ', 'ﾂ', 'ﾃ',
    'ﾄ', 'ﾅ', 'ﾆ', 'ﾇ', 'ﾈ', 'ﾉ', 'ﾊ', 'ﾋ', 'ﾌ', 'ﾍ', 'ﾎ', 'ﾏ', 'ﾐ', 'ﾑ', 'ﾒ', 'ﾓ', 'ﾔ', 'ﾕ', 'ﾖ',
    'ﾗ', 'ﾘ', 'ﾙ', 'ﾚ', 'ﾛ', 'ﾜ', 'ﾝ',
];

/// ASCII characters
const ASCII: &[char] = &[
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's',
    't', 'u', 'v', 'w', 'x', 'y', 'z', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', '<', '>',
    '[', ']', '{', '}', '/', '*', '#', '@',
];

/// Kanji characters (Nika flavor)
const KANJI: &[char] = &[
    '自', '由', 'ニ', 'カ', '海', '賊', '風', '雷', '火', '水', '光', '闇', '星', '月', '龍', '虎',
];

/// Nerd references
const NERD_REFS: &[&str] = &[
    "42", "EOF", "NULL", "sudo", "git", "fn", "async", "OK", "ACK", "SYN", "NaN", "λ", "π", "Ω",
];

/// Nika mascot emojis (very rare)
const NIKA_MASCOTS: &[&str] = &["🐔", "⚡", "📟", "🛰️", "🔌", "🦋"];

/// Spark characters for impact effect
const SPARK_CHARS: &[char] = &['·', '˚', '*', '✦'];

/// Rain colors (Solarized)
const RAIN_COLORS: &[Color] = &[
    solarized::CYAN,
    solarized::GREEN,
    solarized::YELLOW,
    solarized::ORANGE,
    solarized::MAGENTA,
    solarized::VIOLET,
];

/// Spark colors
const SPARK_COLORS: &[Color] = &[
    solarized::YELLOW,
    solarized::BASE3, // white
];

/// Revealed text color
const REVEALED_COLOR: Color = solarized::BASE3; // white

/// Generate random glyph for rain (free function to avoid borrow issues)
fn random_glyph(rng: &mut SmallRng) -> RainGlyph {
    let roll: f32 = rng.gen();

    if roll < 0.45 {
        // 45% katakana
        let idx = rng.gen_range(0..KATAKANA.len());
        RainGlyph::Char(KATAKANA[idx])
    } else if roll < 0.80 {
        // 35% ASCII
        let idx = rng.gen_range(0..ASCII.len());
        RainGlyph::Char(ASCII[idx])
    } else if roll < 0.90 {
        // 10% Kanji
        let idx = rng.gen_range(0..KANJI.len());
        RainGlyph::Char(KANJI[idx])
    } else if roll < 0.98 {
        // 8% Nerd refs
        let idx = rng.gen_range(0..NERD_REFS.len());
        RainGlyph::Text(NERD_REFS[idx].to_string())
    } else {
        // 2% Nika mascots
        let idx = rng.gen_range(0..NIKA_MASCOTS.len());
        RainGlyph::Text(NIKA_MASCOTS[idx].to_string())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// RAIN DROP
// ═══════════════════════════════════════════════════════════════════════════════

/// A single falling character in the rain
#[derive(Clone, Debug)]
struct RainDrop {
    /// Character to display (single char or first char of string)
    glyph: RainGlyph,
    /// Color of this drop
    color: Color,
    /// Vertical position (0.0 = top of rain zone, 1.0 = landed)
    y: f32,
    /// Fall speed (units per tick)
    speed: f32,
    /// Has this drop landed?
    landed: bool,
}

#[derive(Clone, Debug)]
enum RainGlyph {
    Char(char),
    Text(String),
}

impl RainGlyph {
    fn as_str(&self) -> &str {
        match self {
            RainGlyph::Char(c) => {
                // This is a bit of a hack but works for single chars
                // We'll handle this properly in render
                unsafe {
                    std::str::from_utf8_unchecked(std::slice::from_raw_parts(
                        c as *const char as *const u8,
                        c.len_utf8(),
                    ))
                }
            }
            RainGlyph::Text(s) => s,
        }
    }

    fn display(&self) -> String {
        match self {
            RainGlyph::Char(c) => c.to_string(),
            RainGlyph::Text(s) => s.clone(),
        }
    }
}

impl RainDrop {
    fn new(glyph: RainGlyph, color: Color, speed: f32) -> Self {
        Self {
            glyph,
            color,
            y: 0.0,
            speed,
            landed: false,
        }
    }

    /// Tick the drop, returns true if just landed
    fn tick(&mut self) -> bool {
        if self.landed {
            return false;
        }
        self.y += self.speed;
        if self.y >= 1.0 {
            self.y = 1.0;
            self.landed = true;
            return true;
        }
        false
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// SPARK
// ═══════════════════════════════════════════════════════════════════════════════

/// Impact spark particle
#[derive(Clone, Debug)]
struct Spark {
    /// Character to display
    char: char,
    /// X position (column)
    x: usize,
    /// Y offset from text line (-1 = above, 0 = on line, 1 = below)
    y_offset: i8,
    /// Frames remaining before disappearing
    life: u8,
    /// Color
    color: Color,
}

impl Spark {
    fn new(x: usize, rng: &mut SmallRng) -> Self {
        let char = SPARK_CHARS[rng.gen_range(0..SPARK_CHARS.len())];
        let color = SPARK_COLORS[rng.gen_range(0..SPARK_COLORS.len())];
        let y_offset = if rng.gen_bool(0.7) { -1 } else { 0 }; // mostly above
        Self {
            char,
            x,
            y_offset,
            life: rng.gen_range(2..=4),
            color,
        }
    }

    /// Tick the spark, returns true if still alive
    fn tick(&mut self) -> bool {
        if self.life > 0 {
            self.life -= 1;
            true
        } else {
            false
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// TEXT COLUMN STATE
// ═══════════════════════════════════════════════════════════════════════════════

/// State for a single text column
#[derive(Clone, Debug)]
struct TextColumn {
    /// The actual character to reveal (None for space)
    target_char: Option<char>,
    /// Rain drops falling in this column
    drops: Vec<RainDrop>,
    /// Is this column revealed (showing white text)?
    revealed: bool,
    /// Current chaos character (when not revealed)
    chaos_char: Option<RainGlyph>,
    /// Color for chaos char
    chaos_color: Color,
}

impl TextColumn {
    fn new_space() -> Self {
        Self {
            target_char: None,
            drops: Vec::new(),
            revealed: true, // spaces are always "revealed"
            chaos_char: None,
            chaos_color: solarized::BASE01,
        }
    }

    fn new_char(c: char) -> Self {
        Self {
            target_char: Some(c),
            drops: Vec::new(),
            revealed: false,
            chaos_char: None,
            chaos_color: solarized::CYAN,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// MATRIX RAIN REVEAL
// ═══════════════════════════════════════════════════════════════════════════════

/// Main widget for Matrix rain streaming reveal effect
#[derive(Clone, Debug)]
pub struct MatrixRainReveal {
    /// Text columns (one per character position)
    columns: Vec<TextColumn>,
    /// Active sparks
    sparks: Vec<Spark>,
    /// Number of rain lines above text
    rain_height: u8,
    /// Current animation frame
    frame: u64,
    /// RNG seed
    seed: u64,
    /// Characters revealed so far (wave position)
    revealed_count: usize,
    /// Frames since last reveal tick
    reveal_accumulator: f32,
    /// Reveal speed (chars per frame, can be fractional)
    reveal_speed: f32,
    /// Wave delay (chars behind incoming text)
    wave_delay: usize,
}

impl Default for MatrixRainReveal {
    fn default() -> Self {
        Self::new()
    }
}

impl MatrixRainReveal {
    pub fn new() -> Self {
        Self {
            columns: Vec::new(),
            sparks: Vec::new(),
            rain_height: 3,
            frame: 0,
            seed: 42,
            revealed_count: 0,
            reveal_accumulator: 0.0,
            reveal_speed: 0.5, // ~2 frames per char reveal
            wave_delay: 8,     // reveal starts 8 chars behind
        }
    }

    /// Set rain height (lines above text)
    pub fn with_rain_height(mut self, height: u8) -> Self {
        self.rain_height = height.clamp(2, 6);
        self
    }

    /// Set reveal speed
    pub fn with_reveal_speed(mut self, speed: f32) -> Self {
        self.reveal_speed = speed.clamp(0.1, 2.0);
        self
    }

    /// Set wave delay
    pub fn with_wave_delay(mut self, delay: usize) -> Self {
        self.wave_delay = delay;
        self
    }

    /// Set seed for reproducible randomness
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// Set verb (for API compatibility - rain uses Solarized colors regardless)
    #[allow(unused_variables)]
    pub fn with_verb(self, verb: super::DecryptVerb) -> Self {
        // Rain effect uses Solarized colors, verb theming is ignored
        self
    }

    /// Set delay (for API compatibility - use with_reveal_speed instead)
    #[allow(unused_variables)]
    pub fn with_delay(self, delay: usize) -> Self {
        // For rain effect, delay is handled differently
        self
    }

    /// Set decay (for API compatibility - rain uses tick-based animation)
    #[allow(unused_variables)]
    pub fn with_decay(self, decay: f32) -> Self {
        // For rain effect, decay is handled by tick() animation
        self
    }

    /// Clear all state
    pub fn clear(&mut self) {
        self.columns.clear();
        self.sparks.clear();
        self.revealed_count = 0;
        self.reveal_accumulator = 0.0;
        self.frame = 0;
    }

    /// Push new text (streaming)
    pub fn push_text(&mut self, text: &str) {
        let mut rng = SmallRng::seed_from_u64(self.seed.wrapping_add(self.frame));

        for c in text.chars() {
            if c == ' ' || c == '\n' {
                self.columns.push(TextColumn::new_space());
            } else {
                let mut col = TextColumn::new_char(c);
                // Generate initial rain drops for this column
                self.spawn_rain_for_column(&mut col, &mut rng);
                self.columns.push(col);
            }
        }
    }

    /// Spawn rain drops for a column
    fn spawn_rain_for_column(&self, col: &mut TextColumn, rng: &mut SmallRng) {
        let num_drops = rng.gen_range(3..=5);
        for i in 0..num_drops {
            let glyph = random_glyph(rng);
            let color = RAIN_COLORS[rng.gen_range(0..RAIN_COLORS.len())];
            // Stagger drops vertically
            let start_y = -(i as f32 * 0.3);
            let speed = rng.gen_range(0.15..0.35);
            let mut drop = RainDrop::new(glyph, color, speed);
            drop.y = start_y;
            col.drops.push(drop);
        }
        // Set initial chaos char
        col.chaos_char = Some(random_glyph(rng));
        col.chaos_color = RAIN_COLORS[rng.gen_range(0..RAIN_COLORS.len())];
    }

    /// Tick animation (call every frame)
    pub fn tick(&mut self) {
        self.frame += 1;
        let mut rng = SmallRng::seed_from_u64(self.seed.wrapping_add(self.frame * 31337));

        // Collect spark spawns and chaos updates first (to avoid borrow issues)
        let mut new_sparks: Vec<usize> = Vec::new();
        let mut chaos_updates: Vec<(usize, RainGlyph, Color)> = Vec::new();

        // First pass: tick drops and collect updates
        for (col_idx, col) in self.columns.iter_mut().enumerate() {
            if col.target_char.is_none() {
                continue; // Skip spaces
            }

            for drop in &mut col.drops {
                if drop.tick() {
                    // Just landed - mark for spark spawn
                    new_sparks.push(col_idx);
                }
            }

            // Remove landed drops
            col.drops.retain(|d| !d.landed);

            // Mark for chaos update (flicker effect)
            if !col.revealed && rng.gen_bool(0.3) {
                // We'll generate the glyph in the second pass
                chaos_updates.push((col_idx, RainGlyph::Char('?'), solarized::CYAN));
            }
        }

        // Second pass: spawn sparks
        for col_idx in new_sparks {
            self.sparks.push(Spark::new(col_idx, &mut rng));
        }

        // Third pass: update chaos chars (now we can borrow self immutably for random_glyph)
        for (col_idx, _, _) in chaos_updates {
            let glyph = random_glyph(&mut rng);
            let color = RAIN_COLORS[rng.gen_range(0..RAIN_COLORS.len())];
            if let Some(col) = self.columns.get_mut(col_idx) {
                col.chaos_char = Some(glyph);
                col.chaos_color = color;
            }
        }

        // Tick sparks
        self.sparks.retain_mut(|s| s.tick());

        // Advance reveal wave
        self.reveal_accumulator += self.reveal_speed;
        while self.reveal_accumulator >= 1.0 {
            self.reveal_accumulator -= 1.0;
            self.advance_reveal();
        }
    }

    /// Advance the reveal wave by one character
    fn advance_reveal(&mut self) {
        // Find next non-revealed, non-space column
        let target = self.columns.len().saturating_sub(self.wave_delay);
        while self.revealed_count < target {
            if let Some(col) = self.columns.get_mut(self.revealed_count) {
                col.revealed = true;
            }
            self.revealed_count += 1;
        }
    }

    /// Force reveal all (when streaming completes)
    pub fn reveal_all(&mut self) {
        for col in &mut self.columns {
            col.revealed = true;
        }
        self.revealed_count = self.columns.len();
        self.sparks.clear();
    }

    /// Check if fully revealed
    pub fn is_complete(&self) -> bool {
        self.columns.iter().all(|c| c.revealed)
    }

    /// Get the full text
    pub fn text(&self) -> String {
        self.columns
            .iter()
            .map(|c| c.target_char.unwrap_or(' '))
            .collect()
    }

    /// Get total character count
    pub fn len(&self) -> usize {
        self.columns.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }

    /// Build the text line (landing zone)
    fn build_text_line(&self) -> Line<'static> {
        let mut spans = Vec::new();

        for col in &self.columns {
            match (col.target_char, col.revealed) {
                (None, _) => {
                    // Space
                    spans.push(Span::raw(" "));
                }
                (Some(c), true) => {
                    // Revealed - white text
                    spans.push(Span::styled(
                        c.to_string(),
                        Style::default().fg(REVEALED_COLOR),
                    ));
                }
                (Some(_), false) => {
                    // Not revealed - show chaos
                    let display = col
                        .chaos_char
                        .as_ref()
                        .map(|g| g.display())
                        .unwrap_or_else(|| "?".to_string());
                    // Take only first char for width consistency
                    let first_char: String = display.chars().take(1).collect();
                    spans.push(Span::styled(
                        first_char,
                        Style::default()
                            .fg(col.chaos_color)
                            .add_modifier(Modifier::DIM),
                    ));
                }
            }
        }

        Line::from(spans)
    }

    /// Build a rain line (falling characters above text)
    fn build_rain_line(&self, line_idx: u8) -> Line<'static> {
        let mut spans = Vec::new();
        let _line_y = 1.0 - (line_idx as f32 / self.rain_height as f32);

        for col in &self.columns {
            if col.target_char.is_none() {
                spans.push(Span::raw(" "));
                continue;
            }

            // Find a drop at this y position
            let drop = col.drops.iter().find(|d| {
                let drop_line = ((1.0 - d.y) * self.rain_height as f32) as u8;
                drop_line == line_idx && d.y >= 0.0
            });

            if let Some(drop) = drop {
                let display = drop.glyph.display();
                let first_char: String = display.chars().take(1).collect();
                spans.push(Span::styled(
                    first_char,
                    Style::default().fg(drop.color).add_modifier(Modifier::DIM),
                ));
            } else {
                spans.push(Span::raw(" "));
            }
        }

        Line::from(spans)
    }

    /// Build all lines for rendering
    pub fn build_lines(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();

        // Rain lines (top to bottom)
        for i in (0..self.rain_height).rev() {
            lines.push(self.build_rain_line(i));
        }

        // Text line (landing zone)
        lines.push(self.build_text_line());

        lines
    }

    /// Build lines with word wrapping support
    ///
    /// For compatibility with ChatView which needs wrapped text.
    /// The rain effect is shown only above the first/current line.
    pub fn build_lines_wrapped(&self, max_width: usize) -> Vec<Line<'static>> {
        if max_width == 0 || self.columns.is_empty() {
            return self.build_lines();
        }

        // For now, just return unwrapped lines
        // The rain effect works best with single-line streaming
        // Future: implement proper wrapping with rain per line
        self.build_lines()
    }
}

impl Widget for MatrixRainReveal {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let lines = self.build_lines();
        let total_lines = lines.len();

        // Render from bottom up (text line at bottom)
        for (i, line) in lines.into_iter().enumerate() {
            let y = area.y + i as u16;
            if y >= area.y + area.height {
                break;
            }
            buf.set_line(area.x, y, &line, area.width);
        }

        // Render sparks overlay
        for spark in &self.sparks {
            let x = area.x + spark.x as u16;
            let y = area.y + (total_lines as i16 - 1 + spark.y_offset as i16) as u16;
            if x < area.x + area.width && y >= area.y && y < area.y + area.height {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_symbol(&spark.char.to_string());
                    cell.set_style(
                        Style::default()
                            .fg(spark.color)
                            .add_modifier(Modifier::BOLD),
                    );
                }
            }
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
    fn test_new_creates_empty() {
        let reveal = MatrixRainReveal::new();
        assert!(reveal.is_empty());
        assert_eq!(reveal.len(), 0);
    }

    #[test]
    fn test_push_text_creates_columns() {
        let mut reveal = MatrixRainReveal::new();
        reveal.push_text("Hello");
        assert_eq!(reveal.len(), 5);
        assert_eq!(reveal.text(), "Hello");
    }

    #[test]
    fn test_spaces_preserved() {
        let mut reveal = MatrixRainReveal::new();
        reveal.push_text("Hi there");
        assert_eq!(reveal.len(), 8);
        assert_eq!(reveal.text(), "Hi there");
        // Space column should be revealed
        assert!(reveal.columns[2].revealed);
    }

    #[test]
    fn test_tick_advances_animation() {
        let mut reveal = MatrixRainReveal::new();
        reveal.push_text("Test");
        let initial_frame = reveal.frame;
        reveal.tick();
        assert_eq!(reveal.frame, initial_frame + 1);
    }

    #[test]
    fn test_reveal_all() {
        let mut reveal = MatrixRainReveal::new();
        reveal.push_text("Hello world");
        assert!(!reveal.is_complete());
        reveal.reveal_all();
        assert!(reveal.is_complete());
    }

    #[test]
    fn test_build_lines_returns_correct_count() {
        let mut reveal = MatrixRainReveal::new().with_rain_height(3);
        reveal.push_text("Hi");
        let lines = reveal.build_lines();
        // 3 rain lines + 1 text line = 4
        assert_eq!(lines.len(), 4);
    }

    #[test]
    fn test_clear_resets_state() {
        let mut reveal = MatrixRainReveal::new();
        reveal.push_text("Hello");
        reveal.tick();
        reveal.tick();
        reveal.clear();
        assert!(reveal.is_empty());
        assert_eq!(reveal.frame, 0);
    }

    #[test]
    fn test_rain_glyph_distribution() {
        let mut rng = SmallRng::seed_from_u64(42);

        let mut katakana_count = 0;
        let mut ascii_count = 0;
        let mut kanji_count = 0;
        let mut nerd_count = 0;
        let mut emoji_count = 0;

        for _ in 0..1000 {
            match random_glyph(&mut rng) {
                RainGlyph::Char(c) => {
                    if KATAKANA.contains(&c) {
                        katakana_count += 1;
                    } else if ASCII.contains(&c) {
                        ascii_count += 1;
                    } else if KANJI.contains(&c) {
                        kanji_count += 1;
                    }
                }
                RainGlyph::Text(s) => {
                    if NERD_REFS.contains(&s.as_str()) {
                        nerd_count += 1;
                    } else {
                        emoji_count += 1;
                    }
                }
            }
        }

        // Check approximate distribution
        assert!(katakana_count > 350, "Katakana should be ~45%");
        assert!(ascii_count > 250, "ASCII should be ~35%");
        assert!(kanji_count > 50, "Kanji should be ~10%");
        assert!(nerd_count > 30, "Nerd should be ~8%");
        assert!(emoji_count < 50, "Emoji should be ~2%");
    }

    #[test]
    fn test_spark_lifecycle() {
        let mut rng = SmallRng::seed_from_u64(42);
        let mut spark = Spark::new(5, &mut rng);
        let initial_life = spark.life;

        // Tick until dead
        let mut ticks = 0;
        while spark.tick() {
            ticks += 1;
        }

        assert_eq!(ticks, initial_life as usize);
    }

    #[test]
    fn test_revealed_text_is_white() {
        let mut reveal = MatrixRainReveal::new();
        reveal.push_text("Hi");
        reveal.reveal_all();

        let lines = reveal.build_lines();
        let text_line = lines.last().unwrap();

        // Check that revealed chars have white color
        for span in &text_line.spans {
            if !span.content.trim().is_empty() {
                assert_eq!(span.style.fg, Some(REVEALED_COLOR));
            }
        }
    }
}
