// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `MockBlob` — in-memory content-addressable blob store.

use std::collections::BTreeMap;
use std::sync::Arc;

use bytes::Bytes;
use parking_lot::RwLock;

use nika_kernel::blob::{BlobError, BlobMetadata, BlobStore};

/// In-memory blob store for tests.
///
/// Uses a simple incrementing counter for hash generation.
#[derive(Clone, Default)]
pub struct MockBlob {
    store: Arc<RwLock<BTreeMap<String, (Bytes, String)>>>,
    counter: Arc<parking_lot::Mutex<u64>>,
}

impl MockBlob {
    /// Create a new empty blob store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed a blob (builder pattern).
    #[must_use]
    pub fn with_blob(self, hash: &str, data: impl Into<Bytes>, mime: &str) -> Self {
        self.store
            .write()
            .insert(hash.to_string(), (data.into(), mime.to_string()));
        self
    }

    /// Number of stored blobs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.store.read().len()
    }

    /// Whether the store is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.store.read().is_empty()
    }
}

impl BlobStore for MockBlob {
    async fn put(&self, data: Bytes, mime_type: &str) -> Result<BlobMetadata, BlobError> {
        let mut counter = self.counter.lock();
        *counter += 1;
        let hash = format!("mock-{:08x}", *counter);
        let size = data.len() as u64;
        self.store
            .write()
            .insert(hash.clone(), (data, mime_type.to_string()));
        Ok(BlobMetadata::new(hash, mime_type, size))
    }

    async fn get(&self, hash: &str) -> Result<Bytes, BlobError> {
        self.store
            .read()
            .get(hash)
            .map(|(data, _)| data.clone())
            .ok_or_else(|| BlobError::NotFound {
                hash: hash.to_string(),
            })
    }

    async fn exists(&self, hash: &str) -> bool {
        self.store.read().contains_key(hash)
    }

    async fn stat(&self, hash: &str) -> Result<BlobMetadata, BlobError> {
        self.store
            .read()
            .get(hash)
            .map(|(data, mime)| BlobMetadata::new(hash, mime.as_str(), data.len() as u64))
            .ok_or_else(|| BlobError::NotFound {
                hash: hash.to_string(),
            })
    }

    async fn delete(&self, hash: &str) -> Result<(), BlobError> {
        self.store
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
    async fn put_and_get_roundtrip() {
        let blob = MockBlob::new();
        let meta = blob
            .put(Bytes::from_static(b"hello"), "text/plain")
            .await
            .unwrap();
        assert!(meta.hash.starts_with("mock-"));
        assert_eq!(meta.size, 5);
        let data = blob.get(&meta.hash).await.unwrap();
        assert_eq!(data.as_ref(), b"hello");
    }

    #[tokio::test]
    async fn get_missing_returns_not_found() {
        let blob = MockBlob::new();
        let result = blob.get("nonexistent").await;
        assert!(matches!(result, Err(BlobError::NotFound { .. })));
    }

    #[tokio::test]
    async fn exists_works() {
        let blob = MockBlob::new().with_blob("h1", "data", "text/plain");
        assert!(blob.exists("h1").await);
        assert!(!blob.exists("h2").await);
    }

    #[tokio::test]
    async fn stat_works() {
        let blob = MockBlob::new().with_blob("h1", "12345", "image/png");
        let meta = blob.stat("h1").await.unwrap();
        assert_eq!(meta.hash, "h1");
        assert_eq!(meta.mime_type, "image/png");
        assert_eq!(meta.size, 5);
    }

    #[tokio::test]
    async fn delete_works() {
        let blob = MockBlob::new().with_blob("h1", "data", "text/plain");
        blob.delete("h1").await.unwrap();
        assert!(!blob.exists("h1").await);
    }

    #[tokio::test]
    async fn delete_missing_returns_error() {
        let blob = MockBlob::new();
        let result = blob.delete("nope").await;
        assert!(result.is_err());
    }

    #[test]
    fn len_and_is_empty() {
        let blob = MockBlob::new();
        assert!(blob.is_empty());
        let blob = blob.with_blob("h1", "x", "t");
        assert_eq!(blob.len(), 1);
    }

    #[test]
    fn is_empty_returns_false_for_non_empty() {
        let blob = MockBlob::new().with_blob("h1", "data", "text/plain");
        assert!(!blob.is_empty());
    }

    #[tokio::test]
    async fn put_counter_produces_different_hashes() {
        let blob = MockBlob::new();
        let m1 = blob
            .put(Bytes::from_static(b"aaa"), "text/plain")
            .await
            .unwrap();
        let m2 = blob
            .put(Bytes::from_static(b"bbb"), "text/plain")
            .await
            .unwrap();
        assert_ne!(m1.hash, m2.hash);
    }

    #[test]
    fn clone_shares_state() {
        let b1 = MockBlob::new().with_blob("h1", "x", "t");
        let b2 = b1.clone();
        assert_eq!(b2.len(), 1);
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn mock_blob_is_send_sync() {
        _assert_send_sync::<MockBlob>();
    }
}
