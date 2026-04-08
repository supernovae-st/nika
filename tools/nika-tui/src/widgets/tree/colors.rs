// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Solarized color palette for tree widget.
//!
//! Provides consistent colors for different node types across light/dark themes.

use super::NodeKind;
use ratatui::style::Color;

/// Solarized color palette
pub mod solarized {
    use ratatui::style::Color;

    // Base colors
    pub const BASE03: Color = Color::Rgb(0, 43, 54); // #002b36 - darkest
    pub const BASE02: Color = Color::Rgb(7, 54, 66); // #073642
    pub const BASE01: Color = Color::Rgb(88, 110, 117); // #586e75
    pub const BASE00: Color = Color::Rgb(101, 123, 131); // #657b83
    pub const BASE0: Color = Color::Rgb(131, 148, 150); // #839496
    pub const BASE1: Color = Color::Rgb(147, 161, 161); // #93a1a1
    pub const BASE2: Color = Color::Rgb(238, 232, 213); // #eee8d5
    pub const BASE3: Color = Color::Rgb(253, 246, 227); // #fdf6e3 - lightest

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

/// Tree color configuration
#[derive(Debug, Clone)]
pub struct TreeColors {
    /// Background color
    pub bg: Color,
    /// Default text color
    pub fg: Color,
    /// Selected item background
    pub selected_bg: Color,
    /// Selected item foreground
    pub selected_fg: Color,
    /// Directory color
    pub directory: Color,
    /// File color
    pub file: Color,
    /// Hidden item color (dimmed)
    pub hidden: Color,
    /// Ecosystem file color (premium)
    pub ecosystem: Color,
    /// Ecosystem glow color (hover effect)
    pub ecosystem_glow: Color,
    /// Git modified color
    pub git_modified: Color,
    /// Git added color
    pub git_added: Color,
    /// Git deleted color
    pub git_deleted: Color,
    /// Git untracked color
    pub git_untracked: Color,
    /// Indent guide color
    pub indent_guide: Color,
    /// Tree branch lines color
    pub branch_lines: Color,
    /// Dimmed color for non-ecosystem files
    pub dimmed: Color,
}

impl Default for TreeColors {
    fn default() -> Self {
        Self::solarized_dark()
    }
}

impl TreeColors {
    /// Solarized Dark theme
    pub fn solarized_dark() -> Self {
        Self {
            bg: solarized::BASE03,
            fg: solarized::BASE0,
            selected_bg: solarized::BASE02,
            selected_fg: solarized::BASE1,
            directory: solarized::BLUE,
            file: solarized::BASE0,
            hidden: solarized::BASE01,
            ecosystem: solarized::YELLOW,
            ecosystem_glow: solarized::ORANGE,
            git_modified: solarized::YELLOW,
            git_added: solarized::GREEN,
            git_deleted: solarized::RED,
            git_untracked: solarized::CYAN,
            indent_guide: solarized::BASE01,
            branch_lines: solarized::BASE01,
            dimmed: solarized::BASE01, // 50% opacity effect via muted color
        }
    }

    /// Create TreeColors from a Theme (Cosmic-aware)
    ///
    /// Maps the app's Theme colors to tree-specific colors, ensuring the
    /// file browser background matches the rest of the UI (no BASE03 gap).
    pub fn from_theme(theme: &crate::theme::Theme) -> Self {
        Self {
            bg: theme.background,
            fg: theme.text_primary,
            selected_bg: theme.selection,
            selected_fg: theme.text_primary,
            directory: solarized::BLUE,
            file: theme.text_secondary,
            hidden: theme.text_muted,
            ecosystem: solarized::YELLOW,
            ecosystem_glow: solarized::ORANGE,
            git_modified: theme.git_modified,
            git_added: theme.git_added,
            git_deleted: theme.git_deleted,
            git_untracked: solarized::CYAN,
            indent_guide: theme.text_muted,
            branch_lines: theme.text_muted,
            dimmed: theme.text_muted,
        }
    }

    /// Solarized Light theme
    pub fn solarized_light() -> Self {
        Self {
            bg: solarized::BASE3,
            fg: solarized::BASE00,
            selected_bg: solarized::BASE2,
            selected_fg: solarized::BASE01,
            directory: solarized::BLUE,
            file: solarized::BASE00,
            hidden: solarized::BASE1,
            ecosystem: solarized::YELLOW,
            ecosystem_glow: solarized::ORANGE,
            git_modified: solarized::YELLOW,
            git_added: solarized::GREEN,
            git_deleted: solarized::RED,
            git_untracked: solarized::CYAN,
            indent_guide: solarized::BASE1,
            branch_lines: solarized::BASE1,
            dimmed: solarized::BASE1, // 50% opacity effect via muted color
        }
    }

    /// Get color for a specific node kind
    pub fn node_color(&self, kind: NodeKind, is_hidden: bool) -> Color {
        if is_hidden {
            return self.hidden;
        }

        match kind {
            // NIKA ecosystem (premium yellow)
            NodeKind::NikaWorkflow
            | NodeKind::NikaFolder
            | NodeKind::SonAgent
            | NodeKind::SkillFile
            | NodeKind::WorkflowsFolder
            | NodeKind::AgentsFolder
            | NodeKind::SkillsFolder => self.ecosystem,

            // NOVANET (magenta)
            NodeKind::NovanetFolder
            | NodeKind::BrainFolder
            | NodeKind::ModelsFolder
            | NodeKind::SeedFolder => solarized::MAGENTA,

            // Claude Code DX (violet)
            NodeKind::ClaudeFolder | NodeKind::ClaudeMd => solarized::VIOLET,

            // UX: Non-ecosystem files are dimmed to emphasize Nika workflows
            // Directories (dimmed blue)
            NodeKind::Directory
            | NodeKind::SrcFolder
            | NodeKind::TestsFolder
            | NodeKind::DocsFolder
            | NodeKind::ExamplesFolder
            | NodeKind::BenchesFolder => self.dimmed,

            // Special files (dimmed)
            NodeKind::Readme | NodeKind::Changelog | NodeKind::Roadmap => self.dimmed,

            // Config files (dimmed)
            NodeKind::CargoToml | NodeKind::PackageJson | NodeKind::Toml | NodeKind::Json => {
                self.dimmed
            }

            // Code files (dimmed)
            NodeKind::Rust | NodeKind::TypeScript | NodeKind::JavaScript => self.dimmed,

            // Markdown/Yaml non-Nika (dimmed)
            NodeKind::Markdown | NodeKind::Yaml => self.dimmed,

            // Hidden folders (extra dimmed)
            NodeKind::GitFolder | NodeKind::NodeModules | NodeKind::TargetFolder => self.hidden,

            // Default files (dimmed)
            NodeKind::File => self.dimmed,
        }
    }

    /// Get icon color (may differ from text color for emphasis)
    pub fn icon_color(&self, kind: NodeKind) -> Color {
        match kind {
            // Ecosystem files get glow on icon
            NodeKind::NikaWorkflow
            | NodeKind::SonAgent
            | NodeKind::SkillFile
            | NodeKind::ClaudeMd => self.ecosystem_glow,

            // Everything else uses node color
            _ => self.node_color(kind, false),
        }
    }

    /// Check if a node kind should have glow animation
    pub fn has_glow(&self, kind: NodeKind) -> bool {
        matches!(
            kind,
            NodeKind::NikaWorkflow
                | NodeKind::NikaFolder
                | NodeKind::SonAgent
                | NodeKind::SkillFile
                | NodeKind::NovanetFolder
                | NodeKind::ClaudeFolder
                | NodeKind::ClaudeMd
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_solarized_dark_colors() {
        let colors = TreeColors::solarized_dark();
        assert_eq!(colors.bg, solarized::BASE03);
        assert_eq!(colors.ecosystem, solarized::YELLOW);
    }

    #[test]
    fn test_solarized_light_colors() {
        let colors = TreeColors::solarized_light();
        assert_eq!(colors.bg, solarized::BASE3);
        assert_eq!(colors.ecosystem, solarized::YELLOW);
    }

    #[test]
    fn test_ecosystem_node_color() {
        let colors = TreeColors::solarized_dark();
        assert_eq!(
            colors.node_color(NodeKind::NikaWorkflow, false),
            solarized::YELLOW
        );
        assert_eq!(
            colors.node_color(NodeKind::SonAgent, false),
            solarized::YELLOW
        );
    }

    #[test]
    fn test_hidden_node_color() {
        let colors = TreeColors::solarized_dark();
        assert_eq!(
            colors.node_color(NodeKind::NikaWorkflow, true),
            colors.hidden
        );
        assert_eq!(colors.node_color(NodeKind::File, true), colors.hidden);
    }

    #[test]
    fn test_novanet_node_color() {
        let colors = TreeColors::solarized_dark();
        assert_eq!(
            colors.node_color(NodeKind::NovanetFolder, false),
            solarized::MAGENTA
        );
        assert_eq!(
            colors.node_color(NodeKind::BrainFolder, false),
            solarized::MAGENTA
        );
    }

    #[test]
    fn test_claude_node_color() {
        let colors = TreeColors::solarized_dark();
        assert_eq!(
            colors.node_color(NodeKind::ClaudeFolder, false),
            solarized::VIOLET
        );
        assert_eq!(
            colors.node_color(NodeKind::ClaudeMd, false),
            solarized::VIOLET
        );
    }

    #[test]
    fn test_has_glow() {
        let colors = TreeColors::solarized_dark();
        assert!(colors.has_glow(NodeKind::NikaWorkflow));
        assert!(colors.has_glow(NodeKind::NovanetFolder));
        assert!(colors.has_glow(NodeKind::ClaudeMd));
        assert!(!colors.has_glow(NodeKind::File));
        assert!(!colors.has_glow(NodeKind::Directory));
    }

    #[test]
    fn test_icon_color_glow() {
        let colors = TreeColors::solarized_dark();
        // Ecosystem files get glow color for icons
        assert_eq!(
            colors.icon_color(NodeKind::NikaWorkflow),
            colors.ecosystem_glow
        );
        // Regular files use dimmed color (non-ecosystem are de-emphasized)
        assert_eq!(colors.icon_color(NodeKind::File), colors.dimmed);
    }
}
