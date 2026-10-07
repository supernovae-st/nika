// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Regular-file removal proved on the host backend and on a room, every effect
//! inside a parent directory the test alone owns (for a room, `room_harness`;
//! for the host, a private temporary directory with its own `outside/`).
//!
//! Only a regular file is removed, never read, and its neighbours keep their
//! bytes; an absent name, a missing parent (never created), a symlink
//! (internal, external, dangling), a directory, a FIFO and a path that names no
//! final file (`file/`, `file/.`) are refused and leave everything as it was;
//! the raw `remove_file` still unlinks a symlink where the new operation
//! refuses it; a symlinked ancestor is refused; a held parent descriptor keeps
//! the removal in the directory it opened after its visible name was replaced.
//! A room refuses the removal outside the writing phases (its late refusal is
//! counted) and refunds no budget; that the removal is registered for the
//! room's drain is source reuse of the existing write path, not observed here.
//!
//! Not observed here: a substitution of the final name between the
//! classification and the unlink (no deterministic interleaving is driven), a
//! drain or cancellation specific to a removal, and the completion of a
//! removal whose future was dropped. A bare relative host path needs the
//! process working directory: it is proved by its own single-test process
//! (`tests/remove_relative.rs`), never here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use nika_kernel::fs::{FsError, FsWriteDyn};
use nix::sys::stat::Mode;

use crate::room_harness::{Room, names, next_phase};
use crate::{OwnedDir, Phase, RoomLimits, TokioFs};

const BINARY: &[u8] = &[0x00, 0xff, 0x7f, 0x80, b'\n', 0x00];

/// A host parent the test alone owns: `work/` for the operations, `outside/`
/// for every target a link may name.
struct Host {
    parent: tempfile::TempDir,
}

impl Host {
    fn new() -> Self {
        let parent = tempfile::tempdir().unwrap();
        std::fs::create_dir(parent.path().join("work")).unwrap();
        std::fs::create_dir(parent.path().join("outside")).unwrap();
        Self { parent }
    }

    fn work(&self, rel: &str) -> PathBuf {
        self.parent.path().join("work").join(rel)
    }

    fn outside(&self, rel: &str) -> PathBuf {
        self.parent.path().join("outside").join(rel)
    }

    /// Write a fixture, never outside the parent the test owns.
    fn file(&self, path: &Path, bytes: &[u8]) {
        assert!(path.starts_with(self.parent.path()), "{}", path.display());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    /// Every name under the parent with its bytes (a link's target as its
    /// bytes): what a refused removal must leave exactly as it was.
    fn inventory(&self) -> Vec<(String, Vec<u8>)> {
        let mut found = Vec::new();
        let mut stack = vec![PathBuf::new()];
        while let Some(below) = stack.pop() {
            for entry in std::fs::read_dir(self.parent.path().join(&below)).unwrap() {
                let rel = below.join(entry.unwrap().file_name());
                let host = self.parent.path().join(&rel);
                let kind = std::fs::symlink_metadata(&host).unwrap().file_type();
                let bytes = if kind.is_symlink() {
                    std::fs::read_link(&host)
                        .unwrap()
                        .into_os_string()
                        .into_encoded_bytes()
                } else if kind.is_dir() {
                    stack.push(rel.clone());
                    b"<dir>".to_vec()
                } else if kind.is_file() {
                    std::fs::read(&host).unwrap()
                } else {
                    b"<special>".to_vec()
                };
                found.push((rel.display().to_string(), bytes));
            }
        }
        found.sort();
        found
    }
}

fn is_symlink(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

// ─── The host backend ───────────────────────────────────────────────────────

#[tokio::test]
async fn the_host_removes_only_the_regular_file_and_spares_its_neighbours() {
    let host = Host::new();
    host.file(&host.work("out/victim.bin"), BINARY);
    host.file(&host.work("out/neighbour.bin"), BINARY);
    host.file(&host.outside("sentinel.txt"), b"sentinel");
    let mut before = host.inventory();
    TokioFs
        .remove_regular_file(&host.work("out/victim.bin"))
        .await
        .unwrap();
    before.retain(|(name, _)| name != "work/out/victim.bin");
    assert_eq!(host.inventory(), before, "only the victim is gone");
}

#[tokio::test]
async fn the_host_removes_a_file_it_could_never_read() {
    let host = Host::new();
    let victim = host.work("sealed.bin");
    host.file(&victim, BINARY);
    std::fs::set_permissions(&victim, std::fs::Permissions::from_mode(0o000)).unwrap();
    TokioFs.remove_regular_file(&victim).await.unwrap();
    assert!(std::fs::symlink_metadata(&victim).is_err());
}

#[tokio::test]
async fn the_host_refuses_an_absent_name_and_creates_no_parent() {
    let host = Host::new();
    let before = host.inventory();
    let absent = TokioFs.remove_regular_file(&host.work("gone.bin")).await;
    assert!(
        matches!(absent, Err(FsError::NotFound { .. })),
        "{absent:?}"
    );
    let nested = TokioFs
        .remove_regular_file(&host.work("missing/deeper/gone.bin"))
        .await;
    assert!(
        matches!(nested, Err(FsError::NotFound { .. })),
        "{nested:?}"
    );
    assert_eq!(host.inventory(), before);
    assert!(!host.work("missing").exists(), "no parent created");
}

#[tokio::test]
async fn the_host_refuses_every_non_regular_name_and_touches_no_target() {
    let host = Host::new();
    host.file(&host.work("real.bin"), BINARY);
    host.file(&host.outside("target.txt"), b"target");
    std::fs::create_dir(host.work("dir")).unwrap();
    host.file(&host.work("dir/inner.bin"), BINARY);
    std::os::unix::fs::symlink(host.work("real.bin"), host.work("inner-link")).unwrap();
    std::os::unix::fs::symlink(host.outside("target.txt"), host.work("outer-link")).unwrap();
    std::os::unix::fs::symlink(host.outside("nothing"), host.work("dangling")).unwrap();
    nix::unistd::mkfifo(&host.work("pipe"), Mode::S_IRWXU).unwrap();
    let before = host.inventory();
    for link in ["inner-link", "outer-link", "dangling"] {
        let refused = TokioFs.remove_regular_file(&host.work(link)).await;
        assert!(
            matches!(refused, Err(FsError::SymlinkRefused { .. })),
            "{link}: {refused:?}"
        );
    }
    for other in ["dir", "pipe"] {
        let refused = TokioFs.remove_regular_file(&host.work(other)).await;
        assert!(
            matches!(refused, Err(FsError::InvalidData { .. })),
            "{other}: {refused:?}"
        );
    }
    assert_eq!(host.inventory(), before, "every name and target as it was");
}

#[tokio::test]
async fn the_host_never_reads_file_slash_or_file_dot_as_the_file() {
    let host = Host::new();
    let file = host.work("file.bin");
    host.file(&file, BINARY);
    let before = host.inventory();
    let work = host.parent.path().join("work");
    for spelling in ["file.bin/", "file.bin/.", "file.bin/..", "."] {
        let path = PathBuf::from(format!("{}/{spelling}", work.display()));
        let refused = TokioFs.remove_regular_file(&path).await;
        assert!(
            matches!(refused, Err(FsError::InvalidData { .. })),
            "{spelling}: {refused:?}"
        );
    }
    for spelling in ["", "/"] {
        let refused = TokioFs.remove_regular_file(Path::new(spelling)).await;
        assert!(
            matches!(refused, Err(FsError::InvalidData { .. })),
            "{spelling:?}: {refused:?}"
        );
    }
    assert_eq!(host.inventory(), before);
    assert_eq!(std::fs::read(&file).unwrap(), BINARY);
}

#[tokio::test]
async fn the_raw_removal_still_unlinks_a_link_the_regular_removal_refuses() {
    let host = Host::new();
    host.file(&host.outside("target.txt"), b"target");
    let link = host.work("link");
    std::os::unix::fs::symlink(host.outside("target.txt"), &link).unwrap();
    let refused = TokioFs.remove_regular_file(&link).await;
    assert!(
        matches!(refused, Err(FsError::SymlinkRefused { .. })),
        "{refused:?}"
    );
    assert!(is_symlink(&link), "the refused link stays");
    TokioFs.remove_file(&link).await.unwrap();
    assert!(!is_symlink(&link), "the raw removal unlinked the name");
    assert_eq!(
        std::fs::read(host.outside("target.txt")).unwrap(),
        b"target"
    );
}

#[tokio::test]
async fn the_host_refuses_a_symlinked_ancestor_and_spares_its_target() {
    let host = Host::new();
    host.file(&host.outside("dir/victim.bin"), BINARY);
    std::os::unix::fs::symlink(host.outside("dir"), host.work("linked")).unwrap();
    let before = host.inventory();
    let refused = TokioFs
        .remove_regular_file(&host.work("linked/victim.bin"))
        .await;
    // `OwnedDir::open` refuses the linked component: the OS refusal of its
    // `O_NOFOLLOW | O_DIRECTORY` open (the errno is platform-dependent; only
    // macOS was executed here, Linux was not), translated by `FsError::from_io`
    // into `Io`: an implementation-path refusal, never an absence and never the
    // unsupported default.
    let Err(FsError::Io { reason }) = &refused else {
        panic!("an ancestor link is refused as Io: {refused:?}");
    };
    assert!(
        reason.contains("Not a directory") || reason.contains("symbolic links"),
        "{reason}"
    );
    assert!(!reason.contains("unsupported"), "{reason}");
    assert_eq!(host.inventory(), before);
}

#[test]
fn a_held_parent_keeps_the_removal_where_it_was_opened() {
    let host = Host::new();
    host.file(&host.work("held/victim.bin"), b"original");
    let held = OwnedDir::open(&host.work("held")).unwrap();
    // The visible name now leads elsewhere: a replacement holds a sentinel
    // under the same final name.
    std::fs::rename(host.work("held"), host.outside("moved")).unwrap();
    host.file(&host.work("held/victim.bin"), b"replacement");
    crate::remove::remove_regular_at(
        held.as_file(),
        std::ffi::OsStr::new("victim.bin"),
        Path::new("held/victim.bin"),
    )
    .unwrap();
    assert!(!host.outside("moved/victim.bin").exists(), "the held one");
    assert_eq!(
        std::fs::read(host.work("held/victim.bin")).unwrap(),
        b"replacement",
        "the replacement under the visible name is untouched"
    );
}

#[test]
fn the_shared_helper_takes_one_contained_leaf_only() {
    let host = Host::new();
    host.file(&host.work("dir/victim.bin"), BINARY);
    let held = OwnedDir::open(&host.work("")).unwrap();
    let before = host.inventory();
    for name in ["", ".", "..", "dir/victim.bin"] {
        let refused = crate::remove::remove_regular_at(
            held.as_file(),
            std::ffi::OsStr::new(name),
            Path::new(name),
        );
        assert!(
            matches!(refused, Err(FsError::InvalidData { .. })),
            "{name:?}: {refused:?}"
        );
    }
    assert_eq!(host.inventory(), before);
}

// ─── A room ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_room_removes_only_the_regular_file() {
    let room = Room::new(&[("out/victim.bin", "victim"), ("out/neighbour.txt", "kept")]);
    let sentinel = room.sentinel("sentinel.txt", "sentinel");
    room.to_run().await;
    let outside = room.outside_snapshot();
    room.fs
        .remove_regular_file(Path::new("out/victim.bin"))
        .await
        .unwrap();
    assert_eq!(names(&room.host("out")), vec!["neighbour.txt"]);
    assert_eq!(
        std::fs::read_to_string(room.host("out/neighbour.txt")).unwrap(),
        "kept"
    );
    assert_eq!(std::fs::read_to_string(sentinel).unwrap(), "sentinel");
    assert_eq!(room.outside_snapshot(), outside);
}

#[tokio::test]
async fn a_room_refuses_every_non_regular_name_and_touches_no_target() {
    let room = Room::new(&[("real.txt", "real"), ("dir/inner.txt", "inner")]);
    let target = room.sentinel("target.txt", "target");
    std::os::unix::fs::symlink(room.host("real.txt"), room.host("inner-link")).unwrap();
    std::os::unix::fs::symlink(&target, room.host("outer-link")).unwrap();
    std::os::unix::fs::symlink(room.outside("nothing"), room.host("dangling")).unwrap();
    nix::unistd::mkfifo(&room.host("pipe"), Mode::S_IRWXU).unwrap();
    room.to_run().await;
    let outside = room.outside_snapshot();
    let inside = names(&room.host(""));
    for link in ["inner-link", "outer-link", "dangling"] {
        let refused = room.fs.remove_regular_file(Path::new(link)).await;
        assert!(
            matches!(refused, Err(FsError::SymlinkRefused { .. })),
            "{link}: {refused:?}"
        );
    }
    for other in ["dir", "pipe", "real.txt/", "real.txt/.", "."] {
        let refused = room.fs.remove_regular_file(Path::new(other)).await;
        assert!(
            matches!(refused, Err(FsError::InvalidData { .. })),
            "{other}: {refused:?}"
        );
    }
    for escape in ["/etc/hosts", "../outside/target.txt", "dir/../real.txt"] {
        let refused = room.fs.remove_regular_file(Path::new(escape)).await;
        assert!(
            matches!(refused, Err(FsError::PermissionDenied { .. })),
            "{escape}: {refused:?}"
        );
    }
    assert_eq!(names(&room.host("")), inside);
    assert_eq!(
        std::fs::read_to_string(room.host("real.txt")).unwrap(),
        "real"
    );
    assert_eq!(room.outside_snapshot(), outside);
}

#[tokio::test]
async fn a_room_refuses_an_absent_name_and_creates_no_parent() {
    let room = Room::new(&[("kept.txt", "kept")]);
    room.to_run().await;
    let absent = room.fs.remove_regular_file(Path::new("gone.txt")).await;
    assert!(
        matches!(absent, Err(FsError::NotFound { .. })),
        "{absent:?}"
    );
    let nested = room
        .fs
        .remove_regular_file(Path::new("missing/deeper/gone.txt"))
        .await;
    assert!(
        matches!(nested, Err(FsError::NotFound { .. })),
        "{nested:?}"
    );
    assert_eq!(names(&room.host("")), vec!["kept.txt"], "no parent created");
}

#[tokio::test]
async fn a_room_refuses_a_symlinked_ancestor() {
    let room = Room::new(&[]);
    let target = room.sentinel("dir/victim.txt", "victim");
    std::os::unix::fs::symlink(room.outside("dir"), room.host("linked")).unwrap();
    room.to_run().await;
    let refused = room
        .fs
        .remove_regular_file(Path::new("linked/victim.txt"))
        .await;
    assert!(
        matches!(refused, Err(FsError::SymlinkRefused { .. })),
        "{refused:?}"
    );
    assert_eq!(std::fs::read_to_string(target).unwrap(), "victim");
}

#[tokio::test]
async fn a_room_refuses_the_removal_outside_the_writing_phases() {
    let room = Room::new(&[("out/kept.txt", "kept")]);
    room.to_run().await;
    assert_eq!(next_phase(room.fs.ledger()).await, Phase::ReadBack);
    let refused = room.fs.remove_regular_file(Path::new("out/kept.txt")).await;
    assert!(
        matches!(refused, Err(FsError::PermissionDenied { .. })),
        "{refused:?}"
    );
    assert_eq!(room.fs.ledger().late_refusals(), 1, "a late producer");
    assert_eq!(next_phase(room.fs.ledger()).await, Phase::Closed);
    let closed = room.fs.remove_regular_file(Path::new("out/kept.txt")).await;
    assert!(
        matches!(closed, Err(FsError::PermissionDenied { .. })),
        "{closed:?}"
    );
    assert_eq!(
        std::fs::read_to_string(room.host("out/kept.txt")).unwrap(),
        "kept"
    );
}

#[tokio::test]
async fn a_room_removal_refunds_no_budget_and_keeps_the_written_history() {
    let room = Room::with_limits(&[("out/.keep", "")], RoomLimits::new(4, 1));
    room.to_run().await;
    room.fs
        .write(Path::new("out/a.bin"), b"abcd")
        .await
        .unwrap();
    room.fs
        .remove_regular_file(Path::new("out/a.bin"))
        .await
        .unwrap();
    assert!(!room.host("out/a.bin").exists());
    let over = room.fs.write(Path::new("out/b.bin"), b"e").await;
    assert!(
        matches!(over, Err(FsError::Io { .. })),
        "no refund: {over:?}"
    );
    assert!(!room.host("out/b.bin").exists());
    assert_eq!(
        room.fs.ledger().written().collect::<Vec<_>>(),
        vec![PathBuf::from("out/a.bin")],
        "the history keeps what was written"
    );
}
