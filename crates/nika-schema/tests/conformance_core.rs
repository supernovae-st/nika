// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// NIKA_SPEC_DIR is a TEST-HARNESS path override (CI checkout layout) ·
// not a secret — the SecretStore rule targets runtime secret lookup.
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
//! conformance gate must never silently skip).

use std::path::{Path, PathBuf};

use nika_schema::{FileId, ParseMode, SchemaError, analyze, parse};

/// Resolve the nika-spec checkout (env override · sibling default).
fn spec_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("NIKA_SPEC_DIR") {
        return PathBuf::from(dir);
    }
    // CARGO_MANIFEST_DIR = …/02-engineering/repos/engine/crates/nika-schema
    // the spec checkout  = …/02-engineering/repos/spec
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../spec")
        .canonicalize()
        .expect("nika-spec checkout missing — set NIKA_SPEC_DIR or clone ../spec")
}

/// One expected-error entry (`code` XOR `namespace` per protocol).
#[derive(Debug, serde::Deserialize)]
struct ExpectedError {
    code: Option<String>,
    namespace: Option<String>,
    category: Option<String>,
}

/// The `expected.json` contract.
#[derive(Debug, serde::Deserialize)]
struct Expected {
    valid: bool,
    #[serde(default)]
    errors: Vec<ExpectedError>,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    note: Option<String>,
}

/// Run parse + analyze · collect every emitted error.
fn run_engine(yaml: &str, mode: ParseMode) -> Vec<SchemaError> {
    match parse(yaml, FileId::new(0), mode) {
        Ok(wf) => match analyze(&wf) {
            Ok(_) => Vec::new(),
            Err(errors) => errors,
        },
        Err(e) => vec![e],
    }
}

/// Protocol matching · exact `code` OR `namespace`-prefix + `category`.
fn matches_expected(emitted: &SchemaError, expected: &ExpectedError) -> bool {
    let spec = emitted.spec_code();
    let code = spec.to_string();
    if let Some(exact) = &expected.code {
        return &code == exact;
    }
    if let Some(namespace) = &expected.namespace {
        if !code.starts_with(&format!("{namespace}-")) {
            return false;
        }
        if let Some(category) = &expected.category {
            return spec.category.as_str() == category;
        }
        return true;
    }
    false
}

/// Collect every fixture dir (a dir containing `input.yaml`).
fn fixture_dirs(core: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let groups = std::fs::read_dir(core).expect("read conformance/tests/core");
    for group in groups {
        let group = group.expect("dir entry").path();
        if !group.is_dir() {
            continue;
        }
        for fixture in std::fs::read_dir(&group).expect("read group dir") {
            let fixture = fixture.expect("dir entry").path();
            if fixture.join("input.yaml").is_file() {
                dirs.push(fixture);
            }
        }
    }
    dirs.sort();
    assert!(
        !dirs.is_empty(),
        "zero fixtures found under {} — the conformance gate must never be empty",
        core.display()
    );
    dirs
}

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
        let yaml = std::fs::read_to_string(dir.join("input.yaml")).expect("read input.yaml");
        let expected_raw =
            std::fs::read_to_string(dir.join("expected.json")).expect("read expected.json");
        let expected: Expected = serde_json::from_str(&expected_raw).expect("parse expected.json");

        // « default: strict · the test default » per runner-protocol.md.
        let mode = match expected.mode.as_deref() {
            Some("lenient") => ParseMode::Lenient,
            _ => ParseMode::Strict,
        };

        let emitted = run_engine(&yaml, mode);

        if expected.valid {
            if !emitted.is_empty() {
                failures.push(format!(
                    "{label} · expected VALID · engine emitted {} error(s) ·\n{}",
                    emitted.len(),
                    render(&emitted),
                ));
            }
        } else if emitted.is_empty() {
            failures.push(format!(
                "{label} · expected INVALID ({}) · engine accepted{}",
                render_expected(&expected.errors),
                expected
                    .note
                    .as_deref()
                    .map(|n| format!("\n  note · {n}"))
                    .unwrap_or_default(),
            ));
        } else {
            let any_match = emitted
                .iter()
                .any(|e| expected.errors.iter().any(|x| matches_expected(e, x)));
            if !any_match {
                failures.push(format!(
                    "{label} · expected one of [{}] · engine emitted ·\n{}",
                    render_expected(&expected.errors),
                    render(&emitted),
                ));
            }
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

/// Render emitted errors with their spec codes for diagnosis.
fn render(errors: &[SchemaError]) -> String {
    errors
        .iter()
        .map(|e| {
            let spec = e.spec_code();
            format!("  {} [{}] · {e}", spec, spec.category.as_str())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Render the expected entries compactly.
fn render_expected(expected: &[ExpectedError]) -> String {
    expected
        .iter()
        .map(|x| {
            let id = x
                .code
                .clone()
                .or_else(|| x.namespace.clone())
                .unwrap_or_else(|| "<any>".to_owned());
            match &x.category {
                Some(category) => format!("{id}+{category}"),
                None => id,
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}
