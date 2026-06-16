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

use common::{fixture_dirs, fixture_verdict, skip_in_mutants_sandbox, spec_dir};

/// The deep-tier gap ledger · fixture-name prefix → why the engine
/// does not implement the check yet. Closing a gap = implement + DELETE
/// the row (the suite fails on a stale row by design).
const DEEP_GAPS: &[(&str, &str)] = &[
    // 005-invalid-schema CLOSED 2026-06-16 — `analyzer::schema_lint` compiles
    // every literal infer/agent `schema:` with the runtime's exact JSON Schema
    // compiler (`jsonschema::validator_for` · the same call nika-verb-infer's
    // `structured::compile_schema` uses, workspace-pinned · parity-verified) →
    // `NIKA-PARSE-019` (generic structural · validation_error) on a meta-schema
    // violation (`type: objet`, bad `$schema` dialect, …). Row deleted per the
    // ratchet (« the suite fails on a stale row by design »).
    // 006-jq-compile-error CLOSED 2026-06-16 — `analyzer::jq_lint` compiles
    // every literal `nika:jq`/`nika:fetch` jq program with the runtime's exact
    // jaq stack (parity-pinned) → `NIKA-VAR-005` on a compile error, with a
    // clean diagnostic (not the raw jaq Debug repr · jq-3). Row deleted per
    // the ratchet (« the suite fails on a stale row by design »).
    // 013-permits-fit-violation CLOSED 2026-06-16 — `permits_fit::scan_escapes`
    // implements the static PERMITS-FIT check (NIKA-SEC-004) and the deep
    // harness now verdicts against the full `check()` surface (its
    // `capability_escapes`), so the fixture passes. Row deleted per the
    // ratchet (« the suite fails on a stale row by design »).
];

#[test]
fn deep_conformance_suite() {
    if skip_in_mutants_sandbox() {
        return;
    }
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
        let verdict = fixture_verdict(&dir, true);

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
