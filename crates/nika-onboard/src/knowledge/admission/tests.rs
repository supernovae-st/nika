// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The door's own API: the trusted identity's shape, the identity judged before the payload's
//! own claims, the fixture other doors' tests build on, and the snapshot's identity record. The
//! contract's rules themselves are witnessed by the shared vectors (`vectors.rs`) and the
//! constructed cases (`constructed.rs`).

#![cfg_attr(not(unix), allow(unused_imports, dead_code))]

use std::collections::BTreeSet;

use serde_json::json;

use super::super::fixture::{self, Payload};
use super::super::{KnowledgeError, Snapshot};
use super::{ADMISSION_PROFILE, RefusalCode, TrustedIdentity, admit_memory};

const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn refused_code<T: std::fmt::Debug>(result: Result<T, KnowledgeError>) -> RefusalCode {
    match result {
        Err(KnowledgeError::Unavailable { code, .. }) => code,
        other => panic!("not a refusal: {other:?}"),
    }
}

#[test]
fn every_code_has_one_distinct_word_and_displays_it() {
    let words: BTreeSet<String> = RefusalCode::ALL.iter().map(ToString::to_string).collect();
    assert_eq!(words.len(), 38);
    for code in RefusalCode::ALL {
        assert_eq!(code.to_string(), code.as_str());
    }
}

#[test]
fn a_trusted_identity_is_exactly_the_profiles_shape() {
    let identity = TrustedIdentity::new(SHA, "policy-r", SHA).expect("shaped");
    assert_eq!(identity.profile(), ADMISSION_PROFILE);
    let record = json!({
        "profile": ADMISSION_PROFILE,
        "snapshot_sha256": SHA,
        "policy": {"id": "policy-r", "sha256": SHA},
    });
    assert_eq!(TrustedIdentity::from_json(&record), Some(identity));
    let upper = SHA.to_uppercase();
    for malformed in [
        json!(null),
        json!({"profile": "nika-knowledge-release-profile/r2", "snapshot_sha256": SHA,
               "policy": {"id": "policy-r", "sha256": SHA}}),
        json!({"profile": ADMISSION_PROFILE, "snapshot_sha256": upper,
               "policy": {"id": "policy-r", "sha256": SHA}}),
        json!({"profile": ADMISSION_PROFILE, "snapshot_sha256": SHA,
               "policy": {"id": "Policy R", "sha256": SHA}}),
        json!({"profile": ADMISSION_PROFILE, "snapshot_sha256": SHA,
               "policy": {"id": "policy-r", "sha256": SHA, "format": "x"}}),
        json!({"profile": ADMISSION_PROFILE, "snapshot_sha256": SHA,
               "policy": {"id": "policy-r", "sha256": SHA}, "binary_sha256": SHA}),
    ] {
        assert_eq!(TrustedIdentity::from_json(&malformed), None, "{malformed}");
    }
    assert_eq!(TrustedIdentity::new(&SHA[..63], "policy-r", SHA), None);
    assert_eq!(TrustedIdentity::new(SHA, "-policy", SHA), None);
}

#[test]
#[cfg(unix)] // the disk form is defined for Unix descriptors only
fn the_fixture_is_admitted_in_memory_and_on_disk_against_its_identity() {
    let payload = Payload::minimal();
    let identity = payload.identity().expect("the fixture's identity");
    let files = payload.files();
    let admitted = admit_memory(files.clone(), Some(&identity)).expect("admitted in memory");
    assert_eq!(admitted.manifest_sha256, identity.snapshot_sha256());
    let root = tempfile::tempdir().expect("a scratch root");
    payload.write(root.path()).expect("written");
    let snapshot = Snapshot::open(root.path(), Some(&identity)).expect("admitted on disk");
    let record = snapshot.identity();
    assert_eq!(record["snapshot_sha256"], identity.snapshot_sha256());
    assert_eq!(record["verification"]["admission"], ADMISSION_PROFILE);
    assert_eq!(record["verification"]["profile"], ADMISSION_PROFILE);
    assert_eq!(record["verification"]["policy"]["id"], fixture::POLICY_ID);
    let memory = Snapshot::from_files("fixture", files, Some(&identity)).expect("admitted");
    assert_eq!(memory.manifest_sha256(), snapshot.manifest_sha256());
}

#[test]
#[cfg(unix)] // the disk form is defined for Unix descriptors only
fn only_the_trusted_identity_admits_whatever_the_payload_says_of_itself() {
    let payload = Payload::minimal();
    let root = tempfile::tempdir().expect("a scratch root");
    payload.write(root.path()).expect("written");
    assert_eq!(
        refused_code(Snapshot::open(root.path(), None)),
        RefusalCode::Untrusted
    );
    // The identity of another release: the payload's own digests never stand in for it.
    let other = TrustedIdentity::new(&"0".repeat(64), fixture::POLICY_ID, fixture::POLICY_SHA256)
        .expect("shaped");
    assert_eq!(
        refused_code(Snapshot::open(root.path(), Some(&other))),
        RefusalCode::IdentityMismatch
    );
    // The right bytes under a policy the embedder does not trust.
    let identity = payload.identity().expect("the fixture's identity");
    let policy = TrustedIdentity::new(
        identity.snapshot_sha256(),
        "policy-s",
        fixture::POLICY_SHA256,
    )
    .expect("shaped");
    assert_eq!(
        refused_code(Snapshot::open(root.path(), Some(&policy))),
        RefusalCode::PolicyMismatch
    );
    assert_eq!(
        refused_code(Snapshot::from_files("memory", payload.files(), None)),
        RefusalCode::Untrusted
    );
}
