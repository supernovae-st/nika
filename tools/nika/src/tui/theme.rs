//! TUI Theme - NovaNet Taxonomy Colors
//!
//! Color palette derived from NovaNet's visual-encoding.yaml.
//! Provides consistent styling across all TUI components.
//!
//! # ColorMode Detection (v0.7.0+)
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

use ratatui::style::{Color, Modifier, Style};

// ═══════════════════════════════════════════════════════════════════════════
// COLOR MODE DETECTION (v0.7.0+)
// ═══════════════════════════════════════════════════════════════════════════

/// Terminal color mode detection.
///
/// Automatically detects terminal color capabilities based on environment
/// variables. Falls back gracefully for limited terminals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorMode {
    /// 24-bit TrueColor (16 million colors)
    /// Detected via COLORTERM=truecolor or COLORTERM=24bit
    #[default]
    TrueColor,

    /// 256-color palette (8-bit)
    /// Detected via TERM containing "256color"
    Color256,

    /// Basic 16-color palette (4-bit)
    /// Fallback for basic terminals
    Color16,
}

impl ColorMode {
    /// Detect color mode from environment variables.
    ///
    /// Checks in order:
    /// 1. COLORTERM for truecolor/24bit support
    /// 2. TERM for 256color support
    /// 3. Falls back to 16 colors
    pub fn detect() -> Self {
        // Check COLORTERM for modern terminals
        if let Ok(colorterm) = std::env::var("COLORTERM") {
            let ct = colorterm.to_lowercase();
            if ct == "truecolor" || ct == "24bit" {
                return Self::TrueColor;
            }
        }

        // Check TERM for 256 color support
        if let Ok(term) = std::env::var("TERM") {
            if term.contains("256color") || term.contains("24bit") {
                return Self::Color256;
            }
            // Some terminals advertise truecolor via TERM
            if term.contains("truecolor") {
                return Self::TrueColor;
            }
        }

        // Default to 16 colors for maximum compatibility
        Self::Color16
    }

    /// Check if this mode supports RGB colors.
    pub fn supports_rgb(&self) -> bool {
        matches!(self, Self::TrueColor)
    }

    /// Check if this mode supports at least 256 colors.
    pub fn supports_256(&self) -> bool {
        matches!(self, Self::TrueColor | Self::Color256)
    }

    /// Adapt an RGB color for this color mode.
    ///
    /// - TrueColor: Returns the color unchanged
    /// - Color256: Converts to nearest 256-color palette entry
    /// - Color16: Converts to nearest ANSI color
    pub fn adapt_color(&self, color: Color) -> Color {
        match self {
            Self::TrueColor => color,
            Self::Color256 => Self::rgb_to_256(color),
            Self::Color16 => Self::rgb_to_16(color),
        }
    }

    /// Convert RGB color to nearest 256-color palette entry.
    fn rgb_to_256(color: Color) -> Color {
        match color {
            Color::Rgb(r, g, b) => {
                // Use the 6x6x6 color cube (colors 16-231)
                let r_idx = (r as u16 * 5 / 255) as u8;
                let g_idx = (g as u16 * 5 / 255) as u8;
                let b_idx = (b as u16 * 5 / 255) as u8;
                let idx = 16 + 36 * r_idx + 6 * g_idx + b_idx;
                Color::Indexed(idx)
            }
            other => other,
        }
    }

    /// Convert RGB color to nearest ANSI 16-color.
    fn rgb_to_16(color: Color) -> Color {
        match color {
            Color::Rgb(r, g, b) => {
                // Calculate luminance
                let luma = (r as u16 * 299 + g as u16 * 587 + b as u16 * 114) / 1000;
                let bright = luma > 127;

                // Find dominant color channel
                let max = r.max(g).max(b);
                let min = r.min(g).min(b);
                let saturation = if max == 0 {
                    0
                } else {
                    (max - min) as u16 * 255 / max as u16
                };

                if saturation < 50 {
                    // Grayscale
                    if bright {
                        Color::White
                    } else {
                        Color::DarkGray
                    }
                } else if r >= g && r >= b {
                    // Red dominant
                    if g > b / 2 {
                        if bright {
                            Color::Yellow
                        } else {
                            Color::LightYellow
                        }
                    } else if bright {
                        Color::LightRed
                    } else {
                        Color::Red
                    }
                } else if g >= r && g >= b {
                    // Green dominant
                    if b > r / 2 {
                        if bright {
                            Color::Cyan
                        } else {
                            Color::LightCyan
                        }
                    } else if bright {
                        Color::LightGreen
                    } else {
                        Color::Green
                    }
                } else {
                    // Blue dominant
                    if r > g / 2 {
                        if bright {
                            Color::LightMagenta
                        } else {
                            Color::Magenta
                        }
                    } else if bright {
                        Color::LightBlue
                    } else {
                        Color::Blue
                    }
                }
            }
            other => other,
        }
    }
}

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

    /// Get glow version (brighter for active/hover states)
    pub fn glow(&self) -> Color {
        match self {
            Self::Infer => Color::Rgb(167, 139, 250), // Violet-400
            Self::Exec => Color::Rgb(251, 191, 36),   // Amber-400
            Self::Fetch => Color::Rgb(34, 211, 238),  // Cyan-400
            Self::Invoke => Color::Rgb(52, 211, 153), // Emerald-400
            Self::Agent => Color::Rgb(251, 113, 133), // Rose-400
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

    /// Get subtle version (very muted for backgrounds)
    pub fn subtle(&self) -> Color {
        match self {
            Self::Infer => Color::Rgb(55, 48, 83),  // Violet-950/50
            Self::Exec => Color::Rgb(69, 53, 18),   // Amber-950/50
            Self::Fetch => Color::Rgb(22, 57, 67),  // Cyan-950/50
            Self::Invoke => Color::Rgb(20, 61, 47), // Emerald-950/50
            Self::Agent => Color::Rgb(68, 32, 41),  // Rose-950/50
        }
    }

    /// Get icon for this verb (matches CLAUDE.md canonical icons)
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Infer => "⚡",  // LLM generation
            Self::Exec => "📟",   // Shell command
            Self::Fetch => "🛰️",  // HTTP request
            Self::Invoke => "🔌", // MCP tool
            Self::Agent => "🐔",  // Agentic loop (parent)
        }
    }

    /// Get icon for subagent (spawned via spawn_agent)
    pub fn subagent_icon() -> &'static str {
        "🐤" // Spawned subagent
    }

    /// Get ASCII-safe icon for terminals without emoji support
    pub fn icon_ascii(&self) -> &'static str {
        match self {
            Self::Infer => "[I]",
            Self::Exec => "[X]",
            Self::Fetch => "[F]",
            Self::Invoke => "[V]",
            Self::Agent => "[A]",
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
    /// Workflow paused by user
    Pause,
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
            Self::Pause => "⏸",
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
            Self::Pause => "PAUSED",
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
        // Canonical icons from CLAUDE.md
        assert_eq!(VerbColor::Infer.icon(), "⚡"); // LLM generation
        assert_eq!(VerbColor::Exec.icon(), "📟"); // Shell command
        assert_eq!(VerbColor::Fetch.icon(), "🛰️"); // HTTP request
        assert_eq!(VerbColor::Invoke.icon(), "🔌"); // MCP tool
        assert_eq!(VerbColor::Agent.icon(), "🐔"); // Agentic loop (parent)
        assert_eq!(VerbColor::subagent_icon(), "🐤"); // Spawned subagent
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

    // ═══ GLOW COLOR TESTS ═══

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

    // ═══════════════════════════════════════════════════════════════════════════
    // COLOR MODE DETECTION TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_color_mode_default_is_truecolor() {
        let mode = ColorMode::default();
        assert_eq!(mode, ColorMode::TrueColor);
    }

    #[test]
    fn test_color_mode_supports_rgb_truecolor() {
        assert!(ColorMode::TrueColor.supports_rgb());
        assert!(!ColorMode::Color256.supports_rgb());
        assert!(!ColorMode::Color16.supports_rgb());
    }

    #[test]
    fn test_color_mode_supports_256() {
        assert!(ColorMode::TrueColor.supports_256());
        assert!(ColorMode::Color256.supports_256());
        assert!(!ColorMode::Color16.supports_256());
    }

    #[test]
    fn test_color_mode_adapt_color_truecolor_unchanged() {
        let color = Color::Rgb(139, 92, 246);
        assert_eq!(ColorMode::TrueColor.adapt_color(color), color);
    }

    #[test]
    fn test_color_mode_adapt_color_256_converts_to_indexed() {
        let color = Color::Rgb(139, 92, 246);
        let adapted = ColorMode::Color256.adapt_color(color);

        // Should convert to indexed (8-bit) color
        match adapted {
            Color::Indexed(_) => (), // Expected
            _ => panic!("Expected Indexed color, got {:?}", adapted),
        }
    }

    #[test]
    fn test_color_mode_adapt_color_16_converts_to_ansi() {
        let color = Color::Rgb(139, 92, 246);
        let adapted = ColorMode::Color16.adapt_color(color);

        // Should convert to ANSI color (not RGB)
        if let Color::Rgb(_, _, _) = adapted {
            panic!("Should not be RGB for Color16 mode")
        }
        // Expected: White, DarkGray, or other ANSI color
    }

    #[test]
    fn test_color_mode_256_adapt_converts_to_indexed() {
        // Test conversion to 6x6x6 color cube (colors 16-231)
        let mode = ColorMode::Color256;
        let red = mode.adapt_color(Color::Rgb(255, 0, 0));
        let green = mode.adapt_color(Color::Rgb(0, 255, 0));
        let blue = mode.adapt_color(Color::Rgb(0, 0, 255));

        // All should be Indexed colors in range 16-231
        match (red, green, blue) {
            (Color::Indexed(r), Color::Indexed(g), Color::Indexed(b)) => {
                assert!((16..=231).contains(&r));
                assert!((16..=231).contains(&g));
                assert!((16..=231).contains(&b));
            }
            _ => panic!("Expected Indexed colors"),
        }
    }

    #[test]
    fn test_color_mode_256_adapt_preserves_non_rgb() {
        // Non-RGB colors should pass through unchanged
        let mode = ColorMode::Color256;
        let indexed = mode.adapt_color(Color::Indexed(100));
        assert_eq!(indexed, Color::Indexed(100));

        let white = mode.adapt_color(Color::White);
        assert_eq!(white, Color::White);
    }

    #[test]
    fn test_color_mode_16_adapt_grayscale() {
        // Grayscale should become White or DarkGray
        let mode = ColorMode::Color16;
        let light_gray = mode.adapt_color(Color::Rgb(200, 200, 200));
        assert_eq!(light_gray, Color::White);

        let dark_gray = mode.adapt_color(Color::Rgb(50, 50, 50));
        assert_eq!(dark_gray, Color::DarkGray);
    }

    #[test]
    fn test_color_mode_16_adapt_red_dominant() {
        // High red, low green/blue
        let mode = ColorMode::Color16;
        let red = mode.adapt_color(Color::Rgb(200, 30, 30));
        assert_eq!(red, Color::LightRed);
    }

    #[test]
    fn test_color_mode_16_adapt_green_dominant() {
        // High green dominant
        let mode = ColorMode::Color16;
        let green = mode.adapt_color(Color::Rgb(30, 200, 30));
        assert_eq!(green, Color::LightGreen);
    }

    #[test]
    fn test_color_mode_16_adapt_blue_dominant() {
        // High blue dominant
        let mode = ColorMode::Color16;
        let blue = mode.adapt_color(Color::Rgb(30, 30, 200));
        assert_eq!(blue, Color::LightBlue);
    }

    #[test]
    fn test_color_mode_16_adapt_preserves_non_rgb() {
        let mode = ColorMode::Color16;
        let white = mode.adapt_color(Color::White);
        assert_eq!(white, Color::White);

        let indexed = mode.adapt_color(Color::Indexed(50));
        assert_eq!(indexed, Color::Indexed(50));
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
    fn test_theme_novanet_is_dark() {
        let novanet = Theme::novanet();
        let dark = Theme::dark();

        assert_eq!(novanet.background, dark.background);
        assert_eq!(novanet.text_primary, dark.text_primary);
    }

    #[test]
    fn test_theme_light_has_inverted_colors() {
        let light = Theme::light();

        // Light theme should have bright background
        assert_eq!(light.background, Color::Rgb(249, 250, 251));

        // Light theme should have dark text
        assert_eq!(light.text_primary, Color::Rgb(17, 24, 39));
    }

    #[test]
    fn test_theme_mcp_tool_color_describe() {
        let theme = Theme::default();
        assert_eq!(theme.mcp_tool_color("novanet_describe"), theme.mcp_describe);
        assert_eq!(theme.mcp_tool_color("describe"), theme.mcp_describe);
    }

    #[test]
    fn test_theme_mcp_tool_color_traverse() {
        let theme = Theme::default();
        assert_eq!(theme.mcp_tool_color("novanet_traverse"), theme.mcp_traverse);
    }

    #[test]
    fn test_theme_mcp_tool_color_unknown_defaults_to_secondary() {
        let theme = Theme::default();
        assert_eq!(theme.mcp_tool_color("unknown_tool"), theme.text_secondary);
        assert_eq!(theme.mcp_tool_color(""), theme.text_secondary);
    }

    #[test]
    fn test_theme_status_style_pending() {
        let theme = Theme::novanet();
        let style = theme.status_style(TaskStatus::Pending);
        assert_eq!(style.fg, Some(theme.status_pending));
    }

    #[test]
    fn test_theme_status_style_all_variants() {
        let theme = Theme::novanet();

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
        let theme = Theme::novanet();
        let focused = theme.border_style(true);
        assert!(focused.add_modifier.contains(Modifier::BOLD));
        assert_eq!(focused.fg, Some(theme.border_focused));
    }

    #[test]
    fn test_theme_border_style_unfocused_no_bold() {
        let theme = Theme::novanet();
        let unfocused = theme.border_style(false);
        assert!(!unfocused.add_modifier.contains(Modifier::BOLD));
        assert_eq!(unfocused.fg, Some(theme.border_normal));
    }

    #[test]
    fn test_theme_text_style_primary() {
        let theme = Theme::novanet();
        let style = theme.text_style();
        assert_eq!(style.fg, Some(theme.text_primary));
    }

    #[test]
    fn test_theme_text_secondary_style() {
        let theme = Theme::novanet();
        let style = theme.text_secondary_style();
        assert_eq!(style.fg, Some(theme.text_secondary));
    }

    #[test]
    fn test_theme_text_muted_style() {
        let theme = Theme::novanet();
        let style = theme.text_muted_style();
        assert_eq!(style.fg, Some(theme.text_muted));
    }

    #[test]
    fn test_theme_highlight_style_has_bold() {
        let theme = Theme::novanet();
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
    // TASK STATUS TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_task_status_can_be_created() {
        let _ = TaskStatus::Pending;
        let _ = TaskStatus::Running;
        let _ = TaskStatus::Success;
        let _ = TaskStatus::Failed;
        let _ = TaskStatus::Paused;
    }

    #[test]
    fn test_task_status_equality() {
        assert_eq!(TaskStatus::Pending, TaskStatus::Pending);
        assert_ne!(TaskStatus::Pending, TaskStatus::Running);
        assert_ne!(TaskStatus::Success, TaskStatus::Failed);
    }

    #[test]
    fn test_task_status_copy_clone() {
        let status = TaskStatus::Running;
        let copied = status;
        assert_eq!(status, copied);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // MISSION PHASE TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_mission_phase_preflight_icon_and_name() {
        let phase = MissionPhase::Preflight;
        assert_eq!(phase.icon(), "◦");
        assert_eq!(phase.name(), "PREFLIGHT");
    }

    #[test]
    fn test_mission_phase_countdown_icon_and_name() {
        let phase = MissionPhase::Countdown;
        assert_eq!(phase.icon(), "⊙");
        assert_eq!(phase.name(), "COUNTDOWN");
    }

    #[test]
    fn test_mission_phase_launch_icon_and_name() {
        let phase = MissionPhase::Launch;
        assert_eq!(phase.icon(), "⊛");
        assert_eq!(phase.name(), "LAUNCH");
    }

    #[test]
    fn test_mission_phase_orbital_icon_and_name() {
        let phase = MissionPhase::Orbital;
        assert_eq!(phase.icon(), "◉");
        assert_eq!(phase.name(), "ORBITAL");
    }

    #[test]
    fn test_mission_phase_rendezvous_icon_and_name() {
        let phase = MissionPhase::Rendezvous;
        assert_eq!(phase.icon(), "◈");
        assert_eq!(phase.name(), "RENDEZVOUS");
    }

    #[test]
    fn test_mission_phase_success_icon_and_name() {
        let phase = MissionPhase::MissionSuccess;
        assert_eq!(phase.icon(), "✦");
        assert_eq!(phase.name(), "MISSION SUCCESS");
    }

    #[test]
    fn test_mission_phase_abort_icon_and_name() {
        let phase = MissionPhase::Abort;
        assert_eq!(phase.icon(), "⊗");
        assert_eq!(phase.name(), "ABORT");
    }

    #[test]
    fn test_mission_phase_pause_icon_and_name() {
        let phase = MissionPhase::Pause;
        assert_eq!(phase.icon(), "⏸");
        assert_eq!(phase.name(), "PAUSED");
    }

    #[test]
    fn test_mission_phase_all_icons_unique() {
        let icons = [
            MissionPhase::Preflight.icon(),
            MissionPhase::Countdown.icon(),
            MissionPhase::Launch.icon(),
            MissionPhase::Orbital.icon(),
            MissionPhase::Rendezvous.icon(),
            MissionPhase::MissionSuccess.icon(),
            MissionPhase::Abort.icon(),
            MissionPhase::Pause.icon(),
        ];

        // All icons should be unique
        for i in 0..icons.len() {
            for j in (i + 1)..icons.len() {
                assert_ne!(
                    icons[i], icons[j],
                    "Icons at positions {} and {} are identical: {}",
                    i, j, icons[i]
                );
            }
        }
    }

    #[test]
    fn test_mission_phase_equality() {
        assert_eq!(MissionPhase::Preflight, MissionPhase::Preflight);
        assert_ne!(MissionPhase::Preflight, MissionPhase::Countdown);
        assert_ne!(MissionPhase::MissionSuccess, MissionPhase::Abort);
    }

    #[test]
    fn test_mission_phase_copy_clone() {
        let phase = MissionPhase::Orbital;
        let copied = phase;
        assert_eq!(phase, copied);
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

    // ═══════════════════════════════════════════════════════════════════════════
    // INTEGRATION TESTS
    // ═══════════════════════════════════════════════════════════════════════════

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
    fn test_color_mode_detect_consistency() {
        // Multiple calls to detect should return the same result
        // (unless environment variables change)
        let mode1 = ColorMode::detect();
        let mode2 = ColorMode::detect();
        assert_eq!(mode1, mode2);
    }

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
