// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `BlobStore` trait — content-addressable storage abstraction.
//!
//! Replaces the concrete `CasStore`. No ISP split — blob ops are atomic.

use bytes::Bytes;

/// Metadata about a stored blob.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct BlobMetadata {
    /// Blake3 hash of the content.
    pub hash: String,
    /// MIME type (e.g., `"image/png"`).
    pub mime_type: String,
    /// Size in bytes.
    pub size: u64,
}

impl BlobMetadata {
    /// Create new blob metadata.
    #[must_use]
    pub fn new(hash: impl Into<String>, mime_type: impl Into<String>, size: u64) -> Self {
        Self {
            hash: hash.into(),
            mime_type: mime_type.into(),
            size,
        }
    }
}

/// Blob storage errors.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum BlobError {
    /// Blob not found by hash.
    #[error("blob not found: {hash}")]
    NotFound {
        /// The hash that was looked up.
        hash: String,
    },

    /// Storage I/O error.
    #[error("storage I/O error: {reason}")]
    Io {
        /// Description of the I/O failure.
        reason: String,
    },

    /// Blob exceeds maximum allowed size.
    #[error("blob too large: {size} bytes (max {max})")]
    TooLarge {
        /// Actual size in bytes.
        size: u64,
        /// Maximum allowed size.
        max: u64,
    },
}

/// Content-addressable blob storage.
///
/// Production: `DiskBlobStore`.
/// Tests: `MockBlob` (in-memory `BTreeMap`).
///
/// CANCEL SAFETY: every method must be implemented as cancel-safe.
/// CAS semantics save us: partial writes are addressed by a different
/// hash than the complete write, so a dropped `put` cannot corrupt an
/// existing blob. Readers (`get`/`stat`/`exists`) are idempotent.
#[trait_variant::make(BlobStoreDyn: Send)]
pub trait BlobStore: Send + Sync {
    /// Store bytes and return metadata with content hash.
    ///
    /// CANCEL SAFETY: cancel-safe via CAS. If cancelled mid-write, any
    /// temporary file is cleaned up or stays at a hash nobody addresses.
    async fn put(&self, data: Bytes, mime_type: &str) -> Result<BlobMetadata, BlobError>;

    /// Retrieve bytes by content hash.
    ///
    /// CANCEL SAFETY: cancel-safe — read-only.
    async fn get(&self, hash: &str) -> Result<Bytes, BlobError>;

    /// Check if a blob exists by hash.
    ///
    /// CANCEL SAFETY: cancel-safe — read-only.
    async fn exists(&self, hash: &str) -> bool;

    /// Get metadata for a blob by hash.
    ///
    /// CANCEL SAFETY: cancel-safe — read-only.
    async fn stat(&self, hash: &str) -> Result<BlobMetadata, BlobError>;

    /// Delete a blob by hash.
    ///
    /// CANCEL SAFETY: cancel-safe. Delete is idempotent and either the
    /// unlink system call completes or it does not — no intermediate state.
    async fn delete(&self, hash: &str) -> Result<(), BlobError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_metadata_new() {
        let meta = BlobMetadata::new("blake3:abc", "image/png", 1024);
        assert_eq!(meta.hash, "blake3:abc");
        assert_eq!(meta.mime_type, "image/png");
        assert_eq!(meta.size, 1024);
    }

    #[test]
    fn blob_error_not_found_display() {
        let err = BlobError::NotFound {
            hash: "abc123".into(),
        };
        assert_eq!(err.to_string(), "blob not found: abc123");
    }

    #[test]
    fn blob_error_too_large_display() {
        let err = BlobError::TooLarge {
            size: 200,
            max: 100,
        };
        assert_eq!(err.to_string(), "blob too large: 200 bytes (max 100)");
    }

    #[test]
    fn blob_error_io_display() {
        let err = BlobError::Io {
            reason: "disk full".into(),
        };
        assert_eq!(err.to_string(), "storage I/O error: disk full");
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn blob_types_send_sync() {
        _assert_send_sync::<BlobMetadata>();
    }
}
