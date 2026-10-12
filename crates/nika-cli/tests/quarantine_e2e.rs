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
/// run starts: the resumed run's cache hit names it as written. The file
/// is the resumed run's own from then on, so when that run fails too
/// (`boom` fails again), it moves to the resumed run's quarantine with its
/// bytes instead of passing for a whole one.
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
    assert_eq!(resumed.status.code(), Some(1), "boom fails again: {stderr}");
    let latest = journals(dir.path())
        .into_iter()
        .find(|other| *other != trace)
        .expect("the resumed run keeps its own journal");
    let fold = &sealed_covers(&latest)["quarantine"];
    let moved = fold["outputs"]
        .as_array()
        .and_then(|outputs| outputs.iter().find(|o| o["path"] == "semi.txt"))
        .and_then(|o| o["quarantined_to"].as_str())
        .unwrap_or_else(|| panic!("the file put back moved again: {fold}"));
    assert!(
        moved.contains(&stamp(&latest)),
        "{moved} is the resumed run's own quarantine"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join(moved)).expect("its bytes moved with it"),
        "written before the crash"
    );
    assert!(
        !dir.path().join("semi.txt").exists(),
        "nothing passes for a whole file"
    );
}

/// Every journal of the project, oldest stamp first.
fn journals(dir: &Path) -> Vec<PathBuf> {
    let mut all: Vec<PathBuf> = std::fs::read_dir(dir.join(".nika/traces"))
        .expect("traces dir")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "ndjson"))
        .collect();
    all.sort();
    all
}

/// The `task` field of every frame of `kind` in a journal, in order.
fn tasks_of(journal: &Path, kind: &str) -> Vec<String> {
    let text = std::fs::read_to_string(journal).expect("journal reads");
    text.lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|frame| frame["kind"] == kind)
        .filter_map(|frame| {
            frame["fields"]
                .as_array()?
                .iter()
                .find(|kv| kv["key"] == "task")?["value"]
                .as_str()
                .map(str::to_owned)
        })
        .collect()
}

/// The artifact list: every file under the project, relative and sorted,
/// but the journals and the hermetic signing key pair.
fn artifact_list(dir: &Path) -> Vec<String> {
    fn walk(root: &Path, at: &Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(at)
            .expect("dir reads")
            .filter_map(Result::ok)
        {
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let rel = path.strip_prefix(root).expect("under the root");
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.retain(|p| {
        !p.starts_with(".nika/traces/") && p != "run-signing.key" && p != "run-signing.pub"
    });
    out.sort();
    out
}

/// The stamp a journal gives its quarantine dir: its file stem.
fn stamp(journal: &Path) -> String {
    journal
        .file_stem()
        .expect("a journal file")
        .to_string_lossy()
        .into_owned()
}

/// A run that writes `report.md`, then fails until a `go` file exists.
const GATE_WF: &str = r#"
nika: quarantine-refused-restore
permits:
  tools: ["nika:write"]
  fs: { write: ["report.md"] }
  exec: ["test"]
tasks:
  report:
    invoke: { tool: "nika:write", args: { path: "report.md", content: "the run's report" } }
  gate:
    after: { report: success }
    exec: { command: ["test", "-f", "go"] }
"#;

/// (f) A refused restore: the person put their own `report.md` where the
/// failed run's file must go back. The resumed run never counts `report`
/// completed while the run's bytes sit in quarantine: it runs again, says
/// so, and writes the run's report anew.
#[test]
fn a_refused_restore_runs_its_task_again() {
    let dir = tempfile::tempdir().expect("tmpdir");
    std::fs::write(dir.path().join("w.nika"), GATE_WF).expect("workflow");
    let envs = signing_key(dir.path());
    let first = nika_run(dir.path(), "w.nika", &envs);
    assert_eq!(first.status.code(), Some(1), "the gate failed the run");
    let trace = journal(dir.path());
    let qdir = format!(".nika/quarantine/{}", stamp(&trace));
    std::fs::write(dir.path().join("report.md"), "the person's own report").expect("theirs");
    std::fs::write(dir.path().join("go"), "").expect("the cause fixed");

    let resumed = bin()
        .args(["run", "w.nika", "--json", "--color", "never", "--resume"])
        .arg(&trace)
        .current_dir(dir.path())
        .envs(envs.iter().map(|(k, v)| (k, v)))
        .output()
        .expect("binary runs");

    let stderr = String::from_utf8_lossy(&resumed.stderr);
    assert_eq!(
        resumed.status.code(),
        Some(0),
        "the resumed run completes: {stderr}"
    );
    assert!(
        stderr.contains(&format!("report.md stays in {qdir}"))
            && stderr.contains("report runs again"),
        "the refusal and the re-run are said: {stderr}"
    );
    let all = journals(dir.path());
    let second = all
        .iter()
        .find(|j| **j != trace)
        .expect("the resumed journal");
    assert_eq!(tasks_of(second, "task_cache_hit"), Vec::<String>::new());
    assert_eq!(tasks_of(second, "task_completed"), ["report", "gate"]);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("report.md")).expect("rewritten"),
        "the run's report"
    );
    assert_eq!(
        artifact_list(dir.path()),
        [
            format!("{qdir}/manifest.json"),
            format!("{qdir}/report.md"),
            "go".to_owned(),
            "report.md".to_owned(),
            "w.nika".to_owned(),
        ],
        "the first run's file stays in its quarantine; nothing staged is left"
    );
}

/// Three changes to the person's notes, in one run that then fails.
const OVERWRITES_WF: &str = r#"
nika: quarantine-overwrites
permits:
  tools: ["nika:write", "nika:edit"]
  fs: { write: ["notes.md"], read: ["notes.md"] }
  exec: ["false"]
tasks:
  one:
    invoke: { tool: "nika:edit", args: { path: "notes.md", find: "v0", replace: "v1" } }
  two:
    after: { one: success }
    invoke: { tool: "nika:edit", args: { path: "notes.md", find: "v1", replace: "v2" } }
  three:
    after: { two: success }
    invoke: { tool: "nika:write", args: { path: "notes.md", content: "v3 written over" } }
  boom:
    after: { three: success }
    exec: { command: ["false"] }
"#;

/// (g) Several overwrites of one file: the original kept is the person's
/// bytes from BEFORE the first one, never an intermediate state; the file
/// itself stays where they keep it, with the run's last bytes.
#[test]
fn several_overwrites_keep_the_bytes_before_the_first() {
    let dir = tempfile::tempdir().expect("tmpdir");
    std::fs::write(dir.path().join("w.nika"), OVERWRITES_WF).expect("workflow");
    std::fs::write(dir.path().join("notes.md"), "my notes v0").expect("the person's file");
    let envs = signing_key(dir.path());
    let run = nika_run(dir.path(), "w.nika", &envs);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert_eq!(run.status.code(), Some(1), "the run failed: {stderr}");
    let trace = journal(dir.path());
    let qdir = format!(".nika/quarantine/{}", stamp(&trace));

    assert_eq!(
        artifact_list(dir.path()),
        [
            format!("{qdir}/manifest.json"),
            format!("{qdir}/originals/notes.md"),
            "notes.md".to_owned(),
            "w.nika".to_owned(),
        ]
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join(&qdir).join("originals/notes.md")).expect("kept"),
        "my notes v0"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("notes.md")).expect("in place"),
        "v3 written over"
    );
    assert_eq!(tasks_of(&trace, "task_completed"), ["one", "two", "three"]);
    let fold = &sealed_covers(&trace)["quarantine"];
    assert_eq!(
        fold["outputs"],
        serde_json::json!([]),
        "nothing the run created"
    );
    assert_eq!(fold["kept"][0]["path"], serde_json::json!("notes.md"));
    assert!(
        stderr.contains(&format!(
            "notes.md stays in place; its original is at {qdir}/originals/notes.md"
        )),
        "{stderr}"
    );
    // Journal before seal: the frame sits inside the chain the seal signs,
    // and its digest is the manifest's bytes.
    let kinds = kinds_of(&trace);
    assert_eq!(
        kinds[kinds.len() - 3..],
        ["workflow_failed", "quarantine_recorded", "run_sealed"]
    );
    let recorded = recorded_frame(&trace);
    assert_eq!(field(&recorded, "moved"), 0);
    assert_eq!(field(&recorded, "kept"), 1);
    assert_eq!(field(&recorded, "missed"), 0);
    assert_eq!(field(&recorded, "dir"), serde_json::json!(qdir));
    assert_eq!(
        field(&recorded, "manifest_sha256"),
        serde_json::json!(sha256_of(&dir.path().join(&qdir).join("manifest.json")))
    );
}

/// Every frame kind of a journal, in order.
fn kinds_of(journal: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(journal).expect("journal reads");
    text.lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter_map(|frame| frame["kind"].as_str().map(str::to_owned))
        .collect()
}

/// The journal's one `quarantine_recorded` frame.
fn recorded_frame(journal: &Path) -> serde_json::Value {
    let text = std::fs::read_to_string(journal).expect("journal reads");
    text.lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|frame| frame["kind"] == "quarantine_recorded")
        .expect("the quarantine is journaled")
}

/// A frame's field value by key (`null` when absent).
fn field(frame: &serde_json::Value, key: &str) -> serde_json::Value {
    frame["fields"]
        .as_array()
        .and_then(|fields| fields.iter().find(|kv| kv["key"] == key))
        .map_or(serde_json::Value::Null, |kv| kv["value"].clone())
}

/// The lowercase sha256 hex of a file's bytes.
fn sha256_of(path: &Path) -> String {
    use sha2::Digest as _;
    format!(
        "{:x}",
        sha2::Sha256::digest(std::fs::read(path).expect("file reads"))
    )
}

/// (h) Journal before seal, unsigned: with no run-signing key there is no
/// seal, and the journal still records what the teardown did with the
/// files (before, it said nothing at all). HOME is the scratch dir, so no
/// key of the machine is ever read.
#[test]
fn an_unsigned_failed_run_still_journals_its_quarantine() {
    let dir = fail_dir();
    let home = tempfile::tempdir().expect("scratch home");
    let envs = [
        (
            "HOME".to_owned(),
            home.path().to_string_lossy().into_owned(),
        ),
        ("NIKA_KEYCHAIN".to_owned(), "off".to_owned()),
        (
            "NIKA_RUN_KEY_FILE".to_owned(),
            dir.path().join("absent.key").to_string_lossy().into_owned(),
        ),
    ];
    let run = nika_run(dir.path(), "w.nika", &envs);
    assert_eq!(
        run.status.code(),
        Some(1),
        "the workflow failed · stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let trace = journal(dir.path());
    let qdir = format!(".nika/quarantine/{}", stamp(&trace));
    let kinds = kinds_of(&trace);
    assert!(!kinds.iter().any(|k| k == "run_sealed"), "no key, no seal");
    assert_eq!(
        kinds.last().map(String::as_str),
        Some("quarantine_recorded")
    );
    let recorded = recorded_frame(&trace);
    assert_eq!(field(&recorded, "moved"), 1);
    assert_eq!(field(&recorded, "kept"), 1);
    assert_eq!(
        field(&recorded, "manifest"),
        serde_json::json!(format!("{qdir}/manifest.json"))
    );
    assert_eq!(
        field(&recorded, "manifest_sha256"),
        serde_json::json!(sha256_of(&dir.path().join(&qdir).join("manifest.json")))
    );
    assert_eq!(
        artifact_list(dir.path()),
        [
            format!("{qdir}/manifest.json"),
            format!("{qdir}/originals/note.txt"),
            format!("{qdir}/semi.txt"),
            "note.txt".to_owned(),
            "w.nika".to_owned(),
        ]
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

/// The run a crash interrupts: it edits the person's note, creates the
/// report, then sleeps (the kill lands there) and fails unless `go` exists.
#[cfg(unix)]
const CRASH_WF: &str = r#"
nika: quarantine-crash
permits:
  tools: ["nika:write", "nika:edit"]
  fs: { write: ["note.txt", "report.md"], read: ["note.txt"] }
  exec: ["sleep", "test"]
tasks:
  fix:
    invoke: { tool: "nika:edit", args: { path: "note.txt", find: "draft", replace: "final" } }
  report:
    after: { fix: success }
    invoke: { tool: "nika:write", args: { path: "report.md", content: "the report" } }
  wait:
    after: { report: success }
    exec: { command: ["sleep", "5"] }
  gate:
    after: { wait: success }
    exec: { command: ["test", "-f", "go"] }
"#;

/// Any other run in the same project, which settles the crash at its start.
#[cfg(unix)]
const NEXT_WF: &str = r#"
nika: quarantine-next
permits:
  exec: ["true"]
tasks:
  noop:
    exec: { command: ["true"] }
"#;

/// Seed the crash rehearsal: both workflows and the person's note.
#[cfg(unix)]
fn crash_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tmpdir");
    std::fs::write(dir.path().join("w.nika"), CRASH_WF).expect("workflow");
    std::fs::write(dir.path().join("next.nika"), NEXT_WF).expect("next workflow");
    std::fs::write(dir.path().join("note.txt"), "a draft note").expect("the person's note");
    dir
}

/// Run `w.nika` and SIGKILL it while `wait` sleeps: the run dies before its
/// teardown, as a machine that went down would leave it. The journal's
/// `task_completed` for `report` marks the moment (one flushed line each).
#[cfg(unix)]
fn run_killed_in_its_sleep(dir: &Path, envs: &[(String, String)]) {
    let mut child = bin()
        .args(["run", "w.nika", "--json", "--color", "never"])
        .current_dir(dir)
        .envs(envs.iter().map(|(k, v)| (k, v)))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("binary spawns");
    let started = std::time::Instant::now();
    let reported = || {
        dir.join(".nika/traces").is_dir()
            && journals(dir).first().is_some_and(|journal| {
                tasks_of(journal, "task_completed").contains(&"report".to_owned())
            })
    };
    while !reported() {
        assert!(
            started.elapsed() < std::time::Duration::from_secs(60),
            "the report never completed"
        );
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    // Inside the five-second sleep that follows the report.
    std::thread::sleep(std::time::Duration::from_millis(300));
    child.kill().expect("SIGKILL");
    let _ = child.wait();
}

/// The crash's leftovers: the staging records and the note's original.
#[cfg(unix)]
fn staged_leftovers(dir: &Path) -> Vec<String> {
    artifact_list(dir)
        .into_iter()
        .filter(|p| p.starts_with(".nika/originals/"))
        .map(|p| p.rsplit('/').next().unwrap_or_default().to_owned())
        .collect()
}

/// (i) SIGKILL mid-run, then any next run: the crashed run's report (still
/// its bytes) moves to its quarantine and the person's note stays, its
/// original kept beside it; each action is one stderr line and the
/// staging is released. On the old engine the original was nowhere.
#[cfg(unix)]
#[test]
fn a_crashed_runs_leftovers_settle_at_the_next_run() {
    let dir = crash_dir();
    let envs = signing_key(dir.path());
    run_killed_in_its_sleep(dir.path(), &envs);
    assert_eq!(
        staged_leftovers(dir.path()),
        ["1-note.txt", "1.json", "2.json"]
    );
    let crashed = journals(dir.path()).remove(0);
    let qdir = format!(".nika/quarantine/{}", stamp(&crashed));

    let next = nika_run(dir.path(), "next.nika", &envs);

    let stderr = String::from_utf8_lossy(&next.stderr);
    assert_eq!(next.status.code(), Some(0), "{stderr}");
    let run = format!("a crashed run ({})", stamp(&crashed));
    assert!(
        stderr.contains(&format!("{run} left report.md: moved to {qdir}/report.md"))
            && stderr.contains(&format!(
                "{run} changed note.txt: it stays in place; its original is at {qdir}/originals/note.txt"
            )),
        "one line per action: {stderr}"
    );
    assert_eq!(
        artifact_list(dir.path()),
        [
            format!("{qdir}/manifest.json"),
            format!("{qdir}/originals/note.txt"),
            format!("{qdir}/report.md"),
            "next.nika".to_owned(),
            "note.txt".to_owned(),
            "w.nika".to_owned(),
        ]
    );
    let read = |p: &str| std::fs::read_to_string(dir.path().join(p)).expect(p);
    assert_eq!(read("note.txt"), "a final note");
    assert_eq!(read(&format!("{qdir}/originals/note.txt")), "a draft note");
    assert_eq!(read(&format!("{qdir}/report.md")), "the report");
}

/// (ii) A report the person changed after the crash is theirs now: it
/// stays where it is, said, and only the note's original is settled.
#[cfg(unix)]
#[test]
fn a_file_changed_after_the_crash_stays_in_place() {
    let dir = crash_dir();
    let envs = signing_key(dir.path());
    run_killed_in_its_sleep(dir.path(), &envs);
    std::fs::write(dir.path().join("report.md"), "my own version").expect("the person's edit");
    let crashed = journals(dir.path()).remove(0);
    let qdir = format!(".nika/quarantine/{}", stamp(&crashed));

    let next = nika_run(dir.path(), "next.nika", &envs);

    let stderr = String::from_utf8_lossy(&next.stderr);
    assert!(
        stderr.contains(&format!(
            "a crashed run ({}) left report.md in place: you changed it since",
            stamp(&crashed)
        )),
        "{stderr}"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("report.md")).expect("theirs"),
        "my own version"
    );
    assert_eq!(
        artifact_list(dir.path()),
        [
            format!("{qdir}/manifest.json"),
            format!("{qdir}/originals/note.txt"),
            "next.nika".to_owned(),
            "note.txt".to_owned(),
            "report.md".to_owned(),
            "w.nika".to_owned(),
        ]
    );
}

/// (iii) SIGKILL mid-run, then `--resume` of that journal: the settlement
/// moves the report aside and the restore puts it back at once, so the
/// resumed run counts `fix` and `report` as done and finishes the rest.
#[cfg(unix)]
#[test]
fn resuming_a_crashed_run_settles_then_puts_its_files_back() {
    let dir = crash_dir();
    let envs = signing_key(dir.path());
    run_killed_in_its_sleep(dir.path(), &envs);
    let crashed = journals(dir.path()).remove(0);
    let qdir = format!(".nika/quarantine/{}", stamp(&crashed));
    std::fs::write(dir.path().join("go"), "").expect("the cause fixed");

    let resumed = bin()
        .args(["run", "w.nika", "--json", "--color", "never", "--resume"])
        .arg(&crashed)
        .current_dir(dir.path())
        .envs(envs.iter().map(|(k, v)| (k, v)))
        .output()
        .expect("binary runs");

    let stderr = String::from_utf8_lossy(&resumed.stderr);
    assert_eq!(resumed.status.code(), Some(0), "{stderr}");
    assert!(stderr.contains("put back report.md"), "{stderr}");
    let second = journals(dir.path())
        .into_iter()
        .find(|j| *j != crashed)
        .expect("the resumed journal");
    assert_eq!(tasks_of(&second, "task_cache_hit"), ["fix", "report"]);
    assert_eq!(tasks_of(&second, "task_completed"), ["wait", "gate"]);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("report.md")).expect("back"),
        "the report"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join(&qdir).join("originals/note.txt")).expect("kept"),
        "a draft note"
    );
    assert!(
        staged_leftovers(dir.path()).is_empty(),
        "nothing staged is left"
    );
}
