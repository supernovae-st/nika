// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

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

mod streaming;

pub use streaming::StreamingDecrypt;

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

use crate::theme::solarized;

// ═══════════════════════════════════════════════════════════════════════════════
// THEMED EMOJI COLLECTIONS
// ═══════════════════════════════════════════════════════════════════════════════

/// Pirate theme emojis (for fetch: verb - ocean/network)
pub(crate) const PIRATE_EMOJIS: &[&str] = &[
    "🏴\u{200d}☠️",
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
pub(crate) const COSMIC_EMOJIS: &[&str] = &[
    "🪐", "🌌", "🛰️", "🌙", "✨", "🔮", "🌟", "☄️", "🚀", "👽", "🛸", "⭐", "💫", "🌠", "🔭",
];

/// Animal emojis - Nika mascots (for agent: verb)
pub(crate) const ANIMAL_EMOJIS: &[&str] = &[
    "🐔",
    "🐤",
    "🦙",
    "🐭",
    "🐉",
    "🦖",
    "🦀",
    "🦞",
    "🦁",
    "🐦\u{200d}🔥",
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

/// Special/rare emojis (high impact)
pub(crate) const SPECIAL_EMOJIS: &[&str] = &[
    "💎", "🦾", "⚡", "👺", "🫀", "🗿", "⛩️", "🍀", "🎅", "🎨", "🔥", "💀", "👾", "🤖", "🧠", "🎯",
    "🎪", "🎭", "🏆", "🎖️",
];

/// Tech emojis (for invoke: verb - MCP tools)
pub(crate) const TECH_EMOJIS: &[&str] = &[
    "🔌", "💾", "📡", "🖥️", "⚙️", "🔧", "🛠️", "📟", "💿", "🔋", "📶", "🖨️", "⌨️", "🖱️", "💽",
];

/// Nature emojis (for exec: verb - shell/system)
pub(crate) const NATURE_EMOJIS: &[&str] = &[
    "🌵", "🌴", "🥦", "🍀", "🌿", "🍃", "🌱", "🌲", "🌳", "🍂", "🍁", "🌾", "🪴", "🎋", "🎍",
];

/// Half-width Katakana - Classic Matrix style (monospace in terminals!)
pub(crate) const MATRIX_KATAKANA: &[char] = &[
    // Half-width katakana - consistent single-cell width
    'ｱ', 'ｲ', 'ｳ', 'ｴ', 'ｵ', 'ｶ', 'ｷ', 'ｸ', 'ｹ', 'ｺ', 'ｻ', 'ｼ', 'ｽ', 'ｾ', 'ｿ', 'ﾀ', 'ﾁ', 'ﾂ', 'ﾃ',
    'ﾄ', 'ﾅ', 'ﾆ', 'ﾇ', 'ﾈ', 'ﾉ', 'ﾊ', 'ﾋ', 'ﾌ', 'ﾍ', 'ﾎ', 'ﾏ', 'ﾐ', 'ﾑ', 'ﾒ', 'ﾓ', 'ﾔ', 'ﾕ', 'ﾖ',
    'ﾗ', 'ﾘ', 'ﾙ', 'ﾚ', 'ﾛ', 'ﾜ', 'ﾝ',
];

/// Full-width Kanji for flavor (double-width)
pub(crate) const KANJI_CHARS: &[char] = &[
    // 自由 (jiyū = Freedom) + ニカ (Nika)
    '自', '由', 'ニ', 'カ', // 海賊 (kaizoku = Pirate)
    '海', '賊', // Matrix-style kanji
    '風', '雷', '火', '水', '光', '闇', '星', '月', '龍', '虎',
];

/// Nerd/geek reference strings (cycling text)
pub(crate) const NERD_REFS: &[&str] = &[
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

#[derive(Clone)]
pub(crate) enum DecryptGlyph {
    Char(char),
    Emoji(&'static str),
    Kanji(char),
    Text(&'static str),
}

impl DecryptGlyph {
    /// Returns glyph as string. Uses Cow to avoid allocation for static strings.
    pub(crate) fn as_str(&self) -> Cow<'static, str> {
        match self {
            DecryptGlyph::Char(c) => Cow::Owned(c.to_string()),
            DecryptGlyph::Emoji(e) => Cow::Borrowed(*e),
            DecryptGlyph::Kanji(k) => Cow::Owned(k.to_string()),
            DecryptGlyph::Text(t) => Cow::Borrowed(*t),
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

        // TRUE Matrix-style: katakana-dominant, minimal emojis
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
                // 100% opacity - no DIM modifier for vibrant chaos
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
    fn test_emoji_themes_non_empty() {
        assert!(!PIRATE_EMOJIS.is_empty());
        assert!(!COSMIC_EMOJIS.is_empty());
        assert!(!ANIMAL_EMOJIS.is_empty());
        assert!(!SPECIAL_EMOJIS.is_empty());
        assert!(!TECH_EMOJIS.is_empty());
        assert!(!NATURE_EMOJIS.is_empty());
    }
}
