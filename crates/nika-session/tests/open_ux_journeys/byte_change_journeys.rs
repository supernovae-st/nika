// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The change line of a revision comes from its bytes (QA gauntlet q03, q04).

use crate::scenarios::CLAIMED;
use crate::{outcome_kind, run};

/// The author writes the very bytes again with one more value bound and says it removed a line:
/// the proposal says its bytes did not change, and no « Changed from the proposal you saw ».
#[test]
fn a_revision_of_the_very_bytes_says_they_did_not_change() {
    let journey = run("same_bytes_again");
    let first = journey.step(0);
    assert_eq!(outcome_kind(first), "proposal", "{first:#}");
    let again = journey.step(1);
    assert_eq!(outcome_kind(again), "proposal", "{again:#}");
    let said = again["outcome"]["text"].as_str().unwrap_or_default();
    assert!(said.contains(CLAIMED), "{said}");
    assert!(
        said.contains("Même workflow que la proposition vue : ses octets n'ont pas changé."),
        "{said}"
    );
    assert!(!said.contains("Changé depuis la proposition vue"), "{said}");
}
