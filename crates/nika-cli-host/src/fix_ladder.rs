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
#[derive(Clone, Debug)]
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
#[derive(Clone, Debug)]
pub struct StopNotes(pub Vec<String>);

/// A round the loop REFUSED to commit: the transformed text no longer
/// parsed as YAML although the text it started from did. The round is
/// rolled back to its savepoint (the file is never written from it) and
/// this row says what was attempted and why it was refused — a typed
/// refusal, never a silent write of a document `check` cannot read.
///
/// The invariant it enforces (2026-08-18): if `--fix` reports a repair,
/// the document on disk parses at least as far as the document it
/// replaced. Measured before the gate: the shipped 0.108.0 spliced a
/// teaching sentence into a key, announced « 1 repair applied » and
/// left YAML that no longer parsed.
#[derive(Clone, Debug)]
pub struct Refusal {
    /// The repairs the round would have applied (`kind old → new` rows).
    pub attempted: Vec<String>,
    /// The parse failure the transformed text produced.
    pub reason: String,
}

/// Judge one round's transformation: `Some(refusal)` when `after` fails
/// to parse as YAML while `before` did not — the transformation broke
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
        Err(SchemaError::YamlSyntax { message, .. }) => Some(message),
        _ => None,
    };
    if yaml_broken(before).is_some() {
        return None;
    }
    yaml_broken(after).map(|reason| Refusal { attempted, reason })
}

/// Render the refusal rows (one per rolled-back round · refuse glyph).
#[must_use]
pub fn render_refusals(refusals: &[Refusal], theme: Theme) -> String {
    let mut out = String::new();
    for r in refusals {
        let _ = writeln!(
            out,
            " {} {}  refused — {} · the repaired text does not parse ({}) · the file is unchanged",
            theme.paint(Role::Bad, "✗"),
            theme.paint(Role::Strong, "FIX"),
            r.attempted.join(" · "),
            r.reason,
        );
    }
    out
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
                "the fourteen-key identity (nika: v1 · workflow: {id, description})",
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

fn indent_of(line: &str) -> &str {
    line.split_at(line.len() - line.trim_start().len()).0
}

fn wrap_bare_exec(source: &str) -> Option<String> {
    let mut changed = false;
    let mut out = String::new();
    for line in source.lines() {
        if let Some(wrapped) = wrap_one_bare_exec(line) {
            out.push_str(&wrapped);
            changed = true;
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    if !source.ends_with('\n') && out.ends_with('\n') {
        out.pop();
    }
    changed.then_some(out)
}

fn wrap_one_bare_exec(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("exec:")?.trim();
    if rest.is_empty()
        || rest.starts_with('{')
        || rest.starts_with('[')
        || rest.starts_with('|')
        || rest.starts_with('>')
        || rest == "true"
        || rest == "false"
    {
        return None;
    }
    let indent = indent_of(line);
    let unquoted = rest.trim().trim_matches(|c| c == '"' || c == '\'');
    if unquoted.is_empty() {
        return None;
    }
    // Live dialect: argv for inert tokens, `shell:` for metacharacters.
    // Writing both `command:` and `shell: true` is the 0.102 form and
    // PARSE-019s (P08 · C13).
    if unquoted
        .chars()
        .any(|c| matches!(c, '|' | ';' | '&' | '<' | '>' | '`' | '$' | '(' | ')'))
    {
        let escaped = unquoted.replace('\\', "\\\\").replace('"', "\\\"");
        return Some(format!("{indent}exec:\n{indent}  shell: \"{escaped}\""));
    }
    let args: Vec<String> = unquoted
        .split_whitespace()
        .map(|w| format!("\"{w}\""))
        .collect();
    Some(format!(
        "{indent}exec:\n{indent}  command: [{}]",
        args.join(", ")
    ))
}

fn has_bare_exec(source: &str) -> bool {
    source.lines().any(|line| {
        let t = line.trim_start();
        t.strip_prefix("exec:").is_some_and(|rest| {
            let rest = rest.trim();
            !rest.is_empty()
                && !rest.starts_with('{')
                && !rest.starts_with('[')
                && rest != "true"
                && rest != "false"
        })
    })
}

fn rewrite_needs(source: &str) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    let mut changed = false;
    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        if let Some(rewritten) = rewrite_one_needs(line) {
            if sibling_has_after(&lines, i) {
                out.push_str(line);
            } else {
                out.push_str(&rewritten);
                changed = true;
            }
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    if !source.ends_with('\n') && out.ends_with('\n') {
        out.pop();
    }
    changed.then_some(out)
}

fn rewrite_one_needs(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("needs:")?.trim();
    let rest = rest.split('#').next().unwrap_or(rest).trim();
    let inner = rest.strip_prefix('[')?.strip_suffix(']')?.trim();
    if inner.is_empty() {
        return None;
    }
    let mut ids = Vec::new();
    for raw in inner.split(',') {
        let id = raw.trim().trim_matches('"').trim_matches('\'').trim();
        if id.is_empty()
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return None;
        }
        ids.push(id);
    }
    if ids.is_empty() {
        return None;
    }
    let map = ids
        .iter()
        .map(|id| format!("{id}: success"))
        .collect::<Vec<_>>()
        .join(", ");
    let indent = indent_of(line);
    Some(format!("{indent}after: {{ {map} }}"))
}

fn sibling_has_after(lines: &[&str], idx: usize) -> bool {
    let indent = indent_of(lines[idx]);
    let same_key = |line: &str| {
        let t = line.trim_start();
        indent_of(line) == indent && t.starts_with("after:")
    };
    lines[..idx]
        .iter()
        .rev()
        .take_while(|l| {
            let t = l.trim();
            t.is_empty() || t.starts_with('#') || indent_of(l).len() >= indent.len()
        })
        .any(|l| same_key(l))
        || lines[idx + 1..]
            .iter()
            .take_while(|l| {
                let t = l.trim();
                t.is_empty() || t.starts_with('#') || indent_of(l).len() >= indent.len()
            })
            .any(|l| same_key(l))
}

fn has_needs_key(source: &str) -> bool {
    source.lines().any(|line| {
        let t = line.trim_start();
        t.starts_with("needs:") && !t.starts_with("needs: #")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
