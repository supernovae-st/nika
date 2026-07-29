// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// NIKA_SPEC_DIR is a TEST-HARNESS path override (CI checkout layout) ·
// not a secret — the SecretStore rule targets runtime secret lookup.
#![allow(clippy::disallowed_methods)]
// The differential drives the reference Python decoder by COMMAND (the
// Bowtie harness pattern — the check-wasm differential's own idiom),
// never by linkage: `std::process::Command` is the seam here.
#![allow(clippy::disallowed_types)]
// Skip diagnostics print to stderr directly (the differential's own
// output channel · the tracing stack is not wired in this harness).
#![allow(clippy::disallowed_macros, clippy::print_stderr)]

//! NEP-0012 law 4 · the DIFFERENTIAL TWIN: the reference Python decoder
//! (`proof_core.py`, checked out via `NIKA_SPEC_DIR`) and the engine
//! decoder (`decode_untrusted_json`) MUST render the same verdict class
//! over every artifact of the golden corpus — a divergence is a spec
//! bug by definition, never a shrug. Skips gracefully when python3 or
//! the spec checkout is absent (the conformance discipline).

use std::path::{Path, PathBuf};
use std::process::Command;

use nika_dap::bounded::{DecodeRefusal, decode_untrusted_json};

/// The verdict class spelling BOTH evaluators share (the python twin
/// prints exactly these).
fn rust_class(raw: &str) -> String {
    match decode_untrusted_json(raw) {
        Ok(_) => "admit".to_owned(),
        Err(DecodeRefusal::Oversized { .. }) => "Oversized".to_owned(),
        Err(DecodeRefusal::TooDeep { .. }) => "TooDeep".to_owned(),
        Err(DecodeRefusal::Malformed { .. }) => "Malformed".to_owned(),
        Err(DecodeRefusal::ProofFlood { .. }) => "ProofFlood".to_owned(),
        Err(DecodeRefusal::IdOverflow { .. }) => "IdOverflow".to_owned(),
        // A variant newer than this map: the twin cannot produce it, so
        // the differential MUST diverge loudly until both sides learn it.
        Err(other) => format!("UNMAPPED:{other}"),
    }
}

fn spec_proof_core() -> Option<PathBuf> {
    let dir = std::env::var("NIKA_SPEC_DIR").ok()?;
    let candidate = Path::new(&dir).join("conformance/proof_core.py");
    candidate.is_file().then_some(candidate)
}

fn python_class(proof_core: &Path, file: &Path) -> String {
    let out = Command::new("python3")
        .arg(proof_core)
        .arg("decode")
        .arg(file)
        .output()
        .expect("python3 runs the reference decoder");
    assert!(
        out.status.success(),
        "the reference decoder exits clean on {}",
        file.display()
    );
    String::from_utf8(out.stdout)
        .expect("verdict class is utf-8")
        .trim()
        .to_owned()
}

#[test]
fn rust_and_python_render_the_same_verdict_over_the_corpus() {
    let Some(proof_core) = spec_proof_core() else {
        eprintln!("skip · NIKA_SPEC_DIR/conformance/proof_core.py absent");
        return;
    };
    if Command::new("python3").arg("--version").output().is_err() {
        eprintln!("skip · python3 absent");
        return;
    }
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/receipts");
    let mut cases: Vec<PathBuf> = Vec::new();
    cases.push(corpus.join("golden.json"));
    for entry in std::fs::read_dir(corpus.join("malicious")).expect("the corpus exists") {
        cases.push(entry.expect("dir entry").path());
    }
    cases.sort();
    assert!(cases.len() >= 8, "the corpus carries its classes");
    for case in &cases {
        let raw = std::fs::read_to_string(case).expect("corpus file reads");
        let rust = rust_class(&raw);
        let python = python_class(&proof_core, case);
        assert_eq!(
            rust,
            python,
            "DIVERGENCE on {} · rust={rust} python={python} (a spec bug by definition)",
            case.display()
        );
    }
}
