// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The retained a8 release: no longer issued for new authoring, its exact bytes still admitted for
//! the pins that name it, with the project blocks it presents.

use super::*;

pub(super) const A8_SNAPSHOT: &str =
    "b7f3861c55c785ba78fbf3fcfbb495ab79154b30f1bcb8483ce66018cc4659a9";
const A8_POLICY: &str = "41ba74af8c28cffa2abbbbbf5444e510ae9ddd045c4f197f6fdf217cd978987b";

pub(super) fn a8_identity() -> TrustedIdentity {
    TrustedIdentity::new(A8_SNAPSHOT, "policy-r", A8_POLICY).expect("issued")
}

#[test]
fn the_retained_a8_release_admits_in_memory_with_its_exact_counts() {
    assert_ne!(identity().unwrap(), a8_identity(), "a8 is no longer issued");
    let snapshot = admit(Some(&a8_identity())).expect("a8 admitted");
    assert_eq!(snapshot.manifest_sha256(), A8_SNAPSHOT);
    assert_eq!(snapshot.rows("patterns").len(), 12);
    assert_eq!(snapshot.rows("blocks").len(), 8);
    assert_eq!(snapshot.relations.len(), 20);
    assert!(snapshot.rows("examples").is_empty());
    assert!(snapshot.rows("skills").is_empty());
}

#[test]
#[cfg(unix)]
fn the_retained_a8_release_admits_identically_on_disk() {
    let root = tempfile::tempdir().unwrap();
    crate::fixture::write_files(root.path(), &a8::files()).unwrap();
    let disk = Snapshot::open(root.path(), Some(&a8_identity())).unwrap();
    let memory = admit(Some(&a8_identity())).unwrap();
    assert_eq!(disk.manifest_sha256(), memory.manifest_sha256());
    assert_eq!(disk.rows_sha256(), memory.rows_sha256());
}

#[test]
fn extra_project_blocks_are_presented_for_generic_intentions() {
    let snapshot = admit(Some(&a8_identity())).unwrap();
    for (intent, block) in [
        (
            "Group records by an observed field and compute exact numeric totals",
            "block:multi-csv-group-totals",
        ),
        (
            "Look up a record by its supplied key",
            "block:lookup-enrich-by-key",
        ),
        ("Compare two JSON documents", "block:validate-diff-convert"),
        (
            "Read a discovered file set with nika glob for each path",
            "block:glob-read-many",
        ),
    ] {
        let pack = snapshot.pack(intent, None).unwrap();
        let reference = pack
            .references
            .iter()
            .find(|r| r.id == block)
            .expect(intent);
        assert!(reference.text.contains("```yaml"));
        assert!(reference.text.contains("proof CHECKED"));
        assert_eq!(pack.identity["snapshot_sha256"], A8_SNAPSHOT);
        assert!(
            pack.references
                .iter()
                .all(|r| r.kind != "example" && r.kind != "skill")
        );
    }
}

#[test]
fn typed_output_remains_presented_alongside_data_processing_blocks() {
    let intent = "Read ./orders.csv, keep the rows whose status is paid, write them to ./paid.csv with the same header and write their total amount as a number to ./paid-total.txt. Declare typed workflow inputs and outputs, and expose the total amount as a number output.";
    let snapshot = admit(Some(&a8_identity())).unwrap();
    let pack = snapshot.pack(intent, None).unwrap();
    for id in [
        "pattern:typed-output",
        "block:typed-inputs-outputs",
        "block:multi-csv-group-totals",
    ] {
        assert!(
            pack.references.iter().any(|reference| reference.id == id),
            "{id}"
        );
    }
}

#[test]
fn a_changed_a8_file_and_a_wrong_policy_are_refused() {
    let mut files = a8::files();
    files.get_mut("blocks/lookup-enrich-by-key.nika").unwrap()[0] ^= 1;
    assert!(matches!(
        Snapshot::from_files(LABEL, files, Some(&a8_identity())),
        Err(KnowledgeError::Unavailable {
            code: RefusalCode::PinMismatch,
            ..
        })
    ));
    let wrong = TrustedIdentity::new(A8_SNAPSHOT, "policy-r", &"0".repeat(64)).unwrap();
    assert!(matches!(
        admit(Some(&wrong)),
        Err(KnowledgeError::Unavailable {
            code: RefusalCode::PolicyMismatch,
            ..
        })
    ));
}
