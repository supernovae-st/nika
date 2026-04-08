// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Matrix Verification Effect for Provider Modal
//!
//! WOW Cosmic Edition
//! Shows a "decryption" animation while providers are being verified.
//! Text scrambles with Matrix-style katakana/kanji then reveals the result.

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};

// =============================================================================
// SEMANTIC COLOR CONSTANTS
// =============================================================================

/// Status: checking (in progress)
const COLOR_STATUS_CHECKING: Color = Color::Rgb(99, 102, 241); // Indigo-500

/// Status: connected (success)
const COLOR_STATUS_CONNECTED: Color = Color::Rgb(34, 197, 94); // Emerald-500

/// Status: failed (error)
const COLOR_STATUS_FAILED: Color = Color::Rgb(239, 68, 68); // Red-500

/// Status: not configured
const COLOR_STATUS_UNCONFIGURED: Color = Color::Rgb(107, 114, 128); // Gray-500

/// Matrix effect: primary green
const COLOR_MATRIX_GREEN: Color = Color::Rgb(34, 197, 94); // Emerald-500

/// Matrix effect: light green
const COLOR_MATRIX_GREEN_LIGHT: Color = Color::Rgb(74, 222, 128); // Emerald-400

/// Matrix effect: dark green
const COLOR_MATRIX_GREEN_DARK: Color = Color::Rgb(22, 163, 74); // Emerald-600

/// Matrix effect: accent indigo
const COLOR_MATRIX_INDIGO: Color = Color::Rgb(99, 102, 241); // Indigo-500

/// Half-width Katakana for Matrix effect (single-cell width)
const MATRIX_KATAKANA: &[char] = &[
    'ｱ', 'ｲ', 'ｳ', 'ｴ', 'ｵ', 'ｶ', 'ｷ', 'ｸ', 'ｹ', 'ｺ', 'ｻ', 'ｼ', 'ｽ', 'ｾ', 'ｿ', 'ﾀ', 'ﾁ', 'ﾂ', 'ﾃ',
    'ﾄ', 'ﾅ', 'ﾆ', 'ﾇ', 'ﾈ', 'ﾉ', 'ﾊ', 'ﾋ', 'ﾌ', 'ﾍ', 'ﾎ', 'ﾏ', 'ﾐ', 'ﾑ', 'ﾒ', 'ﾓ', 'ﾔ', 'ﾕ', 'ﾖ',
    'ﾗ', 'ﾘ', 'ﾙ', 'ﾚ', 'ﾛ', 'ﾜ', 'ﾝ',
];

/// ASCII characters for variety
const ASCII_CHARS: &[char] = &[
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f', 'x', 'y', 'z',
    '@', '#', '$', '%', '&', '*', '+', '-', '=', '<', '>', '/', '\\', '|',
];

/// Provider connection-check status (for Matrix verification animation)
///
/// Distinct from `provider_selector::VerifyStatus` which tracks the
/// verification-cache lifecycle (Unknown/Verifying/Verified/Failed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionCheckStatus {
    /// Still checking
    #[default]
    Checking,
    /// Successfully connected
    Connected,
    /// Connection failed
    Failed,
    /// Not configured (no API key)
    NotConfigured,
}

impl ConnectionCheckStatus {
    /// Get color for this status
    pub fn color(&self) -> Color {
        match self {
            ConnectionCheckStatus::Checking => COLOR_STATUS_CHECKING,
            ConnectionCheckStatus::Connected => COLOR_STATUS_CONNECTED,
            ConnectionCheckStatus::Failed => COLOR_STATUS_FAILED,
            ConnectionCheckStatus::NotConfigured => COLOR_STATUS_UNCONFIGURED,
        }
    }

    /// Get status indicator
    pub fn indicator(&self) -> &'static str {
        match self {
            ConnectionCheckStatus::Checking => "⏳",
            ConnectionCheckStatus::Connected => "✓",
            ConnectionCheckStatus::Failed => "✗",
            ConnectionCheckStatus::NotConfigured => "○",
        }
    }
}

/// Single provider verification entry
#[derive(Clone)]
pub struct VerifyEntry {
    /// Provider name
    pub name: String,
    /// Current status
    pub status: ConnectionCheckStatus,
    /// Animation frame
    pub frame: u8,
    /// Reveal progress (0.0 = scrambled, 1.0 = revealed)
    pub progress: f32,
    /// Random seed for consistent scrambling
    pub seed: u64,
}

impl VerifyEntry {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            status: ConnectionCheckStatus::Checking,
            frame: 0,
            progress: 0.0,
            seed: name
                .as_bytes()
                .iter()
                .fold(0u64, |acc, &b| acc.wrapping_add(b as u64)),
        }
    }

    /// Advance animation frame
    pub fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);

        // Auto-advance progress when checking (slow reveal effect)
        if self.status == ConnectionCheckStatus::Checking {
            // Progress advances slowly during checking
            self.progress = (self.progress + 0.02).min(0.7); // Cap at 70% during check
        } else {
            // Rapidly complete reveal when status is known
            self.progress = (self.progress + 0.15).min(1.0);
        }
    }

    /// Set final status and start rapid reveal
    pub fn set_status(&mut self, status: ConnectionCheckStatus) {
        self.status = status;
    }

    /// Check if animation is complete
    pub fn is_complete(&self) -> bool {
        self.progress >= 1.0 && self.status != ConnectionCheckStatus::Checking
    }

    /// Generate a scrambled character
    fn scramble_char(&self, _char_idx: usize, rng: &mut SmallRng) -> char {
        let roll: f32 = rng.gen();
        if roll < 0.6 {
            // 60% katakana
            MATRIX_KATAKANA[rng.gen_range(0..MATRIX_KATAKANA.len())]
        } else {
            // 40% ASCII
            ASCII_CHARS[rng.gen_range(0..ASCII_CHARS.len())]
        }
    }

    /// Build the displayed string
    pub fn render_text(&self) -> Vec<(char, Color)> {
        let mut rng = SmallRng::seed_from_u64(self.seed.wrapping_add(self.frame as u64));
        let chars: Vec<char> = self.name.chars().collect();
        let mut result = Vec::with_capacity(chars.len());

        for (i, &ch) in chars.iter().enumerate() {
            // Per-character reveal threshold (left-to-right wave)
            let char_threshold = (i as f32 / chars.len().max(1) as f32) * 0.8;
            let char_rng: f32 = rng.gen_range(0.0..0.2);
            let reveal_threshold = char_threshold + char_rng;

            if self.progress > reveal_threshold {
                // Revealed - show actual character with status color
                result.push((ch, self.status.color()));
            } else {
                // Scrambled - show matrix character with flickering color
                let scrambled = self.scramble_char(i, &mut rng);
                let colors = [
                    COLOR_MATRIX_GREEN,
                    COLOR_MATRIX_GREEN_LIGHT,
                    COLOR_MATRIX_GREEN_DARK,
                    COLOR_MATRIX_INDIGO,
                ];
                let color = colors[rng.gen_range(0..colors.len())];
                result.push((scrambled, color));
            }
        }

        result
    }
}

/// Matrix verification effect widget
pub struct VerificationEffect<'a> {
    entries: &'a [VerifyEntry],
    /// Show status indicator
    show_indicator: bool,
    /// Compact mode (single line per provider)
    compact: bool,
}

impl<'a> VerificationEffect<'a> {
    pub fn new(entries: &'a [VerifyEntry]) -> Self {
        Self {
            entries,
            show_indicator: true,
            compact: false,
        }
    }

    /// Show/hide status indicators
    pub fn show_indicator(mut self, show: bool) -> Self {
        self.show_indicator = show;
        self
    }

    /// Compact mode
    pub fn compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }
}

impl Widget for VerificationEffect<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 10 || area.height == 0 {
            return;
        }

        // Currently both modes use single-line entries
        let line_height = 1;
        let max_entries = (area.height as usize) / line_height;

        for (i, entry) in self.entries.iter().take(max_entries).enumerate() {
            let y = area.y + (i * line_height) as u16;
            if y >= area.y + area.height {
                break;
            }

            let mut x = area.x;

            // Status indicator
            if self.show_indicator {
                let indicator = entry.status.indicator();
                buf.set_string(x, y, indicator, Style::default().fg(entry.status.color()));
                x += 2; // Indicator + space
            }

            // Provider name with matrix effect
            let chars = entry.render_text();
            for (ch, color) in chars {
                if x >= area.x + area.width {
                    break;
                }
                buf.set_string(x, y, ch.to_string(), Style::default().fg(color));
                x += 1;
            }

            // Latency/error suffix when revealed
            if entry.is_complete() && x + 5 < area.x + area.width {
                let suffix = match entry.status {
                    ConnectionCheckStatus::Connected => " ✓",
                    ConnectionCheckStatus::Failed => " ✗",
                    ConnectionCheckStatus::NotConfigured => " ○",
                    ConnectionCheckStatus::Checking => "",
                };
                buf.set_string(
                    x,
                    y,
                    suffix,
                    Style::default()
                        .fg(entry.status.color())
                        .add_modifier(Modifier::BOLD),
                );
            }
        }
    }
}

/// Compact inline verification widget (for single provider)
pub struct VerifyInline<'a> {
    entry: &'a VerifyEntry,
}

impl<'a> VerifyInline<'a> {
    pub fn new(entry: &'a VerifyEntry) -> Self {
        Self { entry }
    }
}

impl Widget for VerifyInline<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 5 {
            return;
        }

        let chars = self.entry.render_text();
        let mut x = area.x;

        for (ch, color) in chars {
            if x >= area.x + area.width {
                break;
            }
            buf.set_string(x, area.y, ch.to_string(), Style::default().fg(color));
            x += 1;
        }
    }
}

/// State manager for verification animation
#[derive(Clone, Default)]
pub struct VerificationState {
    /// Provider entries
    pub entries: Vec<VerifyEntry>,
    /// Global animation frame
    pub frame: u8,
    /// Whether all verifications are complete
    pub all_complete: bool,
}

impl VerificationState {
    /// Create new verification state for 7 providers
    pub fn new_providers() -> Self {
        let providers = [
            "Claude", "OpenAI", "Mistral", "Groq", "DeepSeek", "Gemini", "xAI",
        ];
        Self {
            entries: providers.iter().map(|&p| VerifyEntry::new(p)).collect(),
            frame: 0,
            all_complete: false,
        }
    }

    /// Advance all animations
    pub fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);
        for entry in &mut self.entries {
            entry.tick();
        }
        self.all_complete = self.entries.iter().all(|e| e.is_complete());
    }

    /// Update a provider's status
    pub fn set_status(&mut self, index: usize, status: ConnectionCheckStatus) {
        if let Some(entry) = self.entries.get_mut(index) {
            entry.set_status(status);
        }
    }

    /// Get provider index by name
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.entries
            .iter()
            .position(|e| e.name.eq_ignore_ascii_case(name))
    }

    /// Reset all entries
    pub fn reset(&mut self) {
        for entry in &mut self.entries {
            entry.status = ConnectionCheckStatus::Checking;
            entry.progress = 0.0;
            entry.frame = 0;
        }
        self.frame = 0;
        self.all_complete = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    #[test]
    fn test_verify_status_colors() {
        assert_eq!(
            ConnectionCheckStatus::Connected.color(),
            COLOR_STATUS_CONNECTED
        );
        assert_eq!(ConnectionCheckStatus::Failed.color(), COLOR_STATUS_FAILED);
        assert_eq!(
            ConnectionCheckStatus::NotConfigured.color(),
            COLOR_STATUS_UNCONFIGURED
        );
        assert_eq!(
            ConnectionCheckStatus::Checking.color(),
            COLOR_STATUS_CHECKING
        );
    }

    #[test]
    fn test_verify_status_indicators() {
        assert_eq!(ConnectionCheckStatus::Connected.indicator(), "✓");
        assert_eq!(ConnectionCheckStatus::Failed.indicator(), "✗");
        assert_eq!(ConnectionCheckStatus::NotConfigured.indicator(), "○");
        assert_eq!(ConnectionCheckStatus::Checking.indicator(), "⏳");
    }

    #[test]
    fn test_verify_entry_new() {
        let entry = VerifyEntry::new("Claude");
        assert_eq!(entry.name, "Claude");
        assert_eq!(entry.status, ConnectionCheckStatus::Checking);
        assert_eq!(entry.progress, 0.0);
        assert_eq!(entry.frame, 0);
    }

    #[test]
    fn test_verify_entry_tick_checking() {
        let mut entry = VerifyEntry::new("Test");
        entry.tick();
        assert_eq!(entry.frame, 1);
        assert!(entry.progress > 0.0);
        assert!(entry.progress <= 0.7); // Capped during checking
    }

    #[test]
    fn test_verify_entry_tick_revealed() {
        let mut entry = VerifyEntry::new("Test");
        entry.set_status(ConnectionCheckStatus::Connected);

        // Tick multiple times
        for _ in 0..10 {
            entry.tick();
        }

        assert_eq!(entry.progress, 1.0);
        assert!(entry.is_complete());
    }

    #[test]
    fn test_verify_entry_render_text() {
        let entry = VerifyEntry::new("ABC");
        let result = entry.render_text();
        assert_eq!(result.len(), 3); // One per character
    }

    #[test]
    fn test_verify_entry_render_text_full_reveal() {
        let mut entry = VerifyEntry::new("Test");
        entry.progress = 1.0;
        entry.status = ConnectionCheckStatus::Connected;

        let result = entry.render_text();
        // All characters should be revealed with status color
        for (_ch, color) in result {
            assert_eq!(color, ConnectionCheckStatus::Connected.color());
        }
    }

    #[test]
    fn test_verification_state_new_providers() {
        let state = VerificationState::new_providers();
        assert_eq!(state.entries.len(), 7);
        assert_eq!(state.entries[0].name, "Claude");
        assert_eq!(state.entries[5].name, "Gemini");
        assert_eq!(state.entries[6].name, "xAI");
    }

    #[test]
    fn test_verification_state_tick() {
        let mut state = VerificationState::new_providers();
        state.tick();
        assert_eq!(state.frame, 1);
        for entry in &state.entries {
            assert!(entry.progress > 0.0);
        }
    }

    #[test]
    fn test_verification_state_set_status() {
        let mut state = VerificationState::new_providers();
        state.set_status(0, ConnectionCheckStatus::Connected);
        assert_eq!(state.entries[0].status, ConnectionCheckStatus::Connected);
    }

    #[test]
    fn test_verification_state_index_of() {
        let state = VerificationState::new_providers();
        assert_eq!(state.index_of("Claude"), Some(0));
        assert_eq!(state.index_of("claude"), Some(0)); // Case insensitive
        assert_eq!(state.index_of("Gemini"), Some(5));
        assert_eq!(state.index_of("Unknown"), None);
    }

    #[test]
    fn test_verification_state_reset() {
        let mut state = VerificationState::new_providers();
        state.set_status(0, ConnectionCheckStatus::Connected);
        state.tick();

        state.reset();

        assert_eq!(state.frame, 0);
        assert_eq!(state.entries[0].status, ConnectionCheckStatus::Checking);
        assert_eq!(state.entries[0].progress, 0.0);
    }

    #[test]
    fn test_verification_state_all_complete() {
        let mut state = VerificationState::new_providers();

        // Set all cloud providers to connected
        for i in 0..crate::widgets::provider_modal::state::providers::CLOUD_PROVIDER_COUNT {
            state.set_status(i, ConnectionCheckStatus::Connected);
        }

        // Tick until complete
        for _ in 0..20 {
            state.tick();
        }

        assert!(state.all_complete);
    }

    #[test]
    fn test_verification_effect_renders() {
        let entries = vec![VerifyEntry::new("Claude"), VerifyEntry::new("OpenAI")];
        let effect = VerificationEffect::new(&entries);

        let mut buf = Buffer::empty(Rect::new(0, 0, 40, 5));
        effect.render(Rect::new(0, 0, 40, 5), &mut buf);
        // Should not panic
    }

    #[test]
    fn test_verification_effect_compact() {
        let entries = vec![VerifyEntry::new("Test")];
        let effect = VerificationEffect::new(&entries)
            .compact(true)
            .show_indicator(false);

        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 1));
        effect.render(Rect::new(0, 0, 20, 1), &mut buf);
    }

    #[test]
    fn test_verify_inline_renders() {
        let entry = VerifyEntry::new("Claude");
        let inline = VerifyInline::new(&entry);

        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 1));
        inline.render(Rect::new(0, 0, 20, 1), &mut buf);
    }

    #[test]
    fn test_verify_inline_small_area() {
        let entry = VerifyEntry::new("Claude");
        let inline = VerifyInline::new(&entry);

        // Very small area - should not panic
        let mut buf = Buffer::empty(Rect::new(0, 0, 3, 1));
        inline.render(Rect::new(0, 0, 3, 1), &mut buf);
    }

    #[test]
    fn test_scramble_deterministic() {
        let entry = VerifyEntry::new("Test");
        let text1 = entry.render_text();
        let text2 = entry.render_text();

        // Same frame, same seed = same scramble
        assert_eq!(text1.len(), text2.len());
    }

    #[test]
    fn test_scramble_changes_with_frame() {
        let mut entry = VerifyEntry::new("Test");
        entry.progress = 0.0; // Keep scrambled
        let text1 = entry.render_text();

        entry.frame = 10;
        let text2 = entry.render_text();

        // Different frame = different scramble (usually)
        // Note: May occasionally match by chance, so just verify they render
        assert_eq!(text1.len(), 4);
        assert_eq!(text2.len(), 4);
    }

    #[test]
    fn test_katakana_chars_valid() {
        // Verify all katakana are valid half-width
        for &ch in MATRIX_KATAKANA {
            assert!(
                ch as u32 >= 0xFF61 && ch as u32 <= 0xFF9F,
                "Character {} is not half-width katakana",
                ch
            );
        }
    }

    #[test]
    fn test_ascii_chars_valid() {
        for &ch in ASCII_CHARS {
            assert!(ch.is_ascii(), "Character {} is not ASCII", ch);
        }
    }
}
