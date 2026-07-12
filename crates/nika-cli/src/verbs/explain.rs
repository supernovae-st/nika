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

use crate::display::theme::Theme;
use crate::verbs::VerbOutput;

/// The doc-site home for the error-code registry — the ONE https target the
/// explain surface names. Printed scheme-less (the established prose form);
/// the OSC-8 wrapper carries the scheme.
const DOCS_ERRORS_URL: &str = "https://docs.nika.sh/errors";
const DOCS_ERRORS_TEXT: &str = "docs.nika.sh/errors";

/// The `nika explain <code>` verb. Accepts `NIKA-440`, `NIKA-DAG-003`,
/// or the bare forms (`440` · `DAG-003`). The theme comes from the global
/// `--color`/`--hyperlink` chain: on a TTY the doc-site reference rides an
/// OSC-8 hyperlink; a piped explain keeps its exact bytes.
#[must_use]
pub fn run(wire: &str, theme: Theme) -> VerbOutput {
    // The seam (`Theme::link` → `format::osc8`): text unchanged, escapes
    // only when the links capability resolved on.
    let docs = theme.link(DOCS_ERRORS_URL, DOCS_ERRORS_TEXT);
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
        // Per-builtin runtime codes (`NIKA-BUILTIN-<NAME>-<NNN>`) are emitted
        // by builtins at runtime and ARE valid in `on_codes:` — but they are
        // runtime diagnostics, not spec-conformance rows, so the canon table
        // does not carry each one. Recognize the namespace and teach what it
        // IS (better than a flat "unknown code"), pointing to the contract.
        if let Some(name) = builtin_code_name(&normalized) {
            return VerbOutput::ok(format!(
                "{normalized} · builtin · runtime error from the `nika:{name}` builtin\n\n  \
                 A per-builtin runtime diagnostic (each builtin owns \
                 NIKA-BUILTIN-<NAME>-001..099). Valid in `retry.on_codes:` and \
                 `on_error.on_codes:`. The specific cause is the builtin's own \
                 arg/runtime contract — see spec stdlib (builtins) · \
                 {docs}.\n"
            ));
        }
        // Per-provider runtime codes (`NIKA-PROVIDER-<NNN>`) — 001-099 are
        // allocated PER PROVIDER (spec 05-errors.md §NIKA-PROVIDER) and ARE
        // valid in `on_codes:`. The meaning is provider-defined, so (like the
        // per-builtin namespace) teach what it IS, not a flat "unknown code".
        if is_provider_code(&normalized) {
            return VerbOutput::ok(format!(
                "{normalized} · provider · a provider-adapter runtime error\n\n  \
                 A per-provider diagnostic (each provider adapter owns \
                 NIKA-PROVIDER-001..099). The specific cause is provider-defined \
                 (transport · quota · auth · response shape from that provider). \
                 Valid in `retry.on_codes:` and `on_error.on_codes:` — see \
                 spec/05-errors.md §NIKA-PROVIDER · {docs}.\n"
            ));
        }
        return VerbOutput::file(format!(
            "unknown code `{wire}` — the registry knows NIKA-001..NIKA-9999 \
             (allocated ranges), the spec conformance codes \
             (NIKA-DAG-* · NIKA-VAR-* · …), per-builtin NIKA-BUILTIN-<NAME>-NNN \
             and per-provider NIKA-PROVIDER-NNN codes; see {docs}"
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
    let fix = cli_fix_hint(code)
        .map(|h| format!("  fix: {h}\n\n"))
        .unwrap_or_default();
    Some(format!(
        "{code} · {category} · transient: {transient}\n\n  {failure}\n\n{fix}\
         full docs: https://nika.sh/errors/{code} — `nika check` catches \
         this before a run ever starts.\n",
        category = row.category,
        transient = row.transient,
        failure = row.failure,
    ))
}

/// The ENGINE-side actionable fix for a spec code, when this binary
/// ships one (the canon row states the FAILURE — the SSOT never carries
/// per-CLI affordances, so the flag lives here with the flag itself).
fn cli_fix_hint(code: &str) -> Option<&'static str> {
    match code {
        // F4: the unresolved-vars class is fixable from the CLI.
        "NIKA-VAR-001" => Some(
            "an unbound workflow var is supplied on the CLI — `nika run <file> \
             --var <key>=<value>` (repeatable) — or given a `default:` in the \
             workflow `vars:` block",
        ),
        // The high-traffic conformance codes whose fix is one obvious
        // YAML edit teach it concretely (#145 P1 — the failure states
        // WHAT, this states the edit; the canon row never carries
        // per-CLI affordances, so the fix-form lives here).
        "NIKA-DAG-001" => Some(
            "break the loop — one task in the cycle must drop its \
             `depends_on` on the other (a task can never wait on itself, \
             directly or through a chain)",
        ),
        "NIKA-DAG-002" => Some(
            "the `depends_on:` entry names a task that does not exist — \
             match it to a declared task `id:` (check for a typo first)",
        ),
        "NIKA-DAG-003" => Some(
            "declare the edge the reference implies — add `depends_on: \
             [<that task>]` to the task whose template reads \
             `tasks.<that task>.output`",
        ),
        "NIKA-PARSE-002" => Some(
            "every workflow starts with three lines — `nika: v1`, \
             `workflow: <name>`, and a non-empty `tasks:` list",
        ),
        "NIKA-PARSE-001" => Some(
            "the YAML itself is broken — check the pointed line for a missing \
             `:`, a stray tab (YAML forbids tabs), or unbalanced quotes; if \
             line 1 is blamed, a copier may have de-commented the \
             `# yaml-language-server:` modeline",
        ),
        "NIKA-PARSE-005" => Some(
            "the field is not part of the closed v1 envelope — check the \
             spelling against `nika spec --schema` (the did-you-mean in the finding \
             usually names it); custom metadata belongs in `description:`",
        ),
        "NIKA-PARSE-019" => Some(
            "the field's YAML SHAPE is wrong (a string where a list goes, a \
             list where a map goes) — `tasks:` is a LIST of `- id:` entries, \
             and the finding names the field whose shape to fix",
        ),
        "NIKA-VAR-006" => Some(
            "the expression mixes types — `when:` must be boolean-shaped, \
             `for_each:` must reference an ARRAY (a `.output` of a `schema:` \
             task with `type: array`, or a literal list), and comparisons \
             need both sides the same type",
        ),
        _ => None,
    }
}

/// Parse a per-builtin runtime code `NIKA-BUILTIN-<NAME>-<NNN>` to the builtin
/// name (`write` · `fetch` · `json_merge_patch` · …), or `None` if the wire is
/// not that shape. The generic `NIKA-BUILTIN-001` (no name) is a canon row and
/// is resolved before this is reached.
fn builtin_code_name(code: &str) -> Option<String> {
    let rest = code.strip_prefix("NIKA-BUILTIN-")?;
    let (name, num) = rest.rsplit_once('-')?;
    (num.len() == 3 && num.bytes().all(|b| b.is_ascii_digit()) && !name.is_empty())
        .then(|| name.to_ascii_lowercase())
}

/// Recognize a per-provider runtime code `NIKA-PROVIDER-<NNN>` (001-099 are
/// allocated PER PROVIDER per spec 05-errors.md). There is no single canon
/// row — the meaning is provider-defined — so explain teaches the namespace
/// rather than 404-ing a code that is valid in `on_codes:`.
fn is_provider_code(code: &str) -> bool {
    code.strip_prefix("NIKA-PROVIDER-")
        .is_some_and(|num| num.len() == 3 && num.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verbs::exit;

    /// The sober register (links off) — the byte-frozen baseline every
    /// machine surface reads.
    fn run(wire: &str) -> VerbOutput {
        super::run(wire, Theme::new(false, false, false))
    }

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
        assert!(
            out.text.contains("https://nika.sh/errors/NIKA-DAG-003"),
            "the footer links the code's own page:\n{}",
            out.text
        );
        assert!(
            out.text.contains("depends_on"),
            "the fix-form states the concrete YAML edit (#145 P1):\n{}",
            out.text
        );
    }

    #[test]
    fn var001_teaches_the_var_flag() {
        // F4: `nika explain NIKA-VAR-001` must say HOW to supply a var
        // from the CLI, not just that the reference is unresolved.
        let out = run("NIKA-VAR-001");
        assert_eq!(out.code, exit::OK);
        assert!(out.text.contains("--var"), "names the flag:\n{}", out.text);
        assert!(
            out.text.contains("default:"),
            "names the workflow-side fix too:\n{}",
            out.text
        );
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
    fn provider_namespace_codes_teach_not_404() {
        // A per-provider code (NIKA-PROVIDER-NNN · valid in on_codes:) must
        // EXPLAIN its namespace — symmetric with per-builtin — not flat-404.
        let out = run("NIKA-PROVIDER-001");
        assert_eq!(
            out.code,
            exit::OK,
            "provider namespace must teach:\n{}",
            out.text
        );
        assert!(out.text.contains("provider"));
        assert!(out.text.contains("on_codes"));
        // a non-conforming shape (not 3 digits) stays unknown
        let bad = run("NIKA-PROVIDER-1");
        assert_eq!(bad.code, exit::FILE, "{}", bad.text);
    }

    #[test]
    fn builtin_namespace_codes_teach_not_404() {
        // NEW-3a: the per-builtin runtime code the nika:write null-guard
        // emits must EXPLAIN (builtin name + on_codes usability), not the
        // flat "unknown code" — ONE voice for every emitted code.
        let out = run("NIKA-BUILTIN-WRITE-001");
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(out.text.contains("nika:write"), "{}", out.text);
        assert!(out.text.contains("on_codes"), "{}", out.text);
        // underscore-named builtins normalize too (json_merge_patch).
        assert!(
            run("NIKA-BUILTIN-JSON_MERGE_PATCH-001")
                .text
                .contains("nika:json_merge_patch")
        );
        // a malformed builtin code (empty name) stays a finding.
        assert_eq!(run("NIKA-BUILTIN--001").code, exit::FILE);
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

    /// On the linked register the doc-site reference rides the OSC-8
    /// wrapper (scheme in the URL · prose text unchanged); the sober
    /// register — every pipe — keeps its exact bytes, zero escapes.
    #[test]
    fn doc_site_reference_links_on_the_linked_register() {
        let mut linked = Theme::new(false, false, false);
        linked.links = true;
        for wire in [
            "NIKA-BUILTIN-WRITE-001",
            "NIKA-PROVIDER-001",
            "NIKA-ZZZ-999",
        ] {
            let out = super::run(wire, linked);
            assert!(
                out.text.contains(
                    "\x1b]8;;https://docs.nika.sh/errors\x1b\\docs.nika.sh/errors\x1b]8;;\x1b\\"
                ),
                "{wire} links the doc site: {:?}",
                out.text
            );
            let sober = run(wire);
            assert!(
                !sober.text.contains('\x1b'),
                "{wire} sober register is escape-free"
            );
        }
    }
}
