// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! TUI Theme - Cosmic Tailwind Design System
//!
//! Color palette using Tailwind CSS colors with Cosmic accent themes.
//! Provides consistent styling across all TUI components.
//!
//! # Theme Architecture
//!
//! ```text
//! ┌──────────────────┐     ┌──────────────────┐     ┌──────────────────┐
//! │   CosmicTheme    │ ──► │   TokenResolver  │ ──► │  SemanticColors  │
//! │   (Adapter)      │     │      │     │  ColorPalette    │
//! └──────────────────┘     └──────────────────┘     └──────────────────┘
//!          │
//!          ▼
//! ┌──────────────────┐
//! │      Theme       │ (Convenience API)
//! │                  │
//! └──────────────────┘
//! ```
//!
//! # Theme Variants
//!
//! - **Cosmic Dark**: Slate-900 background, high contrast (default)
//! - **Cosmic Light**: Slate-50 background, inverted colors
//! - **Cosmic Violet**: Violet-950 background, accent theme
//!
//! # ColorMode Detection
//!
//! Automatically detects terminal color capabilities:
//! - TrueColor (24-bit): Modern terminals with COLORTERM=truecolor
//! - Color256 (8-bit): Terminals with TERM containing "256color"
//! - Color16 (4-bit): Basic terminal colors
//!
//! ```rust,ignore
//! let mode = ColorMode::detect();
//! let color = mode.adapt_color(Color::Rgb(139, 92, 246));
//! ```

mod color_mode;
pub mod palette;
mod status;
mod verb;

// Re-export all public types at module level
pub use color_mode::ColorMode;
pub use palette::solarized;
pub use status::{MissionPhase, TaskStatus};
pub use verb::VerbColor;

use ratatui::style::{Color, Modifier, Style};

// ═══════════════════════════════════════════════════════════════════════════
// THEME MODE
// ═══════════════════════════════════════════════════════════════════════════

/// Theme mode selector (TIER 2.4)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
    Solarized,
}

impl ThemeMode {
    /// Toggle between dark and light
    pub fn toggle(&self) -> Self {
        match self {
            Self::Dark => Self::Light,
            Self::Light => Self::Dark,
            Self::Solarized => Self::Dark,
        }
    }

    /// Cycle through all themes
    pub fn cycle(&self) -> Self {
        match self {
            Self::Dark => Self::Light,
            Self::Light => Self::Solarized,
            Self::Solarized => Self::Dark,
        }
    }

    /// Get the theme for this mode
    pub fn theme(&self) -> Theme {
        match self {
            Self::Dark => Theme::dark(),
            Self::Light => Theme::light(),
            Self::Solarized => Theme::solarized(),
        }
    }

    /// Get display label
    pub fn label(&self) -> &'static str {
        match self {
            Self::Dark => "Dark",
            Self::Light => "Light",
            Self::Solarized => "Solarized",
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// THEME STRUCT
// ═══════════════════════════════════════════════════════════════════════════

/// Nika TUI color theme (dark Solarized palette with cosmic accents)
#[derive(Debug, Clone)]
pub struct Theme {
    // ═══════════════════════════════════════════
    // REALMS
    // ═══════════════════════════════════════════
    pub realm_shared: Color,
    pub realm_org: Color,

    // ═══════════════════════════════════════════
    // TRAITS (Data Origin)
    // ═══════════════════════════════════════════
    pub trait_defined: Color,
    pub trait_authored: Color,
    pub trait_imported: Color,
    pub trait_generated: Color,
    pub trait_retrieved: Color,

    // ═══════════════════════════════════════════
    // TASK STATUS
    // ═══════════════════════════════════════════
    pub status_pending: Color,
    pub status_running: Color,
    pub status_success: Color,
    pub status_failed: Color,
    pub status_paused: Color,

    // ═══════════════════════════════════════════
    // MISSION PHASES (Space Theme)
    // ═══════════════════════════════════════════
    pub phase_launch: Color,     // First task executing (pink)
    pub phase_orbital: Color,    // Nominal execution (blue)
    pub phase_rendezvous: Color, // MCP tool invocation (violet)

    // ═══════════════════════════════════════════
    // MCP TOOLS
    // ═══════════════════════════════════════════
    pub mcp_describe: Color,
    pub mcp_context: Color,
    pub mcp_search: Color,
    pub mcp_audit: Color,
    pub mcp_write: Color,

    // ═══════════════════════════════════════════
    // UI ELEMENTS
    // ═══════════════════════════════════════════
    pub border_normal: Color,
    pub border_focused: Color,
    pub text: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub background: Color,
    pub highlight: Color,
    pub selection: Color,

    // ═══════════════════════════════════════════
    // GIT GUTTER
    // ═══════════════════════════════════════════
    pub git_added: Color,    // Green for added lines
    pub git_modified: Color, // Yellow/Orange for modified lines
    pub git_deleted: Color,  // Red for deleted markers

    // ═══════════════════════════════════════════
    // SCROLLBAR
    // ═══════════════════════════════════════════
    pub scrollbar_thumb: Color,  // Main thumb color (visible indicator)
    pub scrollbar_track: Color,  // Track background (subtle)
    pub scrollbar_arrows: Color, // Arrow symbols (accent)

    // ═══════════════════════════════════════════
    // DIAGNOSTICS
    // ═══════════════════════════════════════════
    pub diagnostic_error: Color,
    pub diagnostic_warning: Color,
    pub diagnostic_hint: Color,

    // ═══════════════════════════════════════════
    // POPUP / OVERLAY
    // ═══════════════════════════════════════════
    pub popup_bg: Color,
    pub popup_border: Color,
    pub popup_selected: Color,

    // ═══════════════════════════════════════════
    // VERB GLOW (brighter variant for active/hover)
    // ═══════════════════════════════════════════
    pub verb_infer_glow: Color,
    pub verb_exec_glow: Color,
    pub verb_fetch_glow: Color,
    pub verb_invoke_glow: Color,
    pub verb_agent_glow: Color,

    // ═══════════════════════════════════════════
    // TREE WIDGET
    // ═══════════════════════════════════════════
    pub tree_directory: Color,
    pub tree_file: Color,
    pub tree_hidden: Color,
    pub tree_ecosystem: Color,
    pub tree_ecosystem_glow: Color,
    pub tree_indent_guide: Color,

    // ═══════════════════════════════════════════
    // SYNTAX HIGHLIGHTING
    // ═══════════════════════════════════════════
    pub syntax_keyword: Color,
    pub syntax_string: Color,
    pub syntax_number: Color,
    pub syntax_comment: Color,
    pub syntax_verb: Color,
    pub syntax_template: Color,
    pub syntax_mcp_server: Color,
    pub syntax_task_id: Color,

    // HEADER / STATUS BAR
    /// Background for header and status bar (slightly lighter than main bg)
    pub header_bg: Color,
    /// Pulse color A for parsing/validating phase animation
    pub status_parsing_a: Color,
    /// Pulse color B for parsing/validating phase animation
    pub status_parsing_b: Color,
    /// Pulse color A for executing phase animation
    pub status_executing_a: Color,
    /// Pulse color B for executing phase animation
    pub status_executing_b: Color,
}

impl Default for Theme {
    /// Default theme is Cosmic Dark
    ///
    /// Uses Tailwind CSS Slate palette with Cosmic accents.
    fn default() -> Self {
        // Delegate to cosmic_dark() which uses TokenResolver
        Self::cosmic_dark()
    }
}

impl Theme {
    /// Create dark theme (default Nika theme)
    pub fn default_dark() -> Self {
        Self::dark()
    }

    #[deprecated(since = "0.79.0", note = "use Theme::dark() instead")]
    pub fn novanet() -> Self {
        Self::dark()
    }

    /// Create dark theme (default)
    pub fn dark() -> Self {
        Self::default()
    }

    /// Create theme from config theme name
    ///
    /// Bridges config::ThemeName to Theme struct.
    pub fn from_name(name: &super::config::ThemeName) -> Self {
        match name {
            super::config::ThemeName::Dark => Self::dark(),
            super::config::ThemeName::Light => Self::light(),
            super::config::ThemeName::Solarized => Self::solarized(),
        }
    }

    /// Create light theme
    ///
    /// Uses Tailwind CSS Slate-50 background with adjusted accents.
    pub fn light() -> Self {
        // Delegate to cosmic_light() which uses TokenResolver
        Self::cosmic_light()
    }

    /// Create Solarized theme
    ///
    /// Note: Solarized is mapped to Cosmic Dark as the closest
    /// visual match. The original Solarized palette is preserved in the
    /// design system documentation for reference.
    pub fn solarized() -> Self {
        // Delegate to cosmic_dark() as closest match
        Self::cosmic_dark()
    }

    /// Get color for MCP tool by name
    pub fn mcp_tool_color(&self, tool: &str) -> Color {
        match tool {
            t if t.contains("describe") => self.mcp_describe,
            t if t.contains("search") => self.mcp_search,
            t if t.contains("introspect") => self.mcp_describe, // shares describe color
            t if t.contains("context") => self.mcp_context,
            t if t.contains("write") => self.mcp_write,
            t if t.contains("audit") => self.mcp_audit,
            t if t.contains("batch") => self.mcp_search, // shares search color
            _ => self.text_secondary,
        }
    }

    /// Get style for task status
    pub fn status_style(&self, status: TaskStatus) -> Style {
        let color = match status {
            TaskStatus::Queued => self.text_muted,
            TaskStatus::Pending => self.status_pending,
            TaskStatus::Running => self.status_running,
            TaskStatus::Success => self.status_success,
            TaskStatus::Failed => self.status_failed,
            TaskStatus::Paused => self.status_paused,
            TaskStatus::Skipped => self.text_muted,
        };
        Style::default().fg(color)
    }

    /// Get style for panel border (focused or not)
    pub fn border_style(&self, focused: bool) -> Style {
        if focused {
            Style::default()
                .fg(self.border_focused)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.border_normal)
        }
    }

    /// Get style for primary text
    pub fn text_style(&self) -> Style {
        Style::default().fg(self.text_primary)
    }

    /// Get style for secondary text
    pub fn text_secondary_style(&self) -> Style {
        Style::default().fg(self.text_secondary)
    }

    /// Get style for muted text
    pub fn text_muted_style(&self) -> Style {
        Style::default().fg(self.text_muted)
    }

    /// Get style for highlighted text
    pub fn highlight_style(&self) -> Style {
        Style::default()
            .fg(self.highlight)
            .add_modifier(Modifier::BOLD)
    }

    /// Get verb color (full saturation)
    pub fn verb_color(&self, verb: VerbColor) -> Color {
        verb.rgb()
    }

    /// Get verb color muted (50% opacity simulation)
    pub fn verb_color_muted(&self, verb: VerbColor) -> Color {
        verb.muted()
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // ANIMATED STYLES
    // ═══════════════════════════════════════════════════════════════════════════

    /// Get animated highlight that pulses with frame counter
    pub fn animated_highlight(&self, frame: u8) -> Style {
        let color = solarized::accent_at(frame);
        Style::default().fg(color).add_modifier(Modifier::BOLD)
    }

    /// Get glowing border style for focused elements
    pub fn border_glow(&self, frame: u8) -> Style {
        // Alternate between highlight and brighter version
        let intensity = solarized::pulse_intensity(frame);
        if intensity > 0.5 {
            Style::default()
                .fg(self.border_focused)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.highlight)
        }
    }

    /// Get style for success with optional pulse effect
    pub fn success_pulse(&self, frame: u8) -> Style {
        let intensity = solarized::pulse_intensity(frame);
        let color = if intensity > 0.7 {
            solarized::GREEN
        } else {
            self.status_success
        };
        Style::default().fg(color).add_modifier(Modifier::BOLD)
    }

    /// Get style for running/active with amber pulse
    pub fn running_pulse(&self, frame: u8) -> Style {
        let intensity = solarized::pulse_intensity(frame);
        let color = if intensity > 0.5 {
            solarized::ORANGE
        } else {
            solarized::YELLOW
        };
        Style::default().fg(color).add_modifier(Modifier::BOLD)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_default_creates_valid_colors() {
        let theme = Theme::default();
        // Verify key colors are set (not black/default)
        assert_ne!(theme.realm_shared, Color::Reset);
        assert_ne!(theme.status_running, Color::Reset);
        assert_ne!(theme.mcp_describe, Color::Reset);
    }

    #[test]
    fn test_mcp_tool_color_matches_tool_name() {
        let theme = Theme::dark();
        assert_eq!(theme.mcp_tool_color("nika_describe"), theme.mcp_describe);
        assert_eq!(theme.mcp_tool_color("nika_search"), theme.mcp_search);
        assert_eq!(
            theme.mcp_tool_color("nika_introspect"),
            theme.mcp_describe
        );
        assert_eq!(theme.mcp_tool_color("nika_context"), theme.mcp_context);
        assert_eq!(theme.mcp_tool_color("nika_write"), theme.mcp_write);
        assert_eq!(theme.mcp_tool_color("nika_audit"), theme.mcp_audit);
        assert_eq!(theme.mcp_tool_color("nika_batch"), theme.mcp_search);
    }

    #[test]
    fn test_status_style_returns_correct_color() {
        let theme = Theme::dark();
        let style = theme.status_style(TaskStatus::Running);
        assert_eq!(style.fg, Some(theme.status_running));
    }

    #[test]
    fn test_border_style_focused_vs_unfocused() {
        let theme = Theme::dark();
        let focused = theme.border_style(true);
        let unfocused = theme.border_style(false);
        assert_ne!(focused.fg, unfocused.fg);
    }

    // ═══ TIER 2.4: Theme Mode Tests ═══
    #[test]
    fn test_theme_mode_default_is_dark() {
        let mode = ThemeMode::default();
        assert_eq!(mode, ThemeMode::Dark);
    }

    #[test]
    fn test_theme_mode_toggle() {
        let mode = ThemeMode::Dark;
        assert_eq!(mode.toggle(), ThemeMode::Light);

        let mode = ThemeMode::Light;
        assert_eq!(mode.toggle(), ThemeMode::Dark);
    }

    #[test]
    fn test_theme_mode_theme_returns_correct_theme() {
        let dark_theme = ThemeMode::Dark.theme();
        let light_theme = ThemeMode::Light.theme();

        // Dark has Abyss Dark background
        assert_eq!(dark_theme.background, Color::Rgb(8, 8, 14)); // Abyss

        // Light has Cosmic Light background (Slate-50)
        assert_eq!(light_theme.background, Color::Rgb(248, 250, 252)); // #f8fafc
    }

    #[test]
    fn test_light_theme_colors_differ_from_dark() {
        let dark = Theme::dark();
        let light = Theme::light();

        // Text colors should be inverted
        assert_ne!(dark.text_primary, light.text_primary);
        assert_ne!(dark.background, light.background);
    }

    #[test]
    fn test_theme_verb_color_methods() {
        let theme = Theme::dark();

        // verb_color should return full RGB
        assert_eq!(theme.verb_color(VerbColor::Infer), Color::Rgb(139, 92, 246));
        assert_eq!(theme.verb_color(VerbColor::Agent), Color::Rgb(244, 63, 94));

        // verb_color_muted should return muted version
        assert_eq!(
            theme.verb_color_muted(VerbColor::Infer),
            Color::Rgb(97, 64, 171)
        );
        assert_eq!(
            theme.verb_color_muted(VerbColor::Agent),
            Color::Rgb(170, 44, 66)
        );
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // THEME TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_theme_dark_is_same_as_default() {
        let dark = Theme::dark();
        let default = Theme::default();

        assert_eq!(dark.realm_shared, default.realm_shared);
        assert_eq!(dark.text_primary, default.text_primary);
        assert_eq!(dark.background, default.background);
    }

    #[test]
    fn test_theme_light_has_inverted_colors() {
        let light = Theme::light();

        // Light theme should have Cosmic Light background (Slate-50)
        assert_eq!(light.background, Color::Rgb(248, 250, 252)); // #f8fafc

        // Light theme should have dark text (Slate-900)
        assert_eq!(light.text_primary, Color::Rgb(15, 23, 42)); // #0f172a
    }

    // ═══ SOLARIZED THEME TESTS ═══

    #[test]
    fn test_theme_solarized_maps_to_cosmic_dark() {
        let theme = Theme::solarized();

        assert_eq!(theme.background, Color::Rgb(8, 8, 14)); // Abyss Dark
        assert_eq!(theme.text_primary, Color::Rgb(200, 210, 230)); // Cool white-blue
    }

    #[test]
    fn test_theme_solarized_has_cosmic_status_colors() {
        let theme = Theme::solarized();

        assert_eq!(theme.status_running, Color::Rgb(255, 200, 0)); // Amber
        assert_eq!(theme.status_success, Color::Rgb(0, 230, 120)); // Neon green
        assert_eq!(theme.status_failed, Color::Rgb(255, 60, 80)); // Neon red
    }

    #[test]
    fn test_theme_solarized_equals_dark() {
        let dark = Theme::dark();
        let solarized = Theme::solarized();

        // Solarized maps to Dark
        assert_eq!(dark.background, solarized.background);
        assert_eq!(dark.text_primary, solarized.text_primary);
    }

    #[test]
    fn test_theme_solarized_differs_from_light() {
        let light = Theme::light();
        let solarized = Theme::solarized();

        // Backgrounds should differ (Solarized=Dark, Light=Light)
        assert_ne!(light.background, solarized.background);
        // Text primary should differ
        assert_ne!(light.text_primary, solarized.text_primary);
    }

    #[test]
    fn test_theme_from_name_dark() {
        use super::super::config::ThemeName;
        let theme = Theme::from_name(&ThemeName::Dark);
        let dark = Theme::dark();
        assert_eq!(theme.background, dark.background);
    }

    #[test]
    fn test_theme_from_name_light() {
        use super::super::config::ThemeName;
        let theme = Theme::from_name(&ThemeName::Light);
        let light = Theme::light();
        assert_eq!(theme.background, light.background);
    }

    #[test]
    fn test_theme_from_name_solarized() {
        use super::super::config::ThemeName;
        let theme = Theme::from_name(&ThemeName::Solarized);
        let solarized = Theme::solarized();
        assert_eq!(theme.background, solarized.background);
    }

    #[test]
    fn test_theme_mcp_tool_color_describe() {
        let theme = Theme::default();
        assert_eq!(theme.mcp_tool_color("nika_describe"), theme.mcp_describe);
        assert_eq!(theme.mcp_tool_color("describe"), theme.mcp_describe);
    }

    #[test]
    fn test_theme_mcp_tool_color_context() {
        let theme = Theme::default();
        assert_eq!(theme.mcp_tool_color("nika_context"), theme.mcp_context);
    }

    #[test]
    fn test_theme_mcp_tool_color_unknown_defaults_to_secondary() {
        let theme = Theme::default();
        assert_eq!(theme.mcp_tool_color("unknown_tool"), theme.text_secondary);
        assert_eq!(theme.mcp_tool_color(""), theme.text_secondary);
    }

    #[test]
    fn test_theme_status_style_pending() {
        let theme = Theme::dark();
        let style = theme.status_style(TaskStatus::Pending);
        assert_eq!(style.fg, Some(theme.status_pending));
    }

    #[test]
    fn test_theme_status_style_all_variants() {
        let theme = Theme::dark();

        let pending = theme.status_style(TaskStatus::Pending);
        assert_eq!(pending.fg, Some(theme.status_pending));

        let running = theme.status_style(TaskStatus::Running);
        assert_eq!(running.fg, Some(theme.status_running));

        let success = theme.status_style(TaskStatus::Success);
        assert_eq!(success.fg, Some(theme.status_success));

        let failed = theme.status_style(TaskStatus::Failed);
        assert_eq!(failed.fg, Some(theme.status_failed));

        let paused = theme.status_style(TaskStatus::Paused);
        assert_eq!(paused.fg, Some(theme.status_paused));
    }

    #[test]
    fn test_theme_border_style_focused_has_bold() {
        let theme = Theme::dark();
        let focused = theme.border_style(true);
        assert!(focused.add_modifier.contains(Modifier::BOLD));
        assert_eq!(focused.fg, Some(theme.border_focused));
    }

    #[test]
    fn test_theme_border_style_unfocused_no_bold() {
        let theme = Theme::dark();
        let unfocused = theme.border_style(false);
        assert!(!unfocused.add_modifier.contains(Modifier::BOLD));
        assert_eq!(unfocused.fg, Some(theme.border_normal));
    }

    #[test]
    fn test_theme_text_style_primary() {
        let theme = Theme::dark();
        let style = theme.text_style();
        assert_eq!(style.fg, Some(theme.text_primary));
    }

    #[test]
    fn test_theme_text_secondary_style() {
        let theme = Theme::dark();
        let style = theme.text_secondary_style();
        assert_eq!(style.fg, Some(theme.text_secondary));
    }

    #[test]
    fn test_theme_text_muted_style() {
        let theme = Theme::dark();
        let style = theme.text_muted_style();
        assert_eq!(style.fg, Some(theme.text_muted));
    }

    #[test]
    fn test_theme_highlight_style_has_bold() {
        let theme = Theme::dark();
        let style = theme.highlight_style();
        assert!(style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(style.fg, Some(theme.highlight));
    }

    #[test]
    fn test_theme_trait_colors_are_distinct() {
        let theme = Theme::default();

        let traits = [
            theme.trait_defined,
            theme.trait_authored,
            theme.trait_imported,
            theme.trait_generated,
            theme.trait_retrieved,
        ];

        // All trait colors should be unique
        for i in 0..traits.len() {
            for j in (i + 1)..traits.len() {
                assert_ne!(
                    traits[i], traits[j],
                    "Trait color {} and {} are identical",
                    i, j
                );
            }
        }
    }

    #[test]
    fn test_theme_realm_colors_are_distinct() {
        let theme = Theme::default();
        assert_ne!(theme.realm_shared, theme.realm_org);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // THEME MODE TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_theme_mode_equality() {
        assert_eq!(ThemeMode::Dark, ThemeMode::Dark);
        assert_eq!(ThemeMode::Light, ThemeMode::Light);
        assert_ne!(ThemeMode::Dark, ThemeMode::Light);
    }

    #[test]
    fn test_theme_mode_copy_clone() {
        let mode = ThemeMode::Light;
        let copied = mode;
        assert_eq!(mode, copied);
    }

    #[test]
    fn test_theme_mode_toggle_bidirectional() {
        let dark = ThemeMode::Dark;
        assert_eq!(dark.toggle().toggle(), dark);

        let light = ThemeMode::Light;
        assert_eq!(light.toggle().toggle(), light);
    }

    #[test]
    fn test_theme_mode_cycle() {
        let dark = ThemeMode::Dark;
        assert_eq!(dark.cycle(), ThemeMode::Light);

        let light = ThemeMode::Light;
        assert_eq!(light.cycle(), ThemeMode::Solarized);

        let solarized = ThemeMode::Solarized;
        assert_eq!(solarized.cycle(), ThemeMode::Dark);
    }

    #[test]
    fn test_theme_mode_cycle_full_loop() {
        let start = ThemeMode::Dark;
        let after_3_cycles = start.cycle().cycle().cycle();
        assert_eq!(start, after_3_cycles);
    }

    #[test]
    fn test_theme_mode_label() {
        assert_eq!(ThemeMode::Dark.label(), "Dark");
        assert_eq!(ThemeMode::Light.label(), "Light");
        assert_eq!(ThemeMode::Solarized.label(), "Solarized");
    }

    #[test]
    fn test_theme_mode_solarized_toggle_goes_to_dark() {
        let solarized = ThemeMode::Solarized;
        assert_eq!(solarized.toggle(), ThemeMode::Dark);
    }

    #[test]
    fn test_theme_mode_solarized_theme_returns_cosmic_dark() {
        let solarized_theme = ThemeMode::Solarized.theme();
        // Solarized maps to Cosmic Dark (Abyss Dark)
        assert_eq!(solarized_theme.background, Color::Rgb(8, 8, 14)); // Abyss
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // INTEGRATION TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_theme_dark_vs_light_status_colors_differ() {
        let dark = Theme::dark();
        let light = Theme::light();

        assert_ne!(dark.status_running, light.status_running);
        assert_ne!(dark.status_success, light.status_success);
        assert_ne!(dark.status_failed, light.status_failed);
    }

    #[test]
    fn test_theme_all_realm_colors_present() {
        let theme = Theme::default();
        assert_ne!(theme.realm_shared, Color::Reset);
        assert_ne!(theme.realm_org, Color::Reset);
    }

    #[test]
    fn test_theme_all_ui_element_colors_present() {
        let theme = Theme::default();
        assert_ne!(theme.border_normal, Color::Reset);
        assert_ne!(theme.border_focused, Color::Reset);
        assert_ne!(theme.text_primary, Color::Reset);
        assert_ne!(theme.text_secondary, Color::Reset);
        assert_ne!(theme.text_muted, Color::Reset);
        assert_ne!(theme.background, Color::Reset);
        assert_ne!(theme.highlight, Color::Reset);
    }
}
