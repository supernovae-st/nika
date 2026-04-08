// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! BlobStore trait — content-addressable storage abstraction.
//!
//! Replaces the concrete `CasStore` god type.
//! Wire points: RunContext.cas, TaskExecutor.cas, MediaToolContext, artifact writer.

use bytes::Bytes;

/// Metadata about a stored blob.
#[derive(Debug, Clone)]
pub struct BlobMetadata {
    /// Blake3 hash of the content.
    pub hash: String,
    /// MIME type (e.g. "image/png").
    pub mime_type: String,
    /// Size in bytes.
    pub size: u64,
}

/// Content-addressable blob storage.
///
/// Production: `DiskBlobStore` (current `CasStore`).
/// Tests: `MemoryBlobStore` backed by HashMap.
#[async_trait::async_trait]
pub trait BlobStore: Send + Sync {
    /// Store bytes and return the content hash.
    async fn put(&self, data: Bytes, mime_type: &str) -> Result<BlobMetadata, BlobError>;

    /// Retrieve bytes by content hash.
    async fn get(&self, hash: &str) -> Result<Bytes, BlobError>;

    /// Check if a blob exists by hash.
    async fn exists(&self, hash: &str) -> bool;

    /// Get metadata for a blob by hash.
    async fn stat(&self, hash: &str) -> Result<BlobMetadata, BlobError>;

    /// Delete a blob by hash.
    async fn delete(&self, hash: &str) -> Result<(), BlobError>;
}

/// Blob store errors.
#[derive(Debug, thiserror::Error)]
pub enum BlobError {
    #[error("Blob not found: {hash}")]
    NotFound { hash: String },

    #[error("Storage I/O error: {reason}")]
    Io { reason: String },

    #[error("Blob too large: {size} bytes (max {max})")]
    TooLarge { size: u64, max: u64 },
}
