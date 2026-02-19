//! TUI Theme - NovaNet Taxonomy Colors
//!
//! Color palette derived from NovaNet's visual-encoding.yaml.
//! Provides consistent styling across all TUI components.

use ratatui::style::{Color, Modifier, Style};

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
            realm_shared: Color::Rgb(59, 130, 246),  // #3B82F6 blue
            realm_org: Color::Rgb(16, 185, 129),     // #10B981 emerald

            // Traits
            trait_defined: Color::Rgb(107, 114, 128),   // #6B7280 gray
            trait_authored: Color::Rgb(139, 92, 246),   // #8B5CF6 violet
            trait_imported: Color::Rgb(245, 158, 11),   // #F59E0B amber
            trait_generated: Color::Rgb(16, 185, 129),  // #10B981 emerald
            trait_retrieved: Color::Rgb(6, 182, 212),   // #06B6D4 cyan

            // Status
            status_pending: Color::Rgb(107, 114, 128),  // #6B7280 gray
            status_running: Color::Rgb(245, 158, 11),   // #F59E0B amber
            status_success: Color::Rgb(34, 197, 94),    // #22C55E green
            status_failed: Color::Rgb(239, 68, 68),     // #EF4444 red
            status_paused: Color::Rgb(6, 182, 212),     // #06B6D4 cyan

            // MCP tools
            mcp_describe: Color::Rgb(59, 130, 246),   // #3B82F6 blue
            mcp_traverse: Color::Rgb(236, 72, 153),   // #EC4899 pink
            mcp_search: Color::Rgb(245, 158, 11),     // #F59E0B amber
            mcp_atoms: Color::Rgb(139, 92, 246),      // #8B5CF6 violet
            mcp_generate: Color::Rgb(16, 185, 129),   // #10B981 emerald

            // UI elements
            border_normal: Color::Rgb(75, 85, 99),    // #4B5563 gray-600
            border_focused: Color::Rgb(99, 102, 241), // #6366F1 indigo
            text_primary: Color::Rgb(243, 244, 246),  // #F3F4F6 gray-100
            text_secondary: Color::Rgb(156, 163, 175), // #9CA3AF gray-400
            text_muted: Color::Rgb(107, 114, 128),    // #6B7280 gray-500
            background: Color::Rgb(17, 24, 39),       // #111827 gray-900
            highlight: Color::Rgb(99, 102, 241),      // #6366F1 indigo
        }
    }
}

impl Theme {
    /// Create the default NovaNet theme
    pub fn novanet() -> Self {
        Self::default()
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
}
