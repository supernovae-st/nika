// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! In-memory mock for `nika_kernel::scope::MediaContext`.
//!
//! Designed for unit tests that need to verify which media context
//! methods a tool or adapter invokes, without touching the filesystem
//! or a real CAS store.
//!
//! # Usage
//!
//! ```rust,ignore
//! let mock = Arc::new(MockMediaContext::new());
//! let ctx: Arc<dyn MediaContext> = mock.clone();
//! // ... call tool with ctx ...
//! assert_eq!(mock.store_count(), 1);
//! assert_eq!(mock.read_count(), 1);
//! ```

use std::{
    collections::HashMap,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

use async_trait::async_trait;
use parking_lot::Mutex;

use nika_kernel::{
    builtin::BuiltinError,
    scope::{BlobStoreResult, MediaContext},
};

/// In-memory `MediaContext` mock with call tracking.
///
/// Stores blobs in a `HashMap<hash, bytes>` where hash is computed via
/// blake3 (same algorithm as the production CAS) so deduplication
/// semantics match real behavior.
pub struct MockMediaContext {
    /// In-memory blob store: hash → bytes.
    blobs: Mutex<HashMap<String, Vec<u8>>>,
    /// Number of times `read_blob` has been called.
    pub read_count: AtomicUsize,
    /// Number of times `store_blob` has been called.
    pub store_count: AtomicUsize,
    /// Whether `cancel()` has been called.
    cancelled: AtomicBool,
    /// Optional working directory for path confinement.
    working_dir: Option<std::path::PathBuf>,
}

impl Default for MockMediaContext {
    fn default() -> Self {
        Self {
            blobs: Mutex::new(HashMap::new()),
            read_count: AtomicUsize::new(0),
            store_count: AtomicUsize::new(0),
            cancelled: AtomicBool::new(false),
            working_dir: None,
        }
    }
}

impl MockMediaContext {
    /// Create a new mock with no working directory set.
    pub fn new() -> Self {
        Default::default()
    }

    /// Create a mock with a working directory set for path confinement tests.
    pub fn with_working_dir(dir: std::path::PathBuf) -> Self {
        Self {
            working_dir: Some(dir),
            ..Default::default()
        }
    }

    /// Simulate workflow cancellation.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// Number of times `read_blob` was called.
    pub fn read_count(&self) -> usize {
        self.read_count.load(Ordering::Acquire)
    }

    /// Number of times `store_blob` was called.
    pub fn store_count(&self) -> usize {
        self.store_count.load(Ordering::Acquire)
    }

    /// Pre-populate the mock store with a blob. Returns the blake3 hash.
    ///
    /// Useful for tests that need to `read_blob` without a prior `store_blob`.
    pub fn seed_blob(&self, data: &[u8]) -> String {
        let hash = blake3_hash(data);
        self.blobs.lock().insert(hash.clone(), data.to_vec());
        hash
    }
}

#[async_trait]
impl MediaContext for MockMediaContext {
    async fn read_blob(&self, hash: &str) -> Result<Vec<u8>, BuiltinError> {
        self.read_count.fetch_add(1, Ordering::Release);
        self.blobs
            .lock()
            .get(hash)
            .cloned()
            .ok_or_else(|| BuiltinError::Io {
                tool: "mock:read".into(),
                reason: format!("blob not found: {hash}"),
            })
    }

    async fn store_blob(
        &self,
        data: &[u8],
        _task_id: &str,
    ) -> Result<BlobStoreResult, BuiltinError> {
        self.store_count.fetch_add(1, Ordering::Release);
        let hash = blake3_hash(data);
        let mut blobs = self.blobs.lock();
        let deduplicated = blobs.contains_key(&hash);
        blobs.insert(hash.clone(), data.to_vec());
        Ok(BlobStoreResult {
            hash,
            size: data.len() as u64,
            deduplicated,
        })
    }

    fn working_dir(&self) -> Option<&std::path::Path> {
        self.working_dir.as_deref()
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

/// Compute a blake3 hash with the same "blake3:" prefix as the production CAS.
fn blake3_hash(data: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(data).to_hex())
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    // ── Store / read conformance ──────────────────────────────────────────────

    #[tokio::test]
    async fn store_increments_count() {
        let mock = MockMediaContext::new();
        assert_eq!(mock.store_count(), 0);
        mock.store_blob(b"hello", "t1").await.unwrap();
        assert_eq!(mock.store_count(), 1);
        mock.store_blob(b"world", "t2").await.unwrap();
        assert_eq!(mock.store_count(), 2);
    }

    #[tokio::test]
    async fn read_increments_count() {
        let mock = MockMediaContext::new();
        let hash = mock.seed_blob(b"seed data");
        assert_eq!(mock.read_count(), 0);
        mock.read_blob(&hash).await.unwrap();
        assert_eq!(mock.read_count(), 1);
    }

    #[tokio::test]
    async fn store_and_read_roundtrip() {
        let mock = MockMediaContext::new();
        let data = b"roundtrip test";
        let result = mock.store_blob(data, "task1").await.unwrap();
        assert!(result.hash.starts_with("blake3:"), "hash must have blake3: prefix");
        assert_eq!(result.size, data.len() as u64);
        assert!(!result.deduplicated);

        let retrieved = mock.read_blob(&result.hash).await.unwrap();
        assert_eq!(retrieved, data);
    }

    #[tokio::test]
    async fn deduplication_on_identical_bytes() {
        let mock = MockMediaContext::new();
        let data = b"same bytes";
        let first = mock.store_blob(data, "task1").await.unwrap();
        let second = mock.store_blob(data, "task2").await.unwrap();
        assert_eq!(first.hash, second.hash, "identical bytes must produce same hash");
        assert!(second.deduplicated, "second store must be deduplicated");
        // Count is 2 even for dedup — each call is tracked
        assert_eq!(mock.store_count(), 2);
    }

    #[tokio::test]
    async fn read_unknown_hash_returns_error() {
        let mock = MockMediaContext::new();
        let result = mock.read_blob("blake3:nonexistent").await;
        assert!(result.is_err());
        let err = format!("{:?}", result.unwrap_err());
        assert!(err.contains("blob not found"), "error must mention 'blob not found': {err}");
    }

    // ── Cancellation ─────────────────────────────────────────────────────────

    #[test]
    fn not_cancelled_initially() {
        let mock = MockMediaContext::new();
        assert!(!mock.is_cancelled());
    }

    #[test]
    fn cancel_is_observable() {
        let mock = MockMediaContext::new();
        mock.cancel();
        assert!(mock.is_cancelled());
    }

    #[test]
    fn cancel_is_observable_via_arc() {
        let mock = Arc::new(MockMediaContext::new());
        let ctx: Arc<dyn MediaContext> = Arc::clone(&mock) as Arc<dyn MediaContext>;
        assert!(!ctx.is_cancelled());
        mock.cancel();
        assert!(ctx.is_cancelled(), "cancellation must be visible through Arc<dyn MediaContext>");
    }

    // ── Working directory ─────────────────────────────────────────────────────

    #[test]
    fn working_dir_none_by_default() {
        let mock = MockMediaContext::new();
        assert_eq!(mock.working_dir(), None);
    }

    #[test]
    fn working_dir_set_via_constructor() {
        let dir = std::path::PathBuf::from("/tmp/test");
        let mock = MockMediaContext::with_working_dir(dir.clone());
        assert_eq!(mock.working_dir(), Some(dir.as_path()));
    }

    // ── Object safety ─────────────────────────────────────────────────────────

    #[test]
    fn mock_is_send_sync() {
        fn _assert_send_sync<T: Send + Sync>() {}
        _assert_send_sync::<MockMediaContext>();
    }

    #[test]
    fn arc_dyn_media_context_constructable() {
        let _ctx: Arc<dyn MediaContext> = Arc::new(MockMediaContext::new());
    }

    // ── seed_blob helper ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn seed_blob_does_not_increment_read_count() {
        let mock = MockMediaContext::new();
        let hash = mock.seed_blob(b"seeded");
        assert_eq!(mock.read_count(), 0, "seed_blob must not increment read_count");
        // But reading the seeded blob does increment
        mock.read_blob(&hash).await.unwrap();
        assert_eq!(mock.read_count(), 1);
    }
}
