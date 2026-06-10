// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// NIKA_SPEC_DIR is a TEST-HARNESS path override (CI checkout layout) ·
// not a secret — the SecretStore rule targets runtime secret lookup.
#![allow(clippy::disallowed_methods)]

//! Every published spec example (`nika-spec/examples/*.nika.yaml`)
//! parses + analyzes VALID in strict mode — the examples are normative
//! showcase code; an engine that rejects them is non-conformant.

use std::path::{Path, PathBuf};

use nika_schema::{FileId, ParseMode, analyze, parse};

/// Resolve the nika-spec checkout (env override · sibling default).
fn spec_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("NIKA_SPEC_DIR") {
        return PathBuf::from(dir);
    }
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../spec")
        .canonicalize()
        .expect("nika-spec checkout missing — set NIKA_SPEC_DIR or clone ../spec")
}

#[test]
fn all_spec_examples_are_valid_strict() {
    let examples = spec_dir().join("examples");
    assert!(examples.is_dir(), "missing {}", examples.display());

    let mut walked = 0_usize;
    let mut failures: Vec<String> = Vec::new();
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&examples)
        .expect("read examples dir")
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.to_string_lossy().ends_with(".nika.yaml"))
        .collect();
    entries.sort();

    for path in entries {
        walked += 1;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let yaml = std::fs::read_to_string(&path).expect("read example");

        match parse(&yaml, FileId::new(0), ParseMode::Strict) {
            Err(e) => failures.push(format!("{name} · parse error · {} · {e}", e.spec_code())),
            Ok(wf) => {
                if let Err(errors) = analyze(&wf) {
                    let rendered = errors
                        .iter()
                        .map(|e| format!("  {} · {e}", e.spec_code()))
                        .collect::<Vec<_>>()
                        .join("\n");
                    failures.push(format!("{name} · analyze errors ·\n{rendered}"));
                }
            }
        }
    }

    assert!(walked >= 7, "only {walked} examples walked — layout drift?");
    assert!(
        failures.is_empty(),
        "{} of {walked} spec examples rejected ·\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
