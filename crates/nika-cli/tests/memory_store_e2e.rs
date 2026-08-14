// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
// This suite executes the real `nika-cli` binary (CARGO_BIN_EXE) — the
// (d) case is the binary contract, so it spawns a process (the same
// sanctioned carve-out as quarantine_e2e.rs).
#![allow(clippy::disallowed_types)]

//! F-P8 (SMSR · signed memory) at the REAL binary — the substrate pairs
//! from the CLI crate's vantage (the API the L2 orchestrator will drive)
//! plus the seal integration:
//!
//! - **(a) the positive** — a signed write is admitted and its set digest
//!   lands in the fold (`remember_signed` · `recall_verified` ·
//!   `seal_fold` through the public API, hermetic keypair).
//! - **(b) the out-of-engine edit** — one byte flipped in the entry
//!   file's content field is REJECTED with the named reason
//!   (`bad_signature`), never admitted.
//! - **(c) the unsigned entry** — an envelope whose `sig` was never
//!   minted is REJECTED (`unsigned`) — the SMSR 0 %-admission floor.
//! - **(d) the seal** — a run whose CWD holds `.nika/memory/` seals its
//!   journal with `covers["memory"]` pinning the admitted set's ONE
//!   digest + the counts, and the law's third leg rides the SAME journal:
//!   one `memory_entry_rejected` event per rejection, landed BEFORE the
//!   `run_sealed` line so the chain covers it. v1 has no production
//!   memory writer, so the store is SEEDED before the run (signed with
//!   the same hermetic key the env files carry) — the teardown's
//!   `memory_attend` is exercised for real, end to end.
//! - **(e) the planted root** — a FILE at `.nika/memory` is never
//!   collapsed to "no memory" (H16): the seal names the unreadable root
//!   as an error entry in the covers.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use nika_cap::Integrity;
use nika_store::{
    MEMORY_ROOT, RecallVerdict, RejectReason, UnsignedEntry, recall_verified, remember_signed,
    seal_fold, store_dir,
};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nika-cli"))
}

/// A hermetic run-signing key pair on disk: the `NIKA_RUN_*_FILE` custody
/// path is checked FIRST (the CI door), so the OS keychain is never
/// touched. Returns the pair (for signing entries) + the env array.
fn signing_key(dir: &Path) -> (minisign::KeyPair, [(String, String); 3]) {
    let pair = minisign::KeyPair::generate_unencrypted_keypair().expect("keypair mints");
    let key = dir.join("run-signing.key");
    let pub_ = dir.join("run-signing.pub");
    std::fs::write(&key, pair.sk.to_box(None).expect("sk box").to_string()).expect("write key");
    std::fs::write(&pub_, pair.pk.to_box().expect("pk box").to_string()).expect("write pub");
    let envs = [
        (
            "NIKA_RUN_KEY_FILE".to_owned(),
            key.to_string_lossy().into_owned(),
        ),
        (
            "NIKA_RUN_PUB_FILE".to_owned(),
            pub_.to_string_lossy().into_owned(),
        ),
        // The minted box's password is the empty one — pin it so an
        // ambient operator setting never leaks into the rehearsal.
        ("NIKA_RUN_KEY_PASSWORD".to_owned(), String::new()),
    ];
    (pair, envs)
}

fn entry(store: &str, content: &str) -> UnsignedEntry {
    UnsignedEntry::new(
        serde_json::json!({"content": content}),
        Integrity::untrusted("fetch_page"),
        store.to_owned(),
        "run-e2e".to_owned(),
        1_700_000_000_000,
    )
}

/// The e2e-side re-construction of the fold's set digest (H13): sort +
/// dedup, blake3 over the concatenated 64-hex words. Written twice on
/// purpose — a divergence from the substrate's construction (the sort ·
/// the dedup · the framing) fails this cross-check.
fn set_digest(digests: &[String]) -> String {
    let mut sorted = digests.to_vec();
    sorted.sort();
    sorted.dedup();
    let mut hasher = blake3::Hasher::new();
    for digest in &sorted {
        hasher.update(digest.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

/// (a) The positive: a signed write is admitted and its set digest lands
/// in the fold.
#[test]
fn a_signed_write_is_admitted_and_its_digest_lands_in_the_fold() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (pair, _envs) = signing_key(tmp.path());
    let root = tmp.path().join(MEMORY_ROOT);
    let dir = store_dir(&root, "default").expect("the store dir");

    let written = remember_signed(&dir, entry("default", "the run saw a butterfly"), &pair.sk)
        .expect("the write signs + commits");
    let verdicts = recall_verified(&dir, "default", &pair.pk).expect("recall walks");
    assert_eq!(verdicts.len(), 1);
    assert_eq!(verdicts[0].verdict, RecallVerdict::Admitted);

    let fold = seal_fold(&root, &pair.pk).expect("a memory root folds");
    assert_eq!(fold["stores"][0]["store"], serde_json::json!("default"));
    assert_eq!(
        fold["stores"][0]["set_digest"],
        serde_json::json!(set_digest(&[written.digest()])),
        "the receipt names the verified SET — ONE constant-size digest (O(1))"
    );
    assert_eq!(fold["stores"][0]["admitted_count"], serde_json::json!(1));
    assert_eq!(fold["stores"][0]["rejected"], serde_json::json!(0));
}

/// (b) The out-of-engine edit: one flipped byte in the entry file's
/// content field is rejected with the NAMED reason — never admitted.
#[test]
fn a_direct_file_edit_is_rejected_with_the_named_reason() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (pair, _envs) = signing_key(tmp.path());
    let dir = store_dir(&tmp.path().join(MEMORY_ROOT), "default").expect("the store dir");

    let written =
        remember_signed(&dir, entry("default", "honest bytes"), &pair.sk).expect("the write lands");
    let path = dir.join(nika_store::entry_file_name(&written));
    let text = std::fs::read_to_string(&path).expect("the entry reads");
    std::fs::write(&path, text.replacen("honest", "h0nest", 1)).expect("the edit lands");

    let verdicts = recall_verified(&dir, "default", &pair.pk).expect("recall walks");
    assert_eq!(verdicts.len(), 1);
    let RecallVerdict::Rejected(reason) = &verdicts[0].verdict else {
        panic!("the edit must reject: {:?}", verdicts[0].verdict);
    };
    assert_eq!(
        *reason,
        RejectReason::BadSignature,
        "the named reason — a journaled `memory_entry_rejected` carries this word"
    );
    assert_eq!(reason.as_str(), "bad_signature");
}

/// (c) The unsigned entry: a well-shaped envelope whose `sig` was never
/// minted is rejected `unsigned` — 0 % admission (the theorem's floor).
#[test]
fn an_unsigned_entry_is_rejected_zero_admission() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (pair, _envs) = signing_key(tmp.path());
    let dir = store_dir(&tmp.path().join(MEMORY_ROOT), "default").expect("the store dir");

    let written = remember_signed(&dir, entry("default", "signed"), &pair.sk).expect("write");
    let mut forged = written.file_value();
    forged["sig"] = serde_json::json!("");
    std::fs::write(
        dir.join("1700000000099-0000000000000000.json"),
        serde_json::to_string_pretty(&forged).expect("serializes"),
    )
    .expect("the outside write lands");

    let verdicts = recall_verified(&dir, "default", &pair.pk).expect("recall walks");
    let admitted = verdicts
        .iter()
        .filter(|v| v.verdict == RecallVerdict::Admitted)
        .count();
    let unsigned = verdicts
        .iter()
        .filter(|v| v.verdict == RecallVerdict::Rejected(RejectReason::Unsigned))
        .count();
    assert_eq!((admitted, unsigned), (1, 1), "only the signed entry admits");
    let fold = seal_fold(&tmp.path().join(MEMORY_ROOT), &pair.pk).expect("fold");
    assert_eq!(fold["stores"][0]["rejected"], serde_json::json!(1));
}

// ─── (d) the seal integration at the real binary ─────────────────────

const WF: &str = r#"
nika: memory-store-e2e
permits:
  exec: ["echo"]
tasks:
  a:
    exec: { command: ["echo", "hi"] }
"#;

fn nika_run(dir: &Path, envs: &[(String, String)]) -> Output {
    bin()
        .args(["run", "w.nika.yaml", "--json", "--color", "never"])
        .current_dir(dir)
        .envs(envs.iter().map(|(k, v)| (k, v)))
        .output()
        .expect("binary runs")
}

/// The run's ONE journal (one invocation wrote exactly one).
fn journal(dir: &Path) -> PathBuf {
    std::fs::read_dir(dir.join(".nika/traces"))
        .expect("traces dir")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|x| x == "ndjson"))
        .expect("one journal")
}

/// The terminal `run_sealed` line's parsed `covers` object.
fn sealed_covers(journal: &Path) -> serde_json::Value {
    let text = std::fs::read_to_string(journal).expect("journal reads");
    let line = text
        .lines()
        .find(|l| l.contains("\"kind\":\"run_sealed\""))
        .expect("the journal sealed (the hermetic key rides)");
    let frame: serde_json::Value = serde_json::from_str(line).expect("one JSON event");
    let covers = frame["fields"]
        .as_array()
        .expect("fields")
        .iter()
        .find(|kv| kv["key"] == "covers")
        .and_then(|kv| kv["value"].as_str())
        .expect("the covers field");
    serde_json::from_str(covers).expect("covers parses")
}

/// (d) The seal: a run whose CWD holds `.nika/memory/` seals its journal
/// with `covers["memory"]` — the admitted set's ONE digest beside the
/// counts. The store is SEEDED (v1 has no production writer): one honest
/// entry + one tampered entry, signed/edited with the same hermetic key
/// the env files carry, so the teardown's custody probe stays off the
/// OS keychain.
#[test]
fn the_seal_covers_carry_the_memory_fold() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (pair, envs) = signing_key(tmp.path());
    std::fs::write(tmp.path().join("w.nika.yaml"), WF).expect("workflow");

    // Seed the store: one honest entry…
    let dir = store_dir(&tmp.path().join(MEMORY_ROOT), "default").expect("the store dir");
    let honest = remember_signed(&dir, entry("default", "a fact the run signed"), &pair.sk)
        .expect("the honest write lands");
    // …and the same entry with one byte flipped on disk (the edit an
    // out-of-engine writer can make — the key it never had stays missing).
    let tampered = remember_signed(&dir, entry("default", "a second fact"), &pair.sk)
        .expect("the second write lands");
    let tampered_path = dir.join(nika_store::entry_file_name(&tampered));
    let text = std::fs::read_to_string(&tampered_path).expect("the entry reads");
    std::fs::write(&tampered_path, text.replacen("second", "sec0nd", 1)).expect("the tamper lands");

    let run = nika_run(tmp.path(), &envs);
    assert_eq!(
        run.status.code(),
        Some(0),
        "the run completes · stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    let covers = sealed_covers(&journal(tmp.path()));
    let memory = &covers["memory"];
    assert!(
        memory.is_object(),
        "the memory fold rides the seal's covers: {covers}"
    );
    assert_eq!(memory["v"], serde_json::json!(1), "the fold is versioned");
    assert_eq!(memory["stores"][0]["store"], serde_json::json!("default"));
    assert_eq!(
        memory["stores"][0]["set_digest"],
        serde_json::json!(set_digest(&[honest.digest()])),
        "the seal pins the set's ONE digest (the tampered entry never rides)"
    );
    assert_eq!(
        memory["stores"][0]["admitted_count"],
        serde_json::json!(1),
        "…the set's size beside it"
    );
    assert_eq!(
        memory["stores"][0]["rejected"],
        serde_json::json!(1),
        "…and the rejection count beside it"
    );

    // The law's third leg: each rejection is JOURNALED — one
    // `memory_entry_rejected` event naming store + reason — BEFORE the
    // seal, so the chain the seal signs covers the evidence it attests
    // the count of.
    let text = std::fs::read_to_string(journal(tmp.path())).expect("journal reads");
    let lines: Vec<&str> = text.lines().collect();
    let rejected: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| {
            l.contains("\"kind\":\"memory_entry_rejected\"")
                .then_some(i)
        })
        .collect();
    assert_eq!(
        rejected.len(),
        1,
        "one rejection, one journaled event: {text}"
    );
    let event = lines[rejected[0]];
    assert!(event.contains("bad_signature"), "the reason word: {event}");
    assert!(event.contains("default"), "the store word: {event}");
    let sealed_at = lines
        .iter()
        .position(|l| l.contains("\"kind\":\"run_sealed\""))
        .expect("the journal sealed");
    assert!(
        rejected[0] < sealed_at,
        "the evidence lands BEFORE the seal — the chain covers it"
    );

    // The honest posture, mirrored: the substrate itself agrees with what
    // the seal attests (the fold the teardown computed IS this fold).
    let fold = seal_fold(&tmp.path().join(MEMORY_ROOT), &pair.pk).expect("fold");
    assert_eq!(
        *memory, fold,
        "the seal's attestation IS the substrate's fold"
    );
}

/// The clean posture: a run whose CWD has NO `.nika/memory/` attests
/// NOTHING — the `memory` key stays OUT of the covers (absent is honest).
#[test]
fn a_run_without_a_memory_store_seals_no_memory_key() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_pair, envs) = signing_key(tmp.path());
    std::fs::write(tmp.path().join("w.nika.yaml"), WF).expect("workflow");
    let run = nika_run(tmp.path(), &envs);
    assert_eq!(
        run.status.code(),
        Some(0),
        "the run completes · stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let covers = sealed_covers(&journal(tmp.path()));
    assert!(
        covers.get("memory").is_none(),
        "the key stays OUT — a store-less run attests nothing: {covers}"
    );
}

/// (e) H16 · the planted root: a FILE at `.nika/memory` is NOT collapsed
/// to "no memory" (a plant must never pass as honest absence) — the seal
/// names the unreadable root as an error entry in the covers.
#[test]
fn a_planted_file_at_the_memory_root_is_named_never_absent() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_pair, envs) = signing_key(tmp.path());
    std::fs::write(tmp.path().join("w.nika.yaml"), WF).expect("workflow");
    // The plant: a regular file where the memory root must be a dir.
    std::fs::create_dir_all(tmp.path().join(".nika")).expect("the .nika dir");
    std::fs::write(tmp.path().join(MEMORY_ROOT), "not a directory").expect("the plant lands");

    let run = nika_run(tmp.path(), &envs);
    assert_eq!(
        run.status.code(),
        Some(0),
        "the run completes · stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let covers = sealed_covers(&journal(tmp.path()));
    let entry = &covers["memory"]["stores"][0];
    assert!(
        entry["store"].is_null(),
        "no store name to name — the error denies it: {entry}"
    );
    assert!(
        entry["error"].as_str().is_some(),
        "the unreadable root rides as an error entry, never as absence: {entry}"
    );
}
