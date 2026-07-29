// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

//! The API contract beyond the acceptance pairs: the remaining verdict
//! classes (key mismatch · store mismatch · malformed · unsupported
//! version · name mismatch), the honest empty states, the fold's named
//! error entries, the digest/file-name determinism, the error one-voice
//! mapping (NIKA-605), and the kernel-trait surface (ADR-078).

use std::path::PathBuf;

use crate::{
    MEMORY_ROOT, RecallVerdict, RejectReason, SignedMemoryStore, StoreError, UnsignedEntry,
    recall_verified, remember_signed, seal_fold, seal_fold_detailed, store_dir,
};
use nika_cap::Integrity;
use nika_kernel::memory::{
    MemoryForget, MemoryFrame, MemoryLevel, MemoryRecall, MemoryRemember, MemoryStore, RecallQuery,
};

fn keypair() -> (minisign::PublicKey, minisign::SecretKey) {
    let pair =
        minisign::KeyPair::generate_encrypted_keypair(Some(String::new())).expect("keypair mints");
    (pair.pk, pair.sk)
}

fn entry(store: &str, ts: u64, label: Integrity) -> UnsignedEntry {
    UnsignedEntry::new(
        serde_json::json!({"content": format!("fact {store} {ts}")}),
        label,
        store.to_owned(),
        "run-1".to_owned(),
        ts,
    )
}

fn signed_dir() -> (
    tempfile::TempDir,
    PathBuf,
    minisign::PublicKey,
    minisign::SecretKey,
) {
    let root = tempfile::tempdir().expect("tempdir");
    let (pk, sk) = keypair();
    let dir = store_dir(&root.path().join(MEMORY_ROOT), "default").expect("the store dir");
    (root, dir, pk, sk)
}

#[test]
fn a_signature_from_another_key_is_key_mismatch_never_bad_signature() {
    let (_root, dir, _pk, sk) = signed_dir();
    let (other_pk, _) = keypair();
    remember_signed(
        &dir,
        entry("default", 1_700_000_000_000, Integrity::trusted()),
        &sk,
    )
    .expect("the write lands");
    let verdicts = recall_verified(&dir, "default", &other_pk).expect("recall walks");
    assert_eq!(
        verdicts[0].verdict,
        RecallVerdict::Rejected(RejectReason::KeyMismatch),
        "honestly signed, but not by THIS key — named, like the sign.rs unknown-key class"
    );
}

#[test]
fn a_validly_signed_entry_replayed_across_stores_is_store_mismatch() {
    let root = tempfile::tempdir().expect("tempdir");
    let root = root.path().join(MEMORY_ROOT);
    let (pk, sk) = keypair();
    let a = store_dir(&root, "alpha").expect("the store dir");
    let written = remember_signed(
        &a,
        entry("alpha", 1_700_000_000_000, Integrity::trusted()),
        &sk,
    )
    .expect("the write lands");
    // The replay: copy the VERIFIED bytes into the sibling store's dir (the
    // signature still verifies — the crypto is honest about WHAT it says).
    let b = store_dir(&root, "beta").expect("the store dir");
    std::fs::create_dir_all(&b).expect("beta dir");
    std::fs::copy(
        a.join(crate::entry_file_name(&written)),
        b.join(crate::entry_file_name(&written)),
    )
    .expect("the replay lands");
    let verdicts = recall_verified(&b, "beta", &pk).expect("recall walks");
    assert_eq!(
        verdicts[0].verdict,
        RecallVerdict::Rejected(RejectReason::StoreMismatch),
        "a cross-store replay is rejected — context is part of the envelope"
    );
    // …and the fold counts it under beta, admits nothing.
    let fold = seal_fold(&root, &pk).expect("a memory root folds");
    let beta = fold["stores"]
        .as_array()
        .expect("stores")
        .iter()
        .find(|s| s["store"] == "beta")
        .expect("beta rides the fold");
    assert_eq!(beta["admitted"], serde_json::json!([]));
    assert_eq!(beta["rejected"], serde_json::json!(1));
}

#[test]
fn malformed_files_are_named_not_silent() {
    let (_root, dir, pk, _sk) = signed_dir();
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(dir.join("1700000000001-aaaaaaaaaaaaaaaa.json"), "{not json").expect("junk");
    std::fs::write(
        dir.join("1700000000002-bbbbbbbbbbbbbbbb.json"),
        r#"{"frame": {}, "label": {"kind": "trusted"}}"#,
    )
    .expect("half an envelope");
    // Non-entry files never ride the walk (temp siblings · notes).
    std::fs::write(dir.join(".1700000000001-aaaaaaaaaaaaaaaa.json.tmp"), "x").expect("tmp");
    std::fs::write(dir.join("README.txt"), "not an entry").expect("txt");
    let verdicts = recall_verified(&dir, "default", &pk).expect("recall walks");
    assert_eq!(verdicts.len(), 2, "only .json entry files ride");
    assert!(
        verdicts.iter().all(
            |v| v.verdict == RecallVerdict::Rejected(RejectReason::Malformed) && v.entry.is_none()
        ),
        "both garbage shapes are Malformed with no envelope: {verdicts:?}"
    );
}

#[test]
fn the_honest_empty_states() {
    let root = tempfile::tempdir().expect("tempdir");
    let (pk, _sk) = keypair();
    // A missing store dir is an EMPTY store, never an error.
    let missing = store_dir(&root.path().join(MEMORY_ROOT), "ghost").expect("the store dir");
    let verdicts = recall_verified(&missing, "ghost", &pk).expect("missing dir recalls empty");
    assert!(verdicts.is_empty());
    // No memory root (and a root with no stores) ⇒ NO fold (absent is honest).
    assert!(seal_fold(&root.path().join(MEMORY_ROOT), &pk).is_none());
    let empty_root = root.path().join(MEMORY_ROOT);
    std::fs::create_dir_all(&empty_root).expect("root");
    assert!(
        seal_fold(&empty_root, &pk).is_none(),
        "no stores, no attestation"
    );
    // A non-dir at the store path is a layout error, not a panic.
    let file_store = root.path().join(MEMORY_ROOT);
    std::fs::write(file_store.join("blocked"), "x").expect("a file rides the root");
    let err = recall_verified(&file_store.join("blocked"), "blocked", &pk)
        .expect_err("a file where a dir must be");
    assert!(matches!(err, StoreError::DirLayout { .. }), "{err}");
}

#[test]
fn the_digest_and_file_name_are_deterministic_and_preimage_exact() {
    let (_root, dir, _pk, sk) = signed_dir();
    let one = remember_signed(
        &dir,
        entry("default", 1_700_000_000_000, Integrity::trusted()),
        &sk,
    )
    .expect("one");
    // The JCS preimage is sorted-keys/compact, excludes the sig, and
    // carries the format version INSIDE (the signature commits to `v`).
    assert_eq!(
        one.preimage(),
        format!(
            "{{\"frame\":{{\"content\":\"fact default 1700000000000\"}},\"label\":{{\"kind\":\"trusted\"}},\"parents_digest\":\"\",\"run_id\":\"run-1\",\"store\":\"default\",\"ts_ms\":1700000000000,\"v\":1}}"
        ),
        "the ONE canonicalization voice (JCS · the proof layer's idiom)"
    );
    // Same fields ⇒ same digest ⇒ same file name (idempotent rewrite).
    let two = remember_signed(
        &dir,
        entry("default", 1_700_000_000_000, Integrity::trusted()),
        &sk,
    )
    .expect("two");
    assert_eq!(one.digest(), two.digest());
    assert_eq!(crate::entry_file_name(&one), crate::entry_file_name(&two));
    let count = std::fs::read_dir(&dir).expect("dir").count();
    assert_eq!(count, 1, "an identical rewrite lands on the same name");
    // The MemoryId is content-derived: stable across decodes.
    assert_eq!(one.memory_id(), two.memory_id());
}

#[test]
fn the_error_family_speaks_nika_605_with_the_one_voice() {
    use miette::Diagnostic as _;
    use nika_error::traits::NikaErrorCode as _;
    let io = StoreError::Io {
        reason: "x".to_owned(),
    };
    let ser = StoreError::Serialize {
        reason: "x".to_owned(),
    };
    let lay = StoreError::DirLayout {
        reason: "x".to_owned(),
    };
    for e in [&io, &ser, &lay] {
        assert_eq!(e.nika_code(), nika_error::codes::NIKA_605);
        let help = e.help().map(|h| h.to_string()).expect("help rides");
        assert!(help.contains("Signed-memory store"), "the 605 row: {help}");
    }
    assert!(io.is_transient(), "IO retries");
    assert!(
        !ser.is_transient() && !lay.is_transient(),
        "deterministic classes never retry"
    );
}

#[test]
fn reject_reasons_have_stable_wire_words() {
    let words = [
        (RejectReason::Unsigned, "unsigned"),
        (RejectReason::Malformed, "malformed"),
        (RejectReason::UnsupportedVersion, "unsupported_version"),
        (RejectReason::KeyMismatch, "key_mismatch"),
        (RejectReason::StoreMismatch, "store_mismatch"),
        (RejectReason::NameMismatch, "name_mismatch"),
        (RejectReason::BadSignature, "bad_signature"),
    ];
    for (reason, word) in words {
        assert_eq!(reason.as_str(), word);
        assert_eq!(reason.to_string(), word, "Display is the wire word");
    }
}

// ─── The kernel trait surface (ADR-078) ─────────────────────────────

fn store(dir: PathBuf, sk: minisign::SecretKey, pk: minisign::PublicKey) -> SignedMemoryStore {
    SignedMemoryStore::new(
        dir,
        "default".to_owned(),
        "run-1".to_owned(),
        Integrity::untrusted("inputs.q"),
        sk,
        pk,
        || 1_700_000_000_000,
    )
    .expect("the dir's basename is the store name")
}

fn accepts_memory_store<T: MemoryStore>(_: &T) {}

#[tokio::test]
async fn the_trait_surface_signs_verifies_and_forgets() {
    let (_root, dir, pk, sk) = signed_dir();
    let store = store(dir.clone(), sk, pk.clone());
    accepts_memory_store(&store); // the blanket super-trait holds (ADR-078)

    let frame = MemoryFrame::new("a butterfly fact", MemoryLevel::Semantic);
    let id = store.remember(frame).await.expect("remember signs");
    // The write carried the store's configured label INSIDE the signature.
    let verdicts = recall_verified(&dir, "default", &pk).expect("recall walks");
    assert_eq!(verdicts.len(), 1);
    assert_eq!(verdicts[0].verdict, RecallVerdict::Admitted);
    assert_eq!(
        verdicts[0]
            .entry
            .as_ref()
            .expect("the envelope")
            .label
            .source(),
        Some("inputs.q"),
        "the trait-path write stamps the store's label — inside the signature"
    );

    let hits = store
        .recall(RecallQuery::new("butterfly"))
        .await
        .expect("recall verifies");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, id, "the deterministic digest-id round-trips");
    assert_eq!(hits[0].content, "a butterfly fact");
    assert_eq!(hits[0].level, MemoryLevel::Semantic);

    // The filters bind: a miss recalls nothing.
    assert!(
        store
            .recall(RecallQuery::new("zebra"))
            .await
            .expect("recall")
            .is_empty()
    );
    let mut scoped = RecallQuery::new("butterfly");
    scoped.levels = Some(vec![MemoryLevel::Working]);
    assert!(
        store.recall(scoped).await.expect("recall").is_empty(),
        "level filter binds"
    );

    store.forget(id).await.expect("forget removes");
    assert!(
        store
            .recall(RecallQuery::new("butterfly"))
            .await
            .expect("recall")
            .is_empty()
    );
    store.forget(id).await.expect("forget is idempotent");
}

#[tokio::test]
async fn trait_recall_never_returns_a_rejected_entry() {
    let (_root, dir, pk, sk) = signed_dir();
    let store = store(dir.clone(), sk, pk);
    let frame = MemoryFrame::new("honest bytes", MemoryLevel::Working);
    store.remember(frame).await.expect("remember");
    // Tamper on disk behind the store's back.
    let path = std::fs::read_dir(&dir)
        .expect("dir")
        .next()
        .expect("one entry")
        .expect("entry")
        .path();
    let text = std::fs::read_to_string(&path).expect("the file");
    std::fs::write(&path, text.replacen("honest", "dishonest", 1)).expect("the edit");
    let hits = store
        .recall(RecallQuery::new("bytes"))
        .await
        .expect("recall");
    assert!(
        hits.is_empty(),
        "rejections are never hits — they stay named in recall_verified"
    );
}

#[test]
fn the_store_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<SignedMemoryStore>();
}

// ─── The swarm-batch classes (version dispatch · name binding · the
// fold's failure honesty · the layout/tenant/tier fail-closed trio) ───

#[test]
fn an_unknown_format_version_is_rejected_with_the_named_reason() {
    let (_root, dir, pk, sk) = signed_dir();
    let written = remember_signed(
        &dir,
        entry("default", 1_700_000_000_000, Integrity::trusted()),
        &sk,
    )
    .expect("the write lands");
    // A NEWER envelope: the honest shape with a `v` this version does
    // not know (the signature commits to `v`, so only a future engine
    // mints one honestly — decode names it, never masquerades it as a
    // signature failure).
    let mut value = written.file_value();
    value["v"] = serde_json::json!(2);
    std::fs::write(
        dir.join(crate::entry_file_name(&written)),
        serde_json::to_string_pretty(&value).expect("serializes"),
    )
    .expect("the rewrite lands");
    let verdicts = recall_verified(&dir, "default", &pk).expect("recall walks");
    assert_eq!(
        verdicts[0].verdict,
        RecallVerdict::Rejected(RejectReason::UnsupportedVersion),
        "v:2 is NAMED — never a masquerading bad_signature"
    );
    assert!(
        verdicts[0].entry.is_none(),
        "an unknown version does not decode"
    );
}

#[test]
fn a_verified_entry_riding_a_second_name_is_name_mismatch() {
    let root = tempfile::tempdir().expect("tempdir");
    let root = root.path().join(MEMORY_ROOT);
    let (pk, sk) = keypair();
    let dir = store_dir(&root, "default").expect("the store dir");
    let written = remember_signed(
        &dir,
        entry("default", 1_700_000_000_000, Integrity::trusted()),
        &sk,
    )
    .expect("the write lands");
    // The receipt-pinning attack: copy the VERIFIED bytes to a second
    // name — the signature still verifies, but the name is not the
    // entry's canonical name (ts + digest-prefix).
    std::fs::copy(
        dir.join(crate::entry_file_name(&written)),
        dir.join("1700000000099-c0a1c0a1c0a1c0a1.json"),
    )
    .expect("the copy lands");
    let verdicts = recall_verified(&dir, "default", &pk).expect("recall walks");
    assert_eq!(verdicts.len(), 2);
    let admitted = verdicts
        .iter()
        .filter(|v| v.verdict == RecallVerdict::Admitted)
        .count();
    let mismatched = verdicts
        .iter()
        .filter(|v| v.verdict == RecallVerdict::Rejected(RejectReason::NameMismatch))
        .count();
    assert_eq!(
        (admitted, mismatched),
        (1, 1),
        "the original admits, the copy is NAMED: {verdicts:?}"
    );
    // …and the fold pins the digest exactly ONCE beside the count (the
    // admitted set never dupes — the receipt names the verified SET).
    let fold = seal_fold(&root, &pk).expect("a memory root folds");
    assert_eq!(
        fold["stores"][0]["admitted"],
        serde_json::json!([written.digest()])
    );
    assert_eq!(fold["stores"][0]["rejected"], serde_json::json!(1));
}

#[test]
fn the_detailed_fold_names_each_rejection_and_sorts_the_admitted_set() {
    let root = tempfile::tempdir().expect("tempdir");
    let root = root.path().join(MEMORY_ROOT);
    let (pk, sk) = keypair();
    let dir = store_dir(&root, "default").expect("the store dir");
    let one = remember_signed(
        &dir,
        entry("default", 1_700_000_000_001, Integrity::trusted()),
        &sk,
    )
    .expect("one");
    let two = remember_signed(
        &dir,
        entry("default", 1_700_000_000_002, Integrity::trusted()),
        &sk,
    )
    .expect("two");
    let tampered = remember_signed(
        &dir,
        entry("default", 1_700_000_000_003, Integrity::trusted()),
        &sk,
    )
    .expect("three");
    let tampered_path = dir.join(crate::entry_file_name(&tampered));
    let text = std::fs::read_to_string(&tampered_path).expect("the entry reads");
    std::fs::write(&tampered_path, text.replacen("fact", "fAct", 1)).expect("the tamper lands");

    let (fold, rejections) = seal_fold_detailed(&root, &pk);
    let fold = fold.expect("a memory root folds");
    let mut digests = [one.digest(), two.digest()];
    digests.sort();
    assert_eq!(
        fold["stores"][0]["admitted"],
        serde_json::json!(digests),
        "the admitted SET is sorted"
    );
    assert_eq!(fold["stores"][0]["rejected"], serde_json::json!(1));
    assert_eq!(rejections.len(), 1, "one rejection, named for the journal");
    assert_eq!(rejections[0].store, "default");
    assert_eq!(rejections[0].reason, RejectReason::BadSignature);
    assert_eq!(rejections[0].path, tampered_path);
}

#[test]
fn the_fold_names_a_store_it_cannot_walk_never_collapses() {
    let root = tempfile::tempdir().expect("tempdir");
    let root = root.path().join(MEMORY_ROOT);
    let (pk, sk) = keypair();
    let good = store_dir(&root, "alpha").expect("the store dir");
    let written = remember_signed(
        &good,
        entry("alpha", 1_700_000_000_000, Integrity::trusted()),
        &sk,
    )
    .expect("the write lands");
    // The broken store: a DIRECTORY named like an entry file — reading
    // it fails on any platform (no chmod games), so the walk errors.
    let broken = store_dir(&root, "broken").expect("the store dir");
    std::fs::create_dir_all(broken.join("1700000000001-cccccccccccccccc.json"))
        .expect("the trap lands");
    let fold = seal_fold(&root, &pk).expect("a failed walk still folds — named, never None");
    assert_eq!(fold["v"], serde_json::json!(1));
    let stores = fold["stores"].as_array().expect("stores");
    assert_eq!(stores.len(), 2, "both stores ride: {stores:?}");
    assert_eq!(stores[0]["store"], serde_json::json!("alpha"));
    assert_eq!(stores[0]["admitted"], serde_json::json!([written.digest()]));
    assert_eq!(stores[1]["store"], serde_json::json!("broken"));
    assert!(
        stores[1]["error"].as_str().is_some(),
        "the failed walk rides as an explicit error entry: {stores:?}"
    );
    assert!(
        stores[1].get("admitted").is_none(),
        "no fabricated empty set beside the error"
    );
}

#[test]
fn an_unreadable_root_still_yields_a_fold_with_the_error_entry() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (pk, _sk) = keypair();
    let root = tmp.path().join(MEMORY_ROOT);
    std::fs::create_dir_all(root.parent().expect("the parent")).expect("the parent dir");
    std::fs::write(&root, "a file where the root must be").expect("the blocker");
    let fold =
        seal_fold(&root, &pk).expect("Some — the error is NAMED, never erased as 'no memory'");
    let entry = &fold["stores"][0];
    assert!(entry["store"].is_null(), "no store name to name: {entry}");
    assert!(entry["error"].as_str().is_some(), "{entry}");
}

#[cfg(unix)]
#[test]
fn a_non_utf8_store_name_still_folds() {
    use std::os::unix::ffi::OsStrExt as _;
    let tmp = tempfile::tempdir().expect("tempdir");
    let (pk, _sk) = keypair();
    let root = tmp.path().join(MEMORY_ROOT);
    // APFS (macOS) refuses non-UTF-8 names outright ("Illegal byte
    // sequence") — the lossy fold can only be exercised where the fs
    // permits the name at all (ext4 & co., CI's Linux). A refusal here
    // is the PLATFORM's honesty, never the fold's: skip, named.
    if std::fs::create_dir_all(root.join(std::ffi::OsStr::from_bytes(b"st\xffre"))).is_err() {
        return;
    }
    let fold = seal_fold(&root, &pk).expect("the store rides the fold");
    assert_eq!(
        fold["stores"][0]["store"],
        serde_json::json!("st\u{fffd}re"),
        "lossy, never dropped (recall works on the PathBuf)"
    );
    assert_eq!(fold["stores"][0]["rejected"], serde_json::json!(0));
}

#[test]
fn store_dir_rejects_names_that_escape_the_root() {
    let root = tempfile::tempdir().expect("tempdir");
    for bad in ["", "a/b", "a\\b", "..", "a..b"] {
        let err = store_dir(root.path(), bad).expect_err("not a single segment");
        assert!(
            matches!(err, StoreError::DirLayout { .. }),
            "{bad:?}: {err}"
        );
    }
    let ok = store_dir(root.path(), "default").expect("a clean name");
    assert_eq!(ok, root.path().join("default"));
}

#[test]
fn the_trait_store_pins_its_dir_basename_to_the_store_name() {
    let root = tempfile::tempdir().expect("tempdir");
    let (pk, sk) = keypair();
    // (`expect_err` needs the Ok side to be Debug — the store
    // deliberately carries none: the secret key rides it.)
    let Err(err) = SignedMemoryStore::new(
        root.path().join("alpha"),
        "beta".to_owned(),
        "run-1".to_owned(),
        Integrity::trusted(),
        sk,
        pk,
        || 0,
    ) else {
        panic!("the dir's basename is not the store name — must fail closed");
    };
    assert!(matches!(err, StoreError::DirLayout { .. }), "{err}");
}

#[test]
fn a_leftover_tmp_fails_the_commit_never_a_silent_clobber() {
    let (_root, dir, _pk, sk) = signed_dir();
    let first = remember_signed(
        &dir,
        entry("default", 1_700_000_000_000, Integrity::trusted()),
        &sk,
    )
    .expect("the first write lands");
    let name = crate::entry_file_name(&first);
    std::fs::remove_file(dir.join(&name)).expect("clear the lane");
    let tmp = dir.join(format!(".{name}.tmp"));
    std::fs::write(&tmp, "a crashed write's leftover").expect("the leftover plants");
    let err = remember_signed(
        &dir,
        entry("default", 1_700_000_000_000, Integrity::trusted()),
        &sk,
    )
    .expect_err("create_new refuses the clobber");
    assert!(matches!(err, StoreError::Io { .. }), "{err}");
    assert_eq!(
        std::fs::read_to_string(&tmp).expect("the leftover reads"),
        "a crashed write's leftover",
        "untouched — a planted SYMLINK at the tmp name fails the same way"
    );
}

#[test]
fn a_failed_rename_cleans_the_tmp_up() {
    let (_root, dir, _pk, sk) = signed_dir();
    let first = remember_signed(
        &dir,
        entry("default", 1_700_000_000_000, Integrity::trusted()),
        &sk,
    )
    .expect("the first write lands");
    let name = crate::entry_file_name(&first);
    std::fs::remove_file(dir.join(&name)).expect("clear the lane");
    std::fs::create_dir(dir.join(&name)).expect("a directory rides the final name");
    let err = remember_signed(
        &dir,
        entry("default", 1_700_000_000_000, Integrity::trusted()),
        &sk,
    )
    .expect_err("renaming over a directory fails");
    assert!(matches!(err, StoreError::Io { .. }), "{err}");
    assert!(
        !dir.join(format!(".{name}.tmp")).exists(),
        "the tmp never lingers to wedge later commits"
    );
}

#[tokio::test]
async fn a_non_default_tenant_scope_recalls_nothing_never_everything() {
    let (_root, dir, pk, sk) = signed_dir();
    let store = store(dir, sk, pk);
    store
        .remember(MemoryFrame::new("a butterfly fact", MemoryLevel::Semantic))
        .await
        .expect("remember");
    let foreign = RecallQuery::scoped(nika_error::id::TenantId::new("tenant-b"), "butterfly");
    assert!(
        store.recall(foreign).await.expect("recall").is_empty(),
        "ADR-031 fail-closed: a foreign keyspace never sweeps the default one"
    );
    assert_eq!(
        store
            .recall(RecallQuery::new("butterfly"))
            .await
            .expect("recall")
            .len(),
        1,
        "the default scope still binds"
    );
}

#[tokio::test]
async fn an_unknown_level_word_is_skipped_never_downgraded() {
    let (_root, dir, pk, sk) = signed_dir();
    remember_signed(
        &dir,
        UnsignedEntry::new(
            serde_json::json!({"content": "a butterfly fact", "level": "hyperbolic"}),
            Integrity::trusted(),
            "default".to_owned(),
            "run-1".to_owned(),
            1_700_000_000_000,
        ),
        &sk,
    )
    .expect("the honest-but-futuristic write signs");
    // The substrate ADMITS it (the signature is honest — the tier word
    // is the only unknown)…
    let verdicts = recall_verified(&dir, "default", &pk).expect("recall walks");
    assert_eq!(verdicts[0].verdict, RecallVerdict::Admitted);
    // …and the trait projection SKIPS it — never a silent Working downgrade.
    let store = store(dir, sk, pk);
    assert!(
        store
            .recall(RecallQuery::new("butterfly"))
            .await
            .expect("recall")
            .is_empty(),
        "an unknown tier is skipped, not downgraded"
    );
    // …while a real Working write still recalls AS Working (the word binds).
    store
        .remember(MemoryFrame::new("a working fact", MemoryLevel::Working))
        .await
        .expect("remember");
    let hits = store
        .recall(RecallQuery::new("working"))
        .await
        .expect("recall");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].level, MemoryLevel::Working);
}

/// Every level round-trips through the trait surface: the WRITE side
/// (`level_str`) stamps the level's wire word INSIDE the signed frame and
/// the READ side (`level_from_str`) decodes it back — a deleted arm on
/// either side surfaces as the wrong level or a skipped hit.
#[tokio::test]
async fn every_level_round_trips_through_the_trait_surface() {
    let (_root, dir, pk, sk) = signed_dir();
    let store = store(dir, sk, pk);
    for (i, level) in [
        MemoryLevel::Working,
        MemoryLevel::Episodic,
        MemoryLevel::Semantic,
        MemoryLevel::Procedural,
        MemoryLevel::Reflective,
        MemoryLevel::Conceptual,
    ]
    .into_iter()
    .enumerate()
    {
        store
            .remember(MemoryFrame::new(format!("level fact {i}"), level))
            .await
            .expect("remember");
        let hits = store
            .recall(RecallQuery::new(format!("level fact {i}")))
            .await
            .expect("recall");
        assert_eq!(hits.len(), 1, "level {level:?} recalls its hit");
        assert_eq!(hits[0].level, level, "level {level:?} round-trips");
    }
}

/// The tag filter binds BOTH ways: every wanted tag must ride the frame
/// (a missing tag filters the hit out, a full match lets it through).
#[tokio::test]
async fn the_tag_filter_binds_both_ways() {
    let (_root, dir, pk, sk) = signed_dir();
    let store = store(dir, sk, pk);
    let mut frame = MemoryFrame::new("a tagged fact", MemoryLevel::Semantic);
    frame.tags = vec!["butterfly".to_owned(), "run".to_owned()];
    store.remember(frame).await.expect("remember");
    let mut want = RecallQuery::new("tagged");
    want.tags = Some(vec!["butterfly".to_owned()]);
    assert_eq!(
        store.recall(want).await.expect("recall").len(),
        1,
        "a matching tag set admits"
    );
    let mut want = RecallQuery::new("tagged");
    want.tags = Some(vec!["butterfly".to_owned(), "zebra".to_owned()]);
    assert!(
        store.recall(want).await.expect("recall").is_empty(),
        "one missing tag filters the hit out"
    );
}

/// `dir::remove` is idempotent (a missing file is a no-op — the kernel
/// trait's CANCEL SAFETY contract) and names REAL errors (a directory is
/// not a file), never swallowing them as "already gone".
#[test]
fn remove_is_idempotent_and_names_real_errors() {
    let tmp = tempfile::tempdir().expect("tempdir");
    crate::dir::remove(&tmp.path().join("ghost.json")).expect("missing removes clean");
    let err = crate::dir::remove(tmp.path()).expect_err("a directory is not a file");
    assert!(matches!(err, StoreError::Io { .. }), "{err}");
}
