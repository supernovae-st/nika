// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The run's signed-memory fold (F-P8 · SMSR signed memory) — the
//! teardown-side half of `nika-store`: when the run's CWD holds a
//! `.nika/memory/` store, [`attend`] verifies every entry against the
//! run-signing pair's PUBLIC half and returns the [`MemoryAttend`] — the
//! fold the seal pins in `covers["memory"]`
//! (`{v: 1, stores: [{store, admitted: [digest…], rejected: n} | {store,
//! error}]}` — the admitted SET named by digest beside the rejection
//! count, a failed walk NAMED in place, never collapsed) PLUS the
//! per-entry rejections, named (unsigned · bad signature · relabel =
//! rejected, never filtered). The rejections are the law's third leg:
//! [`journal_rejected`] writes one `memory_entry_rejected` event per
//! rejection INTO the journal BEFORE the seal lands, so the chain the
//! seal signs covers the evidence it attests the count of.
//!
//! The honest postures (the quarantine sibling's): no store ⇒ a
//! fold-less, rejection-less attendance and the `memory` key stays OUT of
//! the covers (absent is honest — and the key custody is never even
//! probed: the short-circuit runs first, so a store-less run never
//! touches the keychain); a store with no usable run-key attests nothing
//! either (the same honesty as an unsealed journal). The pubkey probe is
//! [`crate::seal::load_public_box`] — parse-only, no decrypt: no secret
//! material ever reaches the teardown path. The substrate (signing ·
//! per-entry verdicts · the fold shape) lives in `nika-store` (L1); this
//! module is only the L4 custody-and-context glue — descended with the
//! trust plane (the quarantine precedent: compute descends, render
//! stays).

use std::path::{Path, PathBuf};

use nika_event::{Event, EventKind};
use nika_types::id::EventId;
use nika_types::resource::{KeyValue, Value};
use nika_types::timestamp::Timestamp;

/// One rejected memory entry, named for the journal (the fold counts;
/// the journal NAMES — one `memory_entry_rejected` event each).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RejectedEntry {
    /// The store the entry rides.
    pub store: String,
    /// The entry file's path under the store dir.
    pub path: PathBuf,
    /// The rejection's wire word (`unsigned` · `malformed` ·
    /// `key_mismatch` · `store_mismatch` · `bad_signature` · … — the set
    /// grows additively with `nika_store::RejectReason`).
    pub reason: String,
}

impl RejectedEntry {
    /// Name one rejection (the store · the entry path · the wire-word reason).
    #[must_use]
    pub fn new(store: String, path: PathBuf, reason: String) -> Self {
        Self {
            store,
            path,
            reason,
        }
    }
}

/// The run's memory attendance: the seal fold (what the receipt pins)
/// beside the named per-entry rejections (what the journal journals).
/// Both empty when there is honestly nothing to attest (no
/// `.nika/memory/` dir · no usable run-key on this machine) — absent is
/// honest, and the custody probe never runs for a store-less run.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct MemoryAttend {
    /// The `covers["memory"]` fold (`None` = the key stays OUT).
    pub fold: Option<serde_json::Value>,
    /// The named rejections — journaled one `memory_entry_rejected`
    /// event each BEFORE the seal (the chain then covers them).
    pub rejected: Vec<RejectedEntry>,
}

/// The memory attendance for one run, empty when there is honestly
/// nothing to attest (no `.nika/memory/` dir · no store below it · no
/// usable run-key on this machine). `cwd` is the run's directory:
/// `Some(dir)` for tests, `None` for the process's own CWD (a `nika
/// run` never chdirs — the same posture the quarantine paths keep).
#[must_use]
pub fn attend(cwd: Option<&Path>) -> MemoryAttend {
    let root = cwd.map_or_else(
        || std::path::PathBuf::from(nika_store::MEMORY_ROOT),
        |dir| dir.join(nika_store::MEMORY_ROOT),
    );
    if !root.is_dir() {
        return MemoryAttend::default(); // absent is honest — no store, no custody probe
    }
    let attended = crate::seal::load_public_box()
        .and_then(|pk_box| minisign::PublicKeyBox::from_string(pk_box.trim()).ok())
        .and_then(|pk_box| pk_box.into_public_key().ok())
        .map(|pk| nika_store::seal_fold_detailed(&root, &pk));
    match attended {
        None => MemoryAttend::default(), // no usable run-key: attests nothing (unsealed honesty)
        Some((fold, rejections)) => MemoryAttend {
            fold,
            rejected: rejections
                .iter()
                .map(|r| RejectedEntry {
                    store: r.store.clone(),
                    path: r.path.clone(),
                    reason: r.reason.as_str().to_owned(),
                })
                .collect(),
        },
    }
}

/// The `memory_entry_rejected` journal event for one named rejection —
/// `store` + `entry` + `reason` fields (the kind's documented shape).
/// Stamped from the journal's own epilogue clock with a fresh `UUIDv7` —
/// the seal's own stamping precedent (the runtime's stamper belongs to
/// the run; this is the journal's teardown).
#[must_use]
pub fn rejected_event(entry: &RejectedEntry) -> Event {
    let name = entry
        .path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Event::new(
        EventId::generate(),
        Timestamp::from_unix_ms(crate::journal::now_millis()),
        EventKind::MemoryEntryRejected,
    )
    .with_fields(vec![
        KeyValue::new("store", Value::String(entry.store.clone())),
        KeyValue::new("entry", Value::String(name)),
        KeyValue::new("reason", Value::String(entry.reason.clone())),
    ])
}

/// Journal one `memory_entry_rejected` event per named rejection — the
/// teardown lanes call this BEFORE [`crate::journal::seal_journal_with`],
/// so the events sit INSIDE the chain the seal signs (the seal's
/// `covers["memory"]` attests their count; the chain covers their names).
/// The sink contract is infallible: a broken journal buffers its own
/// error, the run's verdict never depends on the evidence.
pub fn journal_rejected(trace: &mut crate::journal::TraceFileSink, rejected: &[RejectedEntry]) {
    use nika_runtime::EventSink as _;
    for entry in rejected {
        trace.emit(rejected_event(entry));
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    /// The absent-is-honest posture: a CWD with no `.nika/memory/`
    /// attests NOTHING — no fold, no rejection, and the custody probe
    /// never runs (the short-circuit comes first).
    #[test]
    fn a_run_without_a_memory_store_attends_nothing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let attended = attend(Some(tmp.path()));
        assert!(attended.fold.is_none(), "no store ⇒ no fold");
        assert!(
            attended.rejected.is_empty(),
            "no store ⇒ nothing to name: {:?}",
            attended.rejected
        );
    }

    /// The event speaks the kind's documented shape: the
    /// `memory_entry_rejected` kind with `store` + `entry` + `reason`
    /// fields (the entry rides as its digest-derived FILE NAME — the
    /// store dir is the journal's context, the name is the identity).
    #[test]
    fn the_rejected_event_names_store_entry_and_reason() {
        let entry = RejectedEntry {
            store: "default".to_owned(),
            path: PathBuf::from(".nika/memory/default/1700000000000-ab12cd34ef56ab12.json"),
            reason: "bad_signature".to_owned(),
        };
        let ev = rejected_event(&entry);
        assert!(matches!(ev.kind, EventKind::MemoryEntryRejected));
        let get = |key: &str| {
            ev.fields
                .iter()
                .find(|f| f.key == key)
                .and_then(|f| match &f.value {
                    Value::String(s) => Some(s.clone()),
                    _ => None,
                })
        };
        assert_eq!(get("store").as_deref(), Some("default"));
        assert_eq!(
            get("entry").as_deref(),
            Some("1700000000000-ab12cd34ef56ab12.json"),
            "the entry's file name, not its full path"
        );
        assert_eq!(get("reason").as_deref(), Some("bad_signature"));
    }

    /// The law's third leg, chained: `journal_rejected` writes the events
    /// INTO the journal, so a seal landing after them covers their bytes
    /// (the teardown lanes emit before `seal_journal_with`).
    #[test]
    fn the_rejections_land_in_the_journal_before_any_seal() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut trace = crate::journal::TraceFileSink::new(tmp.path().join("traces"));
        let rejected = vec![
            RejectedEntry {
                store: "default".to_owned(),
                path: PathBuf::from(".nika/memory/default/1700000000001-aaaaaaaaaaaaaaaa.json"),
                reason: "unsigned".to_owned(),
            },
            RejectedEntry {
                store: "default".to_owned(),
                path: PathBuf::from(".nika/memory/default/1700000000002-bbbbbbbbbbbbbbbb.json"),
                reason: "store_mismatch".to_owned(),
            },
        ];
        journal_rejected(&mut trace, &rejected);
        let path = trace.path().expect("the journal opened").to_path_buf();
        assert!(trace.into_error().is_none());
        let text = std::fs::read_to_string(path).expect("journal reads");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "one event per rejection");
        for (line, reason) in lines.iter().zip(["unsigned", "store_mismatch"]) {
            assert!(line.contains("\"memory_entry_rejected\""), "{line}");
            assert!(line.contains("\"default\""), "{line}");
            assert!(line.contains(reason), "{line}");
            assert!(line.contains("\"chain\""), "chained like every line");
        }
    }
}
