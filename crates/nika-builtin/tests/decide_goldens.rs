// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// Test-harness plumbing: NIKA_SPEC_DIR is the same path override the
// schema conformance tiers read.
#![allow(clippy::disallowed_methods)]

//! The Python↔Rust decision-kernel differential (spec 11 · the
//! second-evaluator law · G18).
//!
//! Reads the COMMITTED goldens (`conformance/decision-goldens/` ·
//! printed by `conformance/decision_core.py`, the stdlib-Python
//! reference interpreter) and re-judges every snapshot through THIS
//! crate's kernel: the receipts must match byte-canonically. The
//! implementations share no code — a common bug would have to be born
//! twice. HARD-FAILS when the spec dir is missing (a silently-skipped
//! differential is the guard-blind class — the type-core precedent).
//!
//! Lives in `tests/` (not lib) BY DISCIPLINE: the lib-test push gate
//! runs with no `NIKA_SPEC_DIR` and no spec checkout — everything that
//! reads the spec repo is an integration test (the type-differential
//! convention). The hermetic law battery (mutations · Belnap ·
//! dispatcher · embedded pins) stays in `src/decide/tests.rs`.

use std::path::{Path, PathBuf};

use nika_builtin::decide::evaluate;
use serde_json::{Value, json};

fn spec_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("NIKA_SPEC_DIR") {
        return PathBuf::from(dir);
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .parent()
        .expect("engine parent")
        .join("spec")
}

fn goldens_dir() -> PathBuf {
    let dir = spec_dir().join("conformance/decision-goldens");
    assert!(
        dir.is_dir(),
        "decision goldens not found at {} — set NIKA_SPEC_DIR to the spec \
         repo root (a silently-skipped differential is the guard-blind class)",
        dir.display()
    );
    dir
}

fn read_json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{} must read — {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{} must parse — {e}", path.display()))
}

fn bundle() -> Value {
    read_json(&goldens_dir().join("pr-triage.bundle.json"))
}

/// Canonical JSON — `serde_json`'s compact form over its sorted map IS
/// the reference's `sort_keys` + `(",", ":")` + raw UTF-8 spelling.
fn canonical(v: &Value) -> String {
    serde_json::to_string(v).expect("serializes")
}

#[test]
fn goldens_are_byte_equal_with_the_reference() {
    let dir = goldens_dir();
    let bundle = bundle();
    for name in [
        "s1-dominant-risk",
        "s2-missing-required",
        "s3-straddle",
        "s4-conflict",
        "s5-cold",
    ] {
        let snapshot = read_json(&dir.join(format!("{name}.snapshot.json")));
        let receipt =
            evaluate(&bundle, &snapshot).unwrap_or_else(|e| panic!("{name} must evaluate — {e:?}"));
        let got = canonical(&receipt);
        let want = std::fs::read_to_string(dir.join(format!("{name}.receipt.golden.json")))
            .expect("golden reads");
        assert_eq!(
            got,
            want.trim_end(),
            "{name} · engine receipt is not byte-equal with decision_core.py"
        );
    }
}

#[test]
fn determinism_two_runs_byte_equal() {
    let bundle = bundle();
    let snapshot = read_json(&goldens_dir().join("s1-dominant-risk.snapshot.json"));
    let a = canonical(&evaluate(&bundle, &snapshot).expect("evaluates"));
    let b = canonical(&evaluate(&bundle, &snapshot).expect("evaluates"));
    assert_eq!(a, b);
}

#[test]
fn duplicate_snapshot_keys_score_last_wins() {
    // The reference's dict comprehension keeps the LAST claim for
    // scoring (golden s4 pins it end-to-end with authoritative
    // duplicates; this pins the sub-authoritative case against the
    // COMMITTED bundle — no conflict, still last-wins).
    let ev = |key: &str, value: Value, source: &str, digest: &str| {
        json!({
            "key": key, "value": value, "source": source,
            "observed_at": "2026-07-14T20:00:00Z", "digest": digest,
            "confidentiality": "internal", "integrity": "verified",
            "quality": { "freshness": "fresh", "completeness": "complete",
                         "independence_group": source },
        })
    };
    let snapshot = json!({
        "t": "2026-07-14T20:00:00Z",
        "evidence": [
            ev("failed_required_checks", json!(2), "ci", "d1"),
            ev("failed_required_checks", json!(7), "audit-bot", "d9"),
            ev("touches_release_workflow", json!(false), "ci", "d2"),
        ],
    });
    let receipt = evaluate(&bundle(), &snapshot).expect("evaluates");
    assert!(
        receipt["conflicts"]
            .as_array()
            .expect("conflicts")
            .is_empty()
    );
    let contribution = &receipt["dimensions"]["change_risk"]["contributions"][0];
    assert_eq!(contribution["contribution"]["lo"], 7);
}
