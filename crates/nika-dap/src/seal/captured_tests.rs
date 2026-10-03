// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The verifier's side inputs are read through held descriptors and within
//! their caps: an explicit key file, an anchor sidecar and a writer lease
//! behind a symlink, over their cap or not regular are refused (or read
//! `unknown`), never followed, never taken for an absence; nothing is created.

use std::path::Path;

use super::*;
use crate::liveness::{Liveness, hold, lease_path, probe};

fn room() -> tempfile::TempDir {
    tempfile::tempdir().expect("room")
}

#[test]
fn a_key_file_behind_a_symlink_over_its_cap_or_absent_is_refused() {
    let room = room();
    let (public, _) = {
        let pair = minisign::KeyPair::generate_unencrypted_keypair().expect("keypair");
        (pair.pk.to_box().expect("box").to_string(), pair.sk)
    };
    let real = room.path().join("run.pub");
    std::fs::write(&real, &public).expect("key");
    let found = candidate_pubkeys(Some(&real)).expect("a regular key file is read");
    assert!(
        found
            .iter()
            .any(|(_, source)| source == &real.display().to_string())
    );
    let link = room.path().join("link.pub");
    std::os::unix::fs::symlink(&real, &link).expect("link");
    let big = room.path().join("big.pub");
    std::fs::write(
        &big,
        "x".repeat(usize::try_from(KEY_FILE_CAP).expect("cap") + 1),
    )
    .expect("big");
    let followed = candidate_pubkeys(Some(&link)).expect_err("never followed");
    assert!(followed.contains("cannot read --key"), "{followed}");
    assert!(
        !followed.contains("no such file"),
        "a refusal, not an absence: {followed}"
    );
    for (path, why) in [
        (big, "over its byte cap"),
        (room.path().join("gone.pub"), "no such file"),
    ] {
        let refused = candidate_pubkeys(Some(&path)).expect_err("refused");
        assert!(
            refused.contains("cannot read --key") && refused.contains(why),
            "{refused}"
        );
    }
}

#[test]
fn an_anchor_sidecar_is_read_only_as_a_regular_file_within_its_bound() {
    let room = room();
    let trace = room.path().join("run.ndjson").display().to_string();
    let path = crate::anchor::sidecar_path(&trace);
    let bound = crate::bounded::MAX_ARTIFACT_BYTES;
    let read = |path: &std::path::Path| crate::anchor::read_owned(path, bound as u64);
    assert!(matches!(read(&path), Ok(None)), "absent");
    let elsewhere = room.path().join("elsewhere.json");
    std::fs::write(&elsewhere, "{}").expect("elsewhere");
    std::os::unix::fs::symlink(&elsewhere, &path).expect("link");
    assert!(read(&path).is_err(), "never followed");
    std::fs::remove_file(&path).expect("unlink");
    std::fs::write(&path, "x".repeat(bound + 1)).expect("big");
    assert!(read(&path).is_err(), "over the bound");
    std::fs::write(&path, "{ not a sidecar").expect("edited");
    let edited = crate::anchor::load_sidecar(&path).expect_err("a gap, not an absence");
    assert!(!edited.contains("no such file"), "{edited}");
}

#[test]
fn a_lease_is_judged_on_one_descriptor_and_never_created() {
    let room = room();
    let trace = room.path().join("run.ndjson");
    assert_eq!(probe(&trace), Liveness::Unknown, "no lease");
    assert!(!lease_path(&trace).exists(), "the probe creates nothing");
    let held = hold(&trace).expect("the writer's lease");
    assert!(matches!(probe(&trace), Liveness::Alive { .. }), "held");
    drop(held);
    assert!(matches!(probe(&trace), Liveness::Dead { .. }), "released");
    let lease = lease_path(&trace);
    let record = std::fs::read(&lease).expect("record");
    let moved = room.path().join("moved.lock");
    std::fs::rename(&lease, &moved).expect("move");
    std::os::unix::fs::symlink(&moved, &lease).expect("link");
    assert_eq!(
        probe(&trace),
        Liveness::Unknown,
        "a symlinked lease is never followed"
    );
    std::fs::remove_file(&lease).expect("unlink");
    let mut big = record;
    big.resize(
        usize::try_from(crate::liveness::LEASE_CAP).expect("cap") + 1,
        b' ',
    );
    std::fs::write(&lease, big).expect("big");
    assert_eq!(probe(&trace), Liveness::Unknown, "over its cap");
    assert!(Path::new(&moved).exists());
}

/// A directory reached through a symlinked ancestor, as an operator names a
/// project or a HOME that lives behind a link.
fn linked(room: &tempfile::TempDir) -> std::path::PathBuf {
    std::fs::create_dir_all(room.path().join("real")).expect("real");
    let link = room.path().join("link");
    std::os::unix::fs::symlink(room.path().join("real"), &link).expect("link");
    link
}

/// Custody under a symlinked ancestor: a missing keys directory reads as no
/// key, a key file is read, a final link is still refused.
#[test]
fn custody_under_a_symlinked_ancestor_is_read_and_its_absence_is_no_key() {
    let room = room();
    let link = linked(&room);
    let custody = link.join(".nika/keys/run-signing.pub");
    assert!(
        matches!(crate::anchor::read_owned(&custody, 64), Ok(None)),
        "no custody, no key"
    );
    std::fs::write(link.join("run.pub"), "box").expect("key");
    let key = crate::anchor::read_owned(&link.join("run.pub"), 64).expect("read");
    assert_eq!(key.as_deref(), Some("box"));
    std::os::unix::fs::symlink(link.join("run.pub"), link.join("alias.pub")).expect("alias");
    assert!(
        crate::anchor::read_owned(&link.join("alias.pub"), 64).is_err(),
        "final link refused"
    );
}

/// A missing sidecar beside a journal reached through a symlinked ancestor
/// is not present: never a forged anchor.
#[test]
fn a_missing_sidecar_under_a_symlinked_ancestor_is_not_present() {
    use crate::anchor::tier::{AnchorTier, SealVerdict, anchor_tier};
    let room = room();
    let trace = linked(&room).join("run.ndjson").display().to_string();
    let seal = SealVerdict {
        key_id: "k".to_owned(),
        source: "s".to_owned(),
        pk32: [0; 32],
    };
    assert_eq!(
        anchor_tier(&trace, None, &seal, false),
        AnchorTier::NotPresent
    );
}

/// A lease held beside a journal reached through a symlinked ancestor is alive.
#[test]
fn a_lease_under_a_symlinked_ancestor_is_judged() {
    let room = room();
    let trace = linked(&room).join("run.ndjson");
    let held = hold(&trace).expect("the writer's lease");
    assert!(matches!(probe(&trace), Liveness::Alive { .. }), "held");
    drop(held);
}

/// Acquisition apart from content: a sidecar whose final name is a link is
/// unavailable (nothing claimed about it); an opened one over the bound or
/// not UTF-8 is still a gap, as before.
#[test]
fn a_sidecar_that_cannot_be_acquired_is_unavailable_and_bad_bytes_stay_a_gap() {
    use crate::anchor::tier::{AnchorTier, SealVerdict, anchor_tier};
    let room = room();
    let trace = room.path().join("run.ndjson").display().to_string();
    let sidecar = crate::anchor::sidecar_path(&trace);
    let seal = SealVerdict {
        key_id: "k".to_owned(),
        source: "s".to_owned(),
        pk32: [0; 32],
    };
    let elsewhere = room.path().join("elsewhere.json");
    std::fs::write(&elsewhere, "{}").expect("elsewhere");
    std::os::unix::fs::symlink(&elsewhere, &sidecar).expect("link");
    assert!(matches!(
        anchor_tier(&trace, None, &seal, false),
        AnchorTier::Unavailable(_)
    ));
    std::fs::remove_file(&sidecar).expect("unlink");
    std::fs::write(&sidecar, "x".repeat(crate::bounded::MAX_ARTIFACT_BYTES + 1)).expect("big");
    assert!(matches!(
        anchor_tier(&trace, None, &seal, false),
        AnchorTier::Gap(_)
    ));
    std::fs::write(&sidecar, [0xff_u8, 0xfe]).expect("bytes");
    assert!(matches!(
        anchor_tier(&trace, None, &seal, false),
        AnchorTier::Gap(_)
    ));
}
