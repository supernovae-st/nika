// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The current release (r3 under the r2 run contract) against its producer's record, written out
//! apart from the module's constants: the issued identity, the payload's exact inventory and kind
//! counts, the same admission on disk and in memory, and a moved, added or foreign byte refused,
//! typed. Its pins are tested beside the pin, in nika-onboard (`knowledge::pin`).

use nika_compile_seats::foundry::release::r2;

use super::*;

const R2_SNAPSHOT: &str = "b9406fa6606b4c9656f72e9fe5d5898dc3842b7cf13804ad9a9f053a2568b004";
const R2_POLICY: &str = "policy-r2";
const R2_POLICY_SHA256: &str = "fb070b7ac25066c86c3fe41d4253d25741565b8c494908658661b868d1ae5764";
/// The payload as qualified: its files, their bytes, its rows by kind and its relations.
const R2_FILES: usize = 162;
const R2_BYTES: usize = 1_518_687;
const R2_ROWS: [(&str, usize); 15] = [
    ("blocks", 50),
    ("callables", 29),
    ("capability_interfaces", 32),
    ("constructs", 30),
    ("counterexamples", 44),
    ("diagnostics", 112),
    ("examples", 33),
    ("families", 18),
    ("intent_facets", 14),
    ("patterns", 94),
    ("pattern_packs", 31),
    ("repair_principles", 0),
    ("skeletons", 23),
    ("skills", 15),
    ("source_artifacts", 9),
];
const R2_RELATIONS: usize = 2564;

fn r2_identity() -> TrustedIdentity {
    TrustedIdentity::r2(R2_SNAPSHOT, R2_POLICY, R2_POLICY_SHA256).expect("issued")
}

/// The r3.2 release current before r3.5, retained whole: the records pinned to it reopen its
/// issued bytes, never the current ones.
const R3_2_SNAPSHOT: &str = "8d52df01a5105fc4661851218e89ca172c5799bf3016aedbce1574ecac096075";
const R3_2_FILES: usize = 153;
const R3_2_BYTES: usize = 1_297_216;
const R3_2_BLOCKS: usize = 41;
const R3_2_FAMILIES: usize = 16;
const R3_2_RELATIONS: usize = 2108;

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
    crate::fixture::write_files(root.path(), &current::files()).unwrap();
    let disk = Snapshot::open(root.path(), Some(&r2_identity())).unwrap();
    let memory = admit(Some(&r2_identity())).unwrap();
    assert_eq!(disk.manifest_sha256(), memory.manifest_sha256());
    assert_eq!(disk.rows_sha256(), memory.rows_sha256());
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

#[test]
fn the_retained_r3_2_release_reopens_its_issued_bytes() {
    let pinned = TrustedIdentity::r2(R3_2_SNAPSHOT, R2_POLICY, R2_POLICY_SHA256).expect("issued");
    let files = r3_2_20261012::files();
    assert_eq!(files.len(), R3_2_FILES);
    assert_eq!(files.values().map(Vec::len).sum::<usize>(), R3_2_BYTES);
    let snapshot = admit(Some(&pinned)).expect("the retained r3.2 release");
    assert_eq!(snapshot.manifest_sha256(), R3_2_SNAPSHOT);
    assert_eq!(snapshot.profile(), r2::PROFILE);
    assert_eq!(snapshot.rows("blocks").len(), R3_2_BLOCKS);
    assert_eq!(snapshot.rows("families").len(), R3_2_FAMILIES);
    assert_eq!(snapshot.relations.len(), R3_2_RELATIONS);
    assert_ne!(
        R3_2_SNAPSHOT,
        identity().unwrap().snapshot_sha256(),
        "new authoring never selects it"
    );
}
