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

/// One custody a child process weighs: the pair it names (secret, public), the
/// `--key` it passes, and the weighed list it must find, one `<key id> <source>`
/// line per candidate (`refused: <prefix>` for a refusal).
struct Weighing<'a> {
    secret: Option<&'a Path>,
    public: Option<&'a Path>,
    key: Option<&'a Path>,
    expected: String,
}

const WEIGHING_EXPECT: &str = "NIKA_TEST_EXPLICIT_PAIR_EXPECT";
const WEIGHING_KEY: &str = "NIKA_TEST_EXPLICIT_PAIR_KEY";

/// The child's half: weigh this process's own custody against `expected`.
fn weigh_here(expected: &str) {
    let key = key_file_env(WEIGHING_KEY)
        .ok()
        .map(std::path::PathBuf::from);
    let weighed = match candidate_pubkeys(key.as_deref()) {
        Ok(found) => (found.iter())
            .map(|(pk_box, source)| format!("{} {source}", fingerprint(pk_box)))
            .collect::<Vec<_>>()
            .join("\n"),
        Err(why) => format!("refused: {why}"),
    };
    if expected.starts_with("refused: ") {
        assert!(weighed.starts_with(expected), "{weighed}");
    } else {
        assert_eq!(weighed, expected);
    }
}

/// The parent's half: weigh `case` in a child process whose home is `home`, its
/// environment naming only what `case` names.
fn weigh_in_a_child(home: &Path, case: &Weighing<'_>) {
    #[expect(
        clippy::disallowed_types,
        reason = "test-only self subprocess isolates custody configuration from parallel tests; no workflow effect is dispatched"
    )]
    let mut child = std::process::Command::new(std::env::current_exe().expect("test executable"));
    child
        .args([
            "--exact",
            "seal::captured_tests::the_explicit_pair_is_weighed_after_key_and_before_the_custody_files",
        ])
        .env(WEIGHING_EXPECT, &case.expected)
        .env("HOME", home)
        .env("NIKA_KEYCHAIN", "off")
        .env_remove("USERPROFILE")
        .env_remove("NIKA_RUN_KEY_FILE")
        .env_remove("NIKA_RUN_PUB_FILE")
        .env_remove(WEIGHING_KEY);
    for (name, path) in [
        ("NIKA_RUN_KEY_FILE", case.secret),
        ("NIKA_RUN_PUB_FILE", case.public),
        (WEIGHING_KEY, case.key),
    ] {
        if let Some(path) = path {
            child.env(name, path);
        }
    }
    let output = child.output().expect("isolated custody probe");
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.status.success() && said.contains("1 passed"),
        "expected:\n{}\n{said}",
        case.expected
    );
}

/// NIK-271 · the verifier weighs the explicit pair the signer seals with: its
/// public half, read alone (the secret slot is never parsed), after `--key` and
/// before the custody files, the pair resolved by the signer's own reader. A pair
/// the signer refuses (one name alone) adds nothing, an absent public file is no
/// key, a final link is refused like every key file; without a pair the custody
/// files are weighed exactly as before. Custody is process configuration, so
/// each case runs in a child process with its own environment.
#[test]
fn the_explicit_pair_is_weighed_after_key_and_before_the_custody_files() {
    if let Ok(expected) = key_file_env(WEIGHING_EXPECT) {
        return weigh_here(&expected);
    }
    let room = room();
    let home = room.path().join("home");
    let custody = home.join(".nika/keys");
    let elsewhere = room.path().join("etc-nika");
    std::fs::create_dir_all(&custody).expect("custody");
    std::fs::create_dir_all(&elsewhere).expect("a directory outside ~/.nika/keys");
    let public_box = || {
        let pair = minisign::KeyPair::generate_unencrypted_keypair().expect("keypair");
        pair.pk.to_box().expect("box").to_string()
    };
    let (paired, current, retired, named) =
        (public_box(), public_box(), public_box(), public_box());
    std::fs::write(custody.join("run-signing.pub"), &current).expect("current");
    std::fs::write(custody.join("retired.pub"), &retired).expect("retired");
    let secret = elsewhere.join("run.key");
    let public = elsewhere.join("run.pub");
    std::fs::write(&secret, "SYNTHETIC-NOT-A-SECRET-KEY").expect("secret slot");
    std::fs::write(&public, &paired).expect("public half");
    let named_file = room.path().join("named.pub");
    std::fs::write(&named_file, &named).expect("--key");
    let symlinked = elsewhere.join("link.pub");
    std::os::unix::fs::symlink(&public, &symlinked).expect("link");
    let weighed = |pk_box: &str, source: &str| format!("{} {source}", fingerprint(pk_box.trim()));
    let custody_only = format!(
        "{}\n{}",
        weighed(&current, "~/.nika/keys/run-signing.pub"),
        weighed(&retired, "~/.nika/keys/retired.pub"),
    );
    let pair = weighed(&paired, "NIKA_RUN_PUB_FILE");
    let named = weighed(&named, &named_file.display().to_string());
    let absent = elsewhere.join("absent.pub");
    let case = |secret, public, key, expected: String| Weighing {
        secret,
        public,
        key,
        expected,
    };
    let cases = [
        // No pair named: the custody files exactly as before.
        case(None, None, None, custody_only.clone()),
        // One name alone is a pair the signer refuses, so it seals nothing.
        case(None, Some(&public), None, custody_only.clone()),
        // A pair whose public file is absent: no key, never a refusal.
        case(Some(&secret), Some(&absent), None, custody_only.clone()),
        // The signer's order: the pair, then the custody files, then the ledger.
        case(
            Some(&secret),
            Some(&public),
            None,
            format!("{pair}\n{custody_only}"),
        ),
        // `--key` is weighed first, the pair still before the custody files.
        case(
            Some(&secret),
            Some(&public),
            Some(&named_file),
            format!("{named}\n{pair}\n{custody_only}"),
        ),
        // A final link is refused, never followed, never taken for an absence.
        case(
            Some(&secret),
            Some(&symlinked),
            None,
            "refused: cannot read NIKA_RUN_PUB_FILE".to_owned(),
        ),
    ];
    for case in &cases {
        weigh_in_a_child(&home, case);
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
