// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Received harness images, stored below the admitted project held by
//! descriptor (`.nika/blobs`, content addressed). Every operation gets its own
//! finite room and drains it before it answers, so a reusable runtime never
//! shares a sealed ledger. An operation whose future a cancelled task dropped
//! is sealed at the drop and joined by [`ImageRoom::drain_dropped`]. A join is
//! a pinned future with exactly one owner at a time; abandoning a drain hands
//! its unfinished joins back, so the next drain resumes them. The peer's
//! reported path is never an input here: only received bytes are.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::Poll;

use bytes::Bytes;
use nika_blob::FsBlobStore;
use nika_fs::{Drained, EffectLedger, OwnedDir, RoomLimits, RootedFs};
use nika_kernel::blob::BlobStoreDyn;
use nika_kernel::{BlobError, BlobMetadata};

/// The decoded size one received image may have.
pub const IMAGE_MAX_BYTES: u64 = 6 * 1024 * 1024;
/// One operation's budget: the image, its MIME sidecar and the directories it
/// creates (`.nika`, `blobs`, the shard). Writes are never refunded.
const ROOM: (u64, u64) = (IMAGE_MAX_BYTES + 4096, 8);
/// The CAS root, relative to the held project.
const BLOBS: &str = ".nika/blobs";

/// The join of one sealed room: it owns the seal's operations until they end.
type Join = Pin<Box<dyn Future<Output = Drained> + Send>>;

/// The admitted project as a store for received image bytes.
pub struct ImageRoom {
    project: OwnedDir,
    dropped: Mutex<Vec<Join>>,
}

impl std::fmt::Debug for ImageRoom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageRoom")
            .field("project", &self.project)
            .field("pending_drains", &self.pending_drains())
            .finish_non_exhaustive()
    }
}

impl ImageRoom {
    /// Hold `root` by descriptor, every component walked without following a
    /// symlink. Nothing is created or written until a received image is put.
    ///
    /// # Errors
    /// The root is inaccessible or one of its components is a symlink.
    pub fn open(root: &Path) -> std::io::Result<Self> {
        Ok(Self {
            project: OwnedDir::open(root)?,
            dropped: Mutex::new(Vec::new()),
        })
    }

    /// Join every operation a dropped future left in flight. The joins are
    /// held, never waited on under the lock; if this future is abandoned, the
    /// unfinished ones go back to the room and the next call resumes them.
    pub async fn drain_dropped(&self) {
        let joins =
            std::mem::take(&mut *self.dropped.lock().unwrap_or_else(PoisonError::into_inner));
        Held { room: self, joins }.finish().await;
    }

    /// Sealed rooms of dropped operations not yet joined.
    #[must_use]
    pub fn pending_drains(&self) -> usize {
        self.dropped
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    fn lease(&self) -> Result<Lease<'_>, BlobError> {
        let project = self.project.try_clone().map_err(|e| BlobError::Io {
            reason: format!("cannot hold the image room: {e}"),
        })?;
        let ledger = EffectLedger::new(RoomLimits::new(ROOM.0, ROOM.1));
        let fs = Arc::new(RootedFs::new(project, Arc::clone(&ledger)));
        Ok(Lease {
            room: self,
            store: FsBlobStore::new(fs, BLOBS, IMAGE_MAX_BYTES)?,
            ledger: Some(ledger),
        })
    }
}

/// One operation's room. Closing seals and joins it; dropping it unclosed
/// seals it and hands its drain to the room.
struct Lease<'a> {
    room: &'a ImageRoom,
    store: FsBlobStore<RootedFs>,
    ledger: Option<Arc<EffectLedger>>,
}

impl Lease<'_> {
    /// Every blocking operation already answered the store, so this join does
    /// not wait in practice; it still proves nothing of the room is running.
    async fn close<T>(mut self, answer: T) -> T {
        if let Some(ledger) = self.ledger.take() {
            let joins = vec![Box::pin(ledger.seal().join()) as Join];
            Held {
                room: self.room,
                joins,
            }
            .finish()
            .await;
        }
        answer
    }
}

impl Drop for Lease<'_> {
    /// The seal is synchronous: from the drop on, the room registers nothing.
    fn drop(&mut self) {
        if let Some(ledger) = self.ledger.take() {
            let join = Box::pin(ledger.seal().join()) as Join;
            self.room
                .dropped
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(join);
        }
    }
}

/// Joins being waited on by one caller. Dropped unfinished, it returns them.
struct Held<'a> {
    room: &'a ImageRoom,
    joins: Vec<Join>,
}

impl Held<'_> {
    async fn finish(mut self) {
        std::future::poll_fn(|cx| {
            self.joins
                .retain_mut(|join| join.as_mut().poll(cx).is_pending());
            if self.joins.is_empty() {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        })
        .await;
    }
}

impl Drop for Held<'_> {
    fn drop(&mut self) {
        if !self.joins.is_empty() {
            let mut dropped = self
                .room
                .dropped
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            dropped.append(&mut self.joins);
        }
    }
}

impl BlobStoreDyn for ImageRoom {
    /// CANCEL SAFETY: a dropped put leaves its room sealed and listed for
    /// [`ImageRoom::drain_dropped`]; content addressing makes a late write inert.
    async fn put(&self, data: Bytes, mime_type: &str) -> Result<BlobMetadata, BlobError> {
        let lease = self.lease()?;
        let answer = lease.store.put(data, mime_type).await;
        lease.close(answer).await
    }

    /// CANCEL SAFETY: read-only.
    async fn get(&self, hash: &str) -> Result<Bytes, BlobError> {
        let lease = self.lease()?;
        let answer = lease.store.get(hash).await;
        lease.close(answer).await
    }

    /// CANCEL SAFETY: read-only.
    async fn exists(&self, hash: &str) -> bool {
        let Ok(lease) = self.lease() else {
            return false;
        };
        let answer = lease.store.exists(hash).await;
        lease.close(answer).await
    }

    /// CANCEL SAFETY: read-only.
    async fn stat(&self, hash: &str) -> Result<BlobMetadata, BlobError> {
        let lease = self.lease()?;
        let answer = lease.store.stat(hash).await;
        lease.close(answer).await
    }

    /// CANCEL SAFETY: as [`Self::put`].
    async fn delete(&self, hash: &str) -> Result<(), BlobError> {
        let lease = self.lease()?;
        let answer = lease.store.delete(hash).await;
        lease.close(answer).await
    }
}
