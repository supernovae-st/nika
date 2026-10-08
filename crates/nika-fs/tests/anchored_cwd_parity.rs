// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `AnchoredFs` at a root answers every operation exactly as `TokioFs` does
//! from a working directory at that root: the same values, the same error
//! variants and the same path spellings, for relative paths of every shape.
//!
//! The oracle is the host backend itself under a changed working directory.
//! Changing it is process-wide, so this proof is its own integration-test
//! process holding exactly ONE test, and the directory is restored on every
//! exit path by a drop guard. The scenario is built twice at the same path:
//! once for `TokioFs` from inside it, once for `AnchoredFs` from an empty
//! decoy directory, which must stay empty: an unanchored call would land
//! there, never in the repository. Both run the same calls in the same
//! order, writes and removals included, and each answer is one line.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use nika_fs::{AnchoredFs, TokioFs};
use nika_kernel::fs::{FsListDyn, FsMetaDyn, FsReadDyn, FsWriteDyn};

const BRIEF: &[u8] = "Brief · première ligne\nsecond line\n".as_bytes();

/// One kernel call with its relative arguments.
#[derive(Debug)]
enum Op {
    Read(&'static str),
    Text(&'static str),
    Pinned(&'static str),
    Exists(&'static str),
    Canonical(&'static str),
    Meta(&'static str),
    IsDir(&'static str),
    List(&'static str),
    Glob(&'static str, &'static str),
    Write(&'static str, &'static [u8]),
    WriteNew(&'static str, &'static [u8]),
    Mkdir(&'static str),
    RemoveRegular(&'static str),
    Remove(&'static str),
}

/// The calls, in order: reads and queries first, then the effects, then the
/// state the effects left.
const OPS: &[Op] = &[
    Op::Read("./notes/brief.md"),
    Op::Read("notes/link.md"),
    Op::Read("notes/absent.md"),
    Op::Read("../outside.txt"),
    Op::Read(""),
    Op::Text("notes/a.md"),
    Op::Text("notes/bin.dat"),
    Op::Pinned("notes/brief.md"),
    Op::Pinned("notes/link.md"),
    Op::Pinned("./notes/none"),
    Op::Exists("notes/brief.md"),
    Op::Exists("./notes"),
    Op::Exists("."),
    Op::Exists(".."),
    Op::Exists("absent"),
    Op::Exists(""),
    Op::Canonical("."),
    Op::Canonical("./notes"),
    Op::Canonical("notes/link.md"),
    Op::Canonical("../outside.txt"),
    Op::Canonical("notes/absent.md"),
    Op::Canonical(""),
    Op::Meta("notes/brief.md"),
    Op::Meta("absent"),
    Op::IsDir("./notes"),
    Op::List("."),
    Op::List("./notes"),
    Op::List("notes"),
    Op::List("notes/sub"),
    Op::List("./absent"),
    Op::Glob(".", "**/*.md"),
    Op::Glob("./notes", "*.md"),
    Op::Glob("notes", "**"),
    Op::Glob("./absent", "*"),
    Op::Glob(".", "["),
    Op::Write("out/copy.md", BRIEF),
    Op::Write("./out/new/deep.md", b"deep"),
    Op::Write("bare.md", b"bare"),
    Op::Write("./notes/sub", b"x"),
    Op::Write("", b"x"),
    Op::WriteNew("out/fresh.md", b"1"),
    Op::WriteNew("out/fresh.md", b"2"),
    Op::WriteNew("./out/link", b"3"),
    Op::Mkdir("made/deep"),
    Op::Mkdir("out/keep.md/x"),
    Op::Read("out/copy.md"),
    Op::Read("./out/new/deep.md"),
    Op::Read("bare.md"),
    Op::Read("out/fresh.md"),
    Op::Read("out/keep.md"),
    Op::RemoveRegular("./out/stale.md"),
    Op::RemoveRegular("out/stale.md"),
    Op::RemoveRegular("out/dir"),
    Op::RemoveRegular("out/link"),
    Op::RemoveRegular("out/."),
    Op::RemoveRegular("out/"),
    Op::RemoveRegular(".."),
    Op::RemoveRegular("notes/sub/"),
    Op::Remove("out/copy.md"),
    Op::Remove("out/copy.md"),
    Op::List("out"),
    Op::List("./out/new"),
];

/// The full answer to one call, as one comparable line.
async fn answer<F>(fs: &F, op: &Op) -> String
where
    F: FsReadDyn + FsWriteDyn + FsMetaDyn + FsListDyn,
{
    let p = Path::new;
    match *op {
        Op::Read(path) => format!("{:?}", fs.read(p(path)).await),
        Op::Text(path) => format!("{:?}", fs.read_to_string(p(path)).await),
        Op::Pinned(path) => format!("{:?}", fs.read_pinned(p(path)).await),
        Op::Exists(path) => format!("{:?}", fs.exists(p(path)).await),
        Op::Canonical(path) => format!("{:?}", fs.canonicalize(p(path)).await),
        Op::Meta(path) => format!("{:?}", fs.metadata(p(path)).await),
        Op::IsDir(path) => format!("{:?}", fs.metadata(p(path)).await.map(|m| m.is_dir)),
        Op::List(path) => format!("{:?}", fs.list_dir(p(path)).await),
        Op::Glob(root, pattern) => format!("{:?}", fs.glob(p(root), pattern).await),
        Op::Write(path, bytes) => format!("{:?}", fs.write(p(path), bytes).await),
        Op::WriteNew(path, bytes) => format!("{:?}", fs.write_new(p(path), bytes).await),
        Op::Mkdir(path) => format!("{:?}", fs.create_dir_all(p(path)).await),
        Op::RemoveRegular(path) => format!("{:?}", fs.remove_regular_file(p(path)).await),
        Op::Remove(path) => format!("{:?}", fs.remove_file(p(path)).await),
    }
}

async fn battery<F>(fs: &F) -> Vec<String>
where
    F: FsReadDyn + FsWriteDyn + FsMetaDyn + FsListDyn,
{
    let mut lines = Vec::with_capacity(OPS.len());
    for op in OPS {
        lines.push(format!("{op:?} -> {}", answer(fs, op).await));
    }
    lines
}

/// Restores the process's original working directory when dropped.
struct Return(PathBuf);

impl Drop for Return {
    fn drop(&mut self) {
        std::env::set_current_dir(&self.0).expect("return to the original directory");
    }
}

/// The scenario under `root`: files, a nested and a hidden directory, two
/// symlinks, a non-UTF-8 file, and a sibling of the root for `..` paths.
fn build(root: &Path) {
    let _ = std::fs::remove_dir_all(root);
    for dir in ["notes/sub", "notes/.hidden", "out/dir"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    let files: [(&str, &[u8]); 7] = [
        ("notes/brief.md", BRIEF),
        ("notes/a.md", b"a"),
        ("notes/sub/c.md", b"c"),
        ("notes/.hidden/d.md", b"d"),
        ("notes/bin.dat", &[0xff, 0xfe, 0x00]),
        ("out/stale.md", b"stale"),
        ("out/keep.md", b"keep"),
    ];
    for (name, bytes) in files {
        std::fs::write(root.join(name), bytes).unwrap();
    }
    std::os::unix::fs::symlink("brief.md", root.join("notes/link.md")).unwrap();
    std::os::unix::fs::symlink("keep.md", root.join("out/link")).unwrap();
    std::fs::write(root.parent().unwrap().join("outside.txt"), b"outside").unwrap();
}

/// Run `battery` with the process working directory at `dir`, restored after.
fn from_dir<F>(dir: &Path, fs: &F) -> Vec<String>
where
    F: FsReadDyn + FsWriteDyn + FsMetaDyn + FsListDyn,
{
    let original = std::env::current_dir().unwrap();
    let lines = {
        let _return = Return(original.clone());
        std::env::set_current_dir(dir).unwrap();
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(battery(fs))
    };
    assert_eq!(std::env::current_dir().unwrap(), original, "restored");
    lines
}

#[test]
fn an_anchored_root_answers_every_operation_as_a_working_directory_there() {
    let owned = tempfile::tempdir().unwrap();
    let root = owned.path().join("project");
    let decoy = tempfile::tempdir().unwrap();

    build(&root);
    let from_inside = from_dir(&root, &TokioFs);
    build(&root);
    let anchored = from_dir(decoy.path(), &AnchoredFs::new(&root));
    assert_eq!(
        std::fs::read_dir(decoy.path()).unwrap().count(),
        0,
        "no anchored call reached the process directory"
    );

    assert_eq!(anchored.len(), OPS.len(), "one answer per call");
    for (anchored, oracle) in anchored.iter().zip(&from_inside) {
        assert_eq!(
            anchored, oracle,
            "the anchored answer diverges from the oracle"
        );
    }
    // The battery reached real successes and spellings, not only refusals.
    for expected in [
        "Read(\"./notes/brief.md\") -> Ok(",
        "Pinned(\"notes/link.md\") -> Err(SymlinkRefused { path: \"notes/link.md\" })",
        "WriteNew(\"out/fresh.md\", [49]) -> Ok(())",
        "RemoveRegular(\"./out/stale.md\") -> Ok(())",
        "List(\"./notes\") -> Ok([\"./notes/.hidden\", \"./notes/a.md\"",
        "Glob(\"notes\", \"**\") -> Ok([\"notes/a.md\"",
    ] {
        assert!(
            anchored.iter().any(|line| line.starts_with(expected)),
            "missing answer `{expected}` in {anchored:#?}"
        );
    }
}
