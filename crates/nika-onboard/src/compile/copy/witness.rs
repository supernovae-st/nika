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
use std::path::{Component, Path, PathBuf};

use nika_compile_cognition::rehearse::Digest;
use nika_fs::{EffectLedger, OwnedDir, RoomLimits, RootedFs};
use nika_kernel::fs::{FsError, FsMetaDyn as _, FsReadDyn as _};

use crate::compile::room::ObservedRoom;

mod world;
pub use world::WorldBefore;

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
    /// The world paths the request writes and never reads: the only ones a run may advance.
    destinations: Vec<String>,
}

impl Witness {
    /// The witness of `candidate_sha256` over `world`, no path of it a destination.
    pub(super) fn new(candidate_sha256: String, world: Vec<(String, Seen)>) -> Self {
        Self {
            candidate_sha256,
            world,
            destinations: Vec::new(),
        }
    }

    /// This witness with `targets` as its destinations: each a world path, never one the request
    /// also reads (`sources`).
    pub(super) fn writing(mut self, targets: &[String], sources: &[String]) -> Self {
        self.destinations = targets
            .iter()
            .filter(|target| !sources.iter().any(|source| same_path(source, target)))
            .filter(|target| self.world.iter().any(|(path, _)| same_path(path, target)))
            .cloned()
            .collect();
        self
    }

    /// This witness after a settled successful run of its own candidate whose completed writes
    /// were `written`: each of those paths, which must all be destinations, is observed again and
    /// bound to the file it holds now. Sources and every other path stay bound as they were, so a
    /// source edit, a foreign write after this observation, or anything else is still drift.
    ///
    /// The observation follows the run's settlement: a write by another process between the
    /// run's end and this read is bound as if the run had made it. That window is the time the
    /// host takes to report the settled run, and nothing here narrows it further.
    ///
    /// # Errors
    /// Refuses when nothing was written, a written path is a source or no destination of this
    /// witness, or a destination now holds no regular file within the copy bound; the caller then
    /// keeps this witness unchanged.
    pub fn advanced(&self, root: &Path, written: &[String]) -> Result<Self, String> {
        if written.is_empty() {
            return Err("the run completed no write".to_owned());
        }
        if let Some(path) = written
            .iter()
            .find(|path| !self.destinations.iter().any(|it| same_path(it, path)))
        {
            return Err(format!("`{path}` is no destination the rehearsal bound"));
        }
        let now = match on_worker(|| observe(root, written)) {
            Some(Ok(now)) => now,
            Some(Err(why)) => return Err(format!("the project cannot be observed again: {why}")),
            None => return Err("the project cannot be observed again".to_owned()),
        };
        let mut next = self.clone();
        for (path, seen) in written.iter().zip(now) {
            let digest = match seen {
                Ok(Seen::File(digest)) => digest,
                Ok(Seen::Absent) => return Err(format!("`{path}` is absent after the run")),
                Err(why) => return Err(format!("`{path}` cannot be observed: it {why}")),
            };
            let Some(entry) = next.world.iter_mut().find(|(at, _)| same_path(at, path)) else {
                return Err(format!("`{path}` is no path of the rehearsed world"));
            };
            entry.1 = Seen::File(digest);
        }
        Ok(next)
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

/// Whether two request paths name the same path inside the project.
fn same_path(left: &str, right: &str) -> bool {
    relative(left).is_some_and(|at| relative(right) == Some(at))
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

/// Compare saved-witness paths lexically under the project root: remove its prefix and `.`.
/// This performs no I/O, resolves no symlinks and grants no access to either path.
#[must_use]
pub fn same_project_path(root: &Path, left: &Path, right: &Path) -> bool {
    let normal = |path: &Path| -> PathBuf {
        let path = path.strip_prefix(root).unwrap_or(path);
        path.components()
            .filter(|part| !matches!(part, Component::CurDir))
            .collect()
    };
    normal(left) == normal(right)
}
