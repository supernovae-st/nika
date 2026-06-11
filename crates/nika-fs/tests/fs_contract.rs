// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Contract tests for `nika-fs` — the production `TokioFs`.
//!
//! Covers the four kernel trait surfaces (`FsRead` · `FsWrite` · `FsMeta`
//! · `FsList`) against real tempdirs. Includes the Gate 10 parity
//! assertions (behaviour matches the brouillon `TokioFs`: roundtrips,
//! parent auto-creation, hidden-dir glob skip, sorted glob results) plus
//! the Diamond additions (atomic write leaves no temp residue ·
//! `list_dir` · `FsMeta` split · object-safe `*Dyn` variants).

use std::path::{Path, PathBuf};

use nika_fs::TokioFs;
use nika_kernel::fs::FsError;
use nika_kernel::{FileMetadata, Fs, FsList, FsMeta, FsRead, FsWrite};
use tempfile::TempDir;

fn td() -> TempDir {
    tempfile::tempdir().expect("tempdir")
}

// ─── type-level guarantees ───────────────────────────────────────────

#[test]
fn tokiofs_is_zero_sized() {
    assert_eq!(std::mem::size_of::<TokioFs>(), 0);
}

#[test]
fn tokiofs_is_send_sync_copy_default() {
    fn assert_traits<T: Send + Sync + Copy + Default>() {}
    assert_traits::<TokioFs>();
}

#[test]
fn tokiofs_satisfies_blanket_fs() {
    fn accepts_fs<T: Fs>(_: &T) {}
    accepts_fs(&TokioFs);
}

#[test]
fn tokiofs_satisfies_send_future_dyn_variants() {
    // The `*Dyn` trait_variant forms guarantee `Send` futures (they are
    // generic bounds, not dyn-dispatch surfaces — RPITIT is not object
    // safe). TokioFs must satisfy all four via the blanket impls.
    use nika_kernel::fs::{FsListDyn, FsMetaDyn, FsReadDyn, FsWriteDyn};
    fn accepts<T: FsReadDyn + FsWriteDyn + FsMetaDyn + FsListDyn>(_: &T) {}
    accepts(&TokioFs);
}

// ─── FsRead ──────────────────────────────────────────────────────────

#[tokio::test]
async fn read_bytes_roundtrip() {
    let dir = td();
    let path = dir.path().join("binary.bin");
    let fs = TokioFs;

    let data = vec![0u8, 1, 2, 3, 255];
    fs.write(&path, &data).await.unwrap();
    let read_back = fs.read(&path).await.unwrap();
    assert_eq!(&read_back[..], &data[..]);
}

#[tokio::test]
async fn read_to_string_roundtrip() {
    let dir = td();
    let path = dir.path().join("test.txt");
    let fs = TokioFs;

    fs.write(&path, b"hello world").await.unwrap();
    let content = fs.read_to_string(&path).await.unwrap();
    assert_eq!(content, "hello world");
}

#[tokio::test]
async fn read_missing_file_errs_not_found() {
    let dir = td();
    let fs = TokioFs;
    let err = fs.read(&dir.path().join("missing.bin")).await.unwrap_err();
    assert!(
        matches!(err, FsError::NotFound { .. }),
        "NotFound expected, got {err:?}"
    );
}

#[tokio::test]
async fn read_to_string_invalid_utf8_errs() {
    let dir = td();
    let path = dir.path().join("bad.bin");
    let fs = TokioFs;

    fs.write(&path, &[0xFF, 0xFE, 0xFD]).await.unwrap();
    let err = fs.read_to_string(&path).await.unwrap_err();
    assert!(
        matches!(err, FsError::InvalidData { .. }),
        "InvalidData expected, got {err:?}"
    );
}

#[tokio::test]
async fn exists_reports_files_dirs_and_absences() {
    let dir = td();
    let path = dir.path().join("here.txt");
    let fs = TokioFs;

    assert!(!fs.exists(&path).await);
    fs.write(&path, b"x").await.unwrap();
    assert!(fs.exists(&path).await);
    assert!(fs.exists(dir.path()).await);
    assert!(!fs.exists(Path::new("/nonexistent/path/abc123")).await);
}

#[tokio::test]
async fn canonicalize_resolves_absolute() {
    let dir = td();
    let path = dir.path().join("canon.txt");
    let fs = TokioFs;

    fs.write(&path, b"").await.unwrap();
    let canonical = fs.canonicalize(&path).await.unwrap();
    assert!(canonical.is_absolute());
}

#[tokio::test]
async fn canonicalize_missing_errs() {
    let dir = td();
    let fs = TokioFs;
    let err = fs
        .canonicalize(&dir.path().join("ghost"))
        .await
        .unwrap_err();
    assert!(
        matches!(err, FsError::NotFound { .. }),
        "NotFound expected, got {err:?}"
    );
}

// ─── FsWrite ─────────────────────────────────────────────────────────

#[tokio::test]
async fn write_overwrites_existing() {
    let dir = td();
    let path = dir.path().join("over.txt");
    let fs = TokioFs;

    fs.write(&path, b"first").await.unwrap();
    fs.write(&path, b"second").await.unwrap();
    assert_eq!(fs.read_to_string(&path).await.unwrap(), "second");
}

#[tokio::test]
async fn write_creates_parent_dirs() {
    let dir = td();
    let path = dir.path().join("a").join("b").join("c").join("file.txt");
    let fs = TokioFs;

    fs.write(&path, b"nested").await.unwrap();
    assert!(fs.exists(&path).await);
}

#[tokio::test]
async fn write_empty_contents() {
    let dir = td();
    let path = dir.path().join("empty.txt");
    let fs = TokioFs;

    fs.write(&path, b"").await.unwrap();
    assert_eq!(fs.metadata(&path).await.unwrap().len, 0);
}

#[tokio::test]
async fn write_leaves_no_temp_residue() {
    let dir = td();
    let path = dir.path().join("clean.txt");
    let fs = TokioFs;

    fs.write(&path, b"payload").await.unwrap();
    fs.write(&path, b"payload-2").await.unwrap();
    let entries = fs.list_dir(dir.path()).await.unwrap();
    assert_eq!(
        entries,
        vec![path.clone()],
        "atomic write must clean up its temp file on the happy path"
    );
}

#[tokio::test]
async fn write_to_existing_dir_errs_and_cleans_temp() {
    let dir = td();
    let target = dir.path().join("iamadir");
    let fs = TokioFs;

    fs.create_dir_all(&target).await.unwrap();
    let err = fs.write(&target, b"nope").await.unwrap_err();
    assert!(
        matches!(
            err,
            FsError::Io { .. } | FsError::PermissionDenied { .. } | FsError::AlreadyExists { .. }
        ),
        "rename-over-dir must surface the rename failure, got: {err:?}"
    );
    let entries = fs.list_dir(dir.path()).await.unwrap();
    assert_eq!(
        entries,
        vec![target.clone()],
        "failed write must not leave a temp file behind"
    );
}

#[tokio::test]
async fn concurrent_writes_to_one_destination_leave_one_file_no_residue() {
    // The atomic-write contract under contention: N tasks writing the SAME
    // path concurrently (interleaving at every `.await` — temp create, write,
    // rename) must (a) never collide on a temp name — the TMP_COUNTER
    // guarantee, exercised end-to-end here, not just the unit — (b) leave
    // exactly ONE file (the destination, last rename wins), and (c) leave
    // ZERO `.nika-tmp.*` residue. A temp-name collision would surface as a
    // lost write or a leaked sibling; this catches both.
    let dir = td();
    let path = dir.path().join("contended.txt");
    let fs = TokioFs;

    let mut handles = Vec::new();
    for i in 0..32u32 {
        let p = path.clone();
        handles.push(tokio::spawn(async move {
            fs.write(&p, format!("writer-{i}").as_bytes()).await
        }));
    }
    for h in handles {
        h.await.expect("task joins").expect("each write succeeds");
    }

    let entries = fs.list_dir(dir.path()).await.unwrap();
    assert_eq!(
        entries,
        vec![path.clone()],
        "concurrent writes must converge to exactly the destination, no temp residue"
    );
    let content = fs.read_to_string(&path).await.unwrap();
    assert!(
        content.starts_with("writer-"),
        "surviving content must be one intact write (no interleave), got {content:?}"
    );
}

#[tokio::test]
async fn write_path_without_file_name_errs() {
    let fs = TokioFs;
    let err = fs.write(Path::new("/"), b"x").await.unwrap_err();
    assert!(
        matches!(err, FsError::InvalidData { .. }),
        "InvalidInput expected, got {err:?}"
    );
}

#[tokio::test]
async fn create_dir_all_nested_and_idempotent() {
    let dir = td();
    let nested = dir.path().join("x").join("y").join("z");
    let fs = TokioFs;

    fs.create_dir_all(&nested).await.unwrap();
    assert!(fs.exists(&nested).await);
    fs.create_dir_all(&nested).await.unwrap(); // second call must not error
}

#[tokio::test]
async fn remove_file_works() {
    let dir = td();
    let path = dir.path().join("remove_me.txt");
    let fs = TokioFs;

    fs.write(&path, b"doomed").await.unwrap();
    assert!(fs.exists(&path).await);
    fs.remove_file(&path).await.unwrap();
    assert!(!fs.exists(&path).await);
}

#[tokio::test]
async fn remove_missing_file_errs_not_found() {
    let dir = td();
    let fs = TokioFs;
    let err = fs
        .remove_file(&dir.path().join("ghost.txt"))
        .await
        .unwrap_err();
    assert!(
        matches!(err, FsError::NotFound { .. }),
        "NotFound expected, got {err:?}"
    );
}

// ─── FsMeta ──────────────────────────────────────────────────────────

#[tokio::test]
async fn metadata_for_file() {
    let dir = td();
    let path = dir.path().join("meta.txt");
    let fs = TokioFs;

    fs.write(&path, b"12345").await.unwrap();
    let meta = fs.metadata(&path).await.unwrap();
    assert_eq!(meta, FileMetadata::new(5, true, false));
}

#[tokio::test]
async fn metadata_for_dir() {
    let dir = td();
    let fs = TokioFs;

    let meta = fs.metadata(dir.path()).await.unwrap();
    assert!(meta.is_dir);
    assert!(!meta.is_file);
}

#[tokio::test]
async fn metadata_missing_errs_not_found() {
    let dir = td();
    let fs = TokioFs;
    let err = fs.metadata(&dir.path().join("ghost")).await.unwrap_err();
    assert!(
        matches!(err, FsError::NotFound { .. }),
        "NotFound expected, got {err:?}"
    );
}

// ─── FsList · list_dir ───────────────────────────────────────────────

#[tokio::test]
async fn list_dir_returns_sorted_full_paths() {
    let dir = td();
    let fs = TokioFs;

    fs.write(&dir.path().join("b.txt"), b"").await.unwrap();
    fs.write(&dir.path().join("a.txt"), b"").await.unwrap();
    fs.create_dir_all(&dir.path().join("c_dir")).await.unwrap();

    let entries = fs.list_dir(dir.path()).await.unwrap();
    assert_eq!(
        entries,
        vec![
            dir.path().join("a.txt"),
            dir.path().join("b.txt"),
            dir.path().join("c_dir"),
        ],
        "list_dir must return full paths, deterministically sorted"
    );
}

#[tokio::test]
async fn list_dir_empty_dir() {
    let dir = td();
    let fs = TokioFs;
    assert!(fs.list_dir(dir.path()).await.unwrap().is_empty());
}

#[tokio::test]
async fn list_dir_missing_errs_not_found() {
    let dir = td();
    let fs = TokioFs;
    let err = fs.list_dir(&dir.path().join("ghost")).await.unwrap_err();
    assert!(
        matches!(err, FsError::NotFound { .. }),
        "NotFound expected, got {err:?}"
    );
}

// ─── FsList · glob ───────────────────────────────────────────────────

#[tokio::test]
async fn glob_finds_matching_files() {
    let dir = td();
    let fs = TokioFs;

    fs.write(&dir.path().join("a.yaml"), b"").await.unwrap();
    fs.write(&dir.path().join("b.yaml"), b"").await.unwrap();
    fs.write(&dir.path().join("c.txt"), b"").await.unwrap();

    let matches = fs.glob(dir.path(), "*.yaml").await.unwrap();
    assert_eq!(matches.len(), 2);
    assert!(matches.iter().all(|p| p.extension().unwrap() == "yaml"));
}

#[tokio::test]
async fn glob_star_does_not_cross_dir_separator() {
    let dir = td();
    let fs = TokioFs;

    fs.write(&dir.path().join("top.yaml"), b"").await.unwrap();
    fs.write(&dir.path().join("sub").join("deep.yaml"), b"")
        .await
        .unwrap();

    let matches = fs.glob(dir.path(), "*.yaml").await.unwrap();
    assert_eq!(
        matches,
        vec![dir.path().join("top.yaml")],
        "literal_separator(true): `*` must not match across `/`"
    );
}

#[tokio::test]
async fn glob_recurses_subdirs() {
    let dir = td();
    let fs = TokioFs;

    fs.write(&dir.path().join("sub").join("deep.nika.yaml"), b"")
        .await
        .unwrap();
    fs.write(&dir.path().join("top.nika.yaml"), b"")
        .await
        .unwrap();

    let matches = fs.glob(dir.path(), "**/*.nika.yaml").await.unwrap();
    assert_eq!(matches.len(), 2, "`**` must match zero or more components");
}

#[tokio::test]
async fn glob_skips_hidden_dirs() {
    let dir = td();
    let fs = TokioFs;

    fs.write(&dir.path().join(".hidden").join("secret.yaml"), b"")
        .await
        .unwrap();
    fs.write(&dir.path().join("visible.yaml"), b"")
        .await
        .unwrap();

    let matches = fs.glob(dir.path(), "**/*.yaml").await.unwrap();
    assert_eq!(
        matches,
        vec![dir.path().join("visible.yaml")],
        "hidden directories are not traversed (brouillon parity)"
    );
}

#[tokio::test]
async fn glob_results_sorted() {
    let dir = td();
    let fs = TokioFs;

    fs.write(&dir.path().join("z.yaml"), b"").await.unwrap();
    fs.write(&dir.path().join("a.yaml"), b"").await.unwrap();
    fs.write(&dir.path().join("m.yaml"), b"").await.unwrap();

    let matches = fs.glob(dir.path(), "*.yaml").await.unwrap();
    assert_eq!(
        matches,
        vec![
            dir.path().join("a.yaml"),
            dir.path().join("m.yaml"),
            dir.path().join("z.yaml"),
        ]
    );
}

#[tokio::test]
async fn glob_invalid_pattern_errs_invalid_input() {
    let dir = td();
    let fs = TokioFs;
    let err = fs.glob(dir.path(), "a{b").await.unwrap_err();
    assert!(
        matches!(err, FsError::InvalidData { .. }),
        "InvalidInput expected, got {err:?}"
    );
}

#[tokio::test]
async fn glob_missing_root_errs_not_found() {
    let dir = td();
    let fs = TokioFs;
    let err = fs
        .glob(&dir.path().join("ghost"), "*.yaml")
        .await
        .unwrap_err();
    assert!(
        matches!(err, FsError::NotFound { .. }),
        "NotFound expected, got {err:?}"
    );
}

// linux-gated: APFS (macOS) rejects non-UTF-8 names at creation time
// (EILSEQ · verified empirically 2026-06-10) — the scenario only exists
// on byte-string filesystems (ext4 et al.). The byte-level hidden check
// in src is cross-platform regardless.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn glob_skips_hidden_dirs_with_non_utf8_names() {
    use std::os::unix::ffi::OsStrExt;

    let dir = td();
    let fs = TokioFs;

    // `.` prefix + invalid-UTF-8 tail: a to_str()-based check fails
    // OPEN here; the byte-level check must still hide the dir.
    let weird = std::ffi::OsStr::from_bytes(b".\xFF\xFEhidden");
    let hidden_dir = dir.path().join(weird);
    fs.create_dir_all(&hidden_dir).await.unwrap();
    fs.write(&hidden_dir.join("secret.yaml"), b"")
        .await
        .unwrap();
    fs.write(&dir.path().join("visible.yaml"), b"")
        .await
        .unwrap();

    let matches = fs.glob(dir.path(), "**/*.yaml").await.unwrap();
    assert_eq!(
        matches,
        vec![dir.path().join("visible.yaml")],
        "non-UTF-8 dot-dirs must hide too (byte-level check)"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn write_replaces_symlink_destination_without_following() {
    let dir = td();
    let fs = TokioFs;

    let real = dir.path().join("real.txt");
    let link = dir.path().join("link.txt");
    fs.write(&real, b"original").await.unwrap();
    tokio::fs::symlink(&real, &link).await.unwrap();

    fs.write(&link, b"updated").await.unwrap();

    // Rename-replace semantics: the link itself becomes a regular file;
    // the old target keeps its content (documented crate contract).
    let link_meta = tokio::fs::symlink_metadata(&link).await.unwrap();
    assert!(
        link_meta.file_type().is_file(),
        "destination symlink is replaced by a regular file, not followed"
    );
    assert_eq!(fs.read_to_string(&link).await.unwrap(), "updated");
    assert_eq!(fs.read_to_string(&real).await.unwrap(), "original");
}

#[cfg(unix)]
#[tokio::test]
async fn glob_does_not_follow_symlinked_dirs() {
    let dir = td();
    let fs = TokioFs;

    fs.write(&dir.path().join("real").join("inside.yaml"), b"")
        .await
        .unwrap();
    // Symlink loop: link points back at the tree root.
    tokio::fs::symlink(dir.path(), dir.path().join("loop"))
        .await
        .unwrap();

    let matches = fs.glob(dir.path(), "**/*.yaml").await.unwrap();
    assert_eq!(
        matches,
        vec![dir.path().join("real").join("inside.yaml")],
        "symlinked dirs are not traversed — loops terminate"
    );
}

// ─── property tests (Gate 6 · roundtrip + glob subset invariants) ────

mod properties {
    use super::*;
    use proptest::prelude::*;

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime")
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(32))]

        /// Any byte payload survives a write → read roundtrip unchanged.
        #[test]
        fn write_read_roundtrip_arbitrary_bytes(data in proptest::collection::vec(any::<u8>(), 0..4096)) {
            let dir = td();
            let path = dir.path().join("prop.bin");
            let fs = TokioFs;
            let read_back = rt().block_on(async {
                fs.write(&path, &data).await.expect("write");
                fs.read(&path).await.expect("read")
            });
            prop_assert_eq!(&read_back[..], &data[..]);
        }

        /// Glob never invents paths: every match is one of the files we
        /// created, and `*`-suffix globs return exactly the suffix set.
        #[test]
        fn glob_matches_are_exactly_the_created_suffix_set(
            names in proptest::collection::btree_set("[a-z]{1,8}", 1..8),
            yaml_mask in proptest::collection::vec(any::<bool>(), 8),
        ) {
            let dir = td();
            let fs = TokioFs;
            let mut expected: Vec<PathBuf> = Vec::new();
            rt().block_on(async {
                for (i, stem) in names.iter().enumerate() {
                    let is_yaml = yaml_mask[i % yaml_mask.len()];
                    let name = if is_yaml { format!("{stem}.yaml") } else { format!("{stem}.txt") };
                    let p = dir.path().join(&name);
                    fs.write(&p, b"x").await.expect("write");
                    if is_yaml { expected.push(p); }
                }
            });
            expected.sort();
            let matches = rt().block_on(fs.glob(dir.path(), "*.yaml")).expect("glob");
            prop_assert_eq!(matches, expected);
        }
    }
}
