// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Production [`BlobStore`] implementation with blake3 content-addressable storage.
//!
//! This crate sits at L1 in the dependency graph — it implements the
//! [`nika_kernel::store::BlobStore`] trait using filesystem storage with
//! blake3 hashes as keys.
//!
//! # Storage Layout
//!
//! ```text
//! {root}/
//! ├── ab/           ← first 2 chars of hash (sharding)
//! │   └── cdef...   ← remaining hash chars (no extension)
//! └── 01/
//!     └── 2345...
//! ```

use std::path::{Path, PathBuf};

use bytes::Bytes;
use nika_kernel::store::{BlobError, BlobMetadata, BlobStore};

/// Maximum blob size: 500 MB.
const MAX_BLOB_SIZE: u64 = 500 * 1024 * 1024;

/// Hash prefix for Nika CAS hashes.
const HASH_PREFIX: &str = "blake3:";

/// Content-addressable blob storage backed by the filesystem.
///
/// Blobs are stored with their blake3 hash as the filename, sharded
/// into subdirectories by the first 2 hex chars of the hash.
#[derive(Debug, Clone)]
pub struct DiskBlobStore {
    root: PathBuf,
}

impl DiskBlobStore {
    /// Create a blob store at the given root directory.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Create a blob store at the workspace default location.
    ///
    /// Uses `{workspace_root}/.nika/media/store/`.
    /// Respects `NIKA_MEDIA_STORE` env var as override.
    pub fn workspace_default(workspace_root: &Path) -> Self {
        if let Ok(override_path) = std::env::var("NIKA_MEDIA_STORE") {
            let path = PathBuf::from(&override_path);
            let resolved = path.canonicalize().unwrap_or(path);
            return Self::new(resolved);
        }
        Self::new(workspace_root.join(".nika").join("media").join("store"))
    }

    /// The root directory of this blob store.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Derive the filesystem path for a given hash.
    fn blob_path(&self, hash: &str) -> PathBuf {
        let raw = hash.strip_prefix(HASH_PREFIX).unwrap_or(hash);
        if raw.len() >= 2 {
            self.root.join(&raw[..2]).join(&raw[2..])
        } else {
            self.root.join(raw)
        }
    }

    /// Detect MIME type from data bytes.
    fn detect_mime(data: &[u8]) -> String {
        infer::get(data)
            .map(|t| t.mime_type().to_string())
            .unwrap_or_else(|| "application/octet-stream".to_string())
    }
}

/// Convert an `io::Error` to a `BlobError`, mapping NotFound to BlobError::NotFound.
#[allow(clippy::needless_pass_by_value)]
fn io_to_blob_error(e: std::io::Error, hash: &str, op: &str) -> BlobError {
    if e.kind() == std::io::ErrorKind::NotFound {
        BlobError::NotFound {
            hash: hash.to_string(),
        }
    } else {
        BlobError::Io {
            reason: format!("failed to {op} blob: {e}"),
        }
    }
}

#[async_trait::async_trait]
impl BlobStore for DiskBlobStore {
    async fn put(&self, data: Bytes, mime_type: &str) -> Result<BlobMetadata, BlobError> {
        if data.is_empty() {
            return Err(BlobError::Io {
                reason: "cannot store empty blob".to_string(),
            });
        }

        let size = data.len() as u64;
        if size > MAX_BLOB_SIZE {
            return Err(BlobError::TooLarge {
                size,
                max: MAX_BLOB_SIZE,
            });
        }

        // Blake3 hash of original data
        let raw_hash = blake3::hash(&data).to_hex().to_string();
        let prefixed_hash = format!("{HASH_PREFIX}{raw_hash}");

        let path = self.blob_path(&prefixed_hash);

        // Check for deduplication
        if tokio::fs::try_exists(&path).await.unwrap_or(false) {
            tracing::debug!(hash = %prefixed_hash, "blob already exists (deduplicated)");
            return Ok(BlobMetadata {
                hash: prefixed_hash,
                mime_type: mime_type.to_string(),
                size,
            });
        }

        // Create parent directory
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| BlobError::Io {
                    reason: format!("failed to create directory: {e}"),
                })?;
        }

        // Write atomically: write to temp file, then rename
        let temp_path = path.with_extension("tmp");
        tokio::fs::write(&temp_path, &data)
            .await
            .map_err(|e| BlobError::Io {
                reason: format!("failed to write blob: {e}"),
            })?;

        // Rename to final path (atomic on same filesystem)
        match tokio::fs::rename(&temp_path, &path).await {
            Ok(()) => {}
            Err(e) => {
                // Another writer may have placed the file — that's fine (dedup)
                if tokio::fs::try_exists(&path).await.unwrap_or(false) {
                    let _ = tokio::fs::remove_file(&temp_path).await;
                } else {
                    let _ = tokio::fs::remove_file(&temp_path).await;
                    return Err(BlobError::Io {
                        reason: format!("failed to commit blob: {e}"),
                    });
                }
            }
        }

        Ok(BlobMetadata {
            hash: prefixed_hash,
            mime_type: mime_type.to_string(),
            size,
        })
    }

    async fn get(&self, hash: &str) -> Result<Bytes, BlobError> {
        let path = self.blob_path(hash);
        let data = tokio::fs::read(&path)
            .await
            .map_err(|e| io_to_blob_error(e, hash, "read"))?;
        Ok(Bytes::from(data))
    }

    async fn exists(&self, hash: &str) -> bool {
        let path = self.blob_path(hash);
        tokio::fs::try_exists(&path).await.unwrap_or(false)
    }

    async fn stat(&self, hash: &str) -> Result<BlobMetadata, BlobError> {
        let path = self.blob_path(hash);
        let metadata = tokio::fs::metadata(&path)
            .await
            .map_err(|e| io_to_blob_error(e, hash, "stat"))?;

        // Try to detect mime type from stored data
        let data = tokio::fs::read(&path).await.unwrap_or_default();
        let mime_type = Self::detect_mime(&data);

        Ok(BlobMetadata {
            hash: hash.to_string(),
            mime_type,
            size: metadata.len(),
        })
    }

    async fn delete(&self, hash: &str) -> Result<(), BlobError> {
        let path = self.blob_path(hash);
        tokio::fs::remove_file(&path)
            .await
            .map_err(|e| io_to_blob_error(e, hash, "delete"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_store() -> (tempfile::TempDir, DiskBlobStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = DiskBlobStore::new(dir.path().join("blobs"));
        (dir, store)
    }

    #[tokio::test]
    async fn put_and_get_roundtrip() {
        let (_dir, store) = make_store();
        let data = Bytes::from_static(b"hello world");

        let meta = store.put(data.clone(), "text/plain").await.unwrap();
        assert!(meta.hash.starts_with("blake3:"));
        assert_eq!(meta.size, 11);
        assert_eq!(meta.mime_type, "text/plain");

        let retrieved = store.get(&meta.hash).await.unwrap();
        assert_eq!(retrieved, data);
    }

    #[tokio::test]
    async fn deduplication() {
        let (_dir, store) = make_store();
        let data = Bytes::from_static(b"duplicate content");

        let meta1 = store.put(data.clone(), "text/plain").await.unwrap();
        let meta2 = store.put(data, "text/plain").await.unwrap();

        // Same hash — deduplicated
        assert_eq!(meta1.hash, meta2.hash);
    }

    #[tokio::test]
    async fn exists_returns_true_after_put() {
        let (_dir, store) = make_store();
        let data = Bytes::from_static(b"check existence");

        let meta = store.put(data, "text/plain").await.unwrap();
        assert!(store.exists(&meta.hash).await);
    }

    #[tokio::test]
    async fn exists_returns_false_for_missing() {
        let (_dir, store) = make_store();
        assert!(!store.exists("blake3:0000000000000000000000000000000000000000000000000000000000000000").await);
    }

    #[tokio::test]
    async fn stat_returns_metadata() {
        let (_dir, store) = make_store();
        let data = Bytes::from_static(b"stat me");

        let meta = store.put(data, "text/plain").await.unwrap();
        let stat = store.stat(&meta.hash).await.unwrap();

        assert_eq!(stat.size, 7);
        assert_eq!(stat.hash, meta.hash);
    }

    #[tokio::test]
    async fn delete_removes_blob() {
        let (_dir, store) = make_store();
        let data = Bytes::from_static(b"delete me");

        let meta = store.put(data, "text/plain").await.unwrap();
        assert!(store.exists(&meta.hash).await);

        store.delete(&meta.hash).await.unwrap();
        assert!(!store.exists(&meta.hash).await);
    }

    #[tokio::test]
    async fn get_missing_returns_not_found() {
        let (_dir, store) = make_store();
        let result = store.get("blake3:nonexistent").await;
        assert!(matches!(result, Err(BlobError::NotFound { .. })));
    }

    #[tokio::test]
    async fn delete_missing_returns_not_found() {
        let (_dir, store) = make_store();
        let result = store.delete("blake3:nonexistent").await;
        assert!(matches!(result, Err(BlobError::NotFound { .. })));
    }

    #[tokio::test]
    async fn rejects_empty_data() {
        let (_dir, store) = make_store();
        let result = store.put(Bytes::new(), "text/plain").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn hash_is_deterministic() {
        let (_dir, store) = make_store();
        let data = Bytes::from_static(b"deterministic hash test");

        let meta1 = store.put(data.clone(), "text/plain").await.unwrap();
        // Different store, same data -> same hash
        let store2 = DiskBlobStore::new(store.root().parent().unwrap().join("blobs2"));
        let meta2 = store2.put(data, "text/plain").await.unwrap();

        assert_eq!(meta1.hash, meta2.hash);
    }

    #[test]
    fn is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<DiskBlobStore>();
    }

    #[test]
    fn blob_path_shards_correctly() {
        let store = DiskBlobStore::new("/tmp/blobs");
        let path = store.blob_path("blake3:abcdef0123456789");
        assert_eq!(path, PathBuf::from("/tmp/blobs/ab/cdef0123456789"));
    }

    #[test]
    fn blob_path_handles_raw_hash() {
        let store = DiskBlobStore::new("/tmp/blobs");
        let path = store.blob_path("abcdef0123456789");
        assert_eq!(path, PathBuf::from("/tmp/blobs/ab/cdef0123456789"));
    }
}
