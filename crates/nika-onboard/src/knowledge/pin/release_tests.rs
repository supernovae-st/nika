// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The embedded releases through their pins: a new pin names the current release and reopens it,
//! and the pins of the retained releases reopen their exact bytes. Here beside the pin since the
//! knowledge door descended to its member (the ADR-137 amendment); each release's own suites stay
//! with it.

use super::{KnowledgeOrigin, KnowledgePin};
use crate::knowledge::{ComponentCatalog as _, Snapshot, TrustedIdentity, bundled};

/// The retained R3 release (profile r1, `policy-r`).
const R3_SNAPSHOT: &str = "b787fc53d6858db43d55958daaf02539fadcad4feeacc17b63c5aefcb92cc32b";
const R3_POLICY_SHA256: &str = "d0471eeb904416dd411fde12244918771a5578ae1526f1e0a775b5d421087f36";
/// The retained a8 release (profile r1, `policy-r`).
const A8_SNAPSHOT: &str = "b7f3861c55c785ba78fbf3fcfbb495ab79154b30f1bcb8483ce66018cc4659a9";
const A8_POLICY_SHA256: &str = "41ba74af8c28cffa2abbbbbf5444e510ae9ddd045c4f197f6fdf217cd978987b";

fn r3() -> TrustedIdentity {
    TrustedIdentity::new(R3_SNAPSHOT, "policy-r", R3_POLICY_SHA256).expect("issued")
}

fn a8() -> TrustedIdentity {
    TrustedIdentity::new(A8_SNAPSHOT, "policy-r", A8_POLICY_SHA256).expect("issued")
}

/// A pin of `snapshot` as an earlier project recorded it.
fn pin_of(snapshot: &Snapshot, identity: TrustedIdentity) -> KnowledgePin {
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

/// The rows of one kind a release lists in its public catalogue, by their id's prefix.
fn count(snapshot: &Snapshot, prefix: &str) -> usize {
    (snapshot.entries().iter())
        .filter(|row| row["id"].as_str().is_some_and(|id| id.starts_with(prefix)))
        .count()
}

#[test]
fn old_pins_reopen_exact_r3_and_a8() {
    let old = bundled::admit(Some(&r3())).expect("R3 retained");
    let pin = pin_of(&old, r3());
    let reopened = pin.reopen().expect("old pin reopened");
    assert_eq!(pin.moved(&reopened), None);
    assert_eq!(count(&reopened, "block:"), 3);
    let before = old
        .pack("Declare a typed output with a description", None)
        .unwrap();
    assert_eq!(
        before,
        reopened
            .pack("Declare a typed output with a description", None)
            .unwrap()
    );
    let a8_release = bundled::admit(Some(&a8())).expect("a8 retained");
    let pin = pin_of(&a8_release, a8());
    let reopened = pin.reopen().expect("a8 pin reopened");
    assert_eq!(pin.moved(&reopened), None);
    assert_eq!(reopened.manifest_sha256(), A8_SNAPSHOT);
    assert_eq!(count(&reopened, "block:"), 8);
}

/// A new pin names the release this build issues, whichever it is, and reopens its exact bytes;
/// the pins of a8 and R3 keep theirs.
#[test]
fn new_pins_name_the_current_release_and_earlier_pins_keep_their_bytes() {
    let current = bundled::identity().expect("issued");
    let fresh = KnowledgePin::embedded(None).unwrap();
    assert_eq!(fresh.manifest_sha256, current.snapshot_sha256());
    let reopened = fresh.reopen().expect("a new pin reopens");
    assert_eq!(fresh.moved(&reopened), None);
    let admitted = bundled::admit(Some(&current)).expect("the current release");
    assert_eq!(count(&reopened, "block:"), count(&admitted, "block:"));
    let a8_release = bundled::admit(Some(&a8())).expect("a8 retained");
    let earlier = pin_of(&a8_release, a8()).reopen().expect("a8 pin");
    assert_eq!(earlier.manifest_sha256(), A8_SNAPSHOT);
    assert_eq!(count(&earlier, "block:"), 8);
    let r3_pin = pin_of(&bundled::admit(Some(&r3())).unwrap(), r3()).reopen();
    assert_eq!(r3_pin.expect("R3 pin").manifest_sha256(), R3_SNAPSHOT);
}

/// The r2 release current before r3, retained whole: its pins reopen its exact bytes.
const RETAINED_R2_SNAPSHOT: &str =
    "1be7d6101dab9eff54f35be07463f4e320607c4a837d04ef90519abaadf48166";
const RETAINED_R2_POLICY_SHA256: &str =
    "53ef65a30e54220dfe76472f9fd766af2ef4817d4daea6bab38dab1337381cdf";

#[test]
fn the_retained_r2_release_reopens_its_exact_bytes() {
    let identity =
        TrustedIdentity::r2(RETAINED_R2_SNAPSHOT, "policy-r2", RETAINED_R2_POLICY_SHA256)
            .expect("issued");
    let snapshot = bundled::admit(Some(&identity)).expect("retained r2 admitted");
    assert_eq!(snapshot.manifest_sha256(), RETAINED_R2_SNAPSHOT);
    assert_eq!(count(&snapshot, "block:"), 37);
    assert_eq!(count(&snapshot, "family:"), 0);
    let earlier = pin_of(&snapshot, identity).reopen().expect("r2 pin");
    assert_eq!(earlier.manifest_sha256(), RETAINED_R2_SNAPSHOT);
}

/// The r3.2 release current before r3.5, retained whole: its pins reopen its exact bytes.
const RETAINED_R3_2_SNAPSHOT: &str =
    "8d52df01a5105fc4661851218e89ca172c5799bf3016aedbce1574ecac096075";
const RETAINED_R3_2_POLICY_SHA256: &str =
    "fb070b7ac25066c86c3fe41d4253d25741565b8c494908658661b868d1ae5764";

#[test]
fn the_retained_r3_2_release_reopens_its_exact_bytes() {
    let identity = TrustedIdentity::r2(
        RETAINED_R3_2_SNAPSHOT,
        "policy-r2",
        RETAINED_R3_2_POLICY_SHA256,
    )
    .expect("issued");
    let snapshot = bundled::admit(Some(&identity)).expect("retained r3.2 admitted");
    assert_eq!(snapshot.manifest_sha256(), RETAINED_R3_2_SNAPSHOT);
    assert_eq!(count(&snapshot, "block:"), 41);
    assert_eq!(count(&snapshot, "family:"), 16);
    let earlier = pin_of(&snapshot, identity).reopen().expect("r3.2 pin");
    assert_eq!(earlier.manifest_sha256(), RETAINED_R3_2_SNAPSHOT);
    assert_ne!(
        earlier.manifest_sha256(),
        bundled::identity().expect("issued").snapshot_sha256()
    );
}
