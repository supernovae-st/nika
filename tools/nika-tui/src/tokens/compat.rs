// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! # Color Compatibility Module
//!
//! Provides const color aliases as alternatives to inline `Color::Rgb()` values.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crate::tokens::compat::GREEN_500;
//! let color = GREEN_500;
//!
//! // Or use semantic colors directly:
//! use crate::tokens::colors::ColorPalette;
//! let palette = ColorPalette::tailwind();
//! let color = palette.green_500;
//! ```

use ratatui::style::Color;

// ═══════════════════════════════════════════════════════════════════════════════
// TAILWIND CSS CONST ALIASES
// Most frequently used colors in the codebase (sorted by usage count)
// ═══════════════════════════════════════════════════════════════════════════════

// --- GREEN (37 usages) ---
pub const GREEN_400: Color = Color::Rgb(74, 222, 128); // #4ade80
pub const GREEN_500: Color = Color::Rgb(34, 197, 94); // #22c55e

// --- RED (30 usages) ---
pub const RED_400: Color = Color::Rgb(248, 113, 113); // #f87171
pub const RED_500: Color = Color::Rgb(239, 68, 68); // #ef4444
pub const RED_600: Color = Color::Rgb(220, 38, 38); // #dc2626

// --- GRAY (28 usages) ---
pub const GRAY_400: Color = Color::Rgb(156, 163, 175); // #9ca3af
pub const GRAY_500: Color = Color::Rgb(107, 114, 128); // #6b7280
pub const GRAY_600: Color = Color::Rgb(75, 85, 99); // #4b5563
pub const GRAY_700: Color = Color::Rgb(55, 65, 81); // #374151
pub const GRAY_800: Color = Color::Rgb(31, 41, 55); // #1f2937
pub const GRAY_900: Color = Color::Rgb(17, 24, 39); // #111827

// --- VIOLET (26 usages) ---
pub const VIOLET_400: Color = Color::Rgb(167, 139, 250); // #a78bfa
pub const VIOLET_500: Color = Color::Rgb(139, 92, 246); // #8b5cf6
pub const VIOLET_600: Color = Color::Rgb(124, 58, 237); // #7c3aed

// --- AMBER (24 usages) ---
pub const AMBER_400: Color = Color::Rgb(251, 191, 36); // #fbbf24
pub const AMBER_500: Color = Color::Rgb(245, 158, 11); // #f59e0b
pub const AMBER_600: Color = Color::Rgb(217, 119, 6); // #d97706

// --- SLATE (21 usages) ---
pub const SLATE_50: Color = Color::Rgb(248, 250, 252); // #f8fafc
pub const SLATE_100: Color = Color::Rgb(241, 245, 249); // #f1f5f9
pub const SLATE_200: Color = Color::Rgb(226, 232, 240); // #e2e8f0
pub const SLATE_300: Color = Color::Rgb(203, 213, 225); // #cbd5e1
pub const SLATE_400: Color = Color::Rgb(148, 163, 184); // #94a3b8
pub const SLATE_500: Color = Color::Rgb(100, 116, 139); // #64748b
pub const SLATE_600: Color = Color::Rgb(71, 85, 105); // #475569
pub const SLATE_700: Color = Color::Rgb(51, 65, 85); // #334155
pub const SLATE_800: Color = Color::Rgb(30, 41, 59); // #1e293b
pub const SLATE_900: Color = Color::Rgb(15, 23, 42); // #0f172a
pub const SLATE_950: Color = Color::Rgb(2, 6, 23); // #020617

// --- YELLOW (12 usages) ---
pub const YELLOW_300: Color = Color::Rgb(253, 224, 71); // #fde047
pub const YELLOW_400: Color = Color::Rgb(250, 204, 21); // #facc15
pub const YELLOW_500: Color = Color::Rgb(234, 179, 8); // #eab308

// --- CYAN (10 usages) ---
pub const CYAN_400: Color = Color::Rgb(34, 211, 238); // #22d3ee
pub const CYAN_500: Color = Color::Rgb(6, 182, 212); // #06b6d4
pub const CYAN_600: Color = Color::Rgb(8, 145, 178); // #0891b2

// --- BLUE (10 usages) ---
pub const BLUE_400: Color = Color::Rgb(96, 165, 250); // #60a5fa
pub const BLUE_500: Color = Color::Rgb(59, 130, 246); // #3b82f6
pub const BLUE_600: Color = Color::Rgb(37, 99, 235); // #2563eb

// --- EMERALD (6 usages) ---
pub const EMERALD_400: Color = Color::Rgb(52, 211, 153); // #34d399
pub const EMERALD_500: Color = Color::Rgb(16, 185, 129); // #10b981
pub const EMERALD_600: Color = Color::Rgb(5, 150, 105); // #059669

// --- ROSE (5 usages) ---
pub const ROSE_400: Color = Color::Rgb(251, 113, 133); // #fb7185
pub const ROSE_500: Color = Color::Rgb(244, 63, 94); // #f43f5e
pub const ROSE_600: Color = Color::Rgb(225, 29, 72); // #e11d48

// --- PINK ---
pub const PINK_400: Color = Color::Rgb(244, 114, 182); // #f472b6
pub const PINK_500: Color = Color::Rgb(236, 72, 153); // #ec4899

// --- ORANGE ---
pub const ORANGE_400: Color = Color::Rgb(251, 146, 60); // #fb923c
pub const ORANGE_500: Color = Color::Rgb(249, 115, 22); // #f97316

// --- INDIGO (11 usages - not in main palette) ---
pub const INDIGO_400: Color = Color::Rgb(129, 140, 248); // #818cf8
pub const INDIGO_500: Color = Color::Rgb(99, 102, 241); // #6366f1
pub const INDIGO_600: Color = Color::Rgb(79, 70, 229); // #4f46e5

// --- TEAL ---
pub const TEAL_400: Color = Color::Rgb(45, 212, 191); // #2dd4bf
pub const TEAL_500: Color = Color::Rgb(20, 184, 166); // #14b8a6

// --- SKY ---
pub const SKY_400: Color = Color::Rgb(56, 189, 248); // #38bdf8
pub const SKY_500: Color = Color::Rgb(14, 165, 233); // #0ea5e9

// --- LIME ---
pub const LIME_400: Color = Color::Rgb(163, 230, 53); // #a3e635
pub const LIME_500: Color = Color::Rgb(132, 204, 22); // #84cc16

// --- FUCHSIA ---
pub const FUCHSIA_400: Color = Color::Rgb(232, 121, 249); // #e879f9
pub const FUCHSIA_500: Color = Color::Rgb(217, 70, 239); // #d946ef

// --- ZINC ---
pub const ZINC_400: Color = Color::Rgb(161, 161, 170); // #a1a1aa
pub const ZINC_500: Color = Color::Rgb(113, 113, 122); // #71717a

// ═══════════════════════════════════════════════════════════════════════════════
// SOLARIZED COLORS
// Some files use Ethan Schoonover's Solarized palette
// ═══════════════════════════════════════════════════════════════════════════════

/// Solarized base colors (backgrounds)
pub mod solarized {
    use ratatui::style::Color;

    // Base tones
    pub const BASE03: Color = Color::Rgb(0, 43, 54); // #002b36 (dark bg)
    pub const BASE02: Color = Color::Rgb(7, 54, 66); // #073642 (dark bg highlight)
    pub const BASE01: Color = Color::Rgb(88, 110, 117); // #586e75 (content, dark emphasis)
    pub const BASE00: Color = Color::Rgb(101, 123, 131); // #657b83 (content, light emphasis)
    pub const BASE0: Color = Color::Rgb(131, 148, 150); // #839496 (content, dark primary)
    pub const BASE1: Color = Color::Rgb(147, 161, 161); // #93a1a1 (content, light primary)
    pub const BASE2: Color = Color::Rgb(238, 232, 213); // #eee8d5 (light bg highlight)
    pub const BASE3: Color = Color::Rgb(253, 246, 227); // #fdf6e3 (light bg)

    // Accent colors
    pub const YELLOW: Color = Color::Rgb(181, 137, 0); // #b58900
    pub const ORANGE: Color = Color::Rgb(203, 75, 22); // #cb4b16
    pub const RED: Color = Color::Rgb(220, 50, 47); // #dc322f
    pub const MAGENTA: Color = Color::Rgb(211, 54, 130); // #d33682
    pub const VIOLET: Color = Color::Rgb(108, 113, 196); // #6c71c4
    pub const BLUE: Color = Color::Rgb(38, 139, 210); // #268bd2
    pub const CYAN: Color = Color::Rgb(42, 161, 152); // #2aa198
    pub const GREEN: Color = Color::Rgb(133, 153, 0); // #859900
}

// ═══════════════════════════════════════════════════════════════════════════════
// SEMANTIC ALIASES
// High-level meaning-based colors for common use cases
// ═══════════════════════════════════════════════════════════════════════════════

/// Success colors
pub const SUCCESS: Color = GREEN_500;
pub const SUCCESS_LIGHT: Color = GREEN_400;

/// Error colors
pub const ERROR: Color = RED_500;
pub const ERROR_LIGHT: Color = RED_400;

/// Warning colors
pub const WARNING: Color = AMBER_500;
pub const WARNING_LIGHT: Color = AMBER_400;

/// Info colors
pub const INFO: Color = BLUE_500;
pub const INFO_LIGHT: Color = BLUE_400;

/// Muted/disabled colors
pub const MUTED: Color = GRAY_500;
pub const MUTED_LIGHT: Color = GRAY_400;

// ═══════════════════════════════════════════════════════════════════════════════
// VERB COLORS (5 semantic verbs)
// ═══════════════════════════════════════════════════════════════════════════════

/// ⚡ infer: LLM generation
pub const VERB_INFER: Color = VIOLET_500;
/// 📟 exec: Shell command
pub const VERB_EXEC: Color = AMBER_500;
/// 🛰️ fetch: HTTP request
pub const VERB_FETCH: Color = CYAN_500;
/// 🔌 invoke: MCP tool call
pub const VERB_INVOKE: Color = EMERALD_500;
/// 🐔 agent: Multi-turn agentic loop
pub const VERB_AGENT: Color = ROSE_500;

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_const_colors_match_palette() {
        use crate::tokens::colors::ColorPalette;
        let p = ColorPalette::tailwind();

        // Verify const aliases match palette values
        assert_eq!(GREEN_500, p.green_500);
        assert_eq!(RED_500, p.red_500);
        assert_eq!(GRAY_500, p.gray_500);
        assert_eq!(VIOLET_500, p.violet_500);
        assert_eq!(AMBER_500, p.amber_500);
        assert_eq!(SLATE_500, p.slate_500);
        assert_eq!(YELLOW_400, p.yellow_400);
        assert_eq!(CYAN_500, p.cyan_500);
        assert_eq!(BLUE_500, p.blue_500);
    }

    #[test]
    fn test_semantic_aliases() {
        assert_eq!(SUCCESS, GREEN_500);
        assert_eq!(ERROR, RED_500);
        assert_eq!(WARNING, AMBER_500);
        assert_eq!(INFO, BLUE_500);
        assert_eq!(MUTED, GRAY_500);
    }

    #[test]
    fn test_verb_colors() {
        assert_eq!(VERB_INFER, VIOLET_500);
        assert_eq!(VERB_EXEC, AMBER_500);
        assert_eq!(VERB_FETCH, CYAN_500);
        assert_eq!(VERB_INVOKE, EMERALD_500);
        assert_eq!(VERB_AGENT, ROSE_500);
    }

    #[test]
    fn test_solarized_base_colors() {
        // Verify Solarized official values
        assert_eq!(solarized::BASE03, Color::Rgb(0, 43, 54));
        assert_eq!(solarized::BASE3, Color::Rgb(253, 246, 227));
        assert_eq!(solarized::CYAN, Color::Rgb(42, 161, 152));
    }
}
