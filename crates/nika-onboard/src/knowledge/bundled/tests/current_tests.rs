// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The current release, the r2 run contract, against its producer's record, written out here apart
//! from the module's constants: the issued identity, the payload's exact inventory and kind counts,
//! the same admission on disk and in memory, new pins naming it while a8 and R3 pins keep their
//! own bytes, and a moved, added or foreign byte refused, typed.

use nika_compile_seats::foundry::release::r2;

use super::a8_tests::{A8_SNAPSHOT, a8_identity, pin_of};
use super::*;
use crate::knowledge::pin::KnowledgePin;

const R2_SNAPSHOT: &str = "1be7d6101dab9eff54f35be07463f4e320607c4a837d04ef90519abaadf48166";
const R2_POLICY: &str = "policy-r2";
const R2_POLICY_SHA256: &str = "53ef65a30e54220dfe76472f9fd766af2ef4817d4daea6bab38dab1337381cdf";
/// The payload as qualified: its files, their bytes, its rows by kind and its relations.
const R2_FILES: usize = 137;
const R2_BYTES: usize = 951_856;
const R2_ROWS: [(&str, usize); 15] = [
    ("blocks", 37),
    ("callables", 29),
    ("capability_interfaces", 32),
    ("constructs", 30),
    ("counterexamples", 38),
    ("diagnostics", 106),
    ("examples", 26),
    ("families", 0),
    ("intent_facets", 0),
    ("patterns", 20),
    ("pattern_packs", 1),
    ("repair_principles", 0),
    ("skeletons", 23),
    ("skills", 16),
    ("source_artifacts", 9),
];
const R2_RELATIONS: usize = 1121;

fn r2_identity() -> TrustedIdentity {
    TrustedIdentity::r2(R2_SNAPSHOT, R2_POLICY, R2_POLICY_SHA256).expect("issued")
}

/// The typed refusal of the embedded door, under its label.
fn refused(result: Result<Snapshot, KnowledgeError>) -> RefusalCode {
    match result {
        Err(KnowledgeError::Unavailable { root, code, .. }) => {
            assert_eq!(root, PathBuf::from(LABEL));
            code
        }
        other => panic!("refused: {other:?}"),
    }
}

#[test]
fn the_current_release_is_the_issued_r2_payload_with_its_exact_counts() {
    assert_eq!(identity().unwrap(), r2_identity());
    let files = current::files();
    assert_eq!(files.len(), R2_FILES);
    assert_eq!(files.values().map(Vec::len).sum::<usize>(), R2_BYTES);
    let snapshot = admit(Some(&r2_identity())).expect("current admitted");
    assert_eq!(snapshot.manifest_sha256(), R2_SNAPSHOT);
    assert_eq!(snapshot.profile(), r2::PROFILE);
    let record = snapshot.identity();
    assert_eq!(record["dir"], LABEL, "a label, never a path on disk");
    assert_eq!(record["verification"]["admission"], r2::PROFILE);
    assert_eq!(record["verification"]["policy"]["id"], R2_POLICY);
    assert_eq!(record["verification"]["policy"]["sha256"], R2_POLICY_SHA256);
    for (stem, rows) in R2_ROWS {
        assert_eq!(snapshot.rows(stem).len(), rows, "{stem}");
    }
    assert_eq!(snapshot.relations.len(), R2_RELATIONS);
}

#[test]
#[cfg(unix)]
fn the_current_release_admits_identically_on_disk() {
    let root = tempfile::tempdir().unwrap();
    crate::knowledge::fixture::write_files(root.path(), &current::files()).unwrap();
    let disk = Snapshot::open(root.path(), Some(&r2_identity())).unwrap();
    let memory = admit(Some(&r2_identity())).unwrap();
    assert_eq!(disk.manifest_sha256(), memory.manifest_sha256());
    assert_eq!(disk.rows_sha256(), memory.rows_sha256());
}

#[test]
fn new_pins_name_r2_and_earlier_pins_keep_their_bytes() {
    let fresh = KnowledgePin::embedded(None).unwrap();
    assert_eq!(fresh.manifest_sha256, R2_SNAPSHOT);
    let reopened = fresh.reopen().expect("a new pin reopens");
    assert_eq!(fresh.moved(&reopened), None);
    assert_eq!(reopened.rows("blocks").len(), 37);
    let a8 = admit(Some(&a8_identity())).expect("a8 retained");
    let earlier = pin_of(&a8, a8_identity()).reopen().expect("a8 pin");
    assert_eq!(earlier.manifest_sha256(), A8_SNAPSHOT);
    assert_eq!(earlier.rows("blocks").len(), 8);
    let r3 = pin_of(&admit(Some(&issued())).unwrap(), issued()).reopen();
    assert_eq!(r3.expect("R3 pin").manifest_sha256(), ISSUED_SNAPSHOT);
}

#[test]
fn a_moved_or_added_current_file_and_a_wrong_policy_are_refused() {
    let mut files = current::files();
    files
        .get_mut("blocks/validate-quarantine-total.nika")
        .expect("the composition witness's block")[0] ^= 1;
    assert_eq!(
        refused(Snapshot::from_files(LABEL, files, Some(&r2_identity()))),
        RefusalCode::PinMismatch
    );
    let mut files = current::files();
    files.insert(
        "blocks/unlisted.nika".to_owned(),
        b"nika: unlisted\n".to_vec(),
    );
    assert!(matches!(
        refused(Snapshot::from_files(LABEL, files, Some(&r2_identity()))),
        RefusalCode::ExtraFile | RefusalCode::UnexpectedFile
    ));
    let wrong = TrustedIdentity::r2(R2_SNAPSHOT, R2_POLICY, &"0".repeat(64)).unwrap();
    assert_eq!(refused(admit(Some(&wrong))), RefusalCode::PolicyMismatch);
}
