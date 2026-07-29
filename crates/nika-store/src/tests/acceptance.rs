// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

//! The F-P8 acceptance pairs (docs/crate-specs/nika-store.md §4) — the law's
//! own tests: a legit write is admitted; an out-of-engine edit is rejected;
//! a relabeled envelope is rejected by construction; an unsigned entry's
//! admission is 0 % (the SMSR no-provenance-free-filter theorem).

use std::path::Path;

use crate::{
    EntryVerdict, MEMORY_ROOT, RecallVerdict, RejectReason, UnsignedEntry, entry_file_name,
    recall_verified, remember_signed, seal_fold, store_dir,
};
use nika_cap::Integrity;

fn keypair() -> (minisign::PublicKey, minisign::SecretKey) {
    let pair =
        minisign::KeyPair::generate_encrypted_keypair(Some(String::new())).expect("keypair mints");
    (pair.pk, pair.sk)
}

fn entry(store: &str, run: &str, ts: u64, label: Integrity) -> UnsignedEntry {
    UnsignedEntry::new(
        serde_json::json!({"content": format!("the run saw a butterfly ({store}/{run}/{ts})")}),
        label,
        store.to_owned(),
        run.to_owned(),
        ts,
    )
}

/// Write an entry file OUTSIDE the engine (the attacker has the store dir,
/// never the key): raw bytes at the path the honest writer would have used.
fn write_outside(dir: &Path, name: &str, value: &serde_json::Value) {
    std::fs::write(
        dir.join(name),
        serde_json::to_string_pretty(value).expect("serializes"),
    )
    .expect("the outside write lands");
}

/// **positive** — a legit write is signed + labeled; recall admits it; the
/// seal fold pins the admitted digest + the rejection count.
#[test]
fn a_signed_write_is_admitted_and_its_digest_lands_in_the_fold() {
    let root = tempfile::tempdir().expect("tempdir");
    let root = root.path().join(MEMORY_ROOT);
    let (pk, sk) = keypair();
    let dir = store_dir(&root, "default").expect("the store dir");

    let written = remember_signed(
        &dir,
        entry(
            "default",
            "run-1",
            1_700_000_000_000,
            Integrity::untrusted("fetch_page"),
        ),
        &sk,
    )
    .expect("the write signs + commits");

    // Signed + labeled: the label rides INSIDE the signature (the file's
    // preimage minus `sig`), the file lands at its digest-named path.
    let name = entry_file_name(&written);
    assert!(name.starts_with("1700000000000-"), "ts-prefixed: {name}");
    assert!(
        name.contains(&written.digest()[..16]),
        "digest-named: {name} carries {}",
        &written.digest()[..16]
    );
    let text = std::fs::read_to_string(dir.join(&name)).expect("the file exists");
    assert!(text.contains("\"untrusted\"") && text.contains("fetch_page"));
    assert!(text.contains("\"sig\""), "the signature rides the envelope");
    assert!(
        !written.preimage().contains("\"sig\""),
        "the preimage excludes the sig: {}",
        written.preimage()
    );
    assert!(
        written.preimage().contains("\"v\":1"),
        "the format version rides INSIDE the signed preimage: {}",
        written.preimage()
    );

    // Recall admits it, the provenance intact.
    let verdicts = recall_verified(&dir, "default", &pk).expect("recall walks");
    assert_eq!(verdicts.len(), 1);
    let EntryVerdict { entry, verdict, .. } = &verdicts[0];
    assert_eq!(*verdict, RecallVerdict::Admitted);
    let decoded = entry.as_ref().expect("admitted entries carry the envelope");
    assert_eq!(decoded.label.source(), Some("fetch_page"));
    assert_eq!(decoded.run_id, "run-1");
    assert_eq!(
        decoded.digest(),
        written.digest(),
        "digest stable across decode"
    );

    // The fold pins the admitted digest + the zero rejection count (and
    // carries its own format version beside the stores).
    let fold = seal_fold(&root, &pk).expect("a memory root folds");
    assert_eq!(fold["v"], serde_json::json!(1), "the fold is versioned");
    assert_eq!(fold["stores"][0]["store"], serde_json::json!("default"));
    assert_eq!(
        fold["stores"][0]["admitted"],
        serde_json::json!([written.digest()]),
        "the receipt names the verified SET"
    );
    assert_eq!(fold["stores"][0]["rejected"], serde_json::json!(0));
}

/// **negative 1** — an entry added OUTSIDE the engine (a direct file edit
/// in the store dir) is REJECTED at recall — never admitted, never filtered.
#[test]
fn a_direct_file_edit_is_rejected_bad_signature() {
    let root = tempfile::tempdir().expect("tempdir");
    let root = root.path().join(MEMORY_ROOT);
    let (pk, sk) = keypair();
    let dir = store_dir(&root, "default").expect("the store dir");

    let honest = remember_signed(
        &dir,
        entry("default", "run-1", 1_700_000_000_000, Integrity::trusted()),
        &sk,
    )
    .expect("the honest write lands");

    // The outside edit: flip one byte in the entry file's content field.
    let path = dir.join(entry_file_name(&honest));
    let text = std::fs::read_to_string(&path).expect("the file exists");
    assert!(text.contains("butterfly"));
    std::fs::write(&path, text.replacen("butterfly", "bvtterfly", 1)).expect("the edit lands");

    let verdicts = recall_verified(&dir, "default", &pk).expect("recall walks");
    assert_eq!(verdicts.len(), 1);
    assert_eq!(
        verdicts[0].verdict,
        RecallVerdict::Rejected(RejectReason::BadSignature),
        "one flipped byte anywhere in the envelope = rejected"
    );
    let fold = seal_fold(&root, &pk).expect("a memory root folds");
    assert_eq!(fold["stores"][0]["admitted"], serde_json::json!([]));
    assert_eq!(fold["stores"][0]["rejected"], serde_json::json!(1));
}

/// **negative 2** — a write derived from an untrusted recall that claims a
/// Trusted label = signature mismatch = reject: the label inside the
/// preimage is the only label that counts (a relabel IS a byte flip).
#[test]
fn a_relabelled_envelope_is_rejected_by_construction() {
    let root = tempfile::tempdir().expect("tempdir");
    let root = root.path().join(MEMORY_ROOT);
    let (pk, sk) = keypair();
    let dir = store_dir(&root, "default").expect("the store dir");

    let untrusted = remember_signed(
        &dir,
        entry(
            "default",
            "run-1",
            1_700_000_000_000,
            Integrity::untrusted("mcp:browser/open"),
        ),
        &sk,
    )
    .expect("the honest untrusted write lands");

    // The relabel: decode the envelope, claim `"trusted"`, re-serialize —
    // exactly what an attacker WITHOUT the key can do.
    let path = dir.join(entry_file_name(&untrusted));
    let text = std::fs::read_to_string(&path).expect("the file exists");
    let mut value: serde_json::Value = serde_json::from_str(&text).expect("the envelope parses");
    value["label"] = serde_json::json!({"kind": "trusted"});
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&value).expect("serializes"),
    )
    .expect("the relabel lands");

    let verdicts = recall_verified(&dir, "default", &pk).expect("recall walks");
    assert_eq!(verdicts.len(), 1);
    let EntryVerdict { entry, verdict, .. } = &verdicts[0];
    assert_eq!(
        *verdict,
        RecallVerdict::Rejected(RejectReason::BadSignature),
        "LabelMismatch ≡ BadSignature by construction"
    );
    // The DECODED label says trusted — the verdict still rejects: only the
    // label inside the signed preimage counts, and the signature swears it
    // was untrusted.
    assert!(
        entry
            .as_ref()
            .expect("the envelope decodes")
            .label
            .is_trusted(),
        "the relabel decoded — and still never verifies"
    );
}

/// **target** — an unsigned entry = 0 % admission (the SMSR theorem's floor).
#[test]
fn an_unsigned_entry_is_zero_percent_admission() {
    let root = tempfile::tempdir().expect("tempdir");
    let root = root.path().join(MEMORY_ROOT);
    let (pk, sk) = keypair();
    let dir = store_dir(&root, "default").expect("the store dir");

    // A well-shaped envelope whose `sig` never existed (the outsider can
    // copy the honest shape exactly — provenance is what they cannot mint).
    let honest = remember_signed(
        &dir,
        entry("default", "run-1", 1_700_000_000_000, Integrity::trusted()),
        &sk,
    )
    .expect("the honest write lands");
    let mut forged = honest.file_value();
    forged["sig"] = serde_json::json!("");
    write_outside(&dir, "1700000000099-0000000000000000.json", &forged);
    // And a hand-rolled envelope with no sig field at all (no `v` either —
    // an absent version marker decodes as v1, so the naming stays
    // `Unsigned`, the theorem's floor, never a decode-class rejection).
    write_outside(
        &dir,
        "1700000000098-0000000000000001.json",
        &serde_json::json!({
            "frame": {"content": "I was never signed"},
            "label": {"kind": "trusted"},
            "store": "default",
            "run_id": "run-9",
            "ts_ms": 1_700_000_000_098_u64,
            "parents_digest": "",
        }),
    );

    let verdicts = recall_verified(&dir, "default", &pk).expect("recall walks");
    assert_eq!(verdicts.len(), 3);
    let admitted = verdicts
        .iter()
        .filter(|v| v.verdict == RecallVerdict::Admitted)
        .count();
    let unsigned = verdicts
        .iter()
        .filter(|v| v.verdict == RecallVerdict::Rejected(RejectReason::Unsigned))
        .count();
    assert_eq!(admitted, 1, "only the signed entry admits");
    assert_eq!(unsigned, 2, "both unsigned shapes are NAMED: {verdicts:?}");
    let fold = seal_fold(&root, &pk).expect("a memory root folds");
    assert_eq!(
        fold["stores"][0]["admitted"],
        serde_json::json!([honest.digest()])
    );
    assert_eq!(
        fold["stores"][0]["rejected"],
        serde_json::json!(2),
        "the seal pins the 0 %-admission evidence"
    );
}
