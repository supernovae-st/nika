// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Admit the issued knowledge release, as this build embeds it (`bundled`'s table), through the
//! public memory and disk doors, against the owner's identity written out here.

use std::collections::BTreeMap;

use super::super::{Snapshot, bundled, fixture};
use super::TrustedIdentity;

// The release owner's issued identity, independent of the payload's own declarations.
const EXPECTED_SNAPSHOT: &str = "5bcd108a78e9fbb6e27827b34d8090b74f6285a125cdcef812b33dd51738e692";
const EXPECTED_POLICY: &str = "policy-r";
const EXPECTED_POLICY_SHA256: &str =
    "5b567a1557ba430fe57fe9a80934de29b2b09805d4cf26b4868e3395c8795465";
const EXPECTED_PROFILE: &str = "nika-knowledge-release-profile/r1";

fn files() -> BTreeMap<String, Vec<u8>> {
    bundled::FILES
        .into_iter()
        .map(|(path, bytes)| (path.to_owned(), bytes.to_vec()))
        .collect()
}

fn trusted_identity() -> Result<TrustedIdentity, &'static str> {
    TrustedIdentity::new(EXPECTED_SNAPSHOT, EXPECTED_POLICY, EXPECTED_POLICY_SHA256)
        .ok_or("the issued release identity must have the required shape")
}

fn assert_identity(snapshot: &Snapshot) {
    assert_eq!(snapshot.manifest_sha256(), EXPECTED_SNAPSHOT);
    let admitted = snapshot.identity();
    assert_eq!(admitted["snapshot_sha256"], EXPECTED_SNAPSHOT);
    assert_eq!(admitted["verification"]["admission"], EXPECTED_PROFILE);
    assert_eq!(admitted["verification"]["profile"], EXPECTED_PROFILE);
    assert_eq!(admitted["verification"]["policy"]["id"], EXPECTED_POLICY);
    assert_eq!(
        admitted["verification"]["policy"]["sha256"],
        EXPECTED_POLICY_SHA256
    );
}

#[test]
fn qualified_payload_is_admitted_in_memory() -> Result<(), Box<dyn std::error::Error>> {
    let trusted = trusted_identity()?;
    let snapshot = Snapshot::from_files("qualified-release", files(), Some(&trusted))?;
    assert_identity(&snapshot);
    Ok(())
}

#[test]
#[cfg(unix)]
fn qualified_payload_is_admitted_on_disk() -> Result<(), Box<dyn std::error::Error>> {
    let trusted = trusted_identity()?;
    let root = tempfile::tempdir()?;
    assert!(root.path().is_absolute());
    fixture::write_files(root.path(), &files())?;
    let snapshot = Snapshot::open(root.path(), Some(&trusted))?;
    assert_identity(&snapshot);
    Ok(())
}
