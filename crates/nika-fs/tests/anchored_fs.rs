// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! `AnchoredFs` resolves relative paths at its root, not at the process
//! working directory, and leaves absolute and empty paths alone.
//!
//! Every test anchors at a temporary root that is not the process directory
//! (Cargo runs integration tests from the package directory), so a relative
//! call that leaked to the process directory would miss its fixture or leave
//! an entry beside `Cargo.toml`. The exact parity with a working directory at
//! the root, call by call, is `anchored_cwd_parity.rs`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use nika_fs::{AnchoredFs, TokioFs};
use nika_kernel::fs::{FileMetadata, FsError, FsListDyn, FsMetaDyn, FsReadDyn, FsWriteDyn};

const BRIEF: &[u8] = "Brief · première ligne\nsecond line\n".as_bytes();

/// The names relative calls use here never exist beside the process.
const RELATIVE_NAMES: [&str; 3] = ["notes", "out", "made"];

fn assert_process_dir_untouched() {
    let cwd = std::env::current_dir().unwrap();
    for name in RELATIVE_NAMES {
        assert!(
            !cwd.join(name).exists(),
            "`{name}` appeared under the process directory {}",
            cwd.display()
        );
    }
}

fn root_with(files: &[(&str, &[u8])]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (name, bytes) in files {
        let path = dir.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    dir
}

fn paths(spelled: &[&str]) -> Vec<PathBuf> {
    spelled.iter().map(PathBuf::from).collect()
}

// ─── type-level guarantees ───────────────────────────────────────────

#[test]
fn the_adapter_is_a_full_send_backend() {
    fn accepts<T>(_: &T)
    where
        T: nika_kernel::Fs + FsReadDyn + FsWriteDyn + FsMetaDyn + FsListDyn + Clone,
        T: std::fmt::Debug,
        T: Send + Sync + 'static,
    {
    }
    accepts(&AnchoredFs::new("/any/root"));
    assert_eq!(AnchoredFs::new("/any/root").root(), Path::new("/any/root"));
}

// ─── relative paths resolve at the root ──────────────────────────────

#[tokio::test]
async fn relative_reads_resolve_at_the_root_not_the_process_directory() {
    let dir = root_with(&[("notes/brief.md", BRIEF)]);
    let fs = AnchoredFs::new(dir.path());

    assert_eq!(
        fs.read(Path::new("./notes/brief.md"))
            .await
            .unwrap()
            .as_ref(),
        BRIEF
    );
    assert_eq!(
        fs.read_to_string(Path::new("notes/brief.md"))
            .await
            .unwrap()
            .as_bytes(),
        BRIEF
    );
    assert_eq!(
        fs.read_pinned(Path::new("./notes/brief.md"))
            .await
            .unwrap()
            .as_ref(),
        BRIEF,
        "the host pin serves the anchored file, never the refusing default"
    );
    assert!(fs.exists(Path::new("notes/brief.md")).await);
    assert!(!fs.exists(Path::new("notes/absent.md")).await);
    assert_eq!(
        fs.metadata(Path::new("./notes/brief.md")).await.unwrap(),
        FileMetadata::new(u64::try_from(BRIEF.len()).unwrap(), true, false)
    );
    let physical = std::fs::canonicalize(dir.path()).unwrap();
    assert_eq!(fs.canonicalize(Path::new(".")).await.unwrap(), physical);
    assert_eq!(
        fs.canonicalize(Path::new("./notes/brief.md"))
            .await
            .unwrap(),
        physical.join("notes/brief.md"),
        "the identity stays physical and absolute"
    );

    // The unanchored host backend reads the same spelling at the process
    // directory, where the brief does not exist.
    assert!(matches!(
        TokioFs.read(Path::new("./notes/brief.md")).await,
        Err(FsError::NotFound { .. })
    ));
    assert_process_dir_untouched();
}

#[tokio::test]
async fn relative_effects_land_under_the_root() {
    let dir = root_with(&[("out/stale.md", b"stale"), ("out/keep.md", b"keep")]);
    let fs = AnchoredFs::new(dir.path());
    // Canary before any effect: an unanchored backend fails here, at the
    // process directory, instead of writing beside `Cargo.toml` below.
    assert_eq!(
        fs.read(Path::new("out/keep.md")).await.unwrap().as_ref(),
        b"keep"
    );

    fs.write(Path::new("out/copy.md"), BRIEF).await.unwrap();
    assert_eq!(
        std::fs::read(dir.path().join("out/copy.md")).unwrap(),
        BRIEF
    );
    fs.write_new(Path::new("./out/new.md"), b"fresh")
        .await
        .unwrap();
    assert!(
        matches!(
            fs.write_new(Path::new("./out/new.md"), b"again").await,
            Err(FsError::AlreadyExists { ref path }) if path == "./out/new.md"
        ),
        "an occupied name refuses in the caller's spelling"
    );
    assert_eq!(
        std::fs::read(dir.path().join("out/new.md")).unwrap(),
        b"fresh"
    );
    fs.create_dir_all(Path::new("made/deep")).await.unwrap();
    assert!(dir.path().join("made/deep").is_dir());

    fs.remove_regular_file(Path::new("./out/stale.md"))
        .await
        .unwrap();
    assert!(!dir.path().join("out/stale.md").exists());
    assert!(
        matches!(
            fs.remove_regular_file(Path::new("out/stale.md")).await,
            Err(FsError::NotFound { ref path }) if path == "out/stale.md"
        ),
        "an absent name refuses in the caller's spelling"
    );
    fs.remove_file(Path::new("out/copy.md")).await.unwrap();
    assert_eq!(
        fs.list_dir(Path::new("out")).await.unwrap(),
        paths(&["out/keep.md", "out/new.md"]),
        "only the named files moved, and no staging file is left"
    );
    assert_eq!(
        std::fs::read(dir.path().join("out/keep.md")).unwrap(),
        b"keep"
    );
    assert_process_dir_untouched();
}

#[tokio::test]
async fn listings_keep_the_callers_spelling() {
    let dir = root_with(&[
        ("notes/a.md", b"a"),
        ("notes/b.md", b"b"),
        ("notes/sub/c.md", b"c"),
        ("notes/.hidden/d.md", b"d"),
    ]);
    let fs = AnchoredFs::new(dir.path());

    assert_eq!(
        fs.list_dir(Path::new("./notes")).await.unwrap(),
        paths(&[
            "./notes/.hidden",
            "./notes/a.md",
            "./notes/b.md",
            "./notes/sub"
        ])
    );
    assert_eq!(
        fs.glob(Path::new("."), "**/*.md").await.unwrap(),
        paths(&["./notes/a.md", "./notes/b.md", "./notes/sub/c.md"]),
        "the walk skips hidden directories and answers relative to `.`"
    );
    assert_eq!(
        fs.glob(Path::new("notes"), "*.md").await.unwrap(),
        paths(&["notes/a.md", "notes/b.md"])
    );
    assert!(
        matches!(
            fs.glob(Path::new("./absent"), "*").await,
            Err(FsError::NotFound { ref path }) if path == "./absent"
        ),
        "a missing walk root is named as the caller wrote it"
    );
}

// ─── what the anchor does not change ─────────────────────────────────

#[tokio::test]
async fn absolute_paths_keep_their_meaning() {
    let anchored_at = root_with(&[("notes/brief.md", b"anchored")]);
    let elsewhere = root_with(&[("shared/brief.md", BRIEF)]);
    let fs = AnchoredFs::new(anchored_at.path());
    let shared = elsewhere.path().join("shared");

    assert_eq!(
        fs.read(&shared.join("brief.md")).await.unwrap().as_ref(),
        BRIEF
    );
    assert_eq!(
        fs.list_dir(&shared).await.unwrap(),
        TokioFs.list_dir(&shared).await.unwrap(),
        "an absolute listing stays absolute"
    );
    assert_eq!(
        fs.glob(elsewhere.path(), "**/*.md").await.unwrap(),
        vec![shared.join("brief.md")]
    );
    assert_eq!(
        fs.canonicalize(&shared).await.unwrap(),
        std::fs::canonicalize(&shared).unwrap()
    );
    fs.write(&shared.join("copy.md"), b"copy").await.unwrap();
    assert_eq!(std::fs::read(shared.join("copy.md")).unwrap(), b"copy");
    let missing = shared.join("missing.md");
    assert!(
        matches!(
            fs.read(&missing).await,
            Err(FsError::NotFound { ref path }) if *path == missing.display().to_string()
        ),
        "an absolute error keeps its absolute spelling"
    );
    assert!(
        !anchored_at.path().join("shared").exists(),
        "nothing re-rooted"
    );
}

#[tokio::test]
async fn parent_paths_resolve_from_the_root_and_are_not_confined() {
    let base = root_with(&[("outside.txt", b"outside"), ("project/in.txt", b"in")]);
    let fs = AnchoredFs::new(base.path().join("project"));

    // The adapter is not a sandbox: `..` climbs exactly as from a working
    // directory at the root. Confinement is the `permits.fs` boundary's.
    assert_eq!(
        fs.read(Path::new("../outside.txt")).await.unwrap().as_ref(),
        b"outside"
    );
    assert_eq!(
        fs.canonicalize(Path::new("../outside.txt")).await.unwrap(),
        std::fs::canonicalize(base.path().join("outside.txt")).unwrap()
    );
}

#[tokio::test]
async fn symlinks_resolve_as_from_a_working_directory_at_the_root() {
    let dir = root_with(&[("notes/brief.md", BRIEF)]);
    std::os::unix::fs::symlink("brief.md", dir.path().join("notes/link.md")).unwrap();
    let fs = AnchoredFs::new(dir.path());

    assert_eq!(
        fs.read(Path::new("notes/link.md")).await.unwrap().as_ref(),
        BRIEF
    );
    assert!(
        matches!(
            fs.read_pinned(Path::new("notes/link.md")).await,
            Err(FsError::SymlinkRefused { ref path }) if path == "notes/link.md"
        ),
        "the final-component pin still refuses a symlinked name"
    );
    assert!(matches!(
        fs.remove_regular_file(Path::new("./notes/link.md")).await,
        Err(FsError::SymlinkRefused { .. })
    ));
    assert_eq!(
        fs.canonicalize(Path::new("notes/link.md")).await.unwrap(),
        std::fs::canonicalize(dir.path().join("notes/brief.md")).unwrap()
    );
}

#[tokio::test]
async fn empty_paths_and_an_empty_root_are_the_host_backend() {
    let dir = root_with(&[("notes/brief.md", BRIEF)]);
    let fs = AnchoredFs::new(dir.path());
    assert_eq!(
        format!("{:?}", fs.read(Path::new("")).await),
        format!("{:?}", TokioFs.read(Path::new("")).await),
        "an empty path is no path at all, anchored or not"
    );
    assert!(!fs.exists(Path::new("")).await);

    // An empty root anchors nothing: the process directory, as for TokioFs.
    let unanchored = AnchoredFs::new("");
    assert_eq!(
        unanchored.read(Path::new("Cargo.toml")).await.unwrap(),
        TokioFs.read(Path::new("Cargo.toml")).await.unwrap()
    );
}

// ─── concurrency ─────────────────────────────────────────────────────

#[tokio::test]
async fn two_roots_serve_the_same_relative_names_concurrently() {
    let first = root_with(&[("notes/brief.md", b"first")]);
    let second = root_with(&[("notes/brief.md", b"second")]);
    let mut tasks = tokio::task::JoinSet::new();
    for (dir, own) in [(&first, "first"), (&second, "second")] {
        let fs = Arc::new(AnchoredFs::new(dir.path()));
        for round in 0..8 {
            let fs = Arc::clone(&fs);
            tasks.spawn(async move {
                let read = fs.read(Path::new("./notes/brief.md")).await.unwrap();
                assert_eq!(
                    read.as_ref(),
                    own.as_bytes(),
                    "each root reads its own brief"
                );
                let name = format!("out/{round}.md");
                fs.write(Path::new(&name), own.as_bytes()).await.unwrap();
            });
        }
    }
    while let Some(joined) = tasks.join_next().await {
        joined.unwrap();
    }
    for (dir, own) in [(&first, "first"), (&second, "second")] {
        for round in 0..8 {
            let written = std::fs::read(dir.path().join(format!("out/{round}.md"))).unwrap();
            assert_eq!(
                written,
                own.as_bytes(),
                "no write crossed into the other root"
            );
        }
    }
    assert_process_dir_untouched();
}
