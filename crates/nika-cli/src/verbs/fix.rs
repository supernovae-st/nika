// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika check --fix` — apply the machine-applicable renames and converge.
//!
//! The check ladder was DESIGNED for a repair loop (`cargo clippy --fix`
//! shape): parse aborts at the first defect, so parse-level repairs land
//! one per round; check-level typed suggestions (`nika:raed` → `nika:read`
//! · `inpit` → `input`) splice in the same pass; re-parse + re-check
//! until a round applies nothing (capped). The DEAD-FORM arm carries the
//! flag-day migrations — W1 « the map » · W2 « the flow » · C2 « the
//! E-split » · R5 « the predicates » (the old form is repairable, never
//! executable).
//!
//! SAFETY over reach — a repair is applied ONLY when the suggestion is
//! TYPED (never regex-scraped from a human message), the old token
//! occurs EXACTLY ONCE as a whole word (ambiguity skips with an honest
//! note), and the file re-parses after (convergence IS the proof). The
//! file is rewritten only when at least one repair applied; the verb
//! then renders the NORMAL check report of the final state — `--fix` is
//! check plus a converging pen, not a different audit.

use std::fmt::Write as _;

use nika_schema::{ParseMode, SchemaError};

use crate::display::theme::{Role, Theme};
use crate::verbs::{VerbOutput, exit};

/// One applied (or skipped) repair, for the summary.
struct Repair {
    old: String,
    new: String,
    kind: &'static str,
    applied: bool,
}

/// Equivalence-or-stop diagnostics (W2) — rendered verbatim; untouched.
struct StopNotes(Vec<String>);

/// Rounds cap — parse aborts at the first defect, so each parse-level
/// repair costs one round; this bounds pathological inputs.
const MAX_ROUNDS: usize = 16;

/// One round's DEAD-FORM arm — W1 · W2 · C2 · R5. `Some(true)` =
/// applied (the round restarts — the re-parse is the proof) ·
/// `Some(false)` = STOP or nothing mechanical · `None` = not dead-form.
fn apply_dead_form_arm(
    err: &SchemaError,
    source: &mut String,
    repairs: &mut Vec<Repair>,
    stop_notes: &mut StopNotes,
) -> Option<bool> {
    match err {
        // W1 « the map » dead forms (PARSE-020..023): ONE structural
        // repair — the shared migration (comment-preserving ·
        // idempotent). The old form is repairable, never executable.
        SchemaError::W1WorkflowScalar { .. }
        | SchemaError::W1TopLevelDescription { .. }
        | SchemaError::W1TasksSequence { .. }
        | SchemaError::W1TaskIdField { .. } => Some(apply_w1_map(source, repairs)),
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
        _ => None,
    }
}

/// The `nika check <file> --fix` verb. Single real file only (the caller
/// refuses stdin and multi-file — a rewrite needs a place to write).
#[must_use]
pub fn run(path: &str, native_strict: bool, model: Option<&str>, theme: Theme) -> VerbOutput {
    let Ok(original) = std::fs::read_to_string(path) else {
        return VerbOutput::env(format!("cannot read {path}"));
    };
    let mut source = original.clone();
    let mut repairs: Vec<Repair> = Vec::new();
    let mut stop_notes = StopNotes(Vec::new());

    for _ in 0..MAX_ROUNDS {
        let mut round_applied = false;
        match nika_schema::parse(&source, nika_schema::FileId::new(0), ParseMode::Strict) {
            Err(SchemaError::UnknownField {
                field,
                suggestion: Some(to),
                ..
            }) => {
                round_applied |= splice(&mut source, &field, &to, "field", &mut repairs);
                // A parse-fatal we cannot splice (ambiguous token): the
                // loop cannot progress past parse — stop honestly.
                if !round_applied {
                    break;
                }
            }
            Err(e) => match apply_dead_form_arm(&e, &mut source, &mut repairs, &mut stop_notes) {
                Some(true) => {}             // a migration applied — the round restarts
                Some(false) | None => break, // STOP, or not rename-shaped — check will tell
            },
            Ok(wf) => {
                let report = nika_schema::check(&wf);
                if let Some(stop_or_continue) =
                    try_w2_hoist(&report, &mut source, &mut repairs, &mut stop_notes)
                {
                    if stop_or_continue {
                        continue; // re-parse + re-check the hoisted form
                    }
                    break;
                }
                // Collect this round's typed renames FIRST (splicing
                // invalidates nothing — each token is unique by the gate).
                for (old, new, kind) in collect_typed_renames(&report) {
                    round_applied |= splice(&mut source, &old, &new, kind, &mut repairs);
                }
                if !round_applied {
                    break; // converged — nothing left this loop can repair
                }
            }
        }
    }

    let applied = repairs.iter().filter(|r| r.applied).count();
    if applied > 0
        && let Err(e) = nika_migrate::repair::write_atomic(path, &source)
    {
        return VerbOutput::env(format!("cannot write {path}: {e}"));
    }
    // The final truth is the NORMAL check of what is now on disk —
    // --fix is check plus a pen, never a different audit.
    let verdict = super::check::run(path, false, native_strict, model, theme);
    let stops = render_stops(&stop_notes, theme);
    VerbOutput {
        text: format!(
            "{}{}{}",
            summary(&repairs, applied, theme),
            stops,
            verdict.text
        ),
        code: verdict.code,
    }
}

/// This round's typed renames (tools · args · conformance refs), deduped.
fn collect_typed_renames(
    report: &nika_schema::check::CheckReport,
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

/// Render the STOP diagnostic lines (verbatim W2 notes · warn glyph).
fn render_stops(stop_notes: &StopNotes, theme: Theme) -> String {
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
            repairs.push(Repair {
                old: "the pre-W1 envelope (workflow scalar · tasks list)".to_owned(),
                new: "workflow object + task map".to_owned(),
                kind: "w1-map",
                applied: true,
            });
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
            repairs.push(Repair {
                old: "the pre-W2 flow (depends_on · body tasks.* reads)".to_owned(),
                new: "with: bindings + after: predicates".to_owned(),
                kind: "w2-flow",
                applied: true,
            });
            true
        }
        nika_migrate::W2Outcome::Stop(notes) => {
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
                repairs.push(Repair {
                    old: note,
                    new: "advisory — the author decides".to_owned(),
                    kind: "c2-esplit-note",
                    applied: true,
                });
            }
            repairs.push(Repair {
                old: "the dead `vars:` block".to_owned(),
                new: "`inputs:` / `const:` by classification · refs rewritten class-aware"
                    .to_owned(),
                kind: "c2-esplit",
                applied: true,
            });
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

/// The R5 arm — the predicate respelling codemod. `true` = applied ·
/// `false` = nothing to respell (the check teaching stands).
fn apply_predicates(source: &mut String, repairs: &mut Vec<Repair>) -> bool {
    let Some(migrated) = nika_migrate::predicates(source) else {
        return false;
    };
    *source = migrated;
    repairs.push(Repair {
        old: "the dead predicate spellings (succeeded · failed)".to_owned(),
        new: "success · failure in after: blocks".to_owned(),
        kind: "r5-predicates",
        applied: true,
    });
    true
}

/// The NIKA-VAR-021 hoist arm — `Some(true)` = applied (re-run the
/// round) · `Some(false)` = STOP diagnostics captured (end the loop) ·
/// `None` = no boundary finding this round.
fn try_w2_hoist(
    report: &nika_schema::check::CheckReport,
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
            repairs.push(Repair {
                old: "body tasks.* reads".to_owned(),
                new: "with: bindings (hoisted)".to_owned(),
                kind: "w2-hoist",
                applied: true,
            });
            Some(true)
        }
        nika_migrate::W2Outcome::Stop(notes) => {
            stop_notes.0 = notes;
            Some(false)
        }
    }
}

/// Per-repair lines + the closing verdict (count or the honest note).
fn summary(repairs: &[Repair], applied: usize, theme: Theme) -> String {
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
fn splice(
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

/// The env-shaped refusals for `--fix` combinations the loop cannot
/// honor (stdin · `--json`'s immutable audit · multi-file).
#[must_use]
pub fn refuse(reason: &str) -> VerbOutput {
    VerbOutput {
        text: format!("check --fix: {reason}\n"),
        code: exit::ENV,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splice_applies_unique_and_skips_ambiguous() {
        let mut s = "invoke: { tool: \"nika:jq\", args: { inpit: 1 } }".to_owned();
        let mut log = Vec::new();
        assert!(splice(&mut s, "inpit", "input", "arg", &mut log));
        assert!(s.contains("input: 1") && !s.contains("inpit"));
        // ambiguous: two sites → untouched + logged skipped
        let mut s2 = "a: { promt: 1 }\nb: { promt: 2 }".to_owned();
        assert!(!splice(&mut s2, "promt", "prompt", "field", &mut log));
        assert!(s2.contains("promt: 1"), "ambiguous stays untouched");
        let skipped = log.iter().find(|r| r.old == "promt").expect("logged");
        assert!(!skipped.applied);
    }

    #[test]
    fn fix_converges_across_parse_and_check_levels() {
        // The battery's own author-error classes, stacked in one file:
        // a parse-fatal field typo (promt) + a tool typo (nika:raed) +
        // an arg typo (inpit). --fix heals all three across rounds and
        // the final audit is clean.
        let dir = std::env::temp_dir().join(format!("nika-fix-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmpdir");
        let path = dir.join("broken.nika.yaml");
        std::fs::write(
            &path,
            "nika: v1\nworkflow:\n  id: w\nmodel: mock/echo\ntasks:\n  think:\n    infer: { promt: \"hi\", max_tokens: 10 }\n  read_it:\n    invoke: { tool: \"nika:raed\", args: { path: \"./x\" } }\n  shape:\n    invoke: { tool: \"nika:jq\", args: { expression: \".\", inpit: 1 } }\n",
        )
        .expect("write fixture");
        let out = run(
            path.to_str().expect("utf8 path"),
            false,
            None,
            Theme::new(false, true, false),
        );
        let healed = std::fs::read_to_string(&path).expect("re-read");
        assert!(healed.contains("prompt:"), "field healed: {healed}");
        assert!(healed.contains("nika:read"), "tool healed: {healed}");
        assert!(healed.contains("input: 1"), "arg healed: {healed}");
        assert!(
            out.text.contains("field `promt` → `prompt`"),
            "{}",
            out.text
        );
        assert!(
            out.text.contains("tool `nika:raed` → `nika:read`"),
            "{}",
            out.text
        );
        assert!(out.text.contains("arg `inpit` → `input`"), "{}", out.text);
        assert!(out.text.contains("3 repairs applied"), "{}", out.text);
        assert_eq!(out.code, exit::OK, "final audit is clean: {}", out.text);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn fix_without_applicable_repairs_leaves_the_file_alone() {
        // A structural finding (missing required arg) has no rename —
        // the file must be byte-identical after and the note honest.
        let dir = std::env::temp_dir().join(format!("nika-fix-noop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmpdir");
        let path = dir.join("structural.nika.yaml");
        let body =
            "nika: v1\nworkflow:\n  id: w\ntasks:\n  t:\n    invoke: { tool: \"nika:hash\" }\n";
        std::fs::write(&path, body).expect("write fixture");
        let out = run(
            path.to_str().expect("utf8 path"),
            false,
            None,
            Theme::new(false, true, false),
        );
        assert_eq!(
            std::fs::read_to_string(&path).expect("re-read"),
            body,
            "no rewrite without an applied repair"
        );
        assert!(
            out.text.contains("no machine-applicable repairs"),
            "{}",
            out.text
        );
        assert_ne!(out.code, exit::OK, "the structural finding still reds");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn dep_and_ref_renames_converge_across_rounds() {
        // The two-site convergence case the retryable-skip design exists
        // for: `buidl` occurs TWICE (bare as an `after:` control-edge
        // target · inside the qualified `tasks.buidl` outputs reference),
        // so the bare rename is ambiguous in round 1 — but the
        // fully-qualified reference rename (`tasks.buidl` →
        // `tasks.build`) is unique, applies, and leaves the bare token
        // standing alone for round 2. Both heal; a one-shot skip would
        // have left the file half-repaired.
        let dir = std::env::temp_dir().join(format!("nika-fix-conv-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmpdir");
        let path = dir.join("two-site.nika.yaml");
        std::fs::write(
            &path,
            "nika: v1\nworkflow:\n  id: w\ninputs: { topic: { type: string, required: true } }\ntasks:\n  build:\n    invoke: { tool: \"nika:log\", args: { message: \"building ${{ inputs.topik }}\" } }\n  ship:\n    after:\n      buidl: success\n    invoke: { tool: \"nika:log\", args: { message: \"shipping\" } }\noutputs:\n  made: ${{ tasks.buidl.output }}\n",
        )
        .expect("write fixture");
        let out = run(
            path.to_str().expect("utf8 path"),
            false,
            None,
            Theme::new(false, true, false),
        );
        let healed = std::fs::read_to_string(&path).expect("re-read");
        assert!(
            healed.contains("build: success"),
            "control edge healed: {healed}"
        );
        assert!(
            healed.contains("tasks.build.output"),
            "ref healed: {healed}"
        );
        assert!(
            healed.contains("inputs.topic"),
            "inputs ref healed too: {healed}"
        );
        assert!(!healed.contains("buidl") && !healed.contains("topik"));
        assert!(
            out.text.contains("ref `tasks.buidl` → `tasks.build`"),
            "{}",
            out.text
        );
        assert!(out.text.contains("ref `buidl` → `build`"), "{}", out.text);
        assert!(
            out.text.contains("ref `inputs.topik` → `inputs.topic`"),
            "{}",
            out.text
        );
        assert!(!out.text.contains("skipped"), "no residual skip rows");
        assert_eq!(out.code, exit::OK, "clean after convergence: {}", out.text);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn fix_is_idempotent_and_atomic_publish_leaves_no_temp() {
        // Socratic hardening pins (2026-07-11): « what does --fix do when
        // there is NOTHING to do? » — the second run applies zero repairs
        // and leaves the file byte-identical (do-no-harm as a property,
        // not a hope). And the atomic publish never leaves its temp
        // sibling behind on the success path (`.nika-fix-tmp.*` is the
        // crash residue ONLY).
        let dir = std::env::temp_dir().join(format!("nika-fix-idem-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmpdir");
        let path = dir.join("idem.nika.yaml");
        std::fs::write(
            &path,
            "nika: v1\nworkflow:\n  id: w\ntasks:\n  t:\n    invoke: { tool: \"nika:raed\", args: { path: \"./x\" } }\n",
        )
        .expect("write fixture");
        let p = path.to_str().expect("utf8 path");
        let theme = Theme::new(false, true, false);
        let first = run(p, false, None, theme);
        assert!(first.text.contains("1 repair applied"), "{}", first.text);
        let healed = std::fs::read_to_string(&path).expect("re-read");
        // idempotence: the second run touches nothing
        let second = run(p, false, None, theme);
        assert!(
            second.text.contains("no machine-applicable repairs"),
            "{}",
            second.text
        );
        assert_eq!(
            std::fs::read_to_string(&path).expect("re-read 2"),
            healed,
            "second run is a byte-identical no-op"
        );
        // atomicity residue: no temp sibling survives a successful publish
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .expect("readdir")
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(".nika-fix-tmp.")
            })
            .collect();
        assert!(leftovers.is_empty(), "temp residue: {leftovers:?}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn r5_dead_predicates_respell_and_converge_green() {
        // The R5 flag-day repair loop: a pre-R5 file (after: succeeded ·
        // after: failed across flow + block forms) is refused NIKA-DAG-005
        // at parse — --fix respells both spellings in ONE pass and the
        // final audit is clean (the codemod is whole-document, not
        // one-per-round).
        let dir = std::env::temp_dir().join(format!("nika-fix-r5-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmpdir");
        let path = dir.join("prer5.nika.yaml");
        std::fs::write(
            &path,
            "nika: v1\nworkflow:\n  id: w\ntasks:\n  build:\n    exec: { command: [\"true\"] }\n  test:\n    after: { build: succeeded }\n    exec: { command: [\"true\"] }\n  notify:\n    after:\n      test: failed\n    exec: { command: [\"true\"] }\n",
        )
        .expect("write fixture");
        let out = run(
            path.to_str().expect("utf8 path"),
            false,
            None,
            Theme::new(false, true, false),
        );
        let healed = std::fs::read_to_string(&path).expect("re-read");
        assert!(healed.contains("after: { build: success }"), "{healed}");
        assert!(healed.contains("test: failure"), "{healed}");
        assert!(
            !healed.contains("succeeded") && !healed.contains(": failed"),
            "{healed}"
        );
        assert!(out.text.contains("r5-predicates"), "{}", out.text);
        assert_eq!(out.code, exit::OK, "clean after the respell: {}", out.text);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn unknown_predicate_has_no_mechanical_repair() {
        // `passed` is NOT a dead spelling — the codemod stays out and the
        // closed-set teaching renders; the file is never guessed at.
        let dir = std::env::temp_dir().join(format!("nika-fix-r5noop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmpdir");
        let path = dir.join("unknown.nika.yaml");
        let body = "nika: v1\nworkflow:\n  id: w\ntasks:\n  t:\n    exec: { command: [\"true\"] }\n  d:\n    after: { t: passed }\n    exec: { command: [\"true\"] }\n";
        std::fs::write(&path, body).expect("write fixture");
        let out = run(
            path.to_str().expect("utf8 path"),
            false,
            None,
            Theme::new(false, true, false),
        );
        assert_eq!(
            std::fs::read_to_string(&path).expect("re-read"),
            body,
            "no rewrite without an applicable repair"
        );
        assert_ne!(out.code, exit::OK, "the unknown predicate still reds");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn esplit_migrates_the_dead_vars_block_and_converges_green() {
        // The C2 flag-day repair loop: a pre-C2 file (a `vars:` block +
        // `${{ vars.X }}` reads) is refused NIKA-VALUES-001 at parse —
        // --fix classifies the block into inputs:/const:, rewrites the
        // refs class-aware, and the final audit is clean.
        let dir = std::env::temp_dir().join(format!("nika-fix-esplit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmpdir");
        let path = dir.join("prec2.nika.yaml");
        std::fs::write(
            &path,
            "nika: v1\nworkflow:\n  id: w\nmodel: mock/echo\nvars:\n  topic:\n    type: string\n    required: true\n  retries: 3\ntasks:\n  t:\n    infer: { prompt: \"${{ vars.topic }} · up to ${{ vars.retries }}\" }\n",
        )
        .expect("write fixture");
        let out = run(
            path.to_str().expect("utf8 path"),
            false,
            None,
            Theme::new(false, true, false),
        );
        let healed = std::fs::read_to_string(&path).expect("re-read");
        assert!(
            healed.contains("inputs:\n  topic:\n    type: string\n    required: true"),
            "{healed}"
        );
        assert!(healed.contains("const:\n  retries: 3"), "{healed}");
        assert!(
            healed.contains("${{ inputs.topic }}") && healed.contains("${{ const.retries }}"),
            "{healed}"
        );
        assert!(
            !healed.contains("vars:") && !healed.contains("vars."),
            "{healed}"
        );
        assert!(out.text.contains("esplit"), "{}", out.text);
        assert_eq!(out.code, exit::OK, "clean after the E-split: {}", out.text);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn esplit_stop_leaves_the_file_untouched_and_names_the_entry() {
        // Atomic-or-nothing: a credential-shaped entry is outside the
        // ratified rules — the file is NOT written and the diagnostic
        // names the entry (never guess).
        let dir = std::env::temp_dir().join(format!("nika-fix-estop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmpdir");
        let path = dir.join("stop.nika.yaml");
        let body = "nika: v1\nworkflow:\n  id: w\nvars:\n  api_token: abc123\ntasks:\n  t:\n    exec: { command: [\"true\"] }\n";
        std::fs::write(&path, body).expect("write fixture");
        let out = run(
            path.to_str().expect("utf8 path"),
            false,
            None,
            Theme::new(false, true, false),
        );
        assert_eq!(
            std::fs::read_to_string(&path).expect("re-read"),
            body,
            "atomic-or-nothing: a STOP writes nothing"
        );
        assert!(out.text.contains("STOP"), "{}", out.text);
        assert!(out.text.contains("vars.api_token"), "{}", out.text);
        assert_ne!(out.code, exit::OK, "the dead form still reds: {}", out.text);
        let _ = std::fs::remove_file(&path);
    }
}
