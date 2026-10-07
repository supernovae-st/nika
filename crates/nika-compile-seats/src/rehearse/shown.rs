// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A trial run as a judge is shown it: the JSON observation of a completed run of one candidate
//! ([`trial_shown`]: `candidate_sha256`, each input the run read as `{path, text, read_whole}`
//! and each output it read back as `{path, text, written, read_whole}`), whether it proves
//! whole outputs ([`trial_whole`]), and the receipts a question over it keeps, never the texts
//! again ([`trial_receipts`]). Descended from `nika-compile-cognition`'s verifier and rehearsal
//! journal at its crate-size cap (2026-10-07, ADR-146): the semantics are unchanged.

use nika_compile::surface::sha256;
use nika_compile_fidelity::behavior::{Coverage, Run};
use serde_json::{Value, json};

/// What a completed `run` of the candidate whose sha256 is `candidate_sha256` read and wrote, as
/// a judge may read it: each text with whether it was read whole and whether the run wrote it.
/// The caller decides whether the run is one a judge may be shown (completed, vouched for, of
/// these exact bytes).
#[must_use]
pub fn trial_shown(candidate_sha256: &str, run: &Run) -> Value {
    let inputs: Vec<Value> = (run.consumed.iter())
        .map(|read| {
            json!({"path": read.path, "text": read.text,
                "read_whole": read.coverage == Coverage::Complete})
        })
        .collect();
    let outputs: Vec<Value> = (run.read_back.iter())
        .map(|written| {
            json!({"path": written.path, "text": written.text, "written": written.written,
                "read_whole": !written.truncated})
        })
        .collect();
    json!({"candidate_sha256": candidate_sha256, "inputs": inputs, "outputs": outputs})
}

/// Whether a trial run proves whole outputs: at least one output, each written by the run
/// itself, and every input and output read whole. A skipped or unwritten output proves nothing
/// of the part that asked it.
#[must_use]
pub fn trial_whole(observed: &Value) -> bool {
    let texts = |key: &str| observed[key].as_array().map_or(&[][..], Vec::as_slice);
    let (inputs, outputs) = (texts("inputs"), texts("outputs"));
    !outputs.is_empty()
        && outputs
            .iter()
            .all(|output| output["written"] == Value::Bool(true))
        && (inputs.iter().chain(outputs)).all(|text| text["read_whole"] == Value::Bool(true))
}

/// What a trial-run question transmitted, text by text: the role, the path, the size and the
/// digest of each text, and whether it was whole — never the text again.
#[must_use]
pub fn trial_receipts(observed: &Value) -> Value {
    let texts = |role: &str, key: &str| -> Vec<Value> {
        (observed[key]
            .as_array()
            .map_or(&[][..], Vec::as_slice)
            .iter())
        .map(|text| {
            let body = text["text"].as_str().unwrap_or_default();
            json!({"role": role, "path": text["path"], "bytes": body.len(),
                    "sha256": sha256(body), "read_whole": text["read_whole"],
                    "written": text.get("written")})
        })
        .collect()
    };
    let mut all = texts("input", "inputs");
    all.extend(texts("output", "outputs"));
    json!({
        "candidate_sha256": observed["candidate_sha256"],
        "sha256": sha256(&observed.to_string()),
        "texts": all,
    })
}
