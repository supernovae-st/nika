// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The host backend's pin, stated exactly as it behaves: the final component is
//! opened without following a symlink, so the kernel refuses a symlinked name
//! inside the open itself; an ancestor symlink is still followed. Only the
//! boundary judgment made before the open covers the ancestors, and the judged
//! read re-judges the resolved target when the pin refuses.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use nika_kernel::fs::{FsError, FsReadDyn};

use crate::TokioFs;

#[tokio::test]
async fn the_host_pin_serves_a_regular_file() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("real.txt"), "inside-bytes").unwrap();
    let bytes = TokioFs
        .read_pinned(&dir.path().join("real.txt"))
        .await
        .unwrap();
    assert_eq!(&bytes[..], b"inside-bytes");
}

#[tokio::test]
async fn the_host_pin_refuses_a_symlinked_final_component() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("real.txt"), "inside-bytes").unwrap();
    std::os::unix::fs::symlink(dir.path().join("real.txt"), dir.path().join("link.txt")).unwrap();
    let refused = TokioFs.read_pinned(&dir.path().join("link.txt")).await;
    assert!(
        matches!(refused, Err(FsError::SymlinkRefused { .. })),
        "{refused:?}"
    );
}

#[tokio::test]
async fn the_host_pin_still_follows_a_symlinked_ancestor_as_stated() {
    // The declared gap, recorded rather than hidden: the pin covers the final
    // component only. The rooted room is what covers every component.
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("real")).unwrap();
    std::fs::write(dir.path().join("real/a.txt"), "through-the-ancestor").unwrap();
    std::os::unix::fs::symlink(dir.path().join("real"), dir.path().join("linkdir")).unwrap();
    let bytes = TokioFs
        .read_pinned(&dir.path().join("linkdir/a.txt"))
        .await
        .unwrap();
    assert_eq!(&bytes[..], b"through-the-ancestor");
}
