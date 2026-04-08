// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! # Semantic Colors (Level 2)
//!
//! Purpose-driven color assignments derived from the raw palette.
//!
//! ## Philosophy
//!
//! Semantic colors abstract raw palette values into meaningful categories:
//! - **Background**: Canvas, surfaces, states
//! - **Text**: Primary, secondary, muted, disabled
//! - **Border**: Default, focused, subtle
//! - **Accent**: Primary (violet), secondary (cyan), tertiary (pink)
//! - **Status**: Success, warning, error, info
//! - **Verb**: 5 semantic verbs with distinct colors

use super::ColorPalette;
use ratatui::style::Color;

// ═══════════════════════════════════════════════════════════════════════════════
// SEMANTIC COLORS (Level 2)
// ═══════════════════════════════════════════════════════════════════════════════

/// Semantic color assignments for a theme variant
///
/// Maps abstract concepts (bg, text, accent) to concrete colors.
#[derive(Debug, Clone)]
pub struct SemanticColors {
    // ═══════════════════════════════════════════════════════════════════════════
    // BACKGROUNDS
    // ═══════════════════════════════════════════════════════════════════════════
    /// Primary background (main canvas)
    pub bg_primary: Color,
    /// Secondary background (elevated surfaces, sidebars)
    pub bg_secondary: Color,
    /// Tertiary background (cards, panels, modals)
    pub bg_tertiary: Color,
    /// Hover state background
    pub bg_hover: Color,
    /// Active/selected state background
    pub bg_active: Color,

    // ═══════════════════════════════════════════════════════════════════════════
    // TEXT
    // ═══════════════════════════════════════════════════════════════════════════
    /// Primary text (main content, headings)
    pub text_primary: Color,
    /// Secondary text (descriptions, labels)
    pub text_secondary: Color,
    /// Muted text (hints, placeholders)
    pub text_muted: Color,
    /// Disabled text
    pub text_disabled: Color,
    /// Inverse text (on accent backgrounds)
    pub text_inverse: Color,

    // ═══════════════════════════════════════════════════════════════════════════
    // BORDERS
    // ═══════════════════════════════════════════════════════════════════════════
    /// Default border (panels, inputs)
    pub border_default: Color,
    /// Focused border (active elements)
    pub border_focused: Color,
    /// Subtle border (dividers, separators)
    pub border_subtle: Color,

    // ═══════════════════════════════════════════════════════════════════════════
    // ACCENTS (Cosmic Theme)
    // ═══════════════════════════════════════════════════════════════════════════
    /// Primary accent (🦋 Violet - Nika brand)
    pub accent_primary: Color,
    /// Secondary accent (🌌 Cyan - cosmic glow)
    pub accent_secondary: Color,
    /// Tertiary accent (Pink - nebula highlights)
    pub accent_tertiary: Color,

    // ═══════════════════════════════════════════════════════════════════════════
    // STATUS
    // ═══════════════════════════════════════════════════════════════════════════
    /// Success color
    pub status_success: Color,
    /// Warning color
    pub status_warning: Color,
    /// Error color
    pub status_error: Color,
    /// Info color
    pub status_info: Color,

    // ═══════════════════════════════════════════════════════════════════════════
    // VERB COLORS (5 semantic verbs)
    // ═══════════════════════════════════════════════════════════════════════════
    /// Infer verb (Violet) - LLM generation
    pub verb_infer: Color,
    /// Exec verb (Amber) - Shell execution
    pub verb_exec: Color,
    /// Fetch verb (Cyan) - HTTP requests
    pub verb_fetch: Color,
    /// Invoke verb (Emerald) - MCP tool calls
    pub verb_invoke: Color,
    /// Agent verb (Rose) - Multi-turn agentic loop
    pub verb_agent: Color,

    // ═══════════════════════════════════════════════════════════════════════════
    // SCROLLBAR
    // ═══════════════════════════════════════════════════════════════════════════
    /// Scrollbar thumb (main indicator)
    pub scrollbar_thumb: Color,
    /// Scrollbar track (background)
    pub scrollbar_track: Color,
    /// Scrollbar arrows
    pub scrollbar_arrows: Color,
}

impl SemanticColors {
    /// Create Cosmic Dark semantic colors 🌌
    ///
    /// Abyss Dark — near-black hacking aesthetic with neon accents.
    /// Inspired by NovaNet's deep space palette pushed even darker.
    /// Background at (8,8,14) for maximum contrast with neon highlights.
    pub fn cosmic_dark(_palette: &ColorPalette) -> Self {
        // ═══════════════════════════════════════════════════════════════════
        // ABYSS DARK PALETTE  — hacking terminal aesthetic
        // ═══════════════════════════════════════════════════════════════════
        // Backgrounds — near-black with blue undertone
        const VOID: Color = Color::Rgb(4, 4, 8); // deepest black
        const ABYSS: Color = Color::Rgb(8, 8, 14); // #08080e - canvas
        const DEEP: Color = Color::Rgb(12, 12, 20); // #0c0c14 - panels
        const SURFACE0: Color = Color::Rgb(18, 20, 30); // borders
        const SURFACE1: Color = Color::Rgb(25, 28, 42); // hover
        const SURFACE2: Color = Color::Rgb(32, 36, 52); // active/selected

        // Text — high contrast on near-black
        const TEXT: Color = Color::Rgb(200, 210, 230); // cool white-blue
        const SUBTEXT1: Color = Color::Rgb(150, 160, 180); // muted
        const SUBTEXT0: Color = Color::Rgb(90, 100, 120); // hints
        const OVERLAY0: Color = Color::Rgb(55, 60, 75); // disabled

        // Accents — neon glow on dark void
        const ELECTRIC: Color = Color::Rgb(80, 140, 255); // ⚡ electric blue
        const NEON_CYAN: Color = Color::Rgb(0, 220, 200); // 🌌 neon cyan
        const NEON_PURPLE: Color = Color::Rgb(160, 100, 255); // 🦋 purple neon

        // Verbs — vivid neon on void background
        const MAUVE: Color = Color::Rgb(180, 120, 255); // infer - purple glow
        const PEACH: Color = Color::Rgb(255, 160, 80); // exec - warm neon
        const CYAN: Color = Color::Rgb(0, 200, 220); // fetch - cyan neon
        const TEAL: Color = Color::Rgb(0, 220, 150); // invoke - teal neon
        const PINK: Color = Color::Rgb(255, 100, 150); // agent - hot pink

        // Status — vivid neon signals
        const GREEN: Color = Color::Rgb(0, 230, 120); // neon green
        const YELLOW: Color = Color::Rgb(255, 200, 0); // amber
        const RED: Color = Color::Rgb(255, 60, 80); // neon red

        Self {
            // Backgrounds — the void
            bg_primary: ABYSS,     // #08080e - near-black canvas
            bg_secondary: DEEP,    // #0c0c14 - elevated panels
            bg_tertiary: SURFACE0, // cards, modals
            bg_hover: SURFACE1,    // hover highlight
            bg_active: SURFACE2,   // active/selected

            // Text — sharp on the void
            text_primary: TEXT,       // cool white-blue
            text_secondary: SUBTEXT1, // muted descriptions
            text_muted: SUBTEXT0,     // hints, placeholders
            text_disabled: OVERLAY0,  // disabled
            text_inverse: VOID,       // on light surfaces

            // Borders — barely visible, glow on focus
            border_default: SURFACE0, // subtle panel edges
            border_focused: ELECTRIC, // ⚡ electric blue glow
            border_subtle: DEEP,      // nearly invisible dividers

            // Accents — neon on void
            accent_primary: ELECTRIC,     // ⚡ electric blue
            accent_secondary: NEON_CYAN,  // 🌌 neon cyan
            accent_tertiary: NEON_PURPLE, // 🦋 purple neon

            // Status — neon signals
            status_success: GREEN,  // neon green
            status_warning: YELLOW, // amber
            status_error: RED,      // neon red
            status_info: ELECTRIC,  // electric blue

            // Verbs — vivid neon
            verb_infer: MAUVE, // ⚡ LLM (purple glow)
            verb_exec: PEACH,  // 📟 shell (warm neon)
            verb_fetch: CYAN,  // 🛰️ HTTP (cyan neon)
            verb_invoke: TEAL, // 🔌 MCP (teal neon)
            verb_agent: PINK,  // 🐔 agent (hot pink)

            // Scrollbar — subtle
            scrollbar_thumb: SURFACE1,
            scrollbar_track: DEEP,
            scrollbar_arrows: SUBTEXT0,
        }
    }

    /// Create Cosmic Light semantic colors 🌅
    ///
    /// Aurora borealis on light background
    pub fn cosmic_light(palette: &ColorPalette) -> Self {
        Self {
            // Backgrounds - Light slate
            bg_primary: palette.slate_50,    // #f8fafc - Clean white
            bg_secondary: palette.slate_100, // #f1f5f9 - Elevated
            bg_tertiary: palette.slate_200,  // #e2e8f0 - Cards
            bg_hover: palette.slate_100,     // #f1f5f9 - Hover
            bg_active: palette.violet_100,   // #ede9fe - Active (violet tint)

            // Text - Dark for contrast
            text_primary: palette.slate_900,   // #0f172a - Dark
            text_secondary: palette.slate_600, // #475569 - Muted
            text_muted: palette.slate_400,     // #94a3b8 - Hints
            text_disabled: palette.slate_300,  // #cbd5e1 - Disabled
            text_inverse: palette.slate_50,    // #f8fafc - On dark

            // Borders - Visible definition
            border_default: palette.slate_300,  // #cbd5e1
            border_focused: palette.violet_600, // #7c3aed - Violet focus
            border_subtle: palette.slate_200,   // #e2e8f0

            // Accents - Deeper for contrast
            accent_primary: palette.violet_600, // #7c3aed - 🦋 Brand
            accent_secondary: palette.cyan_600, // #0891b2 - 🌌 Glow
            accent_tertiary: palette.pink_600,  // #db2777 - Nebula

            // Status - Darker for light mode
            status_success: palette.emerald_600, // #059669
            status_warning: palette.amber_600,   // #d97706
            status_error: palette.red_600,       // #dc2626
            status_info: palette.blue_600,       // #2563eb

            // Verbs - Slightly darker for readability
            verb_infer: palette.violet_600,   // #7c3aed
            verb_exec: palette.amber_600,     // #d97706
            verb_fetch: palette.cyan_600,     // #0891b2
            verb_invoke: palette.emerald_600, // #059669
            verb_agent: palette.rose_600,     // #e11d48

            // Scrollbar - Subtle
            scrollbar_thumb: palette.slate_400,  // #94a3b8
            scrollbar_track: palette.slate_200,  // #e2e8f0
            scrollbar_arrows: palette.slate_500, // #64748b
        }
    }

    /// Create Cosmic Violet semantic colors 🦋
    ///
    /// Full violet immersion for Nika brand
    pub fn cosmic_violet(palette: &ColorPalette) -> Self {
        Self {
            // Backgrounds - Violet-tinted
            bg_primary: palette.violet_950,   // #2e1065 - Deep violet
            bg_secondary: palette.violet_900, // #4c1d95 - Elevated
            bg_tertiary: palette.violet_800,  // #5b21b6 - Cards
            bg_hover: palette.violet_800,     // #5b21b6 - Hover
            bg_active: palette.cyan_900,      // #164e63 - Active (cyan pop)

            // Text - Light for contrast
            text_primary: palette.violet_50,    // #f5f3ff - Bright
            text_secondary: palette.violet_200, // #ddd6fe - Muted
            text_muted: palette.violet_400,     // #a78bfa - Hints
            text_disabled: palette.violet_600,  // #7c3aed - Disabled
            text_inverse: palette.violet_950,   // #2e1065 - On light

            // Borders - Violet spectrum
            border_default: palette.violet_700, // #6d28d9
            border_focused: palette.cyan_400,   // #22d3ee - Cyan focus (contrast)
            border_subtle: palette.violet_800,  // #5b21b6

            // Accents - Complementary
            accent_primary: palette.cyan_400, // #22d3ee - Inverse accent
            accent_secondary: palette.pink_400, // #f472b6 - Warm
            accent_tertiary: palette.violet_300, // #c4b5fd - Soft

            // Status - Adjusted for violet bg
            status_success: palette.emerald_400, // #34d399 - Brighter
            status_warning: palette.amber_400,   // #fbbf24 - Brighter
            status_error: palette.rose_400,      // #fb7185 - Brighter
            status_info: palette.cyan_400,       // #22d3ee - Brighter

            // Verbs - High contrast on violet
            verb_infer: palette.violet_300, // #c4b5fd - Lighter violet
            verb_exec: palette.amber_400,   // #fbbf24
            verb_fetch: palette.cyan_400,   // #22d3ee
            verb_invoke: palette.emerald_400, // #34d399
            verb_agent: palette.rose_400,   // #fb7185

            // Scrollbar - Cyan accent
            scrollbar_thumb: palette.cyan_500,    // #06b6d4
            scrollbar_track: palette.violet_800,  // #5b21b6
            scrollbar_arrows: palette.violet_200, // #ddd6fe
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // HELPER METHODS
    // ═══════════════════════════════════════════════════════════════════════════

    /// Get verb color by name
    pub fn verb(&self, name: &str) -> Color {
        match name.to_lowercase().as_str() {
            "infer" => self.verb_infer,
            "exec" => self.verb_exec,
            "fetch" => self.verb_fetch,
            "invoke" => self.verb_invoke,
            "agent" => self.verb_agent,
            _ => self.text_muted,
        }
    }

    /// Get status color by state
    pub fn status(&self, state: &str) -> Color {
        match state.to_lowercase().as_str() {
            "success" | "completed" | "done" => self.status_success,
            "warning" | "pending" | "running" => self.status_warning,
            "error" | "failed" => self.status_error,
            "info" => self.status_info,
            _ => self.text_muted,
        }
    }

    /// Check if this is a dark theme (for contrast calculations)
    pub fn is_dark(&self) -> bool {
        // Extract RGB from bg_primary
        if let Color::Rgb(r, g, b) = self.bg_primary {
            // Calculate relative luminance
            let luminance = (r as u32 * 299 + g as u32 * 587 + b as u32 * 114) / 1000;
            luminance < 128
        } else {
            true // Default to dark
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
    fn test_cosmic_dark() {
        let palette = ColorPalette::tailwind();
        let semantic = SemanticColors::cosmic_dark(&palette);

        assert_eq!(semantic.bg_primary, Color::Rgb(8, 8, 14)); // Abyss Dark
        assert_eq!(semantic.accent_primary, Color::Rgb(80, 140, 255)); // Electric blue
        assert!(semantic.is_dark());
    }

    #[test]
    fn test_cosmic_light() {
        let palette = ColorPalette::tailwind();
        let semantic = SemanticColors::cosmic_light(&palette);

        assert_eq!(semantic.bg_primary, palette.slate_50);
        assert_eq!(semantic.accent_primary, palette.violet_600);
        assert!(!semantic.is_dark());
    }

    #[test]
    fn test_cosmic_violet() {
        let palette = ColorPalette::tailwind();
        let semantic = SemanticColors::cosmic_violet(&palette);

        assert_eq!(semantic.bg_primary, palette.violet_950);
        assert!(semantic.is_dark());
    }

    #[test]
    fn test_verb_helper() {
        let palette = ColorPalette::tailwind();
        let semantic = SemanticColors::cosmic_dark(&palette);

        assert_eq!(semantic.verb("infer"), Color::Rgb(180, 120, 255)); // Neon purple
        assert_eq!(semantic.verb("exec"), Color::Rgb(255, 160, 80)); // Neon peach
        assert_eq!(semantic.verb("fetch"), Color::Rgb(0, 200, 220)); // Neon cyan
        assert_eq!(semantic.verb("invoke"), Color::Rgb(0, 220, 150)); // Neon teal
        assert_eq!(semantic.verb("agent"), Color::Rgb(255, 100, 150)); // Hot pink
        assert_eq!(semantic.verb("unknown"), semantic.text_muted);
    }

    #[test]
    fn test_status_helper() {
        let palette = ColorPalette::tailwind();
        let semantic = SemanticColors::cosmic_dark(&palette);

        assert_eq!(semantic.status("success"), Color::Rgb(0, 230, 120)); // Neon green
        assert_eq!(semantic.status("completed"), Color::Rgb(0, 230, 120)); // Neon green
        assert_eq!(semantic.status("warning"), Color::Rgb(255, 200, 0)); // Amber
        assert_eq!(semantic.status("error"), Color::Rgb(255, 60, 80)); // Neon red
        assert_eq!(semantic.status("info"), Color::Rgb(80, 140, 255)); // Electric blue
    }

    #[test]
    fn test_all_themes_have_complete_verbs() {
        let palette = ColorPalette::tailwind();

        for semantic in [
            SemanticColors::cosmic_dark(&palette),
            SemanticColors::cosmic_light(&palette),
            SemanticColors::cosmic_violet(&palette),
        ] {
            // All verb colors should be different
            let verbs = [
                semantic.verb_infer,
                semantic.verb_exec,
                semantic.verb_fetch,
                semantic.verb_invoke,
                semantic.verb_agent,
            ];

            for (i, v1) in verbs.iter().enumerate() {
                for (j, v2) in verbs.iter().enumerate() {
                    if i != j {
                        assert_ne!(v1, v2, "Verb colors must be distinct");
                    }
                }
            }
        }
    }
}
