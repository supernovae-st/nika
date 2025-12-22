//! Hyperspace Theme - Visual Design System
//!
//! Space violet/blue + amber/gold aesthetic inspired by Star Wars hyperspace.

use ratatui::style::{Color, Modifier, Style};

/// Hyperspace color palette
pub struct HyperspaceTheme {
    // Primary palette
    pub space_violet: Color,
    pub amber_gold: Color,
    pub cyan_teal: Color,
    pub deep_space: Color,
    pub star_white: Color,

    // Status colors
    pub success_green: Color,
    pub warning_orange: Color,
    pub error_red: Color,

    // Dimmed versions
    pub dim_violet: Color,
    pub dim_amber: Color,
    pub dim_cyan: Color,
}

impl Default for HyperspaceTheme {
    fn default() -> Self {
        Self {
            // Primary palette
            space_violet: Color::Rgb(138, 43, 226), // #8A2BE2
            amber_gold: Color::Rgb(255, 191, 0),    // #FFBF00
            cyan_teal: Color::Rgb(0, 255, 255),     // #00FFFF
            deep_space: Color::Rgb(13, 17, 23),     // #0D1117
            star_white: Color::Rgb(230, 237, 243),  // #E6EDF3

            // Status colors
            success_green: Color::Rgb(63, 185, 80), // #3FB950
            warning_orange: Color::Rgb(210, 153, 34), // #D29922
            error_red: Color::Rgb(248, 81, 73),     // #F85149

            // Dimmed versions
            dim_violet: Color::Rgb(88, 28, 143), // Darker violet
            dim_amber: Color::Rgb(153, 115, 0),  // Darker amber
            dim_cyan: Color::Rgb(0, 153, 153),   // Darker cyan
        }
    }
}

impl HyperspaceTheme {
    /// Create a new theme instance
    pub fn new() -> Self {
        Self::default()
    }

    // ─────────────────────────────────────────────────────────────────────
    // Paradigm Colors
    // ─────────────────────────────────────────────────────────────────────

    /// Color for Pure (⚡) paradigm tasks
    pub fn pure_color(&self) -> Color {
        self.amber_gold
    }

    /// Color for Context (🧠) paradigm tasks
    pub fn context_color(&self) -> Color {
        self.space_violet
    }

    /// Color for Isolated (🤖) paradigm tasks
    pub fn isolated_color(&self) -> Color {
        self.cyan_teal
    }

    // ─────────────────────────────────────────────────────────────────────
    // Styles
    // ─────────────────────────────────────────────────────────────────────

    /// Default text style
    pub fn text(&self) -> Style {
        Style::default().fg(self.star_white)
    }

    /// Dimmed text style
    pub fn dimmed(&self) -> Style {
        Style::default().fg(Color::Rgb(128, 128, 128))
    }

    /// Bold header style
    pub fn header(&self) -> Style {
        Style::default()
            .fg(self.space_violet)
            .add_modifier(Modifier::BOLD)
    }

    /// Accent style (amber)
    pub fn accent(&self) -> Style {
        Style::default().fg(self.amber_gold)
    }

    /// Highlight style (cyan)
    pub fn highlight(&self) -> Style {
        Style::default()
            .fg(self.cyan_teal)
            .add_modifier(Modifier::BOLD)
    }

    /// Success style
    pub fn success(&self) -> Style {
        Style::default().fg(self.success_green)
    }

    /// Warning style
    pub fn warning(&self) -> Style {
        Style::default().fg(self.warning_orange)
    }

    /// Error style
    pub fn error(&self) -> Style {
        Style::default()
            .fg(self.error_red)
            .add_modifier(Modifier::BOLD)
    }

    // ─────────────────────────────────────────────────────────────────────
    // Border Styles (Paradigm-specific)
    // ─────────────────────────────────────────────────────────────────────

    /// Border style for Pure tasks (thin amber)
    pub fn pure_border(&self) -> Style {
        Style::default().fg(self.amber_gold)
    }

    /// Border style for Context tasks (solid violet)
    pub fn context_border(&self) -> Style {
        Style::default()
            .fg(self.space_violet)
            .add_modifier(Modifier::BOLD)
    }

    /// Border style for Isolated tasks (cyan)
    pub fn isolated_border(&self) -> Style {
        Style::default().fg(self.cyan_teal)
    }

    // ─────────────────────────────────────────────────────────────────────
    // Activity Bar Colors
    // ─────────────────────────────────────────────────────────────────────

    /// Get color for activity bar based on percentage
    pub fn activity_color(&self, percent: f32) -> Color {
        match percent {
            p if p >= 80.0 => self.amber_gold,   // HIGH - bright amber
            p if p >= 60.0 => self.space_violet, // MED - violet
            p if p >= 40.0 => self.dim_violet,   // LOW - dim violet
            p if p >= 20.0 => self.error_red,    // CRIT - red
            _ => Color::Rgb(64, 64, 64),         // EMPTY - gray
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // Context Temperature Colors
    // ─────────────────────────────────────────────────────────────────────

    /// Get color for context temperature based on usage percentage
    pub fn temperature_color(&self, percent: f32) -> Color {
        match percent {
            p if p >= 90.0 => self.error_red,      // Critical
            p if p >= 75.0 => self.warning_orange, // Warning
            p if p >= 50.0 => self.amber_gold,     // Medium
            _ => self.success_green,               // Safe
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Icons and Symbols
// ─────────────────────────────────────────────────────────────────────────────

/// UI Icons used throughout the TUI
pub mod icons {
    // Paradigm icons
    pub const CONTEXT: &str = "🧠";
    pub const ISOLATED: &str = "🤖";

    // Element icons
    pub const MAIN_AGENT: &str = "◉";
    pub const SUBAGENT: &str = "○";
    pub const SKILL: &str = "◆";
    pub const MCP: &str = "▣";
    pub const PORTAL: &str = "◎";

    // Activity bar characters
    pub const BAR_FULL: char = '█';
    pub const BAR_EMPTY: char = '░';
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_defaults() {
        let theme = HyperspaceTheme::new();
        assert_eq!(theme.space_violet, Color::Rgb(138, 43, 226));
        assert_eq!(theme.amber_gold, Color::Rgb(255, 191, 0));
    }

    #[test]
    fn test_activity_color_ranges() {
        let theme = HyperspaceTheme::new();

        assert_eq!(theme.activity_color(100.0), theme.amber_gold);
        assert_eq!(theme.activity_color(80.0), theme.amber_gold);
        assert_eq!(theme.activity_color(60.0), theme.space_violet);
        assert_eq!(theme.activity_color(40.0), theme.dim_violet);
        assert_eq!(theme.activity_color(20.0), theme.error_red);
    }

    #[test]
    fn test_temperature_color_ranges() {
        let theme = HyperspaceTheme::new();

        assert_eq!(theme.temperature_color(95.0), theme.error_red);
        assert_eq!(theme.temperature_color(80.0), theme.warning_orange);
        assert_eq!(theme.temperature_color(60.0), theme.amber_gold);
        assert_eq!(theme.temperature_color(30.0), theme.success_green);
    }
}
