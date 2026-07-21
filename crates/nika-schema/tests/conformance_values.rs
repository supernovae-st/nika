// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
#![allow(clippy::disallowed_methods)]

//! Spec conformance · the value-authority suite (C2 · the E-split).
//!
//! Walks every fixture under `nika-spec/conformance/values/{valid,invalid}/`
//! (`input.yaml` + `expected.json`) and runs `parse` + `analyze` against
//! the expected verdict per `conformance/runner-protocol.md` — the same
//! matching rule as the core tier (exact `code` OR `namespace`-prefix +
//! `category`). The analyzer's `NIKA-VALUES-003` rides LAYERED alongside
//! the `NIKA-VAR-001` unresolved refusal, exactly as the oracle's runner
//! emits both and the protocol matches either.
//!
//! The `VALUES_GAPS` ledger is the same ratchet the deep tier runs ·
//! every entry names the wave that owns the fixture's verdict, and the
//! suite asserts BOTH directions — a fixture NOT in the ledger MUST
//! pass, a fixture IN the ledger MUST still fail (a landed wave forces
//! removing its row · the ledger cannot go stale). Never a silent skip.

mod common;

use common::{fixture_dirs, fixture_verdict, skip_in_mutants_sandbox, spec_dir};

/// The values-tier gap ledger · fixture-name prefix → why the engine
/// does not hold the verdict yet. Closing a gap = implement + DELETE the
/// row.
const VALUES_GAPS: &[(&str, &str)] = &[
    // valid/default-conforms-to-type + invalid/default-type-mismatch
    // CLOSED 2026-07-21 (the R3b wave) — the declaration `type:` speaks
    // the FULL TypeExpr of 09-types (LAW-GRAMMAR-0211 · the parser is
    // shape-only, the analyzer's `check_io_declarations` judges the
    // grammar `NIKA-TYPE-001/006` — `bool` the one boolean spelling,
    // NIKA-PARSE-015 retired-never-reused), and every declared
    // `default:` / typed `const:` `value:` conforms to its declared
    // type (`NIKA-DEFAULT-001` · LAW-TYPE-0211 · the P0 soundness hole
    // closed · the one type core's `fits`, one code for const too).
    // Rows deleted per the ratchet (« the suite fails on a stale row by
    // design »).
];

#[test]
fn values_conformance_suite() {
    if skip_in_mutants_sandbox() {
        return;
    }
    let values = spec_dir().join("conformance/values");
    assert!(
        values.is_dir(),
        "conformance dir missing: {} — set NIKA_SPEC_DIR",
        values.display()
    );

    let mut failures: Vec<String> = Vec::new();
    let mut total = 0_usize;
    let mut gaps_hit = 0_usize;

    for dir in fixture_dirs(&values) {
        total += 1;
        let label = dir
            .strip_prefix(&values)
            .unwrap_or(&dir)
            .display()
            .to_string();
        let gap = VALUES_GAPS.iter().find(|(name, _)| label.starts_with(name));
        let verdict = fixture_verdict(&dir, false);

        match (gap, verdict) {
            // Not in the ledger · must pass.
            (None, Some(failure)) => failures.push(format!("{label} · {failure}")),
            (None, None) => {}
            // In the ledger · must STILL fail (else the row is stale).
            (Some((name, reason)), None) => failures.push(format!(
                "{label} · PASSES but is in VALUES_GAPS (`{name}` · {reason}) — \
                 the wave landed · DELETE its ledger row"
            )),
            (Some(_), Some(_)) => gaps_hit += 1,
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {total} values fixtures FAILED ·\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    assert_eq!(
        gaps_hit,
        VALUES_GAPS.len(),
        "ledger drift — {gaps_hit} gap fixtures hit vs {} ledger rows \
         (a renamed/removed fixture leaves a dead row)",
        VALUES_GAPS.len()
    );
    // Sanity floor — the suite ships 5 valid + 5 invalid fixtures.
    assert!(total >= 10, "only {total} fixtures walked — layout drift?");
}
