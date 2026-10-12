// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
// This suite executes the real `nika-cli` binary (CARGO_BIN_EXE) — its
// whole job is the binary contract, so it spawns processes (the same
// sanctioned carve-out as resume_e2e.rs / trace_export_e2e.rs).
#![allow(clippy::disallowed_types)]

//! F-P14 (NEP-0017 · « obligation de fin — la dette du run », sous le mot
//! réservé `finally`) at the REAL binary — palier v1, the quarantine:
//!
//! - **(a) the positive** — a FAILED run whose writer tasks settled
//!   Success sees the files it CREATED moved under
//!   `.nika/quarantine/<run-stamp>/`, while a person's own file it only
//!   edited stays in place with its original kept under `originals/`;
//!   both attested on the terminal `run_sealed` line's
//!   `covers["quarantine"]` (the F-P2 teardown's receipt surface — F-P2
//!   proves THAT the end happened, F-P14 says WHAT it must do) and in the
//!   dir's `manifest.json`.
//! - **(b) the clean posture** — a SUCCESSFUL run attests NOTHING: the
//!   `quarantine` key stays OUT of the covers (absent is honest).
//! - **(c) the negative acceptance (v1 semantics)** — a follow-up run
//!   reading the OLD path fails LOUD (`NIKA-BUILTIN-READ-001`): the move
//!   is what keeps a semi-written artifact from re-entering as the next
//!   run's input. (The true check-side cross-run finding — prior
//!   quarantine lists × the next workflow's read paths — is the named
//!   v2 owe.)
//! - **(d) resume** — resuming the failed run puts its moved files back
//!   before the run starts (its cache hits name them as written).
//! - **(e) the failure path** — a writer admitted by `after: failure`
//!   writes the record of the failure: its file stays.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nika"))
}

/// A hermetic run-signing key pair on disk (minisign is a dev-dep): the
/// seal — and with it the quarantine attestation — only lands when a
/// key exists. The `NIKA_RUN_*_FILE` custody path is checked FIRST (the
/// CI door), so the OS keychain is never touched.
fn signing_key(dir: &Path) -> [(String, String); 3] {
    let pair = minisign::KeyPair::generate_unencrypted_keypair().expect("keypair mints");
    let key = dir.join("run-signing.key");
    let pub_ = dir.join("run-signing.pub");
    std::fs::write(&key, pair.sk.to_box(None).expect("sk box").to_string()).expect("write key");
    std::fs::write(&pub_, pair.pk.to_box().expect("pk box").to_string()).expect("write pub");
    [
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
    ]
}

fn nika_run(dir: &Path, wf: &str, envs: &[(String, String)]) -> Output {
    bin()
        .args(["run", wf, "--json", "--color", "never"])
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

/// The terminal `run_sealed` line's parsed `covers` object (the receipt
/// surface the quarantine fold rides).
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

/// The failing workflow: two writers settle Success (`first` writes,
/// `fix` edits a pre-existing note), then `boom` fails the run AFTER
/// them — the semi-written debt of F-P14.
const FAIL_WF: &str = r#"
nika: quarantine-e2e
permits:
  tools: ["nika:write", "nika:edit"]
  fs: { write: ["semi.txt", "note.txt"], read: ["note.txt"] }
  exec: ["false"]
tasks:
  first:
    invoke: { tool: "nika:write", args: { path: "semi.txt", content: "written before the crash" } }
  fix:
    invoke: { tool: "nika:edit", args: { path: "note.txt", find: "draft", replace: "final" } }
  boom:
    after: { first: success, fix: success }
    exec: { command: ["false"] }
"#;

/// Seed the failing rehearsal: the workflow + the note `fix` edits.
fn fail_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tmpdir");
    std::fs::write(dir.path().join("w.nika"), FAIL_WF).expect("workflow");
    std::fs::write(dir.path().join("note.txt"), "a draft note").expect("the edit target");
    dir
}

/// (a) The positive: the failed run's semi-written outputs MOVE under
/// `.nika/quarantine/<run-stamp>/` and the fold rides the seal's covers.
#[test]
fn a_failed_run_quarantines_its_semi_written_outputs_and_attests_them() {
    let dir = fail_dir();
    let envs = signing_key(dir.path());
    let run = nika_run(dir.path(), "w.nika", &envs);
    assert_eq!(
        run.status.code(),
        Some(1),
        "the workflow failed · stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // The MOVE (the substance): the file the run created is gone, its
    // bytes live under the quarantine dir named by the journal's own stamp.
    let covers = sealed_covers(&journal(dir.path()));
    let fold = &covers["quarantine"];
    let qdir = fold["dir"].as_str().expect("the fold names its dir");
    assert!(
        qdir.starts_with(".nika/quarantine/"),
        "the debt rides beside the journals: {qdir}"
    );
    assert!(
        !dir.path().join("semi.txt").exists(),
        "the created file's old path no longer exists"
    );
    let moved = dir.path().join(qdir);
    assert_eq!(
        std::fs::read_to_string(moved.join("semi.txt")).expect("the write moved"),
        "written before the crash"
    );
    // The person's own note never moves: it stays edited where they keep
    // it, and its original waits under `originals/`.
    assert_eq!(
        std::fs::read_to_string(dir.path().join("note.txt")).expect("the note stays"),
        "a final note"
    );
    assert_eq!(
        std::fs::read_to_string(moved.join("originals/note.txt")).expect("the original kept"),
        "a draft note"
    );
    assert!(
        !dir.path()
            .join(".nika/originals")
            .read_dir()
            .is_ok_and(|mut d| d.next().is_some()),
        "the staged originals are released once the run settled"
    );

    // The ATTESTATION: the created file with its target, the kept one
    // with its original, and the same fold in the dir's manifest.
    let outputs = fold["outputs"].as_array().expect("the outputs fold");
    assert_eq!(outputs.len(), 1, "only the created file moved: {fold}");
    assert_eq!(outputs[0]["path"], serde_json::json!("semi.txt"));
    assert_eq!(
        outputs[0]["quarantined_to"],
        serde_json::json!(format!("{qdir}/semi.txt"))
    );
    assert_eq!(fold["kept"][0]["path"], serde_json::json!("note.txt"));
    assert_eq!(
        fold["kept"][0]["original"],
        serde_json::json!(format!("{qdir}/originals/note.txt"))
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(moved.join("manifest.json")).expect("manifest"))
            .expect("manifest parses");
    assert_eq!(manifest["outputs"], fold["outputs"]);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("note.txt stays in place") && stderr.contains("semi.txt"),
        "the person is told where their files are: {stderr}"
    );
}

/// (d) Resuming the failed run puts the file it created back before the
/// run starts: the resumed run's cache hit names it as written.
#[test]
fn resuming_the_failed_run_puts_its_files_back() {
    let dir = fail_dir();
    let envs = signing_key(dir.path());
    let first = nika_run(dir.path(), "w.nika", &envs);
    assert_eq!(first.status.code(), Some(1), "the debt run failed");
    assert!(!dir.path().join("semi.txt").exists(), "quarantined");
    let trace = journal(dir.path());

    let resumed = bin()
        .args(["run", "w.nika", "--json", "--color", "never", "--resume"])
        .arg(&trace)
        .current_dir(dir.path())
        .envs(envs.iter().map(|(k, v)| (k, v)))
        .output()
        .expect("binary runs");
    let stderr = String::from_utf8_lossy(&resumed.stderr);
    assert!(
        stderr.contains("put back semi.txt"),
        "the restore is said: {stderr}"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("semi.txt")).expect("back in place"),
        "written before the crash"
    );
}

/// (e) A writer on the failure path writes the record of the failure: its
/// file is the point, never the debt.
#[test]
fn a_failure_path_writer_keeps_its_file() {
    let dir = tempfile::tempdir().expect("tmpdir");
    std::fs::write(
        dir.path().join("w.nika"),
        r#"
nika: quarantine-failure-path
permits:
  tools: ["nika:write"]
  fs: { write: ["prep.txt", "error.txt"] }
  exec: ["false"]
tasks:
  prep:
    invoke: { tool: "nika:write", args: { path: "prep.txt", content: "half the job" } }
  boom:
    after: { prep: success }
    exec: { command: ["false"] }
  report:
    after: { boom: failure }
    invoke: { tool: "nika:write", args: { path: "error.txt", content: "boom failed" } }
"#,
    )
    .expect("workflow");
    let envs = signing_key(dir.path());
    let run = nika_run(dir.path(), "w.nika", &envs);
    assert_eq!(
        run.status.code(),
        Some(1),
        "the workflow failed · stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        !dir.path().join("prep.txt").exists(),
        "the quarantine ran: the half-done file moved"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("error.txt")).expect("the report stays"),
        "boom failed"
    );
}

/// (b) The clean posture: a SUCCESSFUL run attests NOTHING — the
/// `quarantine` key stays OUT of the covers (absent is honest), and the
/// written output stays exactly where the workflow put it.
#[test]
fn a_successful_run_attests_no_quarantine() {
    let dir = tempfile::tempdir().expect("tmpdir");
    std::fs::write(
        dir.path().join("w.nika"),
        r#"
nika: quarantine-clean
permits:
  tools: ["nika:write"]
  fs: { write: ["keep.txt"] }
tasks:
  first:
    invoke: { tool: "nika:write", args: { path: "keep.txt", content: "all good" } }
"#,
    )
    .expect("workflow");
    let envs = signing_key(dir.path());
    let run = nika_run(dir.path(), "w.nika", &envs);
    assert_eq!(
        run.status.code(),
        Some(0),
        "the run completes · stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("keep.txt")).expect("untouched"),
        "all good",
        "a clean run's outputs never move"
    );
    assert!(
        !dir.path().join(".nika/quarantine").exists(),
        "no debt, no quarantine dir"
    );
    let covers = sealed_covers(&journal(dir.path()));
    assert!(
        covers.get("quarantine").is_none(),
        "the key stays OUT — a clean run attests nothing: {covers}"
    );
}

/// (c) The negative acceptance, v1 semantics: after the quarantine, a
/// follow-up run whose reader names the OLD path fails LOUD
/// (`NIKA-BUILTIN-READ-001`) — the semi-written artifact cannot
/// re-enter as the next run's input.
#[test]
fn the_next_run_reading_the_old_path_fails_loud() {
    let dir = fail_dir();
    let envs = signing_key(dir.path());
    let first = nika_run(dir.path(), "w.nika", &envs);
    assert_eq!(first.status.code(), Some(1), "the debt run failed");
    assert!(
        !dir.path().join("semi.txt").exists(),
        "quarantined — the old path is gone"
    );

    std::fs::write(
        dir.path().join("reader.nika"),
        r#"
nika: quarantine-reader
permits:
  tools: ["nika:read"]
  fs: { read: ["semi.txt"] }
tasks:
  consume:
    invoke: { tool: "nika:read", args: { path: "semi.txt" } }
"#,
    )
    .expect("reader workflow");
    let second = nika_run(dir.path(), "reader.nika", &envs);
    let stdout = String::from_utf8(second.stdout).expect("utf8");
    assert_eq!(
        second.status.code(),
        Some(1),
        "the reader run fails, never silently consumes: {stdout}"
    );
    assert!(
        stdout.contains("NIKA-BUILTIN-READ-001"),
        "file not found, said LOUD: {stdout}"
    );
}
