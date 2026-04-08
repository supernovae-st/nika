// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Course progress — TOML-based persistence for course state
//!
//! Tracks which exercises are completed, hints used, and level status.
//! Stored in `.nika/course-progress.toml`.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{NikaInitError, Result};

/// Top-level progress file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CourseProgress {
    pub metadata: CourseMetadata,
    /// Level number (as string key) -> LevelProgress
    pub levels: HashMap<String, LevelProgress>,
}

/// Course-wide metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CourseMetadata {
    /// Schema version for forward compatibility
    pub version: u8,
    /// ISO 8601 timestamp of first init
    pub started_at: String,
    /// ISO 8601 timestamp of last activity
    pub last_activity: String,
    /// Total hints used across all levels
    pub total_hints_used: u32,
}

/// Progress for a single level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelProgress {
    pub status: LevelStatus,
    /// Exercise number (as string key) -> ExerciseStatus
    pub exercises: HashMap<String, ExerciseStatus>,
    /// Number of hints used in this level
    pub hints_used: u32,
}

/// Level completion status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LevelStatus {
    Locked,
    Unlocked,
    InProgress,
    Completed,
}

/// Exercise completion status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExerciseStatus {
    NotStarted,
    Attempted,
    Passed,
    Perfect,
}

impl CourseProgress {
    /// Create a fresh course progress with level 1 unlocked
    pub fn new_course() -> Self {
        let now = now_iso8601();
        let mut levels = HashMap::new();

        // Level 1 starts unlocked, everything else locked
        for level in super::levels::LEVELS {
            let status = if level.number == 1 {
                LevelStatus::Unlocked
            } else {
                LevelStatus::Locked
            };
            let mut exercises = HashMap::new();
            for ex in 1..=level.exercise_count {
                exercises.insert(ex.to_string(), ExerciseStatus::NotStarted);
            }
            levels.insert(
                level.number.to_string(),
                LevelProgress {
                    status,
                    exercises,
                    hints_used: 0,
                },
            );
        }

        Self {
            metadata: CourseMetadata {
                version: 1,
                started_at: now.clone(),
                last_activity: now,
                total_hints_used: 0,
            },
            levels,
        }
    }

    /// Load progress from a TOML file
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).map_err(|e| NikaInitError::ConfigError {
            reason: format!(
                "Failed to read course progress at {}: {}",
                path.display(),
                e
            ),
        })?;
        toml::from_str(&content).map_err(|e| NikaInitError::ConfigError {
            reason: format!("Failed to parse course progress: {}", e),
        })
    }

    /// Save progress to a TOML file
    pub fn save(&mut self, path: &Path) -> Result<()> {
        self.metadata.last_activity = now_iso8601();
        let content = toml::to_string_pretty(self).map_err(|e| NikaInitError::ConfigError {
            reason: format!("Failed to serialize course progress: {}", e),
        })?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| NikaInitError::ConfigError {
                reason: format!("Failed to create directory {}: {}", parent.display(), e),
            })?;
        }
        std::fs::write(path, content).map_err(|e| NikaInitError::ConfigError {
            reason: format!(
                "Failed to write course progress to {}: {}",
                path.display(),
                e
            ),
        })?;
        Ok(())
    }

    /// Mark an exercise as passed; auto-advance level status
    pub fn mark_exercise_passed(&mut self, level: u8, exercise: u8) {
        let level_key = level.to_string();
        let ex_key = exercise.to_string();

        if let Some(lp) = self.levels.get_mut(&level_key) {
            // Only upgrade: NotStarted/Attempted -> Passed (never downgrade Perfect)
            if let Some(es) = lp.exercises.get_mut(&ex_key) {
                if *es != ExerciseStatus::Perfect {
                    *es = ExerciseStatus::Passed;
                }
            }

            // Update level status
            if lp.status == LevelStatus::Locked || lp.status == LevelStatus::Unlocked {
                lp.status = LevelStatus::InProgress;
            }

            // Check if all exercises are Passed or Perfect
            let all_done = lp
                .exercises
                .values()
                .all(|s| *s == ExerciseStatus::Passed || *s == ExerciseStatus::Perfect);
            if all_done {
                lp.status = LevelStatus::Completed;
                // Unlock next level
                let next_key = (level + 1).to_string();
                if let Some(next) = self.levels.get_mut(&next_key) {
                    if next.status == LevelStatus::Locked {
                        next.status = LevelStatus::Unlocked;
                    }
                }
            }
        }
    }

    /// Record a hint usage for a level
    pub fn record_hint(&mut self, level: u8) {
        let level_key = level.to_string();
        if let Some(lp) = self.levels.get_mut(&level_key) {
            lp.hints_used += 1;
        }
        self.metadata.total_hints_used += 1;
    }

    /// Reset a level to Unlocked with all exercises NotStarted
    pub fn reset_level(&mut self, level: u8) {
        let level_key = level.to_string();
        if let Some(lp) = self.levels.get_mut(&level_key) {
            lp.status = LevelStatus::Unlocked;
            lp.hints_used = 0;
            for es in lp.exercises.values_mut() {
                *es = ExerciseStatus::NotStarted;
            }
        }
    }

    /// Count completed exercises across all levels
    pub fn completed_exercises(&self) -> usize {
        self.levels
            .values()
            .flat_map(|lp| lp.exercises.values())
            .filter(|s| **s == ExerciseStatus::Passed || **s == ExerciseStatus::Perfect)
            .count()
    }

    /// Count completed levels
    pub fn completed_levels(&self) -> usize {
        self.levels
            .values()
            .filter(|lp| lp.status == LevelStatus::Completed)
            .count()
    }
}

/// ISO 8601 timestamp (UTC) — no chrono dependency
fn now_iso8601() -> String {
    // Use std::time for a simple epoch-based fallback
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();
    // Format as YYYY-MM-DDTHH:MM:SSZ using basic arithmetic
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    // Compute year/month/day from days since epoch (1970-01-01)
    let (year, month, day) = days_to_ymd(days);

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

/// Convert days since Unix epoch to (year, month, day)
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Civil calendar algorithm (Howard Hinnant)
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_course_has_12_levels() {
        let progress = CourseProgress::new_course();
        assert_eq!(progress.levels.len(), 12);
    }

    #[test]
    fn test_new_course_level1_unlocked() {
        let progress = CourseProgress::new_course();
        let l1 = &progress.levels["1"];
        assert_eq!(l1.status, LevelStatus::Unlocked);
    }

    #[test]
    fn test_new_course_level2_locked() {
        let progress = CourseProgress::new_course();
        let l2 = &progress.levels["2"];
        assert_eq!(l2.status, LevelStatus::Locked);
    }

    #[test]
    fn test_new_course_exercise_count() {
        let progress = CourseProgress::new_course();
        // Level 1 has 5 exercises
        assert_eq!(progress.levels["1"].exercises.len(), 5);
        // Level 12 has 5 exercises
        assert_eq!(progress.levels["12"].exercises.len(), 5);
        // Level 4 has 3 exercises
        assert_eq!(progress.levels["4"].exercises.len(), 3);
    }

    #[test]
    fn test_mark_exercise_passed() {
        let mut progress = CourseProgress::new_course();
        progress.mark_exercise_passed(1, 1);
        assert_eq!(progress.levels["1"].exercises["1"], ExerciseStatus::Passed);
        assert_eq!(progress.levels["1"].status, LevelStatus::InProgress);
    }

    #[test]
    fn test_complete_level_unlocks_next() {
        let mut progress = CourseProgress::new_course();
        // Complete all 5 exercises in level 1
        for ex in 1..=5 {
            progress.mark_exercise_passed(1, ex);
        }
        assert_eq!(progress.levels["1"].status, LevelStatus::Completed);
        assert_eq!(progress.levels["2"].status, LevelStatus::Unlocked);
    }

    #[test]
    fn test_record_hint() {
        let mut progress = CourseProgress::new_course();
        progress.record_hint(1);
        progress.record_hint(1);
        assert_eq!(progress.levels["1"].hints_used, 2);
        assert_eq!(progress.metadata.total_hints_used, 2);
    }

    #[test]
    fn test_reset_level() {
        let mut progress = CourseProgress::new_course();
        progress.mark_exercise_passed(1, 1);
        progress.record_hint(1);
        progress.reset_level(1);
        assert_eq!(progress.levels["1"].status, LevelStatus::Unlocked);
        assert_eq!(progress.levels["1"].hints_used, 0);
        assert_eq!(
            progress.levels["1"].exercises["1"],
            ExerciseStatus::NotStarted
        );
    }

    #[test]
    fn test_completed_exercises() {
        let mut progress = CourseProgress::new_course();
        assert_eq!(progress.completed_exercises(), 0);
        progress.mark_exercise_passed(1, 1);
        progress.mark_exercise_passed(1, 2);
        assert_eq!(progress.completed_exercises(), 2);
    }

    #[test]
    fn test_completed_levels() {
        let mut progress = CourseProgress::new_course();
        assert_eq!(progress.completed_levels(), 0);
        for ex in 1..=5 {
            progress.mark_exercise_passed(1, ex);
        }
        assert_eq!(progress.completed_levels(), 1);
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let mut progress = CourseProgress::new_course();
        progress.mark_exercise_passed(1, 1);
        progress.record_hint(1);

        let dir = std::env::temp_dir().join("nika-course-test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("progress.toml");

        progress.save(&path).unwrap();
        let loaded = CourseProgress::load(&path).unwrap();

        assert_eq!(loaded.levels["1"].exercises["1"], ExerciseStatus::Passed);
        assert_eq!(loaded.levels["1"].hints_used, 1);
        assert_eq!(loaded.metadata.total_hints_used, 1);

        // Cleanup
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_load_nonexistent_file() {
        let result = CourseProgress::load(Path::new("/tmp/nonexistent-nika-course.toml"));
        assert!(result.is_err());
    }

    #[test]
    fn test_metadata_version() {
        let progress = CourseProgress::new_course();
        assert_eq!(progress.metadata.version, 1);
    }

    #[test]
    fn test_iso8601_format() {
        let ts = now_iso8601();
        // Should match YYYY-MM-DDTHH:MM:SSZ
        assert_eq!(ts.len(), 20);
        assert!(ts.ends_with('Z'));
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[7..8], "-");
        assert_eq!(&ts[10..11], "T");
        assert_eq!(&ts[13..14], ":");
        assert_eq!(&ts[16..17], ":");
    }

    #[test]
    fn test_perfect_not_downgraded() {
        let mut progress = CourseProgress::new_course();
        // Manually set to Perfect
        progress
            .levels
            .get_mut("1")
            .unwrap()
            .exercises
            .insert("1".to_string(), ExerciseStatus::Perfect);
        // mark_exercise_passed should not downgrade Perfect -> Passed
        progress.mark_exercise_passed(1, 1);
        assert_eq!(progress.levels["1"].exercises["1"], ExerciseStatus::Perfect);
    }
}
