// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika explain NIKA-XXXX` — teach one error code (spec §2).
//!
//! Two registries, ONE voice: the numeric crate registry
//! (`nika-error::codes` · `NIKA-440`) AND the spec's conformance codes
//! (`NIKA-DAG-003` · the canon's `error_codes` table) — every code the
//! checker can emit gets an answer here. Never invents: a code in
//! neither registry is a finding (`exit 2`), not a guess.

use nika_error::codes::{code_help, lookup};

use crate::verbs::VerbOutput;

/// The `nika explain <code>` verb. Accepts `NIKA-440`, `NIKA-DAG-003`,
/// or the bare forms (`440` · `DAG-003`).
#[must_use]
pub fn run(wire: &str) -> VerbOutput {
    let normalized = if wire.starts_with("NIKA-") {
        wire.to_owned()
    } else {
        format!("NIKA-{wire}")
    };
    let Some(code) = lookup(&normalized) else {
        // Not a numeric registry code — the spec conformance codes
        // (NIKA-DAG-003 …) live in the embedded canon's error_codes
        // table. Same binary, same single source of truth.
        if let Some(text) = canon_row(&normalized) {
            return VerbOutput::ok(text);
        }
        return VerbOutput::file(format!(
            "unknown code `{wire}` — the registry knows NIKA-001..NIKA-9999 \
             (allocated ranges) and the spec conformance codes \
             (NIKA-DAG-* · NIKA-VAR-* · …); see docs.nika.sh/errors"
        ));
    };
    // The category/severity labels are the OWNING crate's canonical
    // kebab-case (`Category::as_str`), not a `Debug` derive an enum rename
    // could silently change — one source of truth, compile-forced complete.
    let text = format!(
        "{code} · {category} · {severity} · {slug}\n\n  {help}\n",
        category = code.category.as_str(),
        severity = code.severity.as_str(),
        slug = code.slug,
        help = code_help(code),
    );
    VerbOutput::ok(text)
}

/// Teach a spec conformance code from the embedded canon's
/// `error_codes` flow-style rows:
///
/// ```yaml
/// - { code: NIKA-DAG-003, category: validation_error, transient: "false", failure: "…" }
/// ```
///
/// The scan is ANCHORED to the `error_codes:` section (a same-shaped
/// row in any other canon table is never served as an error code), and
/// a malformed row is skipped, never fatal — codes in later rows stay
/// reachable. The failure text is the LAST field and quoted — commas
/// inside it are safe under this anchored parse. Hand-rolled (no regex
/// dep): the rows are projector-emitted, shape-stable, and the
/// derived-coverage test walks every one of them.
fn canon_row(code: &str) -> Option<String> {
    let mut in_section = false;
    for line in nika_pack::canon().lines() {
        if line.starts_with("error_codes:") {
            in_section = true;
            continue;
        }
        if in_section && !line.trim().is_empty() && !line.starts_with([' ', '#']) {
            break; // next top-level key — the section is closed
        }
        if !in_section {
            continue;
        }
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("- { code:") else {
            continue;
        };
        let Some((row_code, tail)) = rest.split_once(',') else {
            continue; // malformed row — skip, never abort the scan
        };
        if row_code.trim() != code {
            continue;
        }
        let category = bare_field(tail, "category:")?;
        let transient = quoted_field(tail, "transient:")?;
        let failure = quoted_field(tail, "failure:")?;
        return Some(format!(
            "{code} · {category} · transient: {transient}\n\n  {failure}\n\n\
             spec conformance code — emitted by `nika check`; prose home: \
             spec/05-errors.md (embedded canon `error_codes` is the SSOT row).\n",
        ));
    }
    None
}

/// `key: value` where the value runs to the next `,` or `}` (unquoted).
fn bare_field<'a>(tail: &'a str, key: &str) -> Option<&'a str> {
    let start = tail.find(key)? + key.len();
    let rest = &tail[start..];
    let end = rest.find([',', '}']).unwrap_or(rest.len());
    let value = rest[..end].trim();
    (!value.is_empty()).then_some(value)
}

/// `key: "value"` — the quoted form (commas inside stay intact).
fn quoted_field<'a>(tail: &'a str, key: &str) -> Option<&'a str> {
    let start = tail.find(key)? + key.len();
    let rest = tail[start..].trim_start();
    let inner = rest.strip_prefix('"')?;
    let end = inner.find('"')?;
    Some(&inner[..end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verbs::exit;

    #[test]
    fn numeric_registry_codes_answer_exit_zero() {
        let out = run("NIKA-440");
        assert_eq!(out.code, exit::OK);
        assert!(out.text.contains("NIKA-440"));
    }

    #[test]
    fn spec_conformance_codes_answer_from_the_canon() {
        // ONE voice: every code `nika check` emits is explainable.
        let out = run("NIKA-DAG-003");
        assert_eq!(
            out.code,
            exit::OK,
            "spec codes must teach, not 404:\n{}",
            out.text
        );
        assert!(out.text.contains("NIKA-DAG-003"));
        assert!(out.text.contains("validation_error"));
        assert!(out.text.contains("spec/05-errors.md"));
    }

    #[test]
    fn bare_prefixed_form_normalizes() {
        let out = run("DAG-003");
        assert_eq!(out.code, exit::OK);
        assert!(out.text.contains("NIKA-DAG-003"));
    }

    #[test]
    fn every_canon_table_row_explains() {
        // DERIVED coverage (never a hand-enumerated list): every code the
        // canon's error_codes table carries must answer exit 0, and the
        // anchored parse's standing assumption — rows stay escape-free —
        // is asserted here at the projector seam (an escaped quote would
        // silently truncate the failure text otherwise).
        let mut covered = 0usize;
        for line in nika_pack::canon().lines() {
            let trimmed = line.trim_start();
            let Some(rest) = trimmed.strip_prefix("- { code:") else {
                continue;
            };
            assert!(
                !line.contains('\\'),
                "canon error_codes rows must stay escape-free for the anchored parse: {line}"
            );
            let Some((code, _)) = rest.split_once(',') else {
                continue;
            };
            let code = code.trim();
            let out = run(code);
            assert_eq!(out.code, exit::OK, "{code} must explain:\n{}", out.text);
            covered += 1;
        }
        // ≥ the v0.1 floor — the parse-shape drifting to zero rows must
        // fail loudly, not pass vacuously.
        assert!(
            covered >= 30,
            "canon error_codes parse broke ({covered} rows)"
        );
    }

    #[test]
    fn unknown_codes_stay_a_finding() {
        let out = run("NIKA-ZZZ-999");
        assert_eq!(out.code, exit::FILE);
        assert!(out.text.contains("unknown code"));
    }

    #[test]
    fn quoted_field_keeps_commas_inside_the_failure_text() {
        let tail = r#" category: variable_error, transient: "false", failure: "zero, or multiple, values" }"#;
        assert_eq!(
            quoted_field(tail, "failure:"),
            Some("zero, or multiple, values")
        );
        assert_eq!(bare_field(tail, "category:"), Some("variable_error"));
    }
}
