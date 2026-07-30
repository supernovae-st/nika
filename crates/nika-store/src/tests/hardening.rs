// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

//! Audit wave B (2026-07-30 · the nika-store hardening findings): the
//! commit's read-before-rename (H1 — the identical-entry idempotence ·
//! the named `Conflict`), the walk's two named bounds (`Oversized` ·
//! `Overflow`), the secret-bearing modes (0600 entry files · 0700 fresh
//! dirs — H2), the fold error entries' escape-at-birth (H15), and the
//! trait surface's applied filters (`min_score` · the observed window ·
//! the ts-ranked limit · non-string content · the fail-closed reserved
//! fields). One suite per wave, split out of `api.rs` under the
//! [800, 1500) file-cohesion ratchet; the helpers ride `super::api`.

use super::api::{entry, keypair, signed_dir, store};
use crate::{
    MEMORY_ROOT, RecallVerdict, RejectReason, StoreError, UnsignedEntry, recall_verified,
    remember_signed, seal_fold, store_dir,
};
use nika_cap::Integrity;
use nika_kernel::memory::{MemoryFrame, MemoryLevel, MemoryRecall, MemoryRemember, RecallQuery};

/// H1 · the idempotence claim made true: the canonical name already
/// holding DIFFERENT bytes (an out-of-engine plant) fails with the NAMED
/// conflict — never a silent clobber of what is tamper evidence.
#[test]
fn a_divergent_plant_at_the_canonical_name_is_a_named_conflict() {
    let (_root, dir, _pk, sk) = signed_dir();
    let first = remember_signed(
        &dir,
        entry("default", 1_700_000_000_000, Integrity::trusted()),
        &sk,
    )
    .expect("the first write lands");
    let name = crate::entry_file_name(&first);
    std::fs::write(dir.join(&name), "{\"planted\": \"evidence\"}").expect("the plant lands");
    let err = remember_signed(
        &dir,
        entry("default", 1_700_000_000_000, Integrity::trusted()),
        &sk,
    )
    .expect_err("divergent bytes at the canonical name are a conflict");
    assert!(matches!(err, StoreError::Conflict { .. }), "{err}");
    assert!(
        err.to_string().contains("DIFFERENT entry"),
        "the refusal names why: {err}"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join(&name)).expect("the plant reads"),
        "{\"planted\": \"evidence\"}",
        "the evidence is NEVER silently clobbered"
    );
    assert!(
        !dir.join(format!(".{name}.tmp")).exists(),
        "the tmp goes with the named conflict"
    );
}

/// H1, the honest half: an identical rewrite is a no-op Ok (same content
/// ⇒ same name — read + compare, no rename needed), the file count stays
/// one. (The determinism test pins the name/digest side; this pins the
/// commit's early-Ok path.)
#[test]
fn an_identical_rewrite_is_a_noop_ok() {
    let (_root, dir, _pk, sk) = signed_dir();
    let first = remember_signed(
        &dir,
        entry("default", 1_700_000_000_000, Integrity::trusted()),
        &sk,
    )
    .expect("the first write lands");
    let second = remember_signed(
        &dir,
        entry("default", 1_700_000_000_000, Integrity::trusted()),
        &sk,
    )
    .expect("an identical rewrite is Ok — true idempotence");
    assert_eq!(first.digest(), second.digest());
    assert_eq!(
        std::fs::read_dir(&dir).expect("dir").count(),
        1,
        "one file, no temp sibling left"
    );
}

/// The walk's per-entry size bound: a stuffed `.json` past
/// `MAX_ENTRY_BYTES` is rejected `oversized` (capped read — the multi-GB
/// plant never lands in memory) and counted in the fold, while the honest
/// entries still admit (a bound never collapses the store).
#[test]
fn an_oversized_entry_is_rejected_within_the_bound() {
    let root = tempfile::tempdir().expect("tempdir");
    let root = root.path().join(MEMORY_ROOT);
    let (pk, sk) = keypair();
    let dir = store_dir(&root, "default").expect("the store dir");
    remember_signed(
        &dir,
        entry("default", 1_700_000_000_000, Integrity::trusted()),
        &sk,
    )
    .expect("the honest write lands");
    let stuffed = vec![b'x'; crate::dir::MAX_ENTRY_BYTES + 1];
    std::fs::write(dir.join("1700000000099-eeeeeeeeeeeeeeee.json"), stuffed).expect("the plant");
    let verdicts = recall_verified(&dir, "default", &pk).expect("recall walks");
    assert_eq!(verdicts.len(), 2);
    let oversized = verdicts
        .iter()
        .filter(|v| v.verdict == RecallVerdict::Rejected(RejectReason::Oversized))
        .count();
    assert_eq!(oversized, 1, "the stuffed file is NAMED: {verdicts:?}");
    let fold = seal_fold(&root, &pk).expect("a memory root folds");
    assert_eq!(fold["stores"][0]["admitted_count"], serde_json::json!(1));
    assert_eq!(
        fold["stores"][0]["rejected"],
        serde_json::json!(1),
        "the oversize rejection is counted, never silent"
    );
}

/// The walk's per-store count bound: every `.json` past the first
/// `MAX_WALK_ENTRIES` rides UNREAD as `overflow` — named per file, counted
/// in the fold, and the honest entries below the cap still admit (the
/// overflow is named, never a silent drop and never a store collapse).
#[test]
fn the_entry_count_bound_names_the_overflow() {
    let root = tempfile::tempdir().expect("tempdir");
    let root = root.path().join(MEMORY_ROOT);
    let (pk, sk) = keypair();
    let dir = store_dir(&root, "default").expect("the store dir");
    let honest = remember_signed(
        &dir,
        entry("default", 1_700_000_000_000, Integrity::trusted()),
        &sk,
    )
    .expect("the honest write lands");
    // The honest name sorts FIRST (ts 1_700_000_000_000 < the junk's
    // 1_800_… prefix), so the cap falls strictly on the planted junk.
    let extra = 3_usize;
    for i in 0..crate::dir::MAX_WALK_ENTRIES + extra - 1 {
        std::fs::write(
            dir.join(format!("1800000000000-{i:016x}.json")),
            "{not json",
        )
        .expect("junk");
    }
    let verdicts = recall_verified(&dir, "default", &pk).expect("recall walks");
    let count = |want: RejectReason| {
        verdicts
            .iter()
            .filter(|v| v.verdict == RecallVerdict::Rejected(want.clone()))
            .count()
    };
    assert_eq!(
        verdicts.len(),
        crate::dir::MAX_WALK_ENTRIES + extra,
        "every file rides — nothing silently dropped"
    );
    assert_eq!(
        count(RejectReason::Overflow),
        extra,
        "exactly the beyond-cap files are named Overflow"
    );
    assert_eq!(
        count(RejectReason::Malformed),
        crate::dir::MAX_WALK_ENTRIES - 1,
        "the in-cap junk is read and named Malformed"
    );
    let fold = seal_fold(&root, &pk).expect("a memory root folds");
    assert_eq!(
        fold["stores"][0]["set_digest"],
        serde_json::json!(crate::tests::set_digest(&[honest.digest()])),
        "the honest set still pins in O(1)"
    );
    assert_eq!(
        fold["stores"][0]["rejected"],
        serde_json::json!(crate::dir::MAX_WALK_ENTRIES + extra - 1),
        "the overflow is counted like every rejection"
    );
}

/// H2 · the house convention for secret-bearing paths: entry files land
/// 0600 (set before the rename — born tight) and the dirs the commit
/// creates ride 0700.
#[cfg(unix)]
#[test]
fn entry_files_are_0600_and_fresh_dirs_0700_on_unix() {
    use std::os::unix::fs::PermissionsExt as _;
    let root = tempfile::tempdir().expect("tempdir");
    let root = root.path().join(MEMORY_ROOT);
    let (_pk, sk) = keypair();
    let dir = store_dir(&root, "default").expect("the store dir");
    let written = remember_signed(
        &dir,
        entry("default", 1_700_000_000_000, Integrity::trusted()),
        &sk,
    )
    .expect("the write lands");
    let mode =
        |p: &std::path::Path| std::fs::metadata(p).expect("stat").permissions().mode() & 0o777;
    assert_eq!(
        mode(&dir.join(crate::entry_file_name(&written))),
        0o600,
        "the entry file is born owner-only"
    );
    assert_eq!(mode(&dir), 0o700, "the store dir the commit created");
    assert_eq!(mode(&root), 0o700, "…and the memory root on the chain");
}

/// The fold's error entries are control-stripped AT BIRTH (H15): an
/// io-error text embedding a path with an OSC escape sequence can never
/// reach a terminal through the receipt.
#[cfg(unix)]
#[test]
fn the_fold_error_entries_strip_control_bytes() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (pk, _sk) = keypair();
    // A FILE where the root must be, its path carrying an OSC introducer.
    let root = tmp.path().join(".nika/mem\u{1b}]52;;pwn\u{7}ory");
    std::fs::create_dir_all(root.parent().expect("the parent")).expect("the parent dir");
    std::fs::write(&root, "a file where the root must be").expect("the blocker");
    let fold = seal_fold(&root, &pk).expect("the error entry rides");
    let error = fold["stores"][0]["error"].as_str().expect("the error text");
    assert!(
        !error.chars().any(char::is_control),
        "no live control byte survives: {error:?}"
    );
    assert!(
        error.contains("]52;;pwn"),
        "the text stays readable: {error}"
    );
}

// ─── The query-facet honesty (audit wave B · item 8) ─────────────────

/// `min_score` binds UNIFORMLY against v1's constant 1.0 score: a
/// threshold above 1.0 — or a NaN — recalls NOTHING (the placeholder
/// never fails open), an in-range threshold still binds.
#[tokio::test]
async fn a_min_score_above_one_recalls_nothing_never_failing_open() {
    let (_root, dir, pk, sk) = signed_dir();
    let store = store(dir, sk, pk);
    store
        .remember(MemoryFrame::new("a butterfly fact", MemoryLevel::Semantic))
        .await
        .expect("remember");
    for m in [1.5, f32::NAN] {
        let mut q = RecallQuery::new("butterfly");
        q.min_score = Some(m);
        assert!(
            store.recall(q).await.expect("recall").is_empty(),
            "min_score {m} must admit nothing (score is a constant 1.0)"
        );
    }
    let mut q = RecallQuery::new("butterfly");
    q.min_score = Some(0.5);
    assert_eq!(
        store.recall(q).await.expect("recall").len(),
        1,
        "an in-range threshold binds against the constant score"
    );
}

/// The observed window binds when the frame carries the stamp (strictly
/// inside); a stamp-less entry under a WINDOWED query is SKIPPED — the
/// named class (the window cannot bind what it cannot read).
#[tokio::test]
async fn the_observed_window_binds_and_names_the_stamp_less_skip() {
    let (_root, dir, pk, sk) = signed_dir();
    let store = store(dir, sk, pk);
    let mut old = MemoryFrame::new("an old butterfly fact", MemoryLevel::Semantic);
    old.observed_at = Some(100); // ns-canonical
    let mut new = MemoryFrame::new("a new butterfly fact", MemoryLevel::Semantic);
    new.observed_at = Some(200);
    store.remember(old).await.expect("remember old");
    store.remember(new).await.expect("remember new");
    store
        .remember(MemoryFrame::new(
            "a stamp-less butterfly fact",
            MemoryLevel::Semantic,
        ))
        .await
        .expect("remember stamp-less");

    let mut windowed = RecallQuery::new("butterfly");
    windowed.observed_after = Some(150);
    let hits = store.recall(windowed).await.expect("recall");
    assert_eq!(hits.len(), 1, "only the in-window stamp rides: {hits:?}");
    assert_eq!(hits[0].content, "a new butterfly fact");
    assert_eq!(hits[0].observed_at, Some(200));

    // Both bounds + the strict edge (after is exclusive).
    let mut closed = RecallQuery::new("butterfly");
    closed.observed_after = Some(200);
    closed.observed_before = Some(201);
    assert!(
        store.recall(closed).await.expect("recall").is_empty(),
        "observed_at == after is outside (strictly-after semantics)"
    );
    let mut unwindowed = RecallQuery::new("butterfly");
    unwindowed.limit = Some(10);
    assert_eq!(
        store.recall(unwindowed).await.expect("recall").len(),
        3,
        "an unwindowed query never asks for the stamp"
    );
}

/// `limit` keeps the NEWEST: recall ranks ts-descending (the constant
/// score leaves recency as the honest "ranked") before truncating.
#[tokio::test]
async fn the_limit_keeps_the_newest_first() {
    let (_root, dir, pk, sk) = signed_dir();
    for (ts, fact) in [
        (1_700_000_000_001, "fact alpha"),
        (1_700_000_000_002, "fact beta"),
        (1_700_000_000_003, "fact gamma"),
    ] {
        remember_signed(
            &dir,
            UnsignedEntry::new(
                serde_json::json!({"content": fact}),
                Integrity::trusted(),
                "default".to_owned(),
                "run-1".to_owned(),
                ts,
            ),
            &sk,
        )
        .expect("write");
    }
    let store = store(dir, sk, pk);
    let mut q = RecallQuery::new("fact");
    q.limit = Some(2);
    let hits = store.recall(q).await.expect("recall");
    let contents: Vec<&str> = hits.iter().map(|h| h.content.as_str()).collect();
    assert_eq!(
        contents,
        ["fact gamma", "fact beta"],
        "ts-descending, truncated to the newest — never the oldest"
    );
}

/// A non-string `content` (an honestly-signed non-text frame) is ADMITTED
/// by the substrate and SKIPPED by the trait projection — never a silent
/// empty-content hit, never a vanishing entry.
#[tokio::test]
async fn a_non_string_content_is_skipped_never_an_empty_hit() {
    let (_root, dir, pk, sk) = signed_dir();
    remember_signed(
        &dir,
        UnsignedEntry::new(
            serde_json::json!({"content": {"nested": true}}),
            Integrity::trusted(),
            "default".to_owned(),
            "run-1".to_owned(),
            1_700_000_000_000,
        ),
        &sk,
    )
    .expect("the honest non-text write signs");
    let verdicts = recall_verified(&dir, "default", &pk).expect("recall walks");
    assert_eq!(
        verdicts[0].verdict,
        RecallVerdict::Admitted,
        "the substrate admits it (the signature is honest)"
    );
    let store = store(dir, sk, pk);
    assert!(
        store
            .recall(RecallQuery::new(""))
            .await
            .expect("recall")
            .is_empty(),
        "the projection skips it — an empty-content hit is the failure mode"
    );
}

/// The reserved frame fields REFUSE the write (fail-closed): the v1
/// envelope has no slot for them, so signing anyway would drop them
/// inside the signature's own attestation.
#[tokio::test]
async fn a_reserved_field_refuses_the_write_fail_closed() {
    let (_root, dir, pk, sk) = signed_dir();
    let store = store(dir.clone(), sk, pk.clone());
    for (i, frame) in [
        {
            let mut f = MemoryFrame::new("ciphered", MemoryLevel::Working);
            f.cipher = Some("aes-gcm".to_owned());
            f
        },
        {
            let mut f = MemoryFrame::new("provenanced", MemoryLevel::Working);
            f.provenance = Some("hand-crafted".to_owned());
            f
        },
        {
            let mut f = MemoryFrame::new("retained", MemoryLevel::Working);
            f.retention = Some("ttl:1h".to_owned());
            f
        },
        {
            let mut f = MemoryFrame::new("redacted", MemoryLevel::Working);
            f.redactions = Some(vec!["$.pii".to_owned()]);
            f
        },
    ]
    .into_iter()
    .enumerate()
    {
        let err = store.remember(frame).await.expect_err("reserved refuses");
        assert!(
            err.to_string().contains("reserved frame fields"),
            "the refusal names the class ({i}): {err}"
        );
    }
    assert!(
        recall_verified(&dir, "default", &pk)
            .expect("recall walks")
            .is_empty(),
        "nothing ever landed — the refusal is total"
    );
}
