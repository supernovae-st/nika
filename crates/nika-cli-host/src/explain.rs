// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika explain NIKA-XXXX` — teach one error code (spec §2), or the
//! hint identity `nika check` printed in the same `[brackets]` slot.
//!
//! Two registries, ONE voice: the numeric crate registry
//! (`nika-error::codes` · `NIKA-440`) AND the spec's conformance codes
//! (`NIKA-DAG-002` · the canon's `error_codes` table) — every code the
//! checker can emit gets an answer here. Retired codes (`NIKA-DAG-003` ·
//! `NIKA-PARSE-016`) teach their retirement and point at the successor.
//! Hint kinds (`jq-as-map` · `native-first/006`) are the other occupant
//! of that slot (#1038) — `nika-check::hint_help` is the teaching, same
//! text the MCP tool returns. Never invents: a token in no registry is
//! a finding (`exit 2`), not a guess.

use nika_error::codes::{code_help, lookup};

use crate::display::theme::Theme;
use crate::output::VerbOutput;
use crate::probe::{seat_escape_tail, with_seat_tail};

/// The doc-site home for the error-code registry — the ONE https target the
/// explain surface names. Printed scheme-less (the established prose form);
/// the OSC-8 wrapper carries the scheme.
const DOCS_ERRORS_URL: &str = "https://docs.nika.sh/errors";
const DOCS_ERRORS_TEXT: &str = "docs.nika.sh/errors";

/// The `nika explain <code>` verb. Accepts `NIKA-440`, `NIKA-DAG-002`,
/// or the bare forms (`440` · `DAG-002`). On a TTY the doc-site
/// reference rides an OSC-8 hyperlink; a piped explain keeps its bytes.
/// The door the teaching is worded for (ADR-124 · one ladder, two
/// doors): the CLI names `nika check --fix`; the oracle names the fix
/// an agent without a shell can reach — `nika_check` with `fix: true`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Door {
    /// The operator's terminal.
    #[default]
    Cli,
    /// The MCP oracle (read-only · source in, verdict out).
    Oracle,
}

impl Door {
    fn reword(self, hint: &str) -> String {
        match self {
            Self::Cli => hint.to_owned(),
            Self::Oracle => hint.replace("`nika check --fix`", "`nika_check` with `fix: true`"),
        }
    }
}

/// The CLI door of [`run_for`].
#[must_use]
pub fn run(wire: &str, theme: Theme) -> VerbOutput {
    run_for(wire, theme, Door::Cli)
}

/// The four-rung ladder — a hint kind · the registry · the spec rows ·
/// the namespaces — worded for `door`.
#[must_use]
pub fn run_for(wire: &str, theme: Theme, door: Door) -> VerbOutput {
    // The seam (`Theme::link` → `format::osc8`): text unchanged, escapes
    // only when the links capability resolved on.
    let docs = theme.link(DOCS_ERRORS_URL, DOCS_ERRORS_TEXT);
    // A HINT row prints `kind` (or a numbered `code`) in the same
    // bracketed slot as `NIKA-PARSE-019`. Resolve that token before
    // wrapping `NIKA-` — `jq-as-map` is not `NIKA-jq-as-map` (#1038).
    if let Some(help) = nika_check::hint_help(wire) {
        return VerbOutput::ok(format!("{wire} · hint\n\n  {help}\n"));
    }
    // The resident's wire codes (`nika serve` · #1441 · ADR-131): the same
    // voice as the engine's, from the same table the MCP tool reads.
    if let Some(text) = nika_error::codes::resident_help(wire) {
        return VerbOutput::ok(text);
    }
    let normalized = if wire.starts_with("NIKA-") {
        wire.to_owned()
    } else {
        format!("NIKA-{wire}")
    };
    let Some(code) = lookup(&normalized) else {
        // Not a numeric registry code — the spec conformance codes
        // (NIKA-DAG-002 …) live in the embedded canon's error_codes
        // table. Same binary, same single source of truth.
        if let Some(text) = canon_row(&normalized, door) {
            return VerbOutput::ok(with_seat_tail(
                &normalized,
                seat_escape_tail().as_deref(),
                text,
            ));
        }
        // Retired conformance codes are ANSWERED, not 404'd: the hole in
        // the registry is deliberate (spec 05 · never reuse) and the
        // teaching survives — what the class became, and where it went.
        if let Some(text) = retired_row(&normalized) {
            return VerbOutput::ok(text);
        }
        // Runtime namespaces (per-builtin `NIKA-BUILTIN-<NAME>-<NNN>` ·
        // per-provider `NIKA-PROVIDER-<NNN>`) have no per-code registry
        // row — the shared teaching lives in `nika-error::codes` so the
        // MCP explain tool answers with the SAME text (one voice).
        if let Some(text) = nika_error::codes::namespace_help(&normalized, &docs) {
            return VerbOutput::ok(text);
        }
        return VerbOutput::file(format!(
            "unknown code `{wire}` — the registry knows NIKA-001..NIKA-9999 \
             (allocated ranges), the spec conformance codes \
             (NIKA-DAG-* · NIKA-VAR-* · …), per-builtin NIKA-BUILTIN-<NAME>-NNN \
             and per-provider NIKA-PROVIDER-NNN codes, and the hint kinds \
             `nika check` prints in [brackets]; see {docs}"
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
    VerbOutput::ok(with_seat_tail(
        &normalized,
        seat_escape_tail().as_deref(),
        text,
    ))
}

/// Teach a spec conformance code from the embedded canon's registry —
/// through THE one typed parser ([`nika_pack::error_codes`] · its
/// anchoring, malformed-row tolerance and escape-free invariant are
/// pinned at the nika-pack seam, not re-rolled here).
fn canon_row(code: &str, door: Door) -> Option<String> {
    let row = nika_pack::error_codes()
        .into_iter()
        .find(|r| r.code == code)?;
    // The contract lesson, when the code earned one (one voice: the MCP
    // explain appends the same text — `nika_error::codes::spec_contract_help`).
    let lesson = nika_error::codes::spec_contract_help(code)
        .map(|l| format!("\n{l}"))
        .unwrap_or_default();
    let fix = cli_fix_hint(code)
        .map(|h| format!("  fix: {}\n\n", door.reword(h)))
        .unwrap_or_default();
    Some(format!(
        "{code} · {category} · transient: {transient}\n\n  {failure}\n{lesson}\n{fix}\
         full docs: https://nika.sh/language/errors/{code} — {closer}\n",
        category = row.category,
        transient = row.transient,
        failure = row.failure,
        closer = closer_line(code),
    ))
}

/// The canon row's closing claim — TRUE per code class (V7-2 · wave-3:
/// four personas read « `nika check` catches this before a run ever
/// starts » under a refusal that check CANNOT catch — a computed path
/// is the run's to judge — and Marta « stopped running check at all »).
/// A teaching surface must never promise more than the judge checked.
fn closer_line(code: &str) -> &'static str {
    match code {
        // The boundary refusals: check judges the LITERAL shape (a
        // written path · a `const:`-resolved arg); a computed path (a
        // glob result · an interpolated binding · an upstream output)
        // is judged at RUN — measured: a dir-only grant checked
        // `✔ PERMITS`, then every per-item read died SEC-004.
        "NIKA-SEC-004" => {
            "`nika check` catches the LITERAL shape before a run; a \
             computed path (a glob result · an interpolated binding) is \
             judged at RUN — a green PERMITS is not its promise."
        }
        // The wire refusals: check resolves the MODEL ID in this binary
        // but never dials the server — measured (gauntlet wave-4,
        // founder-fr): a green check printed « local servers not probed »
        // itself, then the run died INFER-001 on a mute endpoint. The
        // closer must not promise the dial it never made.
        "NIKA-INFER-001" | "NIKA-INFER-003" => {
            "`nika check` resolves the model in this binary; the wire \
             itself — a live server, a valid key, a priced usage block — \
             is the RUN's verdict (`nika doctor --ping` dials ahead)."
        }
        // The exec floor (#605): check catches the literal argv (the SAME
        // predicate the run judges) — a `${{ }}` island is the RUN's verdict.
        "NIKA-SEC-001" => {
            "`nika check` catches a literal argv before a run (the same \
             floor predicate the run judges with); a templated command — \
             a `${{ }}` island — is judged at RUN."
        }
        // #1396: check's own EXEC row says « a templated argv is the
        // RUN's verdict », and an exit status can never be a check-time
        // finding — the blanket closer contradicted the card it explained.
        "NIKA-EXEC-001" => {
            "`nika check` refuses a LITERAL argv the exec floor would refuse; a \
             templated program name and a non-zero exit status are the RUN's \
             verdict — an exit status can never be a check-time finding."
        }
        _ => "`nika check` catches this before a run ever starts.",
    }
}

/// The retirement teaching for a conformance code the canon table no
/// longer carries (spec 05 · retired codes are never reused — the
/// allocation hole is deliberate). States what the class BECAME so an
/// old trace, doc or memory that names the code still gets an answer.
fn retired_row(code: &str) -> Option<String> {
    // (teaching, the LIVE successor code — its page is the one that
    // exists; a retired per-code URL is a 404, measured 2026-08-01:
    // the site's error pages project the CURRENT canon table, and a
    // retired code is exactly the row it no longer carries.)
    let (teaching, successor) = match code {
        "NIKA-DAG-003" => (
            "« a `tasks.X` reference with no declared edge » became \
             INEXPRESSIBLE in W2 « the flow »: the `with:` binding IS the \
             edge (derived, never restated), and a reference outside the \
             boundary is NIKA-VAR-021 (hoist it into `with:` — \
             `nika check --fix` applies it)",
            "NIKA-VAR-021",
        ),
        "NIKA-PARSE-016" => (
            "the jq-binding-contains-template class folded into \
             NIKA-VAR-005 at the deep-conformance registry remap",
            "NIKA-VAR-005",
        ),
        _ => return None,
    };
    Some(format!(
        "{code} · retired — never reused\n\n  {teaching}\n\n\
         full docs: https://nika.sh/language/errors/{successor} — the successor \
         code carries the live page; a retired code's own page is gone \
         with its registry row.\n"
    ))
}

/// The ENGINE-side actionable fix for a spec code, when this binary
/// ships one (the canon row states the FAILURE — per-CLI affordances
/// live here, never in the SSOT).
fn cli_fix_hint(code: &str) -> Option<&'static str> {
    match code {
        // R4: the credential refusal names the seat escape hatch (the env
        // ladder alone sent seated operators to a vendor signup).
        "NIKA-INFER-001" => Some(
            "set the key the witness names (custody is the process env: \
             `export <VAR>=…`), or run through a seat present on this machine: \
             `nika run <file> --access claude-code` — any agentic CLI you're \
             signed into serves (its login is judged when the run starts · \
             `nika doctor` lists them)",
        ),
        // F4: the unresolved-vars class is fixable from the CLI.
        "NIKA-VAR-001" => Some(
            "an unbound `inputs:` entry is supplied on the CLI — `nika run <file> \
             --var <key>=<value>` (repeatable) — or given a `default:` in its \
             `inputs:` declaration. Other roots have other fixes: `config:` and \
             `const:` resolve only from their own declared block, and `item` / \
             `index` exist only inside a `for_each` task",
        ),
        // The high-traffic conformance codes teach the one obvious YAML
        // edit concretely (#145 P1 — the fix-form lives here, never in
        // the canon row).
        "NIKA-DAG-001" => Some(
            "break the loop — one task in the cycle must drop the `with:` \
             binding or `after:` entry that closes it (a task can never \
             wait on itself, directly or through a chain)",
        ),
        "NIKA-DAG-002" => Some(
            "the `with:` binding or `after:` entry names a task that does \
             not exist — match it to a declared task key (check for a \
             typo first)",
        ),
        "NIKA-PARSE-024" => Some(
            "`depends_on` is dead since W2 — a data read becomes a `with:` \
             binding (the binding IS the edge · the body reads \
             `${{ with.<name> }}`), a pure ordering becomes `after: \
             {<task>: success}` (or `terminal` for the always-pattern); \
             `nika check --fix` migrates the provable cases",
        ),
        "NIKA-DAG-005" => Some(
            "the `after:` predicate set is closed — pick one of `success` · \
             `failure` · `skipped` · `terminal` (`nika check --fix` respells \
             the dead R5 spellings)",
        ),
        "NIKA-DAG-006" => Some(
            "the task is statically dead — an incoming edge's pass-set \
             excludes every state its producer can reach (e.g. `after: \
             {x: skipped}` on a task that can never skip), or the `when:` \
             gate is false under every reachable upstream combination; \
             loosen the predicate, give the producer the missing route, \
             or delete the dead task",
        ),
        "NIKA-DAG-007" => Some(
            "the status vocabulary is closed — compare against `success` · \
             `failure` · `skipped` · `cancelled` (the finding's \
             did-you-mean names the nearest one)",
        ),
        "NIKA-VAR-021" => Some(
            "hoist the reference into `with:` — the binding declares the \
             edge, the body reads `${{ with.<name> }}` (`nika check --fix` \
             applies it)",
        ),
        "NIKA-PARSE-002" => Some(
            "every workflow starts with `nika: <kebab-id>` and a non-empty \
             `tasks:` map (keyed by task id); description is a `#` comment \
             above `nika:`",
        ),
        "NIKA-PARSE-001" => Some(
            "the YAML itself is broken — check the pointed line for a missing \
             `:`, a stray tab (YAML forbids tabs), or unbalanced quotes; if \
             line 1 is blamed, a copier may have de-commented the \
             `# yaml-language-server:` modeline",
        ),
        "NIKA-PARSE-005" => Some(
            "the field is not in the closed envelope — check the spelling \
             against `nika spec --schema` (the did-you-mean in the finding \
             usually names it); custom prose is a `#` comment above `nika:`",
        ),
        "NIKA-PARSE-019" => Some(
            "the field's YAML SHAPE is wrong (a string where a list goes, a \
             list where a map goes) — `tasks:` is a MAP keyed by task id, \
             and the finding names the field whose shape to fix",
        ),
        "NIKA-VAR-006" => Some(
            "the expression mixes types — `when:` must be boolean-shaped, \
             `for_each:` must reference an ARRAY (a `.output` of a `schema:` \
             task typed `{ array: … }`, or a literal list), and comparisons \
             need both sides the same type",
        ),
        _ => decide_fix_hint(code).or_else(|| run_decl_fix_hint(code)),
    }
}

/// The `decide` bundle hints — split at the fn-length wall.
fn decide_fix_hint(code: &str) -> Option<&'static str> {
    match code {
        "NIKA-DECIDE-001" => Some(
            "the bundle breaks its own laws — weights/thresholds are INTEGER \
             basis-points (8735 = 87.35% · never a float), rules read only \
             evidence_schema keys, identity keys never feed technical \
             dimensions, transforms are total on min..max, and the fixtures \
             must include a `contradictory` case and respect every declared \
             monotonicity (the reference evaluator refuses identically)",
        ),
        "NIKA-DECIDE-002" => Some(
            "the snapshot must satisfy the bundle's evidence_schema — every \
             item's key declared, its value fitting the declared spec-09 \
             type, its source in the authorized list, its integrity at or \
             above the declared floor; a MISSING required key is not an \
             error (the evaluation defers — abstention is a safety property)",
        ),
        _ => None,
    }
}

/// The `run:` declaration hints (F-P3 · NEP-0010 · the dedicated
/// mints) — split out of [`cli_fix_hint`] at the fn-length wall.
fn run_decl_fix_hint(code: &str) -> Option<&'static str> {
    match code {
        "NIKA-PARSE-026" => Some(
            "`entropy: ambient` declares live entropy while `clock: virtual` \
             demands a simulated clock — drop `clock: virtual`, or name the \
             stream (`entropy: none` or `{ seeded: <u64> }`)",
        ),
        "NIKA-PARSE-027" => Some(
            "`entropy: none | seeded` forces byte-identical journals; a wall \
             clock would leak real durations into them — drop `clock: system` \
             (the virtual clock is implied) or declare `clock: virtual`",
        ),
        "NIKA-PARSE-028" => Some(
            "`entropy: none` is a strict no-entropy promise but the body \
             consumes a structural source (a live retry jitter · `nika:uuid`) \
             — set `jitter: false`, drop the uuid call, or name the stream \
             with `entropy: { seeded: <u64> }`",
        ),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::exit;

    /// The sober register (links off) — the byte-frozen baseline every
    /// machine surface reads.
    fn run(wire: &str) -> VerbOutput {
        super::run(wire, Theme::new(false, false, false))
    }

    /// R4 — the credential refusal teaches the seat escape hatch: the
    /// static fix names `--access claude-code` (the census-derived tail
    /// is gated in the host's `with_seat_tail`, pinned separately).
    #[test]
    fn infer_001_teaches_the_seat_escape() {
        let out = run("NIKA-INFER-001");
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(
            out.text.contains("--access claude-code"),
            "the seat door is taught:\n{}",
            out.text
        );
        assert!(
            out.text.contains("custody is the process env"),
            "custody is named:\n{}",
            out.text
        );
    }

    /// Mutation pins for the tail gate: only the auth-class codes earn
    /// the tail, and only when the census found a seat (inverting the
    /// match or dropping the `None` arm must redden this).
    #[test]
    fn the_seat_tail_is_gated_on_code_class_and_census_truth() {
        let text = "body".to_owned();
        let with = crate::probe::with_seat_tail(
            "NIKA-INFER-001",
            Some(
                "or use a seat present on this machine: `--access claude-code` (its login is judged at run)",
            ),
            text.clone(),
        );
        assert!(with.contains("--access claude-code"), "{with}");
        assert!(with.starts_with("body"), "{with}");
        // NIKA-1800 rides the same gate.
        assert!(
            crate::probe::with_seat_tail("NIKA-1800", Some("tail"), text.clone()).contains("tail"),
            "the admission refusal earns the tail too"
        );
        // Another class never does, even with a seat present.
        assert_eq!(
            crate::probe::with_seat_tail("NIKA-DAG-002", Some("tail"), text.clone()),
            "body"
        );
        // No seat, no tail — the teaching never promises a phantom path.
        assert_eq!(
            crate::probe::with_seat_tail("NIKA-INFER-001", None, text.clone()),
            "body"
        );
    }

    #[test]
    fn numeric_registry_codes_answer_exit_zero() {
        let out = run("NIKA-440");
        assert_eq!(out.code, exit::OK);
        assert!(out.text.contains("NIKA-440"));
    }

    /// #1396 — `explain NIKA-EXEC-001` promised what check's own EXEC
    /// row disowns. The closer now draws the literal/run line and says
    /// an exit status is never check's.
    #[test]
    fn the_exec_closer_hands_the_exit_status_to_the_run() {
        let exec = run("NIKA-EXEC-001");
        assert_eq!(exec.code, exit::OK);
        assert!(
            !exec.text.contains("catches this before a run ever starts"),
            "no blanket promise over a run-judged class:\n{}",
            exec.text
        );
        assert!(
            exec.text.contains("exit status") && exec.text.contains("RUN's verdict"),
            "the honest split is taught:\n{}",
            exec.text
        );
    }

    /// V7-2 (wave-3 · 4 personas · Priya BLOCKER): the closing claim is
    /// TRUE per code class. SEC-004's closer stops promising what check
    /// cannot judge (a computed path is the run's) — Marta read the old
    /// line under a green-check-red-run pair and « stopped running check
    /// at all ». A statically-caught class keeps the strong closer.
    #[test]
    fn the_closer_never_promises_more_than_the_judge_checked() {
        let sec = run("NIKA-SEC-004");
        assert_eq!(sec.code, exit::OK);
        assert!(
            !sec.text.contains("catches this before a run ever starts"),
            "the over-claim is gone from the run-judged class:\n{}",
            sec.text
        );
        assert!(
            sec.text.contains("judged at RUN") && sec.text.contains("not its promise"),
            "the honest split is taught:\n{}",
            sec.text
        );
        let dag = run("NIKA-DAG-002");
        assert_eq!(dag.code, exit::OK);
        assert!(
            dag.text.contains("catches this before a run ever starts"),
            "a statically-caught class keeps its strong closer:\n{}",
            dag.text
        );
        // Wave-4 founder-fr: the wire class must not promise the dial
        // check never made — the closer names the RUN's verdict and the
        // door that DOES dial ahead.
        let infer = run("NIKA-INFER-001");
        assert_eq!(infer.code, exit::OK);
        assert!(
            !infer.text.contains("catches this before a run ever starts"),
            "the over-claim is gone from the wire class:\n{}",
            infer.text
        );
        assert!(
            infer.text.contains("doctor --ping") && infer.text.contains("RUN's verdict"),
            "the honest wire split is taught:\n{}",
            infer.text
        );
        // #605: the exec floor IS statically judged now (the same
        // predicate both sides) — but only for a LITERAL argv, so the
        // closer claims exactly that and hands the templated half to RUN.
        let floor = run("NIKA-SEC-001");
        assert_eq!(floor.code, exit::OK);
        assert!(
            !floor.text.contains("catches this before a run ever starts"),
            "no blanket promise while a templated argv is run-judged:\n{}",
            floor.text
        );
        assert!(
            floor.text.contains("literal argv") && floor.text.contains("judged at RUN"),
            "the literal/templated split is taught:\n{}",
            floor.text
        );
    }

    #[test]
    fn spec_conformance_codes_answer_from_the_canon() {
        // ONE voice: every code `nika check` emits is explainable.
        let out = run("NIKA-VAR-021");
        assert_eq!(
            out.code,
            exit::OK,
            "spec codes must teach, not 404:\n{}",
            out.text
        );
        assert!(out.text.contains("NIKA-VAR-021"));
        assert!(out.text.contains("validation_error"));
        assert!(
            out.text
                .contains("https://nika.sh/language/errors/NIKA-VAR-021"),
            "the footer links the code's own page:\n{}",
            out.text
        );
        assert!(
            out.text.contains("hoist the reference into `with:`"),
            "the fix-form states the concrete YAML edit (#145 P1):\n{}",
            out.text
        );
    }

    #[test]
    fn retired_codes_teach_the_retirement_not_a_404() {
        // NIKA-DAG-003 died in W2 « the flow » (the binding IS the edge);
        // NIKA-PARSE-016 folded into NIKA-VAR-005. Old traces and docs
        // still name them — the answer is the retirement, never a guess.
        let out = run("NIKA-DAG-003");
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(out.text.contains("retired"), "{}", out.text);
        assert!(
            out.text.contains("NIKA-VAR-021"),
            "points at the successor class:\n{}",
            out.text
        );
        assert!(
            out.text.contains("the `with:` binding IS the edge"),
            "teaches the W2 law that made it inexpressible:\n{}",
            out.text
        );
        let folded = run("NIKA-PARSE-016");
        assert_eq!(folded.code, exit::OK, "{}", folded.text);
        assert!(folded.text.contains("retired"), "{}", folded.text);
        assert!(folded.text.contains("NIKA-VAR-005"), "{}", folded.text);
        // The test's own name, finally enforced: the docs door taught is
        // the SUCCESSOR's page — a retired code's per-code URL is a 404
        // (measured 2026-08-01 · the site projects the CURRENT canon
        // table, and retirement is exactly the row it no longer has).
        assert!(
            out.text.contains("errors/NIKA-VAR-021"),
            "the taught URL is the live successor page:\n{}",
            out.text
        );
        assert!(
            !out.text.contains("errors/NIKA-DAG-003"),
            "never the retired 404:\n{}",
            out.text
        );
    }

    #[test]
    fn parse_002_teaches_the_nine_key_minimum() {
        // Leftover fourteen-key teaching (`nika: v1` + `workflow:` + a
        // `tasks:` list) would send the author back to a dialect this
        // binary refuses. The minimum is two things: identity on `nika:`
        // and a non-empty `tasks:` map — never a third required key.
        let out = run("NIKA-PARSE-002");
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(
            out.text.contains("`nika: <kebab-id>`"),
            "identity lives on nika:\n{}",
            out.text
        );
        assert!(
            out.text.contains("`tasks:` map"),
            "tasks is a map, not a list:\n{}",
            out.text
        );
        assert!(
            out.text.contains("`#` comment"),
            "description is a comment, not a key:\n{}",
            out.text
        );
        assert!(
            !out.text.contains("nika: v1"),
            "the version slot is gone:\n{}",
            out.text
        );
        assert!(
            !out.text.contains("workflow: <name>") && !out.text.contains("three lines"),
            "do not revive the fourteen-key trio:\n{}",
            out.text
        );
    }

    #[test]
    fn parse_005_teaches_the_closed_envelope_not_description() {
        // `description:` died with the envelope nuke — unknown fields are
        // not parked there; custom prose is a `#` comment above `nika:`.
        let out = run("NIKA-PARSE-005");
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(
            out.text.contains("closed envelope"),
            "names the closed set:\n{}",
            out.text
        );
        assert!(
            out.text.contains("`#` comment") && out.text.contains("`nika:`"),
            "custom prose is a comment above nika:\n{}",
            out.text
        );
        assert!(
            !out.text.contains("description:"),
            "description: is dead:\n{}",
            out.text
        );
    }

    #[test]
    fn parse_024_names_both_w2_doors_and_the_codemod() {
        // The dead-form code is the highest-traffic migration surface:
        // its fix-form must name the two doors (`with:` data · `after:`
        // control) and the machine path (`nika check --fix`).
        let out = run("NIKA-PARSE-024");
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(out.text.contains("depends_on"), "{}", out.text);
        assert!(out.text.contains("`with:`"), "{}", out.text);
        assert!(out.text.contains("after:"), "{}", out.text);
        assert!(out.text.contains("nika check --fix"), "{}", out.text);
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
        let out = run("DAG-005");
        assert_eq!(out.code, exit::OK);
        assert!(out.text.contains("NIKA-DAG-005"));
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
        assert!(
            out.text.contains("[brackets]"),
            "the 404 names the other occupant of the slot:\n{}",
            out.text
        );
    }

    #[test]
    fn a_printed_hint_kind_teaches_instead_of_404() {
        // #1038 · check prints `[jq-as-map]` in the code slot; explain
        // used to wrap it as `NIKA-jq-as-map` and refuse.
        let out = run("jq-as-map");
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(out.text.starts_with("jq-as-map · hint"), "{}", out.text);
        assert!(out.text.contains("($name | map"), "{}", out.text);
        assert!(out.text.contains("paid_ready"), "{}", out.text);
    }

    #[test]
    fn a_numbered_native_first_rule_teaches() {
        let out = run("native-first/006");
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(
            out.text.starts_with("native-first/006 · hint"),
            "{}",
            out.text
        );
        assert!(out.text.contains("nika:wait"), "{}", out.text);
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
