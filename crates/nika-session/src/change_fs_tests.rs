// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Unix filesystem-edge tests for [`crate::change`]: a contained witness
//! (no symlink follow), a directory is not a create, and a write does
//! not leave the root. Owner seam: [`ProjectChangeSet::from_reply`].

#![cfg(unix)]

use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use crate::change::{ChangeError, ProjectChange, ProjectChangeSet, Witness};

const WORKFLOW: &str = "nika: daily\nmodel: mock/echo\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10 }\noutputs:\n  said: ${{ tasks.t.output }}\n";

const OUTSIDE: &str = "OUTSIDE-SECRET-BYTES-do-not-hash\n";

fn reply_with(path: &str, body: &str) -> String {
    format!("Here is the workflow.\n\n```yaml path={path}\n{body}```\n")
}

/// A final symlink at the destination is not a contained file: the
/// public `from_reply` seam must not follow it, hash the outside
/// bytes, or preview `replaces` over them. Preexisting: the write
/// path is already `O_NOFOLLOW` (nika-fs); this leak is the witness
/// read (`std::fs::read` follows).
#[test]
fn a_final_symlink_outside_the_root_is_not_witnessed() {
    let root = tempfile::tempdir().expect("root");
    let outside = tempfile::tempdir().expect("outside");
    let target = outside.path().join("secret.nika.yaml");
    std::fs::write(&target, OUTSIDE).expect("outside");
    symlink(&target, root.path().join("link.nika.yaml")).expect("final symlink");
    let leaked = Witness::of(OUTSIDE.as_bytes());
    let err = ProjectChangeSet::from_reply(
        root.path(),
        "g",
        &reply_with("link.nika.yaml", WORKFLOW),
        &[],
        None,
    )
    .expect_err("a symlink is not a contained witness");
    assert!(
        matches!(err, ChangeError::Io(..)),
        "the class is the file system's: {err}"
    );
    let text = err.to_string();
    assert!(
        text.contains("link.nika.yaml") && text.contains("cannot be witnessed"),
        "{text}"
    );
    assert!(
        !text.contains(leaked.short()) && !text.contains(OUTSIDE.trim()),
        "neither the outside hash nor the outside bytes ride the refusal: {text}"
    );
    assert_eq!(
        std::fs::read_to_string(&target).expect("untouched"),
        OUTSIDE
    );
}

/// A parent directory that is a symlink out of the root is the same
/// leak by another component: `std::fs::read("notes/daily.nika.yaml")`
/// follows `notes/`. The declared path has no `..`; `relative_inside_root`
/// accepts it. The witness must still refuse.
#[test]
fn a_parent_symlink_outside_the_root_is_not_witnessed() {
    let root = tempfile::tempdir().expect("root");
    let outside = tempfile::tempdir().expect("outside");
    let notes = outside.path().join("notes");
    std::fs::create_dir(&notes).expect("outside notes");
    let target = notes.join("daily.nika.yaml");
    std::fs::write(&target, OUTSIDE).expect("outside");
    symlink(&notes, root.path().join("notes")).expect("parent symlink");
    let leaked = Witness::of(OUTSIDE.as_bytes());
    let err = ProjectChangeSet::from_reply(
        root.path(),
        "g",
        &reply_with("notes/daily.nika.yaml", WORKFLOW),
        &[],
        None,
    )
    .expect_err("a redirected parent is not a contained witness");
    assert!(
        matches!(err, ChangeError::Io(..)),
        "the class is the file system's: {err}"
    );
    let text = err.to_string();
    assert!(
        text.contains("notes/daily.nika.yaml") && text.contains("cannot be witnessed"),
        "{text}"
    );
    assert!(
        !text.contains(leaked.short()) && !text.contains(OUTSIDE.trim()),
        "neither the outside hash nor the outside bytes ride the refusal: {text}"
    );
    assert_eq!(
        std::fs::read_to_string(&target).expect("untouched"),
        OUTSIDE
    );
}

/// A directory at the destination is not a create (EISDIR / not a
/// regular file). Preexisting since the unreadable-target fix: `.ok()`
/// no longer treats that error as absence. Kept here so a nofollow
/// witness cannot regress it into a create.
#[test]
fn a_directory_at_the_destination_is_not_a_create() {
    let root = tempfile::tempdir().expect("root");
    std::fs::create_dir(root.path().join("dir.nika.yaml")).expect("dir");
    let err = ProjectChangeSet::from_reply(
        root.path(),
        "g",
        &reply_with("dir.nika.yaml", WORKFLOW),
        &[],
        None,
    )
    .expect_err("a directory is not a missing file");
    assert!(
        matches!(err, ChangeError::Io(..)),
        "the class is the file system's: {err}"
    );
    assert!(err.to_string().contains("cannot be witnessed"), "{err}");
    assert!(root.path().join("dir.nika.yaml").is_dir());
}

/// A write through the set does not follow a final symlink: the outside
/// file keeps its bytes. Preexisting (nika-fs `O_NOFOLLOW`); pinned so
/// the witness fix cannot trade a read leak for a write escape.
#[test]
fn apply_does_not_write_through_a_final_symlink() {
    let root = tempfile::tempdir().expect("root");
    let outside = tempfile::tempdir().expect("outside");
    let target = outside.path().join("secret.nika.yaml");
    std::fs::write(&target, OUTSIDE).expect("outside");
    symlink(&target, root.path().join("link.nika.yaml")).expect("final symlink");
    let set = ProjectChangeSet {
        root: root.path().to_path_buf(),
        goal: "g".to_owned(),
        changes: vec![ProjectChange::CreateWorkflow {
            path: PathBuf::from("link.nika.yaml"),
            content: WORKFLOW.to_owned(),
        }],
        run: None,
        repairs: Vec::new(),
        audits: Vec::new(),
    };
    let err = set.apply().expect_err("write refuses the symlink");
    assert!(
        matches!(err, ChangeError::Stale(_) | ChangeError::Io(..)),
        "contained refusal, not a silent write: {err}"
    );
    assert_eq!(
        std::fs::read_to_string(&target).expect("untouched"),
        OUTSIDE
    );
    assert!(root.path().join("link.nika.yaml").is_symlink());
}

/// A write does not create files under a parent that is a symlink out
/// of the root. Preexisting (`create_below` is `O_NOFOLLOW`).
#[test]
fn apply_does_not_write_through_a_parent_symlink() {
    let root = tempfile::tempdir().expect("root");
    let outside = tempfile::tempdir().expect("outside");
    let notes = outside.path().join("notes");
    std::fs::create_dir(&notes).expect("outside notes");
    symlink(&notes, root.path().join("notes")).expect("parent symlink");
    let set = ProjectChangeSet {
        root: root.path().to_path_buf(),
        goal: "g".to_owned(),
        changes: vec![ProjectChange::CreateWorkflow {
            path: PathBuf::from("notes/daily.nika.yaml"),
            content: WORKFLOW.to_owned(),
        }],
        run: None,
        repairs: Vec::new(),
        audits: Vec::new(),
    };
    let err = set
        .apply()
        .expect_err("write refuses the redirected parent");
    assert!(
        matches!(err, ChangeError::Stale(_) | ChangeError::Io(..)),
        "contained refusal, not a silent write: {err}"
    );
    assert!(
        !notes.join("daily.nika.yaml").exists(),
        "nothing was created outside the root"
    );
}

/// A regular file under the root is still witnessed (nofollow must not
/// break the update path). An absent path is still a create.
#[test]
fn a_regular_file_is_witnessed_and_an_absent_path_is_a_create() {
    let root = tempfile::tempdir().expect("root");
    std::fs::write(root.path().join("daily.nika.yaml"), "nika: old\n").expect("seed");
    let update = ProjectChangeSet::from_reply(
        root.path(),
        "g",
        &reply_with("daily.nika.yaml", WORKFLOW),
        &[],
        None,
    )
    .expect("legal")
    .expect("a block");
    assert!(
        matches!(
            &update.changes[0],
            ProjectChange::UpdateWorkflow { before, .. }
                if *before == Witness::of(b"nika: old\n")
        ),
        "{:?}",
        update.changes[0]
    );
    assert!(
        update
            .preview()
            .contains("replaces `daily.nika.yaml` whole")
    );
    let create = ProjectChangeSet::from_reply(
        root.path(),
        "g",
        &reply_with("fresh.nika.yaml", WORKFLOW),
        &[],
        None,
    )
    .expect("legal")
    .expect("a block");
    assert!(matches!(
        &create.changes[0],
        ProjectChange::CreateWorkflow { path, .. } if path == Path::new("fresh.nika.yaml")
    ));
    assert!(create.preview().contains("creates `fresh.nika.yaml`"));
}
