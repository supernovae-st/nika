// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The retained a8 release: no longer issued for new authoring, its exact bytes still admitted for
//! the pins that name it, with the project blocks it presents.

use super::*;
use crate::knowledge::pin::{KnowledgeOrigin, KnowledgePin};

pub(super) const A8_SNAPSHOT: &str =
    "b7f3861c55c785ba78fbf3fcfbb495ab79154b30f1bcb8483ce66018cc4659a9";
const A8_POLICY: &str = "41ba74af8c28cffa2abbbbbf5444e510ae9ddd045c4f197f6fdf217cd978987b";

pub(super) fn a8_identity() -> TrustedIdentity {
    TrustedIdentity::new(A8_SNAPSHOT, "policy-r", A8_POLICY).expect("issued")
}

/// A pin of `snapshot` as an earlier project recorded it.
pub(super) fn pin_of(snapshot: &Snapshot, identity: TrustedIdentity) -> KnowledgePin {
    KnowledgePin {
        origin: KnowledgeOrigin::Embedded,
        exclude_corpus: None,
        version: snapshot.version().map(str::to_owned),
        digest: snapshot.digest().map(str::to_owned),
        manifest_sha256: identity.snapshot_sha256().to_owned(),
        rows_sha256: snapshot.rows_sha256(),
        identity: Some(identity),
    }
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
    crate::knowledge::fixture::write_files(root.path(), &a8::files()).unwrap();
    let disk = Snapshot::open(root.path(), Some(&a8_identity())).unwrap();
    let memory = admit(Some(&a8_identity())).unwrap();
    assert_eq!(disk.manifest_sha256(), memory.manifest_sha256());
    assert_eq!(disk.rows_sha256(), memory.rows_sha256());
}

#[test]
fn old_pins_reopen_exact_r3_and_a8() {
    let old = admit(Some(&issued())).expect("R3 retained");
    let pin = pin_of(&old, issued());
    let reopened = pin.reopen().expect("old pin reopened");
    assert_eq!(pin.moved(&reopened), None);
    assert_eq!(reopened.rows("blocks").len(), 3);
    let before = old
        .pack("Declare a typed output with a description", None)
        .unwrap();
    assert_eq!(
        before,
        reopened
            .pack("Declare a typed output with a description", None)
            .unwrap()
    );
    let a8 = admit(Some(&a8_identity())).expect("a8 retained");
    let pin = pin_of(&a8, a8_identity());
    let reopened = pin.reopen().expect("a8 pin reopened");
    assert_eq!(pin.moved(&reopened), None);
    assert_eq!(reopened.manifest_sha256(), A8_SNAPSHOT);
    assert_eq!(reopened.rows("blocks").len(), 8);
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
