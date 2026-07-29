// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `.nika/memory/<store>/` layout — one JSON file per entry
//! (`<ts>-<digest-prefix>.json`), committed atomic (temp + rename, the
//! nika-fs pattern: a reader never sees a torn write). The store name is
//! a SINGLE path segment, always — a name with a separator or `..` is a
//! layout error, never an escape out of the memory root.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use crate::RejectReason;
use crate::entry::StoreEntry;
use crate::errors::StoreError;

/// The memory root relative to a run's CWD (`.nika/memory/<store>/` below it).
pub const MEMORY_ROOT: &str = ".nika/memory";

/// The store dir under a memory root. The store name MUST be one path
/// segment: empty · a `/` or `\` separator · `..` is a
/// [`StoreError::DirLayout`] — a store name can never escape the memory
/// root.
///
/// # Errors
///
/// [`StoreError::DirLayout`] when the name is not a single segment.
pub fn store_dir(memory_root: &Path, store: &str) -> Result<PathBuf, StoreError> {
    if store.is_empty() || store.contains(['/', '\\']) || store.contains("..") {
        return Err(StoreError::DirLayout {
            reason: format!("store name {store:?} is not a single path segment"),
        });
    }
    Ok(memory_root.join(store))
}

/// The entry file name: zero-padded ts (sortable) + the digest's first 16 hex.
#[must_use]
pub fn entry_file_name(entry: &StoreEntry) -> String {
    let digest = entry.digest();
    format!("{:013}-{}.json", entry.ts_ms, &digest[..16])
}

/// One walked entry file: the path and the decode outcome — `Ok` the
/// envelope, `Err` the NAMED decode failure (`Malformed` ·
/// `UnsupportedVersion`: recall rejects with that reason, never a silent
/// skip).
pub(crate) struct EntryFile {
    pub path: PathBuf,
    pub decoded: Result<StoreEntry, RejectReason>,
}

fn io(what: &str, path: &Path, e: &std::io::Error) -> StoreError {
    StoreError::Io {
        reason: format!("{what} {}: {e}", path.display()),
    }
}

/// Commit an entry: create the store dir, write a temp sibling, rename over
/// the final name (same content ⇒ same name ⇒ the write is idempotent).
/// The temp rides `create_new` — a leftover from a crashed write (or a
/// symlink planted at the temp name) FAILS the commit, never a silent
/// clobber / a followed symlink; a failed rename removes the temp
/// best-effort (a stale temp would fail every later `create_new`).
pub(crate) fn commit(dir: &Path, entry: &StoreEntry) -> Result<PathBuf, StoreError> {
    std::fs::create_dir_all(dir).map_err(|e| io("create", dir, &e))?;
    let name = entry_file_name(entry);
    let body =
        serde_json::to_string_pretty(&entry.file_value()).map_err(|e| StoreError::Serialize {
            reason: format!("envelope {name}: {e}"),
        })?;
    let tmp = dir.join(format!(".{name}.tmp"));
    {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .map_err(|e| io("write", &tmp, &e))?;
        file.write_all(body.as_bytes())
            .map_err(|e| io("write", &tmp, &e))?;
    }
    let path = dir.join(&name);
    if let Err(e) = std::fs::rename(&tmp, &path) {
        let _ = std::fs::remove_file(&tmp); // best-effort — the error to name is the rename's
        return Err(io("rename", &path, &e));
    }
    Ok(path)
}

/// Walk a store dir's entry files (`.json` only — temp siblings never ride),
/// sorted by path for determinism. A missing dir is an EMPTY store, never an
/// error (absent is honest); a non-dir at the store path is a layout error.
pub(crate) fn walk(dir: &Path) -> Result<Vec<EntryFile>, StoreError> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    if !dir.is_dir() {
        return Err(StoreError::DirLayout {
            reason: format!("{} is not a directory", dir.display()),
        });
    }
    let mut out = Vec::new();
    for item in std::fs::read_dir(dir).map_err(|e| io("read_dir", dir, &e))? {
        let path = item.map_err(|e| io("read_dir", dir, &e))?.path();
        if path.extension().is_none_or(|x| x != "json") {
            continue;
        }
        // Non-UTF-8 / non-JSON bytes are not an IO failure — the file is
        // not a decodable envelope: recall names it Malformed (never silent).
        let decoded = match std::fs::read(&path) {
            Err(e) => return Err(io("read", &path, &e)),
            Ok(bytes) => String::from_utf8(bytes)
                .ok()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
                .map_or(Err(RejectReason::Malformed), |v| {
                    StoreEntry::from_file_value(&v)
                }),
        };
        out.push(EntryFile { path, decoded });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// Remove an entry file (idempotent — a missing file is a no-op, the kernel
/// trait's CANCEL SAFETY contract).
pub(crate) fn remove(path: &Path) -> Result<(), StoreError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io("remove", path, &e)),
    }
}
