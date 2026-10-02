// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Admit the issued knowledge release, as this build embeds it (`bundled`'s table), through the
//! public memory and disk doors, against the owner's identity written out here.

use std::collections::BTreeMap;

use super::super::{Snapshot, bundled, fixture};
use super::TrustedIdentity;

// The release owner's issued identity, independent of the payload's own declarations.
const EXPECTED_SNAPSHOT: &str = "effc8d45b88a62c08cd4569abaadb8863823baaa0d52a313b925e9e1faf51b11";
const EXPECTED_POLICY: &str = "policy-r";
const EXPECTED_POLICY_SHA256: &str =
    "d0471eeb904416dd411fde12244918771a5578ae1526f1e0a775b5d421087f36";
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
    assert_eq!(snapshot.rows("patterns").len(), 3);
    assert_eq!(snapshot.rows("blocks").len(), 3);
    assert!(snapshot.rows("examples").is_empty());
    assert_eq!(snapshot.relations.len(), 3);
    for (block, pattern) in [
        ("block:run-deterministic", "pattern:declared-zero"),
        ("block:typed-inputs-outputs", "pattern:typed-output"),
        ("block:when-skipped-fallback", "pattern:guard-on-value"),
    ] {
        assert!(snapshot.relations.iter().any(|edge| {
            edge["from"] == block && edge["rel"] == "REALIZES" && edge["to"] == pattern
        }));
    }
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
fn qualified_payload_is_admitted_in_memory() -> Result<(), String> {
    let trusted = trusted_identity()?;
    let snapshot = Snapshot::from_files("qualified-release", files(), Some(&trusted))
        .map_err(|error| error.to_string())?;
    assert_identity(&snapshot);
    Ok(())
}

#[test]
#[cfg(unix)]
fn qualified_payload_is_admitted_on_disk() -> Result<(), String> {
    let trusted = trusted_identity()?;
    let root = tempfile::tempdir().map_err(|error| error.to_string())?;
    assert!(root.path().is_absolute());
    fixture::write_files(root.path(), &files()).map_err(|error| error.to_string())?;
    let snapshot =
        Snapshot::open(root.path(), Some(&trusted)).map_err(|error| error.to_string())?;
    assert_identity(&snapshot);
    Ok(())
}
