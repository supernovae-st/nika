// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Streaming decrypt state machine
//!
//! Wraps `MatrixDecrypt` with per-character progress tracking for streaming
//! text (LLM tokens). Handles cascade reveal, initial chaos delay, and
//! word wrapping.

use super::*;

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
    pub(super) verb: DecryptVerb,
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
            reveal_speed: 0.025, // Slower reveal (~40 frames = 667ms at 60fps)
            wave_factor: 0.15,   // Cascade effect - later chars reveal slower
            initial_chaos_frames: 8, // ~130ms of visible chaos before reveal
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
    /// Implements cascade effect and initial chaos delay
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

    /// Get current verb
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
                // 100% opacity - no DIM modifier for vibrant chaos
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

    /// Build multiple Lines with word wrapping
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

    /// Simple word wrapping helper
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
                // 100% opacity - no DIM modifier for vibrant chaos
                let style = Style::default().fg(chaos_color);
                spans.push(Span::styled(glyph, style));
            }
        }

        Line::from(spans)
    }

    /// Generate a random chaos glyph (static version for build_line)
    /// TRUE Matrix-style - katakana-dominant, Nika mascots only
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
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

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
        // Need to skip initial chaos frames before reveal starts
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
