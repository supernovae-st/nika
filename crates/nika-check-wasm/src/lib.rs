// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
    )
)]

//! The static half of `nika check`, compiled to the browser.
//!
//! The spec reserved this seat before this crate existed: conformance
//! `07-conformance.md` Level 2 names « custom engines for specialized
//! environments (embedded · **WASM** · custom LLM gateway) » as a
//! conformance class. This is the checker taking that seat — the same
//! parser, the same judgment crates, the same error voice, with the
//! machine legs that a browser genuinely has (parse · conformance) and
//! none of the ones it does not (no filesystem, no model resolution, no
//! process, no clock).
//!
//! HONESTY IS STRUCTURAL HERE. The verdict names its own coverage: the
//! payload carries `wasm: true` and a closed `legs: []` list, so a
//! consumer can never mistake the browser half for the full binary —
//! the exact discipline the website already applies when it drops
//! environmental findings from captures (« they describe the machine
//! check ran against, not the file »). A finding this crate emits is a
//! finding the binary emits, byte for byte, because both are the same
//! `SchemaError` rendered by the same `Display` with the same
//! `spec_code()`. The differential gate proving that equivalence stays
//! owed and is stated in the crate README — never assumed.

use nika_schema::{FileId, ParseMode, SchemaError, parse};
use wasm_bindgen::prelude::*;

/// One finding, in the report's own row shape (`findings[]` of
/// `nika check --json`): kind · gate · severity · message · code ·
/// `docs_url` · span. The row is assembled from the error's OWN
/// accessors — nothing here invents a wording or a position.
fn finding_row(kind: &str, gate: &str, e: &SchemaError) -> serde_json::Value {
    /* the canonical NIKA-<NAMESPACE>-<NNN> form is SpecCode's Display */
    let code = e.spec_code().to_string();
    let mut row = serde_json::json!({
        "kind": kind,
        "gate": gate,
        "severity": "error",
        "message": e.to_string(),
        "code": code,
        "docs_url": format!("{}/{code}", nika_check::ERROR_DOCS_BASE),
    });
    if let Some(span) = e.span() {
        row["span"] = serde_json::json!({ "start": span.start, "end": span.end });
    }
    row
}

/// The verdict payload. `legs` is the closed list of passes this build
/// actually ran — the in-band honesty marker a rendering surface must
/// carry through (the browser half never claims the binary's coverage).
fn verdict(clean: bool, findings: &[serde_json::Value]) -> String {
    let payload = serde_json::json!({
        "report_version": nika_check::REPORT_VERSION,
        "wasm": true,
        "legs": ["PARSE", "CONFORM"],
        "clean": clean,
        "findings": findings,
    });
    format!("{payload:#}")
}

/// The engine version these findings speak for — a consumer pins it the
/// way the website pins `VERDICT_ENGINE` on its captures.
#[wasm_bindgen]
#[must_use]
pub fn engine_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// Check a workflow source. Returns the verdict as a JSON string:
/// `{ report_version, wasm: true, legs, clean, findings[] }`.
///
/// Strict mode, like the binary's default: an unknown field is a
/// finding, not a shrug — the closed contract is the product.
#[wasm_bindgen]
#[must_use]
pub fn check(source: &str) -> String {
    let wf = match parse(source, FileId::new(0), ParseMode::Strict) {
        Ok(wf) => wf,
        Err(e) => return verdict(false, &[finding_row("parse", "PARSE", &e)]),
    };
    match nika_check::analyze(&wf) {
        Ok(_) => verdict(true, &[]),
        Err(errors) => {
            let rows: Vec<serde_json::Value> = errors
                .iter()
                .map(|e| finding_row("conformance", "CONFORM", e))
                .collect();
            verdict(false, &rows)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_valid_file_is_clean_and_says_what_it_covered() {
        let out = check(
            "nika: v1\nworkflow:\n  id: hello\ntasks:\n  greet:\n    exec:\n      command: [\"echo\", \"hi\"]\n",
        );
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["clean"], true);
        assert_eq!(v["wasm"], true);
        assert_eq!(v["legs"], serde_json::json!(["PARSE", "CONFORM"]));
        assert_eq!(v["findings"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn a_parse_refusal_carries_the_spec_code_and_the_docs_door() {
        let out = check(":");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["clean"], false);
        let row = &v["findings"][0];
        assert_eq!(row["gate"], "PARSE");
        let code = row["code"].as_str().unwrap();
        assert!(code.starts_with("NIKA-"), "spec code, got {code}");
        let docs = row["docs_url"].as_str().unwrap();
        assert!(docs.ends_with(code), "docs_url ends with the code");
    }

    #[test]
    fn a_conformance_finding_is_the_binary_voice_verbatim() {
        // an unknown dependency — the same file the website's hero twins
        // carry, reduced: `judge` needs `dif`, which does not exist.
        let yaml = "nika: v1\nworkflow:\n  id: t\ntasks:\n  diff:\n    exec:\n      command: [\"git\", \"diff\"]\n  judge:\n    with:\n      d: ${{ tasks.dif.output }}\n    exec:\n      command: [\"echo\"]\n";
        let out = check(yaml);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["clean"], false);
        let msgs: Vec<&str> = v["findings"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|f| f["message"].as_str())
            .collect();
        assert!(
            msgs.iter().any(|m| m.contains("dif")),
            "names the missing task: {msgs:?}"
        );
    }
}
