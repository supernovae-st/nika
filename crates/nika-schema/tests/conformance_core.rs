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

mod common;

use common::{fixture_dirs, fixture_verdict, spec_dir};

#[test]
fn core_conformance_suite() {
    let core = spec_dir().join("conformance/tests/core");
    assert!(
        core.is_dir(),
        "conformance dir missing: {} — set NIKA_SPEC_DIR",
        core.display()
    );

    let mut failures: Vec<String> = Vec::new();
    let mut total = 0_usize;

    for dir in fixture_dirs(&core) {
        total += 1;
        let label = dir
            .strip_prefix(&core)
            .unwrap_or(&dir)
            .display()
            .to_string();
        if let Some(failure) = fixture_verdict(&dir) {
            failures.push(format!("{label} · {failure}"));
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {total} core fixtures FAILED ·\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    // Sanity floor — the suite ships 5 groups · 40+ fixtures.
    assert!(total >= 30, "only {total} fixtures walked — layout drift?");
}
