// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]

//! Judge the independent spec's static `nika:remove_file` cases through the
//! composed check.
//!
//! The selection is the exact, ordered set of public case directories under
//! `conformance/tests/stdlib/builtins/` that fix the removal's static
//! contract: authority, argument shape and same-path ordering. Each case is
//! verdicted by the shared [`common::fixture_verdict`] (parse + analyze plus
//! every hard-invalidating check code) against its own `expected.json`; this
//! file adds no parser, projection or filter. Every case is evaluated before
//! the aggregate assertion, so one failure never hides another.
//!
//! Static verdicts only, in process: nothing is removed and no workflow,
//! MCP transport or provider runs.

mod common;

/// The removal's static cases, in spec order. A missing directory, input or
/// expectation is a failure, never a silent skip.
const CASES: &[&str] = &[
    "048-valid-remove-file-write-only",
    "049-remove-file-tool-grant-required",
    "050-remove-file-write-grant-required",
    "051-remove-file-outside-write-boundary",
    "052-remove-file-path-required",
    "053-remove-file-path-type-refused",
    "054-remove-file-unknown-argument-refused",
    "055-remove-file-empty-path-refused",
    "056-remove-file-directory-spelling-refused",
    "057-remove-file-ordered-mutations",
    "058-remove-file-unordered-write-remove",
    "059-remove-file-unordered-removes",
    "060-remove-file-final-dot-refused",
    "061-remove-file-dynamic-path-deferred",
    "062-remove-file-constant-fan-refused",
    "063-remove-file-quoted-reference-not-order",
    "064-remove-file-typed-const-write-escape",
    "065-remove-file-typed-const-literal-collision",
    "066-remove-file-input-default-literal-collision",
    "067-remove-file-unwind-is-not-order",
    "068-remove-file-template-separator-deferred",
    "069-remove-file-template-dot-deferred",
    "070-remove-file-typed-const-write-contained",
];

#[test]
fn file_removal_static_conformance() {
    if common::skip_in_mutants_sandbox() {
        return;
    }
    let root = common::spec_dir().join("conformance/tests/stdlib/builtins");
    let mut lines = Vec::with_capacity(CASES.len());
    let mut failures = Vec::new();
    for case in CASES {
        let dir = root.join(case);
        let missing: Vec<&str> = ["input.yaml", "expected.json"]
            .into_iter()
            .filter(|file| !dir.join(file).is_file())
            .collect();
        let verdict = if missing.is_empty() {
            common::fixture_verdict(&dir, true)
        } else {
            Some(format!("missing {}", missing.join(" + ")))
        };
        match verdict {
            None => lines.push(format!("{case} · PASS")),
            Some(failure) => {
                lines.push(format!("{case} · FAIL · {failure}"));
                failures.push(format!("{case} · {failure}"));
            }
        }
    }
    // The per-case verdicts ARE the evidence of this target: print them all,
    // with the measured total, before the aggregate assertion.
    #[allow(clippy::disallowed_macros, clippy::print_stderr)]
    {
        for line in &lines {
            eprintln!("{line}");
        }
        eprintln!(
            "file removal static cases: {} evaluated · {} passed · {} failed",
            lines.len(),
            lines.len() - failures.len(),
            failures.len()
        );
    }
    assert_eq!(lines.len(), 23, "exactly the 23 removal cases are judged");
    assert!(
        failures.is_empty(),
        "{} of {} removal cases FAILED ·\n\n{}",
        failures.len(),
        lines.len(),
        failures.join("\n\n")
    );
}
