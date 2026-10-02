// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The embedded release against its owner's record, written out here apart from the module's
//! constants (never [`identity`] as its own oracle): the table holds the issued payload, the memory
//! door admits it, a pack composed from it is said as it is — admitted, and empty of references —
//! and another identity or a moved byte is refused, typed.

use super::*;
use crate::knowledge::{ADMISSION_PROFILE, PACK_BUILDER};

/// The release owner's issued identity, independent of the module's constants.
const ISSUED_SNAPSHOT: &str = "5bcd108a78e9fbb6e27827b34d8090b74f6285a125cdcef812b33dd51738e692";
const ISSUED_POLICY: &str = "policy-r";
const ISSUED_POLICY_SHA256: &str =
    "5b567a1557ba430fe57fe9a80934de29b2b09805d4cf26b4868e3395c8795465";
/// The payload as qualified: its files and their bytes.
const ISSUED_FILES: usize = 14;
const ISSUED_BYTES: usize = 80_491;

fn issued() -> TrustedIdentity {
    TrustedIdentity::new(ISSUED_SNAPSHOT, ISSUED_POLICY, ISSUED_POLICY_SHA256)
        .expect("the issued identity is of an identity's shape")
}

#[test]
fn the_table_is_the_issued_payload_and_the_build_trusts_the_issued_identity() {
    assert_eq!(FILES.len(), ISSUED_FILES);
    let bytes: usize = FILES.iter().map(|(_, bytes)| bytes.len()).sum();
    assert_eq!(bytes, ISSUED_BYTES);
    assert_eq!(identity().expect("the build's identity"), issued());
}

#[test]
fn the_embedded_release_is_admitted_in_memory_under_its_label() {
    let snapshot = admit(Some(&issued())).expect("admitted");
    assert_eq!(snapshot.manifest_sha256(), ISSUED_SNAPSHOT);
    let record = snapshot.identity();
    assert_eq!(record["snapshot_sha256"], ISSUED_SNAPSHOT);
    assert_eq!(record["dir"], LABEL, "a label, never a path on disk");
    assert_eq!(record["verification"]["admission"], ADMISSION_PROFILE);
    assert_eq!(record["verification"]["policy"]["id"], ISSUED_POLICY);
    assert_eq!(
        record["verification"]["policy"]["sha256"],
        ISSUED_POLICY_SHA256
    );
}

/// Available and admitted is not composed: the payload holds no pattern, so no block is reached,
/// even by an intent in the blocks' own words, and no repair principle is wired. The pack says so.
#[test]
fn a_pack_from_the_embedded_release_states_its_identity_and_presents_no_reference() {
    let snapshot = admit(Some(&issued())).expect("admitted");
    for intent in [
        "Read ./notes/brief.md and write a summary of it to ./out/summary.md",
        "a deterministic run with typed inputs and outputs, and a skipped fallback",
    ] {
        let pack = snapshot.pack(intent, Some("heldout")).expect("composed");
        assert_eq!(pack.identity["snapshot_sha256"], ISSUED_SNAPSHOT);
        assert_eq!(pack.identity["door"]["builder"], PACK_BUILDER);
        assert!(pack.references.is_empty(), "{intent}: {:#}", pack.selection);
        assert!(pack.repairs.is_empty(), "{intent}");
        let receipt = &pack.selection["receipt"];
        assert_eq!(receipt["patterns"]["available"], 0, "{intent}");
        assert_eq!(receipt["blocks"]["available"], 3, "{intent}");
        assert_eq!(receipt["blocks"]["selected"], 0, "{intent}");
    }
}

#[test]
fn another_identity_or_a_moved_byte_is_refused_typed_never_another_source() {
    let refused = |result: Result<Snapshot, KnowledgeError>| match result {
        Err(KnowledgeError::Unavailable { root, code, .. }) => {
            assert_eq!(root, PathBuf::from(LABEL));
            code
        }
        other => panic!("refused: {other:?}"),
    };
    assert_eq!(refused(admit(None)), RefusalCode::Untrusted);
    let other =
        TrustedIdentity::new(&"0".repeat(64), ISSUED_POLICY, ISSUED_POLICY_SHA256).expect("shaped");
    assert_eq!(refused(admit(Some(&other))), RefusalCode::IdentityMismatch);
    let mut files: BTreeMap<String, Vec<u8>> = FILES
        .into_iter()
        .map(|(path, bytes)| (path.to_owned(), bytes.to_vec()))
        .collect();
    files.get_mut("NOTICE.md").expect("issued")[0] ^= 1;
    assert_eq!(
        refused(Snapshot::from_files(LABEL, files, Some(&issued()))),
        RefusalCode::PinMismatch
    );
}
