// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
// NIKA_SPEC_DIR / NIKA_DIFF_CLI are test-harness overrides, not secrets.
#![allow(clippy::disallowed_methods)]
// std::process::Command is banned in RUNTIME code (the kernel ShellExecutor
// trait is the seam) — leg B is a TEST harness spawning the same-tree CLI to
// diff its output, which is the one place a raw process spawn is the point.
#![allow(clippy::disallowed_types)]

//! The differential gate this crate's README owes before any public
//! surface consumes it. Two legs, two divergence classes:
//!
//! **Leg A · assembly vs library (always).** `check()` is a thin
//! assembly over `nika_schema::parse` + `nika_check::analyze`; the only
//! thing it can get wrong is the assembly itself — dropping a finding,
//! reordering them, rewording a message, losing a span. So run both
//! sides in-process over the whole conformance corpus (125 real
//! fixtures, the same files `nika-check`'s own suite judges) and demand
//! byte equality on every row. No protocol re-implementation, no
//! binary needed, runs everywhere the spec checkout exists.
//!
//! **Leg B · rows vs the CLI (`NIKA_DIFF_CLI=1`).** The CLI assembles
//! its own `--json` findings in `nika-cli`; two assemblies of the same
//! truth is the exact divergence class the check↔run oracle exists to
//! kill, so diff them: same tree (`cargo run -p nika-cli`), same
//! fixtures, rows filtered to the legs both run (PARSE · CONFORM),
//! compared on (gate, severity, code, message). Env-gated because it builds the
//! full CLI; CI wires it, a laptop run stays fast without it.

use std::path::{Path, PathBuf};

use nika_check_wasm::check;
use nika_schema::{FileId, ParseMode, SchemaError, parse};

/// Resolve the nika-spec checkout — env override, then the sibling
/// shapes this estate actually uses (flat sibling · container law).
fn spec_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("NIKA_SPEC_DIR") {
        return PathBuf::from(dir);
    }
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    for candidate in [
        "../../../spec",         // flat sibling (the harness's own default)
        "../../../spec/repo",    // container law from a worktree beside repo/
        "../../../../spec/repo", // container law from engine/<worktree>/crates/*
    ] {
        if let Ok(dir) = here.join(candidate).canonicalize()
            && dir.join("conformance/tests/core").is_dir()
        {
            return dir;
        }
    }
    panic!("nika-spec checkout missing — set NIKA_SPEC_DIR");
}

/// Every fixture dir under conformance/tests/core (category/case layout).
fn fixture_inputs() -> Vec<PathBuf> {
    let core = spec_dir().join("conformance/tests/core");
    let mut inputs = Vec::new();
    for category in std::fs::read_dir(&core).expect("core dir") {
        let category = category.expect("entry").path();
        if !category.is_dir() {
            continue;
        }
        for case in std::fs::read_dir(&category).expect("category dir") {
            let input = case.expect("entry").path().join("input.yaml");
            if input.is_file() {
                inputs.push(input);
            }
        }
    }
    inputs.sort();
    inputs
}

/// Wire `findings[].message` — `SchemaError` Display plus the explain
/// hand-off. Same format wasm `finding_row` and the CLI CONFORM JSON
/// fold emit (`SchemaDiagnostic` Display minus the `[CODE] ` prefix).
fn library_row_message(err: &SchemaError) -> String {
    format!("{err} · → nika explain {}", err.spec_code())
}

/// The library truth for one source — the exact two calls `check()` wraps.
fn library_errors(yaml: &str) -> (bool, Vec<SchemaError>) {
    match parse(yaml, FileId::new(0), ParseMode::Strict) {
        Ok(wf) => match nika_check::analyze(&wf) {
            Ok(_) => (false, Vec::new()),
            Err(errors) => (false, errors),
        },
        Err(e) => (true, vec![e]),
    }
}

#[test]
fn leg_a_the_wasm_assembly_never_diverges_from_the_library() {
    let inputs = fixture_inputs();
    assert!(
        inputs.len() >= 100,
        "corpus shrank to {} fixtures — wrong spec dir?",
        inputs.len()
    );

    for input in &inputs {
        let yaml = std::fs::read_to_string(input).expect("fixture readable");
        let (parse_fatal, lib) = library_errors(&yaml);
        let v: serde_json::Value =
            serde_json::from_str(&check(&yaml)).expect("check() emits valid JSON");
        let rows = v["findings"].as_array().expect("findings[]");
        let at = input.display();

        assert_eq!(v["clean"], lib.is_empty(), "clean disagrees at {at}");
        assert_eq!(rows.len(), lib.len(), "row count disagrees at {at}");

        for (row, err) in rows.iter().zip(&lib) {
            // the row is the error's own accessors, byte for byte —
            // except `message`, which projects `SchemaDiagnostic` minus
            // `[CODE] ` (thiserror Display plus the explain hand-off).
            assert_eq!(
                row["code"].as_str().unwrap(),
                err.spec_code().to_string(),
                "code diverges at {at}"
            );
            assert_eq!(
                row["message"].as_str().unwrap(),
                library_row_message(err),
                "message diverges at {at}"
            );
            // parse() fatal → PARSE (CLI parse_fatal_json). Analyzer
            // errors → PARSE only when the spec family is NIKA-PARSE-*.
            let expected_gate =
                if parse_fatal || err.spec_code().to_string().starts_with("NIKA-PARSE-") {
                    "PARSE"
                } else {
                    "CONFORM"
                };
            assert_eq!(row["gate"], expected_gate, "gate diverges at {at}");
            // the fields NO gate watched (Gate-11 correctness #2): a mutant
            // flipping severity, swapping the parse/conformance literals, or
            // dropping the docs_url separator passed every test in the repo
            // before these three assertions
            assert_eq!(row["severity"], "error", "severity diverges at {at}");
            assert_eq!(
                row["kind"],
                if parse_fatal || err.spec_code().to_string().starts_with("NIKA-PARSE-") {
                    "parse"
                } else {
                    "conformance"
                },
                "kind diverges at {at}"
            );
            assert_eq!(
                row["docs_url"].as_str().unwrap(),
                format!("{}/{}", nika_check::ERROR_DOCS_BASE, err.spec_code()),
                "docs_url shape diverges at {at}"
            );
            match err.span() {
                Some(span) => {
                    assert_eq!(
                        row["span"]["start"],
                        serde_json::json!(span.start),
                        "span.start diverges at {at}"
                    );
                    assert_eq!(
                        row["span"]["end"],
                        serde_json::json!(span.end),
                        "span.end diverges at {at}"
                    );
                }
                None => assert!(
                    row.get("span").is_none(),
                    "a span the library does not carry at {at}"
                ),
            }
        }
    }
}

#[test]
fn leg_b_the_wasm_rows_equal_the_cli_rows_on_the_shared_legs() {
    if std::env::var_os("NIKA_DIFF_CLI").is_none() {
        // loud-skip: the harness's own conformance suites print theirs the
        // same way (test-scope diagnostics ARE the point of a gated leg)
        #[allow(clippy::disallowed_macros, clippy::print_stderr)]
        {
            eprintln!("differential: leg B skipped (set NIKA_DIFF_CLI=1 — builds the full CLI)");
        }
        return;
    }
    let inputs = fixture_inputs();
    for input in &inputs {
        let yaml = std::fs::read_to_string(input).expect("fixture readable");
        let v: serde_json::Value =
            serde_json::from_str(&check(&yaml)).expect("check() emits valid JSON");
        let mine: Vec<(String, String, String, String)> = v["findings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                (
                    r["gate"].as_str().unwrap_or("").to_owned(),
                    r["severity"].as_str().unwrap_or("").to_owned(),
                    r["code"].as_str().unwrap_or("").to_owned(),
                    r["message"].as_str().unwrap_or("").to_owned(),
                )
            })
            .collect();

        // the SAME TREE's CLI — never the release binary, whose code may
        // legitimately postdate or predate this branch. The cargo bin
        // target is `nika-cli`; only packaging renames it `nika`.
        let out = std::process::Command::new("cargo")
            .args(["run", "-q", "-p", "nika-cli", "--bin", "nika-cli", "--"])
            .args(["check", "--json", "--"])
            .arg(input)
            .output()
            .expect("cargo run nika-cli");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let cli: serde_json::Value = serde_json::from_str(stdout.trim())
            .unwrap_or_else(|_| panic!("CLI emitted no JSON at {}", input.display()));
        let theirs: Vec<(String, String, String, String)> = cli["findings"]
            .as_array()
            .map(|rows| {
                rows.iter()
                    .filter(|r| matches!(r["gate"].as_str(), Some("PARSE" | "CONFORM")))
                    .map(|r| {
                        (
                            r["gate"].as_str().unwrap_or("").to_owned(),
                            r["severity"].as_str().unwrap_or("").to_owned(),
                            r["code"].as_str().unwrap_or("").to_owned(),
                            r["message"].as_str().unwrap_or("").to_owned(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();

        assert_eq!(mine, theirs, "rows diverge at {}", input.display());
    }
}
