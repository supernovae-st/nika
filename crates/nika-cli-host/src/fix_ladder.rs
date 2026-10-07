// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `check --fix` repair ladder — the dead-form arms and the splice
//! machinery, descended from `nika-cli::verbs::fix` to the host plane
//! (ADR-110 · the 15k wall: one architectural unit, two members — the
//! ladder is pure compute over (source, report); the verb in `nika-cli`
//! keeps the I/O and the final `check` verdict).
//!
//! One round's contract: parse aborts at the first defect, so each
//! parse-level repair lands one per round; check-level typed suggestions
//! splice in the same pass; re-parse + re-check until a round applies
//! nothing (capped by [`MAX_ROUNDS`]). The dead-form arms carry the
//! flag-day migrations — W1 « the map » · W2 « the flow » · C2 « the
//! E-split » · R5 « the predicates » · D1 « the split » (#572).
//!
//! SAFETY over reach — a repair is applied ONLY when the suggestion is
//! TYPED (never regex-scraped from a human message), the old token
//! occurs EXACTLY ONCE as a whole word (ambiguity skips with an honest
//! note), and the file re-parses after (convergence IS the proof).

use nika_migrate::{has_bare_exec, has_needs_key, rewrite_needs, wrap_bare_exec};
use nika_schema::SchemaError;

// Preserve the host's existing report paths; all repair decisions stay in this module.
pub use nika_display::repair_render::{
    Refusal, Repair, StopNotes, render_refusals, render_stops, summary,
};

/// Judge one round's transformation: `Some(refusal)` when `after` fails
/// to load as YAML, including duplicate keys, while `before` did not — it broke
/// the document and must be rolled back. A document that was already
/// unparsable stays the author's (the loop cannot repair what it cannot
/// read; the arms never run on it). Pure: no I/O, the caller rolls back.
#[must_use]
pub fn judge_round(before: &str, after: &str, attempted: Vec<String>) -> Option<Refusal> {
    let yaml_broken = |text: &str| match nika_schema::parse(
        text,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    ) {
        Err(
            SchemaError::YamlSyntax { message, .. } | SchemaError::DuplicateKey { message, .. },
        ) => Some(message),
        _ => None,
    };
    if yaml_broken(before).is_some() {
        return None;
    }
    yaml_broken(after).map(|reason| Refusal { attempted, reason })
}

/// Rounds cap — parse aborts at the first defect, so each parse-level
/// repair costs one round; this bounds pathological inputs.
pub const MAX_ROUNDS: usize = 16;

/// One round's DEAD-FORM arm — W1 · W2 · C2 · R5 · D1. `Some(true)` =
/// applied (the round restarts — the re-parse is the proof) ·
/// `Some(false)` = STOP or nothing mechanical · `None` = not dead-form.
pub fn apply_dead_form_arm(
    err: &SchemaError,
    source: &mut String,
    repairs: &mut Vec<Repair>,
    stop_notes: &mut StopNotes,
) -> Option<bool> {
    match err {
        // W1 « the map » dead forms (PARSE-022/023): ONE structural
        // repair — the shared migration (comment-preserving ·
        // idempotent). The old form is repairable, never executable.
        //
        // PARSE-020/021 left this arm with the envelope nuke
        // (2026-08-12): their teachings pointed at the `workflow:`
        // object, and repairing a file INTO a form the parser now
        // refuses would be a fix that breaks its own output. The
        // `workflow:`/`description:` keys refuse as UNKNOWN keys now and
        // ride the R1 identity arm below (equivalence-or-stop: the id
        // moves onto `nika:` only when the answer is forced).
        SchemaError::W1TasksSequence { .. } | SchemaError::W1TaskIdField { .. } => {
            Some(apply_w1_map(source, repairs))
        }
        // R1 « the identity » (the nine-key envelope · 2026-08-12): a
        // top-level `workflow:` block (or a bare `description:`) refuses
        // as an unknown envelope key. The codemod moves `workflow.id`
        // onto `nika:` and demotes the prose to a `#` comment ABOVE it —
        // never dropped — and STOPS (never guesses) when `nika:` already
        // names something else, when the block carries a foreign key,
        // when the id is not kebab-case, or when there is no id at all.
        SchemaError::UnknownField {
            field, location, ..
        } if (field == "workflow" || field == "description") && location.contains("envelope") => {
            Some(apply_identity(source, repairs, stop_notes))
        }
        // LOT 3 · the task-body rungs (2026-08-11's sweep inside a task):
        // `output:` → `extract:` (R3) · `on_error.fail_workflow: true`
        // deleted (R4) · task-level `max_parallel`/`fail_fast` INTO the
        // `for_each:` block (R2) · `declassify:`/`inert:` → one `lift:`
        // (R5). Equivalence-or-stop like every rung: `fail_workflow: false`,
        // knobs with no fan-out, a flow-style for_each, a declassify that
        // does not lift to `trusted` — each STOPS with its note.
        SchemaError::UnknownField {
            field, location, ..
        } if (location.starts_with("task `")
            && matches!(
                field.as_str(),
                "output" | "declassify" | "inert" | "max_parallel" | "fail_fast"
            ))
            || (location == "`on_error:`" && field == "fail_workflow") =>
        {
            Some(apply_lot3(source, repairs, stop_notes))
        }
        SchemaError::UnknownField {
            field, location, ..
        } if matches!(
            (location.as_str(), field.as_str()),
            ("`invoke:`", "params") | ("`exec:`", "argv")
        ) =>
        {
            Some(apply_verb_dialect(source, repairs, stop_notes))
        }
        // The parser uses Validation for the retired scalar for_each.
        // The codemod discovers grammar structure; it never scrapes prose.
        SchemaError::Validation { .. } => Some(apply_lot3(source, repairs, stop_notes)),
        // W2 « the flow » dead form (PARSE-024) — the equivalence-or-
        // stop migration (spec 03 §depends_on): data → with: bindings ·
        // provably-strict control → after: {d: success} · every
        // ambiguous case STOPS with its candidates.
        SchemaError::W2DependsOnField { .. } => Some(apply_w2_flow(source, repairs, stop_notes)),
        // C2 « the E-split » dead forms (VALUES-001/002): the `vars:`
        // block is classified into `inputs:`/`const:` by the codemod
        // (classify-not-rename · never a bulk rename). `env:` has NO
        // mechanical repair — re-shaping a flat string map into typed
        // `config:` declarations is a human classification (the teaching
        // names it; the spec codemod carries config=0 for the same
        // reason) · a form this binary predates joins it.
        SchemaError::DeadValueForm {
            form: nika_schema::error::DeadForm::Vars,
            ..
        } => Some(apply_esplit(source, repairs, stop_notes)),
        SchemaError::DeadValueForm { .. } => Some(false),
        // R5 « the predicates » (DAG-005): the 1:1 respelling — a
        // genuinely-unknown predicate (`passed`) has no mechanical
        // repair (the codemod returns Clean · the teaching stands).
        SchemaError::UnknownAfterPredicate { .. } => Some(apply_predicates(source, repairs)),
        // D1 « the split » (the PARSE-019 string command · #572): the
        // 0.102 implicit shell migrates — `shell:` verbatim, or the
        // argv flow form for provably-inert tokens.
        SchemaError::D1StringCommand { .. } => Some(apply_d1_split(source, repairs, stop_notes)),
        _ => None,
    }
}

/// This round's typed renames (tools · args · conformance refs), deduped.
///
/// The derivation lives in `nika_check` so the `check` footer decides
/// whether to OFFER `--fix` from the same answer this loop APPLIES
/// (#1177) — a second copy here is how the offer drifted from the work.
#[must_use]
pub fn collect_typed_renames(
    report: &nika_check::CheckReport,
) -> Vec<(String, String, &'static str)> {
    nika_check::typed_renames(report)
}

/// The R1 identity arm — `nika: v1` + `workflow: {id, description}` (or
/// the scalar / flow forms · a bare `description:`) become `nika: <id>`
/// with the prose demoted to a `#` comment. `true` = applied (the round
/// restarts) · `false` = STOP (each note names the case) or Clean.
fn apply_identity(
    source: &mut String,
    repairs: &mut Vec<Repair>,
    stop_notes: &mut StopNotes,
) -> bool {
    match nika_migrate::identity(source) {
        nika_migrate::IdentityOutcome::Changed(migrated) => {
            *source = migrated;
            repairs.push(Repair::applied(
                "the retired envelope identity (id + description)",
                "nika: <id> · the description as a # comment above it",
                "r1-identity",
            ));
            true
        }
        nika_migrate::IdentityOutcome::Stop(notes) => {
            stop_notes.0 = notes;
            false
        }
        nika_migrate::IdentityOutcome::Clean => false,
    }
}

/// The LOT 3 task-body arm — R2 · R3 · R4 · R5 in one pass. `true` =
/// applied (the round restarts) · `false` = STOP (each note names the
/// case) or Clean.
fn apply_lot3(source: &mut String, repairs: &mut Vec<Repair>, stop_notes: &mut StopNotes) -> bool {
    match nika_migrate::lot3(source) {
        nika_migrate::Lot3Outcome::Changed {
            source: migrated,
            applied,
        } => {
            *source = migrated;
            for rung in applied {
                let (from, to) = match rung {
                    "invoke-args" => ("invoke.params:", "invoke.args:"),
                    "exec-command" => ("exec.argv:", "exec.command: [arguments]"),
                    "for-each-items" => ("for_each: collection", "for_each: { items: collection }"),
                    "r3-extract" => ("output:", "extract:"),
                    "r4-fail-workflow" => {
                        ("on_error.fail_workflow: true", "the default IS the failure")
                    }
                    "r2-for-each" => (
                        "task-level max_parallel / fail_fast",
                        "inside the for_each: block",
                    ),
                    "r5-lift" => ("declassify: / inert:", "lift: [{law, from?, because}]"),
                    _ => ("a retired task form", "its nine-key shape"),
                };
                repairs.push(Repair::applied(from, to, rung));
            }
            true
        }
        nika_migrate::Lot3Outcome::Stop(notes) => {
            stop_notes.0 = notes;
            false
        }
        nika_migrate::Lot3Outcome::Clean => false,
    }
}

// Unknown verb fields name the limits of this structural codemod when
// no supported context was found. Canonical unvisited maps stay clean.
fn apply_verb_dialect(
    source: &mut String,
    repairs: &mut Vec<Repair>,
    stop_notes: &mut StopNotes,
) -> bool {
    let applied = apply_lot3(source, repairs, stop_notes);
    if !applied && stop_notes.0.is_empty() {
        stop_notes.0.push("the verb mapping is outside the proved block/single-line-flow repair shapes — write args: or command: [...] explicitly; unvisited contexts are unchanged".to_owned());
    }
    applied
}

/// The W1 dead-form arm — the shared map migration. `true` = applied.
fn apply_w1_map(source: &mut String, repairs: &mut Vec<Repair>) -> bool {
    match nika_migrate::w1(source) {
        Some(migrated) => {
            *source = migrated;
            repairs.push(Repair::applied(
                "the pre-W1 tasks list",
                "tasks: map keyed by task id",
                "w1-map",
            ));
            true
        }
        None => false,
    }
}

/// The PARSE-024 arm — the whole-document W2 migration. `true` =
/// applied (the round restarts) · `false` = STOP diagnostics captured.
fn apply_w2_flow(
    source: &mut String,
    repairs: &mut Vec<Repair>,
    stop_notes: &mut StopNotes,
) -> bool {
    match nika_migrate::w2(source) {
        nika_migrate::W2Outcome::Changed(migrated) => {
            *source = migrated;
            repairs.push(Repair::applied(
                "the pre-W2 flow (depends_on · body tasks.* reads)",
                "with: bindings + after: predicates",
                "w2-flow",
            ));
            true
        }
        nika_migrate::W2Outcome::Stop(notes) => {
            stop_notes.0 = notes;
            false
        }
    }
}

/// The PARSE-019 string-command arm — the D1 codemod (#572). `true` =
/// applied (the round restarts) · `false` = STOP diagnostics captured.
fn apply_d1_split(
    source: &mut String,
    repairs: &mut Vec<Repair>,
    stop_notes: &mut StopNotes,
) -> bool {
    match nika_migrate::d1(source) {
        nika_migrate::D1Outcome::Changed(migrated) => {
            *source = migrated;
            repairs.push(Repair::applied(
                "the pre-0.103 string command (implicit shell)",
                "shell: verbatim · argv flow for inert tokens",
                "d1-split",
            ));
            true
        }
        nika_migrate::D1Outcome::Stop(notes) => {
            stop_notes.0 = notes;
            false
        }
    }
}

/// The VALUES-001 arm — the C2 E-split codemod. `true` = applied (the
/// round restarts) · `false` = STOP diagnostics captured (or nothing to
/// classify — the check teaching stands). The codemod's left-alone refs
/// ride as advisory notes (the author decides).
fn apply_esplit(
    source: &mut String,
    repairs: &mut Vec<Repair>,
    stop_notes: &mut StopNotes,
) -> bool {
    match nika_migrate::esplit(source) {
        nika_migrate::EsplitOutcome::Changed(migrated, notes) => {
            *source = migrated;
            for note in notes {
                repairs.push(Repair::applied(
                    &note,
                    "advisory — the author decides",
                    "c2-esplit-note",
                ));
            }
            repairs.push(Repair::applied(
                "the dead `vars:` block",
                "`inputs:` / `const:` by classification · refs rewritten class-aware",
                "c2-esplit",
            ));
            true
        }
        nika_migrate::EsplitOutcome::Stop(notes) => {
            stop_notes.0 = notes;
            false
        }
        // Clean (nothing to classify — the check teaching stands) ·
        // #[non_exhaustive] — a future outcome joins deliberately (the
        // forward-compat wildcard · never a silent swallow of a new case).
        _ => false,
    }
}

/// The DAG-005 arm — the R5 predicate respelling (`succeeded` →
/// `success` · `failed` → `failure`). `true` = applied.
fn apply_predicates(source: &mut String, repairs: &mut Vec<Repair>) -> bool {
    let Some(migrated) = nika_migrate::predicates(source) else {
        return false;
    };
    *source = migrated;
    repairs.push(Repair::applied(
        "the dead predicate spellings (succeeded · failed)",
        "success · failure in after: blocks",
        "r5-predicates",
    ));
    true
}

/// The VAR-021 hoist arm (a `tasks.*` read outside the boundary): the
/// whole-document W2 migration answers it (the bindings it emits ARE
/// the hoist). `Some(true)` = applied · `Some(false)` = STOP · `None`
/// = no VAR-021 on the report (not this arm's round).
pub fn try_w2_hoist(
    report: &nika_check::CheckReport,
    source: &mut String,
    repairs: &mut Vec<Repair>,
    stop_notes: &mut StopNotes,
) -> Option<bool> {
    if !report.conformance.iter().any(|v| v.code == "NIKA-VAR-021") {
        return None;
    }
    match nika_migrate::w2(source) {
        nika_migrate::W2Outcome::Changed(migrated) => {
            *source = migrated;
            repairs.push(Repair::applied(
                "body tasks.* reads",
                "with: bindings (hoisted)",
                "w2-hoist",
            ));
            Some(true)
        }
        nika_migrate::W2Outcome::Stop(notes) => {
            stop_notes.0 = notes;
            Some(false)
        }
    }
}

/// Splice `old` → `new` when `old` occurs EXACTLY ONCE in `source` as a
/// whole word — the byte surgery rides the shared
/// [`nika_migrate::repair`] door; this wrapper keeps the CLI's repair
/// bookkeeping (retry-upgrade rows). Returns whether it applied.
pub fn splice(
    source: &mut String,
    old: &str,
    new: &str,
    kind: &'static str,
    repairs: &mut Vec<Repair>,
) -> bool {
    // An APPLIED token never re-applies. A SKIPPED one stays retryable:
    // an earlier round's splice can make it unique (the two-site case —
    // `buidl` in `after:` is ambiguous while a qualified `tasks.buidl`
    // reference exists; once the reference heals, the control-edge token
    // stands alone and the next round heals it too). Convergence, not
    // one-shot.
    if repairs
        .iter()
        .any(|r| r.applied && r.old == old && r.kind == kind)
    {
        return false;
    }
    let applied = nika_migrate::repair::splice_unique(source, old, new);
    // One log row per (old, kind): a retry that succeeds UPGRADES its
    // earlier skip row (the summary reports final outcomes, not rounds).
    if let Some(row) = repairs.iter_mut().find(|r| r.old == old && r.kind == kind) {
        row.applied = row.applied || applied;
        new.clone_into(&mut row.new);
    } else {
        repairs.push(Repair {
            old: old.to_owned(),
            new: new.to_owned(),
            kind,
            applied,
        });
    }
    applied
}

/// C13 · B14: wrap a bare `exec:` scalar and rewrite a simple `needs:`
/// list, or STOP with why. Runs once before the ladder so parse can see
/// a mapping / an `after:` edge — on BOTH doors (`nika check --fix` and
/// the oracle's `fix: true` · ADR-124: one ladder, two doors).
pub fn apply_prepass(source: &mut String, repairs: &mut Vec<Repair>, stop_notes: &mut StopNotes) {
    if let Some(next) = wrap_bare_exec(source) {
        *source = next;
        repairs.push(Repair::applied(
            "bare exec: string",
            "command: argv or shell: mapping",
            "bare-exec",
        ));
    } else if has_bare_exec(source) {
        stop_notes.0.push(
            "bare `exec:` string must be a YAML mapping — write `command: [\"prog\", …]` \
             or `shell: \"…\"`"
                .to_owned(),
        );
    }
    if let Some(next) = rewrite_needs(source) {
        *source = next;
        repairs.push(Repair::applied(
            "needs:",
            "after: { id: success }",
            "needs-after",
        ));
    }
    if has_needs_key(source) {
        stop_notes.0.push(
            "`needs:` is a foreign dialect key — --fix will not guess `with:` data \
             vs `after:` order; rewrite to `after: { task: success }` for order, \
             or a `with:` binding for data"
                .to_owned(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_map_repair_teaches_only_the_task_shape_it_changes() {
        let mut source = "nika: c14\ntasks:\n  - id: a\n    exec: { command: [true] }\n".to_owned();
        let mut repairs = Vec::new();
        assert!(apply_w1_map(&mut source, &mut repairs));
        assert_eq!(repairs[0].new, "tasks: map keyed by task id");
        assert!(!repairs[0].old.contains("workflow"));
        assert!(source.contains("  a:"));
    }

    /// The filed #905 corruption: `--fix` nested `description: |` one level
    /// deeper and left the block body at the old indent, so YAML died
    /// (`simple key expect ':'`). The round judge must refuse that write.
    #[test]
    fn judge_round_refuses_the_underindented_block_scalar() {
        let before = "workflow: demo\ndescription: |\n  the first line\n  the second line\ntasks:\n  - id: t\n    run: echo hi\n";
        let after = "workflow:\n  id: demo\n  description: |\n  the first line\n  the second line\ntasks:\n  t:\n    run: echo hi\n";
        let refusal = judge_round(before, after, vec!["w1-map `envelope` → `map`".to_owned()])
            .expect("the under-indented block scalar is broken YAML");
        assert!(
            !refusal.reason.is_empty(),
            "the YAML error rides the refusal"
        );
        assert_eq!(
            refusal.attempted,
            vec!["w1-map `envelope` → `map`".to_owned()]
        );
    }

    #[test]
    fn judge_round_refuses_a_new_duplicate_key_even_after_a_valid_yaml_rename() {
        let before = "nika: w\ntasks:\n  a:\n    invoke: {tool: nika:log, params: {}, args: {}}\n";
        let after = before.replace("params:", "args:");
        assert!(matches!(
            nika_schema::parse(
                before,
                nika_schema::FileId::new(0),
                nika_schema::ParseMode::Strict
            ),
            Err(SchemaError::UnknownField { .. })
        ));
        assert!(matches!(
            nika_schema::parse(
                &after,
                nika_schema::FileId::new(0),
                nika_schema::ParseMode::Strict
            ),
            Err(SchemaError::DuplicateKey { .. })
        ));
        assert!(judge_round(before, &after, vec!["params → args".to_owned()]).is_some());
    }

    #[test]
    fn generic_validation_does_not_rewrite_scalar_task_payloads_or_canonical_flow() {
        for source in [
            "nika: w\ntasks: |\n  a:\n    invoke:\n      params: {message: keep}\n",
            "nika: w\ntasks:\n  a: |\n    invoke:\n      params: {message: keep}\n",
            "nika: w\ntasks: {a: {exec: {command: []}}}\n",
        ] {
            let error = nika_schema::parse(
                source,
                nika_schema::FileId::new(0),
                nika_schema::ParseMode::Strict,
            )
            .expect_err("invalid document");
            assert!(matches!(error, SchemaError::Validation { .. }), "{error:?}");
            let mut actual = source.to_owned();
            let mut repairs = Vec::new();
            let mut stops = StopNotes(Vec::new());
            assert_ne!(
                apply_dead_form_arm(&error, &mut actual, &mut repairs, &mut stops),
                Some(true)
            );
            assert_eq!(actual, source);
            assert!(repairs.is_empty() && stops.0.is_empty());
        }
    }

    #[test]
    fn judge_round_commits_a_document_that_still_parses() {
        let before = "nika: w\ntasks:\n  t:\n    exec: { command: [\"true\"] }\n";
        let after = "nika: w\ntasks:\n  task:\n    exec: { command: [\"true\"] }\n";
        assert!(
            judge_round(before, after, vec!["field `t` → `task`".to_owned()]).is_none(),
            "a still-YAML document is not a refusal"
        );
    }

    #[test]
    fn judge_round_does_not_judge_an_already_unparsable_source() {
        let broken = "nika: [unclosed\n";
        assert!(
            judge_round(broken, "also: [broken\n", vec!["x → y".to_owned()]).is_none(),
            "the loop cannot repair what it cannot read"
        );
    }

    /// The prepass (ADR-124 · one ladder, two doors): a bare `exec:`
    /// scalar becomes the argv mapping.
    #[test]
    fn the_prepass_wraps_a_bare_exec_scalar_into_argv() {
        let mut source = "nika: w\ntasks:\n  t:\n    exec: echo hi\n".to_owned();
        let mut repairs = Vec::new();
        let mut stops = StopNotes(Vec::new());
        apply_prepass(&mut source, &mut repairs, &mut stops);
        assert_eq!(
            source,
            "nika: w\ntasks:\n  t:\n    exec:\n      command: [\"echo\", \"hi\"]\n"
        );
        assert_eq!(repairs.len(), 1, "{repairs:?}");
        assert!(
            repairs[0].applied && repairs[0].kind == "bare-exec",
            "{repairs:?}"
        );
        assert!(stops.0.is_empty(), "{stops:?}");
    }

    /// A metacharacter line takes the explicit `shell:` door, never argv.
    #[test]
    fn the_prepass_routes_a_metacharacter_line_to_shell() {
        let mut source = "nika: w\ntasks:\n  t:\n    exec: ls | wc -l\n".to_owned();
        let mut repairs = Vec::new();
        let mut stops = StopNotes(Vec::new());
        apply_prepass(&mut source, &mut repairs, &mut stops);
        assert!(source.contains("shell: \"ls | wc -l\""), "{source}");
        assert!(stops.0.is_empty(), "{stops:?}");
    }

    /// A simple `needs:` list becomes the `after:` control edge.
    #[test]
    fn the_prepass_rewrites_a_simple_needs_list_into_after() {
        let mut source = "nika: w\ntasks:\n  a:\n    exec: { command: [\"true\"] }\n  b:\n    needs: [a]\n    exec: { command: [\"true\"] }\n".to_owned();
        let mut repairs = Vec::new();
        let mut stops = StopNotes(Vec::new());
        apply_prepass(&mut source, &mut repairs, &mut stops);
        assert!(source.contains("after: { a: success }"), "{source}");
        assert!(!source.contains("needs:"), "{source}");
        assert_eq!(repairs.len(), 1, "{repairs:?}");
        assert!(repairs[0].kind == "needs-after", "{repairs:?}");
        assert!(stops.0.is_empty(), "{stops:?}");
    }

    /// A `needs:` form the prepass will not guess STOPS with why — no
    /// repair row, one note.
    #[test]
    fn the_prepass_stops_on_a_needs_form_it_will_not_guess() {
        let mut source =
            "nika: w\ntasks:\n  b:\n    needs: a\n    exec: { command: [\"true\"] }\n".to_owned();
        let mut repairs = Vec::new();
        let mut stops = StopNotes(Vec::new());
        apply_prepass(&mut source, &mut repairs, &mut stops);
        assert!(repairs.is_empty(), "{repairs:?}");
        assert_eq!(stops.0.len(), 1, "{stops:?}");
        assert!(stops.0[0].contains("needs:"), "{stops:?}");
    }

    // ── W2 · the PARSE-024 promise vs. the W2 fixer, shape by shape ──
    //
    // `NIKA-PARSE-024` carries `provable` (nika-schema · read off the
    // PARSED sequence) and this ladder arms `nika_migrate::w2()` (a raw
    // LINE scanner). Two readers, one claim — so the table below runs
    // BOTH on the same bytes. They disagreed twice before it existed:
    // `["a"]` was provable and the fixer answered « [S7] malformed …
    // rewrite by hand » (the promise and the refusal on one screen), and
    // `[]` was NOT provable while `--fix` silently dropped the dead line.

    /// What the real W2 pass did with the whole document.
    #[derive(Debug, PartialEq, Eq, Clone, Copy)]
    enum Verdict {
        /// Migrated — an `after:` block, or a dead line dropped.
        Changed,
        /// `[S7] … malformed depends_on entries` — the shape the scanner
        /// refuses to read. This is the ONLY answer a non-provable shape
        /// may get.
        Malformed,
        /// A SEMANTIC stop on a shape the scanner DID read (S1 skippable
        /// producer · S3 status-only · S4 armor): `provable` is necessary,
        /// never sufficient.
        SemanticStop,
        /// Any other stop — never expected; the tail names it.
        OtherStop,
    }

    /// A workflow whose task `c` carries the `depends_on:` under test.
    fn w2_doc(deps: &str) -> String {
        format!(
            "nika: t\ntasks:\n  a:\n    exec: {{ command: [\"true\"] }}\n  b:\n    exec: {{ command: [\"true\"] }}\n  c:\n{deps}    exec: {{ command: [\"true\"] }}\n"
        )
    }

    /// What the FINDING promises about this document's shape.
    fn finding_is_provable(source: &str) -> bool {
        let err = nika_schema::parse(
            source,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect_err("the dead form is refused at parse");
        match err {
            SchemaError::W2DependsOnField { provable, .. } => provable,
            other => unreachable!("PARSE-024 is the finding: {other:?}"),
        }
    }

    /// What the FIXER answers for the same bytes (+ a tail for the failure).
    fn fixer_verdict(source: &str) -> (Verdict, String) {
        match nika_migrate::w2(source) {
            nika_migrate::W2Outcome::Changed(migrated) => (Verdict::Changed, migrated),
            nika_migrate::W2Outcome::Stop(notes) => {
                let tail = notes.join(" | ");
                let verdict = if tail.contains("malformed depends_on entries") {
                    Verdict::Malformed
                } else if tail.contains("[S1]") || tail.contains("[S3]") || tail.contains("[S4]") {
                    Verdict::SemanticStop
                } else {
                    Verdict::OtherStop
                };
                (verdict, tail)
            }
        }
    }

    /// The shape table — one row per `depends_on:` shape, both readers.
    #[test]
    fn the_parse_024_promise_and_the_w2_fixer_agree_shape_by_shape() {
        for (shape, deps, promise, answer) in [
            ("[a]", "    depends_on: [a]\n", true, Verdict::Changed),
            ("[a, b]", "    depends_on: [a, b]\n", true, Verdict::Changed),
            (
                "[\"a\"]",
                "    depends_on: [\"a\"]\n",
                true,
                Verdict::Changed,
            ),
            ("['a']", "    depends_on: ['a']\n", true, Verdict::Changed),
            (
                "[a] # note",
                "    depends_on: [a] # note\n",
                true,
                Verdict::Changed,
            ),
            (
                "block - a",
                "    depends_on:\n      - a\n",
                true,
                Verdict::Changed,
            ),
            (
                "block - \"a\"",
                "    depends_on:\n      - \"a\"\n",
                true,
                Verdict::Changed,
            ),
            ("[] (empty)", "    depends_on: []\n", true, Verdict::Changed),
            ("[a, 1]", "    depends_on: [a, 1]\n", true, Verdict::Changed),
            (
                "a (scalar)",
                "    depends_on: a\n",
                false,
                Verdict::Malformed,
            ),
            (
                "{a: success} (map)",
                "    depends_on: { a: success }\n",
                false,
                Verdict::Malformed,
            ),
            (
                "[\"a -> b\"] (expression)",
                "    depends_on: [\"a -> b\"]\n",
                false,
                Verdict::Malformed,
            ),
            (
                "[a, \"${{ x }}\"]",
                "    depends_on: [a, \"${{ x }}\"]\n",
                false,
                Verdict::Malformed,
            ),
            (
                "[[a]] (nested)",
                "    depends_on: [[a]]\n",
                false,
                Verdict::Malformed,
            ),
        ] {
            let source = w2_doc(deps);
            let promised = finding_is_provable(&source);
            let (answered, tail) = fixer_verdict(&source);
            assert_eq!(promised, promise, "{shape}: the promise moved\n{source}");
            assert_eq!(answered, answer, "{shape}: the answer moved — {tail}");
            // THE invariant: the shape clause is spoken iff the scanner
            // read the shape (a semantic stop is still a READ shape).
            assert_eq!(
                promised,
                answered != Verdict::Malformed,
                "{shape}: provable={promised} while the fixer answered \
                 {answered:?} — {tail}"
            );
        }
    }

    /// `provable` is NECESSARY, never sufficient: `[a]` on a producer that
    /// may SKIP is a shape the scanner reads — and the fixer still stops
    /// the whole file (S1). The finding says that out loud now, so the two
    /// screens agree.
    #[test]
    fn a_readable_shape_still_stops_on_a_skippable_producer() {
        let source = "nika: t\ntasks:\n  a:\n    on_error: { skip: true }\n    exec: { command: [\"true\"] }\n  c:\n    depends_on: [a]\n    exec: { command: [\"true\"] }\n";
        assert!(finding_is_provable(source), "the SHAPE is readable");
        let (answered, tail) = fixer_verdict(source);
        assert_eq!(answered, Verdict::SemanticStop, "{tail}");
        assert!(tail.contains("may SKIP"), "{tail}");
        let err = nika_schema::parse(
            source,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect_err("dead form");
        let text = err.to_string();
        assert!(text.contains("can migrate this shape"), "{text}");
        assert!(text.contains("still stops the file"), "{text}");
    }

    /// The empty list declares no edge — migrating it means DROPPING the
    /// dead line, and no `after:` edge is invented. The finding calls the
    /// shape migratable for exactly this reason.
    #[test]
    fn the_empty_depends_on_list_is_dropped_by_the_ladder() {
        let mut source = w2_doc("    depends_on: []\n");
        let mut repairs = Vec::new();
        let mut stops = StopNotes(Vec::new());
        assert!(
            apply_w2_flow(&mut source, &mut repairs, &mut stops),
            "{stops:?}"
        );
        assert!(!source.contains("depends_on"), "{source}");
        assert!(!source.contains("after:"), "no edge is invented: {source}");
        assert_eq!(repairs.len(), 1, "{repairs:?}");
    }

    /// The prepass and repair-report byte oracle, through the unchanged host
    /// paths (`apply_prepass`, the four prepass helpers, `render_refusals`,
    /// `render_stops`, `summary`). Every expected digest below was captured by
    /// running the immutable pre-move code on these same
    /// inputs, never by the code under test.
    mod ladder_oracle {
        use sha2::{Digest as _, Sha256};

        use super::*;
        use crate::display::theme::Theme;

        const SOURCES: [(&str, &str); 14] = [
            ("bare_exec", "nika: a\ntasks:\n  t:\n    exec: echo hi\n"),
            (
                "metachar",
                "nika: a\ntasks:\n  t:\n    exec: echo hi | wc -l\n",
            ),
            ("quoted", "nika: a\ntasks:\n  t:\n    exec: \"echo é ✨\"\n"),
            (
                "mapping",
                "nika: a\ntasks:\n  t:\n    exec: { command: [true] }\n",
            ),
            (
                "list_tasks",
                "nika: a\ntasks:\n  - id: a\n    exec: ls -la\n",
            ),
            (
                "needs_flow",
                "nika: a\ntasks:\n  a:\n    exec: { command: [true] }\n  b:\n    needs: [a]\n    exec: { command: [true] }\n",
            ),
            (
                "needs_two",
                "nika: a\ntasks:\n  b:\n    needs: [a, c]\n    infer: { prompt: x }\n",
            ),
            (
                "needs_block",
                "nika: a\ntasks:\n  b:\n    needs:\n      - a\n    infer: { prompt: x }\n",
            ),
            (
                "needs_scalar",
                "nika: a\ntasks:\n  b:\n    needs: a\n    infer: { prompt: x }\n",
            ),
            (
                "needs_empty",
                "nika: a\ntasks:\n  b:\n    needs: []\n    infer: { prompt: x }\n",
            ),
            (
                "needs_after",
                "nika: a\ntasks:\n  b:\n    after: { c: success }\n    needs: [a]\n    infer: { prompt: x }\n",
            ),
            (
                "both_crlf",
                "nika: a\r\ntasks:\r\n  t:\r\n    needs: [a]\r\n    exec: echo hi\r\n",
            ),
            (
                "unicode_ids",
                "nika: é\ntasks:\n  tâche:\n    needs: [étape]\n    exec: echo « ✨ »\n",
            ),
            ("empty", ""),
        ];

        fn themes() -> [(&'static str, Theme); 5] {
            [
                ("plain", Theme::new(false, false, false)),
                ("color", Theme::new(true, false, false)),
                ("ascii", Theme::new(false, true, false)),
                ("color_ascii", Theme::new(true, true, false)),
                ("animate", Theme::new(true, false, true)),
            ]
        }

        fn prepass_cases() -> Vec<(String, String)> {
            let mut out = Vec::new();
            for (name, source) in SOURCES {
                let helpers = format!(
                    "{:?}|{:?}|{:?}|{:?}",
                    wrap_bare_exec(source),
                    has_bare_exec(source),
                    rewrite_needs(source),
                    has_needs_key(source),
                );
                out.push((format!("prepass/{name}/helpers"), helpers));
                let mut text = source.to_owned();
                let mut repairs = Vec::new();
                let mut stop_notes = StopNotes(Vec::new());
                apply_prepass(&mut text, &mut repairs, &mut stop_notes);
                out.push((
                    format!("prepass/{name}/apply"),
                    format!("{text}\n--\n{repairs:?}\n--\n{:?}", stop_notes.0),
                ));
            }
            out
        }

        fn report_cases() -> Vec<(String, String)> {
            let refusals = [
                ("none", vec![]),
                (
                    "one",
                    vec![Refusal {
                        attempted: vec!["w1-map `envelope` → `map`".to_owned()],
                        reason: "simple key expect ':'".to_owned(),
                    }],
                ),
                (
                    "two",
                    vec![
                        Refusal {
                            attempted: vec![],
                            reason: "duplicate key « tâche » \"x\" \\".to_owned(),
                        },
                        Refusal {
                            attempted: vec!["a → b".to_owned(), "c ✨ → d".to_owned()],
                            reason: "line one\nline two".to_owned(),
                        },
                    ],
                ),
            ];
            let stops = [
                ("none", StopNotes(vec![])),
                (
                    "two",
                    StopNotes(vec![
                        "`needs:` is foreign — rewrite it".to_owned(),
                        "équipe \"x\" \\ ✨\nsecond".to_owned(),
                    ]),
                ),
            ];
            let repairs = vec![
                Repair::applied("bare exec: string", "command: argv", "bare-exec"),
                Repair {
                    old: "needs: « é »".to_owned(),
                    new: "after: { id: success }".to_owned(),
                    kind: "needs-after",
                    applied: false,
                },
                Repair::applied("tasks: list", "tasks: map keyed by task id", "w1-map"),
            ];
            let summaries = [
                ("none", vec![], 0),
                ("skipped", vec![repairs[1].clone()], 0),
                ("mixed", repairs.clone(), 2),
                ("overcount", repairs, 3),
            ];
            let mut out = Vec::new();
            for (t, theme) in themes() {
                for (r, rows) in &refusals {
                    out.push((
                        format!("report/refusals/{r}/{t}"),
                        render_refusals(rows, theme),
                    ));
                }
                for (s, notes) in &stops {
                    out.push((format!("report/stops/{s}/{t}"), render_stops(notes, theme)));
                }
                for (s, rows, applied) in &summaries {
                    out.push((
                        format!("report/summary/{s}/{t}"),
                        summary(rows, *applied, theme),
                    ));
                }
            }
            out
        }

        /// `(group, cases, sha256)` per leading two name segments, in order.
        fn groups(cases: &[(String, String)]) -> Vec<(String, usize, String)> {
            let mut out: Vec<(String, usize, Sha256)> = Vec::new();
            for (name, text) in cases {
                let key = name.splitn(3, '/').take(2).collect::<Vec<_>>().join("/");
                if out.last().is_none_or(|(k, _, _)| *k != key) {
                    out.push((key, 0, Sha256::new()));
                }
                if let Some((_, n, hasher)) = out.last_mut() {
                    *n += 1;
                    hasher.update(name.as_bytes());
                    hasher.update([0]);
                    hasher.update(text.len().to_string().as_bytes());
                    hasher.update([0]);
                    hasher.update(text.as_bytes());
                }
            }
            out.into_iter()
                .map(|(k, n, h)| (k, n, format!("{:x}", h.finalize())))
                .collect()
        }

        /// `(group, cases, sha256)` captured from the pre-move code; never recomputed by the code under test.
        const EXPECTED: &[(&str, usize, &str)] = &[
            (
                "prepass/bare_exec",
                2,
                "7430a48a526b1897cd1b88fd6200c908492ba69e6a07753c3050ad55186e0140",
            ),
            (
                "prepass/metachar",
                2,
                "90e2e29a7c6c69d6da1e9de6ba384b5644a49ac3056cdac20e01448dc4e0c914",
            ),
            (
                "prepass/quoted",
                2,
                "7171e3528a5dfd7730d5ba58611b0e364743206b322bb7b4fb413850747db5d4",
            ),
            (
                "prepass/mapping",
                2,
                "0bcc698c83aefadc9dd7cc431a2b1f7b14bfdd552a40ea0a016d61deffbedb43",
            ),
            (
                "prepass/list_tasks",
                2,
                "4c5dd211544b858fe63db3ddd7f0be824fd5a91fa05bfd6777a7b7a067dd76bb",
            ),
            (
                "prepass/needs_flow",
                2,
                "714335c9a37f7465a5730eeb3807cd1d5a69edf9694ca26dc1b3164c3020cbc4",
            ),
            (
                "prepass/needs_two",
                2,
                "2815640bafaa1ec6280f17e896be9afad89aaac0992b9e17e8079f3e9057ac7e",
            ),
            (
                "prepass/needs_block",
                2,
                "fea2623eec21543bf321e84c26e92da3a84096709cb0f45d1e9580e58e2426b3",
            ),
            (
                "prepass/needs_scalar",
                2,
                "12d2c982dd258b1a005c921c7a39d5c3bab241762673341f7a5f04f2a5fb7979",
            ),
            (
                "prepass/needs_empty",
                2,
                "4e47a9bee228a8848e2d2dbc152d747e882cd1db831197821ad7f5fc56e30c22",
            ),
            (
                "prepass/needs_after",
                2,
                "729695a1202075f5ce6b03725fb0a479f597b2428d3601781dd29a3f79d40f4c",
            ),
            (
                "prepass/both_crlf",
                2,
                "e93ca9795ca7ec3fb605dbd35e2c90e9a22bfcb64b3931289b94b05de2945b69",
            ),
            (
                "prepass/unicode_ids",
                2,
                "06fa81ac9acd9f920139e25cd0e9ebaeb5a547a1ff0fc419cc8f2dcfc6c4f4cd",
            ),
            (
                "prepass/empty",
                2,
                "d310d185af7ed5c3184819b2bd8a32cb9dd39f9a05e7c48da01eccb5cd22f055",
            ),
            (
                "report/refusals",
                3,
                "1f5a1efb65433b681bf7ace19c8e6df38e66aa4deeab98a8a4d603657b045d98",
            ),
            (
                "report/stops",
                2,
                "9f6ac8239b5d77d184701ef41846cf496d162a7f3347c0d85e64198fb86aefe7",
            ),
            (
                "report/summary",
                4,
                "17ca5d752c8a71c0d9b3f57c28fce17ef2801a422ecde74c6aaa489b0b80b1b8",
            ),
            (
                "report/refusals",
                3,
                "8004da668ed351a1d816731b46fa692135c3daf18d1d89bb7e21d6f85a3547b4",
            ),
            (
                "report/stops",
                2,
                "2b9d258d50a10fbc17d29b930541d14e8b0971602f01f5d0165cb509ce711a14",
            ),
            (
                "report/summary",
                4,
                "57c189e4302668abaff53eb63f101305c9ce41e9a4138b1af5b93274cde7b8d6",
            ),
            (
                "report/refusals",
                3,
                "0331c7395a0e852acf2e63acbddb05cc094bf1289346d85e403953293f7f5b6a",
            ),
            (
                "report/stops",
                2,
                "6258abce7d6a891cc496970dff7160f6430dddd38985ed93c278a6e69ce35a6a",
            ),
            (
                "report/summary",
                4,
                "6ba45aecd2f6d1a210f0cc7a19e7d4c952e7d8561e82e987acebd2a2927bf4a7",
            ),
            (
                "report/refusals",
                3,
                "9eb87bd037511d3b46a3a565666fae333156c82ec7a5459f6b4aa55acc0c684e",
            ),
            (
                "report/stops",
                2,
                "e3aaca34462fa513e19e2c8eb14342f228a53866fd12f8334a04065df07158a9",
            ),
            (
                "report/summary",
                4,
                "13454e245428db99895291d32ac239f1bebd35b7e4ac123325526555cf2bea95",
            ),
            (
                "report/refusals",
                3,
                "f2fea7701fd614b316dd370f85786d55b7563770b9cfe1f3a0d40a3074d07fae",
            ),
            (
                "report/stops",
                2,
                "1b7751aff6c93e28cf56f6340e1702f8dac556a2cf49220ef3f0b5b2838987f2",
            ),
            (
                "report/summary",
                4,
                "eeee4d00ed48be913e88272fec6e0186f5f462e3454b107b4354dc29b0d834cf",
            ),
        ];

        #[test]
        fn the_prepass_and_report_keep_the_pre_move_bytes() {
            let mut cases = prepass_cases();
            cases.extend(report_cases());
            let want: Vec<(String, usize, String)> = EXPECTED
                .iter()
                .map(|(g, n, s)| ((*g).to_owned(), *n, (*s).to_owned()))
                .collect();
            assert_eq!(groups(&cases), want);
        }
    }
}
