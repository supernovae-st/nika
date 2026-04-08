// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Media pipeline types: MediaRef, MediaType, MediaBudget

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

/// Reference to a media file stored in the CAS.
///
/// Serializes to JSON for use in `{{with.task_id.media[0].hash}}` templates.
/// Hash stores the algorithm-prefixed hash: `"blake3:af1349..."`.
/// CAS filenames are hash-only (no extension). Extension is stored only here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MediaRef {
    /// Algorithm-prefixed hash (e.g., "blake3:af1349b9...")
    pub hash: String,

    /// Detected MIME type (e.g., "image/png", "audio/wav")
    pub mime_type: String,

    /// File size in bytes (decoded, not base64)
    pub size_bytes: u64,

    /// Absolute path to the stored file
    pub path: PathBuf,

    /// File extension without dot (e.g., "png", "wav")
    pub extension: String,

    /// Task ID that produced this media
    pub created_by: String,

    /// Auto-enriched metadata (dimensions, thumbhash, etc.)
    ///
    /// Template access: `{{with.task.media[0].metadata.width}}`
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub metadata: serde_json::Map<String, serde_json::Value>,
}

/// Broad media type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)] // Used in tests
pub enum MediaType {
    Image,
    Audio,
    Document,
    Unknown,
}

impl MediaType {
    /// Classify a MIME type string into a MediaType.
    #[allow(dead_code)] // Used in tests
    pub fn from_mime(mime: &str) -> Self {
        if mime.starts_with("image/") {
            Self::Image
        } else if mime.starts_with("audio/") {
            Self::Audio
        } else if mime.starts_with("application/pdf")
            || mime.starts_with("application/vnd.openxmlformats")
            || mime.starts_with("application/msword")
        {
            Self::Document
        } else {
            Self::Unknown
        }
    }
}

/// Media budget enforcement for memory safety.
///
/// Tracks cumulative bytes processed per run to prevent unbounded media
/// accumulation (e.g., from `for_each` with many media-producing iterations).
/// Uses `AtomicU64` for lock-free concurrent access from parallel tasks.
pub struct MediaBudget {
    run_bytes: AtomicU64,
    max_per_run: u64,
}

impl MediaBudget {
    /// Default per-run budget: 500MB.
    pub const DEFAULT_MAX_PER_RUN: u64 = 500 * 1024 * 1024;

    /// Create a new budget with default limits.
    pub fn new() -> Self {
        Self {
            run_bytes: AtomicU64::new(0),
            max_per_run: Self::DEFAULT_MAX_PER_RUN,
        }
    }

    /// Create a new budget with custom per-run limit.
    pub fn with_max_per_run(max_per_run: u64) -> Self {
        Self {
            run_bytes: AtomicU64::new(0),
            max_per_run,
        }
    }

    /// Check budget and add bytes atomically. Returns error if budget exceeded.
    ///
    /// Uses `fetch_update` (CAS loop) so the increment and comparison
    /// are one atomic unit — prevents concurrent overruns.
    pub fn check_and_add(&self, size: u64, _task_id: &str) -> Result<(), super::error::MediaError> {
        let result = self
            .run_bytes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                let new_total = current + size;
                if new_total > self.max_per_run {
                    None // reject: over budget
                } else {
                    Some(new_total) // accept: update counter
                }
            });
        match result {
            Ok(_) => Ok(()),
            Err(current) => Err(super::error::MediaError::RunBudgetExceeded {
                current: current + size,
                max: self.max_per_run,
            }),
        }
    }

    /// Roll back a budget allocation (e.g., when CAS store fails after budget was charged).
    ///
    /// Uses `fetch_update` with `saturating_sub` to prevent underflow.
    /// Without this, a rollback larger than the current value would wrap to `u64::MAX`,
    /// effectively disabling budget enforcement for the rest of the run.
    pub fn rollback(&self, size: u64) {
        let _ = self
            .run_bytes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                Some(current.saturating_sub(size))
            });
    }

    /// Get current accumulated bytes.
    pub fn current_bytes(&self) -> u64 {
        self.run_bytes.load(Ordering::Acquire)
    }
}

impl Default for MediaBudget {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_budget_under_limit() {
        let budget = MediaBudget::with_max_per_run(1024);
        assert!(budget.check_and_add(512, "t1").is_ok());
        assert_eq!(budget.current_bytes(), 512);
    }

    #[test]
    fn media_budget_exceeds_limit() {
        let budget = MediaBudget::with_max_per_run(1024);
        budget.check_and_add(512, "t1").unwrap();
        let err = budget.check_and_add(1024, "t2").unwrap_err();
        assert_eq!(err.code(), "NIKA-259");
        // Budget should NOT have increased
        assert_eq!(budget.current_bytes(), 512);
    }

    #[test]
    fn media_budget_accumulated_tracking() {
        let budget = MediaBudget::with_max_per_run(1000);
        budget.check_and_add(300, "t1").unwrap();
        budget.check_and_add(300, "t2").unwrap();
        budget.check_and_add(300, "t3").unwrap();
        assert_eq!(budget.current_bytes(), 900);
        // This should fail (900 + 200 = 1100 > 1000)
        assert!(budget.check_and_add(200, "t4").is_err());
    }

    #[test]
    fn media_ref_serde_roundtrip() {
        let mr = MediaRef {
            hash: "blake3:af1349b9".to_string(),
            mime_type: "image/png".to_string(),
            size_bytes: 4096,
            path: PathBuf::from("/tmp/store/af/1349b9"),
            extension: "png".to_string(),
            created_by: "gen_img".to_string(),
            metadata: serde_json::Map::new(),
        };
        let json = serde_json::to_string(&mr).unwrap();
        let back: MediaRef = serde_json::from_str(&json).unwrap();
        assert_eq!(mr, back);
        assert!(json.contains("blake3:af1349b9"));
    }

    #[test]
    fn media_type_from_mime() {
        assert_eq!(MediaType::from_mime("image/png"), MediaType::Image);
        assert_eq!(MediaType::from_mime("image/jpeg"), MediaType::Image);
        assert_eq!(MediaType::from_mime("audio/wav"), MediaType::Audio);
        assert_eq!(MediaType::from_mime("audio/mpeg"), MediaType::Audio);
        assert_eq!(MediaType::from_mime("application/pdf"), MediaType::Document);
        assert_eq!(MediaType::from_mime("text/plain"), MediaType::Unknown);
    }

    // ═══════════════════════════════════════════════════════════════
    // GAP 1/2 support: MediaBudget::rollback unit test
    // Verifies that rollback correctly decrements the counter
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn media_budget_rollback_restores_capacity() {
        let budget = MediaBudget::with_max_per_run(1000);

        // Charge 600 bytes
        budget.check_and_add(600, "t1").unwrap();
        assert_eq!(budget.current_bytes(), 600);

        // Roll back 600 bytes (simulates failure after budget charge)
        budget.rollback(600);
        assert_eq!(
            budget.current_bytes(),
            0,
            "rollback should restore budget to 0"
        );

        // After rollback, the full budget should be available again
        budget.check_and_add(900, "t2").unwrap();
        assert_eq!(budget.current_bytes(), 900);
    }

    #[test]
    fn media_budget_partial_rollback() {
        let budget = MediaBudget::with_max_per_run(1000);

        budget.check_and_add(300, "t1").unwrap();
        budget.check_and_add(400, "t2").unwrap();
        assert_eq!(budget.current_bytes(), 700);

        // Roll back only t2's allocation
        budget.rollback(400);
        assert_eq!(
            budget.current_bytes(),
            300,
            "partial rollback should leave t1's allocation intact"
        );

        // Now we have 700 bytes of capacity remaining
        budget.check_and_add(700, "t3").unwrap();
        assert_eq!(budget.current_bytes(), 1000);
    }

    #[test]
    fn media_budget_rollback_then_reuse() {
        let budget = MediaBudget::with_max_per_run(100);

        // Fill to capacity
        budget.check_and_add(100, "t1").unwrap();
        assert!(
            budget.check_and_add(1, "t2").is_err(),
            "should be at capacity"
        );

        // Roll back and verify capacity is restored
        budget.rollback(100);
        assert_eq!(budget.current_bytes(), 0);

        // Now the full budget is available
        budget.check_and_add(100, "t3").unwrap();
        assert_eq!(budget.current_bytes(), 100);
    }
}
