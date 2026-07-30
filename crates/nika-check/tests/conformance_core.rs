// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
#![allow(clippy::disallowed_methods)]

//! Spec conformance · the FULL `core` suite.
//!
//! Walks every fixture under `nika-spec/conformance/tests/core/**`
//! (`input.yaml` + `expected.json`) and runs `parse` + `analyze`
//! against the expected verdict per `conformance/runner-protocol.md` ·
//!
//! - `valid: true`  → the engine MUST accept (zero errors).
//! - `valid: false` → the engine MUST reject AND at least one emitted
//!   error matches at least one expected entry · exact `code` match OR
//!   `namespace`-prefix + `category` match.
//! - `mode` defaults to `strict` (« the test default »).
//!
//! The spec dir resolves from `$NIKA_SPEC_DIR` or the sibling checkout
//! (`<engine>/../spec`) — the suite HARD-FAILS when missing (the
//! conformance gate must never silently skip). Harness plumbing is
//! shared with the `deep` tier (`tests/common/mod.rs`).
//!
//! The `CORE_GAPS` ledger mirrors the deep tier's ratchet: every entry
//! names the wave that owns the fixture's expected verdict, and the
//! suite asserts BOTH directions — a fixture NOT in the ledger MUST
//! pass, a fixture IN the ledger MUST still fail (a landed wave forces
//! removing its row · the ledger cannot go stale).

mod common;

use common::{fixture_dirs, fixture_verdict, skip_in_mutants_sandbox, spec_dir};

/// The core-tier gap ledger · fixture-name prefix → why the engine does
/// not hold the verdict yet. Closing a gap = implement + DELETE the row.
const CORE_GAPS: &[(&str, &str)] = &[
    // ── The R5 predicates wave CLOSED (spec #118 · the engine speaks
    // the outcome-class spellings success·failure·skipped·terminal —
    // the 8 after:-carrying rows deleted the day the rename landed, per
    // the ratchet « a landed wave forces removing its row »).
    // ── R3b · envelope/010 CLOSED 2026-07-30: the operator locked TYPE
    // (conformance `runner-protocol.md` class D — « the type system owns
    // the type-fit ») and the spec re-pointed the fixture at the
    // NIKA-TYPE-001 both oracles already emitted. The row deleted the
    // day the pin landed, per the ratchet.
    // ── EMPTY on purpose. The core tier is 131/131 and the ledger is
    // bidirectional: adding a row here is a claim that the engine
    // CANNOT hold a verdict, and the suite proves that claim by
    // failing when the fixture starts passing.
];

#[test]
fn core_conformance_suite() {
    if skip_in_mutants_sandbox() {
        return;
    }
    let core = spec_dir().join("conformance/tests/core");
    assert!(
        core.is_dir(),
        "conformance dir missing: {} — set NIKA_SPEC_DIR",
        core.display()
    );

    let mut failures: Vec<String> = Vec::new();
    let mut total = 0_usize;
    let mut gaps_hit = 0_usize;

    for dir in fixture_dirs(&core) {
        total += 1;
        let label = dir
            .strip_prefix(&core)
            .unwrap_or(&dir)
            .display()
            .to_string();
        let gap = CORE_GAPS.iter().find(|(name, _)| label.starts_with(name));
        let verdict = fixture_verdict(&dir, false);

        match (gap, verdict) {
            // Not in the ledger · must pass.
            (None, Some(failure)) => failures.push(format!("{label} · {failure}")),
            (None, None) => {}
            // In the ledger · must STILL fail (else the row is stale).
            (Some((name, reason)), None) => failures.push(format!(
                "{label} · PASSES but is in CORE_GAPS (`{name}` · {reason}) — \
                 the wave landed · DELETE its ledger row"
            )),
            (Some(_), Some(_)) => gaps_hit += 1,
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {total} core fixtures FAILED ·\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    assert_eq!(
        gaps_hit,
        CORE_GAPS.len(),
        "ledger drift — {gaps_hit} gap fixtures hit vs {} ledger rows \
         (a renamed/removed fixture leaves a dead row)",
        CORE_GAPS.len()
    );
    // Sanity floor — the suite ships 5 groups · 40+ fixtures.
    assert!(total >= 30, "only {total} fixtures walked — layout drift?");
}
