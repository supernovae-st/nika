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

/// Teach a spec conformance code from the embedded canon's registry —
/// through THE one typed parser ([`nika_pack::error_codes`] · its
/// anchoring, malformed-row tolerance and escape-free invariant are
/// pinned at the nika-pack seam, not re-rolled here).
fn canon_row(code: &str) -> Option<String> {
    let row = nika_pack::error_codes()
        .into_iter()
        .find(|r| r.code == code)?;
    Some(format!(
        "{code} · {category} · transient: {transient}\n\n  {failure}\n\n\
         spec conformance code — emitted by `nika check`; prose home: \
         spec/05-errors.md (embedded canon `error_codes` is the SSOT row).\n",
        category = row.category,
        transient = row.transient,
        failure = row.failure,
    ))
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
        // DERIVED coverage (never a hand-enumerated list): every code
        // the typed registry carries must answer exit 0. The parse
        // contract itself (anchoring · escape-free rows · count vs the
        // canon's own field) is pinned in nika-pack's seam tests.
        let rows = nika_pack::error_codes();
        assert!(
            rows.len() >= 30,
            "registry parse broke ({} rows)",
            rows.len()
        );
        for row in rows {
            let out = run(row.code);
            assert_eq!(
                out.code,
                exit::OK,
                "{} must explain:\n{}",
                row.code,
                out.text
            );
        }
    }

    #[test]
    fn unknown_codes_stay_a_finding() {
        let out = run("NIKA-ZZZ-999");
        assert_eq!(out.code, exit::FILE);
        assert!(out.text.contains("unknown code"));
    }

    #[test]
    fn commas_inside_the_failure_text_render_intact() {
        // The parse detail lives in nika-pack now; this pins the
        // CONSUMER-visible property — VAR-002's comma-bearing failure
        // text arrives whole through the typed seam.
        let out = run("NIKA-VAR-002");
        assert_eq!(out.code, exit::OK);
        assert!(out.text.contains("zero or multiple values"), "{}", out.text);
    }
}
