// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Memory traits + types — Cortex kernel hooks.
//!
//! ISP decomposition: `MemoryRemember`, `MemoryRecall`, `MemoryForget`.
//! Super-trait: `MemoryStore` (blanket for all 3).
//! Separate: `EmbeddingProvider` (embedding generation).
//!
//! These hooks land Phase 1 to avoid breaking-change cascades on
//! `#[non_exhaustive]` structs (ROI 6.7x per `POST_AUDIT` decision 3).
//! Business logic lives in `nika-memory` (Phase 9+).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

// ─── Types ───────────────────────────────────────────────────────────

/// Opaque memory identifier. Displayed as `"mem-{hex}"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MemoryId(u128);

impl MemoryId {
    /// Create a new memory identifier.
    #[must_use]
    pub fn new(id: u128) -> Self {
        Self(id)
    }

    /// Create a nil (zero) memory identifier.
    #[must_use]
    pub fn nil() -> Self {
        Self(0)
    }

    /// Get the raw u128 value.
    #[must_use]
    pub fn as_u128(&self) -> u128 {
        self.0
    }
}

impl std::fmt::Display for MemoryId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "mem-{:032x}", self.0)
    }
}

impl Serialize for MemoryId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for MemoryId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        let hex = s
            .strip_prefix("mem-")
            .ok_or_else(|| serde::de::Error::custom("expected 'mem-' prefix"))?;
        let id = u128::from_str_radix(hex, 16)
            .map_err(|e| serde::de::Error::custom(format!("invalid hex: {e}")))?;
        Ok(Self(id))
    }
}

/// Memory level in the cognitive hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum MemoryLevel {
    /// Short-term working memory (current task context).
    Working,
    /// Episode-specific memories (what happened).
    Episodic,
    /// General knowledge (facts, concepts).
    Semantic,
    /// How-to knowledge (procedures, recipes).
    Procedural,
    /// Meta-cognitive observations (patterns, self-reflection).
    Reflective,
    /// Abstract concept relationships.
    Conceptual,
}

/// A memory frame to store.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct MemoryFrame {
    /// The content to remember.
    pub content: String,
    /// Cognitive level.
    pub level: MemoryLevel,
    /// Searchable tags.
    pub tags: Vec<String>,
    /// Arbitrary key-value metadata.
    pub metadata: BTreeMap<String, String>,
    /// Origin source (e.g., workflow name, task ID).
    pub source: Option<String>,
    /// When the observation was made (Unix epoch millis).
    pub observed_at: Option<u64>,
}

impl MemoryFrame {
    /// Create a new memory frame with content and level.
    #[must_use]
    pub fn new(content: impl Into<String>, level: MemoryLevel) -> Self {
        Self {
            content: content.into(),
            level,
            tags: Vec::new(),
            metadata: BTreeMap::new(),
            source: None,
            observed_at: None,
        }
    }
}

/// A query to recall memories.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RecallQuery {
    /// Text to search for (semantic similarity).
    pub text: String,
    /// Filter by memory levels.
    pub levels: Option<Vec<MemoryLevel>>,
    /// Filter by tags.
    pub tags: Option<Vec<String>>,
    /// Maximum number of results.
    pub limit: Option<usize>,
    /// Minimum similarity score (0.0–1.0).
    pub min_score: Option<f32>,
}

impl RecallQuery {
    /// Create a new recall query with search text.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            levels: None,
            tags: None,
            limit: None,
            min_score: None,
        }
    }
}

/// A memory recall result with similarity score.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct MemoryHit {
    /// Memory identifier.
    pub id: MemoryId,
    /// Memory content.
    pub content: String,
    /// Cognitive level.
    pub level: MemoryLevel,
    /// Similarity score (0.0–1.0).
    pub score: f32,
    /// Tags.
    pub tags: Vec<String>,
    /// Metadata.
    pub metadata: BTreeMap<String, String>,
}

impl MemoryHit {
    /// Create a new memory hit.
    #[must_use]
    pub fn new(id: MemoryId, content: impl Into<String>, level: MemoryLevel, score: f32) -> Self {
        Self {
            id,
            content: content.into(),
            level,
            score,
            tags: Vec::new(),
            metadata: BTreeMap::new(),
        }
    }
}

/// Directive for memory behavior during inference.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum MemoryDirective {
    /// Let the system decide (default).
    #[default]
    Auto,
    /// Only recall, don't store.
    RecallOnly,
    /// Only store, don't recall.
    RememberOnly,
    /// Explicit recall and remember lists.
    Explicit {
        /// Topics to recall.
        recall: Vec<String>,
        /// Topics to remember.
        remember: Vec<String>,
    },
    /// Disable memory entirely for this request.
    Disabled,
}

/// Lightweight memory reference for `InferResponse`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct MemoryFrameRef {
    /// Memory identifier.
    pub id: MemoryId,
    /// Cognitive level.
    pub level: MemoryLevel,
    /// Brief summary.
    pub summary: String,
}

impl MemoryFrameRef {
    /// Create a new memory frame reference.
    #[must_use]
    pub fn new(id: MemoryId, level: MemoryLevel, summary: impl Into<String>) -> Self {
        Self {
            id,
            level,
            summary: summary.into(),
        }
    }
}

// ─── Errors ──────────────────────────────────────────────────────────

/// Memory subsystem errors.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum MemoryError {
    /// Memory system not available.
    #[error("memory unavailable: {reason}")]
    Unavailable {
        /// Why the memory system is unavailable.
        reason: String,
    },

    /// Memory not found.
    #[error("memory not found: {id}")]
    NotFound {
        /// The ID that was looked up.
        id: MemoryId,
    },

    /// Embedding generation failed.
    #[error("embedding failed: {reason}")]
    EmbeddingFailed {
        /// Why embedding failed.
        reason: String,
    },

    /// Storage backend error.
    #[error("memory storage error: {reason}")]
    Storage {
        /// Description of the storage failure.
        reason: String,
    },
}

// ─── Traits ──────────────────────────────────────────────────────────

/// Store a memory frame.
#[trait_variant::make(MemoryRememberDyn: Send)]
pub trait MemoryRemember: Send + Sync {
    /// Store a memory frame and return its identifier.
    async fn remember(&self, frame: MemoryFrame) -> Result<MemoryId, MemoryError>;
}

/// Recall memories by similarity.
#[trait_variant::make(MemoryRecallDyn: Send)]
pub trait MemoryRecall: Send + Sync {
    /// Search memories and return ranked results.
    async fn recall(&self, query: RecallQuery) -> Result<Vec<MemoryHit>, MemoryError>;
}

/// Forget a memory by its identifier.
#[trait_variant::make(MemoryForgetDyn: Send)]
pub trait MemoryForget: Send + Sync {
    /// Remove a memory from the store.
    async fn forget(&self, id: MemoryId) -> Result<(), MemoryError>;
}

/// Full memory store — blanket super-trait.
pub trait MemoryStore: MemoryRemember + MemoryRecall + MemoryForget {}
impl<T: MemoryRemember + MemoryRecall + MemoryForget> MemoryStore for T {}

/// Embedding vector generation.
#[trait_variant::make(EmbeddingProviderDyn: Send)]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate an embedding vector for the given text.
    async fn embed(&self, text: &str) -> Result<Vec<f32>, MemoryError>;

    /// Dimensionality of the embedding vectors.
    fn dimension(&self) -> usize;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_id_display() {
        let id = MemoryId::new(255);
        assert_eq!(id.to_string(), "mem-000000000000000000000000000000ff");
    }

    #[test]
    fn memory_id_nil() {
        let id = MemoryId::nil();
        assert_eq!(id.as_u128(), 0);
        assert_eq!(id.to_string(), "mem-00000000000000000000000000000000");
    }

    #[test]
    fn memory_id_as_u128_nonzero() {
        let id = MemoryId::new(42);
        assert_eq!(id.as_u128(), 42);
    }

    #[test]
    fn memory_id_serde_roundtrip() {
        let id = MemoryId::new(42);
        let json = serde_json::to_string(&id).expect("serialize");
        let back: MemoryId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(id, back);
    }

    #[test]
    fn memory_level_serde_roundtrip() {
        let level = MemoryLevel::Episodic;
        let json = serde_json::to_string(&level).expect("serialize");
        assert_eq!(json, "\"episodic\"");
        let back: MemoryLevel = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, level);
    }

    #[test]
    fn memory_frame_new_defaults() {
        let frame = MemoryFrame::new("test content", MemoryLevel::Working);
        assert_eq!(frame.content, "test content");
        assert_eq!(frame.level, MemoryLevel::Working);
        assert!(frame.tags.is_empty());
        assert!(frame.metadata.is_empty());
        assert!(frame.source.is_none());
        assert!(frame.observed_at.is_none());
    }

    #[test]
    fn recall_query_new_defaults() {
        let query = RecallQuery::new("search term");
        assert_eq!(query.text, "search term");
        assert!(query.levels.is_none());
        assert!(query.tags.is_none());
        assert!(query.limit.is_none());
        assert!(query.min_score.is_none());
    }

    #[test]
    fn memory_hit_new() {
        let hit = MemoryHit::new(MemoryId::new(1), "content", MemoryLevel::Semantic, 0.95);
        assert_eq!(hit.score, 0.95);
        assert_eq!(hit.level, MemoryLevel::Semantic);
    }

    #[test]
    fn memory_directive_default_is_auto() {
        let directive = MemoryDirective::default();
        assert!(matches!(directive, MemoryDirective::Auto));
    }

    #[test]
    fn memory_frame_ref_new() {
        let r = MemoryFrameRef::new(MemoryId::new(1), MemoryLevel::Working, "summary");
        assert_eq!(r.summary, "summary");
    }

    #[test]
    fn memory_error_display() {
        let err = MemoryError::Unavailable {
            reason: "not configured".into(),
        };
        assert!(err.to_string().contains("unavailable"));

        let err = MemoryError::NotFound {
            id: MemoryId::new(5),
        };
        assert!(err.to_string().contains("not found"));
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn memory_types_send_sync() {
        _assert_send_sync::<MemoryId>();
        _assert_send_sync::<MemoryFrame>();
        _assert_send_sync::<RecallQuery>();
        _assert_send_sync::<MemoryHit>();
        _assert_send_sync::<MemoryFrameRef>();
    }
}
