// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a run created and what it changed. The first time a run writes a
//! path through `nika:write` or `nika:edit`, the ledger records whether a
//! file already stood there and, when one did, copies its original bytes
//! aside before the write replaces them. A failed run then moves only the
//! files it created into quarantine: a person's own file stays where they
//! keep it, and its original stays available to restore.
//!
//! Each first sighting also lands as a small record beside the copies
//! (`<n>.json`: the path, created or existed, the copy's name), written
//! before the run's write proceeds; a created file's record gains the
//! sha256 of its bytes once a write of it landed. A run killed before its
//! teardown leaves those records behind: a later run reads them to settle
//! the dead run's files the way a failed run's are, and the digest tells
//! the run's bytes from a person's later edit.
//!
//! The ledger is a record, never a gate: a copy or a record that cannot be
//! made is stated or skipped, and the write proceeds as it always did. The
//! copy reads through the backend's pin (the final component is never a
//! followed symlink), after the `permits.fs` gate has judged the path.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use nika_kernel::io::fs::{FsMetaDyn, FsReadDyn, FsWriteDyn};

/// The largest original copied aside, in bytes. Past it the original is
/// not kept and the record says so: the copy holds the whole file in
/// memory once, and a workflow overwriting a large export should not pay
/// for a safety copy it never asked for. A created file past it is not
/// read back for its digest either.
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
    /// Where originals are copied, as `<n>-<file name>`, and each first
    /// sighting's record as `<n>.json`, resolved like the paths the run
    /// writes.
    staging: PathBuf,
    /// The first sighting of every path the run wrote, with its number.
    seen: Mutex<BTreeMap<String, (WriteOrigin, usize)>>,
    /// The next sighting's number (unique even when two writes race).
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
        self.lock().get(path).map(|(origin, _)| origin.clone())
    }

    fn lock(&self) -> MutexGuard<'_, BTreeMap<String, (WriteOrigin, usize)>> {
        self.seen.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Count `path` as a file this run created although it did not write
    /// it: a resumed run put it back from its failed prior run's
    /// quarantine, so it is this run's to quarantine again if this run
    /// fails too. A path already seen keeps its first origin. No record is
    /// staged for it: a crash of this run leaves it where it stands.
    pub fn adopt(&self, path: &str) {
        let number = self.next.fetch_add(1, Ordering::Relaxed);
        self.lock()
            .entry(path.to_owned())
            .or_insert((WriteOrigin::Created, number));
    }

    /// Record `path` before the run writes it. The first sighting decides:
    /// a later write of the same path, such as an edit after the write that
    /// created it, changes nothing. Its record lands before the write.
    pub(crate) async fn note<F>(&self, fs: &F, path: &str)
    where
        F: FsReadDyn + FsWriteDyn + FsMetaDyn,
    {
        if self.lock().contains_key(path) {
            return;
        }
        let number = self.next.fetch_add(1, Ordering::Relaxed);
        let origin = if fs.exists(Path::new(path)).await {
            WriteOrigin::Existed {
                original: self.copy_original(fs, path, number).await,
            }
        } else {
            WriteOrigin::Created
        };
        let first = {
            let mut seen = self.lock();
            if seen.contains_key(path) {
                false
            } else {
                seen.insert(path.to_owned(), (origin.clone(), number));
                true
            }
        };
        if first {
            self.write_record(fs, path, number, &origin, None).await;
        }
    }

    /// After a write of `path` landed: a file the run created gains the
    /// sha256 of the bytes now there in its record, so a later run can tell
    /// this run's bytes from a person's edit. A person's own file is never
    /// moved and needs none; a file past the copy bound is not read back,
    /// and its record proves nothing (a later run then leaves it in place).
    pub(crate) async fn wrote<F>(&self, fs: &F, path: &str)
    where
        F: FsReadDyn + FsWriteDyn + FsMetaDyn,
    {
        let sighting = self.lock().get(path).cloned();
        let Some((WriteOrigin::Created, number)) = sighting else {
            return;
        };
        let source = Path::new(path);
        let Ok(meta) = fs.metadata(source).await else {
            return;
        };
        if !meta.is_file || meta.len > ORIGINAL_COPY_MAX {
            return;
        }
        let Ok(bytes) = fs.read_pinned(source).await else {
            return;
        };
        let digest = crate::image::save::sha256_hex(&bytes);
        self.write_record(fs, path, number, &WriteOrigin::Created, Some(&digest))
            .await;
    }

    /// Copy the original bytes of `path` under the staging directory, as
    /// sighting `number`'s copy.
    async fn copy_original<F>(&self, fs: &F, path: &str, number: usize) -> Result<PathBuf, String>
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
        let copy = self.staging.join(format!("{number}-{name}"));
        fs.write(&copy, &bytes).await.map_err(|e| e.to_string())?;
        Ok(copy)
    }

    /// Write sighting `number`'s record (`<n>.json`): the path, `created`
    /// or `existed`, the copy's file name or why there is none, and a
    /// created file's digest once known. Best effort, like the copy: a
    /// record that cannot be written leaves a later run less to settle,
    /// never a wrong move.
    async fn write_record<F>(
        &self,
        fs: &F,
        path: &str,
        number: usize,
        origin: &WriteOrigin,
        sha256: Option<&str>,
    ) where
        F: FsWriteDyn,
    {
        let mut record = serde_json::Map::new();
        record.insert("path".to_owned(), path.into());
        match origin {
            WriteOrigin::Created => {
                record.insert("origin".to_owned(), "created".into());
            }
            WriteOrigin::Existed { original } => {
                record.insert("origin".to_owned(), "existed".into());
                match original {
                    Ok(copy) => {
                        let name = copy.file_name().map(|n| n.to_string_lossy().into_owned());
                        record.insert("original".to_owned(), name.into());
                    }
                    Err(reason) => {
                        record.insert("original_error".to_owned(), reason.as_str().into());
                    }
                }
            }
        }
        if let Some(digest) = sha256 {
            record.insert("sha256".to_owned(), digest.into());
        }
        let Ok(bytes) = serde_json::to_vec(&serde_json::Value::Object(record)) else {
            return;
        };
        let _ = fs
            .write(&self.staging.join(format!("{number}.json")), &bytes)
            .await;
    }
}

#[cfg(test)]
mod tests;
