// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A crashed run's leftovers, settled at the next run's start: only on a
//! lease that reads dead, a created file only while its bytes are the run's.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use super::*;

/// The crashed run's execution id, and the journal stamp it named.
const UUID: &str = "01a1309c-17d4-7362-90dc-9bd0f733d248";
const STAMP: &str = "2026-10-12T14-54-58Z-d248";

fn sha(text: &str) -> String {
    crate::seal::sha256_hex(text.as_bytes())
}

/// A run's journal under `root`, its first frame naming the execution.
fn journal(root: &Path) -> PathBuf {
    let traces = root.join(crate::store::TRACE_DIR);
    std::fs::create_dir_all(&traces).expect("traces");
    let path = traces.join(format!("{STAMP}.ndjson"));
    let frame = serde_json::json!({ "kind": "workflow_started", "execution": { "uuid": UUID } });
    std::fs::write(&path, format!("{frame}\n")).expect("journal");
    path
}

/// The journal of a run that died: its lease exists and nobody holds it.
fn dead_journal(root: &Path) -> PathBuf {
    let path = journal(root);
    drop(crate::liveness::hold(&path).expect("lease"));
    path
}

/// The crashed run's staging directory, with its copies and records.
fn staging(root: &Path, files: &[(&str, &str)], records: &[serde_json::Value]) -> PathBuf {
    let dir = root
        .join(crate::store::ORIGINALS_DIR)
        .join(format!("exe-{UUID}"));
    std::fs::create_dir_all(&dir).expect("staging");
    for (name, bytes) in files {
        std::fs::write(dir.join(name), bytes).expect("copy");
    }
    for (n, record) in records.iter().enumerate() {
        std::fs::write(dir.join(format!("{}.json", n + 1)), record.to_string()).expect("record");
    }
    dir
}

fn read(root: &Path, path: &str) -> String {
    std::fs::read_to_string(root.join(path)).expect(path)
}

/// The crash the witness reproduced: the run edited the person's note,
/// created a report, and died. The report (still the run's bytes) moves to
/// the crashed run's quarantine; the note stays, its original kept beside
/// it; the manifest lets `--resume` of that journal put the report back.
#[cfg(unix)]
#[test]
fn a_dead_runs_own_files_move_and_the_persons_stay() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let journal = dead_journal(root);
    std::fs::write(root.join("notes.md"), "a final note").expect("edited");
    std::fs::write(root.join("report.md"), "the report").expect("created");
    let staged = staging(
        root,
        &[("1-notes.md", "a draft note")],
        &[
            serde_json::json!({ "path": "notes.md", "origin": "existed", "original": "1-notes.md" }),
            serde_json::json!({ "path": "report.md", "origin": "created", "sha256": sha("the report") }),
        ],
    );

    let folds = settle_crashed(root);

    let dir = format!(".nika/quarantine/{STAMP}");
    assert_eq!(folds.len(), 1, "{folds:?}");
    let fold = &folds[0];
    assert_eq!(fold["crashed"], STAMP);
    assert_eq!(
        fold["outputs"],
        serde_json::json!([{ "path": "report.md", "quarantined_to": format!("{dir}/report.md") }])
    );
    assert_eq!(
        fold["kept"],
        serde_json::json!([{
            "path": "notes.md",
            "action": "kept_in_place",
            "original": format!("{dir}/originals/notes.md"),
        }])
    );
    assert!(!root.join("report.md").exists(), "the run's own file moved");
    assert_eq!(read(root, &format!("{dir}/report.md")), "the report");
    assert_eq!(
        read(root, "notes.md"),
        "a final note",
        "the person's file stays"
    );
    assert_eq!(
        read(root, &format!("{dir}/originals/notes.md")),
        "a draft note"
    );
    assert!(
        fold["manifest"].is_string() && fold["manifest_sha256"].is_string(),
        "{fold}"
    );
    assert!(!staged.exists(), "the staging is released once settled");
    assert!(settle_crashed(root).is_empty(), "settled once");

    let restored = super::super::restore_at(root, &journal).expect("the manifest restores");
    assert_eq!(restored["restored"][0]["path"], "report.md");
    assert_eq!(read(root, "report.md"), "the report");
}

/// A created file the person changed since the crash, or one whose last
/// write left no digest, stays where it is, and the fold says why.
#[cfg(unix)]
#[test]
fn a_file_changed_since_the_crash_stays_in_place() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    dead_journal(root);
    std::fs::write(root.join("report.md"), "my own version").expect("the person's edit");
    std::fs::write(root.join("draft.md"), "unproven").expect("no digest");
    staging(
        root,
        &[],
        &[
            serde_json::json!({ "path": "report.md", "origin": "created", "sha256": sha("the report") }),
            serde_json::json!({ "path": "draft.md", "origin": "created" }),
            serde_json::json!({ "path": "gone.md", "origin": "created", "sha256": sha("x") }),
        ],
    );

    let folds = settle_crashed(root);

    assert_eq!(folds[0]["outputs"], serde_json::json!([]));
    assert_eq!(
        folds[0]["kept"],
        serde_json::json!([
            { "path": "report.md", "action": "kept_in_place", "reason": "you changed it since" },
            {
                "path": "draft.md",
                "action": "kept_in_place",
                "reason": "its last write left no digest to prove it unchanged",
            },
        ])
    );
    assert_eq!(read(root, "report.md"), "my own version");
    assert_eq!(read(root, "draft.md"), "unproven");
}

/// Dead only on positive evidence: a run whose lease is held, a staging
/// directory with no journal, and a journal without a lease are untouched.
#[cfg(unix)]
#[test]
fn a_live_or_unjudged_run_is_never_touched() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    std::fs::write(root.join("report.md"), "the report").expect("created");
    let records = [
        serde_json::json!({ "path": "report.md", "origin": "created", "sha256": sha("the report") }),
    ];
    let staged = staging(root, &[], &records);

    assert!(settle_crashed(root).is_empty(), "no journal: unknown");
    let journal = journal(root);
    assert!(settle_crashed(root).is_empty(), "no lease: unknown");
    let lease = crate::liveness::hold(&journal).expect("lease");
    assert!(settle_crashed(root).is_empty(), "a live run");
    drop(lease);

    assert!(staged.join("1.json").exists(), "the staging is as it was");
    assert_eq!(read(root, "report.md"), "the report");
    assert_eq!(settle_crashed(root).len(), 1, "dead now: settled");
}

/// A run killed during its own teardown already has its quarantine dir:
/// its staging is left as it is, never half merged, and the fold says so.
#[cfg(unix)]
#[test]
fn a_run_whose_teardown_had_begun_is_left_as_it_is() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    dead_journal(root);
    std::fs::create_dir_all(root.join(format!(".nika/quarantine/{STAMP}"))).expect("begun");
    let staged = staging(
        root,
        &[("1-notes.md", "a draft note")],
        &[serde_json::json!({ "path": "notes.md", "origin": "existed", "original": "1-notes.md" })],
    );

    let folds = settle_crashed(root);

    assert_eq!(folds[0]["reason"], "its teardown had begun");
    assert_eq!(
        folds[0]["staging_left"],
        serde_json::json!(format!(".nika/originals/exe-{UUID}"))
    );
    assert_eq!(
        read(root, &staged.join("1-notes.md").to_string_lossy()),
        "a draft note"
    );
}

/// Only a path inside the project is ever moved; a copy the records do
/// not name keeps the staging directory, named in the fold.
#[cfg(unix)]
#[test]
fn an_outside_path_stays_and_an_unrecorded_copy_keeps_the_staging() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    dead_journal(root);
    let staged = staging(
        root,
        &[("7-orphan.md", "an original without its record")],
        &[serde_json::json!({ "path": "../outside.md", "origin": "created", "sha256": sha("x") })],
    );

    let folds = settle_crashed(root);

    assert_eq!(
        folds[0]["kept"],
        serde_json::json!([
            { "path": "../outside.md", "action": "kept_in_place", "reason": "outside the project" },
        ])
    );
    assert_eq!(
        folds[0]["staging_left"],
        serde_json::json!(format!(".nika/originals/exe-{UUID}"))
    );
    assert!(staged.join("7-orphan.md").exists(), "never deleted");
}
