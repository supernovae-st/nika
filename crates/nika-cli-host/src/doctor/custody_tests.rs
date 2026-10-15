// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The run-key row: one line says where the key this process would
//! sign with comes from, its fingerprint, and why the keychain is or is not
//! part of it; when a signed release and the other builds sign with two keys,
//! or may, it says so and names `nika key align`. A dev build compares only
//! through a release's record of the unchanged keychain item. A key file
//! others can read fails, as ssh refuses one. The row is decided from custody
//! facts alone: nothing here opens the keychain or asks for a password.

use std::path::PathBuf;

use nika_dap::seal::{Custody, KeyFile, KeychainUse};

use super::*;

/// The release's keychain key and the 0600 files' key, as fingerprints.
const KEYCHAIN: &str = "0f1e2d3c4b5a6978";
const FILES: &str = "a1b2c3d4e5f60718";

fn key() -> PathBuf {
    PathBuf::from("/home/u/.nika/keys/run-signing.key")
}

#[test]
fn an_ad_hoc_build_names_why_the_keychain_is_not_opened() {
    let none = run_key_finding(&Custody::new(
        None,
        KeychainUse::AdHoc,
        Some((key(), KeyFile::Absent)),
    ));
    assert_eq!((none.label.as_str(), none.level), ("run-key", Level::Warn));
    assert!(
        none.detail.contains("runs stay unsigned") && none.detail.contains("ad-hoc build"),
        "{}",
        none.detail
    );
    assert!(
        none.fix
            .as_deref()
            .is_some_and(|fix| fix.contains("NIKA_RUN_KEY_FILE")),
        "never `nika key init`, which would mint a second identity: {:?}",
        none.fix
    );
    let filed = run_key_finding(
        &Custody::new(None, KeychainUse::AdHoc, Some((key(), KeyFile::Private))).with_keys(
            Some(FILES.to_owned()),
            Some(false),
            None,
            Some(FILES.to_owned()),
        ),
    );
    assert_eq!(filed.level, Level::Ok);
    assert!(
        filed.detail.starts_with(&format!(
            "/home/u/.nika/keys/run-signing.key (0600) · signs with {FILES} · "
        )),
        "{}",
        filed.detail
    );
}

#[test]
fn a_signed_release_keeps_the_keychain_first_and_names_its_key() {
    let opened = run_key_finding(
        &Custody::new(None, KeychainUse::Opened, Some((key(), KeyFile::Absent))).with_keys(
            Some(KEYCHAIN.to_owned()),
            Some(true),
            Some(KEYCHAIN.to_owned()),
            None,
        ),
    );
    assert_eq!(opened.level, Level::Ok);
    assert!(
        opened.detail.contains("keychain first")
            && opened.detail.contains("keeps its grant")
            && opened.detail.contains(&format!(
                "signs with {KEYCHAIN} · dev builds and nika serve hold no key"
            )),
        "{}",
        opened.detail
    );
    let empty = run_key_finding(&Custody::new(
        None,
        KeychainUse::Opened,
        Some((key(), KeyFile::Absent)),
    ));
    assert_eq!(empty.level, Level::Warn);
    assert_eq!(empty.fix.as_deref(), Some("nika key init"));
}

/// The state Thibaut's machine is in: the keychain item a signed release
/// signs with, and an older 0600 pair dev builds and `nika serve` sign with.
/// The release names both keys and the one gesture that makes them one.
#[test]
fn a_release_and_the_other_builds_signing_with_two_keys_is_said_with_its_gesture() {
    let two = run_key_finding(
        &Custody::new(None, KeychainUse::Opened, Some((key(), KeyFile::Private))).with_keys(
            Some(KEYCHAIN.to_owned()),
            Some(true),
            Some(KEYCHAIN.to_owned()),
            Some(FILES.to_owned()),
        ),
    );
    assert_eq!(two.level, Level::Warn);
    assert!(
        two.detail.ends_with(&format!(
            "signs with {KEYCHAIN} · dev builds and nika serve sign with {FILES} (~/.nika/keys): two identities"
        )),
        "{}",
        two.detail
    );
    assert_eq!(
        two.fix,
        Some(format!(
            "nika key align (~/.nika/keys takes {KEYCHAIN}; {FILES} is retired, the journals it sealed stay verifiable)"
        ))
    );
    let one = run_key_finding(
        &Custody::new(None, KeychainUse::Opened, Some((key(), KeyFile::Private))).with_keys(
            Some(KEYCHAIN.to_owned()),
            Some(true),
            Some(KEYCHAIN.to_owned()),
            Some(KEYCHAIN.to_owned()),
        ),
    );
    assert_eq!((one.level, one.fix.as_deref()), (Level::Ok, None));
    assert!(
        one.detail.ends_with(
            "~/.nika/keys holds the same key: one identity for releases, dev builds and nika serve"
        ),
        "{}",
        one.detail
    );
}

/// A dev build never reads the keychain key. Beside it, with no fresh
/// record (none, or the item changed since a release wrote it), it names the
/// key it signs with, says one key or two is unknown here, and sends the
/// comparison and the gesture to a signed release.
#[test]
fn a_dev_build_beside_a_keychain_key_without_a_fresh_record_says_unknown() {
    let beside = run_key_finding(
        &Custody::new(None, KeychainUse::AdHoc, Some((key(), KeyFile::Private))).with_keys(
            Some(FILES.to_owned()),
            Some(true),
            None,
            Some(FILES.to_owned()),
        ),
    );
    assert_eq!(beside.level, Level::Warn);
    assert!(
        beside.detail.starts_with(&format!(
            "/home/u/.nika/keys/run-signing.key (0600) · signs with {FILES} · the keychain also holds a run-key, which a signed release signs with: one key or two is unknown here · "
        )) && beside.detail.contains("ad-hoc build"),
        "{}",
        beside.detail
    );
    assert_eq!(
        beside.fix.as_deref(),
        Some(
            "run nika doctor from a signed release to compare them; nika key align there makes them one"
        )
    );
}

/// With a fresh record a dev build compares: the keychain key a release
/// recorded is its own (one identity, after `nika key align`), or another
/// one (two identities, with the gesture that makes them one).
#[test]
fn a_dev_build_compares_through_a_fresh_record_of_the_keychain_key() {
    let dev = |recorded: &str| {
        run_key_finding(
            &Custody::new(None, KeychainUse::AdHoc, Some((key(), KeyFile::Private)))
                .with_keys(
                    Some(FILES.to_owned()),
                    Some(true),
                    None,
                    Some(FILES.to_owned()),
                )
                .with_record(Some(recorded.to_owned())),
        )
    };
    let one = dev(FILES);
    assert_eq!((one.level, one.fix.as_deref()), (Level::Ok, None));
    assert!(
        one.detail.contains(&format!(
            "signs with {FILES} · the keychain holds the same key, as a signed release recorded it: one identity · "
        )),
        "{}",
        one.detail
    );
    let two = dev(KEYCHAIN);
    assert_eq!(two.level, Level::Warn);
    assert!(
        two.detail.contains(&format!(
            "signs with {FILES} · a signed release signs with {KEYCHAIN}, the keychain key: two identities · "
        )),
        "{}",
        two.detail
    );
    assert_eq!(
        two.fix,
        Some(format!(
            "nika key align from a signed release (~/.nika/keys takes {KEYCHAIN}; {FILES} is retired, the journals it sealed stay verifiable)"
        ))
    );
}

/// Beside a keychain key a dev build is never told to mint one: that would
/// be a second identity. With no key anywhere, `nika key init` is the fix.
#[test]
fn a_keyless_dev_build_is_told_to_align_beside_a_keychain_key_and_to_init_without_one() {
    let beside = run_key_finding(
        &Custody::new(None, KeychainUse::AdHoc, Some((key(), KeyFile::Absent))).with_keys(
            None,
            Some(true),
            None,
            None,
        ),
    );
    let fix = beside.fix.unwrap_or_default();
    assert!(
        fix.contains("nika key align") && !fix.contains("nika key init"),
        "{fix}"
    );
    let alone = run_key_finding(
        &Custody::new(None, KeychainUse::AdHoc, Some((key(), KeyFile::Absent))).with_keys(
            None,
            Some(false),
            None,
            None,
        ),
    );
    assert_eq!(
        (alone.level, alone.fix.as_deref()),
        (Level::Warn, Some("nika key init"))
    );
}

#[test]
fn a_key_file_others_can_read_fails_like_ssh_refuses_it() {
    let loose = run_key_finding(&Custody::new(
        None,
        KeychainUse::Opened,
        Some((key(), KeyFile::Exposed(0o644))),
    ));
    assert_eq!(loose.level, Level::Fail);
    assert!(
        loose.detail.contains("readable by others (644)"),
        "{}",
        loose.detail
    );
    assert_eq!(
        loose.fix.as_deref(),
        Some("chmod 600 /home/u/.nika/keys/run-signing.key")
    );
    let explicit = run_key_finding(&Custody::new(
        Some(Ok((PathBuf::from("/ci/run.key"), KeyFile::Exposed(0o640)))),
        KeychainUse::Off,
        None,
    ));
    assert_eq!(explicit.level, Level::Fail);
    assert_eq!(explicit.fix.as_deref(), Some("chmod 600 /ci/run.key"));
}

#[test]
fn an_explicit_pair_is_the_custody_and_the_keychain_off_is_said() {
    let explicit = run_key_finding(
        &Custody::new(
            Some(Ok((PathBuf::from("/ci/run.key"), KeyFile::Private))),
            KeychainUse::Off,
            None,
        )
        .with_keys(Some(FILES.to_owned()), None, None, None),
    );
    assert_eq!(explicit.level, Level::Ok);
    assert_eq!(
        explicit.detail,
        format!("/ci/run.key (0600) · NIKA_RUN_KEY_FILE · signs with {FILES}")
    );
    let unpaired = run_key_finding(&Custody::new(
        Some(Ok((PathBuf::from("/ci/run.key"), KeyFile::Private))),
        KeychainUse::Off,
        None,
    ));
    assert_eq!(unpaired.level, Level::Fail);
    assert!(
        unpaired.detail.contains("not usable · runs stay unsigned"),
        "{}",
        unpaired.detail
    );
    let off = run_key_finding(&Custody::new(None, KeychainUse::Off, None));
    assert_eq!(off.level, Level::Warn);
    assert!(off.detail.contains("NIKA_KEYCHAIN"), "{}", off.detail);
    let broken = run_key_finding(&Custody::new(
        Some(Err(
            "NIKA_RUN_KEY_FILE and NIKA_RUN_PUB_FILE must both name distinct, non-empty paths"
                .to_owned(),
        )),
        KeychainUse::Opened,
        None,
    ));
    assert_eq!(broken.level, Level::Fail);
}
