// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `NullMemoryStore` + `NullEmbeddingProvider` — no-op Cortex stubs.

use nika_kernel::memory::{
    EmbeddingProvider, MemoryError, MemoryForget, MemoryFrame, MemoryHit, MemoryId,
    MemoryRecall, MemoryRemember, RecallQuery,
};

/// No-op memory store that accepts all writes and returns empty recalls.
///
/// Used when tests don't exercise memory but need a value for type requirements.
#[derive(Clone, Debug, Default)]
pub struct NullMemoryStore;

impl NullMemoryStore {
    /// Create a new null memory store.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl MemoryRemember for NullMemoryStore {
    async fn remember(&self, _frame: MemoryFrame) -> Result<MemoryId, MemoryError> {
        Ok(MemoryId::nil())
    }
}

impl MemoryRecall for NullMemoryStore {
    async fn recall(&self, _query: RecallQuery) -> Result<Vec<MemoryHit>, MemoryError> {
        Ok(Vec::new())
    }
}

impl MemoryForget for NullMemoryStore {
    async fn forget(&self, _id: MemoryId) -> Result<(), MemoryError> {
        Ok(())
    }
}

/// No-op embedding provider that returns zero vectors.
///
/// Used when tests don't exercise embeddings but need a value.
#[derive(Clone, Debug)]
pub struct NullEmbeddingProvider {
    dim: usize,
}

impl NullEmbeddingProvider {
    /// Create a new null embedding provider with the given dimensionality.
    #[must_use]
    pub fn new(dimension: usize) -> Self {
        Self { dim: dimension }
    }
}

impl EmbeddingProvider for NullEmbeddingProvider {
    async fn embed(&self, _text: &str) -> Result<Vec<f32>, MemoryError> {
        Ok(vec![0.0; self.dim])
    }

    fn dimension(&self) -> usize {
        self.dim
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_kernel::memory::MemoryLevel;

    #[tokio::test]
    async fn null_remember_returns_nil() {
        let store = NullMemoryStore::new();
        let frame = MemoryFrame::new("test", MemoryLevel::Working);
        let id = store.remember(frame).await.unwrap();
        assert_eq!(id, MemoryId::nil());
    }

    #[tokio::test]
    async fn null_recall_returns_empty() {
        let store = NullMemoryStore::new();
        let hits = store.recall(RecallQuery::new("search")).await.unwrap();
        assert!(hits.is_empty());
    }

    #[tokio::test]
    async fn null_forget_succeeds() {
        let store = NullMemoryStore::new();
        store.forget(MemoryId::nil()).await.unwrap();
    }

    #[test]
    fn null_memory_satisfies_memory_store() {
        fn _accepts<T: nika_kernel::MemoryStore>(_: &T) {}
        _accepts(&NullMemoryStore::new());
    }

    #[tokio::test]
    async fn null_embedding_returns_zero_vector() {
        let provider = NullEmbeddingProvider::new(384);
        let vec = provider.embed("hello").await.unwrap();
        assert_eq!(vec.len(), 384);
        assert!(vec.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn null_embedding_dimension() {
        let provider = NullEmbeddingProvider::new(768);
        assert_eq!(provider.dimension(), 768);
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn null_types_are_send_sync() {
        _assert_send_sync::<NullMemoryStore>();
        _assert_send_sync::<NullEmbeddingProvider>();
    }
}
