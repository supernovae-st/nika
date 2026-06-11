// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
#![allow(clippy::disallowed_methods)]

//! Spec conformance · the `deep` static tier — the expression layer the
//! JSON Schema cannot see (CEL EBNF parse · `when:` shape · binding
//! purity · durations · builtin arg shapes · permits-fit).
//!
//! The `DEEP_GAPS` ledger is a RATCHET, not a skip list · every entry
//! names an unimplemented check with its reason, and the suite asserts
//! BOTH directions ·
//!
//! - a fixture NOT in the ledger MUST pass (a regression screams);
//! - a fixture IN the ledger MUST still fail (an implemented check
//!   forces removing its row — the ledger cannot go stale).

mod common;

use common::{fixture_dirs, fixture_verdict, spec_dir};

/// The deep-tier gap ledger · fixture-name prefix → why the engine
/// does not implement the check yet. Closing a gap = implement + DELETE
/// the row (the suite fails on a stale row by design).
const DEEP_GAPS: &[(&str, &str)] = &[
    (
        "005-invalid-schema",
        "schema: blocks are not meta-validated (needs a Draft 2020-12 \
         check_schema pass · jsonschema-class dep decision pending)",
    ),
    (
        "006-jq-compile-error",
        "jq compile-checking needs a jq engine in the validator (jaq \
         dep decision pending · the Python oracle shells out to jq)",
    ),
    (
        "009-write-without-content",
        "builtin arg-shape checks (nika:write content:) not implemented",
    ),
    (
        "010-done-standalone",
        "nika:done placement check (agent-loop sentinel) not implemented",
    ),
    (
        "011-jq-wrong-arg-name",
        "builtin arg-shape checks (nika:jq expression:) not implemented",
    ),
    (
        "012-wait-both-modes",
        "builtin arg-shape checks (nika:wait duration XOR until) not \
         implemented",
    ),
    (
        "013-permits-fit-violation",
        "PERMITS-FIT static check (body must fit the declared boundary \
         · NIKA-SEC-004) not implemented",
    ),
];

#[test]
fn deep_conformance_suite() {
    let deep = spec_dir().join("conformance/tests/deep");
    assert!(
        deep.is_dir(),
        "conformance dir missing: {} — set NIKA_SPEC_DIR",
        deep.display()
    );

    let mut failures: Vec<String> = Vec::new();
    let mut total = 0_usize;
    let mut gaps_hit = 0_usize;

    for dir in fixture_dirs(&deep) {
        total += 1;
        let label = dir
            .strip_prefix(&deep)
            .unwrap_or(&dir)
            .display()
            .to_string();
        let gap = DEEP_GAPS.iter().find(|(name, _)| label.starts_with(name));
        let verdict = fixture_verdict(&dir);

        match (gap, verdict) {
            // Not in the ledger · must pass.
            (None, Some(failure)) => failures.push(format!("{label} · {failure}")),
            (None, None) => {}
            // In the ledger · must STILL fail (else the row is stale).
            (Some((name, reason)), None) => failures.push(format!(
                "{label} · PASSES but is in DEEP_GAPS (`{name}` · {reason}) — \
                 the check landed · DELETE its ledger row"
            )),
            (Some(_), Some(_)) => gaps_hit += 1,
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {total} deep fixtures FAILED ·\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    assert_eq!(
        gaps_hit,
        DEEP_GAPS.len(),
        "ledger drift — {gaps_hit} gap fixtures hit vs {} ledger rows \
         (a renamed/removed fixture leaves a dead row)",
        DEEP_GAPS.len()
    );
    // Sanity floor — the deep tier ships 14+ fixtures.
    assert!(total >= 12, "only {total} fixtures walked — layout drift?");
}
