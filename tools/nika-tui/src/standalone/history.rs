// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Execution history for standalone TUI mode.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

/// Execution history entry (persisted to ~/.nika/history.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Path to the workflow file
    pub workflow_path: PathBuf,
    /// Timestamp of execution
    pub timestamp: SystemTime,
    /// Duration of execution
    pub duration_ms: u64,
    /// Number of tasks
    pub task_count: usize,
    /// Success or failure
    pub success: bool,
    /// Brief summary (first task output or error)
    pub summary: String,
}

impl HistoryEntry {
    pub fn new(
        workflow_path: PathBuf,
        duration: Duration,
        task_count: usize,
        success: bool,
        summary: String,
    ) -> Self {
        Self {
            workflow_path,
            timestamp: SystemTime::now(),
            duration_ms: duration.as_millis() as u64,
            task_count,
            success,
            summary,
        }
    }

    /// Format duration for display
    pub fn duration_display(&self) -> String {
        let secs = self.duration_ms as f64 / 1000.0;
        if secs < 60.0 {
            format!("{:.1}s", secs)
        } else {
            let mins = secs / 60.0;
            format!("{:.1}m", mins)
        }
    }

    /// Format timestamp for display using correct Gregorian calendar arithmetic
    pub fn timestamp_display(&self) -> String {
        use std::time::UNIX_EPOCH;
        let secs = match self.timestamp.duration_since(UNIX_EPOCH) {
            Ok(d) => d.as_secs(),
            Err(_) => return "unknown".to_string(),
        };

        // Time components
        let secs_today = secs % 86400;
        let hours = secs_today / 3600;
        let mins = (secs_today % 3600) / 60;
        let secs_part = secs_today % 60;

        // Gregorian date: peel off years using correct leap-year calculation
        let mut days = secs / 86400;
        let mut year = 1970u32;
        loop {
            let days_in_year = if is_leap_year(year) { 366 } else { 365 };
            if days < days_in_year {
                break;
            }
            days -= days_in_year;
            year += 1;
        }
        let month_days: &[u64] = if is_leap_year(year) {
            &[31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
        } else {
            &[31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
        };
        let mut month = 1u32;
        for &md in month_days {
            if days < md {
                break;
            }
            days -= md;
            month += 1;
        }
        let day = days + 1;

        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            year, month, day, hours, mins, secs_part
        )
    }
}

fn is_leap_year(y: u32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_history_entry_new() {
        let path = PathBuf::from("test.nika.yaml");
        let duration = Duration::from_millis(2700);
        let entry = HistoryEntry::new(path.clone(), duration, 3, true, "test summary".to_string());

        assert_eq!(entry.workflow_path, path);
        assert_eq!(entry.duration_ms, 2700);
        assert_eq!(entry.task_count, 3);
        assert!(entry.success);
        assert_eq!(entry.summary, "test summary");
    }

    #[test]
    fn test_history_entry_duration_display_seconds() {
        let entry = HistoryEntry::new(
            PathBuf::from("test.nika.yaml"),
            Duration::from_millis(2700),
            3,
            true,
            "test".to_string(),
        );
        assert_eq!(entry.duration_display(), "2.7s");
    }

    #[test]
    fn test_history_entry_duration_display_minutes() {
        let entry = HistoryEntry::new(
            PathBuf::from("test.nika.yaml"),
            Duration::from_secs(90),
            5,
            true,
            "test".to_string(),
        );
        assert_eq!(entry.duration_display(), "1.5m");
    }

    #[test]
    fn test_history_entry_duration_display_zero() {
        let entry = HistoryEntry::new(
            PathBuf::from("test.nika.yaml"),
            Duration::from_millis(0),
            1,
            true,
            "instant".to_string(),
        );
        assert_eq!(entry.duration_display(), "0.0s");
    }

    #[test]
    fn test_history_entry_duration_display_sub_second() {
        let entry = HistoryEntry::new(
            PathBuf::from("test.nika.yaml"),
            Duration::from_millis(150),
            1,
            true,
            "fast".to_string(),
        );
        assert_eq!(entry.duration_display(), "0.1s");
    }

    #[test]
    fn test_history_entry_duration_display_exactly_60_seconds() {
        let entry = HistoryEntry::new(
            PathBuf::from("test.nika.yaml"),
            Duration::from_secs(60),
            1,
            true,
            "minute".to_string(),
        );
        assert_eq!(entry.duration_display(), "1.0m");
    }

    #[test]
    fn test_history_entry_timestamp_display() {
        let entry = HistoryEntry::new(
            PathBuf::from("test.nika.yaml"),
            Duration::from_secs(1),
            1,
            true,
            "test".to_string(),
        );

        let timestamp_str = entry.timestamp_display();
        // Format: YYYY-MM-DD HH:MM:SS
        assert_eq!(timestamp_str.len(), 19); // "2026-02-20 15:30:45"
        assert_eq!(timestamp_str.chars().nth(4), Some('-'));
        assert_eq!(timestamp_str.chars().nth(7), Some('-'));
        assert_eq!(timestamp_str.chars().nth(10), Some(' '));
        assert_eq!(timestamp_str.chars().nth(13), Some(':'));
        assert_eq!(timestamp_str.chars().nth(16), Some(':'));
    }

    #[test]
    fn timestamp_never_produces_month_13() {
        // 2026-01-01 00:00:00 UTC = 1767225600 seconds
        let ts = std::time::UNIX_EPOCH + Duration::from_secs(1_767_225_600);
        let entry = HistoryEntry {
            workflow_path: PathBuf::from("test.nika.yaml"),
            timestamp: ts,
            duration_ms: 0,
            task_count: 1,
            success: true,
            summary: String::new(),
        };
        let s = entry.timestamp_display();
        assert!(s.starts_with("2026-01"), "expected 2026-01-xx, got: {s}");
        assert!(!s.contains("-13-"), "month 13 found in: {s}");
    }

    #[test]
    fn timestamp_leap_year_feb29() {
        // 2024-02-29 00:00:00 UTC = 1709164800
        let ts = std::time::UNIX_EPOCH + Duration::from_secs(1_709_164_800);
        let entry = HistoryEntry {
            workflow_path: PathBuf::from("test.nika.yaml"),
            timestamp: ts,
            duration_ms: 0,
            task_count: 1,
            success: true,
            summary: String::new(),
        };
        let s = entry.timestamp_display();
        assert!(s.starts_with("2024-02-29"), "expected 2024-02-29, got: {s}");
    }

    #[test]
    fn test_history_entry_serialization() {
        let path = PathBuf::from("test.nika.yaml");
        let entry = HistoryEntry::new(path, Duration::from_secs(5), 2, true, "summary".to_string());

        let json = serde_json::to_string(&entry).expect("serialization should succeed");
        let deserialized: HistoryEntry =
            serde_json::from_str(&json).expect("deserialization should succeed");

        assert_eq!(deserialized.workflow_path, entry.workflow_path);
        assert_eq!(deserialized.duration_ms, entry.duration_ms);
        assert_eq!(deserialized.task_count, entry.task_count);
        assert_eq!(deserialized.success, entry.success);
        assert_eq!(deserialized.summary, entry.summary);
    }

    #[test]
    fn test_history_entry_clone() {
        let path = PathBuf::from("test.nika.yaml");
        let entry1 =
            HistoryEntry::new(path, Duration::from_secs(5), 2, true, "summary".to_string());
        let entry2 = entry1.clone();

        assert_eq!(entry1.workflow_path, entry2.workflow_path);
        assert_eq!(entry1.duration_ms, entry2.duration_ms);
        assert_eq!(entry1.task_count, entry2.task_count);
        assert_eq!(entry1.success, entry2.success);
        assert_eq!(entry1.summary, entry2.summary);
    }
}
