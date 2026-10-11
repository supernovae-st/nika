// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The constructed cases of the r1 contract §11.4, built here from `p01-base` (and `p02-empty`).
//! Their verdicts and steps are shared with the producer, and each case compares both: a refusal
//! with the right code at another step fails. Their bytes are each side's own. The barrier cases
//! swap an entry between two steps of the walk through the door's probe, as a synthetic barrier.
//! The disk form is defined for Unix descriptors only (§3.2).
#![cfg(unix)]

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::super::Snapshot;
use super::super::canonical::{canonical_json, row_digest};
use super::vectors::{Files, Verdict, outcome, positive, write};
use super::{
    MANIFEST_PATH, MAX_BYTES, MAX_ENTRIES, MAX_FILES, MAX_LINE_BYTES, MAX_LINE_VALUES,
    MAX_MANIFEST_BYTES, MAX_MANIFEST_VALUES, Stage, TrustedIdentity, admit, admit_memory,
};
use nika_event::source_id::sha256_hex;

const FAMILIES: &str = "knowledge/families.jsonl";
const BLOCKS: &str = "knowledge/blocks.jsonl";
const BLOCK_FILE: &str = "blocks/digest.nika";
const NOTICE: &str = "NOTICE.md";

fn disk(files: &Files) -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("a scratch root");
    write(root.path(), files);
    root
}

fn on_disk(root: &Path, identity: &TrustedIdentity) -> Verdict {
    outcome(admit(root, Some(identity), &mut |_, _| {}))
}

fn in_memory(files: Files, identity: &TrustedIdentity) -> Verdict {
    outcome(admit_memory(files, Some(identity)))
}

/// `path` with a trailing slash, its bytes kept as they are.
fn slashed(path: &Path) -> PathBuf {
    let mut text = path.as_os_str().to_owned();
    text.push("/");
    PathBuf::from(text)
}

/// A FIFO at `path`, made by the system's `mkfifo`.
#[expect(
    clippy::disallowed_types,
    reason = "Create a FIFO fixture to verify non-regular inputs are refused"
)]
fn fifo(path: &Path) {
    let made = std::process::Command::new("mkfifo")
        .arg(path)
        .status()
        .expect("mkfifo runs");
    assert!(made.success(), "mkfifo {}", path.display());
}

/// How many values §9.1 counts in `value`: each object, array, string, member name, number and
/// literal, itself included.
fn values(value: &Value) -> usize {
    match value {
        Value::Array(items) => 1 + items.iter().map(values).sum::<usize>(),
        Value::Object(fields) => 1 + fields.values().map(|item| 1 + values(item)).sum::<usize>(),
        _ => 1,
    }
}

/// The one row of `file`, edited and signed again.
fn edit_row(files: &mut Files, file: &str, edit: impl Fn(&mut Value)) {
    let text = String::from_utf8(files[file].clone()).expect("UTF-8");
    let mut row: Value = serde_json::from_str(text.trim_end()).expect("one row");
    edit(&mut row);
    row.as_object_mut().expect("a row").remove("sha256");
    row["sha256"] = json!(row_digest(&row).expect("a digest"));
    let line = canonical_json(&row).expect("canonical");
    files.insert(file.to_owned(), format!("{line}\n").into_bytes());
}

/// `files` with the manifest pinning them again, and the identity of the manifest's bytes.
fn reseal(files: Files, identity: &TrustedIdentity) -> (Files, TrustedIdentity) {
    let mut manifest: Value = serde_json::from_slice(&files[MANIFEST_PATH]).expect("a manifest");
    manifest["files"] = files
        .iter()
        .filter(|(path, _)| path.as_str() != MANIFEST_PATH)
        .map(|(path, bytes)| (path.clone(), json!(sha256_hex(bytes))))
        .collect::<serde_json::Map<String, Value>>()
        .into();
    let bytes = serde_json::to_vec_pretty(&manifest).expect("JSON");
    with_manifest(files, identity, bytes)
}

/// `files` with these manifest bytes, and their identity.
fn with_manifest(
    mut files: Files,
    identity: &TrustedIdentity,
    bytes: Vec<u8>,
) -> (Files, TrustedIdentity) {
    let identity = TrustedIdentity::new(
        &sha256_hex(&bytes),
        identity.policy_id(),
        identity.policy_sha256(),
    )
    .expect("an identity");
    files.insert(MANIFEST_PATH.to_owned(), bytes);
    (files, identity)
}

#[test]
fn no_identity_touches_no_root_and_parses_no_manifest() {
    let scratch = tempfile::tempdir().expect("a scratch root");
    // An absent root would be ROOT_INVALID: this refusal comes before it is ever inspected.
    let absent = scratch.path().join("absent");
    let mut steps = 0_usize;
    let refused = outcome(admit(&absent, None, &mut |_, _| steps += 1));
    assert_eq!(refused, Err(("ADMISSION_UNTRUSTED", "A1")));
    assert_eq!(steps, 0);
    let unparsed = Files::from([(MANIFEST_PATH.to_owned(), b"{".to_vec())]);
    assert_eq!(
        outcome(admit_memory(unparsed, None)),
        Err(("ADMISSION_UNTRUSTED", "A1"))
    );
}

#[test]
fn the_root_is_absolute_present_a_directory_and_never_a_link_its_ancestors_are_not_judged() {
    let (files, identity, snapshot) = positive("p01-base");
    let scratch = tempfile::tempdir().expect("a scratch root");
    let root = scratch.path().join("real/payload");
    write(&root, &files);
    assert_eq!(
        on_disk(Path::new("relative/payload"), &identity),
        Err(("PATH_NOT_ABSOLUTE", "B1"))
    );
    let file = scratch.path().join("file");
    std::fs::write(&file, b"x").expect("a file");
    for absent_or_file in [scratch.path().join("absent"), file.clone()] {
        assert_eq!(
            on_disk(&absent_or_file, &identity),
            Err(("ROOT_INVALID", "B2"))
        );
    }
    for (name, target) in [
        ("to-payload", root.clone()),
        ("dangling", scratch.path().join("nothing")),
        ("to-file", file),
    ] {
        let link = scratch.path().join(name);
        std::os::unix::fs::symlink(&target, &link).expect("a link");
        assert_eq!(
            on_disk(&link, &identity),
            Err(("PAYLOAD_SYMLINK", "B2")),
            "{name}"
        );
    }
    let via = scratch.path().join("via");
    std::os::unix::fs::symlink(scratch.path().join("real"), &via).expect("a link");
    assert_eq!(on_disk(&via.join("payload"), &identity), Ok(snapshot));
}

#[test]
fn the_root_is_judged_as_text_before_its_final_component_is_inspected() {
    let (files, identity, snapshot) = positive("p01-base");
    let scratch = tempfile::tempdir().expect("a scratch root");
    let root = scratch.path().join("payload");
    write(&root, &files);
    assert_eq!(on_disk(&slashed(&root), &identity), Ok(snapshot));
    for (name, target) in [
        ("to-payload", root.clone()),
        ("dangling", scratch.path().join("nothing")),
    ] {
        let link = scratch.path().join(name);
        std::os::unix::fs::symlink(&target, &link).expect("a link");
        assert_eq!(
            on_disk(&slashed(&link), &identity),
            Err(("PAYLOAD_SYMLINK", "B2")),
            "{name}/"
        );
    }
    for named in [
        root.join("."),
        root.join("knowledge").join(".."),
        PathBuf::from("/"),
    ] {
        assert_eq!(
            on_disk(&named, &identity),
            Err(("ROOT_INVALID", "B2")),
            "{}",
            named.display()
        );
    }
}

#[test]
fn a_member_link_a_fifo_and_a_directory_outside_the_layout_are_refused() {
    let (files, identity, _) = positive("p01-base");
    let root = disk(&files);
    let elsewhere = tempfile::tempdir().expect("a scratch root");
    std::fs::write(elsewhere.path().join("notice"), &files[NOTICE]).expect("a copy");
    std::fs::remove_file(root.path().join(NOTICE)).expect("removed");
    std::os::unix::fs::symlink(elsewhere.path().join("notice"), root.path().join(NOTICE))
        .expect("a link");
    assert_eq!(
        on_disk(root.path(), &identity),
        Err(("PAYLOAD_SYMLINK", "B4"))
    );
    let root = disk(&files);
    std::fs::rename(root.path().join("blocks"), elsewhere.path().join("blocks")).expect("moved");
    std::os::unix::fs::symlink(elsewhere.path().join("blocks"), root.path().join("blocks"))
        .expect("a link");
    assert_eq!(
        on_disk(root.path(), &identity),
        Err(("PAYLOAD_SYMLINK", "B4"))
    );
    let root = disk(&files);
    fifo(&root.path().join("knowledge/queue"));
    assert_eq!(
        on_disk(root.path(), &identity),
        Err(("PAYLOAD_NOT_REGULAR", "B4"))
    );
    for dir in ["skills", "knowledge/sub"] {
        let root = disk(&files);
        std::fs::create_dir(root.path().join(dir)).expect("a directory");
        assert_eq!(
            on_disk(root.path(), &identity),
            Err(("PROFILE_UNEXPECTED_FILE", "B4")),
            "{dir}"
        );
    }
    let (empty, empty_identity, empty_snapshot) = positive("p02-empty");
    let root = disk(&empty);
    for dir in ["blocks", "LICENSES"] {
        std::fs::create_dir(root.path().join(dir)).expect("a directory");
    }
    assert_eq!(on_disk(root.path(), &empty_identity), Ok(empty_snapshot));
}

#[test]
fn a_directory_listing_is_bounded_before_any_entry_is_handled() {
    let (files, identity, _) = positive("p01-base");
    let entries = files
        .keys()
        .map(|path| path.split('/').next().unwrap_or(path))
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    for (extra, want) in [
        (MAX_ENTRIES - entries, ("PROFILE_UNEXPECTED_FILE", "B4")),
        (MAX_ENTRIES - entries + 1, ("PAYLOAD_TOO_LARGE", "B3")),
    ] {
        let root = disk(&files);
        for at in 0..extra {
            std::fs::create_dir(root.path().join(format!("d{at:05}"))).expect("a directory");
        }
        assert_eq!(on_disk(root.path(), &identity), Err(want), "{extra} extra");
    }
}

#[test]
fn the_file_and_byte_bounds_count_the_manifest_in_memory_and_on_disk() {
    let (files, identity, _) = positive("p01-base");
    for (fill, want) in [
        (MAX_FILES - files.len(), ("INVENTORY_EXTRA", "E2")),
        (MAX_FILES - files.len() + 1, ("PAYLOAD_TOO_LARGE", "B5")),
    ] {
        let mut many = files.clone();
        many.extend((0..fill).map(|at| (format!("knowledge/fill-{at:05}.txt"), b"x".to_vec())));
        assert_eq!(
            in_memory(many.clone(), &identity),
            Err(want),
            "{fill} files"
        );
        assert_eq!(
            on_disk(disk(&many).path(), &identity),
            Err(want),
            "{fill} files"
        );
    }
    let used = files.values().map(Vec::len).sum::<usize>();
    let room = usize::try_from(MAX_BYTES).expect("a size") - used;
    for (size, want) in [
        (room, ("INVENTORY_EXTRA", "E2")),
        (room + 1, ("PAYLOAD_TOO_LARGE", "B5")),
    ] {
        let mut big = files.clone();
        big.insert("knowledge/fill.bin".to_owned(), vec![0; size]);
        assert_eq!(in_memory(big.clone(), &identity), Err(want), "{size} bytes");
        assert_eq!(
            on_disk(disk(&big).path(), &identity),
            Err(want),
            "{size} bytes"
        );
    }
}

#[test]
fn the_manifest_is_bounded_in_bytes_and_values() {
    let (files, identity, _) = positive("p01-base");
    let limit = usize::try_from(MAX_MANIFEST_BYTES).expect("a size");
    for (size, admits) in [(limit, true), (limit + 1, false)] {
        let mut bytes = files[MANIFEST_PATH].clone();
        bytes.resize(size, b' ');
        let (padded, padded_identity) = with_manifest(files.clone(), &identity, bytes);
        let want = if admits {
            Ok(padded_identity.snapshot_sha256().to_owned())
        } else {
            Err(("PAYLOAD_TOO_LARGE", "B5"))
        };
        assert_eq!(in_memory(padded.clone(), &padded_identity), want, "{size}");
        assert_eq!(
            on_disk(disk(&padded).path(), &padded_identity),
            want,
            "{size}"
        );
    }
    let manifest: Value = serde_json::from_slice(&files[MANIFEST_PATH]).expect("a manifest");
    let mut probe = manifest.clone();
    probe["pad"] = json!([]);
    let room = MAX_MANIFEST_VALUES - values(&probe);
    for (extra, want) in [
        (room, ("MANIFEST_SHAPE", "D5")),
        (room + 1, ("MANIFEST_NOT_STRICT", "D1")),
    ] {
        let mut padded = manifest.clone();
        padded["pad"] = json!(vec![""; extra]);
        assert_eq!(values(&padded), MAX_MANIFEST_VALUES + extra - room);
        let bytes = serde_json::to_vec_pretty(&padded).expect("JSON");
        let (files, identity) = with_manifest(files.clone(), &identity, bytes);
        assert_eq!(in_memory(files, &identity), Err(want), "{extra} values");
    }
}

#[test]
fn a_line_is_bounded_in_values_and_bytes() {
    let (files, identity, _) = positive("p01-base");
    let mut probe = files.clone();
    edit_row(&mut probe, FAMILIES, |row| row["pad"] = json!([]));
    let row: Value = serde_json::from_slice(&probe[FAMILIES]).expect("one row");
    let room = MAX_LINE_VALUES - values(&row);
    for (extra, want) in [
        (room, ("ROW_UNKNOWN_FIELD", "F7")),
        (room + 1, ("ROW_MALFORMED", "F1")),
    ] {
        let mut padded = files.clone();
        edit_row(&mut padded, FAMILIES, |row| {
            row["pad"] = json!(vec![""; extra]);
        });
        let (padded, identity) = reseal(padded, &identity);
        assert_eq!(in_memory(padded, &identity), Err(want), "{extra} values");
    }
    let mut probe = files.clone();
    edit_row(&mut probe, FAMILIES, |row| row["pad"] = json!(""));
    let room = MAX_LINE_BYTES - (probe[FAMILIES].len() - 1);
    for (extra, want) in [
        (room, ("ROW_UNKNOWN_FIELD", "F7")),
        (room + 1, ("ROW_MALFORMED", "F1")),
    ] {
        let mut padded = files.clone();
        edit_row(&mut padded, FAMILIES, |row| {
            row["pad"] = json!("x".repeat(extra));
        });
        let (padded, identity) = reseal(padded, &identity);
        assert_eq!(in_memory(padded, &identity), Err(want), "{extra} bytes");
    }
}

#[test]
fn the_notices_and_a_block_file_are_bounded_at_65536_bytes() {
    let (files, identity, _) = positive("p01-base");
    for (size, admits) in [(65_536_usize, true), (65_537, false)] {
        let mut notice = format!("{}\n", "n".repeat(63))
            .repeat(size / 64)
            .into_bytes();
        notice.resize(size, b'n');
        let mut edited = files.clone();
        edited.insert(NOTICE.to_owned(), notice);
        let (edited, identity) = reseal(edited, &identity);
        let want = if admits {
            Ok(identity.snapshot_sha256().to_owned())
        } else {
            Err(("NOTICE_INVALID", "H5"))
        };
        assert_eq!(in_memory(edited, &identity), want, "notice {size}");
        let mut block = files[BLOCK_FILE].clone();
        block.resize(size, b'#');
        let digest = sha256_hex(&block);
        let mut edited = files.clone();
        edited.insert(BLOCK_FILE.to_owned(), block);
        edit_row(&mut edited, BLOCKS, |row| {
            row["file_sha256"] = json!(digest);
            row["check_receipt"]["sha256"] = json!(digest);
        });
        let (edited, identity) = reseal(edited, &identity);
        let want = if admits {
            Ok(identity.snapshot_sha256().to_owned())
        } else {
            Err(("ROW_FILE", "F11"))
        };
        assert_eq!(in_memory(edited, &identity), want, "block {size}");
    }
}

/// Admit `files` on disk with a barrier that runs `swap(root, outside)` at `stage` for `path`.
fn barrier(
    files: &Files,
    identity: &TrustedIdentity,
    stage: Stage,
    path: &str,
    swap: impl Fn(&Path, &Path),
) -> Verdict {
    let root = disk(files);
    let outside = tempfile::tempdir().expect("a scratch root");
    let (root_path, outside_path) = (root.path().to_path_buf(), outside.path().to_path_buf());
    outcome(admit(root.path(), Some(identity), &mut |now, at| {
        if now == stage && at == path {
            swap(&root_path, &outside_path);
        }
    }))
}

#[test]
fn a_swap_between_two_steps_of_the_walk_is_caught_and_a_held_directory_stays_held() {
    let (files, identity, snapshot) = positive("p01-base");
    let swapped = barrier(
        &files,
        &identity,
        Stage::DirInspected,
        "blocks",
        |root, out| {
            std::fs::rename(root.join("blocks"), out.join("blocks")).expect("moved");
            std::os::unix::fs::symlink(out.join("blocks"), root.join("blocks")).expect("a link");
        },
    );
    assert_eq!(swapped, Err(("PAYLOAD_IO", "B6")));
    let renamed = barrier(&files, &identity, Stage::DirHeld, "blocks", |root, out| {
        std::fs::rename(root.join("blocks"), out.join("moved")).expect("moved");
    });
    assert_eq!(
        renamed,
        Ok(snapshot),
        "the read stays on the held descriptor"
    );
    let notice = files[NOTICE].clone();
    let linked = barrier(
        &files,
        &identity,
        Stage::FileInspected,
        NOTICE,
        |root, out| {
            std::fs::write(out.join("copy"), &notice).expect("a copy");
            std::fs::remove_file(root.join(NOTICE)).expect("removed");
            std::os::unix::fs::symlink(out.join("copy"), root.join(NOTICE)).expect("a link");
        },
    );
    assert_eq!(linked, Err(("PAYLOAD_IO", "B6")));
    let piped = barrier(
        &files,
        &identity,
        Stage::FileInspected,
        NOTICE,
        |root, _| {
            std::fs::remove_file(root.join(NOTICE)).expect("removed");
            fifo(&root.join(NOTICE));
        },
    );
    assert_eq!(
        piped,
        Err(("PAYLOAD_IO", "B6")),
        "and the open did not block"
    );
    for stage in [Stage::FileInspected, Stage::FileRead] {
        let grown = barrier(&files, &identity, stage, NOTICE, |root, _| {
            let mut bytes = std::fs::read(root.join(NOTICE)).expect("read");
            bytes.push(b'+');
            std::fs::write(root.join(NOTICE), bytes).expect("grown");
        });
        assert_eq!(grown, Err(("PAYLOAD_IO", "B6")), "{stage:?}");
    }
}

#[test]
fn an_admitted_snapshot_presents_its_bytes_after_its_root_is_gone() {
    let (files, identity, _) = positive("p01-base");
    let root = disk(&files);
    let snapshot = Snapshot::open(root.path(), Some(&identity)).expect("admitted");
    std::fs::write(root.path().join(BLOCK_FILE), b"# changed\n").expect("changed");
    drop(root);
    let pack = snapshot
        .pack("summarize one document into a short digest", None)
        .expect("composed from the admitted bytes");
    let block = pack
        .references
        .iter()
        .find(|reference| reference.kind == "block")
        .expect("the block is presented");
    assert!(
        block.text.contains("FIXTURE-BLOCK-MARKER"),
        "{}",
        block.text
    );
}

/// Removing only REALIZES leaves recalled patterns but no reachable blocks. The mutation receives
/// its own test identity; the issued identity refuses those changed bytes.
#[test]
fn removing_realizes_edges_keeps_patterns_and_removes_blocks_from_the_pack() {
    use super::super::bundled;
    let issued = super::TrustedIdentity::new(
        "b787fc53d6858db43d55958daaf02539fadcad4feeacc17b63c5aefcb92cc32b",
        "policy-r",
        "d0471eeb904416dd411fde12244918771a5578ae1526f1e0a775b5d421087f36",
    )
    .expect("issued R3");
    let original = bundled::admit(Some(&issued)).expect("admitted");
    let mut files: Files = bundled::FILES
        .into_iter()
        .map(|(path, bytes)| (path.to_owned(), bytes.to_vec()))
        .collect();
    files.insert("knowledge/relations.jsonl".to_owned(), Vec::new());
    let mut manifest: Value = serde_json::from_slice(&files[MANIFEST_PATH]).expect("manifest");
    manifest["relations"]["count"] = json!(0);
    manifest["files"]["knowledge/relations.jsonl"] = json!(sha256_hex(b""));
    let bytes = serde_json::to_vec_pretty(&manifest).expect("manifest bytes");
    let (files, mutant_identity) = with_manifest(files, &issued, bytes);
    assert_ne!(issued, mutant_identity);
    assert_eq!(
        in_memory(files.clone(), &issued),
        Err(("IDENTITY_MISMATCH", "C2"))
    );
    let mutant =
        Snapshot::from_files("edge-ablation", files, Some(&mutant_identity)).expect("admitted");
    for (intent, pattern) in [
        (
            "Grant zero authority to a pure compute workflow",
            "pattern:declared-zero",
        ),
        (
            "Declare a typed output with a description",
            "pattern:typed-output",
        ),
        (
            "Skip the fallback step when a boolean flag is false",
            "pattern:guard-on-value",
        ),
    ] {
        let before = original.pack(intent, None).expect("composed");
        let after = mutant.pack(intent, None).expect("composed");
        assert_eq!(before.references.len(), 2);
        assert_eq!(after.references.len(), 1);
        assert_eq!(after.references[0].kind, "pattern");
        assert_eq!(after.references[0].id, pattern);
        assert_eq!(after.references[0], before.references[0]);
        assert_eq!(after.selection["receipt"]["blocks"]["candidates"], 0);
        assert_eq!(after.selection["files"]["verified"], 0);
        assert_ne!(
            before.identity["door"]["pack_sha256"],
            after.identity["door"]["pack_sha256"]
        );
    }
}
