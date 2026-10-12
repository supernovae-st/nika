// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The quarantine acts only on the very file a run created, inside the
//! project, through held descriptors: a link, a replaced parent, a path
//! leaving the project, a replaced or re-linked file and a person's newer
//! file are each refused by name, with both files left where they stand.
//! These run on macOS and Linux alike (the crate builds on unix only).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::os::unix::fs::{MetadataExt as _, symlink};
use std::path::Path;

use super::*;

const STAMP: &str = "2026-10-13T09-00-00Z-cd34";

/// A project and a directory beside it that the project must never touch.
struct Layout {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    outside: PathBuf,
}

impl Layout {
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().join("project");
        let outside = tmp.path().join("outside");
        std::fs::create_dir_all(&root).expect("project");
        std::fs::create_dir_all(&outside).expect("outside");
        Self {
            _tmp: tmp,
            root,
            outside,
        }
    }

    /// The run creates `path` with `body`; the ledger records which file.
    fn create(&self, path: &str, body: &str) -> WriteOrigin {
        let file = self.root.join(path);
        std::fs::create_dir_all(file.parent().expect("parent")).expect("mkdir");
        std::fs::write(&file, body).expect("write");
        let meta = std::fs::symlink_metadata(&file).expect("meta");
        let identity = nika_kernel::fs::FileIdentity::new(meta.dev(), meta.ino(), meta.nlink());
        WriteOrigin::Created {
            file: Some(CreatedFile::new(identity, meta.len())),
        }
    }

    fn quarantined(&self) -> PathBuf {
        self.root.join(crate::store::QUARANTINE_DIR).join(STAMP)
    }
}

/// Quarantine `paths` with the origins recorded in `ledger`.
fn run(layout: &Layout, ledger: &BTreeMap<String, WriteOrigin>) -> serde_json::Value {
    let paths: Vec<String> = ledger.keys().cloned().collect();
    quarantine(&layout.root, &paths, STAMP, &|path: &str| {
        ledger.get(path).cloned()
    })
}

/// The one fold entry for `path` among the outputs.
fn entry<'f>(fold: &'f serde_json::Value, path: &str) -> &'f serde_json::Value {
    fold["outputs"]
        .as_array()
        .and_then(|entries| entries.iter().find(|e| e["path"] == path))
        .expect("the path has an entry")
}

#[test]
fn a_symlink_swapped_in_place_of_a_created_file_is_never_followed_or_moved() {
    let layout = Layout::new();
    let secret = layout.outside.join("secret.txt");
    std::fs::write(&secret, "not the run's").expect("secret");
    let ledger = BTreeMap::from([("out.txt".to_owned(), layout.create("out.txt", "half"))]);
    std::fs::remove_file(layout.root.join("out.txt")).expect("remove");
    symlink(&secret, layout.root.join("out.txt")).expect("swap in a link");

    let fold = run(&layout, &ledger);

    let out = entry(&fold, "out.txt");
    assert_eq!(out["action"], "left_in_place", "{fold}");
    assert!(
        out["error"]
            .as_str()
            .is_some_and(|e| e.contains("symbolic link")),
        "{fold}"
    );
    let link = std::fs::symlink_metadata(layout.root.join("out.txt")).expect("still there");
    assert!(link.file_type().is_symlink(), "the link stays where it is");
    assert_eq!(
        std::fs::read_to_string(&secret).expect("target"),
        "not the run's"
    );
    assert!(
        !layout.quarantined().join("out.txt").exists(),
        "nothing moved into the quarantine"
    );
}

#[test]
fn a_parent_directory_replaced_by_a_symlink_is_refused() {
    let layout = Layout::new();
    let ledger = BTreeMap::from([(
        "sub/out.txt".to_owned(),
        layout.create("sub/out.txt", "half"),
    )]);
    std::fs::rename(layout.root.join("sub"), layout.root.join("sub-kept")).expect("move aside");
    std::fs::write(layout.outside.join("out.txt"), "outside").expect("bait");
    symlink(&layout.outside, layout.root.join("sub")).expect("swap the parent");

    let fold = run(&layout, &ledger);

    let out = entry(&fold, "sub/out.txt");
    assert_eq!(out["action"], "left_in_place", "{fold}");
    assert!(
        out["error"]
            .as_str()
            .is_some_and(|e| e.contains("symbolic link or not a directory")),
        "{fold}"
    );
    assert_eq!(
        std::fs::read_to_string(layout.outside.join("out.txt")).expect("outside"),
        "outside",
        "the file behind the link never moves"
    );
    assert!(layout.root.join("sub-kept/out.txt").exists());
}

#[test]
fn a_recorded_path_that_leaves_the_root_is_refused_before_any_io() {
    let layout = Layout::new();
    let secret = layout.outside.join("secret.txt");
    std::fs::write(&secret, "outside").expect("secret");
    let meta = std::fs::symlink_metadata(&secret).expect("meta");
    let identity = nika_kernel::fs::FileIdentity::new(meta.dev(), meta.ino(), meta.nlink());
    let claimed = WriteOrigin::Created {
        file: Some(CreatedFile::new(identity, meta.len())),
    };
    let absolute = secret.to_string_lossy().into_owned();
    let ledger = BTreeMap::from([
        ("../outside/secret.txt".to_owned(), claimed.clone()),
        (absolute.clone(), claimed),
    ]);

    let fold = run(&layout, &ledger);

    for path in ["../outside/secret.txt", absolute.as_str()] {
        let out = entry(&fold, path);
        assert_eq!(out["action"], "left_in_place", "{fold}");
        assert_eq!(out["error"], "the path leaves the project", "{fold}");
    }
    assert_eq!(std::fs::read_to_string(&secret).expect("secret"), "outside");
    assert!(
        !layout.root.join(".nika").exists(),
        "refused by name: no directory was even created"
    );
}

#[test]
fn a_file_replaced_after_the_run_is_not_quarantined() {
    let layout = Layout::new();
    let ledger = BTreeMap::from([
        (
            "replaced.txt".to_owned(),
            layout.create("replaced.txt", "run"),
        ),
        ("linked.txt".to_owned(), layout.create("linked.txt", "run")),
        ("grown.txt".to_owned(), layout.create("grown.txt", "run")),
    ]);
    // A person's file takes the path (a new file renamed over it).
    std::fs::write(layout.root.join("mine.tmp"), "the person's").expect("mine");
    std::fs::rename(
        layout.root.join("mine.tmp"),
        layout.root.join("replaced.txt"),
    )
    .expect("replace");
    // The run's file gained a second name.
    std::fs::hard_link(
        layout.root.join("linked.txt"),
        layout.root.join("second.txt"),
    )
    .expect("link");
    // The run's file was changed in place.
    std::fs::write(layout.root.join("grown.txt"), "changed since").expect("grow");

    let fold = run(&layout, &ledger);

    for (path, why) in [
        ("replaced.txt", "another file stands at the path"),
        ("linked.txt", "the file has more than one name"),
        ("grown.txt", "the file changed since the run wrote it"),
    ] {
        let out = entry(&fold, path);
        assert_eq!(out["action"], "left_in_place", "{path}: {fold}");
        assert_eq!(out["error"], why, "{path}: {fold}");
        assert!(layout.root.join(path).exists(), "{path} stays in place");
    }
    assert_eq!(
        std::fs::read_to_string(layout.root.join("replaced.txt")).expect("mine"),
        "the person's"
    );
}

#[test]
fn restore_never_overwrites_a_newer_person_file() {
    let layout = Layout::new();
    let ledger = BTreeMap::from([("out.txt".to_owned(), layout.create("out.txt", "half"))]);
    let fold = run(&layout, &ledger);
    assert!(
        entry(&fold, "out.txt")["quarantined_to"].is_string(),
        "{fold}"
    );
    std::fs::write(layout.root.join("out.txt"), "the person's newer file").expect("newer");

    let journal = Path::new(".nika/traces").join(format!("{STAMP}.ndjson"));
    let restored = restore_at(&layout.root, &journal).expect("a restore was tried");

    assert_eq!(
        restored["restored"][0]["action"], "left_quarantined",
        "{restored}"
    );
    assert!(
        restored["restored"][0]["reason"]
            .as_str()
            .is_some_and(|reason| reason.starts_with("conflict")),
        "{restored}"
    );
    assert_eq!(
        std::fs::read_to_string(layout.root.join("out.txt")).expect("theirs"),
        "the person's newer file"
    );
    assert_eq!(
        std::fs::read_to_string(layout.quarantined().join("out.txt")).expect("ours"),
        "half",
        "both files stay"
    );
}

#[test]
fn a_state_directory_replaced_by_a_symlink_takes_no_file() {
    let layout = Layout::new();
    let ledger = BTreeMap::from([("out.txt".to_owned(), layout.create("out.txt", "half"))]);
    symlink(&layout.outside, layout.root.join(".nika")).expect("swap the state dir");

    let fold = run(&layout, &ledger);

    let out = entry(&fold, "out.txt");
    assert_eq!(out["action"], "left_in_place", "{fold}");
    assert!(
        out["error"]
            .as_str()
            .is_some_and(|e| e.contains("quarantine directory cannot be opened")),
        "{fold}"
    );
    assert!(layout.root.join("out.txt").exists(), "the file stays");
    assert!(
        std::fs::read_dir(&layout.outside)
            .expect("outside")
            .next()
            .is_none(),
        "nothing was created behind the link"
    );
}
