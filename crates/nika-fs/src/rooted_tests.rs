// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The rooted filesystem proved over rooms whose every effect stays in a parent
//! directory the test alone owns (see `room_harness`).
//!
//! `.` answers the room's identity; reads and writes stay in the room; an
//! absolute path, a `..`, a symlink at any parent component and a special file
//! are refused by the backend itself, for reads and for every mutation; a write
//! over a symlinked final name replaces the name and spares the target, and a
//! removal unlinks the name only; a claimed name is the only evidence, and a
//! failed publish leaves no temporary file and returns its budget; read-back
//! refuses every write and a closed room every operation; copy-in, overwrites
//! and created directories are charged and nothing is refunded; a read never
//! consumes past the room's bound, and an unrepresentable bound is refused.
//!
//! The special-file witness runs beside the harness's FIFO watchdog: a read that
//! needed the watchdog's peer waited for one, which is never a pass, and the
//! harness proves that report on a read that does block.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{self, Read};
use std::path::{Path, PathBuf};

use nika_kernel::fs::{FsError, FsListDyn, FsMetaDyn, FsReadDyn, FsWriteDyn};
use nix::sys::stat::Mode;

use crate::room_harness::{BOUND, Room, SHORT, names, next_phase, read_beside_watchdog};
use crate::{Phase, RoomLimits, read_capped};

// ─── Capped reads ───────────────────────────────────────────────────────────

/// A finite source that counts every byte handed out.
struct Counting {
    left: usize,
    consumed: usize,
}

impl Read for Counting {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = buf.len().min(self.left);
        buf[..n].fill(b'x');
        self.left -= n;
        self.consumed += n;
        Ok(n)
    }
}

/// A source that hands out one byte per read.
struct Trickle {
    left: usize,
    consumed: usize,
}

impl Read for Trickle {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.left == 0 || buf.is_empty() {
            return Ok(0);
        }
        buf[0] = b'x';
        self.left -= 1;
        self.consumed += 1;
        Ok(1)
    }
}

/// A source that fails once its prefix is out.
struct Failing {
    prefix: usize,
}

impl Read for Failing {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.prefix == 0 {
            return Err(io::Error::other("the source failed after its prefix"));
        }
        let n = buf.len().min(self.prefix);
        buf[..n].fill(b'x');
        self.prefix -= n;
        Ok(n)
    }
}

#[test]
fn a_capped_read_consumes_at_most_one_byte_past_its_limit() {
    let mut source = Counting {
        left: 1 << 20,
        consumed: 0,
    };
    let capped = read_capped(&mut source, 1024).unwrap();
    assert!(capped.over, "the source held more");
    assert_eq!(capped.bytes.len(), 1024);
    assert!(
        source.consumed <= 1025,
        "consumed {} bytes",
        source.consumed
    );
}

#[test]
fn a_capped_read_at_and_past_the_exact_limit() {
    let exact = read_capped(
        &mut Counting {
            left: 1024,
            consumed: 0,
        },
        1024,
    )
    .unwrap();
    assert!(!exact.over && exact.bytes.len() == 1024, "{:?}", exact.over);
    let one_more = read_capped(
        &mut Counting {
            left: 1025,
            consumed: 0,
        },
        1024,
    )
    .unwrap();
    assert!(
        one_more.over && one_more.bytes.len() == 1024,
        "{:?}",
        one_more.over
    );
}

#[test]
fn a_zero_limit_reads_nothing_yet_tells_an_empty_source_from_a_full_one() {
    let mut empty = Counting {
        left: 0,
        consumed: 0,
    };
    let none = read_capped(&mut empty, 0).unwrap();
    let mut full = Counting {
        left: 5,
        consumed: 0,
    };
    let some = read_capped(&mut full, 0).unwrap();
    assert!(none.bytes.is_empty() && !none.over, "{none:?}");
    assert!(some.bytes.is_empty() && some.over, "{some:?}");
    assert!(full.consumed <= 1, "consumed {} bytes", full.consumed);
}

#[test]
fn an_unrepresentable_limit_is_refused_before_any_byte_is_read() {
    let mut source = Counting {
        left: 1 << 20,
        consumed: 0,
    };
    let refused = read_capped(&mut source, u64::MAX);
    assert!(
        matches!(&refused, Err(error) if error.kind() == io::ErrorKind::InvalidInput),
        "{refused:?}"
    );
    assert_eq!(source.consumed, 0);
}

#[test]
fn short_reads_are_gathered_up_to_the_limit() {
    let mut source = Trickle {
        left: 5000,
        consumed: 0,
    };
    let capped = read_capped(&mut source, 4096).unwrap();
    assert!(
        capped.over && capped.bytes.len() == 4096,
        "{}",
        capped.bytes.len()
    );
    assert!(
        source.consumed <= 4097,
        "consumed {} bytes",
        source.consumed
    );
}

#[test]
fn an_io_error_after_a_prefix_is_an_error_not_a_short_result() {
    let failed = read_capped(&mut Failing { prefix: 10 }, 1024);
    assert!(failed.is_err(), "{failed:?}");
}

// ─── Paths, identity, containment ──────────────────────────────────────────

#[tokio::test]
async fn the_room_identity_answers_dot_and_contained_paths() {
    let room = Room::new(&[("data/a.txt", "a")]);
    let identity = room.fs.identity().to_path_buf();
    let dot = room.fs.canonicalize(Path::new(".")).await;
    let contained = room.fs.canonicalize(Path::new("./data/a.txt")).await;
    let absent = room.fs.canonicalize(Path::new("out/new.txt")).await;
    assert_eq!(dot.unwrap(), identity);
    assert_eq!(contained.unwrap(), identity.join("data/a.txt"));
    assert!(
        matches!(absent, Err(FsError::NotFound { .. })),
        "an absent path is not found, so a boundary folds its tail: {absent:?}"
    );
}

#[tokio::test]
async fn a_contained_read_and_a_nested_new_write_stay_in_the_room() {
    let room = Room::new(&[("data/sales.csv", "a,1\n")]);
    let bytes = room.fs.read(Path::new("./data/sales.csv")).await.unwrap();
    assert_eq!(&bytes[..], b"a,1\n");
    room.to_run().await;
    room.fs.create_dir_all(Path::new("out/deep")).await.unwrap();
    room.fs
        .write(Path::new("./out/deep/top.json"), b"[]")
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(room.host("out/deep/top.json")).unwrap(),
        b"[]"
    );
    assert_eq!(
        room.fs.ledger().written().collect::<Vec<_>>(),
        vec![PathBuf::from("out/deep/top.json")]
    );
}

#[tokio::test]
async fn the_backend_refuses_escapes_by_itself() {
    // Every target is the test's own: a sentinel beside the room, in the private parent.
    let room = Room::new(&[("data/a.txt", "a")]);
    let sentinel = room.sentinel("sentinel.txt", "outside-bytes");
    let escape = room.outside("escape.txt");
    let before = room.outside_snapshot();
    for path in [
        sentinel.as_path(),
        Path::new("../outside/sentinel.txt"),
        Path::new("data/../../outside/sentinel.txt"),
    ] {
        let read = room.fs.read(path).await;
        assert!(
            matches!(read, Err(FsError::PermissionDenied { .. })),
            "{}: {read:?}",
            path.display()
        );
    }
    room.to_run().await;
    for path in [
        escape.as_path(),
        Path::new("../outside/escape.txt"),
        Path::new("out/../../outside/escape.txt"),
    ] {
        let write = room.fs.write(path, b"x").await;
        let exclusive = room.fs.write_new(path, b"x").await;
        for refused in [write, exclusive] {
            assert!(
                matches!(refused, Err(FsError::PermissionDenied { .. })),
                "{}: {refused:?}",
                path.display()
            );
        }
    }
    assert_eq!(
        room.outside_snapshot(),
        before,
        "nothing outside the room changed"
    );
}

#[tokio::test]
async fn a_symlink_at_any_component_is_refused_by_the_backend() {
    let room = Room::new(&[("real/a.txt", "inside")]);
    room.sentinel("dir/secret.txt", "secret-bytes");
    std::os::unix::fs::symlink(room.outside("dir"), room.host("linkdir")).unwrap();
    std::os::unix::fs::symlink(room.host("real/a.txt"), room.host("link.txt")).unwrap();
    for path in ["linkdir/secret.txt", "link.txt"] {
        let read = room.fs.read(Path::new(path)).await;
        assert!(
            matches!(read, Err(FsError::SymlinkRefused { .. })),
            "{path}: {read:?}"
        );
        let pinned = room.fs.read_pinned(Path::new(path)).await;
        assert!(
            matches!(pinned, Err(FsError::SymlinkRefused { .. })),
            "{path}: {pinned:?}"
        );
        let canonical = room.fs.canonicalize(Path::new(path)).await;
        assert!(
            matches!(canonical, Err(FsError::SymlinkRefused { .. })),
            "{path}: {canonical:?}"
        );
    }
}

#[test]
fn a_special_file_is_refused_without_waiting_for_a_peer() {
    // The read runs on its own runtime beside the owned watchdog, both joined before any
    // verdict. The watchdog's peer frees a read stuck in the open; its intervention is a bound
    // exceeded, HARNESS_INVALID and never a pass, whatever the freed read then answered.
    let room = Room::new(&[]);
    let pipe = room.host("pipe");
    nix::unistd::mkfifo(&pipe, Mode::S_IRWXU).unwrap();
    let (read, intervened) = read_beside_watchdog(&pipe, BOUND, room.fs.read(Path::new("pipe")));
    assert!(
        !intervened,
        "HARNESS_INVALID: bound exceeded, the watchdog had to open a peer for the read, which is \
         never a pass whatever the freed read answered"
    );
    let read = read.expect("a special file is refused within the bound");
    assert!(matches!(read, Err(FsError::InvalidData { .. })), "{read:?}");
}

#[test]
fn the_watchdog_reports_a_read_that_waited_for_a_peer() {
    // The harness proves its own release: a read that blocks the runtime thread in a FIFO
    // open only completes through the watchdog's peer, and the intervention is reported, so
    // the witness above can never pass that way. No code under test runs here.
    let room = Room::new(&[]);
    let pipe = room.host("pipe");
    nix::unistd::mkfifo(&pipe, Mode::S_IRWXU).unwrap();
    let (read, intervened) = read_beside_watchdog(&pipe, SHORT, async { std::fs::read(&pipe) });
    assert!(intervened, "a read freed by the watchdog went unreported");
    assert!(
        matches!(&read, Ok(Ok(bytes)) if bytes.is_empty()),
        "the freed read ends on the closed peer: {read:?}"
    );
}

#[tokio::test]
async fn metadata_and_existence_never_follow_a_symlink() {
    let room = Room::new(&[("real/a.txt", "inside")]);
    std::os::unix::fs::symlink(room.host("real/a.txt"), room.host("link.txt")).unwrap();
    let file = room.fs.metadata(Path::new("./real/a.txt")).await.unwrap();
    let dot = room.fs.metadata(Path::new(".")).await.unwrap();
    let linked = room.fs.metadata(Path::new("link.txt")).await;
    assert!(file.is_file && !file.is_dir && file.len == 6, "{file:?}");
    assert!(dot.is_dir, "{dot:?}");
    assert!(
        matches!(linked, Err(FsError::SymlinkRefused { .. })),
        "{linked:?}"
    );
    assert!(room.fs.exists(Path::new("real")).await);
    assert!(!room.fs.exists(Path::new("link.txt")).await);
    assert!(!room.fs.exists(Path::new("../outside")).await);
}

#[tokio::test]
async fn listing_and_globbing_answer_in_caller_coordinates_without_following() {
    let room = Room::new(&[
        ("data/a.csv", "1"),
        ("data/deep/b.csv", "2"),
        ("data/.hidden/c.csv", "3"),
    ]);
    room.sentinel("dir/d.csv", "4");
    std::os::unix::fs::symlink(room.outside("dir"), room.host("data/linkdir")).unwrap();
    let listed = room.fs.list_dir(Path::new("./data")).await.unwrap();
    let globbed = room.fs.glob(Path::new("data"), "**/*.csv").await.unwrap();
    assert_eq!(
        listed,
        [
            "./data/.hidden",
            "./data/a.csv",
            "./data/deep",
            "./data/linkdir"
        ]
        .map(PathBuf::from)
    );
    assert_eq!(
        globbed,
        ["data/a.csv", "data/deep/b.csv"].map(PathBuf::from)
    );
}

// ─── Mutations ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_write_over_a_symlinked_name_replaces_the_name_and_spares_the_target() {
    let room = Room::new(&[]);
    let victim = room.sentinel("victim.txt", "untouched");
    std::fs::create_dir(room.host("out")).unwrap();
    std::os::unix::fs::symlink(&victim, room.host("out/top.json")).unwrap();
    room.to_run().await;
    room.fs
        .write(Path::new("out/top.json"), b"[]")
        .await
        .unwrap();
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "untouched");
    let placed = std::fs::symlink_metadata(room.host("out/top.json")).unwrap();
    assert!(
        placed.file_type().is_file(),
        "the name now holds a regular file"
    );
}

#[tokio::test]
async fn mutations_through_a_symlinked_parent_are_refused_and_the_outside_is_intact() {
    let room = Room::new(&[]);
    room.sentinel("dir/secret.txt", "secret-bytes");
    std::os::unix::fs::symlink(room.outside("dir"), room.host("linkdir")).unwrap();
    std::os::unix::fs::symlink(room.outside("dir/secret.txt"), room.host("link.txt")).unwrap();
    room.to_run().await;
    let before = room.outside_snapshot();
    let mutations = [
        (
            "write",
            room.fs.write(Path::new("linkdir/new.txt"), b"x").await,
        ),
        (
            "write_new",
            room.fs.write_new(Path::new("linkdir/new.txt"), b"x").await,
        ),
        (
            "create_dir_all",
            room.fs.create_dir_all(Path::new("linkdir/sub")).await,
        ),
        (
            "remove_file",
            room.fs.remove_file(Path::new("linkdir/secret.txt")).await,
        ),
    ];
    let unlinked = room.fs.remove_file(Path::new("link.txt")).await;
    for (what, refused) in mutations {
        assert!(
            matches!(refused, Err(FsError::SymlinkRefused { .. })),
            "{what}: {refused:?}"
        );
    }
    assert!(unlinked.is_ok(), "{unlinked:?}");
    assert!(
        std::fs::symlink_metadata(room.host("link.txt")).is_err(),
        "the symlinked name itself is gone"
    );
    assert_eq!(
        room.outside_snapshot(),
        before,
        "the outside is intact, the target included"
    );
    assert!(room.fs.ledger().written().next().is_none());
}

#[tokio::test]
async fn an_exclusive_write_claims_only_a_free_name() {
    // Three bytes of budget: each refused claim returns what it reserved, so the free name can
    // still take all three, and then nothing is left.
    let room = Room::with_limits(&[("out/taken.json", "old")], RoomLimits::new(3, 8));
    let victim = room.sentinel("victim.txt", "untouched");
    std::os::unix::fs::symlink(&victim, room.host("out/link.json")).unwrap();
    room.to_run().await;
    let taken = room.fs.write_new(Path::new("out/taken.json"), b"new").await;
    let linked = room.fs.write_new(Path::new("out/link.json"), b"new").await;
    let free = room.fs.write_new(Path::new("out/free.json"), b"new").await;
    let over = room.fs.write(Path::new("out/more.json"), b"1").await;
    assert!(
        matches!(taken, Err(FsError::AlreadyExists { .. })),
        "{taken:?}"
    );
    assert!(
        matches!(linked, Err(FsError::AlreadyExists { .. })),
        "{linked:?}"
    );
    assert!(free.is_ok(), "{free:?}");
    assert!(
        matches!(&over, Err(FsError::Io { reason }) if reason.contains("budget")),
        "the exclusive write was charged: {over:?}"
    );
    assert_eq!(
        std::fs::read_to_string(room.host("out/taken.json")).unwrap(),
        "old"
    );
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "untouched");
    assert_eq!(
        std::fs::read_to_string(room.host("out/free.json")).unwrap(),
        "new"
    );
    assert_eq!(
        room.fs.ledger().written().collect::<Vec<_>>(),
        vec![PathBuf::from("out/free.json")]
    );
    assert_eq!(
        names(&room.host("out")),
        ["free.json", "link.json", "taken.json"],
        "no temporary name is left"
    );
    assert_eq!(room.fs.ledger().leftovers(), 0);
}

#[tokio::test]
async fn a_failed_publish_is_never_write_evidence_leaves_no_temp_and_returns_its_budget() {
    // A directory already holds the second target's name, so its rename fails. Only the first
    // publish is evidence, nothing is left beside the names, and the failed write's two bytes
    // come back: the third write takes them, and then nothing is left.
    let room = Room::with_limits(&[("out/top.json/keep.txt", "dir")], RoomLimits::new(4, 8));
    room.to_run().await;
    room.fs
        .write(Path::new("out/ok.json"), b"{}")
        .await
        .unwrap();
    let failed = room.fs.write(Path::new("out/top.json"), b"[]").await;
    let third = room.fs.write(Path::new("out/two.json"), b"ab").await;
    let over = room.fs.write(Path::new("out/more.json"), b"1").await;
    assert!(failed.is_err(), "{failed:?}");
    assert!(
        third.is_ok(),
        "the failed write's budget came back: {third:?}"
    );
    assert!(
        matches!(&over, Err(FsError::Io { reason }) if reason.contains("budget")),
        "{over:?}"
    );
    assert_eq!(
        room.fs.ledger().written().collect::<Vec<_>>(),
        vec![PathBuf::from("out/ok.json"), PathBuf::from("out/two.json")]
    );
    assert_eq!(
        std::fs::read_to_string(room.host("out/top.json/keep.txt")).unwrap(),
        "dir"
    );
    assert_eq!(
        names(&room.host("out")),
        ["ok.json", "top.json", "two.json"],
        "no temporary file is left"
    );
    assert_eq!(room.fs.ledger().leftovers(), 0);
}

#[tokio::test]
async fn a_full_room_refuses_the_write_that_would_overdraw_it() {
    let room = Room::with_limits(&[], RoomLimits::new(8, 8));
    room.to_run().await;
    room.fs
        .write(Path::new("a.txt"), b"12345678")
        .await
        .unwrap();
    let over = room.fs.write(Path::new("b.txt"), b"9").await;
    assert!(
        matches!(&over, Err(FsError::Io { reason }) if reason.contains("budget")),
        "{over:?}"
    );
    assert!(!room.host("b.txt").exists());
    assert_eq!(
        room.fs.ledger().written().collect::<Vec<_>>(),
        vec![PathBuf::from("a.txt")]
    );
}

#[tokio::test]
async fn a_copy_in_through_the_backend_is_charged_but_never_run_evidence() {
    // Ten bytes and four files. The copy-in takes six bytes and a file; the run's overwrite of
    // the same name four bytes and a second file; removing it refunds nothing, so one more
    // byte is refused.
    let room = Room::with_limits(&[], RoomLimits::new(10, 4));
    room.fs.write(Path::new("in.txt"), b"123456").await.unwrap();
    room.to_run().await;
    let copied = room.fs.ledger().written().next();
    room.fs.write(Path::new("in.txt"), b"7890").await.unwrap();
    room.fs.remove_file(Path::new("in.txt")).await.unwrap();
    let over = room.fs.write(Path::new("x.txt"), b"1").await;
    assert!(
        copied.is_none(),
        "a copy-in write is never run evidence: {copied:?}"
    );
    assert!(
        matches!(&over, Err(FsError::Io { reason }) if reason.contains("budget")),
        "{over:?}"
    );
    assert_eq!(
        room.fs.ledger().written().collect::<Vec<_>>(),
        vec![PathBuf::from("in.txt")]
    );
    assert!(!room.host("in.txt").exists() && !room.host("x.txt").exists());
}

#[tokio::test]
async fn every_directory_a_write_creates_is_one_budgeted_file() {
    // Two files of budget. The nested write needs its file and two new directories: the
    // first directory takes the second file, the next one is refused naming the budget, and
    // nothing is published. The kept directory stays charged.
    let room = Room::with_limits(&[], RoomLimits::new(1024, 2));
    room.to_run().await;
    let nested = room.fs.write(Path::new("a/b/c.txt"), b"x").await;
    let beside = room.fs.write(Path::new("a/top.txt"), b"x").await;
    let full = room.fs.write(Path::new("a/more.txt"), b"x").await;
    assert!(
        matches!(&nested, Err(FsError::Io { reason }) if reason.contains("budget")),
        "{nested:?}"
    );
    assert!(room.host("a").is_dir() && !room.host("a/b").exists());
    assert!(beside.is_ok(), "{beside:?}");
    assert!(
        matches!(&full, Err(FsError::Io { reason }) if reason.contains("budget")),
        "{full:?}"
    );
    assert_eq!(
        room.fs.ledger().written().collect::<Vec<_>>(),
        vec![PathBuf::from("a/top.txt")]
    );
}

// ─── Phases ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn the_read_back_phase_refuses_every_write() {
    let room = Room::new(&[("out/kept.txt", "kept")]);
    room.to_run().await;
    assert_eq!(next_phase(room.fs.ledger()).await, Phase::ReadBack);
    let mutations = [
        (
            "write",
            room.fs.write(Path::new("out/late.txt"), b"x").await,
        ),
        (
            "write_new",
            room.fs.write_new(Path::new("out/late.txt"), b"x").await,
        ),
        (
            "create_dir_all",
            room.fs.create_dir_all(Path::new("out")).await,
        ),
        (
            "remove_file",
            room.fs.remove_file(Path::new("out/kept.txt")).await,
        ),
    ];
    for (what, refused) in mutations {
        assert!(
            matches!(refused, Err(FsError::PermissionDenied { .. })),
            "{what}: {refused:?}"
        );
    }
    assert!(!room.host("out/late.txt").exists());
    assert_eq!(
        std::fs::read_to_string(room.host("out/kept.txt")).unwrap(),
        "kept"
    );
    assert!(room.fs.ledger().written().next().is_none());
    assert_eq!(
        room.fs.ledger().late_refusals(),
        4,
        "a write after the run is a late producer"
    );
}

#[tokio::test]
async fn a_closed_room_refuses_reads_and_mutations() {
    let room = Room::new(&[("out/kept.txt", "kept")]);
    room.to_run().await;
    assert_eq!(next_phase(room.fs.ledger()).await, Phase::ReadBack);
    assert_eq!(next_phase(room.fs.ledger()).await, Phase::Closed);
    let read = room.fs.read(Path::new("out/kept.txt")).await;
    let metadata = room.fs.metadata(Path::new("out/kept.txt")).await;
    let listed = room.fs.list_dir(Path::new("out")).await;
    let found = room.fs.exists(Path::new("out/kept.txt")).await;
    let mutations = [
        (
            "write",
            room.fs.write(Path::new("out/late.txt"), b"x").await,
        ),
        (
            "write_new",
            room.fs.write_new(Path::new("out/late.txt"), b"x").await,
        ),
        (
            "create_dir_all",
            room.fs.create_dir_all(Path::new("out")).await,
        ),
        (
            "remove_file",
            room.fs.remove_file(Path::new("out/kept.txt")).await,
        ),
    ];
    assert!(
        matches!(read, Err(FsError::PermissionDenied { .. })),
        "{read:?}"
    );
    assert!(
        matches!(metadata, Err(FsError::PermissionDenied { .. })),
        "{metadata:?}"
    );
    assert!(
        matches!(listed, Err(FsError::PermissionDenied { .. })),
        "{listed:?}"
    );
    assert!(!found, "a closed room answers no existence");
    for (what, refused) in mutations {
        assert!(
            matches!(refused, Err(FsError::PermissionDenied { .. })),
            "{what}: {refused:?}"
        );
    }
    assert_eq!(
        std::fs::read_to_string(room.host("out/kept.txt")).unwrap(),
        "kept"
    );
}

// ─── Bounds ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_room_read_never_consumes_past_the_room_bound() {
    // A file placed in the room behind the ledger's back, larger than the room's whole
    // budget: the read refuses, naming the bound, instead of loading it.
    let room = Room::with_limits(&[], RoomLimits::new(1024, 8));
    std::fs::write(room.host("big.bin"), vec![b'x'; 4096]).unwrap();
    let read = room.fs.read(Path::new("big.bin")).await;
    assert!(
        matches!(&read, Err(FsError::InvalidData { reason, .. }) if reason.contains("bound")),
        "{read:?}"
    );
}

#[tokio::test]
async fn a_room_bound_that_is_not_representable_refuses_every_read() {
    let room = Room::with_limits(&[("a.txt", "a")], RoomLimits::new(u64::MAX, 8));
    let read = room.fs.read(Path::new("a.txt")).await;
    assert!(
        matches!(&read, Err(FsError::Io { reason }) if reason.contains("not representable")),
        "{read:?}"
    );
}
