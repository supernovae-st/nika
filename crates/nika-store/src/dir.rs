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

/// The per-entry size bound (NEP-0012 law 1 for the memory lane — a
/// stuffed store must never turn a run's teardown into seconds of verify
/// and a memory blow): an entry file larger than this is rejected
/// [`RejectReason::Oversized`] WITHOUT a full read — a multi-GB planted
/// `.json` never lands in memory.
pub(crate) const MAX_ENTRY_BYTES: usize = 1024 * 1024; // 1 MiB

/// The per-store entry-count bound: a store holding more `.json` entry
/// files walks the FIRST [`MAX_WALK_ENTRIES`] (path-sorted) normally and
/// names every file beyond the cap [`RejectReason::Overflow`] — UNREAD and
/// unexamined, counted and journaled like every rejection, never silently
/// dropped. 10^4 planted junk files then cost the teardown a bounded read
/// set plus one dirent name each. Generous for v1's honest traffic (a run
/// writes tens of entries); amend the number when the L2 orchestrator's
/// real shape lands.
pub(crate) const MAX_WALK_ENTRIES: usize = 1024;

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

/// Create the store dir chain (0700 for the dirs WE create on unix —
/// entry frames may carry secrets, the house convention for
/// secret-bearing paths; a pre-existing dir keeps its mode: the engine
/// never tightens what it did not create).
fn create_store_dir(dir: &Path) -> Result<(), StoreError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .map_err(|e| io("create", dir, &e))
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(dir).map_err(|e| io("create", dir, &e))
    }
}

/// Commit an entry: create the store dir, write a temp sibling (0600 on
/// unix — set BEFORE the rename so the landed file is born tight, the
/// `write_0600` convention), rename over the final name. The rename is
/// IDEMPOTENT BY CONTENT, the claim made true by reading first — where
/// the content that counts is the PREIMAGE (the entry's identity, what
/// the digest and therefore the file NAME commit to), never the raw
/// bytes: minisign randomizes the sig box (a nonce + a `timestamp:`
/// trusted comment), so two honest signings of ONE entry differ in bytes
/// while being the same attestation. A final name holding the same
/// preimage is a no-op Ok; a name holding a divergent — or undecodable —
/// envelope fails with a named [`StoreError::Conflict`]: an out-of-engine
/// plant at the canonical name is tamper evidence, never a silent
/// clobber.
/// The temp rides `create_new` — a leftover from a crashed write (or a
/// symlink planted at the temp name) FAILS the commit, never a silent
/// clobber / a followed symlink; a failed rename removes the temp
/// best-effort (a stale temp would fail every later `create_new`).
///
/// Durability, named: nothing fsyncs — the guarantee is rename-level
/// ATOMICITY (a reader never sees a torn write), not survival of a crash:
/// a crash post-rename can lose the entry. Loss is not forgery — the fold
/// attests what is present, and an absent entry is honest.
pub(crate) fn commit(dir: &Path, entry: &StoreEntry) -> Result<PathBuf, StoreError> {
    create_store_dir(dir)?;
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
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))
                .map_err(|e| io("chmod", &tmp, &e))?;
        }
        file.write_all(body.as_bytes())
            .map_err(|e| io("write", &tmp, &e))?;
    }
    let path = dir.join(&name);
    if path.exists() {
        // The read the idempotence claim needs: same PREIMAGE (the entry's
        // identity — the sig box is randomized, so bytes never compare) is
        // a no-op; divergent or undecodable bytes are a NAMED conflict (a
        // planted dir reads as the io error it is).
        let held = std::fs::read(&path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            io("read", &path, &e)
        })?;
        let _ = std::fs::remove_file(&tmp); // the name is taken either way — the temp never lingers
        let same_entry = serde_json::from_slice::<serde_json::Value>(&held)
            .ok()
            .and_then(|v| StoreEntry::from_file_value(&v).ok())
            .is_some_and(|existing| existing.preimage() == entry.preimage());
        return if same_entry {
            Ok(path)
        } else {
            Err(StoreError::Conflict {
                reason: format!(
                    "{} already holds a DIFFERENT entry — an out-of-engine plant at the canonical name is tamper evidence, never silently clobbered",
                    path.display()
                ),
            })
        };
    }
    if let Err(e) = std::fs::rename(&tmp, &path) {
        let _ = std::fs::remove_file(&tmp); // best-effort — the error to name is the rename's
        return Err(io("rename", &path, &e));
    }
    Ok(path)
}

/// Read + decode one entry file within the size bound: a CAPPED read
/// ([`MAX_ENTRY_BYTES`] + 1 — the planted multi-GB file never lands in
/// memory), over the cap named [`RejectReason::Oversized`]. Non-UTF-8 /
/// non-JSON bytes are not an IO failure — the file is not a decodable
/// envelope: recall names it Malformed (never silent). A real read error
/// aborts the walk ([`StoreError::Io`]) — the fold names the store's error.
fn read_entry(path: &Path) -> Result<Result<StoreEntry, RejectReason>, StoreError> {
    use std::io::Read as _;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|f| f.take(MAX_ENTRY_BYTES as u64 + 1).read_to_end(&mut bytes))
        .map_err(|e| io("read", path, &e))?;
    if bytes.len() > MAX_ENTRY_BYTES {
        return Ok(Err(RejectReason::Oversized));
    }
    Ok(String::from_utf8(bytes)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .map_or(Err(RejectReason::Malformed), |v| {
            StoreEntry::from_file_value(&v)
        }))
}

/// Walk a store dir's entry files (`.json` only — temp siblings never ride),
/// sorted by path for determinism. A missing dir is an EMPTY store, never an
/// error (absent is honest); a non-dir at the store path is a layout error.
///
/// The bounds (NEP-0012 law 1 for the memory lane — a stuffed store must
/// never turn a run's teardown into seconds of verify + a memory blow): an
/// entry file over [`MAX_ENTRY_BYTES`] rides as [`RejectReason::Oversized`]
/// (capped read — never fully loaded), and every `.json` past the first
/// [`MAX_WALK_ENTRIES`] (path-sorted) rides UNREAD as
/// [`RejectReason::Overflow`] — the overflow is NAMED per file (path +
/// reason) and counted in the fold's `rejected`, never silently dropped.
pub(crate) fn walk(dir: &Path) -> Result<Vec<EntryFile>, StoreError> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    if !dir.is_dir() {
        return Err(StoreError::DirLayout {
            reason: format!("{} is not a directory", dir.display()),
        });
    }
    let mut paths = Vec::new();
    for item in std::fs::read_dir(dir).map_err(|e| io("read_dir", dir, &e))? {
        let path = item.map_err(|e| io("read_dir", dir, &e))?.path();
        if path.extension().is_none_or(|x| x != "json") {
            continue;
        }
        paths.push(path);
    }
    paths.sort();
    let mut out = Vec::with_capacity(paths.len());
    for (index, path) in paths.into_iter().enumerate() {
        // The entry-count bound: past the cap the file is never opened —
        // named Overflow (unexamined ⇒ not admitted), never a silent skip.
        if index >= MAX_WALK_ENTRIES {
            out.push(EntryFile {
                path,
                decoded: Err(RejectReason::Overflow),
            });
            continue;
        }
        let decoded = read_entry(&path)?;
        out.push(EntryFile { path, decoded });
    }
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
