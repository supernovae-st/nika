// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Bytes whose own check fails are never shown: the author reads the findings, repairs the
//! candidate, and only the repaired bytes reach the person (QA gauntlet q04).

use crate::scenarios::{CLAUDE, TEMPERATURE};
use crate::{candidate, outcome_kind, run, work};

/// A temperature on a Claude 5 model fails the candidate's own check: the proposal is refused to
/// the author with the check's finding, nothing offers to save or run those bytes, and the
/// person is shown only the repaired candidate, whose check is clean.
#[test]
fn bytes_failing_their_own_check_are_never_shown() {
    let journey = run("failing_check");
    let refused = (journey.agent.iter())
        .map(crate::peer::Seen::last)
        .find(|said| said.contains("the check of these bytes fails"));
    let refused = refused.expect("the author read the check's refusal");
    assert!(refused.contains("temperature"), "{refused}");
    let shown = journey.step(0);
    assert_eq!(outcome_kind(shown), "proposal", "{shown:#}");
    let bytes = candidate(shown);
    assert!(bytes.contains(CLAUDE), "{bytes}");
    assert!(
        !bytes.contains(TEMPERATURE),
        "only the repaired bytes: {bytes}"
    );
    let preview = shown["outcome"]["text"].as_str().unwrap_or_default();
    assert!(!preview.contains("findings ✖"), "{preview}");
    assert!(work(shown)["saved"].is_null(), "{shown:#}");
    assert_eq!(
        journey.agent.len(),
        4,
        "the refusal sent the author to repair once"
    );
}
