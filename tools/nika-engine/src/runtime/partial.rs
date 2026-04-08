// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Partial completion support for agent execution
//!
//! When an agent hits a limit (turns, tokens, cost, duration), this module
//! allows capturing and saving the partial progress for potential resumption.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use nika::runtime::partial::{PartialResult, PartialCheckpoint};
//!
//! // Create checkpoint when limit is reached
//! let checkpoint = PartialCheckpoint::new(
//!     "task-1",
//!     0.65,  // 65% complete
//!     "Generated 3 of 5 sections",
//! );
//!
//! // Save progress
//! checkpoint.save_to_file("./checkpoints/task-1.json").await?;
//!
//! // Load and resume later
//! let restored = PartialCheckpoint::load_from_file("./checkpoints/task-1.json").await?;
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::NikaError;

// ═══════════════════════════════════════════════════════════════════════════
// PARTIAL RESULT
// ═══════════════════════════════════════════════════════════════════════════

/// Result of a partially completed task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialResult {
    /// Current output content (may be incomplete)
    pub content: String,

    /// Progress percentage (0.0-1.0)
    pub progress: f64,

    /// Preview of the result for display
    pub preview: String,

    /// Reason why execution stopped
    pub stop_reason: StopReason,

    /// Number of turns completed
    pub turns_completed: u32,

    /// Total tokens used
    pub tokens_used: u64,

    /// Cost incurred (USD)
    pub cost_usd: f64,
}

impl PartialResult {
    /// Create a new partial result
    pub fn new(content: impl Into<String>, progress: f64, stop_reason: StopReason) -> Self {
        let content = content.into();
        let preview = Self::generate_preview(&content);

        Self {
            content,
            progress: progress.clamp(0.0, 1.0),
            preview,
            stop_reason,
            turns_completed: 0,
            tokens_used: 0,
            cost_usd: 0.0,
        }
    }

    /// Set usage statistics
    pub fn with_usage(mut self, turns: u32, tokens: u64, cost: f64) -> Self {
        self.turns_completed = turns;
        self.tokens_used = tokens;
        self.cost_usd = cost;
        self
    }

    /// Generate a preview from content (first 100 chars or first line)
    fn generate_preview(content: &str) -> String {
        let first_line = content.lines().next().unwrap_or("");
        if first_line.len() > 100 {
            format!("{}...", crate::util::truncate_str(first_line, 97))
        } else if first_line.len() < content.len() {
            format!("{}...", first_line)
        } else {
            first_line.to_string()
        }
    }

    /// Check if this is a meaningful partial result
    pub fn is_meaningful(&self) -> bool {
        !self.content.is_empty() && self.progress > 0.0
    }

    /// Calculate progress percentage as integer (0-100)
    pub fn progress_percent(&self) -> u32 {
        (self.progress * 100.0).round() as u32
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// STOP REASON
// ═══════════════════════════════════════════════════════════════════════════

/// Reason for partial completion
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopReason {
    /// Hit maximum turns limit
    TurnsLimit,
    /// Hit token budget limit
    TokensLimit,
    /// Hit cost budget limit
    CostLimit,
    /// Hit duration limit
    DurationLimit,
    /// User requested stop
    UserRequested,
    /// Error occurred
    Error,
}

impl StopReason {
    /// Get human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            Self::TurnsLimit => "Maximum turns reached",
            Self::TokensLimit => "Token budget exhausted",
            Self::CostLimit => "Cost budget exhausted",
            Self::DurationLimit => "Duration limit reached",
            Self::UserRequested => "User requested stop",
            Self::Error => "Error occurred",
        }
    }

    /// Get short label for display
    pub fn label(&self) -> &'static str {
        match self {
            Self::TurnsLimit => "turns",
            Self::TokensLimit => "tokens",
            Self::CostLimit => "cost",
            Self::DurationLimit => "duration",
            Self::UserRequested => "user",
            Self::Error => "error",
        }
    }

    /// Check if this is a limit-based stop
    pub fn is_limit(&self) -> bool {
        matches!(
            self,
            Self::TurnsLimit | Self::TokensLimit | Self::CostLimit | Self::DurationLimit
        )
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PARTIAL CHECKPOINT
// ═══════════════════════════════════════════════════════════════════════════

/// Checkpoint for resuming partial execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialCheckpoint {
    /// Version for compatibility checking
    pub version: u32,

    /// Task identifier
    pub task_id: Arc<str>,

    /// Timestamp when checkpoint was created (Unix epoch seconds)
    pub created_at: u64,

    /// Current progress (0.0-1.0)
    pub progress: f64,

    /// The partial result
    pub result: PartialResult,

    /// Conversation history for resumption
    pub history: Vec<CheckpointMessage>,

    /// Context data for resumption
    pub context: HashMap<String, serde_json::Value>,

    /// Provider that was being used
    pub provider: Option<String>,

    /// Model that was being used
    pub model: Option<String>,
}

/// Message in checkpoint history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointMessage {
    /// Role: "user", "assistant", or "system"
    pub role: String,
    /// Message content
    pub content: String,
}

impl PartialCheckpoint {
    /// Current checkpoint format version
    pub const CURRENT_VERSION: u32 = 1;

    /// Create a new checkpoint
    pub fn new(
        task_id: impl Into<Arc<str>>,
        progress: f64,
        content: impl Into<String>,
        stop_reason: StopReason,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Self {
            version: Self::CURRENT_VERSION,
            task_id: task_id.into(),
            created_at: now,
            progress: progress.clamp(0.0, 1.0),
            result: PartialResult::new(content, progress, stop_reason),
            history: Vec::new(),
            context: HashMap::new(),
            provider: None,
            model: None,
        }
    }

    /// Add a message to history
    pub fn add_message(&mut self, role: impl Into<String>, content: impl Into<String>) {
        self.history.push(CheckpointMessage {
            role: role.into(),
            content: content.into(),
        });
    }

    /// Set context data
    pub fn set_context(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.context.insert(key.into(), value);
    }

    /// Set provider info
    pub fn with_provider(mut self, provider: impl Into<String>, model: impl Into<String>) -> Self {
        self.provider = Some(provider.into());
        self.model = Some(model.into());
        self
    }

    /// Set usage statistics
    pub fn with_usage(mut self, turns: u32, tokens: u64, cost: f64) -> Self {
        self.result = self.result.with_usage(turns, tokens, cost);
        self
    }

    /// Save checkpoint to file
    pub async fn save_to_file(&self, path: impl AsRef<Path>) -> Result<(), NikaError> {
        let path = path.as_ref();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(NikaError::IoError)?;
        }

        // Serialize to JSON
        let json =
            serde_json::to_string_pretty(self).map_err(|e| NikaError::SerializationError {
                details: format!("Failed to serialize checkpoint: {}", e),
            })?;

        // Write atomically (write to temp, then rename)
        let temp_path = path.with_extension("tmp");
        tokio::fs::write(&temp_path, &json)
            .await
            .map_err(NikaError::IoError)?;
        tokio::fs::rename(&temp_path, path)
            .await
            .map_err(NikaError::IoError)?;

        Ok(())
    }

    /// Load checkpoint from file
    pub async fn load_from_file(path: impl AsRef<Path>) -> Result<Self, NikaError> {
        let path = path.as_ref();

        let json = tokio::fs::read_to_string(path)
            .await
            .map_err(NikaError::IoError)?;

        let checkpoint: Self =
            serde_json::from_str(&json).map_err(|e| NikaError::SerializationError {
                details: format!("Failed to parse checkpoint: {}", e),
            })?;

        // Check version compatibility
        if checkpoint.version > Self::CURRENT_VERSION {
            return Err(NikaError::ValidationError {
                reason: format!(
                    "Checkpoint version {} is newer than supported version {}",
                    checkpoint.version,
                    Self::CURRENT_VERSION
                ),
            });
        }

        Ok(checkpoint)
    }

    /// Check if checkpoint exists at path
    pub async fn exists(path: impl AsRef<Path>) -> bool {
        tokio::fs::metadata(path.as_ref()).await.is_ok()
    }

    /// Generate checkpoint filename for a task
    pub fn filename(task_id: &str) -> String {
        // Sanitize task_id for use as filename
        let safe_id: String = task_id
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        format!("{}.checkpoint.json", safe_id)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ───────────────────────────────────────────────────────────────────────
    // PartialResult tests
    // ───────────────────────────────────────────────────────────────────────

    #[test]
    fn partial_result_new() {
        let result = PartialResult::new("Hello world", 0.5, StopReason::TurnsLimit);
        assert_eq!(result.content, "Hello world");
        assert!((result.progress - 0.5).abs() < 0.0001);
        assert_eq!(result.stop_reason, StopReason::TurnsLimit);
    }

    #[test]
    fn partial_result_clamps_progress() {
        let result = PartialResult::new("test", 1.5, StopReason::CostLimit);
        assert!((result.progress - 1.0).abs() < 0.0001);

        let result = PartialResult::new("test", -0.5, StopReason::CostLimit);
        assert!((result.progress - 0.0).abs() < 0.0001);
    }

    #[test]
    fn partial_result_with_usage() {
        let result =
            PartialResult::new("test", 0.75, StopReason::TokensLimit).with_usage(10, 5000, 0.05);

        assert_eq!(result.turns_completed, 10);
        assert_eq!(result.tokens_used, 5000);
        assert!((result.cost_usd - 0.05).abs() < 0.0001);
    }

    #[test]
    fn partial_result_preview_short() {
        let result = PartialResult::new("Short content", 0.5, StopReason::TurnsLimit);
        assert_eq!(result.preview, "Short content");
    }

    #[test]
    fn partial_result_preview_multiline() {
        let result = PartialResult::new(
            "First line\nSecond line\nThird",
            0.5,
            StopReason::TurnsLimit,
        );
        assert_eq!(result.preview, "First line...");
    }

    #[test]
    fn partial_result_preview_long_line() {
        let long_content = "x".repeat(150);
        let result = PartialResult::new(&long_content, 0.5, StopReason::TurnsLimit);
        assert!(result.preview.ends_with("..."));
        assert!(result.preview.len() <= 103); // 97 + "..."
    }

    #[test]
    fn partial_result_is_meaningful() {
        let meaningful = PartialResult::new("content", 0.5, StopReason::TurnsLimit);
        assert!(meaningful.is_meaningful());

        let empty = PartialResult::new("", 0.5, StopReason::TurnsLimit);
        assert!(!empty.is_meaningful());

        let zero_progress = PartialResult::new("content", 0.0, StopReason::TurnsLimit);
        assert!(!zero_progress.is_meaningful());
    }

    #[test]
    fn partial_result_progress_percent() {
        let result = PartialResult::new("test", 0.654, StopReason::TurnsLimit);
        assert_eq!(result.progress_percent(), 65);
    }

    // ───────────────────────────────────────────────────────────────────────
    // StopReason tests
    // ───────────────────────────────────────────────────────────────────────

    #[test]
    fn stop_reason_descriptions() {
        assert!(!StopReason::TurnsLimit.description().is_empty());
        assert!(!StopReason::TokensLimit.description().is_empty());
        assert!(!StopReason::CostLimit.description().is_empty());
        assert!(!StopReason::DurationLimit.description().is_empty());
        assert!(!StopReason::UserRequested.description().is_empty());
        assert!(!StopReason::Error.description().is_empty());
    }

    #[test]
    fn stop_reason_labels() {
        assert_eq!(StopReason::TurnsLimit.label(), "turns");
        assert_eq!(StopReason::TokensLimit.label(), "tokens");
        assert_eq!(StopReason::CostLimit.label(), "cost");
        assert_eq!(StopReason::DurationLimit.label(), "duration");
    }

    #[test]
    fn stop_reason_is_limit() {
        assert!(StopReason::TurnsLimit.is_limit());
        assert!(StopReason::TokensLimit.is_limit());
        assert!(StopReason::CostLimit.is_limit());
        assert!(StopReason::DurationLimit.is_limit());
        assert!(!StopReason::UserRequested.is_limit());
        assert!(!StopReason::Error.is_limit());
    }

    // ───────────────────────────────────────────────────────────────────────
    // PartialCheckpoint tests
    // ───────────────────────────────────────────────────────────────────────

    #[test]
    fn checkpoint_new() {
        let checkpoint = PartialCheckpoint::new(
            "task-1",
            0.75,
            "Partial content here",
            StopReason::TurnsLimit,
        );

        assert_eq!(checkpoint.version, PartialCheckpoint::CURRENT_VERSION);
        assert_eq!(&*checkpoint.task_id, "task-1");
        assert!((checkpoint.progress - 0.75).abs() < 0.0001);
        assert!(checkpoint.created_at > 0);
    }

    #[test]
    fn checkpoint_add_message() {
        let mut checkpoint = PartialCheckpoint::new("task-1", 0.5, "test", StopReason::TurnsLimit);
        checkpoint.add_message("user", "Hello");
        checkpoint.add_message("assistant", "Hi there");

        assert_eq!(checkpoint.history.len(), 2);
        assert_eq!(checkpoint.history[0].role, "user");
        assert_eq!(checkpoint.history[1].content, "Hi there");
    }

    #[test]
    fn checkpoint_set_context() {
        let mut checkpoint = PartialCheckpoint::new("task-1", 0.5, "test", StopReason::TurnsLimit);
        checkpoint.set_context("key1", serde_json::json!({"nested": "value"}));

        assert!(checkpoint.context.contains_key("key1"));
    }

    #[test]
    fn checkpoint_with_provider() {
        let checkpoint = PartialCheckpoint::new("task-1", 0.5, "test", StopReason::TurnsLimit)
            .with_provider("claude", "claude-sonnet-4-6");

        assert_eq!(checkpoint.provider, Some("claude".to_string()));
        assert_eq!(checkpoint.model, Some("claude-sonnet-4-6".to_string()));
    }

    #[test]
    fn checkpoint_with_usage() {
        let checkpoint = PartialCheckpoint::new("task-1", 0.5, "test", StopReason::TurnsLimit)
            .with_usage(5, 10000, 0.15);

        assert_eq!(checkpoint.result.turns_completed, 5);
        assert_eq!(checkpoint.result.tokens_used, 10000);
        assert!((checkpoint.result.cost_usd - 0.15).abs() < 0.0001);
    }

    #[test]
    fn checkpoint_filename() {
        assert_eq!(
            PartialCheckpoint::filename("task-1"),
            "task-1.checkpoint.json"
        );
        assert_eq!(
            PartialCheckpoint::filename("task/with/slashes"),
            "task_with_slashes.checkpoint.json"
        );
        assert_eq!(
            PartialCheckpoint::filename("task with spaces"),
            "task_with_spaces.checkpoint.json"
        );
    }

    #[test]
    fn checkpoint_serialization_roundtrip() {
        let mut checkpoint =
            PartialCheckpoint::new("task-1", 0.75, "Content", StopReason::CostLimit)
                .with_provider("openai", "gpt-4o")
                .with_usage(3, 5000, 0.05);

        checkpoint.add_message("user", "Question");
        checkpoint.add_message("assistant", "Answer");
        checkpoint.set_context("data", serde_json::json!({"key": "value"}));

        let json = serde_json::to_string(&checkpoint).unwrap();
        let restored: PartialCheckpoint = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.task_id, checkpoint.task_id);
        assert_eq!(restored.progress, checkpoint.progress);
        assert_eq!(restored.history.len(), 2);
        assert!(restored.context.contains_key("data"));
    }

    // ───────────────────────────────────────────────────────────────────────
    // Async file tests (require tokio runtime)
    // ───────────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn checkpoint_save_and_load() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.checkpoint.json");

        let checkpoint =
            PartialCheckpoint::new("test-task", 0.8, "Test content", StopReason::TokensLimit)
                .with_usage(5, 8000, 0.12);

        // Save
        checkpoint.save_to_file(&file_path).await.unwrap();
        assert!(file_path.exists());

        // Load
        let loaded = PartialCheckpoint::load_from_file(&file_path).await.unwrap();
        assert_eq!(&*loaded.task_id, "test-task");
        assert!((loaded.progress - 0.8).abs() < 0.0001);
        assert_eq!(loaded.result.tokens_used, 8000);
    }

    #[tokio::test]
    async fn checkpoint_exists() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("exists.json");

        assert!(!PartialCheckpoint::exists(&file_path).await);

        tokio::fs::write(&file_path, "{}").await.unwrap();
        assert!(PartialCheckpoint::exists(&file_path).await);
    }

    #[tokio::test]
    async fn checkpoint_load_nonexistent() {
        let result = PartialCheckpoint::load_from_file("/nonexistent/path.json").await;
        assert!(result.is_err());
    }
}
