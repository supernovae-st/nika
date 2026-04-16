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

// ─── Types descended to nika-error (Phase 0) ────────────────────────
pub use nika_error::memory::{MemoryDirective, MemoryFrameRef, MemoryId, MemoryLevel};

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
    /// Encryption cipher used (v0.95 Cortex — encrypted memory).
    /// Reserved: always `None` until nika-memory crate ships.
    pub cipher: Option<String>,
    /// Provenance chain (v0.95 Cortex — who created this memory).
    /// Reserved: always `None` until nika-memory crate ships.
    pub provenance: Option<String>,
    /// Retention policy tag (v0.95 Cortex — TTL / archival).
    /// Reserved: always `None` until nika-memory crate ships.
    pub retention: Option<String>,
    /// Redacted field paths (v0.95 Cortex — PII scrubbing).
    /// Reserved: always `None` until nika-memory crate ships.
    pub redactions: Option<Vec<String>>,
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
            cipher: None,
            provenance: None,
            retention: None,
            redactions: None,
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
    /// Filter: only return memories observed after this timestamp (Unix ms).
    /// Reserved for nika-memory-temporal (v0.95).
    pub observed_after: Option<u64>,
    /// Filter: only return memories observed before this timestamp (Unix ms).
    /// Reserved for nika-memory-temporal (v0.95).
    pub observed_before: Option<u64>,
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
            observed_after: None,
            observed_before: None,
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
    /// When this memory was observed (parity with `MemoryFrame`).
    /// Reserved for nika-memory-temporal ranking (v0.95).
    pub observed_at: Option<u64>,
    /// Source of this memory (parity with `MemoryFrame`).
    /// Reserved for v0.95 Cortex provenance tracking.
    pub source: Option<String>,
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
            observed_at: None,
            source: None,
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
    ///
    /// CANCEL SAFETY: cancel-safe if impl uses atomic insert (single DB
    /// transaction, single fs rename). A dropped future never leaves a
    /// half-committed frame that can be recalled. Impls using multi-step
    /// transactions MUST wrap them in rollback-on-drop guards.
    async fn remember(&self, frame: MemoryFrame) -> Result<MemoryId, MemoryError>;
}

/// Recall memories by similarity.
#[trait_variant::make(MemoryRecallDyn: Send)]
pub trait MemoryRecall: Send + Sync {
    /// Search memories and return ranked results.
    ///
    /// CANCEL SAFETY: cancel-safe — read-only similarity search.
    async fn recall(&self, query: RecallQuery) -> Result<Vec<MemoryHit>, MemoryError>;
}

/// Forget a memory by its identifier.
#[trait_variant::make(MemoryForgetDyn: Send)]
pub trait MemoryForget: Send + Sync {
    /// Remove a memory from the store.
    ///
    /// CANCEL SAFETY: cancel-safe — idempotent delete. Retrying on a
    /// missing id is a no-op.
    async fn forget(&self, id: MemoryId) -> Result<(), MemoryError>;
}

/// Full memory store — blanket super-trait.
pub trait MemoryStore: MemoryRemember + MemoryRecall + MemoryForget {}
impl<T: MemoryRemember + MemoryRecall + MemoryForget> MemoryStore for T {}

/// Embedding vector generation.
#[trait_variant::make(EmbeddingProviderDyn: Send)]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate an embedding vector for the given text.
    ///
    /// CANCEL SAFETY: cancel-safe — embeddings are deterministic over
    /// (model, text) and produce no side effects. Drops abort the inference
    /// call without leaking state.
    async fn embed(&self, text: &str) -> Result<Vec<f32>, MemoryError>;

    /// Dimensionality of the embedding vectors.
    fn dimension(&self) -> usize;
}

/// Batch embed texts by calling [`EmbeddingProvider::embed`] sequentially.
///
/// Sequential fallback for providers that lack native batch embedding.
/// Override-capable providers (v0.95 hnsw, autodesc) should call their
/// backend's batch API directly instead.
///
/// # Errors
///
/// Returns [`MemoryError`] if any individual embed call fails.
///
/// CANCEL SAFETY: cancel-safe. Cancelling mid-batch abandons the remaining
/// per-element embeddings; already-completed results are dropped with the
/// future. Retrying from scratch is always safe (embeddings are pure).
pub async fn embed_batch_sequential(
    provider: &(impl EmbeddingProvider + ?Sized),
    texts: &[&str],
) -> Result<Vec<Vec<f32>>, MemoryError> {
    let mut results = Vec::with_capacity(texts.len());
    for text in texts {
        results.push(provider.embed(text).await?);
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    // MemoryId, MemoryLevel, MemoryDirective, MemoryFrameRef tests
    // are in nika-error::memory::tests (Phase 0 descent).

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
    fn memory_frame_new_cortex_reserved_fields_default_none() {
        let frame = MemoryFrame::new("test", MemoryLevel::Working);
        assert!(frame.cipher.is_none(), "cipher should default to None");
        assert!(
            frame.provenance.is_none(),
            "provenance should default to None"
        );
        assert!(
            frame.retention.is_none(),
            "retention should default to None"
        );
        assert!(
            frame.redactions.is_none(),
            "redactions should default to None"
        );
    }

    #[test]
    fn recall_query_new_defaults() {
        let query = RecallQuery::new("search term");
        assert_eq!(query.text, "search term");
        assert!(query.levels.is_none());
        assert!(query.tags.is_none());
        assert!(query.limit.is_none());
        assert!(query.min_score.is_none());
        assert!(query.observed_after.is_none());
        assert!(query.observed_before.is_none());
    }

    #[test]
    fn memory_hit_new() {
        let hit = MemoryHit::new(
            MemoryId::new(Uuid::from_u128(1)),
            "content",
            MemoryLevel::Semantic,
            0.95,
        );
        assert_eq!(hit.score, 0.95);
        assert_eq!(hit.level, MemoryLevel::Semantic);
        assert!(hit.observed_at.is_none());
        assert!(hit.source.is_none());
    }

    #[test]
    fn memory_error_display() {
        let err = MemoryError::Unavailable {
            reason: "not configured".into(),
        };
        assert!(err.to_string().contains("unavailable"));

        let err = MemoryError::NotFound {
            id: MemoryId::new(Uuid::from_u128(5)),
        };
        assert!(err.to_string().contains("not found"));
    }

    /// Test-only embedding provider returning zero vectors.
    struct ZeroEmbedder;
    impl EmbeddingProvider for ZeroEmbedder {
        async fn embed(&self, _text: &str) -> Result<Vec<f32>, MemoryError> {
            Ok(vec![0.0; 3])
        }
        fn dimension(&self) -> usize {
            3
        }
    }

    #[tokio::test]
    async fn embed_batch_sequential_calls_embed_per_text() {
        let embedder = ZeroEmbedder;
        let results = embed_batch_sequential(&embedder, &["hello", "world"])
            .await
            .expect("embed_batch should succeed");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].len(), 3);
        assert_eq!(results[1].len(), 3);
    }

    #[tokio::test]
    async fn embed_batch_sequential_empty_returns_empty() {
        let embedder = ZeroEmbedder;
        let results = embed_batch_sequential(&embedder, &[])
            .await
            .expect("empty batch should succeed");
        assert!(results.is_empty());
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn memory_types_send_sync() {
        _assert_send_sync::<MemoryId>();
        _assert_send_sync::<MemoryFrame>();
        _assert_send_sync::<RecallQuery>();
        _assert_send_sync::<MemoryHit>();
    }
}
