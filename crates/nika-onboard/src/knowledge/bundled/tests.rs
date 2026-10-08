// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The embedded release against its owner's record, written out here apart from the module's
//! constants (never [`identity`] as its own oracle): the table holds the issued payload, the memory
//! door admits it, lexical recall presents its patterns and realizing blocks, and unrelated
//! requests receive no references. Another identity or a moved byte is refused, typed.

use super::*;
use crate::knowledge::{ADMISSION_PROFILE, PACK_BUILDER};

/// The release owner's issued identity, independent of the module's constants.
const ISSUED_SNAPSHOT: &str = "b787fc53d6858db43d55958daaf02539fadcad4feeacc17b63c5aefcb92cc32b";
const ISSUED_POLICY: &str = "policy-r";
const ISSUED_POLICY_SHA256: &str =
    "d0471eeb904416dd411fde12244918771a5578ae1526f1e0a775b5d421087f36";
/// The payload as qualified: its files and their bytes.
const ISSUED_FILES: usize = 14;
const ISSUED_BYTES: usize = 82_439;

fn issued() -> TrustedIdentity {
    TrustedIdentity::new(ISSUED_SNAPSHOT, ISSUED_POLICY, ISSUED_POLICY_SHA256)
        .expect("the issued identity is of an identity's shape")
}

#[test]
fn the_r3_table_is_preserved_and_the_new_default_is_distinct() {
    assert_eq!(FILES.len(), ISSUED_FILES);
    let bytes: usize = FILES.iter().map(|(_, bytes)| bytes.len()).sum();
    assert_eq!(bytes, ISSUED_BYTES);
    assert_ne!(identity().expect("the new default identity"), issued());
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

/// Frozen queries select references, not a generated workflow's source.
#[test]
fn each_released_pattern_presents_its_realizing_block() {
    let snapshot = admit(Some(&issued())).expect("admitted");
    for (intent, pattern, block, file) in [
        (
            "Grant zero authority to a pure compute workflow",
            "pattern:declared-zero",
            "block:run-deterministic",
            "blocks/run-deterministic.nika",
        ),
        (
            "Declare a typed output with a description",
            "pattern:typed-output",
            "block:typed-inputs-outputs",
            "blocks/typed-inputs-outputs.nika",
        ),
        (
            "Skip the fallback step when a boolean flag is false",
            "pattern:guard-on-value",
            "block:when-skipped-fallback",
            "blocks/when-skipped-fallback.nika",
        ),
    ] {
        let pack = snapshot.pack(intent, None).expect("composed");
        assert_eq!(pack.identity["snapshot_sha256"], ISSUED_SNAPSHOT);
        assert_eq!(pack.identity["door"]["builder"], PACK_BUILDER);
        let ids: Vec<_> = pack
            .references
            .iter()
            .map(|r| (r.kind.as_str(), r.id.as_str()))
            .collect();
        assert_eq!(ids, [("pattern", pattern), ("block", block)], "{intent}");
        for kind in ["patterns", "blocks"] {
            let receipt = &pack.selection["receipt"][kind];
            for (field, expected) in [
                ("available", 3),
                ("candidates", 1),
                ("selected", 1),
                ("presented", 1),
            ] {
                assert_eq!(receipt[field], expected, "{intent}: {kind}.{field}");
            }
        }
        assert_eq!(
            pack.selection["blocks"][0]["why"],
            format!("realizes {pattern} · bm25")
        );
        let code =
            std::str::from_utf8(FILES.iter().find(|(path, _)| *path == file).unwrap().1).unwrap();
        let text = &pack.references[1].text;
        assert!(
            text.contains(code.trim_end()),
            "the admitted block is presented"
        );
        assert!(text.contains("proof CHECKED"), "{text}");
        assert!(text.contains("check CURRENT_CHECKED"), "{text}");
        assert_eq!(pack.selection["files"]["verified"], 1);
        assert!(pack.repairs.is_empty());
    }
}

#[test]
fn an_unrelated_intent_presents_nothing_from_the_release() {
    let snapshot = admit(Some(&issued())).expect("admitted");
    for intent in [
        "Translate this poem into French",
        "Summarize ./notes/brief.md into three bullets",
        "Fetch the weather forecast for Paris",
        "Read the invoice and email the total",
    ] {
        let pack = snapshot.pack(intent, None).expect("composed");
        assert!(
            pack.references.is_empty() && pack.repairs.is_empty(),
            "{intent}"
        );
        for kind in ["patterns", "blocks"] {
            assert_eq!(pack.selection["receipt"][kind]["available"], 3);
            for field in ["candidates", "selected", "presented"] {
                assert_eq!(
                    pack.selection["receipt"][kind][field], 0,
                    "{intent}: {kind}.{field}"
                );
            }
        }
        assert!(pack.selection["no_match"].is_string());
        assert_eq!(pack.selection["files"]["verified"], 0);
    }
}

#[test]
fn excluding_a_corpus_does_not_remove_patterns_or_blocks() {
    let snapshot = admit(Some(&issued())).expect("admitted");
    // This profile has no examples; corpus exclusion is an examples filter.
    for intent in [
        "Grant zero authority to a pure compute workflow",
        "Declare a typed output with a description",
        "Skip the fallback step when a boolean flag is false",
    ] {
        let pack = snapshot.pack(intent, None).expect("composed");
        assert_eq!(pack.references.len(), 2);
        for excluded in ["heldout", "another-corpus"] {
            assert_eq!(
                pack,
                snapshot.pack(intent, Some(excluded)).expect("composed")
            );
        }
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
    let previous = TrustedIdentity::new(
        "5bcd108a78e9fbb6e27827b34d8090b74f6285a125cdcef812b33dd51738e692",
        ISSUED_POLICY,
        "5b567a1557ba430fe57fe9a80934de29b2b09805d4cf26b4868e3395c8795465",
    )
    .expect("the previous release identity");
    assert_eq!(
        refused(admit(Some(&previous))),
        RefusalCode::IdentityMismatch
    );
    let wrong_policy =
        TrustedIdentity::new(ISSUED_SNAPSHOT, ISSUED_POLICY, &"0".repeat(64)).expect("shaped");
    assert_eq!(
        refused(admit(Some(&wrong_policy))),
        RefusalCode::PolicyMismatch
    );
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

mod a8_tests;
mod current_tests;
