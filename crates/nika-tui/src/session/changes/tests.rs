// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::path::{Path, PathBuf};

use nika_session::change::{ProjectChange, ProjectChangeSet, Witness};
use nika_session::outcome::ProposalId;
use nika_session::work;

use super::*;

/// A project root that removes itself.
struct Room(PathBuf);

impl Room {
    fn new(tag: &str) -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("nika-changes-{tag}-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&root).expect("room");
        Self(root)
    }
}

impl Drop for Room {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A workflow of `n` numbered lines, line `changed` saying `word` instead.
fn workflow(n: usize, changed: usize, word: &str) -> String {
    (1..=n)
        .map(|at| {
            if at == changed {
                format!("  # line {at} {word}")
            } else {
                format!("  # line {at}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The candidate that replaces `path`, its bytes witnessed `before`, with `content`.
fn replacing(root: &Path, path: &str, before: &[u8], content: &str) -> work::Candidate {
    let change = ProjectChange::UpdateWorkflow {
        path: PathBuf::from(path),
        before: Witness::of(before),
        content: content.to_owned(),
    };
    let set = ProjectChangeSet::project_change(root, "revise", change);
    work::Candidate::of(ProposalId::of("p"), &set, false, false)
}

/// One line changed in a long workflow reads as that line, removed then added, among the two
/// lines around it, at its line, with the comparison's size (live gauntlet r06 B8: 109 lines
/// printed whole for one line changed).
#[test]
fn one_changed_line_reads_as_itself_among_its_neighbours() {
    let room = Room::new("one-line");
    let before = workflow(109, 42, "before");
    std::fs::write(room.0.join("clean.nika"), &before).expect("the saved workflow");
    let after = workflow(109, 42, "after");
    let candidate = replacing(&room.0, "clean.nika", before.as_bytes(), &after);
    let shown = against_saved(&room.0, &candidate).expect("a replaced file");
    let short = &Witness::of(before.as_bytes()).0[..8];
    let expected = format!(
        "changes in `clean.nika` against the file as it is now (witnessed {short}) · +1 −1 lines\n  at line 40\n        # line 40\n        # line 41\n    -   # line 42 before\n    +   # line 42 after\n        # line 43\n        # line 44"
    );
    assert_eq!(shown, expected);
}

/// Changes far apart read as hunks of their own; an added and a removed line count once each;
/// the same bytes say so.
#[test]
fn far_changes_read_apart_and_the_same_bytes_say_so() {
    let before = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj";
    let after = "a\nB\nc\nd\ne\nf\ng\nh\ni\nj\nk";
    let shown = compared("w.nika", "12345678", before, after);
    assert!(shown.contains("· +2 −1 lines"), "{shown}");
    assert_eq!(shown.matches("\n  at line ").count(), 2, "{shown}");
    assert!(shown.contains("\n    - b\n    + B"), "{shown}");
    assert!(shown.contains("\n    + k"), "{shown}");
    let same = compared("w.nika", "12345678", before, before);
    assert!(same.ends_with("the same lines, byte for byte"), "{same}");
}

/// No comparison against bytes the proposal does not replace: a file changed since, a file
/// that is gone, or a set that only creates files.
#[test]
fn a_file_changed_since_or_gone_is_never_compared() {
    let room = Room::new("changed");
    std::fs::write(room.0.join("w.nika"), "edited since\n").expect("the edited file");
    let candidate = replacing(&room.0, "w.nika", b"the bytes it was built on\n", "new\n");
    let shown = against_saved(&room.0, &candidate).expect("a replaced file");
    assert!(
        shown.contains("changed since this proposal was made"),
        "{shown}"
    );
    assert!(!shown.contains("    + new"), "{shown}");
    let gone = replacing(&room.0, "gone.nika", b"old\n", "new\n");
    let shown = against_saved(&room.0, &gone).expect("a replaced file");
    assert!(shown.contains("cannot be read now"), "{shown}");
    let create = ProjectChange::CreateWorkflow {
        path: PathBuf::from("fresh.nika"),
        content: "fresh\n".to_owned(),
    };
    let set = ProjectChangeSet::project_change(&room.0, "create", create);
    let created = work::Candidate::of(ProposalId::of("c"), &set, false, false);
    assert_eq!(against_saved(&room.0, &created), None);
}
