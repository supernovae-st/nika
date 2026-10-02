// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a selection is bound to: the candidate's exact bytes, and the user's world as the room
//! copied it, judged again before the yes and before a run. A path is observed read-only,
//! bounded and never through a symlink, through a rooted handle on the project whose ledger is
//! past every writing phase, each read capped at the room's copy bound.
//!
//! What an observation finds at a path:
//! - a regular file is its digest;
//! - nothing at the path is an absence;
//! - anything else (a symlink, a special file, a directory, an error, a file past the bound) is no
//!   observation at all, never an absence.

use std::future::Future;
use std::path::{Path, PathBuf};

use nika_compile_cognition::rehearse::Digest;
use nika_fs::{EffectLedger, OwnedDir, RoomLimits, RootedFs};
use nika_kernel::fs::{FsError, FsMetaDyn as _, FsReadDyn as _};

use crate::compile::room::ObservedRoom;

/// What one path held when it was observed.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Seen {
    /// A regular file: its length and sha256.
    File(Digest),
    /// Nothing at the path. A symlink or a failed read is never an absence.
    Absent,
}

/// The candidate's exact bytes and the user's world a selection was made on, kept beside the
/// proposal: each path as the request names it, and what it held when the room copied it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Witness {
    candidate_sha256: String,
    world: Vec<(String, Seen)>,
}

impl Witness {
    /// The witness of `candidate_sha256` over `world`.
    pub(super) fn new(candidate_sha256: String, world: Vec<(String, Seen)>) -> Self {
        Self {
            candidate_sha256,
            world,
        }
    }

    /// The sha256 of the selected candidate's bytes (lowercase hex).
    #[must_use]
    pub fn candidate_sha256(&self) -> &str {
        &self.candidate_sha256
    }

    /// Each path of the user's world and what it held when the room copied it, in order.
    #[must_use]
    pub fn world(&self) -> &[(String, Seen)] {
        &self.world
    }

    /// Whether the project under `root` still holds the world the selection was made on: every
    /// file the same bytes, every absence still an absence and, with `saved`, that workflow file
    /// the candidate's own bytes. `None` when it does; else what moved, in words.
    #[must_use]
    pub fn drift(&self, root: &Path, saved: Option<&Path>) -> Option<String> {
        let saved = saved.map(|path| path.display().to_string());
        let mut paths: Vec<String> = self.world.iter().map(|(path, _)| path.clone()).collect();
        paths.extend(saved.clone());
        let now = match on_worker(|| observe(root, &paths)) {
            Some(Ok(now)) => now,
            Some(Err(why)) => return Some(format!("the project cannot be observed again: {why}")),
            None => return Some("the project cannot be observed again".to_owned()),
        };
        let mut moved: Vec<String> = self
            .world
            .iter()
            .zip(&now)
            .filter_map(|((path, then), seen)| changed(path, then, seen))
            .collect();
        if let (Some(path), Some(seen)) = (saved, now.get(self.world.len())) {
            match seen {
                Ok(Seen::File(digest)) if digest.sha256 == self.candidate_sha256 => {}
                Ok(_) => moved.push(format!(
                    "the saved workflow `{path}` is not the rehearsed one"
                )),
                Err(why) => moved.push(format!(
                    "the saved workflow `{path}` cannot be observed: it {why}"
                )),
            }
        }
        (!moved.is_empty()).then(|| moved.join(" · "))
    }
}

/// What moved at `path` since it held `then`, in words; `None` when nothing did.
fn changed(path: &str, then: &Seen, now: &Result<Seen, String>) -> Option<String> {
    match (then, now) {
        (_, Err(why)) => Some(format!("`{path}` cannot be observed: it {why}")),
        (Seen::File(was), Ok(Seen::File(is))) if was == is => None,
        (Seen::Absent, Ok(Seen::Absent)) => None,
        (Seen::File(was), Ok(Seen::File(is))) => Some(format!(
            "`{path}` changed ({} B, sha256 {} when rehearsed; {} B, sha256 {} now)",
            was.bytes,
            short(&was.sha256),
            is.bytes,
            short(&is.sha256)
        )),
        (Seen::File(_), Ok(Seen::Absent)) => Some(format!("`{path}` is gone")),
        (Seen::Absent, Ok(Seen::File(_))) => Some(format!("`{path}` appeared")),
    }
}

/// The first twelve characters of a digest.
fn short(sha256: &str) -> &str {
    sha256.get(..12).unwrap_or(sha256)
}

/// Each path observed now under `root`: what it holds, or why it cannot be observed. The project
/// is held through a rooted handle whose ledger is past every writing phase, so nothing here can
/// write; each read is capped at the room's copy bound and never follows a symlink.
pub(super) async fn observe(
    root: &Path,
    paths: &[String],
) -> Result<Vec<Result<Seen, String>>, String> {
    let held = OwnedDir::open(root).map_err(|error| error.to_string())?;
    let ledger = EffectLedger::new(RoomLimits::new(ObservedRoom::COPY_BOUND, 0));
    for _ in 0..2 {
        let _drained = ledger.seal_and_drain().await;
        ledger.advance().map_err(|refusal| refusal.to_string())?;
    }
    let project = RootedFs::new(held, ledger);
    let mut seen = Vec::with_capacity(paths.len());
    for path in paths {
        seen.push(match relative(path) {
            Some(at) => observed(&project, &at).await,
            None => Err("is not a path inside the project".to_owned()),
        });
    }
    Ok(seen)
}

/// What the project holds at `at`.
async fn observed(project: &RootedFs, at: &Path) -> Result<Seen, String> {
    match project.metadata(at).await {
        Err(FsError::NotFound { .. }) => Ok(Seen::Absent),
        Err(error) => Err(unreadable(&error)),
        Ok(found) if !found.is_file => Err("is not a regular file".to_owned()),
        Ok(_) => project
            .read(at)
            .await
            .map(|bytes| Seen::File(Digest::of(&bytes)))
            .map_err(|error| unreadable(&error)),
    }
}

/// Why a path cannot be observed, in the words of what it is.
fn unreadable(error: &FsError) -> String {
    match error {
        FsError::SymlinkRefused { .. } => {
            "goes through a symlink, which is never followed".to_owned()
        }
        FsError::InvalidData { .. } => {
            "is not a regular file read whole within the copy bound".to_owned()
        }
        other => format!("could not be read: {other}"),
    }
}

/// `path` as a path relative to the project: `.` and empty components dropped; an absolute path,
/// a `..` or no name at all is none.
pub(super) fn relative(path: &str) -> Option<PathBuf> {
    if path.starts_with('/') {
        return None;
    }
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => return None,
            name => parts.push(name),
        }
    }
    (!parts.is_empty()).then(|| parts.iter().collect())
}

/// Run the future `make` gives to completion on a thread and an executor of its own, never
/// inside a caller's runtime: `None` when the executor could not start or the thread panicked.
pub(super) fn on_worker<T, F>(make: impl FnOnce() -> F + Send) -> Option<T>
where
    T: Send,
    F: Future<Output = T>,
{
    std::thread::scope(|scope| {
        scope
            .spawn(move || {
                let executor = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .ok()?;
                Some(executor.block_on(make()))
            })
            .join()
            .ok()
            .flatten()
    })
}
