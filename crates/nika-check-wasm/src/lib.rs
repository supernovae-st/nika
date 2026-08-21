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
//! finding the binary emits, byte for byte, because both project
//! `SchemaError::diagnostic()` (not `SchemaError`'s thiserror Display).
//! The differential gate proving that equivalence is CI's wasm leg.

use nika_schema::{FileId, ParseMode, SchemaError, parse};
use wasm_bindgen::prelude::*;

fn kind_and_gate(e: &SchemaError) -> (&'static str, &'static str) {
    if e.spec_code().to_string().starts_with("NIKA-PARSE-") {
        ("parse", "PARSE")
    } else {
        ("conformance", "CONFORM")
    }
}

/// One finding, in the report's own row shape (`findings[]` of
/// `nika check --json`): kind · gate · severity · message · code ·
/// `docs_url` · span. The row is assembled from the error's OWN
/// accessors — nothing here invents a wording or a position.
///
/// Beyond the CLI's shape, a row with a span ALSO carries `line`/`col`
/// (1-based · col in CHARACTERS): the span is a byte offset, and a
/// JS consumer that slices the source by it mis-highlights silently
/// the moment a multi-byte character sits left of the caret. The line
/// arithmetic lives HERE, beside the source, in the language whose
/// `.chars()` is the same arithmetic the CLI prints.
fn finding_row(kind: &str, gate: &str, source: &str, e: &SchemaError) -> serde_json::Value {
    /* the canonical NIKA-<NAMESPACE>-<NNN> form is SpecCode's Display */
    let code = e.spec_code().to_string();
    // Same well as `SchemaDiagnostic` Display minus `[CODE] ` (the row
    // already carries `code`). CLI `--json` PARSE strips the same way
    // from `PARSE ✗  […]`; CONFORM JSON folds this exact format.
    let message = format!("{e} · → nika explain {code}");
    let mut row = serde_json::json!({
        "kind": kind,
        "gate": gate,
        "severity": "error",
        "message": message,
        "code": code,
        "docs_url": format!("{}/{code}", nika_check::ERROR_DOCS_BASE),
    });
    if let Some(span) = e.span() {
        row["span"] = serde_json::json!({ "start": span.start, "end": span.end });
        let (line, col) = line_col(source, span.start.0 as usize);
        row["line"] = serde_json::json!(line);
        row["col"] = serde_json::json!(col);
    }
    row
}

/// 1-based line and CHARACTER column for a byte offset — the CLI's own
/// arithmetic, kept beside the source so no JS consumer ever re-derives it
/// from the byte span (the two disagree the moment a multi-byte character
/// sits left of the caret).
///
/// The parser's offsets land on char boundaries; a mid-char or past-end
/// offset would be an engine bug, and clamping to the last boundary keeps
/// this a rendering aid rather than a new failure mode.
fn line_col(source: &str, byte: usize) -> (usize, usize) {
    let mut at = byte.min(source.len());
    while at > 0 && !source.is_char_boundary(at) {
        at -= 1;
    }
    let head = &source[..at];
    let line_start = head.rfind('\n').map_or(0, |i| i + 1);
    (
        head.matches('\n').count() + 1,
        head[line_start..].chars().count() + 1,
    )
}

/// The verdict payload. `legs` is the closed list of passes this build
/// actually ran — the in-band honesty marker a rendering surface must
/// carry through (the browser half never claims the binary's coverage).
///
/// Gate-accurate, with one nuance worth stating: the CONFORM leg's
/// analyzer also compile-checks embedded jq programs and JSON Schemas
/// (the deep-static tier the CLI files under the same gate), so this
/// artifact does run two compilers over the user's file — both behind
/// the same pre-flight caps the parser's other recursion doors carry.
fn verdict(clean: bool, findings: &[serde_json::Value]) -> String {
    let payload = serde_json::json!({
        "report_version": nika_check::REPORT_VERSION,
        "wasm": true,
        "legs": ["PARSE", "CONFORM"],
        "clean": clean,
        "findings": findings,
    });
    escape_for_embedding(&format!("{payload:#}"))
}

/// Defense in depth for a JSON text whose `message` fields echo attacker
/// bytes (Gate-11 security finding F3): serde escapes every C0 control, but
/// `<` `>` `&` U+2028 U+2029 are legal in JSON strings and hostile in the
/// contexts a website inlines JSON into (a `</script`> ends the element ·
/// U+2028/9 break a JS parse). These five only ever occur INSIDE string
/// literals of the emitted text, so a whole-text `\uXXXX` rewrite is
/// exactly equivalent JSON — escaping is still every consumer's duty; this
/// removes the class at the source for all of them.
fn escape_for_embedding(json: &str) -> String {
    let mut out = String::with_capacity(json.len());
    for c in json.chars() {
        match c {
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            _ => out.push(c),
        }
    }
    out
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
        // CLI `parse_fatal_json` always stamps PARSE, even when the
        // spec code is not `NIKA-PARSE-*` (DAG-005 unknown predicate).
        Err(e) => return verdict(false, &[finding_row("parse", "PARSE", source, &e)]),
    };
    match nika_check::analyze(&wf) {
        Ok(_) => verdict(true, &[]),
        Err(errors) => {
            let rows: Vec<serde_json::Value> = errors
                .iter()
                .map(|e| {
                    let (kind, gate) = kind_and_gate(e);
                    finding_row(kind, gate, source, e)
                })
                .collect();
            verdict(false, &rows)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hostile_jq_bomb_is_refused_not_a_stack_overflow() {
        // Gate-11 security F1, as a regression: 470 nested brackets — a
        // 1,047-byte paste — overflowed the 1 MiB wasm stack through jaq's
        // unlimited recursion, and a trapped instance never recovers. The
        // pre-flight depth guard refuses it as a finding instead. Depth
        // 5000 here: if the guard ever stops running first, this test does
        // not fail, it dies — which is the point.
        let bomb = format!(
            "nika: t\ntasks:\n  j:\n    invoke:\n      tool: \"nika:jq\"\n      args:\n        input: \"{{}}\"\n        expression: '{}'\n",
            "[".repeat(5000) + &"]".repeat(5000)
        );
        let v: serde_json::Value = serde_json::from_str(&check(&bomb)).unwrap();
        assert_eq!(v["clean"], false);
        let msgs: Vec<&str> = v["findings"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|f| f["message"].as_str())
            .collect();
        assert!(
            msgs.iter().any(|m| m.contains("nesting depth exceeds")),
            "the depth guard names itself: {msgs:?}"
        );
        // and the instance SURVIVES: a clean file still judges clean after
        let ok = check("nika: k\ntasks:\n  a:\n    exec:\n      command: [\"echo\"]\n");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&ok).unwrap()["clean"],
            true
        );
    }

    #[test]
    fn past_the_suggestion_budget_the_finding_still_fires_without_guessing() {
        // Gate-11 security F2: did_you_mean is O(n²·L²) across all ids —
        // 28s of sync CPU at the task cap. Past 256 candidates the
        // unresolved-ref finding keeps firing but stops guessing.
        use std::fmt::Write as _;
        let mut yaml = String::from("nika: t\ntasks:\n");
        for i in 0..300 {
            let _ = writeln!(yaml, "  t{i}:\n    exec:\n      command: [\"echo\"]");
        }
        yaml.push_str("  bad:\n    with:\n      d: ${{ tasks.t9999x.output }}\n    exec:\n      command: [\"echo\"]\n");
        let v: serde_json::Value = serde_json::from_str(&check(&yaml)).unwrap();
        let dag: Vec<&str> = v["findings"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|f| f["code"] == "NIKA-DAG-002")
            .filter_map(|f| f["message"].as_str())
            .collect();
        assert!(!dag.is_empty(), "the finding must still fire");
        assert!(
            dag.iter().all(|m| !m.contains("did you mean")),
            "past the budget it stops guessing: {dag:?}"
        );
    }

    #[test]
    fn hostile_bytes_in_identifiers_never_reach_the_text_unescaped() {
        // Gate-11 security F3: messages echo user identifiers verbatim, and
        // < > & U+2028/9 are hostile in every context a site inlines JSON
        // into. The emitted TEXT carries none of the five, ever; parsing it
        // back yields the identifier intact (escaping, not mangling).
        let hostile = "nika: t\n\"</script><img src=x>\": 1\ntasks:\n  a:\n    exec:\n      command: [\"echo\"]\n";
        let out = check(hostile);
        for needle in ["<", ">", "&", "\u{2028}", "\u{2029}"] {
            assert!(!out.contains(needle), "raw {needle:?} in the emitted text");
        }
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        let joined = v["findings"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|f| f["message"].as_str())
            .collect::<String>();
        assert!(
            joined.contains("</script>"),
            "the identifier survives the round-trip"
        );
    }

    #[test]
    fn the_version_is_the_manifest_not_a_string_someone_typed() {
        // kills the two engine_version mutants (String::new() · "xyzzy"):
        // consumers PIN this value against their captured provenance, so a
        // wrong version is a lie with a receipt attached.
        assert_eq!(engine_version(), env!("CARGO_PKG_VERSION"));
        assert!(!engine_version().is_empty());
    }

    proptest::proptest! {
        /// Gate-6 property · for ANY unicode source and ANY byte offset,
        /// line_col agrees with a naive char-walking reference and never
        /// panics — mid-char offsets, past-end offsets, empty sources,
        /// newline-at-offset included. The walker is a DIFFERENT algorithm
        /// (one pass over chars, counting) so a shared blind spot would
        /// need the same bug written twice.
        #[test]
        fn line_col_agrees_with_a_char_walker(
            source in "\\PC{0,120}",
            raw_offset in 0usize..200,
        ) {
            let (line, col) = line_col(&source, raw_offset);
            let mut at = raw_offset.min(source.len());
            while at > 0 && !source.is_char_boundary(at) { at -= 1; }
            let (mut rl, mut rc) = (1usize, 1usize);
            for c in source[..at].chars() {
                if c == '\n' { rl += 1; rc = 1; } else { rc += 1; }
            }
            proptest::prop_assert_eq!((line, col), (rl, rc));
        }
    }

    #[test]
    fn a_valid_file_is_clean_and_says_what_it_covered() {
        let out =
            check("nika: hello\ntasks:\n  greet:\n    exec:\n      command: [\"echo\", \"hi\"]\n");
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
        let message = row["message"].as_str().unwrap();
        assert!(
            message.contains("→ nika explain") && message.contains(code),
            "PARSE message must project SchemaDiagnostic, not SchemaError Display: {message}"
        );
    }

    #[test]
    fn an_analyzer_parse_code_wears_the_parse_gate() {
        let v: serde_json::Value = serde_json::from_str(&check("nika: w\n")).unwrap();
        let row = &v["findings"][0];
        assert_eq!(row["gate"], "PARSE");
        assert_eq!(row["kind"], "parse");
        assert_eq!(row["code"], "NIKA-PARSE-002");
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)] // test arithmetic on a 130-byte fixture
    fn the_column_counts_characters_never_bytes() {
        // the law this field exists for: a multi-byte char ON THE CARET'S
        // LINE, left of it, moves the byte column and must not move the
        // printed one. Finding that shape took three probes: the engine
        // anchors an unresolved-ref finding at the with-VALUE START, so a
        // block mapping can never put text left of the caret on its own
        // line — a FLOW mapping can, and the é in the first entry sits
        // exactly there. (Two earlier drafts put the é on another line or
        // under the caret; both times the test refuted its author, which
        // is the job.)
        let yaml = "nika: t\ntasks:\n  a:\n    with: { p: \"é\", d: \"${{ tasks.zz.output }}\" }\n    exec:\n      command: [\"echo\"]\n";
        let v: serde_json::Value = serde_json::from_str(&check(yaml)).unwrap();
        let row = &v["findings"][0];
        let (line, col) = (row["line"].as_u64().unwrap(), row["col"].as_u64().unwrap());
        // recompute independently in CHARS from the row's own span
        let start = row["span"]["start"].as_u64().unwrap() as usize;
        let head = &yaml[..start];
        assert_eq!(line as usize, head.matches('\n').count() + 1);
        let ls = head.rfind('\n').map_or(0, |i| i + 1);
        assert_eq!(col as usize, head[ls..].chars().count() + 1);
        // and the byte column DISAGREES on this file — the whole point
        assert_ne!(col as usize, head.len() - ls + 1, "é must split the two");
    }

    #[test]
    fn a_conformance_finding_is_the_binary_voice_verbatim() {
        // an unknown dependency — the same file the website's hero twins
        // carry, reduced: `judge` needs `dif`, which does not exist.
        let yaml = "nika: t\ntasks:\n  diff:\n    exec:\n      command: [\"git\", \"diff\"]\n  judge:\n    with:\n      d: ${{ tasks.dif.output }}\n    exec:\n      command: [\"echo\"]\n";
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
        assert!(
            msgs.iter()
                .any(|m| m.contains("→ nika explain") && m.contains("NIKA-")),
            "CONFORM message must project SchemaDiagnostic, not SchemaError Display: {msgs:?}"
        );
    }
}
