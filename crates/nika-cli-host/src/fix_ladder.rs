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

use std::fmt::Write as _;

use nika_schema::SchemaError;

use nika_display::theme::{Role, Theme};

/// One applied (or skipped) repair, for the summary.
pub struct Repair {
    /// The dead form the repair replaces (the summary's left side).
    pub old: String,
    /// The repaired form (the summary's right side).
    pub new: String,
    /// The ladder kind (`w1-map` · `w2-flow` · `d1-split` · …).
    pub kind: &'static str,
    /// Whether the repair landed (a skip stays retryable — a later
    /// round's splice can make the token unique).
    pub applied: bool,
}

impl Repair {
    /// One APPLIED repair row (the 15k-wall constructor — the splice
    /// site sets its own flag from the gate, hence not this).
    #[must_use]
    pub fn applied(old: &str, new: &str, kind: &'static str) -> Self {
        Self {
            old: old.to_owned(),
            new: new.to_owned(),
            kind,
            applied: true,
        }
    }
}

/// Equivalence-or-stop diagnostics (W2 · D1) — rendered verbatim.
pub struct StopNotes(pub Vec<String>);

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
        // `workflow:`/`description:` keys refuse as unknown keys now,
        // which is not a dead form this ladder can mechanically repair —
        // the id must be MOVED onto `nika:`, and only the author knows
        // whether an existing `nika:` line already holds the name.
        SchemaError::W1TasksSequence { .. } | SchemaError::W1TaskIdField { .. } => {
            Some(apply_w1_map(source, repairs))
        }
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
#[must_use]
pub fn collect_typed_renames(
    report: &nika_check::CheckReport,
) -> Vec<(String, String, &'static str)> {
    let mut renames: Vec<(String, String, &'static str)> = Vec::new();
    for t in &report.unknown_tools {
        if let Some(s) = &t.suggestion {
            renames.push((t.tool.clone(), s.clone(), "tool"));
        }
    }
    for a in &report.unknown_args {
        if let Some(s) = &a.suggestion {
            renames.push((a.arg.clone(), s.clone(), "arg"));
        }
    }
    // Conformance renames (typed `offending`/`suggestion` —
    // an unknown `after:`/`with:` edge target rides the BARE
    // task name · an unresolved `${{ }}` ref rides fully
    // qualified so a splice keeps the namespace).
    for v in &report.conformance {
        if let (Some(o), Some(s)) = (&v.offending, &v.suggestion) {
            renames.push((o.clone(), s.clone(), "ref"));
        }
    }
    renames.sort();
    renames.dedup();
    renames
}

/// Render the STOP diagnostic lines (verbatim W2/D1 notes · warn glyph).
#[must_use]
pub fn render_stops(stop_notes: &StopNotes, theme: Theme) -> String {
    let mut stops = String::new();
    for note in &stop_notes.0 {
        let _ = writeln!(
            stops,
            " {} {}  {note}",
            theme.paint(Role::Warn, "◼"),
            theme.paint(Role::Strong, "STOP"),
        );
    }
    stops
}

/// The W1 dead-form arm — the shared map migration. `true` = applied.
fn apply_w1_map(source: &mut String, repairs: &mut Vec<Repair>) -> bool {
    match nika_migrate::w1(source) {
        Some(migrated) => {
            *source = migrated;
            repairs.push(Repair::applied(
                "the pre-W1 envelope (workflow scalar · tasks list)",
                "workflow object + task map",
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

/// Per-repair lines + the closing verdict (count or the honest note).
#[must_use]
pub fn summary(repairs: &[Repair], applied: usize, theme: Theme) -> String {
    let mut out = String::new();
    for r in repairs {
        if r.applied {
            let _ = writeln!(
                out,
                " {} {}  {} `{}` → `{}`",
                theme.paint(Role::Good, "✔"),
                theme.paint(Role::Strong, "FIX"),
                r.kind,
                r.old,
                r.new,
            );
        } else {
            let _ = writeln!(
                out,
                " {} {}  {} `{}` → `{}` skipped — `{}` is not unique in the file \
                 (a blind splice could rewrite the wrong site)",
                theme.paint(Role::Dim, "○"),
                theme.paint(Role::Strong, "FIX"),
                r.kind,
                r.old,
                r.new,
                r.old,
            );
        }
    }
    if applied == 0 {
        let _ = writeln!(
            out,
            " {} {}  no machine-applicable repairs (typed rename suggestions only \
             — structural findings stay yours)",
            theme.paint(Role::Dim, "○"),
            theme.paint(Role::Strong, "FIX"),
        );
    } else {
        let plural = if applied == 1 { "repair" } else { "repairs" };
        let _ = writeln!(
            out,
            " {} {}  {applied} {plural} applied · re-audit below",
            theme.paint(Role::Good, "✔"),
            theme.paint(Role::Strong, "FIX"),
        );
    }
    out
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
