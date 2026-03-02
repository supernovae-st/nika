//! Matrix Decrypt Widget - Text reveal effect with emoji chaos
//!
//! Creates a "decryption" effect where text is revealed progressively
//! from emoji/kanji chaos, synchronized with streaming tokens.
//!
//! ```text
//! Frame 0: 🏴‍☠️自🐉ニ🦋カ🪐海🦄賊🌌🐔🦈🔮🐙✨🌊🧚🛰️💎🦁
//! Frame 3: l🏴‍☠️t 自bl🐉ck ニ= B🦋ock:カ:bor🪐ere海d()
//! Frame 6: let bl🐙ck = Block::bor✨ered().title("Wel🌊ome");
//! Frame 10: let block = Block::bordered().title("Welcome");
//! ```

use std::borrow::Cow;

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Widget,
};

use crate::tui::theme::solarized;

// ═══════════════════════════════════════════════════════════════════════════════
// THEMED EMOJI COLLECTIONS
// ═══════════════════════════════════════════════════════════════════════════════

/// Pirate theme emojis (for fetch: verb - ocean/network)
const PIRATE_EMOJIS: &[&str] = &[
    "🏴‍☠️",
    "🐙",
    "🐳",
    "🌊",
    "🦈",
    "⚓",
    "🗡️",
    "💀",
    "🦜",
    "🏝️",
    "🧭",
    "⛵",
    "🦑",
    "🐚",
    "🪸",
];

/// Cosmic theme emojis (for infer: verb - thinking/AI)
const COSMIC_EMOJIS: &[&str] = &[
    "🪐", "🌌", "🛰️", "🌙", "✨", "🔮", "🌟", "☄️", "🚀", "👽", "🛸", "⭐", "💫", "🌠", "🔭",
];

/// Unicorn theme emojis (for magical/creative outputs)
const UNICORN_EMOJIS: &[&str] = &[
    "🦄", "🌈", "🧚", "🦋", "✨", "💖", "🌸", "🎀", "💫", "🍭", "🎠", "💜", "🩷", "🪷", "🦩",
];

/// Animal emojis - Nika mascots (for agent: verb)
const ANIMAL_EMOJIS: &[&str] = &[
    "🐔",
    "🐤",
    "🦙",
    "🐭",
    "🐉",
    "🦖",
    "🦀",
    "🦞",
    "🦁",
    "🐦‍🔥",
    "🦆",
    "🦦",
    "🐒",
    "🦊",
    "🦚",
    "🪽",
    "🐿️",
    "🦝",
    "🐸",
    "🦎",
];

/// Food & drink emojis (fun sprinkles)
const FOOD_EMOJIS: &[&str] = &[
    "🧀", "🍕", "🍺", "🍸", "🍻", "🥂", "🍣", "🌮", "🥦", "🌵", "🌴", "🧃", "🍜", "🥖", "🥐", "🌶️",
    "🍩", "🧁", "🍰", "🎂",
];

/// Special/rare emojis (high impact)
const SPECIAL_EMOJIS: &[&str] = &[
    "💎", "🦾", "⚡", "👺", "🫀", "🗿", "⛩️", "🍀", "🎅", "🎨", "🔥", "💀", "👾", "🤖", "🧠", "🎯",
    "🎪", "🎭", "🏆", "🎖️",
];

/// Tech emojis (for invoke: verb - MCP tools)
const TECH_EMOJIS: &[&str] = &[
    "🔌", "💾", "📡", "🖥️", "⚙️", "🔧", "🛠️", "📟", "💿", "🔋", "📶", "🖨️", "⌨️", "🖱️", "💽",
];

/// Nature emojis (for exec: verb - shell/system)
const NATURE_EMOJIS: &[&str] = &[
    "🌵", "🌴", "🥦", "🍀", "🌿", "🍃", "🌱", "🌲", "🌳", "🍂", "🍁", "🌾", "🪴", "🎋", "🎍",
];

/// Half-width Katakana - Classic Matrix style (monospace in terminals!)
const MATRIX_KATAKANA: &[char] = &[
    // Half-width katakana - consistent single-cell width
    'ｱ', 'ｲ', 'ｳ', 'ｴ', 'ｵ', 'ｶ', 'ｷ', 'ｸ', 'ｹ', 'ｺ', 'ｻ', 'ｼ', 'ｽ', 'ｾ', 'ｿ', 'ﾀ', 'ﾁ', 'ﾂ', 'ﾃ',
    'ﾄ', 'ﾅ', 'ﾆ', 'ﾇ', 'ﾈ', 'ﾉ', 'ﾊ', 'ﾋ', 'ﾌ', 'ﾍ', 'ﾎ', 'ﾏ', 'ﾐ', 'ﾑ', 'ﾒ', 'ﾓ', 'ﾔ', 'ﾕ', 'ﾖ',
    'ﾗ', 'ﾘ', 'ﾙ', 'ﾚ', 'ﾛ', 'ﾜ', 'ﾝ',
];

/// Full-width Kanji for flavor (double-width)
const KANJI_CHARS: &[char] = &[
    // 自由 (jiyū = Freedom) + ニカ (Nika)
    '自', '由', 'ニ', 'カ', // 海賊 (kaizoku = Pirate)
    '海', '賊', // Matrix-style kanji
    '風', '雷', '火', '水', '光', '闇', '星', '月', '龍', '虎',
];

/// Nerd/geek reference strings (cycling text)
const NERD_REFS: &[&str] = &[
    "42",    // Hitchhiker's Guide
    "EOF",   // End of file
    "NULL",  // Null pointer
    "sudo",  // Unix
    "git",   // Version control
    "0x",    // Hex prefix
    "fn",    // Rust function
    "async", // Async programming
    "OK",    // HTTP 200
    "ACK",   // Network acknowledgment
    "SYN",   // TCP handshake
    "NaN",   // Not a Number
    "π",     // Pi
    "∞",     // Infinity
    "λ",     // Lambda
    "Ω",     // Omega
];

/// Cycling text words
const CYCLING_WORDS: &[&str] = &[
    "Nika", "Freedom", "Liberté", "自由", "LOVE", "PEACE", "CODE", "RUST", "ASYNC", "MCP", "DAG",
    "YAML", "ARR", "AHOY", "MAGIC",
];

// ═══════════════════════════════════════════════════════════════════════════════
// VERB TYPE FOR COLOR MATCHING
// ═══════════════════════════════════════════════════════════════════════════════

/// Verb type for color theming
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DecryptVerb {
    /// LLM inference - violet/cosmic theme
    #[default]
    Infer,
    /// Shell execution - green/nature theme
    Exec,
    /// HTTP fetch - cyan/pirate theme
    Fetch,
    /// MCP invoke - blue/tech theme
    Invoke,
    /// Agent loop - yellow/animal theme
    Agent,
    /// Chat message - neutral
    Chat,
}

impl DecryptVerb {
    /// Get the primary color for this verb
    pub fn color(&self) -> Color {
        match self {
            DecryptVerb::Infer => solarized::VIOLET,
            DecryptVerb::Exec => solarized::GREEN,
            DecryptVerb::Fetch => solarized::CYAN,
            DecryptVerb::Invoke => solarized::BLUE,
            DecryptVerb::Agent => solarized::YELLOW,
            DecryptVerb::Chat => solarized::BASE0,
        }
    }

    /// Get the themed emojis for this verb
    pub fn emojis(&self) -> &'static [&'static str] {
        match self {
            DecryptVerb::Infer => COSMIC_EMOJIS,
            DecryptVerb::Exec => NATURE_EMOJIS,
            DecryptVerb::Fetch => PIRATE_EMOJIS,
            DecryptVerb::Invoke => TECH_EMOJIS,
            DecryptVerb::Agent => ANIMAL_EMOJIS,
            DecryptVerb::Chat => SPECIAL_EMOJIS,
        }
    }

    /// Get the icon for this verb
    pub fn icon(&self) -> &'static str {
        match self {
            DecryptVerb::Infer => "⚡",
            DecryptVerb::Exec => "📟",
            DecryptVerb::Fetch => "🛰️",
            DecryptVerb::Invoke => "🔌",
            DecryptVerb::Agent => "🐔",
            DecryptVerb::Chat => "💬",
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// MATRIX DECRYPT STATE
// ═══════════════════════════════════════════════════════════════════════════════

/// State for a single character being decrypted
#[derive(Clone)]
struct CharState {
    /// The final revealed character
    target: char,
    /// Current display (may be emoji or random char)
    current: DecryptGlyph,
    /// Reveal progress (0.0 = chaos, 1.0 = revealed)
    progress: f32,
    /// Per-character random seed
    seed: u64,
}

#[derive(Clone)]
enum DecryptGlyph {
    Char(char),
    Emoji(&'static str),
    Kanji(char),
    Text(&'static str),
}

impl DecryptGlyph {
    /// Returns glyph as string. Uses Cow to avoid allocation for static strings.
    fn as_str(&self) -> Cow<'static, str> {
        match self {
            DecryptGlyph::Char(c) => Cow::Owned(c.to_string()),
            DecryptGlyph::Emoji(e) => Cow::Borrowed(*e),
            DecryptGlyph::Kanji(k) => Cow::Owned(k.to_string()),
            DecryptGlyph::Text(t) => Cow::Borrowed(*t),
        }
    }

    fn display_width(&self) -> usize {
        match self {
            DecryptGlyph::Char(_) => 1,
            DecryptGlyph::Emoji(_) => 2,
            DecryptGlyph::Kanji(_) => 2,
            DecryptGlyph::Text(t) => t.chars().count(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// MATRIX DECRYPT WIDGET
// ═══════════════════════════════════════════════════════════════════════════════

/// Matrix Decrypt effect widget
///
/// Reveals text progressively from emoji/kanji chaos, synchronized with streaming.
pub struct MatrixDecrypt<'a> {
    /// The text to reveal
    text: &'a str,
    /// Current reveal progress (0.0 = all chaos, 1.0 = all revealed)
    progress: f32,
    /// Animation frame for flickering effect
    frame: u8,
    /// Verb type for theming
    verb: DecryptVerb,
    /// Seed for reproducible randomness
    seed: u64,
    /// Whether to include rare emojis
    with_rare_emojis: bool,
    /// Whether to include kanji
    with_kanji: bool,
    /// Reveal speed multiplier
    speed: f32,
}

impl<'a> Default for MatrixDecrypt<'a> {
    fn default() -> Self {
        Self {
            text: "",
            progress: 0.0,
            frame: 0,
            verb: DecryptVerb::default(),
            seed: 42,
            with_rare_emojis: true,
            with_kanji: true,
            speed: 1.0,
        }
    }
}

impl<'a> MatrixDecrypt<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            ..Default::default()
        }
    }

    /// Set the text to reveal
    pub fn text(mut self, text: &'a str) -> Self {
        self.text = text;
        self
    }

    /// Set reveal progress (0.0 = chaos, 1.0 = fully revealed)
    pub fn progress(mut self, progress: f32) -> Self {
        self.progress = progress.clamp(0.0, 1.0);
        self
    }

    /// Set animation frame for flickering
    pub fn frame(mut self, frame: u8) -> Self {
        self.frame = frame;
        self
    }

    /// Set verb type for color theming
    pub fn verb(mut self, verb: DecryptVerb) -> Self {
        self.verb = verb;
        self
    }

    /// Set random seed
    pub fn seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// Enable/disable rare emojis
    pub fn with_rare_emojis(mut self, enable: bool) -> Self {
        self.with_rare_emojis = enable;
        self
    }

    /// Enable/disable kanji characters
    pub fn with_kanji(mut self, enable: bool) -> Self {
        self.with_kanji = enable;
        self
    }

    /// Set reveal speed multiplier
    pub fn speed(mut self, speed: f32) -> Self {
        self.speed = speed.max(0.1);
        self
    }

    /// Generate a random chaos glyph based on verb theme
    fn random_glyph(&self, rng: &mut SmallRng) -> DecryptGlyph {
        let roll: f32 = rng.gen();

        // v0.8.1 TRUE Matrix-style: katakana-dominant, minimal emojis
        if roll < 0.35 {
            // 35% - ASCII characters (monospace)
            let chars: &[char] = &[
                'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p',
                'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z', '0', '1', '2', '3', '4', '5',
                '6', '7', '8', '9', '_', '<', '>', '[', ']', '{', '}', '/', '*', '#', '@', '^',
            ];
            let idx = rng.gen_range(0..chars.len());
            DecryptGlyph::Char(chars[idx])
        } else if roll < 0.80 {
            // 45% - Half-width katakana (THE Matrix look!)
            let idx = rng.gen_range(0..MATRIX_KATAKANA.len());
            DecryptGlyph::Char(MATRIX_KATAKANA[idx])
        } else if roll < 0.90 && self.with_kanji {
            // 10% - Full-width Kanji (Nika flavor)
            let idx = rng.gen_range(0..KANJI_CHARS.len());
            DecryptGlyph::Kanji(KANJI_CHARS[idx])
        } else if roll < 0.98 {
            // 8% - Nerd references
            let idx = rng.gen_range(0..NERD_REFS.len());
            DecryptGlyph::Text(NERD_REFS[idx])
        } else if self.with_rare_emojis {
            // 2% - Nika mascots ONLY (very rare!)
            const NIKA_MASCOTS: &[&str] = &["🐔", "⚡", "📟", "🛰️", "🔌", "🦋"];
            let idx = rng.gen_range(0..NIKA_MASCOTS.len());
            DecryptGlyph::Emoji(NIKA_MASCOTS[idx])
        } else {
            // Fallback to katakana
            let idx = rng.gen_range(0..MATRIX_KATAKANA.len());
            DecryptGlyph::Char(MATRIX_KATAKANA[idx])
        }
    }

    /// Calculate if a character should be revealed based on position and progress
    fn should_reveal(&self, char_index: usize, total_chars: usize, rng: &mut SmallRng) -> bool {
        if total_chars == 0 {
            return true;
        }

        // Base reveal threshold based on position (left reveals first)
        let position_factor = char_index as f32 / total_chars as f32;

        // Add some randomness for organic feel
        let random_offset: f32 = rng.gen_range(-0.15..0.15);

        // Adjusted threshold
        let threshold = (position_factor + random_offset).clamp(0.0, 1.0);

        // Reveal if progress exceeds threshold
        self.progress * self.speed > threshold
    }

    /// Build the display line with reveal effect
    fn build_line(&self) -> Line<'a> {
        let mut rng = SmallRng::seed_from_u64(self.seed.wrapping_add(self.frame as u64));
        let chars: Vec<char> = self.text.chars().collect();
        let total = chars.len();

        let mut spans = Vec::new();
        let base_color = self.verb.color();

        for (i, &ch) in chars.iter().enumerate() {
            // Per-character seed for consistent chaos pattern
            let char_seed = self.seed.wrapping_add(i as u64 * 31337);
            let mut char_rng = SmallRng::seed_from_u64(char_seed.wrapping_add(self.frame as u64));

            if self.should_reveal(i, total, &mut rng) {
                // Revealed - show actual character
                let style = Style::default().fg(base_color);
                spans.push(Span::styled(ch.to_string(), style));
            } else {
                // Not revealed - show chaos glyph
                let glyph = self.random_glyph(&mut char_rng);

                // Chaos color (dimmer, different hue)
                let chaos_color = self.chaos_color(&mut char_rng);
                // v0.8.1: 100% opacity - no DIM modifier for vibrant chaos
                let style = Style::default().fg(chaos_color);

                spans.push(Span::styled(glyph.as_str(), style));
            }
        }

        Line::from(spans)
    }

    /// Get a chaos color (varies by frame for flickering)
    fn chaos_color(&self, rng: &mut SmallRng) -> Color {
        let colors = [
            solarized::CYAN,
            solarized::GREEN,
            solarized::YELLOW,
            solarized::ORANGE,
            solarized::MAGENTA,
            solarized::VIOLET,
            solarized::BLUE,
            solarized::RED,
        ];
        colors[rng.gen_range(0..colors.len())]
    }
}

impl Widget for MatrixDecrypt<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 || self.text.is_empty() {
            return;
        }

        let line = self.build_line();

        // Render line at top of area
        let x = area.x;
        let y = area.y;

        buf.set_line(x, y, &line, area.width);
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// STREAMING DECRYPT STATE
// ═══════════════════════════════════════════════════════════════════════════════

/// Stateful wrapper for streaming text with decrypt effect
#[derive(Debug, Clone)]
pub struct StreamingDecrypt {
    /// Full text received so far
    text: String,
    /// Current reveal progress per character (0.0 to 1.0)
    char_progress: Vec<f32>,
    /// Birth frame for each character (for initial chaos delay)
    char_birth_frames: Vec<u8>,
    /// Animation frame counter
    frame: u8,
    /// Verb type for theming
    verb: DecryptVerb,
    /// Random seed
    seed: u64,
    /// Auto-advance reveal speed (progress per frame)
    reveal_speed: f32,
    /// Wave factor: how much position affects reveal delay (0.0 to 1.0)
    /// Higher = later characters reveal much slower
    wave_factor: f32,
    /// Initial chaos frames: how long new chars stay in chaos before revealing
    initial_chaos_frames: u8,
    /// Whether fully revealed
    is_complete: bool,
}

impl Default for StreamingDecrypt {
    fn default() -> Self {
        Self {
            text: String::new(),
            char_progress: Vec::new(),
            char_birth_frames: Vec::new(),
            frame: 0,
            verb: DecryptVerb::default(),
            seed: 42,
            reveal_speed: 0.025, // v0.8.1: Slower reveal (~40 frames = 667ms at 60fps)
            wave_factor: 0.15,   // v0.8.1: Cascade effect - later chars reveal slower
            initial_chaos_frames: 8, // v0.8.1: ~130ms of visible chaos before reveal
            is_complete: false,
        }
    }
}

impl StreamingDecrypt {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set verb type for theming
    pub fn with_verb(mut self, verb: DecryptVerb) -> Self {
        self.verb = verb;
        self
    }

    /// Set reveal speed (progress per frame, 0.0 to 1.0)
    pub fn with_reveal_speed(mut self, speed: f32) -> Self {
        self.reveal_speed = speed.clamp(0.005, 1.0);
        self
    }

    /// Set wave factor (cascade effect strength, 0.0 to 1.0)
    /// Higher values = later characters reveal much slower
    pub fn with_wave_factor(mut self, factor: f32) -> Self {
        self.wave_factor = factor.clamp(0.0, 0.5);
        self
    }

    /// Set initial chaos frames (delay before revealing starts)
    pub fn with_initial_chaos(mut self, frames: u8) -> Self {
        self.initial_chaos_frames = frames;
        self
    }

    /// Set random seed
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// Append new streaming text (from LLM token)
    pub fn push_text(&mut self, new_text: &str) {
        for ch in new_text.chars() {
            self.text.push(ch);
            // New characters start at 0% revealed
            self.char_progress.push(0.0);
            // Record birth frame for initial chaos delay
            self.char_birth_frames.push(self.frame);
        }
    }

    /// Advance animation frame and reveal progress
    /// v0.8.1: Implements cascade effect and initial chaos delay
    pub fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);

        let total_chars = self.char_progress.len();
        if total_chars == 0 {
            return;
        }

        // Advance reveal progress for each character with cascade effect
        let mut all_revealed = true;
        for (i, progress) in self.char_progress.iter_mut().enumerate() {
            if *progress >= 1.0 {
                continue;
            }

            // Check initial chaos delay
            let birth_frame = self.char_birth_frames.get(i).copied().unwrap_or(0);
            let frames_alive = self.frame.wrapping_sub(birth_frame);
            if frames_alive < self.initial_chaos_frames {
                // Stay in chaos - don't advance progress
                all_revealed = false;
                continue;
            }

            // Calculate cascade effect: later chars have reduced speed
            let position_factor = i as f32 / total_chars.max(1) as f32;
            let speed_multiplier = 1.0 - (position_factor * self.wave_factor);
            let effective_speed = self.reveal_speed * speed_multiplier;

            *progress = (*progress + effective_speed).min(1.0);
            if *progress < 1.0 {
                all_revealed = false;
            }
        }

        self.is_complete = all_revealed && !self.text.is_empty();
    }

    /// Force complete reveal (skip animation)
    pub fn reveal_all(&mut self) {
        for progress in &mut self.char_progress {
            *progress = 1.0;
        }
        self.is_complete = true;
    }

    /// Check if fully revealed
    pub fn is_complete(&self) -> bool {
        self.is_complete
    }

    /// Get current text
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Get current verb (v0.17.1)
    pub fn verb(&self) -> DecryptVerb {
        self.verb
    }

    /// Clear state for new message
    pub fn clear(&mut self) {
        self.text.clear();
        self.char_progress.clear();
        self.char_birth_frames.clear();
        self.frame = 0;
        self.is_complete = false;
    }

    /// Calculate overall progress (0.0 to 1.0)
    pub fn overall_progress(&self) -> f32 {
        if self.char_progress.is_empty() {
            return 1.0;
        }
        self.char_progress.iter().sum::<f32>() / self.char_progress.len() as f32
    }

    /// Create a widget for rendering current state
    pub fn widget(&self) -> MatrixDecrypt<'_> {
        MatrixDecrypt::new(&self.text)
            .progress(self.overall_progress())
            .frame(self.frame)
            .verb(self.verb)
            .seed(self.seed)
    }

    /// Render directly to buffer
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        self.widget().render(area, buf);
    }

    /// Build Line for use in List context (ChatView)
    /// Returns a Line with the matrix decrypt effect applied
    pub fn build_line(&self) -> Line<'static> {
        use rand::rngs::SmallRng;
        use rand::SeedableRng;

        let chars: Vec<char> = self.text.chars().collect();

        let mut spans = Vec::new();
        let base_color = self.verb.color();

        for (i, &ch) in chars.iter().enumerate() {
            // Per-character seed for consistent chaos pattern
            let char_seed = self.seed.wrapping_add(i as u64 * 31337);
            let mut char_rng = SmallRng::seed_from_u64(char_seed.wrapping_add(self.frame as u64));

            // Get progress for this character
            let progress = self.char_progress.get(i).copied().unwrap_or(1.0);

            if progress >= 1.0 {
                // Revealed - show actual character
                let style = Style::default().fg(base_color);
                spans.push(Span::styled(ch.to_string(), style));
            } else {
                // Not revealed - show chaos glyph
                let glyph = Self::random_glyph_static(&mut char_rng);

                // Chaos color (varies by frame for flickering)
                let colors = [
                    solarized::CYAN,
                    solarized::GREEN,
                    solarized::YELLOW,
                    solarized::ORANGE,
                    solarized::MAGENTA,
                    solarized::VIOLET,
                    solarized::BLUE,
                    solarized::RED,
                ];
                let chaos_color = colors[char_rng.gen_range(0..colors.len())];
                // v0.8.1: 100% opacity - no DIM modifier for vibrant chaos
                let style = Style::default().fg(chaos_color);

                spans.push(Span::styled(glyph, style));
            }
        }

        Line::from(spans)
    }

    /// Build multiple Lines for multi-line text (splits on newlines)
    pub fn build_lines(&self) -> Vec<Line<'static>> {
        // Split text into lines and build each one
        let lines_text: Vec<&str> = self.text.lines().collect();
        let mut result = Vec::new();
        let mut char_offset = 0;

        for line_text in lines_text {
            let line_len = line_text.chars().count();

            // Build line with the subset of char_progress for this line
            let line = self.build_line_segment(line_text, char_offset);
            result.push(line);

            char_offset += line_len + 1; // +1 for newline
        }

        result
    }

    /// Build multiple Lines with word wrapping (v0.8.1)
    ///
    /// Wraps text at the specified max_width to prevent overflow.
    /// Maintains character progress indices across wrapped lines.
    pub fn build_lines_wrapped(&self, max_width: usize) -> Vec<Line<'static>> {
        if max_width == 0 {
            return self.build_lines();
        }

        let mut result = Vec::new();
        let mut global_char_offset = 0;

        // Process each original line (split by \n)
        for original_line in self.text.lines() {
            let original_len = original_line.chars().count();

            // Wrap this line to max_width
            let wrapped = Self::wrap_line(original_line, max_width);

            let mut line_char_offset = global_char_offset;
            for wrapped_segment in wrapped {
                let segment_len = wrapped_segment.chars().count();
                let line = self.build_line_segment(&wrapped_segment, line_char_offset);
                result.push(line);
                line_char_offset += segment_len;
            }

            global_char_offset += original_len + 1; // +1 for newline
        }

        if result.is_empty() {
            result.push(Line::from(vec![]));
        }

        result
    }

    /// Simple word wrapping helper (v0.8.1)
    fn wrap_line(text: &str, max_width: usize) -> Vec<String> {
        if max_width == 0 || text.is_empty() {
            return vec![text.to_string()];
        }

        let mut result = Vec::new();
        let mut current_line = String::new();
        let mut current_width = 0;

        for word in text.split_whitespace() {
            let word_width = word.chars().count();

            if word_width > max_width {
                // Word is too long, must split it
                if !current_line.is_empty() {
                    result.push(current_line);
                    current_line = String::new();
                    current_width = 0;
                }
                // Split long word into chunks
                let mut chars = word.chars().peekable();
                while chars.peek().is_some() {
                    let chunk: String = chars.by_ref().take(max_width).collect();
                    if !chunk.is_empty() {
                        result.push(chunk);
                    }
                }
            } else if current_width == 0 {
                // First word on line
                current_line = word.to_string();
                current_width = word_width;
            } else if current_width + 1 + word_width <= max_width {
                // Word fits with space
                current_line.push(' ');
                current_line.push_str(word);
                current_width += 1 + word_width;
            } else {
                // Word doesn't fit, start new line
                result.push(current_line);
                current_line = word.to_string();
                current_width = word_width;
            }
        }

        if !current_line.is_empty() {
            result.push(current_line);
        }

        if result.is_empty() {
            result.push(String::new());
        }

        result
    }

    /// Build a Line for a specific segment of text
    fn build_line_segment(&self, text: &str, char_offset: usize) -> Line<'static> {
        use rand::rngs::SmallRng;
        use rand::SeedableRng;

        let chars: Vec<char> = text.chars().collect();

        let mut spans = Vec::new();
        let base_color = self.verb.color();

        for (i, &ch) in chars.iter().enumerate() {
            let global_idx = char_offset + i;
            let char_seed = self.seed.wrapping_add(global_idx as u64 * 31337);
            let mut char_rng = SmallRng::seed_from_u64(char_seed.wrapping_add(self.frame as u64));

            // Get progress for this character
            let progress = self.char_progress.get(global_idx).copied().unwrap_or(1.0);

            if progress >= 1.0 {
                let style = Style::default().fg(base_color);
                spans.push(Span::styled(ch.to_string(), style));
            } else {
                let glyph = Self::random_glyph_static(&mut char_rng);
                let colors = [
                    solarized::CYAN,
                    solarized::GREEN,
                    solarized::YELLOW,
                    solarized::ORANGE,
                    solarized::MAGENTA,
                    solarized::VIOLET,
                    solarized::BLUE,
                    solarized::RED,
                ];
                let chaos_color = colors[char_rng.gen_range(0..colors.len())];
                // v0.8.1: 100% opacity - no DIM modifier for vibrant chaos
                let style = Style::default().fg(chaos_color);
                spans.push(Span::styled(glyph, style));
            }
        }

        Line::from(spans)
    }

    /// Generate a random chaos glyph (static version for build_line)
    /// v0.8.1: TRUE Matrix-style - katakana-dominant, Nika mascots only
    fn random_glyph_static(rng: &mut SmallRng) -> String {
        let roll: f32 = rng.gen();

        if roll < 0.35 {
            // 35% - ASCII characters (monospace)
            let chars: &[char] = &[
                'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p',
                'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z', '0', '1', '2', '3', '4', '5',
                '6', '7', '8', '9', '_', '<', '>', '[', ']', '{', '}', '/', '*', '#', '@', '^',
            ];
            let idx = rng.gen_range(0..chars.len());
            chars[idx].to_string()
        } else if roll < 0.80 {
            // 45% - Half-width katakana (THE Matrix look!)
            let idx = rng.gen_range(0..MATRIX_KATAKANA.len());
            MATRIX_KATAKANA[idx].to_string()
        } else if roll < 0.90 {
            // 10% - Full-width Kanji (Nika flavor)
            let idx = rng.gen_range(0..KANJI_CHARS.len());
            KANJI_CHARS[idx].to_string()
        } else if roll < 0.98 {
            // 8% - Nerd references
            let idx = rng.gen_range(0..NERD_REFS.len());
            NERD_REFS[idx].to_string()
        } else {
            // 2% - Nika mascots ONLY (very rare!)
            const NIKA_MASCOTS: &[&str] = &["🐔", "⚡", "📟", "🛰️", "🔌", "🦋"];
            let idx = rng.gen_range(0..NIKA_MASCOTS.len());
            NIKA_MASCOTS[idx].to_string()
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// MULTI-LINE STREAMING DECRYPT
// ═══════════════════════════════════════════════════════════════════════════════

/// Multi-line streaming decrypt for longer outputs
#[derive(Clone, Default)]
pub struct MultiLineDecrypt {
    /// Lines of text
    lines: Vec<StreamingDecrypt>,
    /// Current line being written to
    current_line: usize,
    /// Verb type for theming
    verb: DecryptVerb,
    /// Reveal speed
    reveal_speed: f32,
    /// Seed
    seed: u64,
}

impl MultiLineDecrypt {
    pub fn new() -> Self {
        Self {
            lines: vec![StreamingDecrypt::default()],
            current_line: 0,
            verb: DecryptVerb::default(),
            reveal_speed: 0.05,
            seed: 42,
        }
    }

    pub fn with_verb(mut self, verb: DecryptVerb) -> Self {
        self.verb = verb;
        for line in &mut self.lines {
            line.verb = verb;
        }
        self
    }

    pub fn with_reveal_speed(mut self, speed: f32) -> Self {
        self.reveal_speed = speed;
        for line in &mut self.lines {
            line.reveal_speed = speed;
        }
        self
    }

    /// Push streaming text, handling newlines
    pub fn push_text(&mut self, text: &str) {
        for ch in text.chars() {
            if ch == '\n' {
                // Start new line
                self.current_line += 1;
                let new_line = StreamingDecrypt::new()
                    .with_verb(self.verb)
                    .with_reveal_speed(self.reveal_speed)
                    .with_seed(self.seed.wrapping_add(self.current_line as u64));
                self.lines.push(new_line);
            } else {
                // Add to current line
                if self.current_line >= self.lines.len() {
                    let new_line = StreamingDecrypt::new()
                        .with_verb(self.verb)
                        .with_reveal_speed(self.reveal_speed)
                        .with_seed(self.seed.wrapping_add(self.current_line as u64));
                    self.lines.push(new_line);
                }
                self.lines[self.current_line].push_text(&ch.to_string());
            }
        }
    }

    /// Advance animation for all lines
    pub fn tick(&mut self) {
        for line in &mut self.lines {
            line.tick();
        }
    }

    /// Reveal all lines immediately
    pub fn reveal_all(&mut self) {
        for line in &mut self.lines {
            line.reveal_all();
        }
    }

    /// Check if all lines are revealed
    pub fn is_complete(&self) -> bool {
        self.lines.iter().all(|l| l.is_complete())
    }

    /// Clear all content
    pub fn clear(&mut self) {
        self.lines.clear();
        self.lines.push(
            StreamingDecrypt::new()
                .with_verb(self.verb)
                .with_reveal_speed(self.reveal_speed)
                .with_seed(self.seed),
        );
        self.current_line = 0;
    }

    /// Get line count
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Render to buffer with scroll offset
    pub fn render(&self, area: Rect, buf: &mut Buffer, scroll_offset: usize) {
        for (i, line) in self.lines.iter().enumerate().skip(scroll_offset) {
            let y = area.y + (i - scroll_offset) as u16;
            if y >= area.y + area.height {
                break;
            }
            let line_area = Rect::new(area.x, y, area.width, 1);
            line.render(line_area, buf);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn test_decrypt_verb_colors() {
        assert_eq!(DecryptVerb::Infer.color(), solarized::VIOLET);
        assert_eq!(DecryptVerb::Exec.color(), solarized::GREEN);
        assert_eq!(DecryptVerb::Fetch.color(), solarized::CYAN);
        assert_eq!(DecryptVerb::Invoke.color(), solarized::BLUE);
        assert_eq!(DecryptVerb::Agent.color(), solarized::YELLOW);
    }

    #[test]
    fn test_decrypt_verb_emojis() {
        assert!(!DecryptVerb::Infer.emojis().is_empty());
        assert!(!DecryptVerb::Agent.emojis().is_empty());
        assert!(DecryptVerb::Infer.emojis().contains(&"🪐"));
        assert!(DecryptVerb::Agent.emojis().contains(&"🐔"));
    }

    #[test]
    fn test_decrypt_verb_icons() {
        assert_eq!(DecryptVerb::Infer.icon(), "⚡");
        assert_eq!(DecryptVerb::Agent.icon(), "🐔");
    }

    #[test]
    fn test_matrix_decrypt_renders() {
        let backend = TestBackend::new(40, 5);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let decrypt = MatrixDecrypt::new("Hello, World!")
                    .progress(0.5)
                    .frame(0)
                    .verb(DecryptVerb::Infer);
                frame.render_widget(decrypt, frame.area());
            })
            .unwrap();
    }

    #[test]
    fn test_matrix_decrypt_full_reveal() {
        let backend = TestBackend::new(40, 5);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let decrypt = MatrixDecrypt::new("Fully revealed")
                    .progress(1.0)
                    .verb(DecryptVerb::Exec);
                frame.render_widget(decrypt, frame.area());
            })
            .unwrap();
    }

    #[test]
    fn test_matrix_decrypt_chaos() {
        let backend = TestBackend::new(40, 5);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let decrypt = MatrixDecrypt::new("All chaos")
                    .progress(0.0)
                    .verb(DecryptVerb::Fetch);
                frame.render_widget(decrypt, frame.area());
            })
            .unwrap();
    }

    #[test]
    fn test_streaming_decrypt_push() {
        let mut stream = StreamingDecrypt::new();
        stream.push_text("Hello");
        assert_eq!(stream.text(), "Hello");
        assert_eq!(stream.char_progress.len(), 5);

        stream.push_text(" World");
        assert_eq!(stream.text(), "Hello World");
        assert_eq!(stream.char_progress.len(), 11);
    }

    #[test]
    fn test_streaming_decrypt_tick() {
        // v0.8.1: Need to skip initial chaos frames before reveal starts
        let mut stream = StreamingDecrypt::new()
            .with_reveal_speed(0.5)
            .with_initial_chaos(0) // Disable chaos delay for this test
            .with_wave_factor(0.0); // Disable cascade for predictable test
        stream.push_text("Hi");

        assert!(!stream.is_complete());

        stream.tick();
        assert_eq!(stream.char_progress[0], 0.5);

        stream.tick();
        assert_eq!(stream.char_progress[0], 1.0);
        assert!(stream.is_complete());
    }

    #[test]
    fn test_streaming_decrypt_initial_chaos_delay() {
        let mut stream = StreamingDecrypt::new()
            .with_reveal_speed(1.0) // Would reveal instantly without chaos delay
            .with_initial_chaos(5) // 5 frames of chaos (frames 1-4, then reveal at 5)
            .with_wave_factor(0.0);
        stream.push_text("A");

        // First 4 ticks should NOT advance progress (chaos phase)
        // Note: initial_chaos_frames=5 means frames_alive must be >= 5 to reveal
        // tick increments frame first, so: tick 1->frame=1, tick 4->frame=4 still in chaos
        for i in 0..4 {
            stream.tick();
            assert_eq!(
                stream.char_progress[0],
                0.0,
                "Should stay at 0 during chaos at tick {}",
                i + 1
            );
        }

        // 5th tick: frame=5, frames_alive=5, 5 >= 5 so reveal starts
        stream.tick();
        assert_eq!(stream.char_progress[0], 1.0, "Should reveal after chaos");
        assert!(stream.is_complete());
    }

    #[test]
    fn test_streaming_decrypt_cascade_effect() {
        let mut stream = StreamingDecrypt::new()
            .with_reveal_speed(0.1)
            .with_initial_chaos(0) // No delay
            .with_wave_factor(0.5); // Strong cascade
        stream.push_text("ABC");

        // After one tick, first char should have more progress than last
        stream.tick();
        let first_progress = stream.char_progress[0];
        let last_progress = stream.char_progress[2];
        assert!(
            first_progress > last_progress,
            "First char ({}) should reveal faster than last ({})",
            first_progress,
            last_progress
        );
    }

    #[test]
    fn test_streaming_decrypt_reveal_all() {
        let mut stream = StreamingDecrypt::new();
        stream.push_text("Test text");

        assert!(!stream.is_complete());

        stream.reveal_all();
        assert!(stream.is_complete());
        assert!(stream.char_progress.iter().all(|&p| p == 1.0));
    }

    #[test]
    fn test_streaming_decrypt_clear() {
        let mut stream = StreamingDecrypt::new();
        stream.push_text("Some text");
        stream.tick();

        stream.clear();
        assert!(stream.text().is_empty());
        assert!(stream.char_progress.is_empty());
        assert!(!stream.is_complete());
    }

    #[test]
    fn test_streaming_decrypt_verb() {
        let stream = StreamingDecrypt::new().with_verb(DecryptVerb::Agent);
        assert_eq!(stream.verb, DecryptVerb::Agent);
    }

    #[test]
    fn test_multi_line_decrypt() {
        let mut multi = MultiLineDecrypt::new();
        multi.push_text("Line 1\nLine 2\nLine 3");

        assert_eq!(multi.line_count(), 3);
        assert_eq!(multi.lines[0].text(), "Line 1");
        assert_eq!(multi.lines[1].text(), "Line 2");
        assert_eq!(multi.lines[2].text(), "Line 3");
    }

    #[test]
    fn test_multi_line_tick() {
        let mut multi = MultiLineDecrypt::new().with_reveal_speed(1.0); // Fast reveal
        multi.push_text("A\nB");

        // v0.8.1: Need multiple ticks due to initial chaos delay (default 8 frames)
        // Tick enough times to pass chaos delay + reveal
        for _ in 0..15 {
            multi.tick();
        }
        assert!(multi.is_complete());
    }

    #[test]
    fn test_multi_line_reveal_all() {
        let mut multi = MultiLineDecrypt::new();
        multi.push_text("Line 1\nLine 2");

        assert!(!multi.is_complete());
        multi.reveal_all();
        assert!(multi.is_complete());
    }

    #[test]
    fn test_kanji_chars_included() {
        assert!(KANJI_CHARS.contains(&'自'));
        assert!(KANJI_CHARS.contains(&'由'));
        assert!(KANJI_CHARS.contains(&'ニ'));
        assert!(KANJI_CHARS.contains(&'カ'));
        assert!(KANJI_CHARS.contains(&'海'));
        assert!(KANJI_CHARS.contains(&'賊'));
    }

    #[test]
    fn test_nerd_refs_included() {
        assert!(NERD_REFS.contains(&"42"));
        assert!(NERD_REFS.contains(&"NULL"));
        assert!(NERD_REFS.contains(&"sudo"));
    }

    #[test]
    fn test_cycling_words_included() {
        assert!(CYCLING_WORDS.contains(&"Nika"));
        assert!(CYCLING_WORDS.contains(&"Freedom"));
        assert!(CYCLING_WORDS.contains(&"自由"));
    }

    #[test]
    fn test_emoji_themes_non_empty() {
        assert!(!PIRATE_EMOJIS.is_empty());
        assert!(!COSMIC_EMOJIS.is_empty());
        assert!(!UNICORN_EMOJIS.is_empty());
        assert!(!ANIMAL_EMOJIS.is_empty());
        assert!(!FOOD_EMOJIS.is_empty());
        assert!(!SPECIAL_EMOJIS.is_empty());
        assert!(!TECH_EMOJIS.is_empty());
        assert!(!NATURE_EMOJIS.is_empty());
    }

    #[test]
    fn test_decrypt_glyph_width() {
        assert_eq!(DecryptGlyph::Char('a').display_width(), 1);
        assert_eq!(DecryptGlyph::Emoji("🐔").display_width(), 2);
        assert_eq!(DecryptGlyph::Kanji('自').display_width(), 2);
        assert_eq!(DecryptGlyph::Text("42").display_width(), 2);
    }

    #[test]
    fn test_overall_progress() {
        let mut stream = StreamingDecrypt::new();
        assert_eq!(stream.overall_progress(), 1.0); // Empty = complete

        stream.push_text("AB");
        assert_eq!(stream.overall_progress(), 0.0);

        stream.char_progress[0] = 1.0;
        assert_eq!(stream.overall_progress(), 0.5);

        stream.char_progress[1] = 1.0;
        assert_eq!(stream.overall_progress(), 1.0);
    }

    #[test]
    fn test_build_line_returns_line() {
        let mut stream = StreamingDecrypt::new().with_verb(DecryptVerb::Agent);
        stream.push_text("Hello");
        stream.reveal_all(); // Fully reveal

        let line = stream.build_line();
        // Should have 5 spans (one per character)
        assert_eq!(line.spans.len(), 5);
    }

    #[test]
    fn test_build_lines_multiline() {
        let mut stream = StreamingDecrypt::new();
        stream.push_text("Line 1\nLine 2\nLine 3");
        stream.reveal_all();

        let lines = stream.build_lines();
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_build_line_chaos_when_not_revealed() {
        let stream = StreamingDecrypt::new().with_verb(DecryptVerb::Fetch);
        // Don't push any text or tick - should be empty
        let line = stream.build_line();
        assert!(line.spans.is_empty());
    }

    #[test]
    fn test_build_lines_wrapped_short_text() {
        let mut stream = StreamingDecrypt::new();
        stream.push_text("Hello World");
        stream.reveal_all();

        // Width is large enough - no wrapping needed
        let lines = stream.build_lines_wrapped(50);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_build_lines_wrapped_wraps_long_text() {
        let mut stream = StreamingDecrypt::new();
        stream.push_text("Hello World Test");
        stream.reveal_all();

        // Width of 10 should wrap "Hello World Test" into multiple lines
        let lines = stream.build_lines_wrapped(10);
        assert!(lines.len() >= 2);
    }

    #[test]
    fn test_build_lines_wrapped_zero_width_fallback() {
        let mut stream = StreamingDecrypt::new();
        stream.push_text("Hello\nWorld");
        stream.reveal_all();

        // Width 0 should fall back to build_lines
        let lines = stream.build_lines_wrapped(0);
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_build_lines_wrapped_multiline_with_wrap() {
        let mut stream = StreamingDecrypt::new();
        stream.push_text("This is a long first line\nShort second");
        stream.reveal_all();

        // Width of 15 should wrap the first line
        let lines = stream.build_lines_wrapped(15);
        assert!(lines.len() >= 3); // First line wraps + second line
    }

    #[test]
    fn test_wrap_line_simple() {
        let result = StreamingDecrypt::wrap_line("Hello World", 20);
        assert_eq!(result, vec!["Hello World"]);
    }

    #[test]
    fn test_wrap_line_breaks_at_space() {
        let result = StreamingDecrypt::wrap_line("Hello World Test", 10);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "Hello");
        assert_eq!(result[1], "World Test");
    }

    #[test]
    fn test_wrap_line_long_word_split() {
        let result = StreamingDecrypt::wrap_line("Supercalifragilistic", 5);
        assert!(result.len() >= 4); // Should split the long word
    }

    #[test]
    fn test_wrap_line_empty() {
        let result = StreamingDecrypt::wrap_line("", 10);
        assert_eq!(result, vec![""]);
    }
}
