// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Verb-specific colors for DAG visualization.
//!
//! Each Nika verb (infer, exec, fetch, invoke, agent) has a distinct color
//! with multiple intensity variants: full, glow, muted, and subtle.

use ratatui::style::Color;

use super::super::icons::verb as icons_verb;
use super::super::tokens::TokenResolver;

/// Verb-specific colors for DAG visualization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VerbColor {
    Infer,  // Violet #8B5CF6
    Exec,   // Amber #F59E0B
    Fetch,  // Cyan #06B6D4
    Invoke, // Emerald #10B981
    Agent,  // Rose #F43F5E
    Spawn,  // Rose 300 #FDA4AF (spawned sub-agent)
    User,   // Sky #0EA5E9 (user message in chat)
}

impl VerbColor {
    /// Get the RGB color for this verb
    pub fn rgb(&self) -> Color {
        match self {
            Self::Infer => Color::Rgb(139, 92, 246),  // Violet 500
            Self::Exec => Color::Rgb(245, 158, 11),   // Amber 500
            Self::Fetch => Color::Rgb(6, 182, 212),   // Cyan 500
            Self::Invoke => Color::Rgb(16, 185, 129), // Emerald 500
            Self::Agent => Color::Rgb(244, 63, 94),   // Rose 500
            Self::Spawn => Color::Rgb(253, 164, 175), // Rose 300
            Self::User => Color::Rgb(14, 165, 233),   // Sky 500
        }
    }

    /// Get glow version (brighter for active/hover states)
    pub fn glow(&self) -> Color {
        match self {
            Self::Infer => Color::Rgb(167, 139, 250), // Violet 400
            Self::Exec => Color::Rgb(251, 191, 36),   // Amber 400
            Self::Fetch => Color::Rgb(34, 211, 238),  // Cyan 400
            Self::Invoke => Color::Rgb(52, 211, 153), // Emerald 400
            Self::Agent => Color::Rgb(251, 113, 133), // Rose 400
            Self::Spawn => Color::Rgb(254, 205, 211), // Rose 200
            Self::User => Color::Rgb(56, 189, 248),   // Sky 400
        }
    }

    /// Get muted version (50% opacity simulation)
    pub fn muted(&self) -> Color {
        match self {
            Self::Infer => Color::Rgb(97, 64, 171),
            Self::Exec => Color::Rgb(171, 110, 8),
            Self::Fetch => Color::Rgb(4, 127, 148),
            Self::Invoke => Color::Rgb(11, 129, 90),
            Self::Agent => Color::Rgb(170, 44, 66),
            Self::Spawn => Color::Rgb(177, 115, 122), // ~0.7x Rose 300
            Self::User => Color::Rgb(10, 115, 163),   // ~0.7x Sky 500
        }
    }

    /// Get subtle version (very muted for backgrounds)
    pub fn subtle(&self) -> Color {
        match self {
            Self::Infer => Color::Rgb(55, 48, 83),  // Violet 950/50
            Self::Exec => Color::Rgb(69, 53, 18),   // Amber 950/50
            Self::Fetch => Color::Rgb(22, 57, 67),  // Cyan 950/50
            Self::Invoke => Color::Rgb(20, 61, 47), // Emerald 950/50
            Self::Agent => Color::Rgb(68, 32, 41),  // Rose 950/50
            Self::Spawn => Color::Rgb(70, 45, 48),  // Rose 950/50 (lighter)
            Self::User => Color::Rgb(12, 51, 70),   // Sky 950/50
        }
    }

    /// Get icon for this verb (delegates to icons module)
    ///
    /// Canonical icons from CLAUDE.md:
    /// - Infer (LLM generation)
    /// - Exec (Shell command)
    /// - Fetch (HTTP request)
    /// - Invoke (MCP tool)
    /// - Agent (Agentic loop)
    /// - Spawn (Spawned sub-agent)
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Infer => icons_verb::INFER,
            Self::Exec => icons_verb::EXEC,
            Self::Fetch => icons_verb::FETCH,
            Self::Invoke => icons_verb::INVOKE,
            Self::Agent => icons_verb::AGENT,
            Self::Spawn => icons_verb::SUBAGENT,
            Self::User => "\u{1F464}",
        }
    }

    /// Get icon for subagent (spawned via spawn_agent)
    pub fn subagent_icon() -> &'static str {
        icons_verb::SUBAGENT
    }

    /// Get RGB tuple for gradient interpolation
    pub fn rgb_tuple(&self) -> (u8, u8, u8) {
        match self {
            Self::Infer => (139, 92, 246),  // Violet 500
            Self::Exec => (245, 158, 11),   // Amber 500
            Self::Fetch => (6, 182, 212),   // Cyan 500
            Self::Invoke => (16, 185, 129), // Emerald 500
            Self::Agent => (244, 63, 94),   // Rose 500
            Self::Spawn => (253, 164, 175), // Rose 300
            Self::User => (14, 165, 233),   // Sky 500
        }
    }

    /// Get glow RGB tuple for gradient interpolation
    pub fn glow_tuple(&self) -> (u8, u8, u8) {
        match self {
            Self::Infer => (167, 139, 250), // Violet 400
            Self::Exec => (251, 191, 36),   // Amber 400
            Self::Fetch => (34, 211, 238),  // Cyan 400
            Self::Invoke => (52, 211, 153), // Emerald 400
            Self::Agent => (251, 113, 133), // Rose 400
            Self::Spawn => (254, 205, 211), // Rose 200
            Self::User => (56, 189, 248),   // Sky 400
        }
    }

    /// Get muted RGB tuple for gradient interpolation
    pub fn muted_tuple(&self) -> (u8, u8, u8) {
        match self {
            Self::Infer => (97, 64, 171),   // Muted violet
            Self::Exec => (171, 110, 8),    // Muted amber
            Self::Fetch => (4, 127, 148),   // Muted cyan
            Self::Invoke => (11, 129, 90),  // Muted emerald
            Self::Agent => (170, 44, 66),   // Muted rose
            Self::Spawn => (177, 115, 122), // Muted rose 300
            Self::User => (10, 115, 163),   // Muted sky
        }
    }

    /// Get ASCII-safe icon for terminals without emoji support
    /// (delegates to icons module)
    pub fn icon_ascii(&self) -> &'static str {
        match self {
            Self::Infer => icons_verb::INFER_ASCII,
            Self::Exec => icons_verb::EXEC_ASCII,
            Self::Fetch => icons_verb::FETCH_ASCII,
            Self::Invoke => icons_verb::INVOKE_ASCII,
            Self::Agent => icons_verb::AGENT_ASCII,
            Self::Spawn => icons_verb::SUBAGENT_ASCII,
            Self::User => "[U]",
        }
    }

    /// Get hex color string for CSS/HTML
    pub fn hex(&self) -> &'static str {
        match self {
            Self::Infer => "#8b5cf6",
            Self::Exec => "#f59e0b",
            Self::Fetch => "#06b6d4",
            Self::Invoke => "#10b981",
            Self::Agent => "#f43f5e",
            Self::Spawn => "#fda4af",
            Self::User => "#0ea5e9",
        }
    }

    /// Get uppercase label for this verb
    pub fn label(&self) -> &'static str {
        match self {
            Self::Infer => "INFER",
            Self::Exec => "EXEC",
            Self::Fetch => "FETCH",
            Self::Invoke => "INVOKE",
            Self::Agent => "AGENT",
            Self::Spawn => "SPAWN",
            Self::User => "USER",
        }
    }

    /// Get icon and label combined (e.g., "INFER")
    pub fn icon_label(&self) -> String {
        format!("{} {}", self.icon(), self.label())
    }

    /// Get border color (darker variant for outlines)
    pub fn border_rgb(&self) -> Color {
        match self {
            Self::Infer => Color::Rgb(124, 58, 237),  // Violet 600
            Self::Exec => Color::Rgb(217, 119, 6),    // Amber 600
            Self::Fetch => Color::Rgb(8, 145, 178),   // Cyan 600
            Self::Invoke => Color::Rgb(5, 150, 105),  // Emerald 600
            Self::Agent => Color::Rgb(225, 29, 72),   // Rose 600
            Self::Spawn => Color::Rgb(251, 113, 133), // Rose 400
            Self::User => Color::Rgb(2, 132, 199),    // Sky 600
        }
    }

    /// Get verb color from TokenResolver
    ///
    /// Use this for new code that uses the Cosmic design system.
    pub fn color_from_resolver(&self, resolver: &TokenResolver) -> Color {
        match self {
            Self::Infer => resolver.verb_infer(),
            Self::Exec => resolver.verb_exec(),
            Self::Fetch => resolver.verb_fetch(),
            Self::Invoke => resolver.verb_invoke(),
            Self::Agent => resolver.verb_agent(),
            // Spawn uses direct RGB (Rose 300) - lighter than Agent
            Self::Spawn => self.rgb(),
            // User uses direct RGB (Sky 500)
            Self::User => self.rgb(),
        }
    }

    /// Parse from verb name string
    pub fn from_verb(verb: &str) -> Self {
        match verb.to_lowercase().as_str() {
            "infer" => Self::Infer,
            "exec" => Self::Exec,
            "fetch" => Self::Fetch,
            "invoke" => Self::Invoke,
            "agent" => Self::Agent,
            "spawn" | "subagent" => Self::Spawn,
            _ => Self::Infer, // default
        }
    }

    /// Get animated color based on frame (for pulsing effects)
    pub fn animated(&self, frame: u8) -> Color {
        // Alternate between normal and glow every 8 frames
        if (frame / 8) % 2 == 0 {
            self.rgb()
        } else {
            self.glow()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verb_color_rgb_returns_correct_colors() {
        assert_eq!(VerbColor::Infer.rgb(), Color::Rgb(139, 92, 246)); // Violet
        assert_eq!(VerbColor::Exec.rgb(), Color::Rgb(245, 158, 11)); // Amber
        assert_eq!(VerbColor::Fetch.rgb(), Color::Rgb(6, 182, 212)); // Cyan
        assert_eq!(VerbColor::Invoke.rgb(), Color::Rgb(16, 185, 129)); // Emerald
        assert_eq!(VerbColor::Agent.rgb(), Color::Rgb(244, 63, 94)); // Rose
    }

    #[test]
    fn test_verb_color_muted_returns_darker_colors() {
        // Muted colors should be different from full saturation
        assert_ne!(VerbColor::Infer.muted(), VerbColor::Infer.rgb());
        assert_ne!(VerbColor::Exec.muted(), VerbColor::Exec.rgb());
        assert_ne!(VerbColor::Fetch.muted(), VerbColor::Fetch.rgb());
        assert_ne!(VerbColor::Invoke.muted(), VerbColor::Invoke.rgb());
        assert_ne!(VerbColor::Agent.muted(), VerbColor::Agent.rgb());

        // Verify actual muted values
        assert_eq!(VerbColor::Infer.muted(), Color::Rgb(97, 64, 171));
        assert_eq!(VerbColor::Agent.muted(), Color::Rgb(170, 44, 66));
    }

    #[test]
    fn test_verb_color_icons() {
        // Canonical icons
        assert_eq!(VerbColor::Infer.icon(), "\u{26A1}");
        assert_eq!(VerbColor::Exec.icon(), "\u{1F4DF}");
        assert_eq!(VerbColor::Invoke.icon(), "\u{1F50C}");
        assert_eq!(VerbColor::Agent.icon(), "\u{1F414}");
        assert_eq!(VerbColor::subagent_icon(), "\u{1F424}");
    }

    #[test]
    fn test_verb_color_from_verb_string() {
        assert_eq!(VerbColor::from_verb("infer"), VerbColor::Infer);
        assert_eq!(VerbColor::from_verb("exec"), VerbColor::Exec);
        assert_eq!(VerbColor::from_verb("fetch"), VerbColor::Fetch);
        assert_eq!(VerbColor::from_verb("invoke"), VerbColor::Invoke);
        assert_eq!(VerbColor::from_verb("agent"), VerbColor::Agent);
    }

    #[test]
    fn test_verb_color_from_verb_case_insensitive() {
        assert_eq!(VerbColor::from_verb("INFER"), VerbColor::Infer);
        assert_eq!(VerbColor::from_verb("Exec"), VerbColor::Exec);
        assert_eq!(VerbColor::from_verb("FeTcH"), VerbColor::Fetch);
    }

    #[test]
    fn test_verb_color_from_verb_unknown_defaults_to_infer() {
        assert_eq!(VerbColor::from_verb("unknown"), VerbColor::Infer);
        assert_eq!(VerbColor::from_verb(""), VerbColor::Infer);
        assert_eq!(VerbColor::from_verb("transform"), VerbColor::Infer);
    }

    #[test]
    fn test_verb_color_all_variants_have_distinct_colors() {
        let colors = [
            VerbColor::Infer.rgb(),
            VerbColor::Exec.rgb(),
            VerbColor::Fetch.rgb(),
            VerbColor::Invoke.rgb(),
            VerbColor::Agent.rgb(),
        ];

        // All colors should be unique
        for i in 0..colors.len() {
            for j in (i + 1)..colors.len() {
                assert_ne!(
                    colors[i], colors[j],
                    "Verb colors {} and {} are identical",
                    i, j
                );
            }
        }
    }

    #[test]
    fn test_verb_color_glow_is_brighter() {
        // Glow colors should be different from normal RGB
        assert_ne!(VerbColor::Infer.glow(), VerbColor::Infer.rgb());
        assert_ne!(VerbColor::Exec.glow(), VerbColor::Exec.rgb());
        assert_ne!(VerbColor::Fetch.glow(), VerbColor::Fetch.rgb());
        assert_ne!(VerbColor::Invoke.glow(), VerbColor::Invoke.rgb());
        assert_ne!(VerbColor::Agent.glow(), VerbColor::Agent.rgb());

        // Verify specific glow values
        assert_eq!(VerbColor::Infer.glow(), Color::Rgb(167, 139, 250)); // Violet-400
        assert_eq!(VerbColor::Exec.glow(), Color::Rgb(251, 191, 36)); // Amber-400
    }

    #[test]
    fn test_verb_color_subtle_is_darker() {
        // Subtle colors should be different from normal and muted
        assert_ne!(VerbColor::Infer.subtle(), VerbColor::Infer.rgb());
        assert_ne!(VerbColor::Infer.subtle(), VerbColor::Infer.muted());
        assert_ne!(VerbColor::Agent.subtle(), VerbColor::Agent.rgb());
    }

    #[test]
    fn test_verb_color_animated_alternates() {
        // Frame 0-7 should return normal color
        assert_eq!(VerbColor::Infer.animated(0), VerbColor::Infer.rgb());
        assert_eq!(VerbColor::Infer.animated(7), VerbColor::Infer.rgb());

        // Frame 8-15 should return glow color
        assert_eq!(VerbColor::Infer.animated(8), VerbColor::Infer.glow());
        assert_eq!(VerbColor::Infer.animated(15), VerbColor::Infer.glow());

        // Frame 16-23 should return normal color again
        assert_eq!(VerbColor::Infer.animated(16), VerbColor::Infer.rgb());
    }

    #[test]
    fn test_verb_color_icon_ascii() {
        assert_eq!(VerbColor::Infer.icon_ascii(), "[I]");
        assert_eq!(VerbColor::Exec.icon_ascii(), "[X]");
        assert_eq!(VerbColor::Fetch.icon_ascii(), "[F]");
        assert_eq!(VerbColor::Invoke.icon_ascii(), "[V]");
        assert_eq!(VerbColor::Agent.icon_ascii(), "[A]");
    }

    #[test]
    fn test_verb_color_all_methods_return_rgb() {
        let verbs = [
            VerbColor::Infer,
            VerbColor::Exec,
            VerbColor::Fetch,
            VerbColor::Invoke,
            VerbColor::Agent,
        ];

        for verb in verbs {
            // All should return RGB colors
            match verb.rgb() {
                Color::Rgb(_, _, _) => (),
                _ => panic!("Expected RGB color for {:?}", verb),
            }

            match verb.glow() {
                Color::Rgb(_, _, _) => (),
                _ => panic!("Expected RGB color for glow: {:?}", verb),
            }

            match verb.muted() {
                Color::Rgb(_, _, _) => (),
                _ => panic!("Expected RGB color for muted: {:?}", verb),
            }

            match verb.subtle() {
                Color::Rgb(_, _, _) => (),
                _ => panic!("Expected RGB color for subtle: {:?}", verb),
            }
        }
    }

    #[test]
    fn test_verbcolor_spawn_variant_rgb() {
        let color = VerbColor::Spawn;
        assert_eq!(color.rgb(), Color::Rgb(253, 164, 175)); // Rose 300
    }

    #[test]
    fn test_verbcolor_spawn_variant_glow() {
        let color = VerbColor::Spawn;
        assert_eq!(color.glow(), Color::Rgb(254, 205, 211)); // Rose 200
    }

    #[test]
    fn test_verbcolor_spawn_hex() {
        assert_eq!(VerbColor::Spawn.hex(), "#fda4af");
    }

    #[test]
    fn test_verbcolor_spawn_label() {
        assert_eq!(VerbColor::Spawn.label(), "SPAWN");
    }

    #[test]
    fn test_verbcolor_spawn_border_rgb() {
        let color = VerbColor::Spawn;
        assert_eq!(color.border_rgb(), Color::Rgb(251, 113, 133)); // Rose 400
    }

    #[test]
    fn test_verbcolor_spawn_icon() {
        assert_eq!(VerbColor::Spawn.icon(), "\u{1F424}"); // Subagent chick
    }

    #[test]
    fn test_verbcolor_has_all_methods_for_spawn() {
        let spawn = VerbColor::Spawn;
        // Verify all methods work
        let _ = spawn.rgb();
        let _ = spawn.glow();
        let _ = spawn.muted();
        let _ = spawn.subtle();
        let _ = spawn.hex();
        let _ = spawn.icon();
        let _ = spawn.label();
        let _ = spawn.border_rgb();
        let _ = spawn.rgb_tuple();
        let _ = spawn.glow_tuple();
        let _ = spawn.muted_tuple();
        let _ = spawn.icon_ascii();
        let _ = spawn.animated(0);
    }

    #[test]
    fn test_verbcolor_from_verb_spawn() {
        assert_eq!(VerbColor::from_verb("spawn"), VerbColor::Spawn);
        assert_eq!(VerbColor::from_verb("SPAWN"), VerbColor::Spawn);
        assert_eq!(VerbColor::from_verb("Spawn"), VerbColor::Spawn);
    }

    #[test]
    fn test_verbcolor_all_six_variants_distinct() {
        let colors = [
            VerbColor::Infer.rgb(),
            VerbColor::Exec.rgb(),
            VerbColor::Fetch.rgb(),
            VerbColor::Invoke.rgb(),
            VerbColor::Agent.rgb(),
            VerbColor::Spawn.rgb(),
        ];

        // All colors should be unique
        for i in 0..colors.len() {
            for j in (i + 1)..colors.len() {
                assert_ne!(
                    colors[i], colors[j],
                    "Verb colors {} and {} are identical",
                    i, j
                );
            }
        }
    }
}
