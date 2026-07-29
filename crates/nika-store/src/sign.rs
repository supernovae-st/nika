// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Sign at write · verify at recall (F-P8): [`remember_signed`] mints the
//! signed envelope, [`recall_verified`] walks a store dir and names every
//! entry's verdict — Admitted, or Rejected with the reason. Never filtered.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::dir;
use crate::entry::{StoreEntry, UnsignedEntry};
use crate::errors::StoreError;

/// Why an entry was rejected at recall (the SMSR theorem's floor, named).
/// The set grows ADDITIVELY (the wire words below are stable; a new class
/// joins as a new word, never a re-meaning).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RejectReason {
    /// No signature rides the envelope (0 % admission — the theorem).
    Unsigned,
    /// The file is not a decodable envelope (bad bytes · missing content
    /// fields — a missing `sig` alone is [`Self::Unsigned`], not this).
    Malformed,
    /// The envelope's format version `"v"` is one this version does not
    /// know — named, never masqueraded as a signature failure (the
    /// signature commits to `v`, so only an honestly-signed NEWER
    /// envelope can carry it).
    UnsupportedVersion,
    /// The signature's key id is not the injected pubkey's (another key signed).
    KeyMismatch,
    /// The signature verifies but the entry names a different store — a
    /// cross-store replay (the crypto is honest about WHAT it says, not WHERE).
    StoreMismatch,
    /// The signature verifies but the FILE NAME is not the entry's
    /// canonical name (ts + digest-prefix) — the same bytes riding a
    /// second name: a copy would pin the digest twice in the receipt and
    /// dupe recall, a rename would steer recall order. The name is part
    /// of the envelope's context, checked only AFTER the crypto (the
    /// strongest claim decides first).
    NameMismatch,
    /// The signature does not verify over the JCS preimage — ANY byte flip,
    /// including a rewritten label (the label rides INSIDE the preimage:
    /// a relabel IS a signature mismatch, by construction).
    BadSignature,
}

impl RejectReason {
    /// The stable wire word (the journaled `memory_entry_rejected`'s `reason`).
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Unsigned => "unsigned",
            Self::Malformed => "malformed",
            Self::UnsupportedVersion => "unsupported_version",
            Self::KeyMismatch => "key_mismatch",
            Self::StoreMismatch => "store_mismatch",
            Self::NameMismatch => "name_mismatch",
            Self::BadSignature => "bad_signature",
        }
    }
}

impl std::fmt::Display for RejectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The per-entry recall verdict (the `sign.rs` verdict shape — each surface
/// maps its own rendering).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RecallVerdict {
    /// The signature verifies over the exact envelope bytes — admitted.
    Admitted,
    /// Rejected with the named reason — never silently filtered.
    Rejected(RejectReason),
}

/// One entry file's recall outcome. The spec's sketched tuple
/// (`(StoreEntry, RecallVerdict)`) cannot name a malformed file — the
/// no-silent-filtering law requires naming it, so the entry is optional
/// and the path always rides.
#[derive(Debug)]
#[non_exhaustive]
pub struct EntryVerdict {
    /// The entry file's path.
    pub path: PathBuf,
    /// The decoded envelope (`Some` whenever the file parses).
    pub entry: Option<StoreEntry>,
    /// The named verdict.
    pub verdict: RecallVerdict,
}

/// The deterministic, zero-LLM write path (F-F3): the label the caller
/// inherited rides INSIDE the signature; the envelope commits atomically.
///
/// # Errors
///
/// [`StoreError`] on IO/serialize failures (the entry never partially lands).
pub fn remember_signed(
    dir: &Path,
    entry: UnsignedEntry,
    key: &minisign::SecretKey,
) -> Result<StoreEntry, StoreError> {
    let mut entry = StoreEntry::from_unsigned(entry);
    let sig = minisign::sign(
        None,
        key,
        Cursor::new(entry.preimage().as_bytes()),
        None,
        None,
    )
    .map_err(|e| StoreError::Serialize {
        reason: format!("minisign: {e}"),
    })?;
    entry.sig = sig.into_string();
    dir::commit(dir, &entry)?;
    Ok(entry)
}

/// Verify one decoded entry: crypto FIRST (the strongest claim decides),
/// then the context — the store a validly-signed entry was replayed
/// across, then the FILE NAME the entry rides (a verified digest is the
/// only one worth name-checking: a tampered entry answers `BadSignature`,
/// never the name mismatch its shifted digest would also produce).
fn verdict(
    path: &Path,
    entry: &StoreEntry,
    store: &str,
    pubkey: &minisign::PublicKey,
) -> RecallVerdict {
    if entry.sig.is_empty() {
        return RecallVerdict::Rejected(RejectReason::Unsigned);
    }
    let Ok(sig) = minisign::SignatureBox::from_string(&entry.sig) else {
        return RecallVerdict::Rejected(RejectReason::BadSignature);
    };
    if sig.keynum() != pubkey.keynum() {
        return RecallVerdict::Rejected(RejectReason::KeyMismatch);
    }
    if minisign::verify(
        pubkey,
        &sig,
        Cursor::new(entry.preimage().as_bytes()),
        true,
        false,
        false,
    )
    .is_err()
    {
        return RecallVerdict::Rejected(RejectReason::BadSignature);
    }
    if entry.store != store {
        return RecallVerdict::Rejected(RejectReason::StoreMismatch);
    }
    if path.file_name() != Some(std::ffi::OsStr::new(&dir::entry_file_name(entry))) {
        return RecallVerdict::Rejected(RejectReason::NameMismatch);
    }
    RecallVerdict::Admitted
}

/// The verified recall: walk a store dir, verify every entry, admit or
/// reject — the caller journals the rejections (`memory_entry_rejected`).
/// A missing dir is an empty store (absent is honest), never an error.
///
/// # Errors
///
/// [`StoreError`] when the dir cannot be walked or an entry file cannot be read.
pub fn recall_verified(
    dir: &Path,
    store: &str,
    pubkey: &minisign::PublicKey,
) -> Result<Vec<EntryVerdict>, StoreError> {
    let files = dir::walk(dir)?;
    let mut out = Vec::with_capacity(files.len());
    for file in files {
        let v = match &file.decoded {
            Err(reason) => RecallVerdict::Rejected(reason.clone()),
            Ok(entry) => verdict(&file.path, entry, store, pubkey),
        };
        out.push(EntryVerdict {
            path: file.path,
            entry: file.decoded.ok(),
            verdict: v,
        });
    }
    Ok(out)
}

/// One rejection the fold counted, NAMED for the journal (the fold's
/// `rejected` count is the receipt's evidence; the per-entry names are
/// the `memory_entry_rejected` events the teardown lanes journal BEFORE
/// the seal, so the chain covers them).
#[derive(Debug)]
#[non_exhaustive]
pub struct FoldRejection {
    /// The store the entry rides.
    pub store: String,
    /// The entry file's path.
    pub path: PathBuf,
    /// The named reason (the event's `reason` wire word).
    pub reason: RejectReason,
}

/// The seal fold (F-P8 riding the F-P2 covers): [`seal_fold_detailed`]'s
/// fold half, discarding the named rejections.
#[must_use]
pub fn seal_fold(memory_root: &Path, pubkey: &minisign::PublicKey) -> Option<serde_json::Value> {
    seal_fold_detailed(memory_root, pubkey).0
}

/// Walk every store under a memory root, verify per entry, and shape what
/// the run's seal pins — `{"v": 1, "stores": [{"store", "admitted":
/// [digest…], "rejected": n}]}` — the admitted set NAMED by digest, the
/// rejections counted AND returned (the [`FoldRejection`] list the
/// teardown journals one `memory_entry_rejected` event each).
///
/// Failure is NEVER collapsed to `None` (a `None` reads as "no memory" —
/// it would erase tamper evidence): a store whose walk fails rides the
/// fold as `{"store": s, "error": "<reason>"}` beside the
/// admitted/rejected shape; a per-item `read_dir` error at the root rides
/// the same way with `store: null` (the name is what the error denies);
/// a wholly-unreadable root still yields `Some` with the error entry.
/// `None` is kept STRICTLY for "no memory root / no stores" (absent is
/// honest) — the `memory` key then stays OUT of the covers.
#[must_use]
pub fn seal_fold_detailed(
    memory_root: &Path,
    pubkey: &minisign::PublicKey,
) -> (Option<serde_json::Value>, Vec<FoldRejection>) {
    let mut rejections = Vec::new();
    let mut folds: Vec<serde_json::Value> = Vec::new();
    let error_entry = |store: Option<&str>, reason: String| json!({ "store": store.map_or(serde_json::Value::Null, serde_json::Value::from), "error": reason });
    if !memory_root.exists() {
        return (None, rejections); // absent is honest — no attestation at all
    }
    match std::fs::read_dir(memory_root) {
        Err(e) => folds.push(error_entry(
            None,
            format!("read_dir {}: {e}", memory_root.display()),
        )),
        Ok(items) => {
            let mut stores: Vec<(String, PathBuf)> = Vec::new();
            for item in items {
                match item {
                    Err(e) => folds.push(error_entry(
                        None,
                        format!("read_dir {}: {e}", memory_root.display()),
                    )),
                    Ok(item) => {
                        let path = item.path();
                        if path.is_dir() {
                            // Lossy, never dropped: a non-UTF-8 store name
                            // still FOLDS (recall works on the PathBuf).
                            stores.push((item.file_name().to_string_lossy().into_owned(), path));
                        }
                    }
                }
            }
            stores.sort();
            for (store, path) in stores {
                match recall_verified(&path, &store, pubkey) {
                    Err(e) => folds.push(error_entry(Some(&store), e.to_string())),
                    Ok(entries) => {
                        let mut admitted = Vec::new();
                        let mut rejected = 0_usize;
                        for e in &entries {
                            if e.verdict == RecallVerdict::Admitted
                                && let Some(entry) = &e.entry
                            {
                                admitted.push(entry.digest());
                            } else {
                                rejected += 1;
                                if let RecallVerdict::Rejected(reason) = &e.verdict {
                                    rejections.push(FoldRejection {
                                        store: store.clone(),
                                        path: e.path.clone(),
                                        reason: reason.clone(),
                                    });
                                }
                            }
                        }
                        // The receipt pins a SET: sorted + deduped. The
                        // name binding (RejectReason::NameMismatch) makes
                        // a repeated digest unreachable through the fs —
                        // the dedup is the belt-and-braces the SET law
                        // buys for free.
                        admitted.sort();
                        admitted.dedup();
                        folds.push(json!({
                            "store": store,
                            "admitted": admitted,
                            "rejected": rejected,
                        }));
                    }
                }
            }
        }
    }
    // `None` strictly for "no stores below the root (and nothing failed
    // naming them)" — an error entry ALWAYS attests `Some`.
    let fold = (!folds.is_empty()).then(|| json!({ "v": 1, "stores": folds }));
    (fold, rejections)
}
