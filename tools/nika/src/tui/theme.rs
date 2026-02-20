//! TUI Theme - NovaNet Taxonomy Colors
//!
//! Color palette derived from NovaNet's visual-encoding.yaml.
//! Provides consistent styling across all TUI components.

use ratatui::style::{Color, Modifier, Style};

// ═══════════════════════════════════════════════════════════════════════════
// VERB COLORS (DAG Visualization)
// ═══════════════════════════════════════════════════════════════════════════

/// Verb-specific colors for DAG visualization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerbColor {
    Infer,  // Violet #8B5CF6
    Exec,   // Amber #F59E0B
    Fetch,  // Cyan #06B6D4
    Invoke, // Emerald #10B981
    Agent,  // Rose #F43F5E
}

impl VerbColor {
    /// Get the RGB color for this verb
    pub fn rgb(&self) -> Color {
        match self {
            Self::Infer => Color::Rgb(139, 92, 246),  // Violet
            Self::Exec => Color::Rgb(245, 158, 11),   // Amber
            Self::Fetch => Color::Rgb(6, 182, 212),   // Cyan
            Self::Invoke => Color::Rgb(16, 185, 129), // Emerald
            Self::Agent => Color::Rgb(244, 63, 94),   // Rose
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
        }
    }

    /// Get icon for this verb
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Infer => "🧠",
            Self::Exec => "⚡",
            Self::Fetch => "🌐",
            Self::Invoke => "🔧",
            Self::Agent => "🤖",
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
            _ => Self::Infer, // default
        }
    }
}

/// Theme mode selector (TIER 2.4)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
}

impl ThemeMode {
    /// Toggle between dark and light
    pub fn toggle(&self) -> Self {
        match self {
            Self::Dark => Self::Light,
            Self::Light => Self::Dark,
        }
    }

    /// Get the theme for this mode
    pub fn theme(&self) -> Theme {
        match self {
            Self::Dark => Theme::dark(),
            Self::Light => Theme::light(),
        }
    }
}

/// NovaNet-inspired color theme for the TUI
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
    // MCP TOOLS
    // ═══════════════════════════════════════════
    pub mcp_describe: Color,
    pub mcp_traverse: Color,
    pub mcp_search: Color,
    pub mcp_atoms: Color,
    pub mcp_generate: Color,

    // ═══════════════════════════════════════════
    // UI ELEMENTS
    // ═══════════════════════════════════════════
    pub border_normal: Color,
    pub border_focused: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub background: Color,
    pub highlight: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            // Realms
            realm_shared: Color::Rgb(59, 130, 246), // #3B82F6 blue
            realm_org: Color::Rgb(16, 185, 129),    // #10B981 emerald

            // Traits
            trait_defined: Color::Rgb(107, 114, 128), // #6B7280 gray
            trait_authored: Color::Rgb(139, 92, 246), // #8B5CF6 violet
            trait_imported: Color::Rgb(245, 158, 11), // #F59E0B amber
            trait_generated: Color::Rgb(16, 185, 129), // #10B981 emerald
            trait_retrieved: Color::Rgb(6, 182, 212), // #06B6D4 cyan

            // Status
            status_pending: Color::Rgb(107, 114, 128), // #6B7280 gray
            status_running: Color::Rgb(245, 158, 11),  // #F59E0B amber
            status_success: Color::Rgb(34, 197, 94),   // #22C55E green
            status_failed: Color::Rgb(239, 68, 68),    // #EF4444 red
            status_paused: Color::Rgb(6, 182, 212),    // #06B6D4 cyan

            // MCP tools
            mcp_describe: Color::Rgb(59, 130, 246), // #3B82F6 blue
            mcp_traverse: Color::Rgb(236, 72, 153), // #EC4899 pink
            mcp_search: Color::Rgb(245, 158, 11),   // #F59E0B amber
            mcp_atoms: Color::Rgb(139, 92, 246),    // #8B5CF6 violet
            mcp_generate: Color::Rgb(16, 185, 129), // #10B981 emerald

            // UI elements
            border_normal: Color::Rgb(75, 85, 99), // #4B5563 gray-600
            border_focused: Color::Rgb(99, 102, 241), // #6366F1 indigo
            text_primary: Color::Rgb(243, 244, 246), // #F3F4F6 gray-100
            text_secondary: Color::Rgb(156, 163, 175), // #9CA3AF gray-400
            text_muted: Color::Rgb(107, 114, 128), // #6B7280 gray-500
            background: Color::Rgb(17, 24, 39),    // #111827 gray-900
            highlight: Color::Rgb(99, 102, 241),   // #6366F1 indigo
        }
    }
}

impl Theme {
    /// Create the default NovaNet theme (dark)
    pub fn novanet() -> Self {
        Self::dark()
    }

    /// Create dark theme (default)
    pub fn dark() -> Self {
        Self::default()
    }

    /// Create light theme (TIER 2.4)
    pub fn light() -> Self {
        Self {
            // Realms
            realm_shared: Color::Rgb(37, 99, 235), // #2563EB blue-600
            realm_org: Color::Rgb(5, 150, 105),    // #059669 emerald-600

            // Traits
            trait_defined: Color::Rgb(75, 85, 99), // #4B5563 gray-600
            trait_authored: Color::Rgb(124, 58, 237), // #7C3AED violet-600
            trait_imported: Color::Rgb(217, 119, 6), // #D97706 amber-600
            trait_generated: Color::Rgb(5, 150, 105), // #059669 emerald-600
            trait_retrieved: Color::Rgb(8, 145, 178), // #0891B2 cyan-600

            // Status
            status_pending: Color::Rgb(75, 85, 99), // #4B5563 gray-600
            status_running: Color::Rgb(217, 119, 6), // #D97706 amber-600
            status_success: Color::Rgb(22, 163, 74), // #16A34A green-600
            status_failed: Color::Rgb(220, 38, 38), // #DC2626 red-600
            status_paused: Color::Rgb(8, 145, 178), // #0891B2 cyan-600

            // MCP tools
            mcp_describe: Color::Rgb(37, 99, 235), // #2563EB blue-600
            mcp_traverse: Color::Rgb(219, 39, 119), // #DB2777 pink-600
            mcp_search: Color::Rgb(217, 119, 6),   // #D97706 amber-600
            mcp_atoms: Color::Rgb(124, 58, 237),   // #7C3AED violet-600
            mcp_generate: Color::Rgb(5, 150, 105), // #059669 emerald-600

            // UI elements - light theme
            border_normal: Color::Rgb(209, 213, 219), // #D1D5DB gray-300
            border_focused: Color::Rgb(79, 70, 229),  // #4F46E5 indigo-600
            text_primary: Color::Rgb(17, 24, 39),     // #111827 gray-900
            text_secondary: Color::Rgb(75, 85, 99),   // #4B5563 gray-600
            text_muted: Color::Rgb(156, 163, 175),    // #9CA3AF gray-400
            background: Color::Rgb(249, 250, 251),    // #F9FAFB gray-50
            highlight: Color::Rgb(79, 70, 229),       // #4F46E5 indigo-600
        }
    }

    /// Get color for MCP tool by name
    pub fn mcp_tool_color(&self, tool: &str) -> Color {
        match tool {
            t if t.contains("describe") => self.mcp_describe,
            t if t.contains("traverse") => self.mcp_traverse,
            t if t.contains("search") => self.mcp_search,
            t if t.contains("atoms") => self.mcp_atoms,
            t if t.contains("generate") => self.mcp_generate,
            _ => self.text_secondary,
        }
    }

    /// Get style for task status
    pub fn status_style(&self, status: TaskStatus) -> Style {
        let color = match status {
            TaskStatus::Pending => self.status_pending,
            TaskStatus::Running => self.status_running,
            TaskStatus::Success => self.status_success,
            TaskStatus::Failed => self.status_failed,
            TaskStatus::Paused => self.status_paused,
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
}

/// Task status for styling
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Running,
    Success,
    Failed,
    Paused,
}

/// Mission phase for space theme
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissionPhase {
    /// Pre-launch checks, DAG validation
    Preflight,
    /// Loading configs, MCP connections
    Countdown,
    /// First task executing
    Launch,
    /// Nominal execution
    Orbital,
    /// MCP tool invocation
    Rendezvous,
    /// Workflow completed successfully
    MissionSuccess,
    /// Workflow failed
    Abort,
}

impl MissionPhase {
    /// Get icon for mission phase
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Preflight => "◦",
            Self::Countdown => "⊙",
            Self::Launch => "⊛",
            Self::Orbital => "◉",
            Self::Rendezvous => "◈",
            Self::MissionSuccess => "✦",
            Self::Abort => "⊗",
        }
    }

    /// Get display name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Preflight => "PREFLIGHT",
            Self::Countdown => "COUNTDOWN",
            Self::Launch => "LAUNCH",
            Self::Orbital => "ORBITAL",
            Self::Rendezvous => "RENDEZVOUS",
            Self::MissionSuccess => "MISSION SUCCESS",
            Self::Abort => "ABORT",
        }
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
        let theme = Theme::novanet();
        assert_eq!(theme.mcp_tool_color("novanet_describe"), theme.mcp_describe);
        assert_eq!(theme.mcp_tool_color("novanet_traverse"), theme.mcp_traverse);
        assert_eq!(theme.mcp_tool_color("novanet_search"), theme.mcp_search);
        assert_eq!(theme.mcp_tool_color("novanet_atoms"), theme.mcp_atoms);
        assert_eq!(theme.mcp_tool_color("novanet_generate"), theme.mcp_generate);
    }

    #[test]
    fn test_status_style_returns_correct_color() {
        let theme = Theme::novanet();
        let style = theme.status_style(TaskStatus::Running);
        assert_eq!(style.fg, Some(theme.status_running));
    }

    #[test]
    fn test_border_style_focused_vs_unfocused() {
        let theme = Theme::novanet();
        let focused = theme.border_style(true);
        let unfocused = theme.border_style(false);
        assert_ne!(focused.fg, unfocused.fg);
    }

    #[test]
    fn test_mission_phase_icons() {
        assert_eq!(MissionPhase::Preflight.icon(), "◦");
        assert_eq!(MissionPhase::Orbital.icon(), "◉");
        assert_eq!(MissionPhase::MissionSuccess.icon(), "✦");
        assert_eq!(MissionPhase::Abort.icon(), "⊗");
    }

    #[test]
    fn test_mission_phase_names() {
        assert_eq!(MissionPhase::Countdown.name(), "COUNTDOWN");
        assert_eq!(MissionPhase::MissionSuccess.name(), "MISSION SUCCESS");
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

        // Dark has dark background
        assert_eq!(dark_theme.background, Color::Rgb(17, 24, 39));

        // Light has light background
        assert_eq!(light_theme.background, Color::Rgb(249, 250, 251));
    }

    #[test]
    fn test_light_theme_colors_differ_from_dark() {
        let dark = Theme::dark();
        let light = Theme::light();

        // Text colors should be inverted
        assert_ne!(dark.text_primary, light.text_primary);
        assert_ne!(dark.background, light.background);
    }

    // ═══ VERB COLOR TESTS ═══

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
        assert_eq!(VerbColor::Infer.icon(), "🧠");
        assert_eq!(VerbColor::Exec.icon(), "⚡");
        assert_eq!(VerbColor::Fetch.icon(), "🌐");
        assert_eq!(VerbColor::Invoke.icon(), "🔧");
        assert_eq!(VerbColor::Agent.icon(), "🤖");
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
    fn test_theme_verb_color_methods() {
        let theme = Theme::novanet();

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
}
