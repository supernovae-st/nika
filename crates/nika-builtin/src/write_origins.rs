// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a run created and what it changed. The first time a run writes a
//! path through `nika:write` or `nika:edit`, the ledger records whether a
//! file already stood there and, when one did, copies its original bytes
//! aside before the write replaces them. A failed run then moves only the
//! files it created into quarantine: a person's own file stays where they
//! keep it, and its original stays available to restore.
//!
//! The ledger is a record, never a gate: a copy that cannot be made is
//! stated, and the write proceeds as it always did. The copy reads through
//! the backend's pin (the final component is never a followed symlink),
//! after the `permits.fs` gate has judged the path.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use nika_kernel::io::fs::{FsMetaDyn, FsReadDyn, FsWriteDyn};

/// The largest original copied aside, in bytes. Past it the original is
/// not kept and the record says so: the copy holds the whole file in
/// memory once, and a workflow overwriting a large export should not pay
/// for a safety copy it never asked for.
pub const ORIGINAL_COPY_MAX: u64 = 64 * 1024 * 1024;

/// What a run's first write of a path found there.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum WriteOrigin {
    /// Nothing stood at the path: the run created this file.
    Created,
    /// A file stood at the path before the run first wrote it.
    Existed {
        /// Where its original bytes were copied, or why they were not.
        original: Result<PathBuf, String>,
    },
}

/// One run's write origins, shared by its builtin plane and its host.
#[derive(Debug)]
pub struct WriteOrigins {
    /// Where originals are copied, as `<n>-<file name>`, resolved like the
    /// paths the run writes.
    staging: PathBuf,
    /// The first sighting of every path the run wrote.
    seen: Mutex<BTreeMap<String, WriteOrigin>>,
    /// The next copy's number (unique even when two writes race).
    next: AtomicUsize,
}

impl WriteOrigins {
    /// An empty ledger that copies originals under `staging`.
    #[must_use]
    pub fn new(staging: PathBuf) -> Self {
        Self {
            staging,
            seen: Mutex::new(BTreeMap::new()),
            next: AtomicUsize::new(1),
        }
    }

    /// The directory originals are copied under.
    #[must_use]
    pub fn staging(&self) -> &Path {
        &self.staging
    }

    /// The origin of `path` as this run first wrote it, `None` when the run
    /// never wrote it through `nika:write` or `nika:edit`.
    #[must_use]
    pub fn origin(&self, path: &str) -> Option<WriteOrigin> {
        self.lock().get(path).cloned()
    }

    fn lock(&self) -> MutexGuard<'_, BTreeMap<String, WriteOrigin>> {
        self.seen.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Record `path` before the run writes it. The first sighting decides:
    /// a later write of the same path, such as an edit after the write that
    /// created it, changes nothing.
    pub(crate) async fn note<F>(&self, fs: &F, path: &str)
    where
        F: FsReadDyn + FsWriteDyn + FsMetaDyn,
    {
        if self.lock().contains_key(path) {
            return;
        }
        let origin = if fs.exists(Path::new(path)).await {
            WriteOrigin::Existed {
                original: self.copy_original(fs, path).await,
            }
        } else {
            WriteOrigin::Created
        };
        self.lock().entry(path.to_owned()).or_insert(origin);
    }

    /// Copy the original bytes of `path` under the staging directory.
    async fn copy_original<F>(&self, fs: &F, path: &str) -> Result<PathBuf, String>
    where
        F: FsReadDyn + FsWriteDyn + FsMetaDyn,
    {
        let source = Path::new(path);
        let meta = fs.metadata(source).await.map_err(|e| e.to_string())?;
        if !meta.is_file {
            return Err("not a regular file".to_owned());
        }
        if meta.len > ORIGINAL_COPY_MAX {
            return Err(format!(
                "{} bytes, over the {ORIGINAL_COPY_MAX}-byte copy bound",
                meta.len
            ));
        }
        let bytes = fs.read_pinned(source).await.map_err(|e| e.to_string())?;
        let name = source
            .file_name()
            .map_or_else(|| "file".to_owned(), |n| n.to_string_lossy().into_owned());
        let number = self.next.fetch_add(1, Ordering::Relaxed);
        let copy = self.staging.join(format!("{number}-{name}"));
        fs.write(&copy, &bytes).await.map_err(|e| e.to_string())?;
        Ok(copy)
    }
}

#[cfg(test)]
mod tests;
