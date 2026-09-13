// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// Same carve-out as gate_matrix_conformance: this suite's WHOLE JOB is
// the shipped verify surface, and NIKA_SPEC_DIR is harness plumbing.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]

//! The spec `runtime/trace` contract fixtures replayed through the REAL
//! `nika trace verify` (spec 17 · NEP-0007): each fixture's
//! `expected-verify.json` names the verdict — `clean` (OK · no finding
//! line) · `finding` (OK · the REQUIRED-witness FINDING rides) ·
//! `forged` (FILE · the chain walk breaks) · `incomplete` (the missing end is
//! named) · `refused` (the journal is rejected before a walk). Optional
//! `cost_replay` expectations pin replayed/refused/unrecorded rendering. The
//! expectation is READ from the fixture, never hand-written here — the golden
//! swap that flips 001 from `finding` to `clean` flips this suite with it.
//!
//! The spec dir resolves from `$NIKA_SPEC_DIR` or the sibling checkout
//! (`<engine>/../spec`) — the suite HARD-FAILS when missing (the
//! conformance gate must never silently skip).

use std::path::PathBuf;

fn spec_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("NIKA_SPEC_DIR") {
        return PathBuf::from(dir);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../spec")
}

#[test]
fn runtime_trace_fixtures_hold_their_verify_verdict() {
    let root = spec_dir().join("conformance/tests/runtime/trace");
    assert!(
        root.is_dir(),
        "conformance dir missing: {} — set NIKA_SPEC_DIR",
        root.display()
    );
    let mut seen = 0_usize;
    let mut entries: Vec<_> = std::fs::read_dir(&root)
        .expect("readable fixture dir")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    entries.sort();
    for dir in entries {
        let expected_path = dir.join("expected-verify.json");
        let trace = dir.join("trace.ndjson");
        if !expected_path.is_file() || !trace.is_file() {
            continue;
        }
        seen += 1;
        let expected: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&expected_path).expect("expected json"))
                .expect("valid expected-verify.json");
        let verdict = expected["verdict"].as_str().expect("a verdict string");
        let out = nika_cli::verbs::trace_verify::verify(&trace.to_string_lossy());
        let name = dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        match verdict {
            "clean" => {
                assert_eq!(out.code, 0, "{name}: clean verdict exits OK: {}", out.text);
                assert!(
                    !out.text.contains("FINDING"),
                    "{name}: clean carries no finding line: {}",
                    out.text
                );
            }
            "finding" => {
                assert_eq!(out.code, 0, "{name}: a finding never fails: {}", out.text);
                assert!(
                    out.text.contains("FINDING"),
                    "{name}: the REQUIRED-witness finding rides: {}",
                    out.text
                );
            }
            "forged" => {
                assert_eq!(
                    out.code, 2,
                    "{name}: forged is the FILE class: {}",
                    out.text
                );
            }
            "incomplete" => {
                assert_eq!(
                    out.code, 5,
                    "{name}: incomplete exits its own class (ADR-129) — never OK, never tampered: {}",
                    out.text
                );
                assert!(
                    out.text.contains("INCOMPLETE"),
                    "{name}: the reader names the missing end: {}",
                    out.text
                );
            }
            "refused" => {
                assert_eq!(
                    out.code, 2,
                    "{name}: a refusal exits nonzero, before any walk: {}",
                    out.text
                );
                assert!(
                    !out.text.contains("chain intact"),
                    "{name}: a refused journal has no walk verdict: {}",
                    out.text
                );
            }
            other => panic!("{name}: unknown fixture verdict {other}"),
        }
        assert_cost_projection(&expected, &out.text, &name);
        if let Some(items) = expected["items"].as_object() {
            assert_item_projection(&trace, &name, items);
        }
    }
    assert!(
        seen >= 7,
        "the spec runtime/trace corpus has >= 7 fixtures (saw {seen})"
    );
}

fn assert_item_projection(
    trace: &std::path::Path,
    name: &str,
    items: &serde_json::Map<String, serde_json::Value>,
) {
    let outputs = nika_cli::verbs::trace::outputs_json(&trace.to_string_lossy());
    assert_eq!(outputs.code, 0, "{name}: {}", outputs.text);
    let document: serde_json::Value =
        serde_json::from_str(&outputs.text).expect("trace outputs JSON");
    let tasks = document["tasks"].as_array().expect("task projections");
    for (id, rows) in items {
        let task = tasks.iter().find(|task| task["id"] == *id).expect("task");
        assert_eq!(&task["items"], rows, "{name}: {id} item evidence");
    }
}

fn assert_cost_projection(expected: &serde_json::Value, text: &str, name: &str) {
    if let Some(cost) = expected["cost_replay"].as_str() {
        let marker = match cost {
            "replayed" => "COST-REPLAY — the pinned pricing table is this engine's",
            "refused" => "COST-REPLAY — REFUSED",
            "unrecorded" => "COST-REPLAY — unrecorded",
            other => panic!("{name}: unknown cost_replay claim {other}"),
        };
        assert!(
            text.contains(marker),
            "{name}: cost_replay `{cost}` renders `{marker}`: {text}"
        );
    }
}
