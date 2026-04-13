// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Engine-side implementation of the `MediaContext` kernel trait.
//!
//! `EngineMediaContext` wraps `MediaToolContext` (nika-media's concrete
//! type) and adapts it to `nika_kernel::scope::MediaContext`. This is the
//! bridge between the object-safe kernel trait (consumed by nika-builtin
//! and future verb crates) and the engine's concrete media infrastructure.
//!
//! # Compute dispatch
//!
//! `compute_blocking<F, T>` is intentionally NOT on `MediaContext` because
//! generic methods break vtable dispatch (`Arc<dyn MediaContext>` would
//! be impossible). Media tools access `MediaToolContext.compute` directly
//! — they always have the concrete type inside nika-media.
//!
//! # Object safety
//!
//! `Arc<dyn MediaContext>` coercion requires an explicit `let` binding with
//! type ascription at the construction site (Rust's unsizing coercion does
//! not fire deep inside generic argument inference):
//!
//! ```rust,ignore
//! // Correct: coercion fires at the binding site
//! let ctx: Arc<dyn MediaContext> = Arc::new(EngineMediaContext::new(inner));
//!
//! // Wrong: coercion may not fire inside argument position
//! router.with_media(Arc::new(EngineMediaContext::new(inner)));
//! ```

use std::sync::Arc;

use async_trait::async_trait;
use nika_kernel::{
    builtin::BuiltinError,
    scope::{BlobStoreResult, MediaContext},
};
use nika_media::tools::context::MediaToolContext;

/// Engine implementation of `MediaContext`.
///
/// Wraps `Arc<MediaToolContext>` — the concrete nika-media context shared
/// across all media tool invocations in a single workflow run.
///
pub(crate) struct EngineMediaContext {
    inner: Arc<MediaToolContext>,
}

impl EngineMediaContext {
    /// Create from a shared `MediaToolContext`.
    pub(crate) fn new(ctx: Arc<MediaToolContext>) -> Self {
        Self { inner: ctx }
    }

    /// Clone the underlying `Arc<MediaToolContext>`.
    ///
    /// Used by `MediaToolAdapter::new` to share a single context between
    /// the trait object and the direct `MediaOp::execute` call site.
    pub(crate) fn inner_arc(&self) -> &Arc<MediaToolContext> {
        &self.inner
    }
}

#[async_trait]
impl MediaContext for EngineMediaContext {
    async fn read_blob(&self, hash: &str) -> Result<Vec<u8>, BuiltinError> {
        // MediaToolContext::read_media delegates to cas.read() (async, no blocking).
        self.inner.read_media(hash).await.map_err(|e| BuiltinError::Io {
            tool: "media:read".into(),
            reason: format!("CAS read failed for {hash}: {e}"),
        })
    }

    async fn store_blob(
        &self,
        data: &[u8],
        task_id: &str,
    ) -> Result<BlobStoreResult, BuiltinError> {
        // MediaToolContext::store_media handles budget check + rollback on failure.
        // Projects the 6-field StoreResult to the 3-field kernel subset.
        let result = self.inner.store_media(data, task_id).await.map_err(|e| {
            BuiltinError::Io {
                tool: "media:store".into(),
                reason: format!("CAS store failed: {e}"),
            }
        })?;
        Ok(BlobStoreResult {
            hash: result.hash,
            size: result.size,
            deduplicated: result.deduplicated,
        })
    }

    fn working_dir(&self) -> Option<&std::path::Path> {
        // MediaToolContext.working_dir is Option<PathBuf>; as_deref → Option<&Path>
        self.inner.working_dir.as_deref()
    }

    fn is_cancelled(&self) -> bool {
        // Field is `cancel: CancellationToken` (NOT `cancellation_token`).
        self.inner.cancel.is_cancelled()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use nika_media::{store::CasStore, types::MediaBudget};
    use nika_media::tools::context::{ComputePool, WorkingMemoryBudget};
    use tokio_util::sync::CancellationToken;

    // ── Fixtures ─────────────────────────────────────────────────────────────

    /// Build a fully-functional EngineMediaContext backed by a temp CAS,
    /// default budgets, and a 2-thread rayon pool.
    fn make_ctx() -> (EngineMediaContext, tempfile::TempDir) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let inner = MediaToolContext {
            cas: CasStore::new(tmp.path().join("cas")),
            budget: Arc::new(MediaBudget::new()),
            compute: Arc::new(ComputePool::new().expect("rayon pool")),
            working_memory: Arc::new(WorkingMemoryBudget::new()),
            cancel: CancellationToken::new(),
            working_dir: Some(tmp.path().to_path_buf()),
        };
        (EngineMediaContext::new(Arc::new(inner)), tmp)
    }

    /// Build a context where the CancellationToken can be fired externally.
    fn make_cancellable_ctx() -> (EngineMediaContext, CancellationToken, tempfile::TempDir) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let token = CancellationToken::new();
        let inner = MediaToolContext {
            cas: CasStore::new(tmp.path().join("cas")),
            budget: Arc::new(MediaBudget::new()),
            compute: Arc::new(ComputePool::new().expect("rayon pool")),
            working_memory: Arc::new(WorkingMemoryBudget::new()),
            cancel: token.clone(),
            working_dir: Some(tmp.path().to_path_buf()),
        };
        (EngineMediaContext::new(Arc::new(inner)), token, tmp)
    }

    #[tokio::test]
    async fn not_cancelled_initially() {
        let (ctx, _tmp) = make_ctx();
        assert!(!ctx.is_cancelled(), "fresh context must not be cancelled");
    }

    #[tokio::test]
    async fn cancellation_observable_via_trait() {
        let (ctx, token, _tmp) = make_cancellable_ctx();
        assert!(!ctx.is_cancelled());
        token.cancel();
        assert!(ctx.is_cancelled(), "cancel() must be observable via is_cancelled()");
    }

    #[tokio::test]
    async fn store_and_read_roundtrip() {
        let (ctx, _tmp) = make_ctx();
        let data = b"hello world from engine media context";

        let result = ctx.store_blob(data, "test_task").await
            .expect("store_blob must succeed");

        // Validate hash prefix (type check)
        assert!(
            result.hash.starts_with("blake3:"),
            "hash must have blake3: prefix, got: {}",
            result.hash
        );
        // Validate size (programmatic check, not `!is_empty()`)
        assert_eq!(result.size, data.len() as u64, "size must match bytes stored");
        assert!(!result.deduplicated, "first store must not be deduplicated");

        // Read back and verify content
        let retrieved = ctx.read_blob(&result.hash).await
            .expect("read_blob must succeed for a just-stored blob");
        assert_eq!(retrieved, data, "round-trip must return identical bytes");
    }

    #[tokio::test]
    async fn dedup_on_second_store() {
        let (ctx, _tmp) = make_ctx();
        let data = b"deduplication test bytes";

        let first = ctx.store_blob(data, "task1").await.expect("first store");
        let second = ctx.store_blob(data, "task2").await.expect("second store");

        assert_eq!(first.hash, second.hash, "identical bytes must produce same hash");
        assert!(second.deduplicated, "second store with same bytes must be deduplicated");
    }

    #[tokio::test]
    async fn store_empty_bytes_returns_error() {
        // CAS store enforces NIKA-258: empty content is rejected.
        // Storing empty bytes is a degenerate case that the engine explicitly guards.
        let (ctx, _tmp) = make_ctx();
        let result = ctx.store_blob(&[], "empty_task").await;
        assert!(result.is_err(), "storing empty bytes must return Err (NIKA-258)");
    }

    #[tokio::test]
    async fn read_unknown_hash_returns_io_error() {
        let (ctx, _tmp) = make_ctx();
        // A well-formed but non-existent hash
        let fake_hash = "blake3:0000000000000000000000000000000000000000000000000000000000000000";
        let result = ctx.read_blob(fake_hash).await;

        assert!(result.is_err(), "reading unknown hash must return Err");
        let err = result.unwrap_err();
        // Must identify the failing operation
        let err_str = format!("{err:?}");
        assert!(
            err_str.contains("media:read"),
            "error must identify 'media:read', got: {err_str}"
        );
    }

    #[tokio::test]
    async fn working_dir_matches_inner() {
        let (ctx, tmp) = make_ctx();
        let got = ctx.working_dir();
        assert_eq!(
            got,
            Some(tmp.path()),
            "working_dir must match the path set on MediaToolContext"
        );
    }

    #[tokio::test]
    async fn working_dir_none_when_not_set() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let inner = MediaToolContext {
            cas: CasStore::new(tmp.path().join("cas")),
            budget: Arc::new(MediaBudget::new()),
            compute: Arc::new(ComputePool::new().expect("rayon pool")),
            working_memory: Arc::new(WorkingMemoryBudget::new()),
            cancel: CancellationToken::new(),
            working_dir: None,  // deliberately unset
        };
        let ctx = EngineMediaContext::new(Arc::new(inner));
        assert_eq!(ctx.working_dir(), None, "working_dir must be None when not configured");
    }

    #[tokio::test]
    async fn store_multiple_distinct_blobs() {
        let (ctx, _tmp) = make_ctx();
        let blobs: &[&[u8]] = &[b"alpha", b"beta", b"gamma"];
        let mut hashes = Vec::new();

        for (i, blob) in blobs.iter().enumerate() {
            let r = ctx.store_blob(blob, &format!("task_{i}")).await
                .expect("store must succeed");
            hashes.push(r.hash);
        }

        // All hashes must be distinct
        let unique_count = {
            let mut deduped = hashes.clone();
            deduped.sort();
            deduped.dedup();
            deduped.len()
        };
        assert_eq!(
            unique_count,
            blobs.len(),
            "distinct blobs must produce distinct hashes"
        );
    }

    #[test]
    fn engine_media_context_is_send_sync() {
        // Compile-time check: EngineMediaContext implements Send + Sync
        fn _assert_send_sync<T: Send + Sync>() {}
        _assert_send_sync::<EngineMediaContext>();
    }
}
