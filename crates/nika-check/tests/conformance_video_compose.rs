// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]

//! Judge the independent spec's static `nika:video_compose` cases through the
//! composed check: every media input a judged read, `out` a judged write, a
//! computed list deferred unless a side admits no path, the closed argument
//! set and segment map, the media reference, the bounds (the 200/201 segment
//! and 16384/16385 metadata boundaries included), the order law that binds
//! `exec:` only, and the trifecta that counts the call as a write sink whose
//! outputs keep the taint of what they echo.
//!
//! The selection is the exact, ordered set of public case directories
//! `080`..=`117` under `conformance/tests/stdlib/builtins/`. Each case is
//! verdicted by the shared [`common::fixture_verdict`] against its own
//! `expected.json`; `081` (a typo'd tool name the spec's schema layer
//! rejects, with no code to match) must be refused by any code. Beyond the
//! protocol's « one expected code matches », the spec oracle emits EXACTLY its
//! expected code for every case: the codes the engine emits are compared as a
//! set, and every divergence is printed and refused unless listed below with
//! its reason. Static verdicts only, in process: nothing is read, encoded or
//! written.

mod common;

use std::collections::BTreeSet;

use nika_schema::ParseMode;

/// The cases, in spec order. A missing directory, input or expectation is a
/// failure, never a silent skip.
const CASES: &[&str] = &[
    "080-valid-video-compose",
    "081-invalid-video-compose-tool-typo",
    "082-video-compose-absent-permits-refused",
    "083-video-compose-clip-outside-read-boundary",
    "084-video-compose-soundtrack-outside-read-boundary",
    "085-video-compose-out-outside-write-boundary",
    "086-video-compose-dynamic-segments-without-read-grant",
    "087-valid-video-compose-storyboard-deferred",
    "088-video-compose-media-reference-needs-sha256",
    "089-video-compose-fade-reserved",
    "090-video-compose-odd-size-refused",
    "091-video-compose-soundtrack-with-clips-refused",
    "092-video-compose-unknown-segment-key-refused",
    "093-valid-video-compose-after-fetch",
    "094-exec-ffmpeg-after-fetch-order-law",
    "095-video-compose-out-extension-refused",
    "096-video-compose-empty-segments-refused",
    "097-video-compose-segment-budget-refused",
    "098-valid-video-compose-two-hundred-segments",
    "099-video-compose-pad-with-duration-refused",
    "100-video-compose-clipless-segment-needs-duration",
    "101-video-compose-duration-out-of-range",
    "102-video-compose-pad-out-of-range",
    "103-video-compose-fps-out-of-range",
    "104-video-compose-size-out-of-range",
    "105-video-compose-timeout-out-of-range",
    "106-video-compose-scale-closed-set",
    "107-video-compose-quality-closed-set",
    "108-video-compose-audio-codec-closed-set",
    "109-video-compose-unknown-transition-refused",
    "110-video-compose-metadata-not-an-object",
    "111-video-compose-metadata-over-16-kib",
    "112-valid-video-compose-metadata-at-16-kib",
    "113-video-compose-unknown-argument-refused",
    "114-video-compose-uppercase-sha256-refused",
    "115-video-compose-short-sha256-refused",
    "116-video-compose-dynamic-out-without-write-grant",
    "117-video-compose-output-keeps-argument-taint",
];

/// The cases whose engine code set is a STRICT superset of the oracle's,
/// each with the reason. The protocol verdict above still binds them.
const EXTRA_CODES: &[(&str, &str)] = &[(
    "117-video-compose-output-keeps-argument-taint",
    "the case's `nika:notify` passes `url:`, outside notify's closed argument set, and no \
     `target:`, its required webhook URL: the catalog row refuses both (NIKA-BUILTIN-001), \
     which the oracle does not model for notify",
)];

/// The set of codes the engine emits for one case: the analyze tier plus
/// every hard-invalidating check-only code.
fn emitted(dir: &std::path::Path) -> BTreeSet<String> {
    let yaml = std::fs::read_to_string(dir.join("input.yaml")).expect("read input.yaml");
    let mut codes: BTreeSet<String> = common::run_engine_in(&yaml, ParseMode::Strict, dir)
        .iter()
        .map(|e| e.spec_code().to_string())
        .collect();
    codes.extend(
        common::check_extra(&yaml, ParseMode::Strict, dir)
            .iter()
            .map(ToString::to_string),
    );
    codes
}

/// The set of codes the case expects.
fn expected(dir: &std::path::Path) -> (bool, BTreeSet<String>) {
    let raw = std::fs::read_to_string(dir.join("expected.json")).expect("read expected.json");
    let expected: common::Expected = serde_json::from_str(&raw).expect("parse expected.json");
    let codes = expected
        .errors
        .iter()
        .filter_map(|e| e.code.clone())
        .collect();
    (expected.valid, codes)
}

#[test]
fn video_compose_static_conformance() {
    if common::skip_in_mutants_sandbox() {
        return;
    }
    let root = common::spec_dir().join("conformance/tests/stdlib/builtins");
    let mut lines = Vec::with_capacity(CASES.len());
    let mut failures = Vec::new();
    let mut exact = 0_usize;
    for case in CASES {
        let dir = root.join(case);
        let missing: Vec<&str> = ["input.yaml", "expected.json"]
            .into_iter()
            .filter(|file| !dir.join(file).is_file())
            .collect();
        if !missing.is_empty() {
            failures.push(format!("{case} · missing {}", missing.join(" + ")));
            lines.push(format!("{case} · FAIL · missing"));
            continue;
        }
        let (valid, want) = expected(&dir);
        let got = emitted(&dir);
        // The protocol verdict — or, for a case with no code to match, a refusal.
        let verdict = if !valid && want.is_empty() {
            got.is_empty()
                .then(|| "expected a refusal · the engine accepted".to_owned())
        } else {
            common::fixture_verdict(&dir, true)
        };
        if let Some(failure) = verdict {
            lines.push(format!("{case} · FAIL · {failure}"));
            failures.push(format!("{case} · {failure}"));
            continue;
        }
        if want.is_empty() || got == want {
            exact += 1;
            lines.push(format!("{case} · PASS · exact {got:?}"));
            continue;
        }
        let extra: Vec<&String> = got.difference(&want).collect();
        if let Some((_, reason)) = EXTRA_CODES.iter().find(|(name, _)| name == case)
            && got.is_superset(&want)
        {
            lines.push(format!(
                "{case} · PASS · beside {want:?} the engine adds {extra:?} ({reason})"
            ));
        } else {
            lines.push(format!("{case} · FAIL · not exact"));
            failures.push(format!(
                "{case} · the oracle emits exactly {want:?}, the engine {got:?} (extra {extra:?})"
            ));
        }
    }
    #[allow(clippy::disallowed_macros, clippy::print_stderr)]
    {
        for line in &lines {
            eprintln!("{line}");
        }
        eprintln!(
            "video compose static cases: {} evaluated · {} passed · {} exact · {} failed",
            lines.len(),
            lines.len() - failures.len(),
            exact,
            failures.len()
        );
    }
    assert_eq!(lines.len(), 38, "exactly the 38 video cases are judged");
    assert!(
        failures.is_empty(),
        "{} of {} video cases FAILED ·\n\n{}",
        failures.len(),
        lines.len(),
        failures.join("\n\n")
    );
    for (name, _) in EXTRA_CODES {
        assert!(CASES.contains(name), "a dead EXTRA_CODES row: {name}");
    }
}
