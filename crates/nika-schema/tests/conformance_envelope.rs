// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]

//! Spec conformance · core/envelope fixtures.
//!
//! Runs the canonical `nika-spec` envelope conformance fixtures
//! (`nika/02-engineering/repos/spec/conformance/tests/core/envelope/`) against the
//! `nika-schema` parser. Each fixture is an `input.yaml` + an `expected.json`
//! carrying `{ "valid": bool, ... }`. The parser contract · `parse` returns
//! `Ok` ⟺ the envelope is valid, `Err` ⟺ invalid.
//!
//! Scope · the 4 LENIENT/parser-level fixtures (001-valid-minimal,
//! 002-valid-full-outputs, 003-nika-version-bad, 004-workflow-id-bad). The
//! 005-unknown-top-level-key fixture is `"mode": "strict"` per its own
//! `expected.json` note — strict-mode unknown-key rejection is the Round-3
//! analyzer's job (the parser is lenient-by-default + documented as such),
//! so it is NOT asserted here (tracked · the `TODO(round-2e)` in parser/mod.rs).
//!
//! This is the RED→GREEN gate for the envelope-canonicalization
//! (`schema:`→`nika:` · `name:`→`workflow:` · task `name:`→`id:`) admission
//! work · per `nika/02-engineering/repos/spec/spec/01-envelope.md`.

use std::path::PathBuf;

use nika_schema::{FileId, parse};

/// The 4 parser-level (lenient-mode) envelope fixtures + their expected validity.
const ENVELOPE_CASES: &[(&str, bool)] = &[
    ("001-valid-minimal", true),
    ("002-valid-full-outputs", true),
    ("003-nika-version-bad", false),
    ("004-workflow-id-bad", false),
];

/// Absolute path to the spec conformance envelope dir, resolved from the
/// crate manifest dir (portable · no hardcoded `/Users/...`).
fn envelope_dir() -> PathBuf {
    // CARGO_MANIFEST_DIR = .../nika/02-engineering/repos/engine/crates/nika-schema
    // spec lives at      = .../nika/02-engineering/repos/spec
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../spec/conformance/tests/core/envelope")
}

#[test]
#[ignore = "RED · admission-prep · pins the canonical nika:/workflow:/id: envelope \
            contract (nika-spec 01-envelope.md). The parser still reads the legacy \
            schema:/name: keys — GREEN (the envelope rename) lands in the nika-schema \
            Round-4 12-gate admission arc (mutation >=90% + 3-agent review swarm), NOT \
            here. Run explicitly · `cargo test -p nika-schema --test conformance_envelope \
            -- --ignored`."]
fn envelope_conformance_parser_level() {
    let dir = envelope_dir();
    assert!(
        dir.is_dir(),
        "spec conformance envelope dir not found at {} — check the relative path",
        dir.display()
    );

    let mut failures = Vec::new();

    for (case, expect_valid) in ENVELOPE_CASES {
        let input_path = dir.join(case).join("input.yaml");
        let yaml = std::fs::read_to_string(&input_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", input_path.display()));

        let got_valid = parse(&yaml, FileId::new(0)).is_ok();

        if got_valid != *expect_valid {
            failures.push(format!(
                "  {case} · expected valid={expect_valid} · got valid={got_valid}",
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "envelope conformance failures (parser reads canonical spec keys nika:/workflow:?):\n{}",
        failures.join("\n")
    );
}
