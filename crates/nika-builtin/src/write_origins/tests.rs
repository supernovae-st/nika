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

    assert_eq!(origins.origin(&report), Some(WriteOrigin::Created));
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

    assert_eq!(origins.origin(&draft), Some(WriteOrigin::Created));
    assert!(
        !scratch.0.join("staging").exists(),
        "nothing was copied for a file the run created"
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
    assert!(!scratch.0.join("staging").exists(), "nothing was copied");
}
