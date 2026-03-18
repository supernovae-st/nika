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
}

/// Broad media type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaType {
    Image,
    Audio,
    Document,
    Unknown,
}

impl MediaType {
    /// Classify a MIME type string into a MediaType.
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
    pub fn check_and_add(
        &self,
        size: u64,
        _task_id: &str,
    ) -> Result<(), super::error::MediaError> {
        let new_total = self.run_bytes.fetch_add(size, Ordering::Relaxed) + size;
        if new_total > self.max_per_run {
            self.run_bytes.fetch_sub(size, Ordering::Relaxed);
            return Err(super::error::MediaError::RunBudgetExceeded {
                current: new_total,
                max: self.max_per_run,
            });
        }
        Ok(())
    }

    /// Get current accumulated bytes.
    pub fn current_bytes(&self) -> u64 {
        self.run_bytes.load(Ordering::Relaxed)
    }
}

impl Default for MediaBudget {
    fn default() -> Self {
        Self::new()
    }
}
