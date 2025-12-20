//! TUI Widgets - UI Components
//!
//! Each widget is a stateless renderer that takes ViewState and produces
//! Ratatui primitives. Business logic stays in the Domain Layer.

// Widget modules will be added as we implement them:
// mod header;
// mod dag;
// mod session;
// mod subagents;
// mod activity;
// mod connections;
// mod skills;
// mod memory;
// mod context;
// mod footer;

// For now, widgets are implemented inline in app.rs
// They will be extracted into separate modules in Phase 2

/// Common widget utilities
pub mod utils {
    use ratatui::style::Color;

    /// Create a gradient color based on percentage
    pub fn gradient_color(percent: f32, low: Color, mid: Color, high: Color) -> Color {
        if percent < 50.0 {
            low
        } else if percent < 80.0 {
            mid
        } else {
            high
        }
    }

    /// Format duration as HH:MM:SS
    pub fn format_duration(secs: u64) -> String {
        format!(
            "{:02}:{:02}:{:02}",
            secs / 3600,
            (secs % 3600) / 60,
            secs % 60
        )
    }

    /// Truncate string with ellipsis
    pub fn truncate(s: &str, max_len: usize) -> String {
        if s.len() <= max_len {
            s.to_string()
        } else if max_len <= 3 {
            s[..max_len].to_string()
        } else {
            format!("{}...", &s[..max_len - 3])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::utils::*;

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(0), "00:00:00");
        assert_eq!(format_duration(61), "00:01:01");
        assert_eq!(format_duration(3661), "01:01:01");
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 8), "hello...");
        assert_eq!(truncate("hi", 2), "hi");
    }
}
