// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A crashed run's leftovers, settled by the next run. A run killed before
//! its teardown (SIGKILL, a machine that went down) leaves its staging
//! directory under `.nika/originals/<execution>/`: the copies of the
//! person's files it changed, and one record per path it first wrote
//! (`<n>.json`, written by the write-origins ledger before each write).
//!
//! When the journal's lease proves that run dead, the next run settles its
//! leftovers the way its own teardown would have. A file it created moves
//! under `.nika/quarantine/<its stamp>/` only when its bytes are still
//! exactly the ones it last wrote (the record's digest): a file the person
//! changed since, or one whose last write left no digest, stays in place
//! and the fold says why. A person's file it changed stays where they keep
//! it, and its original moves under `originals/`. The fold lands as that
//! dir's manifest, so `--resume` of the crashed journal puts the moved
//! files back. A staging directory whose run is alive, or whose liveness
//! this host cannot judge, is never touched; one whose teardown had begun
//! is left as it is and said.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::liveness::Liveness;

/// A created file past this size has no digest: the ledger reads back only
/// what it would copy (its 64 MiB original bound), so a larger file cannot
/// be proven unchanged.
const DIGEST_MAX: u64 = 64 * 1024 * 1024;

/// Settle every crashed run's leftovers under the project at `root`: one
/// fold per run settled (or left as its teardown had begun), in staging
/// order, for the caller to say. Each fold names the crashed run's stamp
/// under `crashed`.
#[must_use]
pub fn settle_crashed(root: &Path) -> Vec<serde_json::Value> {
    let Ok(entries) = std::fs::read_dir(root.join(crate::store::ORIGINALS_DIR)) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
        .iter()
        .filter_map(|name| settle_one(root, name))
        .collect()
}

/// The journal of the run whose staging directory is `name` (its execution
/// id), when the store holds one.
fn journal_of(root: &Path, name: &str) -> Option<PathBuf> {
    let nika = nika_fs::OwnedDir::open(&root.join(".nika")).ok()?;
    let file = crate::cost_journal::trace_of(&nika, name)?;
    Some(root.join(crate::store::TRACE_DIR).join(file))
}

/// Settle one staging directory when its run is provably dead.
fn settle_one(root: &Path, name: &str) -> Option<serde_json::Value> {
    let journal = journal_of(root, name)?;
    if !matches!(crate::liveness::probe(&journal), Liveness::Dead { .. }) {
        return None;
    }
    let stamp = journal.file_stem()?.to_string_lossy().into_owned();
    let dir = PathBuf::from(crate::store::QUARANTINE_DIR).join(&stamp);
    let staging = PathBuf::from(crate::store::ORIGINALS_DIR).join(name);
    if root.join(&dir).exists() {
        // Its teardown had begun: what it moved is in its own dir, and the
        // staging stays as it is rather than being half merged.
        return Some(serde_json::json!({
            "crashed": stamp,
            "dir": dir.to_string_lossy(),
            "staging_left": staging.to_string_lossy(),
            "reason": "its teardown had begun",
        }));
    }
    let mut place = super::Place::new(root, &stamp);
    let mut fold = settle_records(&mut place, root, &staging, &dir);
    if let Some(map) = fold.as_object_mut() {
        map.insert("crashed".to_owned(), stamp.clone().into());
    }
    if place.opened() {
        super::record_manifest(&mut place, &dir, &mut fold);
    }
    release_staging(root, &staging, &mut fold);
    Some(fold)
}

/// The moves of one crashed run, as a quarantine fold. A created file moves
/// only while it is the very file the crashed run's last write left (its
/// record names it), held as the live quarantine holds it.
fn settle_records(
    place: &mut super::Place<'_>,
    root: &Path,
    staging: &Path,
    dir: &Path,
) -> serde_json::Value {
    let originals = dir.join("originals");
    let mut outputs = Vec::new();
    let mut kept = Vec::new();
    let mut taken = BTreeSet::new();
    let mut taken_originals = BTreeSet::new();
    for record in read_records(&root.join(staging)) {
        let Some(path) = record["path"].as_str() else {
            continue;
        };
        if !inside(Path::new(path)) {
            kept.push(serde_json::json!({
                "path": path,
                "action": "kept_in_place",
                "reason": "outside the project",
            }));
            continue;
        }
        match record["origin"].as_str() {
            Some("created") => match unchanged(root, path, record["sha256"].as_str()) {
                Ok(true) => {
                    let made = recorded_file(&record);
                    let moved = super::move_aside(place, path, made.as_ref(), &mut taken);
                    outputs.push(super::output_entry(dir, path, moved));
                }
                Ok(false) => {}
                Err(reason) => kept.push(serde_json::json!({
                    "path": path,
                    "action": "kept_in_place",
                    "reason": reason,
                })),
            },
            Some("existed") => {
                let original = match record["original"].as_str() {
                    Some(copy) if is_name(copy) => Ok(staging.join(copy)),
                    Some(_) => Err("its original was recorded under an unusable name".to_owned()),
                    None => Err(record["original_error"]
                        .as_str()
                        .unwrap_or("no original was recorded")
                        .to_owned()),
                };
                let moved = super::keep_original(place, path, original, &mut taken_originals);
                kept.push(super::kept_entry(&originals, path, moved));
            }
            _ => {}
        }
    }
    let mut fold = serde_json::json!({
        "dir": dir.to_string_lossy(),
        "outputs": outputs,
    });
    if let Some(map) = fold.as_object_mut()
        && !kept.is_empty()
    {
        map.insert("kept".to_owned(), serde_json::Value::Array(kept));
    }
    fold
}

/// The staging records (`<n>.json`), in sighting order; an unreadable one
/// is skipped (its file, if any, is left as it is).
fn read_records(staging: &Path) -> Vec<serde_json::Value> {
    let Ok(entries) = std::fs::read_dir(staging) else {
        return Vec::new();
    };
    let mut numbered: Vec<(u64, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let number = name.strip_suffix(".json")?.parse::<u64>().ok()?;
            Some((number, entry.path()))
        })
        .collect();
    numbered.sort();
    numbered
        .into_iter()
        .filter_map(|(_, path)| {
            let bytes = std::fs::read(path).ok()?;
            serde_json::from_slice(&bytes).ok()
        })
        .collect()
}

/// Whether a created file is still exactly what the crashed run last wrote:
/// `Ok(true)` moves it, `Ok(false)` means it is gone (nothing to settle),
/// and `Err` says why it stays.
fn unchanged(root: &Path, path: &str, digest: Option<&str>) -> Result<bool, String> {
    let target = root.join(path);
    let Ok(meta) = std::fs::symlink_metadata(&target) else {
        return Ok(false);
    };
    if !meta.is_file() {
        return Err("it is no longer a regular file".to_owned());
    }
    let Some(digest) = digest else {
        return Err("its last write left no digest to prove it unchanged".to_owned());
    };
    if meta.len() > DIGEST_MAX {
        return Err("you changed it since".to_owned());
    }
    let bytes = std::fs::read(&target).map_err(|e| format!("it could not be read: {e}"))?;
    if crate::seal::sha256_hex(&bytes) == digest {
        Ok(true)
    } else {
        Err("you changed it since".to_owned())
    }
}

/// Which file the crashed run's last write left at a created path, as its
/// record names it (`device`, `inode`, `len`); `None` when the record names
/// none (an older engine wrote it): then nothing moves.
fn recorded_file(record: &serde_json::Value) -> Option<nika_runtime::CreatedFile> {
    let number = |key: &str| record[key].as_u64();
    let identity = nika_kernel::fs::FileIdentity::new(number("device")?, number("inode")?, 1);
    Some(nika_runtime::CreatedFile::new(identity, number("len")?))
}

/// A relative path that stays inside the project (no root, no `..`).
fn inside(path: &Path) -> bool {
    path.is_relative()
        && path.components().all(|component| {
            matches!(
                component,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        })
}

/// A single plain file name, as the ledger names its copies.
fn is_name(name: &str) -> bool {
    let path = Path::new(name);
    path.file_name()
        .is_some_and(|file| file == path.as_os_str())
}

/// Remove the staging directory once only records remain in it; a copy
/// left there (a move that failed, an unrecorded copy) keeps the directory,
/// and the fold names it.
fn release_staging(root: &Path, staging: &Path, fold: &mut serde_json::Value) {
    let at = root.join(staging);
    let leftover = std::fs::read_dir(&at).is_ok_and(|entries| {
        entries.filter_map(Result::ok).any(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.strip_suffix(".json")
                .is_none_or(|number| number.parse::<u64>().is_err())
        })
    });
    if leftover {
        if let Some(map) = fold.as_object_mut() {
            map.insert("staging_left".to_owned(), staging.to_string_lossy().into());
        }
    } else {
        let _ = std::fs::remove_dir_all(&at);
    }
}

#[cfg(test)]
mod tests;
