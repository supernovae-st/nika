// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The room's read-only composition check (R5): the candidate captured with its child workflows
//! where its host will save it, every captured workflow checked composed, nothing written or run.
//! A room told no place resolves no relative child, a changed child is another closure, and a
//! proposal is checked again where it lands, against the child units its check recorded: a child
//! changed or removed since, even at the same path, does not hold. The pack's own parent and child.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use nika_compile::surface::{finish, initial, sha256};
use nika_onboard::compile::rehearse::{Closure, Composed, Rehearse, composed};
use nika_onboard::compile::room::ObservedRoom;
use nika_onboard::compile::{CompileOutcome, CompileStatus};
use serde_json::json;
use std::path::Path;

const PARENT: &str = include_str!("../../nika-pack/pack/examples/10-compose-pipeline.nika");
const CHILD: &str = include_str!("../../nika-pack/pack/examples/10-compose-child.nika");

/// A project holding `files` (path, bytes).
fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, bytes) in files {
        let at = dir.path().join(path);
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(at, bytes).unwrap();
    }
    dir
}

/// The room over `dir`, every candidate saved at `at`.
fn room_at(dir: &Path, at: &'static str) -> ObservedRoom {
    ObservedRoom::new(dir.canonicalize().unwrap()).located(move |_| Some(at.to_owned()))
}

fn clean(composed: Composed) -> Closure {
    match composed {
        Composed::Clean(closure) => closure,
        other => panic!("not clean: {other:?}"),
    }
}

fn refused(composed: Composed) -> String {
    match composed {
        Composed::Refused { reason } => reason,
        other => panic!("not refused: {other:?}"),
    }
}

/// A child of the parent's contract that itself calls `target`.
fn calling(target: &str) -> String {
    format!(
        "nika: compose-child\nmodel: mock/echo\npermits: {{}}\ninputs:\n  topic: {{ type: string, default: \"t\", required: true }}\ntasks:\n  deeper:\n    invoke:\n      workflow: \"{target}\"\n      args: {{ topic: \"${{{{ inputs.topic }}}}\" }}\n    returns: {{ object: {{ summary: string }} }}\noutputs:\n  summary:\n    value: ${{{{ tasks.deeper.output.summary }}}}\n    type: string\n"
    )
}

#[test]
fn a_clean_child_beside_the_saved_parent_composes_with_its_exact_closure() {
    let dir = project(&[("10-compose-child.nika", CHILD)]);
    let closure = clean(room_at(dir.path(), "compose-pipeline.nika").compose(PARENT));
    assert_eq!(closure.candidate_sha256, sha256(PARENT));
    assert_eq!(closure.logical_root, "compose-pipeline.nika");
    let unit = |path: &str| {
        (closure.units.iter())
            .find(|(unit, _)| unit == path)
            .map(|(_, digest)| digest.clone())
    };
    let child = unit("10-compose-child.nika").expect("the child is captured");
    assert!(child.contains(&sha256(CHILD)), "{child}");
    let root = unit("compose-pipeline.nika").expect("the candidate is captured");
    assert!(root.contains(&sha256(PARENT)), "{root}");
    // Nothing was written: the project holds the child alone.
    let names: Vec<_> = (std::fs::read_dir(dir.path()).unwrap())
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, ["10-compose-child.nika"]);
}

#[test]
fn a_missing_broken_or_cyclic_child_refuses_with_its_reason() {
    let missing = project(&[]);
    let why = refused(room_at(missing.path(), "compose-pipeline.nika").compose(PARENT));
    assert!(why.contains("compose-pipeline.nika"), "{why}");
    assert!(
        why.contains("10-compose-child.nika"),
        "the missing child is named: {why}"
    );
    // The closure is judged, never the root alone: a child calling a missing grandchild.
    let broken = project(&[("10-compose-child.nika", &calling("./gone.nika"))]);
    let why = refused(room_at(broken.path(), "compose-pipeline.nika").compose(PARENT));
    assert!(why.contains("gone.nika"), "{why}");
    // A child calling its parent back closes a cycle.
    let cyclic = project(&[("10-compose-child.nika", &calling("./compose-pipeline.nika"))]);
    let why = refused(room_at(cyclic.path(), "compose-pipeline.nika").compose(PARENT));
    assert!(!why.is_empty(), "{why}");
}

#[test]
fn a_room_told_no_place_resolves_no_child() {
    let dir = project(&[("10-compose-child.nika", CHILD)]);
    let room = ObservedRoom::new(dir.path().canonicalize().unwrap());
    assert!(
        matches!(room.compose(PARENT), Composed::Unresolved { .. }),
        "{:?}",
        room.compose(PARENT)
    );
}

#[test]
fn a_changed_child_is_another_closure() {
    let dir = project(&[("10-compose-child.nika", CHILD)]);
    let room = room_at(dir.path(), "compose-pipeline.nika");
    let before = clean(room.compose(PARENT));
    let changed = CHILD.replace("default: \"the DAG\"", "default: \"the graph\"");
    assert_ne!(changed, CHILD);
    std::fs::write(dir.path().join("10-compose-child.nika"), changed).unwrap();
    let after = clean(room.compose(PARENT));
    assert_eq!(before.candidate_sha256, after.candidate_sha256);
    assert_ne!(before.snapshot_identity, after.snapshot_identity);
    assert_ne!(before.units, after.units);
}

/// The outcome a real check leaves: `finish`'s source-only hold on the child call, lifted by the
/// room's clean closure at `compose-pipeline.nika` and recorded on `decision.composition`.
fn checked(dir: &Path) -> CompileOutcome {
    let mut out = initial();
    finish(PARENT.to_owned(), &mut out);
    composed(&room_at(dir, "compose-pipeline.nika"), &mut out);
    assert_eq!(out.status, CompileStatus::Ready, "{:#?}", out.diagnostics);
    let record = &out.provenance.decision.as_ref().unwrap()["composition"];
    assert_eq!(record["verdict"], "clean", "{record:#}");
    let units = record["units"].to_string();
    assert!(
        units.contains("10-compose-child.nika"),
        "the child's unit is kept: {units}"
    );
    out
}

#[test]
fn a_proposal_elsewhere_is_checked_again_where_it_lands() {
    let dir = project(&[("10-compose-child.nika", CHILD)]);
    let out = checked(dir.path());
    let room = ObservedRoom::new(dir.path().canonicalize().unwrap());
    // Where it was checked, and beside the same child under another name, it holds.
    assert_eq!(room.recompose(&out, "compose-pipeline.nika"), Ok(()));
    assert_eq!(room.recompose(&out, "renamed.nika"), Ok(()));
    // In a directory holding no child, the hold does not lift: the refusal says both places.
    let why = room
        .recompose(&out, "sub/compose-pipeline.nika")
        .unwrap_err();
    assert!(why.contains("`sub/compose-pipeline.nika`"), "{why}");
    assert!(why.contains("\"compose-pipeline.nika\""), "{why}");
    // Nothing lifted: nothing is checked.
    let mut unlifted = out.clone();
    let none = json!({"composition": {"discharged": [], "logical_root": null}});
    unlifted.provenance.decision = Some(none);
    assert_eq!(
        room.recompose(&unlifted, "sub/compose-pipeline.nika"),
        Ok(())
    );
}

#[test]
fn a_child_changed_or_removed_at_the_same_path_after_its_check_does_not_hold() {
    let dir = project(&[("10-compose-child.nika", CHILD)]);
    let out = checked(dir.path());
    let room = ObservedRoom::new(dir.path().canonicalize().unwrap());
    // A valid child rewritten at the same path between the check and the proposal: the parent's
    // bytes are unchanged, its closure is not, so the proposal does not hold there.
    let changed = CHILD.replace("default: \"the DAG\"", "default: \"the graph\"");
    std::fs::write(dir.path().join("10-compose-child.nika"), changed).unwrap();
    assert_eq!(out.candidate.as_deref(), Some(PARENT));
    let why = room.recompose(&out, "compose-pipeline.nika").unwrap_err();
    assert!(why.contains("`10-compose-child.nika`"), "{why}");
    // Removed at the same path: the capture itself refuses there.
    std::fs::remove_file(dir.path().join("10-compose-child.nika")).unwrap();
    let why = room.recompose(&out, "compose-pipeline.nika").unwrap_err();
    assert!(why.contains("10-compose-child.nika"), "{why}");
    // Restored byte for byte, the recorded closure holds again.
    std::fs::write(dir.path().join("10-compose-child.nika"), CHILD).unwrap();
    assert_eq!(room.recompose(&out, "compose-pipeline.nika"), Ok(()));
}
