// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The host's regular removal of a bare relative name: its parent is the
//! process working directory (`""` becomes `.`), held, then the name removed.
//!
//! Changing the working directory is process-wide, so this proof is its own
//! integration-test process holding exactly ONE test: no other test can run
//! beside it while the directory is changed. The test owns an exclusive
//! temporary directory, enters it, removes `victim.bin`, and returns to the
//! process's original directory (on every exit path, by a drop guard) before
//! the temporary directory is cleaned up.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use nika_fs::TokioFs;
use nika_kernel::fs::FsWriteDyn;

/// Restores the process's original working directory when dropped.
struct Return(PathBuf);

impl Drop for Return {
    fn drop(&mut self) {
        std::env::set_current_dir(&self.0).expect("return to the original directory");
    }
}

/// Every name in `dir` with its bytes (a directory as a marker).
fn inventory(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut found: Vec<(String, Vec<u8>)> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            let bytes = if entry.file_type().unwrap().is_dir() {
                b"<dir>".to_vec()
            } else {
                std::fs::read(entry.path()).unwrap()
            };
            (entry.file_name().to_string_lossy().into_owned(), bytes)
        })
        .collect();
    found.sort();
    found
}

#[test]
fn a_bare_relative_name_is_removed_from_the_working_directory() {
    let owned = tempfile::tempdir().unwrap();
    let dir = owned.path();
    std::fs::write(dir.join("victim.bin"), [0x00, 0xff, b'\n']).unwrap();
    std::fs::write(dir.join("neighbour.bin"), [0x01, 0x02]).unwrap();
    std::fs::create_dir(dir.join("nested")).unwrap();
    std::fs::write(dir.join("nested/victim.bin"), b"nested").unwrap();
    let before = inventory(dir);

    let original = std::env::current_dir().unwrap();
    let removed = {
        let _return = Return(original.clone());
        std::env::set_current_dir(dir).unwrap();
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(TokioFs.remove_regular_file(Path::new("victim.bin")))
    };
    assert_eq!(
        std::env::current_dir().unwrap(),
        original,
        "directory restored"
    );
    removed.unwrap();

    let mut expected = before;
    expected.retain(|(name, _)| name != "victim.bin");
    assert_eq!(inventory(dir), expected, "only the bare name is gone");
    assert_eq!(
        std::fs::read(dir.join("nested/victim.bin")).unwrap(),
        b"nested",
        "the same leaf in a subdirectory is untouched"
    );
}
