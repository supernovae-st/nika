// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! MemoryBlobStore — in-memory content-addressable storage for tests.

use std::collections::HashMap;
use std::sync::Arc;

use bytes::Bytes;
use parking_lot::RwLock;

use nika_kernel::store::{BlobError, BlobMetadata, BlobStore};

/// In-memory blob store backed by HashMap.
#[derive(Clone, Default)]
pub struct MemoryBlobStore {
    blobs: Arc<RwLock<HashMap<String, (Bytes, String)>>>,
}

impl MemoryBlobStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of blobs stored.
    pub fn len(&self) -> usize {
        self.blobs.read().len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.blobs.read().is_empty()
    }

    fn hash(data: &[u8]) -> String {
        // Simple hash for testing — not blake3, just content length + first bytes
        format!("mock:{:016x}", {
            let mut h: u64 = data.len() as u64;
            for (i, &b) in data.iter().take(32).enumerate() {
                h = h.wrapping_mul(31).wrapping_add(b as u64 + i as u64);
            }
            h
        })
    }
}

#[async_trait::async_trait]
impl BlobStore for MemoryBlobStore {
    async fn put(&self, data: Bytes, mime_type: &str) -> Result<BlobMetadata, BlobError> {
        let hash = Self::hash(&data);
        let size = data.len() as u64;
        self.blobs
            .write()
            .insert(hash.clone(), (data, mime_type.to_string()));
        Ok(BlobMetadata {
            hash,
            mime_type: mime_type.to_string(),
            size,
        })
    }

    async fn get(&self, hash: &str) -> Result<Bytes, BlobError> {
        self.blobs
            .read()
            .get(hash)
            .map(|(data, _)| data.clone())
            .ok_or_else(|| BlobError::NotFound {
                hash: hash.to_string(),
            })
    }

    async fn exists(&self, hash: &str) -> bool {
        self.blobs.read().contains_key(hash)
    }

    async fn stat(&self, hash: &str) -> Result<BlobMetadata, BlobError> {
        self.blobs
            .read()
            .get(hash)
            .map(|(data, mime)| BlobMetadata {
                hash: hash.to_string(),
                mime_type: mime.clone(),
                size: data.len() as u64,
            })
            .ok_or_else(|| BlobError::NotFound {
                hash: hash.to_string(),
            })
    }

    async fn delete(&self, hash: &str) -> Result<(), BlobError> {
        self.blobs
            .write()
            .remove(hash)
            .map(|_| ())
            .ok_or_else(|| BlobError::NotFound {
                hash: hash.to_string(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn put_get_roundtrip() {
        let store = MemoryBlobStore::new();
        let data = Bytes::from("hello blob");
        let meta = store.put(data.clone(), "text/plain").await.unwrap();
        let retrieved = store.get(&meta.hash).await.unwrap();
        assert_eq!(retrieved, data);
    }

    #[tokio::test]
    async fn get_missing_errors() {
        let store = MemoryBlobStore::new();
        let err = store.get("nonexistent").await.unwrap_err();
        assert!(matches!(err, BlobError::NotFound { .. }));
    }

    #[tokio::test]
    async fn exists_check() {
        let store = MemoryBlobStore::new();
        let meta = store
            .put(Bytes::from("data"), "application/octet-stream")
            .await
            .unwrap();
        assert!(store.exists(&meta.hash).await);
        assert!(!store.exists("bogus").await);
    }

    #[tokio::test]
    async fn stat_returns_metadata() {
        let store = MemoryBlobStore::new();
        let meta = store.put(Bytes::from("test"), "image/png").await.unwrap();
        let stat = store.stat(&meta.hash).await.unwrap();
        assert_eq!(stat.mime_type, "image/png");
        assert_eq!(stat.size, 4);
    }

    #[tokio::test]
    async fn delete_then_gone() {
        let store = MemoryBlobStore::new();
        let meta = store.put(Bytes::from("ephemeral"), "text/plain").await.unwrap();
        store.delete(&meta.hash).await.unwrap();
        assert!(!store.exists(&meta.hash).await);
    }
}
