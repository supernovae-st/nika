// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The ledger over the REAL filesystem: the copy reads through the
//! backend's pin, which `MockFs` does not have.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use nika_fs::TokioFs;

use super::*;

static SCRATCH_SEQ: AtomicU64 = AtomicU64::new(0);

/// A unique scratch directory, removed when the test ends.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let n = SCRATCH_SEQ.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("nika-write-origins-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&root).expect("scratch");
        Self(root)
    }

    fn path(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn a_new_path_is_created_and_an_existing_file_keeps_its_original() {
    let scratch = Scratch::new();
    let notes = scratch.path("notes.md");
    std::fs::write(&notes, "my own notes").expect("seed");
    let report = scratch.path("report.md");
    let origins = WriteOrigins::new(scratch.0.join("staging"));

    origins.note(&TokioFs, &notes).await;
    origins.note(&TokioFs, &report).await;

    assert_eq!(
        origins.origin(&report),
        Some(WriteOrigin::Created { file: None })
    );
    let Some(WriteOrigin::Existed { original: Ok(copy) }) = origins.origin(&notes) else {
        panic!(
            "an existing file keeps its original: {:?}",
            origins.origin(&notes)
        );
    };
    assert!(copy.starts_with(origins.staging()), "{}", copy.display());
    assert_eq!(
        std::fs::read_to_string(&copy).expect("copy"),
        "my own notes"
    );
    assert_eq!(origins.origin(&scratch.path("never.md")), None);
}

#[tokio::test]
async fn the_first_sighting_decides() {
    // The run creates a file, then edits it: it stays a file the run
    // created, never mistaken for a person's own.
    let scratch = Scratch::new();
    let draft = scratch.path("draft.md");
    let origins = WriteOrigins::new(scratch.0.join("staging"));

    origins.note(&TokioFs, &draft).await;
    std::fs::write(&draft, "written by the run").expect("write");
    origins.note(&TokioFs, &draft).await;

    assert_eq!(
        origins.origin(&draft),
        Some(WriteOrigin::Created { file: None })
    );
    assert_eq!(
        staged(&scratch),
        ["1.json"],
        "no copy for a file the run created, only its one record"
    );
}

/// The file names under the scratch's staging directory, sorted.
fn staged(scratch: &Scratch) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(scratch.0.join("staging"))
        .map(|dir| {
            dir.filter_map(Result::ok)
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// A staged record, parsed.
fn record(scratch: &Scratch, number: usize) -> serde_json::Value {
    let bytes = std::fs::read(scratch.0.join("staging").join(format!("{number}.json")))
        .expect("the record");
    serde_json::from_slice(&bytes).expect("a JSON record")
}

/// Each first sighting leaves its record before the run writes: the path,
/// what stood there, and the copy that holds a person's original bytes. A
/// run killed before its teardown leaves exactly this for a later run.
#[tokio::test]
async fn each_first_sighting_leaves_a_record_beside_its_copy() {
    let scratch = Scratch::new();
    let notes = scratch.path("notes.md");
    std::fs::write(&notes, "my own notes").expect("seed");
    let report = scratch.path("report.md");
    let origins = WriteOrigins::new(scratch.0.join("staging"));

    origins.note(&TokioFs, &notes).await;
    origins.note(&TokioFs, &report).await;

    assert_eq!(staged(&scratch), ["1-notes.md", "1.json", "2.json"]);
    assert_eq!(
        record(&scratch, 1),
        serde_json::json!({ "path": notes, "origin": "existed", "original": "1-notes.md" })
    );
    assert_eq!(
        record(&scratch, 2),
        serde_json::json!({ "path": report, "origin": "created" })
    );
}

/// A file a resume put back counts as created by the resumed run, unless
/// the run already met the path: the first sighting still decides.
#[tokio::test]
async fn an_adopted_path_counts_as_created_unless_already_seen() {
    let scratch = Scratch::new();
    let notes = scratch.path("notes.md");
    std::fs::write(&notes, "my own notes").expect("seed");
    let origins = WriteOrigins::new(scratch.0.join("staging"));

    origins.adopt("report.md", None);
    origins.note(&TokioFs, &notes).await;
    origins.adopt(&notes, None);

    assert_eq!(
        origins.origin("report.md"),
        Some(WriteOrigin::Created { file: None })
    );
    assert!(
        matches!(origins.origin(&notes), Some(WriteOrigin::Existed { .. })),
        "{:?}",
        origins.origin(&notes)
    );
    assert_eq!(
        staged(&scratch),
        ["2-notes.md", "2.json"],
        "an adoption stages nothing"
    );
}

/// A created file's record gains the sha256 of the bytes its last write
/// left; a person's file needs none (it is never moved).
#[tokio::test]
async fn a_created_files_record_carries_the_digest_of_its_last_write() {
    use sha2::Digest as _;
    let digest = |text: &str| crate::data::hex_encode(&sha2::Sha256::digest(text.as_bytes()));
    let scratch = Scratch::new();
    let notes = scratch.path("notes.md");
    std::fs::write(&notes, "my own notes").expect("seed");
    let report = scratch.path("report.md");
    let origins = WriteOrigins::new(scratch.0.join("staging"));

    origins.note(&TokioFs, &report).await;
    std::fs::write(&report, "first draft").expect("write");
    origins.wrote(&TokioFs, &report).await;
    assert_eq!(
        record(&scratch, 1)["sha256"],
        serde_json::json!(digest("first draft"))
    );
    {
        use std::os::unix::fs::MetadataExt as _;
        let meta = std::fs::metadata(&report).expect("meta");
        assert_eq!(record(&scratch, 1)["inode"], serde_json::json!(meta.ino()));
        assert_eq!(record(&scratch, 1)["len"], serde_json::json!(11));
        let Some(WriteOrigin::Created { file: Some(known) }) = origins.origin(&report) else {
            panic!(
                "the file the run created is known: {:?}",
                origins.origin(&report)
            );
        };
        assert_eq!(
            (known.identity.device, known.identity.inode, known.len),
            (meta.dev(), meta.ino(), 11)
        );
    }
    std::fs::write(&report, "the report").expect("write again");
    origins.wrote(&TokioFs, &report).await;
    assert_eq!(
        record(&scratch, 1)["sha256"],
        serde_json::json!(digest("the report"))
    );

    origins.note(&TokioFs, &notes).await;
    std::fs::write(&notes, "edited by the run").expect("edit");
    origins.wrote(&TokioFs, &notes).await;
    assert!(
        record(&scratch, 2).get("sha256").is_none(),
        "a person's file has no digest"
    );
}

#[tokio::test]
async fn an_original_that_cannot_be_copied_is_stated_never_a_gate() {
    let scratch = Scratch::new();
    let folder = scratch.path("folder");
    std::fs::create_dir_all(&folder).expect("dir");
    let origins = WriteOrigins::new(scratch.0.join("staging"));

    origins.note(&TokioFs, &folder).await;

    assert!(
        matches!(
            origins.origin(&folder),
            Some(WriteOrigin::Existed { original: Err(reason) }) if reason.contains("regular file")
        ),
        "{:?}",
        origins.origin(&folder)
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_symlinked_name_is_never_followed_for_the_copy() {
    // The copy reads through the pin: a name that is a symlink to a file
    // elsewhere is not copied into the project's staging directory.
    let scratch = Scratch::new();
    let elsewhere = scratch.path("elsewhere.txt");
    std::fs::write(&elsewhere, "not the run's to copy").expect("seed");
    let link = scratch.path("link.txt");
    std::os::unix::fs::symlink(&elsewhere, &link).expect("symlink");
    let origins = WriteOrigins::new(scratch.0.join("staging"));

    origins.note(&TokioFs, &link).await;

    assert!(
        matches!(
            origins.origin(&link),
            Some(WriteOrigin::Existed { original: Err(_) })
        ),
        "{:?}",
        origins.origin(&link)
    );
    assert_eq!(
        staged(&scratch),
        ["1.json"],
        "nothing was copied, only the record"
    );
    assert!(record(&scratch, 1).get("original").is_none());
}
