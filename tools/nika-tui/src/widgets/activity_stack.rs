// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Activity data types
//!
//! Hot (executing), warm (recent), and queued activity items.
//! Used by ChatView for real-time task status display.

use std::time::{Duration, Instant};

use ratatui::style::Color;

// ═══════════════════════════════════════════════════════════════════════════
// DEFAULT COLORS (fallbacks for theme-aware rendering)
// ═══════════════════════════════════════════════════════════════════════════

const DEFAULT_HOT_COLOR: Color = Color::Rgb(251, 146, 60); // orange
const DEFAULT_WARM_COLOR: Color = Color::Rgb(250, 204, 21); // yellow
const DEFAULT_QUEUED_COLOR: Color = Color::Rgb(156, 163, 175); // gray

/// Activity temperature
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActivityTemp {
    /// Currently executing
    Hot,
    /// Recently completed (< 30s)
    Warm,
    /// Waiting for dependencies
    #[default]
    Queued,
}

impl ActivityTemp {
    pub fn header(&self) -> (&'static str, Color) {
        match self {
            Self::Hot => ("🔥 HOT (executing now)", DEFAULT_HOT_COLOR),
            Self::Warm => ("🟡 WARM (recently completed)", DEFAULT_WARM_COLOR),
            Self::Queued => ("⚪ QUEUED (waiting)", DEFAULT_QUEUED_COLOR),
        }
    }
}

/// Activity item
#[derive(Debug, Clone)]
pub struct ActivityItem {
    pub id: String,
    pub verb: String,
    pub temp: ActivityTemp,
    pub started: Option<Instant>,
    pub duration: Option<Duration>,
    pub tokens: Option<(u64, u64)>, // (in, out)
    pub waiting_on: Option<String>,
    pub detail: Option<String>,
    pub frame: u8,
}

impl ActivityItem {
    /// Create a hot (currently executing) activity
    pub fn hot(id: impl Into<String>, verb: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            verb: verb.into(),
            temp: ActivityTemp::Hot,
            started: Some(Instant::now()),
            duration: None,
            tokens: None,
            waiting_on: None,
            detail: None,
            frame: 0,
        }
    }

    /// Create a warm (recently completed) activity
    pub fn warm(id: impl Into<String>, verb: impl Into<String>, duration: Duration) -> Self {
        Self {
            id: id.into(),
            verb: verb.into(),
            temp: ActivityTemp::Warm,
            started: None,
            duration: Some(duration),
            tokens: None,
            waiting_on: None,
            detail: None,
            frame: 0,
        }
    }

    /// Create a queued (waiting) activity
    pub fn queued(
        id: impl Into<String>,
        verb: impl Into<String>,
        waiting_on: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            verb: verb.into(),
            temp: ActivityTemp::Queued,
            started: None,
            duration: None,
            tokens: None,
            waiting_on: Some(waiting_on.into()),
            detail: None,
            frame: 0,
        }
    }

    pub fn with_tokens(mut self, tokens_in: u64, tokens_out: u64) -> Self {
        self.tokens = Some((tokens_in, tokens_out));
        self
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn with_frame(mut self, frame: u8) -> Self {
        self.frame = frame;
        self
    }

    /// Get elapsed time for hot items
    pub fn elapsed(&self) -> Option<Duration> {
        self.started.map(|s| s.elapsed())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_activity_item_hot() {
        let item = ActivityItem::hot("task1", "infer");
        assert_eq!(item.id, "task1");
        assert_eq!(item.verb, "infer");
        assert_eq!(item.temp, ActivityTemp::Hot);
        assert!(item.started.is_some());
    }

    #[test]
    fn test_activity_item_warm() {
        let item = ActivityItem::warm("task2", "exec", Duration::from_secs(5));
        assert_eq!(item.temp, ActivityTemp::Warm);
        assert_eq!(item.duration, Some(Duration::from_secs(5)));
    }

    #[test]
    fn test_activity_item_queued() {
        let item = ActivityItem::queued("task3", "fetch", "task1");
        assert_eq!(item.temp, ActivityTemp::Queued);
        assert_eq!(item.waiting_on, Some("task1".to_string()));
    }

    #[test]
    fn test_with_tokens() {
        let item = ActivityItem::hot("t", "infer").with_tokens(100, 50);
        assert_eq!(item.tokens, Some((100, 50)));
    }

    #[test]
    fn test_with_detail() {
        let item =
            ActivityItem::warm("t", "exec", Duration::from_secs(1)).with_detail("build completed");
        assert_eq!(item.detail, Some("build completed".to_string()));
    }

    #[test]
    fn test_elapsed() {
        let item = ActivityItem::hot("t", "infer");
        // Elapsed should be very small (just created)
        let elapsed = item.elapsed();
        assert!(elapsed.is_some());
        assert!(elapsed.unwrap() < Duration::from_secs(1));

        // Queued items have no elapsed
        let item = ActivityItem::queued("t", "exec", "other");
        assert!(item.elapsed().is_none());
    }

    #[test]
    fn test_activity_temp_headers() {
        let (header, _) = ActivityTemp::Hot.header();
        assert!(header.contains("HOT"));

        let (header, _) = ActivityTemp::Warm.header();
        assert!(header.contains("WARM"));

        let (header, _) = ActivityTemp::Queued.header();
        assert!(header.contains("QUEUED"));
    }

    #[test]
    fn test_default_temp() {
        assert_eq!(ActivityTemp::default(), ActivityTemp::Queued);
    }
}
