// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Verb Color Taxonomy
//!
//! Re-exports VerbColor from theme.rs (single source of truth).
//! Provides domain-specific color utilities for HTTP and exit codes.
//!
//! Uses centralized color tokens from `tui::tokens::compat` module.

// Re-export canonical VerbColor from theme
pub use crate::theme::VerbColor;

/// Status colors (shared across all verbs)
pub mod status {
    use crate::theme::Theme;
    use crate::tokens::compat;
    use ratatui::style::Color;

    /// Success green (Tailwind Green 500)
    pub const SUCCESS: Color = compat::GREEN_500;

    /// Error red (Tailwind Red 500)
    pub const ERROR: Color = compat::RED_500;

    /// Warning amber (Tailwind Amber 500)
    pub const WARNING: Color = compat::AMBER_500;

    /// Running/active yellow (Tailwind Yellow 400)
    pub const RUNNING: Color = compat::YELLOW_400;

    /// Muted/disabled gray (Tailwind Slate 500)
    pub const MUTED: Color = compat::SLATE_500;

    /// Info blue (Tailwind Blue 500)
    pub const INFO: Color = compat::BLUE_500;

    /// Get status color with theme support (fallback to constant)
    pub fn success(theme: Option<&Theme>) -> Color {
        theme.map(|t| t.status_success).unwrap_or(SUCCESS)
    }

    /// Get error color with theme support
    pub fn error(theme: Option<&Theme>) -> Color {
        theme.map(|t| t.status_failed).unwrap_or(ERROR)
    }

    /// Get warning color with theme support
    pub fn warning(theme: Option<&Theme>) -> Color {
        theme.map(|t| t.status_running).unwrap_or(WARNING)
    }

    /// Get muted color with theme support
    pub fn muted(theme: Option<&Theme>) -> Color {
        theme.map(|t| t.text_muted).unwrap_or(MUTED)
    }
}

/// HTTP status code colors
pub mod http {
    use crate::tokens::compat;
    use ratatui::style::Color;

    /// Get color for HTTP status code
    pub fn status_color(code: u16) -> Color {
        match code {
            200..=299 => compat::GREEN_500, // Green - success
            300..=399 => compat::CYAN_500,  // Cyan - redirect
            400..=499 => compat::AMBER_500, // Amber - client error
            500..=599 => compat::RED_500,   // Red - server error
            _ => compat::SLATE_500,         // Gray - unknown
        }
    }

    /// Get status text for code
    pub fn status_text(code: u16) -> &'static str {
        match code {
            200 => "OK",
            201 => "Created",
            204 => "No Content",
            301 => "Moved Permanently",
            302 => "Found",
            304 => "Not Modified",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            429 => "Too Many Requests",
            500 => "Internal Server Error",
            502 => "Bad Gateway",
            503 => "Service Unavailable",
            504 => "Gateway Timeout",
            _ => "Unknown",
        }
    }
}

/// Exit code colors for exec verb
pub mod exit {
    use crate::tokens::compat;
    use ratatui::style::Color;

    /// Get color for exit code
    pub fn code_color(code: i32) -> Color {
        match code {
            0 => compat::GREEN_500,       // Green - success
            126 => compat::AMBER_500,     // Amber - permission denied
            127 => compat::AMBER_500,     // Amber - command not found
            128..=255 => compat::RED_500, // Red - signal termination
            _ => compat::RED_500,         // Red - other errors
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;
    use crate::tokens::compat;
    use ratatui::style::Color;

    #[test]
    fn test_verbcolor_reexport_matches_theme() {
        use crate::theme::VerbColor as ThemeVerbColor;

        // Both should be the same type after consolidation
        let theme_infer: ThemeVerbColor = ThemeVerbColor::Infer;
        let taskbox_infer: VerbColor = VerbColor::Infer;

        // They're the same type, so this compiles
        assert_eq!(theme_infer.rgb(), taskbox_infer.rgb());
        assert_eq!(theme_infer.hex(), taskbox_infer.hex());
        assert_eq!(theme_infer.icon(), taskbox_infer.icon());
        assert_eq!(theme_infer.label(), taskbox_infer.label());
    }

    #[test]
    fn test_verbcolor_spawn_via_reexport() {
        // Spawn should be accessible via re-export
        let spawn = VerbColor::Spawn;
        assert_eq!(spawn.hex(), "#fda4af");
        assert_eq!(spawn.icon(), "🐤");
        assert_eq!(spawn.label(), "SPAWN");
    }

    #[test]
    fn test_verb_colors() {
        assert_eq!(VerbColor::Infer.hex(), "#8b5cf6");
        assert_eq!(VerbColor::Exec.hex(), "#f59e0b");
        assert_eq!(VerbColor::Fetch.hex(), "#06b6d4");
        assert_eq!(VerbColor::Invoke.hex(), "#10b981");
        assert_eq!(VerbColor::Agent.hex(), "#f43f5e");
        assert_eq!(VerbColor::Spawn.hex(), "#fda4af");
    }

    #[test]
    fn test_verb_icons() {
        assert_eq!(VerbColor::Infer.icon(), "⚡");
        assert_eq!(VerbColor::Exec.icon(), "📟");
        assert_eq!(VerbColor::Fetch.icon(), "🛰️");
        assert_eq!(VerbColor::Invoke.icon(), "🔌");
        assert_eq!(VerbColor::Agent.icon(), "🐔");
        assert_eq!(VerbColor::Spawn.icon(), "🐤");
    }

    #[test]
    fn test_verb_labels() {
        assert_eq!(VerbColor::Infer.label(), "INFER");
        assert_eq!(VerbColor::Agent.label(), "AGENT");
        assert_eq!(VerbColor::Spawn.label(), "SPAWN");
    }

    #[test]
    fn test_rgb_colors() {
        assert_eq!(VerbColor::Infer.rgb(), compat::VIOLET_500);
        assert_eq!(VerbColor::Agent.rgb(), compat::ROSE_500);
        assert_eq!(VerbColor::Spawn.rgb(), Color::Rgb(253, 164, 175)); // rose_300
    }

    #[test]
    fn test_http_status_colors() {
        use http::status_color;

        // 2xx - green
        assert_eq!(status_color(200), compat::GREEN_500);
        assert_eq!(status_color(201), compat::GREEN_500);

        // 3xx - cyan
        assert_eq!(status_color(301), compat::CYAN_500);

        // 4xx - amber
        assert_eq!(status_color(404), compat::AMBER_500);

        // 5xx - red
        assert_eq!(status_color(500), compat::RED_500);
    }

    #[test]
    fn test_http_status_text() {
        use http::status_text;

        assert_eq!(status_text(200), "OK");
        assert_eq!(status_text(404), "Not Found");
        assert_eq!(status_text(500), "Internal Server Error");
        assert_eq!(status_text(999), "Unknown");
    }

    #[test]
    fn test_exit_code_colors() {
        use exit::code_color;

        // Success
        assert_eq!(code_color(0), compat::GREEN_500);

        // Permission/command errors
        assert_eq!(code_color(126), compat::AMBER_500);
        assert_eq!(code_color(127), compat::AMBER_500);

        // Signal termination
        assert_eq!(code_color(137), compat::RED_500); // SIGKILL

        // Other errors
        assert_eq!(code_color(1), compat::RED_500);
    }

    #[test]
    fn test_status_constants() {
        assert_eq!(status::SUCCESS, compat::GREEN_500);
        assert_eq!(status::ERROR, compat::RED_500);
        assert_eq!(status::WARNING, compat::AMBER_500);
    }

    #[test]
    fn test_status_with_theme() {
        let dark = Theme::dark();
        let light = Theme::light();

        // With theme, uses theme colors
        assert_eq!(status::success(Some(&dark)), dark.status_success);
        assert_eq!(status::error(Some(&light)), light.status_failed);

        // Without theme, uses fallback constants
        assert_eq!(status::success(None), status::SUCCESS);
        assert_eq!(status::error(None), status::ERROR);
    }
}
