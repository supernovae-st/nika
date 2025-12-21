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

    /// Seconds per hour (for time formatting)
    const SECONDS_PER_HOUR: u64 = 3600;
    /// Seconds per minute (for time formatting)
    const SECONDS_PER_MINUTE: u64 = 60;

    /// Gradient threshold for "medium" level
    const GRADIENT_MID_THRESHOLD: f32 = 50.0;
    /// Gradient threshold for "high" level
    const GRADIENT_HIGH_THRESHOLD: f32 = 80.0;

    /// Ellipsis suffix length
    const ELLIPSIS_LEN: usize = 3;

    /// Create a gradient color based on percentage
    pub fn gradient_color(percent: f32, low: Color, mid: Color, high: Color) -> Color {
        if percent < GRADIENT_MID_THRESHOLD {
            low
        } else if percent < GRADIENT_HIGH_THRESHOLD {
            mid
        } else {
            high
        }
    }

    /// Format duration as HH:MM:SS
    pub fn format_duration(secs: u64) -> String {
        format!(
            "{:02}:{:02}:{:02}",
            secs / SECONDS_PER_HOUR,
            (secs % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE,
            secs % SECONDS_PER_MINUTE
        )
    }

    /// Truncate string with ellipsis
    pub fn truncate(s: &str, max_len: usize) -> String {
        if s.len() <= max_len {
            s.to_string()
        } else if max_len <= ELLIPSIS_LEN {
            s[..max_len].to_string()
        } else {
            format!("{}...", &s[..max_len - ELLIPSIS_LEN])
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
