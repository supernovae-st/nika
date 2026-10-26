// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]

//! Judge the independent spec's static `nika:image_compose` and keyless
//! `nika:tts_generate` (`provider: system`) cases through the composed check.
//!
//! The selection is the exact, ordered set of public case directories under
//! `conformance/tests/stdlib/builtins/` that fix the composition's static
//! contract — every input a judged read, `out` a judged write, a templated
//! list deferred, the closed argument set and the layout enum — and the
//! system voice's authority (no `permits.exec`). Each case is verdicted by
//! the shared [`common::fixture_verdict`] against its own `expected.json`;
//! every case is evaluated before the aggregate assertion.
//!
//! Static verdicts only, in process: no image is read or written and no
//! synthesizer starts.

mod common;

/// The cases, in spec order. A missing directory, input or expectation is a
/// failure, never a silent skip. (`072` — the schema's closed tool enum
/// refusing a typo — is the spec runner's schema layer, not this surface.)
const CASES: &[&str] = &[
    "071-valid-image-compose",
    "073-image-compose-read-grant-required",
    "074-image-compose-input-outside-read-boundary",
    "075-image-compose-write-grant-required",
    "076-image-compose-dynamic-inputs-deferred",
    "077-image-compose-layout-refused",
    "078-image-compose-unknown-argument-refused",
    "079-valid-tts-system-no-exec-grant",
];

#[test]
fn media_compose_static_conformance() {
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
    #[allow(clippy::disallowed_macros, clippy::print_stderr)]
    {
        for line in &lines {
            eprintln!("{line}");
        }
        eprintln!(
            "media compose static cases: {} evaluated · {} passed · {} failed",
            lines.len(),
            lines.len() - failures.len(),
            failures.len()
        );
    }
    assert_eq!(lines.len(), 8, "exactly the 8 media cases are judged");
    assert!(
        failures.is_empty(),
        "{} of {} media cases FAILED ·\n\n{}",
        failures.len(),
        lines.len(),
        failures.join("\n\n")
    );
}
