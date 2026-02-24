//! Verb Color Taxonomy
//!
//! Tailwind-based color palette for the 5 semantic verbs.
//! Each verb has a distinct color for visual differentiation.

use ratatui::style::Color;

/// Verb types with associated colors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VerbColor {
    /// ⚡ infer: LLM generation (Violet 500)
    Infer,
    /// 📟 exec: Shell command (Amber 500)
    Exec,
    /// 🛰️ fetch: HTTP request (Cyan 500)
    Fetch,
    /// 🔌 invoke: MCP tool call (Emerald 500)
    Invoke,
    /// 🐔 agent: Multi-turn agentic loop (Rose 500)
    Agent,
    /// 🐤 spawn: Spawned sub-agent (Rose 300)
    Spawn,
}

impl VerbColor {
    /// Get the RGB color for this verb
    pub fn rgb(&self) -> Color {
        match self {
            Self::Infer => Color::Rgb(139, 92, 246), // Violet 500 #8b5cf6
            Self::Exec => Color::Rgb(245, 158, 11),  // Amber 500 #f59e0b
            Self::Fetch => Color::Rgb(6, 182, 212),  // Cyan 500 #06b6d4
            Self::Invoke => Color::Rgb(16, 185, 129), // Emerald 500 #10b981
            Self::Agent => Color::Rgb(244, 63, 94),  // Rose 500 #f43f5e
            Self::Spawn => Color::Rgb(253, 164, 175), // Rose 300 #fda4af
        }
    }

    /// Get the hex color string for this verb
    pub fn hex(&self) -> &'static str {
        match self {
            Self::Infer => "#8b5cf6",
            Self::Exec => "#f59e0b",
            Self::Fetch => "#06b6d4",
            Self::Invoke => "#10b981",
            Self::Agent => "#f43f5e",
            Self::Spawn => "#fda4af",
        }
    }

    /// Get the icon for this verb
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Infer => "⚡",
            Self::Exec => "📟",
            Self::Fetch => "🛰️",
            Self::Invoke => "🔌",
            Self::Agent => "🐔",
            Self::Spawn => "🐤",
        }
    }

    /// Get the label for this verb
    pub fn label(&self) -> &'static str {
        match self {
            Self::Infer => "INFER",
            Self::Exec => "EXEC",
            Self::Fetch => "FETCH",
            Self::Invoke => "INVOKE",
            Self::Agent => "AGENT",
            Self::Spawn => "SPAWN",
        }
    }

    /// Get icon and label combined
    pub fn icon_label(&self) -> String {
        format!("{} {}", self.icon(), self.label())
    }

    /// Darker variant for borders (subtract ~20 from RGB)
    pub fn border_rgb(&self) -> Color {
        match self {
            Self::Infer => Color::Rgb(124, 58, 237), // Violet 600 #7c3aed
            Self::Exec => Color::Rgb(217, 119, 6),   // Amber 600 #d97706
            Self::Fetch => Color::Rgb(8, 145, 178),  // Cyan 600 #0891b2
            Self::Invoke => Color::Rgb(5, 150, 105), // Emerald 600 #059669
            Self::Agent => Color::Rgb(225, 29, 72),  // Rose 600 #e11d48
            Self::Spawn => Color::Rgb(251, 113, 133), // Rose 400 #fb7185
        }
    }

    /// Light variant for backgrounds
    pub fn light_rgb(&self) -> Color {
        match self {
            Self::Infer => Color::Rgb(167, 139, 250), // Violet 400 #a78bfa
            Self::Exec => Color::Rgb(251, 191, 36),   // Amber 400 #fbbf24
            Self::Fetch => Color::Rgb(34, 211, 238),  // Cyan 400 #22d3ee
            Self::Invoke => Color::Rgb(52, 211, 153), // Emerald 400 #34d399
            Self::Agent => Color::Rgb(251, 113, 133), // Rose 400 #fb7185
            Self::Spawn => Color::Rgb(254, 205, 211), // Rose 200 #fecdd3
        }
    }
}

/// Status colors (shared across all verbs)
pub mod status {
    use ratatui::style::Color;

    /// Success green (Tailwind Green 500)
    pub const SUCCESS: Color = Color::Rgb(34, 197, 94); // #22c55e

    /// Error red (Tailwind Red 500)
    pub const ERROR: Color = Color::Rgb(239, 68, 68); // #ef4444

    /// Warning amber (Tailwind Amber 500)
    pub const WARNING: Color = Color::Rgb(245, 158, 11); // #f59e0b

    /// Running/active yellow (Tailwind Yellow 400)
    pub const RUNNING: Color = Color::Rgb(250, 204, 21); // #facc15

    /// Muted/disabled gray (Tailwind Slate 500)
    pub const MUTED: Color = Color::Rgb(100, 116, 139); // #64748b

    /// Info blue (Tailwind Blue 500)
    pub const INFO: Color = Color::Rgb(59, 130, 246); // #3b82f6
}

/// HTTP status code colors
pub mod http {
    use ratatui::style::Color;

    /// Get color for HTTP status code
    pub fn status_color(code: u16) -> Color {
        match code {
            200..=299 => Color::Rgb(34, 197, 94),  // Green - success
            300..=399 => Color::Rgb(6, 182, 212),  // Cyan - redirect
            400..=499 => Color::Rgb(245, 158, 11), // Amber - client error
            500..=599 => Color::Rgb(239, 68, 68),  // Red - server error
            _ => Color::Rgb(100, 116, 139),        // Gray - unknown
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
    use ratatui::style::Color;

    /// Get color for exit code
    pub fn code_color(code: i32) -> Color {
        match code {
            0 => Color::Rgb(34, 197, 94),         // Green - success
            126 => Color::Rgb(245, 158, 11),      // Amber - permission denied
            127 => Color::Rgb(245, 158, 11),      // Amber - command not found
            128..=255 => Color::Rgb(239, 68, 68), // Red - signal termination
            _ => Color::Rgb(239, 68, 68),         // Red - other errors
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }

    #[test]
    fn test_icon_label() {
        assert_eq!(VerbColor::Infer.icon_label(), "⚡ INFER");
        assert_eq!(VerbColor::Agent.icon_label(), "🐔 AGENT");
    }

    #[test]
    fn test_rgb_colors() {
        assert_eq!(VerbColor::Infer.rgb(), Color::Rgb(139, 92, 246));
        assert_eq!(VerbColor::Agent.rgb(), Color::Rgb(244, 63, 94));
    }

    #[test]
    fn test_http_status_colors() {
        use http::status_color;

        // 2xx - green
        assert_eq!(status_color(200), Color::Rgb(34, 197, 94));
        assert_eq!(status_color(201), Color::Rgb(34, 197, 94));

        // 3xx - cyan
        assert_eq!(status_color(301), Color::Rgb(6, 182, 212));

        // 4xx - amber
        assert_eq!(status_color(404), Color::Rgb(245, 158, 11));

        // 5xx - red
        assert_eq!(status_color(500), Color::Rgb(239, 68, 68));
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
        assert_eq!(code_color(0), Color::Rgb(34, 197, 94));

        // Permission/command errors
        assert_eq!(code_color(126), Color::Rgb(245, 158, 11));
        assert_eq!(code_color(127), Color::Rgb(245, 158, 11));

        // Signal termination
        assert_eq!(code_color(137), Color::Rgb(239, 68, 68)); // SIGKILL

        // Other errors
        assert_eq!(code_color(1), Color::Rgb(239, 68, 68));
    }

    #[test]
    fn test_status_constants() {
        assert_eq!(status::SUCCESS, Color::Rgb(34, 197, 94));
        assert_eq!(status::ERROR, Color::Rgb(239, 68, 68));
        assert_eq!(status::WARNING, Color::Rgb(245, 158, 11));
    }
}
