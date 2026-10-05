// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! CAS over an injected filesystem: containment and publication stay with its
//! owner. No ambient path is opened by this adapter.

use std::path::{Component, PathBuf};
use std::sync::Arc;

use bytes::Bytes;
use nika_kernel::blob::BlobStoreDyn;
use nika_kernel::fs::{FsError, FsMetaDyn, FsReadDyn, FsWriteDyn};
use nika_kernel::{BlobError, BlobMetadata};

use crate::{DEFAULT_MIME, DiskBlobStore, HASH_PREFIX};

/// Content-addressed blobs below a relative path in an injected filesystem.
///
/// Production callers must inject the descriptor-anchored filesystem owner
/// with atomic publication, such as `nika_fs::RootedFs`. The kernel traits do
/// not themselves promise containment. This adapter neither canonicalizes an
/// ambient path nor falls back to an ambient filesystem after a refusal.
/// The caller owns the filesystem's aggregate budget and seal/drain lifecycle.
#[derive(Debug)]
#[non_exhaustive]
pub struct FsBlobStore<F> {
    fs: Arc<F>,
    root: PathBuf,
    max_size: u64,
}

impl<F> FsBlobStore<F> {
    /// Construct without I/O. `root` must consist only of relative, normal
    /// components; directories are created by the injected owner on `put`.
    ///
    /// # Errors
    /// Returns [`BlobError::Io`] for an empty, absolute or traversing root.
    pub fn new(fs: Arc<F>, root: impl Into<PathBuf>, max_size: u64) -> Result<Self, BlobError> {
        let root = root.into();
        if root.as_os_str().is_empty()
            || !root
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
        {
            return Err(BlobError::Io {
                reason: "blob root must be a non-empty relative path without traversal".into(),
            });
        }
        Ok(Self { fs, root, max_size })
    }

    /// Resolve only a validated content hash, never a provider-reported path.
    fn paths(&self, raw: &str) -> (PathBuf, PathBuf, PathBuf) {
        let shard = self.root.join(&raw[..2]);
        let blob = shard.join(&raw[2..]);
        let mime = shard.join(format!("{}.mime", &raw[2..]));
        (shard, blob, mime)
    }
}

/// Validate the CAS key before any filesystem operation.
fn raw_hash(hash: &str) -> Result<String, BlobError> {
    DiskBlobStore::canonical_raw(hash).ok_or_else(|| BlobError::NotFound { hash: hash.into() })
}

/// Keep absence distinct from a refused or failed filesystem operation.
fn fs_error(error: &FsError, hash: &str, operation: &str) -> BlobError {
    if matches!(error, FsError::NotFound { .. }) {
        BlobError::NotFound { hash: hash.into() }
    } else {
        BlobError::Io {
            reason: format!("failed to {operation} blob: {error}"),
        }
    }
}

impl<F: FsReadDyn + FsWriteDyn + FsMetaDyn> BlobStoreDyn for FsBlobStore<F> {
    /// Publish by an exclusive owner operation, verifying existing bytes on a
    /// dedup race. The MIME sidecar is atomically replaced by the owner.
    ///
    /// CANCEL SAFETY: the anchored atomic owner may finish a dropped write;
    /// its ledger must be drained by the caller. Existing bytes are never
    /// opened for an in-place write or replaced during a dedup race.
    async fn put(&self, data: Bytes, mime_type: &str) -> Result<BlobMetadata, BlobError> {
        if data.is_empty() || mime_type.trim().is_empty() {
            return Err(BlobError::Io {
                reason: "blob bytes and MIME must not be empty".into(),
            });
        }
        let size = data.len() as u64;
        if size > self.max_size {
            return Err(BlobError::TooLarge {
                size,
                max: self.max_size,
            });
        }
        let raw = blake3::hash(&data).to_hex().to_string();
        let (shard, blob, mime) = self.paths(&raw);
        self.fs
            .create_dir_all(&shard)
            .await
            .map_err(|e| fs_error(&e, &raw, "create shard for"))?;
        match self.fs.write_new(&blob, &data).await {
            Ok(()) => {}
            Err(FsError::AlreadyExists { .. }) => {
                let existing = self
                    .fs
                    .read_pinned(&blob)
                    .await
                    .map_err(|e| fs_error(&e, &raw, "verify"))?;
                if existing != data {
                    return Err(BlobError::Io {
                        reason: "existing blob bytes do not match their content hash".into(),
                    });
                }
            }
            Err(error) => return Err(fs_error(&error, &raw, "publish")),
        }
        self.fs
            .write(&mime, mime_type.as_bytes())
            .await
            .map_err(|e| fs_error(&e, &raw, "write MIME for"))?;
        Ok(BlobMetadata::new(
            format!("{HASH_PREFIX}{raw}"),
            mime_type,
            size,
        ))
    }

    /// Read through the owner's pin; unsupported pins refuse explicitly.
    ///
    /// CANCEL SAFETY: read-only.
    async fn get(&self, hash: &str) -> Result<Bytes, BlobError> {
        let raw = raw_hash(hash)?;
        self.fs
            .read_pinned(&self.paths(&raw).1)
            .await
            .map_err(|e| fs_error(&e, hash, "read"))
    }

    /// Malformed keys, non-files and refused paths report absence.
    ///
    /// CANCEL SAFETY: read-only.
    async fn exists(&self, hash: &str) -> bool {
        let Ok(raw) = raw_hash(hash) else {
            return false;
        };
        self.fs
            .metadata(&self.paths(&raw).1)
            .await
            .is_ok_and(|meta| meta.is_file)
    }

    /// Size from a regular file, MIME from its pinned sidecar. Only missing or
    /// blank MIME falls back to the default, never a refused path.
    ///
    /// CANCEL SAFETY: read-only.
    async fn stat(&self, hash: &str) -> Result<BlobMetadata, BlobError> {
        let raw = raw_hash(hash)?;
        let (_, blob, sidecar) = self.paths(&raw);
        let metadata = self
            .fs
            .metadata(&blob)
            .await
            .map_err(|e| fs_error(&e, hash, "stat"))?;
        if !metadata.is_file {
            return Err(BlobError::Io {
                reason: "blob is not a regular file".into(),
            });
        }
        let mime = match self.fs.read_pinned(&sidecar).await {
            Ok(bytes) => String::from_utf8(bytes.to_vec()).map_err(|error| BlobError::Io {
                reason: format!("blob MIME is not UTF-8: {error}"),
            })?,
            Err(FsError::NotFound { .. }) => String::new(),
            Err(error) => return Err(fs_error(&error, hash, "read MIME for")),
        };
        let mime = if mime.trim().is_empty() {
            DEFAULT_MIME
        } else {
            &mime
        };
        Ok(BlobMetadata::new(
            format!("{HASH_PREFIX}{raw}"),
            mime,
            metadata.len,
        ))
    }

    /// Remove the regular blob, then its optional sidecar. Missing blobs still
    /// return `NotFound`; final symlinks are refused by the owner's operation.
    ///
    /// CANCEL SAFETY: each unlink is atomic; the owner drains registered work.
    async fn delete(&self, hash: &str) -> Result<(), BlobError> {
        let raw = raw_hash(hash)?;
        let (_, blob, mime) = self.paths(&raw);
        self.fs
            .remove_regular_file(&blob)
            .await
            .map_err(|e| fs_error(&e, hash, "delete"))?;
        let _ = self.fs.remove_regular_file(&mime).await;
        Ok(())
    }
}
